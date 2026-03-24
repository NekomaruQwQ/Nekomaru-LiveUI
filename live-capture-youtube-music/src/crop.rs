//! CSS-based crop rect computation for the YouTube Music player bar.
//!
//! Derives the crop rectangle from YTM's CSS layout constants and the actual
//! window DPI, rather than hardcoded pixel margins.  This makes the crop
//! DPI-independent — it works at any display scale factor.
//!
//! ## Coordinate pipeline
//!
//! 1. CSS viewport size = `GetClientRect` physical pixels / scale factor
//! 2. Bar bounding box in CSS pixels (from YTM's known CSS layout)
//! 3. Shrink inward by [`PADDING`] to avoid window-border artifacts
//! 4. Scale back to physical client-area pixels
//! 5. Offset into captured-texture space (which is `GetWindowRect`, including
//!    title bar and frame padding)

use anyhow::Context as _;
use euclid::default::{Box2D, Point2D, Size2D, Vector2D};

use windows::Win32::Foundation::*;
use windows::Win32::Graphics::Gdi::*;
use windows::Win32::UI::HiDpi::*;
use windows::Win32::UI::WindowsAndMessaging::*;

// ── YTM CSS layout constants ────────────────────────────────────────────────

/// Height of the player bar at the bottom of the viewport.
const BAR_HEIGHT: f32 = 72.0;

/// Width of the browser scrollbar on the right edge.
const SCROLLBAR_WIDTH: f32 = 12.0;

/// Inward padding: shrinks the bar bounding box on all sides to avoid
/// capturing colored window borders or other artifacts at the edges.
const PADDING: f32 = 4.0;

// ── Win32 geometry helpers ──────────────────────────────────────────────────

/// Returns the window's outer bounds (title bar + borders + client area).
///
/// WinRT Graphics Capture captures the full window, so these dimensions
/// match the captured texture size.
fn get_window_rect(hwnd: HWND) -> anyhow::Result<RECT> {
    let mut rect = RECT::default();
    // SAFETY: `hwnd` is a valid handle from window enumeration;
    // `&raw mut rect` points to a valid stack-local `RECT`.
    unsafe { GetWindowRect(hwnd, &raw mut rect) }
        .map_err(|e| anyhow::anyhow!("GetWindowRect: {e}"))?;
    Ok(rect)
}

/// Returns the window's client-area dimensions.
fn get_client_rect(hwnd: HWND) -> anyhow::Result<RECT> {
    let mut rect = RECT::default();
    // SAFETY: same as `get_window_rect`.
    unsafe { GetClientRect(hwnd, &raw mut rect) }
        .map_err(|e| anyhow::anyhow!("GetClientRect: {e}"))?;
    Ok(rect)
}

/// Returns the offset from the window's top-left corner (GetWindowRect origin)
/// to the client area's top-left corner, in physical pixels.
///
/// This is the frame thickness: left border width and title bar + top border
/// height.  Used to project client-area coordinates into captured-texture
/// coordinates.
fn get_frame_offset(hwnd: HWND, window_rect: &RECT) -> anyhow::Result<Vector2D<u32>> {
    let mut origin = POINT { x: 0, y: 0 };
    // SAFETY: `hwnd` is valid; `&raw mut origin` is a valid stack-local POINT.
    // ClientToScreen translates client (0,0) to screen coordinates.
    unsafe { ClientToScreen(hwnd, &raw mut origin) }
        .ok()
        .with_context(|| anyhow::anyhow!("ClientToScreen failed"))?;
    Ok(Vector2D::new(
        (origin.x - window_rect.left) as u32,
        (origin.y - window_rect.top) as u32))
}

// ── Crop rect computation ───────────────────────────────────────────────────

/// Compute the crop rectangle for the YTM player bar in captured-texture
/// coordinates (`GetWindowRect` space).
///
/// Returns a `Box2D<u32>` with min (inclusive) / max (exclusive) corners
/// suitable for passing to `live-capture --crop-min-x/y --crop-max-x/y`.
pub fn compute_crop_rect(hwnd: HWND) -> anyhow::Result<Box2D<u32>> {
    let window_rect = get_window_rect(hwnd)?;
    let client_rect = get_client_rect(hwnd)?;

    // DPI scale factor (96 DPI = 100% = scale 1.0).
    // SAFETY: `hwnd` is valid; returns 0 only for invalid handles.
    let dpi = unsafe { GetDpiForWindow(hwnd) };
    anyhow::ensure!(dpi > 0, "GetDpiForWindow returned 0 for hwnd {}", hwnd.0 as usize);
    let scale = dpi as f32 / 96.0;

    // Client area in physical pixels → CSS viewport size.
    let client_phys = Size2D::<f32>::new(
        (client_rect.right - client_rect.left) as f32,
        (client_rect.bottom - client_rect.top) as f32);
    let viewport = Size2D::new(client_phys.width / scale, client_phys.height / scale);

    // Bar bounding box in CSS viewport coordinates, shrunk inward by PADDING.
    let css_crop = Box2D::<f32>::new(
        Point2D::new(
            PADDING,
            viewport.height - BAR_HEIGHT + PADDING),
        Point2D::new(
            viewport.width - SCROLLBAR_WIDTH - PADDING,
            viewport.height - PADDING));

    // CSS → physical client-area pixels.
    // floor(min) and ceil(max) to avoid clipping content at sub-pixel boundaries.
    let client_crop = Box2D::<u32>::new(
        Point2D::new(
            (css_crop.min.x * scale).floor() as u32,
            (css_crop.min.y * scale).floor() as u32),
        Point2D::new(
            (css_crop.max.x * scale).ceil() as u32,
            (css_crop.max.y * scale).ceil() as u32));

    // Project from client area into captured-texture space by adding the
    // frame offset (title bar height + left border width).
    let offset = get_frame_offset(hwnd, &window_rect)?;

    Ok(Box2D::new(
        client_crop.min + offset,
        client_crop.max + offset))
}

