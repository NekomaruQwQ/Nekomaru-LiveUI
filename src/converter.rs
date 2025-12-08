use nkcore::*;

use windows::core::Interface as _;
use windows::Win32::Graphics::Direct3D11::*;

/// Stateless format converter helper for converting BGRA textures to NV12
/// using GPU Video Processor.
pub struct FormatConverter {
    device: ID3D11VideoDevice,
    device_context: ID3D11VideoContext,
    processor: ID3D11VideoProcessor,
    enumerator: ID3D11VideoProcessorEnumerator,
}

impl FormatConverter {
    /// Create a new format converter.
    pub fn new(
        device: &ID3D11Device,
        device_context: &ID3D11DeviceContext)
        -> anyhow::Result<Self> {
        // Query video device from D3D11 device
        let device = api_call!(device.cast::<ID3D11VideoDevice>())?;
        // Get device context and cast to video context
        let device_context = api_call!(device_context.cast::<ID3D11VideoContext>())?;

        // Create video processor enumerator descriptor
        let desc = D3D11_VIDEO_PROCESSOR_CONTENT_DESC {
            InputFrameFormat: D3D11_VIDEO_FRAME_FORMAT_PROGRESSIVE,
            InputFrameRate: Default::default(),
            InputWidth: 3840,   // Max expected width
            InputHeight: 2160,  // Max expected height
            OutputFrameRate: Default::default(),
            OutputWidth: 3840,
            OutputHeight: 2160,
            Usage: D3D11_VIDEO_USAGE_PLAYBACK_NORMAL,
        };

        // Create video processor enumerator
        let enumerator = api_call!(unsafe {
            device.CreateVideoProcessorEnumerator(&raw const desc)
        })?;

        // Create video processor
        let processor = api_call!(unsafe {
            device.CreateVideoProcessor(
                &enumerator,
                // RateConversionIndex (0 for no rate conversion)
                0)
        })?;


        Ok(Self {
            device,
            device_context,
            processor,
            enumerator,
        })
    }

    /// Convert BGRA texture to NV12.
    ///
    /// # Arguments
    /// * `bgra_texture` - Input BGRA texture (DXGI_FORMAT_B8G8R8A8_UNORM)
    /// * `nv12_texture` - Output NV12 texture (DXGI_FORMAT_NV12), must be pre-allocated by caller
    ///
    /// # Errors
    /// Returns error if video processing fails
    pub fn convert(
        &mut self,
        bgra_texture: &ID3D11Texture2D,
        nv12_texture: &ID3D11Texture2D)
        -> anyhow::Result<()> {
        // Create input view
        let input_view_desc = D3D11_VIDEO_PROCESSOR_INPUT_VIEW_DESC {
            FourCC: 0,
            ViewDimension: D3D11_VPIV_DIMENSION_TEXTURE2D,
            Anonymous: D3D11_VIDEO_PROCESSOR_INPUT_VIEW_DESC_0 {
                Texture2D: D3D11_TEX2D_VPIV {
                    MipSlice: 0,
                    ArraySlice: 0,
                },
            },
        };

        let mut input_view = None;
        api_call!(unsafe {
            self.device.CreateVideoProcessorInputView(
                bgra_texture,
                &self.enumerator,
                &raw const input_view_desc,
                Some(&raw mut input_view))
        })?;
        let input_view = input_view.ok_or_else(|| anyhow::anyhow!("input view is null"))?;

        // Create output view
        let output_view_desc = D3D11_VIDEO_PROCESSOR_OUTPUT_VIEW_DESC {
            ViewDimension: D3D11_VPOV_DIMENSION_TEXTURE2D,
            Anonymous: D3D11_VIDEO_PROCESSOR_OUTPUT_VIEW_DESC_0 {
                Texture2D: D3D11_TEX2D_VPOV {
                    MipSlice: 0,
                },
            },
        };

        let mut output_view = None;
        api_call!(unsafe {
            self.device.CreateVideoProcessorOutputView(
                nv12_texture,
                &self.enumerator,
                &raw const output_view_desc,
                Some(&raw mut output_view))
        })?;
        let output_view = output_view.ok_or_else(|| anyhow::anyhow!("output view is null"))?;

        // Setup stream for video processor
        let stream = D3D11_VIDEO_PROCESSOR_STREAM {
            Enable: true.into(),
            OutputIndex: 0,
            InputFrameOrField: 0,
            PastFrames: 0,
            FutureFrames: 0,
            ppPastSurfaces: std::ptr::null_mut(),
            ppFutureSurfaces: std::ptr::null_mut(),
            pInputSurface: std::mem::ManuallyDrop::new(Some(input_view)),
            ppPastSurfacesRight: std::ptr::null_mut(),
            ppFutureSurfacesRight: std::ptr::null_mut(),
            pInputSurfaceRight: std::mem::ManuallyDrop::new(None),
        };

        // Perform video processing (BGRA → NV12 conversion)
        api_call!(unsafe {
            self.device_context.VideoProcessorBlt(
                &self.processor,
                &output_view,
                0,  // OutputFrame
                &[stream])
        }).context("failed to perform BGRA→NV12 conversion")?;

        Ok(())
    }
}
