//! `live-capture.exe` — standalone screen capture + H.264 encoding to stdout.
//!
//! Captures a window by HWND, encodes with NVENC, and writes
//! `live-protocol` framed binary messages to stdout.  Pipe through
//! `live-ws` for WebSocket delivery to the server.
//!
//! Three exclusive capture modes:
//! - **Base** (default): scales the full window to `--width x --height`.
//! - **Crop**: extracts an absolute subrect via `--crop-min-x/y --crop-max-x/y`.
//! - **Auto**: (Phase 2) foreground polling + hot-swap capture session.
//!
//! ## Usage
//!
//! ```text
//! # Base mode — capture + encode to stdout, pipe through live-ws
//! live-capture --hwnd 0x1A2B --width 1920 --height 1200 \
//!   | live-ws --mode video --server ws://machineA:3000/api/v1/ws/video/main/input
//!
//! # Dump to file for testing (production code path)
//! live-capture --hwnd 0x1A2B --width 1920 --height 1200 > dump.bin
//!
//! # Utility modes
//! live-capture --enumerate-windows
//! live-capture --foreground-window
//! ```

mod d3d11;
mod capture;
mod converter;
mod encoder;
mod resample;

use capture::{CaptureSession, CropBox};
use converter::NV12Converter;
use encoder::{H264Encoder, H264EncoderConfig};
use resample::Resampler;

use live_capture::{NALUnit, NALUnitType};
use live_protocol::{MessageType, flags, write_message};
use live_protocol::avcc::serialize_avcc_payload;
use live_protocol::video::{CodecParams, write_codec_params_payload, write_frame_payload};

use clap::Parser;
use nkcore::prelude::*;
use nkcore::prelude::euclid::Size2D;

use std::io::{BufWriter, Write as _};
use std::sync::Mutex;
use std::thread;
use std::time::Duration;

use windows::Win32::Foundation::*;
use windows::Win32::Graphics::Direct3D11::*;
use windows::Win32::Graphics::Dxgi::Common::*;
use windows::Win32::System::Com::*;

// ── Constants ───────────────────────────────────────────────────────────────

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

    // ── Resample mode (base) ──────────────────────────────────────────────
    // Conflicts with --crop-* args: you pick one mode or the other.

    /// Output width for resample mode (must be a multiple of 16).
    #[arg(long, requires = "height",
        conflicts_with_all = ["crop_min_x", "crop_min_y", "crop_max_x", "crop_max_y"])]
    width: Option<u32>,

    /// Output height for resample mode (must be a multiple of 16).
    #[arg(long, requires = "width",
        conflicts_with_all = ["crop_min_x", "crop_min_y", "crop_max_x", "crop_max_y"])]
    height: Option<u32>,

    // ── Crop mode ──────────────────────────────────────────────────────────
    // Absolute bounding box in source pixels.  All four are required together.
    // Non-16-aligned dimensions are accepted; the encoder output is padded
    // to the next multiple of 16.

    /// Left edge of the crop rect (inclusive), in source pixels.
    #[arg(long,
        requires_all = ["crop_min_y", "crop_max_x", "crop_max_y"],
        conflicts_with_all = ["width", "height"])]
    crop_min_x: Option<u32>,

    /// Top edge of the crop rect (inclusive), in source pixels.
    #[arg(long,
        requires_all = ["crop_min_x", "crop_max_x", "crop_max_y"],
        conflicts_with_all = ["width", "height"])]
    crop_min_y: Option<u32>,

    /// Right edge of the crop rect (exclusive), in source pixels.
    #[arg(long,
        requires_all = ["crop_min_x", "crop_min_y", "crop_max_y"],
        conflicts_with_all = ["width", "height"])]
    crop_max_x: Option<u32>,

    /// Bottom edge of the crop rect (exclusive), in source pixels.
    #[arg(long,
        requires_all = ["crop_min_x", "crop_min_y", "crop_max_x"],
        conflicts_with_all = ["width", "height"])]
    crop_max_y: Option<u32>,

    /// Encoder frame rate (1-60). Lower values save GPU cycles for
    /// near-static content like the YouTube Music playback bar.
    #[arg(long, default_value_t = 60, value_parser = clap::value_parser!(u32).range(1..=60))]
    fps: u32,

    /// Stream ID tag for log output (e.g. "main", "youtube-music").
    /// When set, log lines include `@<stream_id>` for disambiguation
    /// when multiple instances write to the same inherited stderr.
    #[arg(long)]
    stream_id: Option<String>,
}