// ── Tests ───────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    /// Verify the CSS → physical math at a known geometry.
    ///
    /// Simulates a 1920×1080 client area at 150% DPI (144 DPI) with a frame
    /// offset of (8, 40) — typical for a Win32 window with a title bar.
    #[test]
    fn css_crop_at_150_percent() {
        let scale = 144.0 / 96.0; // 1.5
        let client_w = 1920.0;
        let client_h = 1080.0;

        let viewport = Size2D::<f32>::new(client_w / scale, client_h / scale);
        // viewport = (1280.0, 720.0)
        assert!((viewport.width - 1280.0).abs() < 0.01);
        assert!((viewport.height - 720.0).abs() < 0.01);

        let css_crop = Box2D::<f32>::new(
            Point2D::new(PADDING, viewport.height - BAR_HEIGHT + PADDING),
            Point2D::new(
                viewport.width - SCROLLBAR_WIDTH - PADDING,
                viewport.height - PADDING));

        // Expected CSS crop:
        //   min = (4.0, 720 - 72 + 4) = (4.0, 652.0)
        //   max = (1280 - 12 - 4, 720 - 4) = (1264.0, 716.0)
        assert!((css_crop.min.x - 4.0).abs() < 0.01);
        assert!((css_crop.min.y - 652.0).abs() < 0.01);
        assert!((css_crop.max.x - 1264.0).abs() < 0.01);
        assert!((css_crop.max.y - 716.0).abs() < 0.01);

        // CSS → physical (at 1.5x)
        let client_crop = Box2D::<u32>::new(
            Point2D::new(
                (css_crop.min.x * scale).floor() as u32,
                (css_crop.min.y * scale).floor() as u32),
            Point2D::new(
                (css_crop.max.x * scale).ceil() as u32,
                (css_crop.max.y * scale).ceil() as u32));

        // min = floor(4*1.5, 652*1.5) = floor(6.0, 978.0) = (6, 978)
        // max = ceil(1264*1.5, 716*1.5) = ceil(1896.0, 1074.0) = (1896, 1074)
        assert_eq!(client_crop.min, Point2D::new(6, 978));
        assert_eq!(client_crop.max, Point2D::new(1896, 1074));

        // Add frame offset (8, 40) → texture coords
        let offset = Vector2D::new(8u32, 40u32);
        let texture_crop = Box2D::new(
            client_crop.min + offset,
            client_crop.max + offset);

        assert_eq!(texture_crop.min, Point2D::new(14, 1018));
        assert_eq!(texture_crop.max, Point2D::new(1904, 1114));
    }

    /// Verify at 100% DPI (96 DPI, scale = 1.0).
    #[test]
    fn css_crop_at_100_percent() {
        let scale = 1.0;
        let client_w = 1280.0;
        let client_h = 720.0;

        let viewport = Size2D::<f32>::new(client_w / scale, client_h / scale);
        assert!((viewport.width - 1280.0).abs() < 0.01);

        let css_crop = Box2D::<f32>::new(
            Point2D::new(PADDING, viewport.height - BAR_HEIGHT + PADDING),
            Point2D::new(
                viewport.width - SCROLLBAR_WIDTH - PADDING,
                viewport.height - PADDING));

        let client_crop = Box2D::<u32>::new(
            Point2D::new(
                (css_crop.min.x * scale).floor() as u32,
                (css_crop.min.y * scale).floor() as u32),
            Point2D::new(
                (css_crop.max.x * scale).ceil() as u32,
                (css_crop.max.y * scale).ceil() as u32));

        // At 100%, CSS pixels = physical pixels.
        // min = (4, 720 - 72 + 4) = (4, 652)
        // max = (1280 - 12 - 4, 720 - 4) = (1264, 716)
        assert_eq!(client_crop.min, Point2D::new(4, 652));
        assert_eq!(client_crop.max, Point2D::new(1264, 716));

        // Frame offset (0, 31) — borderless window with 31px title bar at 100%
        let offset = Vector2D::new(0u32, 31u32);
        let texture_crop = Box2D::new(
            client_crop.min + offset,
            client_crop.max + offset);

        assert_eq!(texture_crop.min, Point2D::new(4, 683));
        assert_eq!(texture_crop.max, Point2D::new(1264, 747));
    }
}
