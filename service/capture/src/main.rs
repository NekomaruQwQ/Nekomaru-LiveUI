//! `live-capture.exe` — standalone screen capture + H.264 encoding to stdout.
//!
//! Captures a window by HWND, encodes with NVENC, and writes binary IPC
//! messages (see [`live_capture`]) to stdout.  All log output goes to stderr
//! so stdout stays exclusively binary.
//!
//! Two exclusive capture modes:
//! - **Resample**: scales the full window to `--width x --height` (letterboxed).
//! - **Crop**: extracts a subrect at native resolution via `--crop-*` args.
//!
//! ## Usage
//!
//! ```text
//! # Resample mode
//! live-capture.exe --hwnd 0x1A2B3C --width 1920 --height 1200
//!
//! # Crop mode (center-aligned 1280x720 subrect)
//! live-capture.exe --hwnd 0x1A2B3C --crop-width 1280 --crop-height 720 --crop-align center
//!
//! # Utility modes
//! live-capture.exe --enumerate-windows
//! live-capture.exe --foreground-window
//! ```

mod d3d11;
mod capture;
mod converter;
mod encoder;
mod resample;

use capture::{Alignment, CaptureSession, CropDimension, CropSpec};
use converter::NV12Converter;
use encoder::{H264Encoder, H264EncoderConfig};
use resample::Resampler;

use live_capture::*;

use clap::Parser;
use nkcore::prelude::*;
use nkcore::prelude::euclid::Size2D;

use std::io::{BufWriter, Write as _};
use std::thread;
use std::time::Duration;

use windows::Win32::Foundation::*;
use windows::Win32::Graphics::Direct3D11::*;
use windows::Win32::Graphics::Dxgi::Common::*;
use windows::Win32::System::Com::*;

// ── Constants ───────────────────────────────────────────────────────────────

const FRAME_RATE: u32 = 60;
const BITRATE: u32 = 8_000_000; // 8 Mbps CBR

// ── CLI ─────────────────────────────────────────────────────────────────────

/// Standalone screen capture + H.264 encoding to stdout.
#[derive(Parser)]
#[command(name = "live-capture")]
struct CliArgs {
    /// List visible windows as JSON and exit.
    #[arg(long)]
    enumerate_windows: bool,

    /// Print the current foreground window as JSON and exit.
    #[arg(long)]
    foreground_window: bool,

    /// Window handle (decimal or 0x hex). Required for capture mode.
    #[arg(long, value_parser = parse_hwnd,
        required_unless_present_any = ["enumerate_windows", "foreground_window"])]
    hwnd: Option<isize>,

    // ── Resample mode ────────────────────────────────────────────────────
    // Conflicts with --crop-* args: you pick one mode or the other.

    /// Output width for resample mode (must be a multiple of 16).
    #[arg(long, requires = "height",
        conflicts_with_all = ["crop_width", "crop_height", "crop_align"])]
    width: Option<u32>,

    /// Output height for resample mode (must be a multiple of 16).
    #[arg(long, requires = "width",
        conflicts_with_all = ["crop_width", "crop_height", "crop_align"])]
    height: Option<u32>,

    // ── Crop mode ────────────────────────────────────────────────────────

    /// Crop width in source pixels, or "full" for the source width.
    /// Must be a multiple of 16 (unless "full").
    #[arg(long, value_parser = parse_crop_dimension,
        requires = "crop_height",
        conflicts_with_all = ["width", "height"])]
    crop_width: Option<CropDimension>,

    /// Crop height in source pixels, or "full" for the source height.
    /// Must be a multiple of 16 (unless "full").
    #[arg(long, value_parser = parse_crop_dimension,
        requires = "crop_width",
        conflicts_with_all = ["width", "height"])]
    crop_height: Option<CropDimension>,

    /// Alignment of the crop rect within the source window.
    /// One of: center, top-left, top, top-right, left, right,
    /// bottom-left, bottom, bottom-right.  Defaults to center.
    #[arg(long, value_parser = parse_alignment, default_value = "center",
        conflicts_with_all = ["width", "height"])]
    crop_align: Alignment,
}

/// Resolved capture mode after CLI validation.
enum CaptureMode {
    /// Scale the full window to fit `width x height` with letterboxing.
    Resample { width: u32, height: u32 },
    /// Extract a subrect at native resolution.
    Crop(CropSpec),
}

// ── CLI parsers ─────────────────────────────────────────────────────────────

/// Parses a window handle from decimal (`12345`) or hex (`0x1A2B3C`).
fn parse_hwnd(s: &str) -> Result<isize, String> {
    let value =
        s
            .strip_prefix("0x")
            .map_or_else(|| s.parse(), |hex| isize::from_str_radix(hex, 16));
    let value = value.map_err(|e| format!("invalid HWND '{s}': {e}"))?;
    if value == 0 {
        Err("HWND must be non-zero".into())
    } else {
        Ok(value)
    }
}