/// Resolved capture mode after CLI validation.
#[derive(Clone, Copy)]
enum CaptureMode {
    /// Scale the full window to fit `width x height` with letterboxing.
    Resample { width: u32, height: u32 },
    /// Extract an absolute subrect at native resolution.
    Crop(CropBox),
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

    if let (Some(min_x), Some(min_y), Some(max_x), Some(max_y)) =
        (args.crop_min_x, args.crop_min_y, args.crop_max_x, args.crop_max_y) {
        anyhow::ensure!(max_x > min_x, "crop-max-x ({max_x}) must be greater than crop-min-x ({min_x})");
        anyhow::ensure!(max_y > min_y, "crop-max-y ({max_y}) must be greater than crop-min-y ({min_y})");
        return Ok(Some(CaptureMode::Crop(CropBox { min_x, min_y, max_x, max_y })));
    }

    anyhow::bail!(
        "either --width/--height (resample) or --crop-min-x/y --crop-max-x/y (crop) is required");
}

// ── Logging ─────────────────────────────────────────────────────────────────

/// Set up dual-output logging:
/// - Encoder init diagnostics (info/debug/trace from `live_capture::encoder`)
///   go to `live-capture.encoder.log` next to the executable.
/// - Warnings and errors from encoder code still go to stderr.
/// - Everything else goes to stderr as usual.
///
/// `capture_mode` controls whether the encoder log file is created.  Utility
/// modes (`--enumerate-windows`, `--foreground-window`) pass `false` to avoid
/// truncating a log file that a concurrent capture process is writing to.
fn init_logger(capture_mode: bool, stream_id: Option<String>) {
    use pretty_env_logger::env_logger::fmt::Color;

    let encoder_log_file: Option<Mutex<std::fs::File>> = if capture_mode {
        std::env::current_exe()
            .ok()
            .and_then(|p| p.parent().map(|d| d.join("live-capture.encoder.log")))
            .and_then(|p| std::fs::File::create(p).ok())
            .map(Mutex::new)
    } else {
        None
    };

    let tag = stream_id.map_or_else(String::new, |id| format!(" @{id}"));

    pretty_env_logger::env_logger::Builder::from_env(
        pretty_env_logger::env_logger::Env::default().default_filter_or("info"))
        .format(move |buf, record| {
            let is_encoder = record.target().starts_with("live_capture::encoder");
            let is_diagnostic = record.level() >= log::Level::Info;
            if is_encoder && is_diagnostic
                && let Some(ref file) = encoder_log_file {
                    let mut f = file.lock().unwrap();
                    writeln!(f, "[{}{tag} {}] {}", record.level(), record.target(), record.args())?;
                    drop(f);
                    return Ok(());
                }

            let level = buf.default_styled_level(record.level());
            let mut tag_style = buf.style();
            tag_style.set_color(Color::Cyan).set_bold(true);
            let mut target_style = buf.style();
            target_style.set_color(Color::Black).set_bold(true);

            writeln!(buf, " {level} {} {} > {}",
                tag_style.value(&tag),
                target_style.value(record.target()),
                record.args())
        })
        .init();
}

// ── Entry point ─────────────────────────────────────────────────────────────

fn main() {
    let _ = set_dpi_awareness::per_monitor_v2();

    let args = CliArgs::parse();
    let is_capture_mode = !args.enumerate_windows && !args.foreground_window;
    init_logger(is_capture_mode, args.stream_id.clone());

    if args.enumerate_windows {
        let windows = enumerate_windows::enumerate_windows();
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
        Ok(None) => return,
        Err(e) => {
            eprintln!("error: {e}");
            std::process::exit(1);
        }
    };

    if let Err(e) = run(hwnd, mode, args.fps) {
        eprintln!("fatal: {e}");
        std::process::exit(1);
    }
}

