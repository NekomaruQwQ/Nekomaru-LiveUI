//! Capture session wrapper, viewport calculation, and crop specification.
//!
//! Re-exports `winrt_capture::CaptureSession` and provides the
//! aspect-ratio-preserving viewport calculation used by the resample path,
//! plus the crop types and box computation used by the crop path.

pub use winrt_capture::CaptureSession;

use nkcore::prelude::euclid::Size2D;
use windows::Win32::Graphics::Direct3D11::{D3D11_BOX, D3D11_VIEWPORT};

// ── Crop types ──────────────────────────────────────────────────────────────

/// A single crop dimension: either a fixed pixel count or the full source extent.
#[derive(Debug, Clone, Copy)]
pub enum CropDimension {
    Pixels(u32),
    Full,
}

/// Where within the source window the crop rect is anchored.
#[derive(Debug, Clone, Copy, Default)]
pub enum Alignment {
    TopLeft, Top, TopRight,
    Left, #[default] Center, Right,
    BottomLeft, Bottom, BottomRight,
}

/// Specifies a subrect to extract from the captured window.
#[derive(Debug, Clone)]
pub struct CropSpec {
    pub width: CropDimension,
    pub height: CropDimension,
    pub align: Alignment,
}

impl CropSpec {
    /// Resolve `Full` dimensions against a concrete source size, returning the
    /// output (staging / encoder) resolution.  Both axes are clamped to the
    /// source and rounded down to the nearest multiple of 16.
    pub fn resolve_output_size(&self, source: Size2D<u32>) -> Size2D<u32> {
        let w = match self.width {
            CropDimension::Pixels(px) => px.min(source.width),
            CropDimension::Full => source.width,
        };
        let h = match self.height {
            CropDimension::Pixels(px) => px.min(source.height),
            CropDimension::Full => source.height,
        };
        // Round down to a multiple of 16 (encoder requirement).
        Size2D::new(w & !15, h & !15)
    }
}

/// Compute the `D3D11_BOX` that selects the crop region from a source texture.
///
/// `crop_size` is the already-resolved output size (from [`CropSpec::resolve_output_size`]).
/// The box is positioned according to `align` and clamped so it never exceeds
/// the source bounds.
pub fn compute_crop_box(
    crop_size: Size2D<u32>,
    align: Alignment,
    source_size: Size2D<u32>) -> D3D11_BOX {
    let w = crop_size.width.min(source_size.width);
    let h = crop_size.height.min(source_size.height);

    let (ox, oy) = match align {
        Alignment::TopLeft     => (0, 0),
        Alignment::Top         => ((source_size.width - w) / 2, 0),
        Alignment::TopRight    => (source_size.width - w, 0),
        Alignment::Left        => (0, (source_size.height - h) / 2),
        Alignment::Center      => ((source_size.width - w) / 2, (source_size.height - h) / 2),
        Alignment::Right       => (source_size.width - w, (source_size.height - h) / 2),
        Alignment::BottomLeft  => (0, source_size.height - h),
        Alignment::Bottom      => ((source_size.width - w) / 2, source_size.height - h),
        Alignment::BottomRight => (source_size.width - w, source_size.height - h),
    };

    D3D11_BOX {
        left: ox,
        top: oy,
        front: 0,
        right: ox + w,
        bottom: oy + h,
        back: 1,
    }
}

/// Compute a viewport that fits `source_size` into `target_size` with
/// aspect-ratio-preserving letterboxing.
///
/// The result is a `D3D11_VIEWPORT` centered within `target_size`, scaled
/// uniformly so the source fills as much of the target as possible without
/// stretching.
pub fn calculate_resample_viewport(
    source_size: Size2D<u32>,
    target_size: Size2D<u32>) -> D3D11_VIEWPORT {
    let scale =
        f32::min(
            target_size.width as f32 / source_size.width as f32,
            target_size.height as f32 / source_size.height as f32);
    let source_size_scaled =
        (source_size.to_f32() * scale).floor().to_u32();
    let target_offset =
        (target_size - source_size_scaled).to_vector() / 2;

    D3D11_VIEWPORT {
        TopLeftX: target_offset.x as _,
        TopLeftY: target_offset.y as _,
        Width: source_size_scaled.width as _,
        Height: source_size_scaled.height as _,
        MinDepth: 0.0,
        MaxDepth: 1.0,
    }
}