/// Parses a crop dimension: a positive integer (must be multiple of 16) or
/// the literal `"full"`.
fn parse_crop_dimension(s: &str) -> Result<CropDimension, String> {
    if s.eq_ignore_ascii_case("full") {
        return Ok(CropDimension::Full);
    }
    let px: u32 = s.parse().map_err(|e| format!("invalid crop dimension '{s}': {e}"))?;
    if px == 0 {
        Err("crop dimension must be positive".into())
    } else if !px.is_multiple_of(16) {
        Err(format!("crop dimension must be a multiple of 16 (got {px})"))
    } else {
        Ok(CropDimension::Pixels(px))
    }
}

/// Parses an alignment keyword.
fn parse_alignment(s: &str) -> Result<Alignment, String> {
    match s.to_ascii_lowercase().as_str() {
        "center"       => Ok(Alignment::Center),
        "top-left"     => Ok(Alignment::TopLeft),
        "top"          => Ok(Alignment::Top),
        "top-right"    => Ok(Alignment::TopRight),
        "left"         => Ok(Alignment::Left),
        "right"        => Ok(Alignment::Right),
        "bottom-left"  => Ok(Alignment::BottomLeft),
        "bottom"       => Ok(Alignment::Bottom),
        "bottom-right" => Ok(Alignment::BottomRight),
        _ => Err(format!("unknown alignment '{s}' (expected: center, top-left, top, top-right, left, right, bottom-left, bottom, bottom-right)")),
    }
}

/// Validate and resolve the CLI args into a `CaptureMode`.
///
/// Returns `None` for utility modes (enumerate / foreground).
fn resolve_capture_mode(args: &CliArgs) -> anyhow::Result<Option<CaptureMode>> {
    if args.enumerate_windows || args.foreground_window {
        return Ok(None);
    }

    // Clap enforces mutual exclusivity, so at most one group is present.
    if let (Some(w), Some(h)) = (args.width, args.height) {
        anyhow::ensure!(
            w.is_multiple_of(16) && h.is_multiple_of(16),
            "width and height must be multiples of 16 (got {w}x{h})");
        return Ok(Some(CaptureMode::Resample { width: w, height: h }));
    }

    if let (Some(cw), Some(ch)) = (args.crop_width, args.crop_height) {
        return Ok(Some(CaptureMode::Crop(CropSpec {
            width: cw,
            height: ch,
            align: args.crop_align,
        })));
    }

    anyhow::bail!(
        "either --width/--height (resample) or --crop-width/--crop-height (crop) is required");
}

// ── Entry point ─────────────────────────────────────────────────────────────

fn main() {
    pretty_env_logger::init();

    let args = CliArgs::parse();

    if args.enumerate_windows {
        let windows = enumerate_windows::enumerate_windows();
        // Stdout is JSON here (not binary IPC), so the server can parse it directly.
        println!("{}", serde_json::to_string(&windows).expect("JSON serialization failed"));
        return;
    }

    if args.foreground_window {
        let window = enumerate_windows::get_foreground_window();
        println!("{}", serde_json::to_string(&window).expect("JSON serialization failed"));
        return;
    }

    let hwnd = args.hwnd.expect("clap should enforce --hwnd");
    let mode = match resolve_capture_mode(&args) {
        Ok(Some(m)) => m,
        Ok(None) => return, // utility mode already handled above
        Err(e) => {
            eprintln!("error: {e}");
            std::process::exit(1);
        }
    };

    if let Err(e) = run(hwnd, mode) {
        eprintln!("fatal: {e:?}");
        std::process::exit(1);
    }
}