#[expect(clippy::too_many_lines, reason = "main capture loop and encoding thread are necessarily long and complex")]
fn run(hwnd: isize, mode: CaptureMode, frame_rate: u32) -> anyhow::Result<()> {
    // SAFETY: Called once at the start of the main thread before any COM usage.
    unsafe { CoInitializeEx(None, COINIT_MULTITHREADED) }
        .ok()
        .context("CoInitializeEx failed")?;

    let hwnd_handle = HWND(hwnd as _);

    let (_, device, device_context) =
        d3d11::create_device()
            .context("failed to create D3D11 device")?;

    let mut capture =
        CaptureSession::from_hwnd(&device, hwnd_handle)
            .context("failed to start capture session")?;

    let (frame_size, crop_box) = match mode {
        CaptureMode::Resample { width, height } => {
            let size = Size2D::new(width, height);
            log::info!("resample mode: HWND={hwnd:#X}, output={width}x{height}");
            (size, None)
        }
        CaptureMode::Crop(crop) => {
            let output = crop.output_size();
            log::info!(
                "crop mode: HWND={hwnd:#X}, box=({},{})..({},{}), output={}x{}",
                crop.min_x, crop.min_y, crop.max_x, crop.max_y,
                output.width, output.height);
            (output, Some(crop))
        }
    };

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

    // Clear to dark gray so the first few frames aren't random garbage.
    // SAFETY: `device_context` and `staging_bgra8_rtv` are valid D3D11 objects
    // created from the same device.
    unsafe {
        device_context.ClearRenderTargetView(
            &staging_bgra8_rtv,
            &[0.16, 0.16, 0.16, 1.0]);
    }

    let deferred_context: ID3D11DeviceContext = {
        let mut ctx = None;
        // SAFETY: `device` is a valid D3D11 device; `ctx` is a stack-local out-param.
        unsafe { device.CreateDeferredContext(0, Some(&raw mut ctx)) }
            .context("failed to create deferred context")?;
        ctx.ok_or_else(|| anyhow::anyhow!("deferred context is null"))?
    };

    let resampler = if crop_box.is_none() {
        Some(Resampler::new(&device).context("failed to create resampler")?)
    } else {
        None
    };

    let encoding_handle = thread::Builder::new()
        .name("encoding".to_owned())
        .spawn({
            let device = device.clone();
            let device_context = device_context.clone();
            let frame_source = staging_bgra8.clone();
            move || encoding_thread(&device, &device_context, &frame_source, frame_size, frame_rate)
        })
        .context("failed to spawn encoding thread")?;

    log::info!("capture session started");

    // ── Capture loop ────────────────────────────────────────────────────
    loop {
        if encoding_handle.is_finished() {
            anyhow::bail!("encoding thread exited unexpectedly");
        }

        match capture.get_next_frame(&device_context) {
            Ok(Some(frame)) => {
                // SAFETY: `deferred_context` and `staging_bgra8_rtv` are valid
                // D3D11 objects from the same device.
                unsafe {
                    deferred_context.ClearRenderTargetView(
                        &staging_bgra8_rtv,
                        &[0.16, 0.16, 0.16, 1.0]);
                }

                if let Some(crop) = crop_box {
                    let d3d_box = crop.to_d3d11_box(frame.size);
                    // SAFETY: valid D3D11 objects from the same device.
                    unsafe {
                        deferred_context.CopySubresourceRegion(
                            &staging_bgra8,
                            0,
                            0, 0, 0,
                            &frame.raw_texture,
                            0,
                            Some(&raw const d3d_box));
                    }
                } else {
                    let viewport =
                        capture::calculate_resample_viewport(frame.size, frame_size);
                    // SAFETY: `deferred_context` is valid.
                    unsafe { deferred_context.RSSetViewports(Some(&[viewport])); }

                    let source_srv =
                        d3d11::create_srv_for_texture_2d(&device, &frame.raw_texture)
                            .context("failed to create SRV for captured frame")?;
                    resampler.as_ref().unwrap()
                        .resample(&deferred_context, &source_srv, &staging_bgra8_rtv);

                    // SAFETY: `deferred_context` is valid.
                    unsafe { deferred_context.RSSetViewports(Some(&[])); }
                }

                let command_list = {
                    let mut list = None;
                    // SAFETY: `deferred_context` has recorded valid GPU commands.
                    unsafe { deferred_context.FinishCommandList(false, Some(&raw mut list)) }
                        .context("FinishCommandList failed")?;
                    list.ok_or_else(|| anyhow::anyhow!("command list is null"))?
                };
                // SAFETY: valid immediate context + command list.
                unsafe {
                    device_context.ExecuteCommandList(&command_list, true);
                }
                // SAFETY: valid immediate context.
                unsafe {
                    device_context.Flush();
                }
                thread::sleep(Duration::from_millis(5));
            },
            Ok(None) => {
                thread::sleep(Duration::from_millis(1));
            },
            Err(e) => {
                log::error!("capture error: {e:?}");
                thread::sleep(Duration::from_millis(100));
            },
        }
    }
}