fn run(hwnd: isize, mode: CaptureMode) -> anyhow::Result<()> {
    // SAFETY: Called once at the start of the main thread before any COM usage.
    unsafe { CoInitializeEx(None, COINIT_MULTITHREADED) }
        .ok()
        .context("CoInitializeEx failed")?;

    let hwnd_handle = HWND(hwnd as _);

    // Create D3D11 device
    let (_, device, device_context) =
        d3d11::create_device()
            .context("failed to create D3D11 device")?;

    // Create capture session (needed early in crop mode to resolve `full` dimensions)
    let mut capture =
        CaptureSession::from_hwnd(&device, hwnd_handle)
            .context("failed to start capture session")?;

    // Determine the output frame size (used for staging texture, NV12, encoder).
    // In resample mode this is the explicit --width/--height.
    // In crop mode this is the resolved crop size (clamped to source, aligned to 16).
    let (frame_size, crop_spec) = match mode {
        CaptureMode::Resample { width, height } => {
            let size = Size2D::new(width, height);
            log::info!("resample mode: HWND={hwnd:#X}, output={width}x{height}");
            (size, None)
        }
        CaptureMode::Crop(spec) => {
            // Peek the first frame to learn the source resolution and resolve `full`.
            let source_size = peek_source_size(&mut capture, &device_context)?;
            let size = spec.resolve_output_size(source_size);
            anyhow::ensure!(
                size.width > 0 && size.height > 0,
                "resolved crop size is zero (source={source_size:?}, spec={spec:?})");
            log::info!(
                "crop mode: HWND={hwnd:#X}, source={source_size:?}, crop={}x{}, align={:?}",
                size.width, size.height, spec.align);
            (size, Some(spec))
        }
    };

    // Create staging BGRA8 texture (shared between capture thread and encoding thread).
    // In resample mode, the resampler needs it as a render target + shader resource.
    // In crop mode, CopySubresourceRegion only needs it as a default-usage texture,
    // but we keep the same bind flags for simplicity (no perf difference).
    let staging_bgra8 =
        d3d11::create_texture_2d(
            &device,
            frame_size,
            DXGI_FORMAT_B8G8R8A8_UNORM,
            &[D3D11_BIND_SHADER_RESOURCE, D3D11_BIND_RENDER_TARGET])
            .context("failed to create BGRA8 staging texture")?;
    let staging_bgra8_rtv =
        d3d11::create_rtv_for_texture_2d(&device, &staging_bgra8)
            .context("failed to create BGRA8 staging RTV")?;

    // Clear to dark gray so the first few frames aren't random garbage
    // SAFETY: `device_context` and `staging_bgra8_rtv` are valid D3D11 objects
    // created from the same device. The RGBA array is a stack-local float[4].
    unsafe {
        device_context.ClearRenderTargetView(
            &staging_bgra8_rtv,
            &[0.16, 0.16, 0.16, 1.0]);
    }

    // Only needed in resample mode; skip shader compilation in crop mode.
    let resampler = if crop_spec.is_none() {
        Some(Resampler::new(&device).context("failed to create resampler")?)
    } else {
        None
    };

    // Spawn encoding thread
    let encoding_handle = thread::Builder::new()
        .name("encoding".to_owned())
        .spawn({
            let device = device.clone();
            let device_context = device_context.clone();
            let frame_source = staging_bgra8.clone();
            move || encoding_thread(device, device_context, frame_source, frame_size)
        })
        .context("failed to spawn encoding thread")?;

    log::info!("capture session started");

    // ── Capture loop ────────────────────────────────────────────────────
    // Runs on the main thread.  Continuously grabs frames from the capture
    // session and writes them into the shared staging texture.  The encoding
    // thread reads from this texture at its own pace ("bakery model").
    loop {
        if encoding_handle.is_finished() {
            anyhow::bail!("encoding thread exited unexpectedly");
        }

        match capture.get_next_frame(&device_context) {
            Ok(Some(frame)) => {
                if let Some(ref spec) = crop_spec {
                    // ── Crop path ────────────────────────────────────
                    // Copy a subrect of the captured frame into staging_bgra8
                    // at native resolution (no scaling).
                    let crop_box =
                        capture::compute_crop_box(frame_size, spec.align, frame.size);
                    // SAFETY: `device_context`, `staging_bgra8`, and `frame.raw_texture`
                    // are valid D3D11 objects from the same device. `crop_box` is computed
                    // to stay within source bounds by `compute_crop_box`.
                    unsafe {
                        device_context.CopySubresourceRegion(
                            &staging_bgra8,    // dst
                            0,                 // dst subresource
                            0, 0, 0,           // dst x, y, z
                            &frame.raw_texture, // src
                            0,                 // src subresource
                            Some(&raw const crop_box));
                    }
                } else {
                    // ── Resample path ────────────────────────────────
                    // Scale the full captured frame into staging_bgra8 with
                    // aspect-ratio-preserving letterboxing.
                    let viewport =
                        capture::calculate_resample_viewport(frame.size, frame_size);
                    // SAFETY: `device_context` is valid; `viewport` is a stack-local struct.
                    unsafe { device_context.RSSetViewports(Some(&[viewport])); }

                    let source_srv =
                        d3d11::create_srv_for_texture_2d(&device, &frame.raw_texture)
                            .context("failed to create SRV for captured frame")?;
                    resampler.as_ref().unwrap()
                        .resample(&device_context, &source_srv, &staging_bgra8_rtv);

                    // SAFETY: `device_context` is valid; clearing the viewport array.
                    unsafe { device_context.RSSetViewports(Some(&[])); }
                }

                // Flush GPU commands so the encoding thread sees the new frame.
                // The small sleep gives the GPU time to finish before the encoder reads.
                // SAFETY: `device_context` is a valid D3D11 device context.
                unsafe { device_context.Flush(); }
                thread::sleep(Duration::from_millis(5));
            },
            Ok(None) => {
                thread::sleep(Duration::from_millis(1));
            },
            Err(e) => {
                log::error!("capture error: {e:?}");
                // Non-fatal: the encoder will re-encode the last good frame.
                thread::sleep(Duration::from_millis(100));
            },
        }
    }
}

/// Wait for the first captured frame and return its size.
///
/// Used in crop mode to resolve `CropDimension::Full` before allocating
/// textures.  Blocks until a frame arrives (typically < 16ms).
fn peek_source_size(
    capture: &mut CaptureSession,
    ctx: &ID3D11DeviceContext) -> anyhow::Result<Size2D<u32>> {
    loop {
        match capture.get_next_frame(ctx) {
            Ok(Some(frame)) => return Ok(frame.size),
            Ok(None) => thread::sleep(Duration::from_millis(1)),
            Err(e) => anyhow::bail!("failed to peek source size: {e:?}"),
        }
    }
}

// ── Encoding thread ─────────────────────────────────────────────────────────

fn encoding_thread(
    device: ID3D11Device,
    device_context: ID3D11DeviceContext,
    frame_source: ID3D11Texture2D,
    frame_size: Size2D<u32>) {
    log::info!("encoding thread started");

    // SAFETY: Called once at the start of the encoding thread before any COM usage.
    unsafe { CoInitializeEx(None, COINIT_MULTITHREADED) }
        .ok()
        .expect("CoInitializeEx failed on encoding thread");

    let nv12_converter =
        NV12Converter::new(&device, &device_context, frame_size.width, frame_size.height)
            .expect("failed to create NV12 converter");
    let nv12_staging =
        d3d11::create_texture_2d(
            &device,
            frame_size,
            DXGI_FORMAT_NV12,
            &[D3D11_BIND_RENDER_TARGET])
            .expect("failed to create NV12 staging texture");
    log::info!("NV12 converter and staging texture created");

    let stdout = std::io::stdout();
    let mut writer = BufWriter::new(stdout.lock());
    let mut last_sps: Option<Vec<u8>> = None;
    let mut last_pps: Option<Vec<u8>> = None;

    let encoder = H264Encoder::new(&device, H264EncoderConfig {
        frame_size,
        frame_rate: FRAME_RATE,
        bitrate: BITRATE,
    }).expect("failed to create H.264 encoder");

    encoder.run(
        // Frame source: convert BGRA8 → NV12
        || {
            nv12_converter
                .convert(&frame_source, &nv12_staging)
                .expect("BGRA8 \u{2192} NV12 conversion failed");
            nv12_staging.clone()
        },
        // Frame target: serialize to stdout via IPC protocol
        |nal_units: Vec<NALUnit>| {
            if nal_units.is_empty() {
                return;
            }

            // Extract SPS/PPS from IDR frames and send CodecParams if changed
            let sps = nal_units.iter().find(|u| u.unit_type == NALUnitType::SPS);
            let pps = nal_units.iter().find(|u| u.unit_type == NALUnitType::PPS);

            if let (Some(sps), Some(pps)) = (sps, pps) {
                let sps_changed = last_sps.as_ref() != Some(&sps.data);
                let pps_changed = last_pps.as_ref() != Some(&pps.data);

                if sps_changed || pps_changed {
                    let params = CodecParams {
                        sps: sps.data.clone(),
                        pps: pps.data.clone(),
                        width: frame_size.width,
                        height: frame_size.height,
                    };

                    if let Err(e) = write_codec_params(&mut writer, &params) {
                        log::error!("failed to write CodecParams: {e}");
                        return;
                    }

                    last_sps = Some(sps.data.clone());
                    last_pps = Some(pps.data.clone());
                    log::info!(
                        "sent CodecParams: {}x{}, SPS={}B, PPS={}B",
                        frame_size.width, frame_size.height,
                        params.sps.len(), params.pps.len());
                }
            }

            // Build and write Frame message
            let is_keyframe = nal_units.iter().any(|u| u.unit_type == NALUnitType::IDR);
            let frame = FrameMessage {
                // The encoder sets sample timestamps in 100ns units; we approximate
                // with wall-clock time here.  The server doesn't rely on exact values.
                timestamp_us: std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .unwrap_or_default()
                    .as_micros() as u64,
                is_keyframe,
                nal_units,
            };

            if let Err(e) = write_frame(&mut writer, &frame) {
                log::error!("failed to write Frame: {e}");
                // Stdout broken (server killed us or pipe closed) — exit cleanly
                let _ = writer.flush();
                std::process::exit(0);
            }
        });
}