// ── Encoding thread ─────────────────────────────────────────────────────────

/// Encoding thread: reads from the shared staging texture, converts BGRA→NV12,
/// encodes H.264 via NVENC, converts NALs to AVCC, and writes `live-protocol`
/// framed messages to stdout.
#[expect(clippy::similar_names, reason = "last_sps/last_pps are intentionally parallel")]
#[expect(clippy::exit, reason = "intentional exit when stdout pipe breaks")]
fn encoding_thread(
    device: &ID3D11Device,
    device_context: &ID3D11DeviceContext,
    frame_source: &ID3D11Texture2D,
    frame_size: Size2D<u32>,
    frame_rate: u32) {
    log::info!("encoding thread started");

    // SAFETY: Called once at the start of the encoding thread before any COM usage.
    unsafe { CoInitializeEx(None, COINIT_MULTITHREADED) }
        .ok()
        .expect("CoInitializeEx failed on encoding thread");

    let nv12_converter =
        NV12Converter::new(device, device_context, frame_size.width, frame_size.height)
            .expect("failed to create NV12 converter");
    let nv12_staging =
        d3d11::create_texture_2d(
            device,
            frame_size,
            DXGI_FORMAT_NV12,
            &[D3D11_BIND_RENDER_TARGET])
            .expect("failed to create NV12 staging texture");
    log::info!("NV12 converter and staging texture created");

    let stdout = std::io::stdout();
    let mut writer = BufWriter::new(stdout.lock());
    let mut last_sps: Option<Vec<u8>> = None;
    let mut last_pps: Option<Vec<u8>> = None;

    let encoder = H264Encoder::new(device, H264EncoderConfig {
        frame_size,
        frame_rate,
        bitrate: BITRATE,
    }).expect("failed to create H.264 encoder");

    encoder.run(
        // Frame source: convert BGRA8 → NV12
        || {
            nv12_converter
                .convert(frame_source, &nv12_staging)
                .expect("BGRA8 \u{2192} NV12 conversion failed");
            nv12_staging.clone()
        },
        // Frame callback: convert to AVCC, write live-protocol messages to stdout
        |nal_units: Vec<NALUnit>| {
            if nal_units.is_empty() {
                return;
            }

            // Extract SPS/PPS from IDR frames and send CodecParams if changed.
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

                    let payload = write_codec_params_payload(&params);
                    if let Err(e) = write_message(
                        &mut writer, MessageType::CodecParams, 0, &payload) {
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

            // Build AVCC payload from all NAL units.
            let is_keyframe = nal_units.iter().any(|u| u.unit_type == NALUnitType::IDR);
            let nal_data: Vec<&[u8]> = nal_units.iter().map(|u| u.data.as_slice()).collect();
            let avcc_payload = serialize_avcc_payload(&nal_data);

            let timestamp_us = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap_or_default()
                .as_micros() as u64;

            let frame_payload = write_frame_payload(timestamp_us, &avcc_payload);
            let frame_flags = if is_keyframe { flags::IS_KEYFRAME } else { 0 };

            if let Err(e) = write_message(
                &mut writer, MessageType::Frame, frame_flags, &frame_payload) {
                log::error!("failed to write Frame: {e}");
                // Stdout broken (pipe closed) — exit cleanly.
                let _ = writer.flush();
                std::process::exit(0);
            }
        });
}
