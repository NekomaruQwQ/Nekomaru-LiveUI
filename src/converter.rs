use nkcore::*;

use windows::core::Interface as _;
use windows::Win32::Graphics::Direct3D11::*;

/// Stateless format converter helper for converting BGRA textures to NV12
/// using GPU Video Processor.
pub struct NV12Converter {
    device: ID3D11VideoDevice,
    device_context: ID3D11VideoContext,
    processor: ID3D11VideoProcessor,
    enumerator: ID3D11VideoProcessorEnumerator,
}

impl NV12Converter {
    /// Create a new format converter.
    pub fn new(device: &ID3D11Device, device_context: &ID3D11DeviceContext)
        -> anyhow::Result<Self> {
        let device =
            api_call!(device.cast::<ID3D11VideoDevice>())?;
        let device_context =
            api_call!(device_context.cast::<ID3D11VideoContext>())?;

        let desc = D3D11_VIDEO_PROCESSOR_CONTENT_DESC {
            InputFrameFormat: D3D11_VIDEO_FRAME_FORMAT_PROGRESSIVE,
            InputFrameRate: Default::default(),
            InputWidth: 1920,
            InputHeight: 1200,
            OutputFrameRate: Default::default(),
            OutputWidth: 1920,
            OutputHeight: 1200,
            Usage: D3D11_VIDEO_USAGE_PLAYBACK_NORMAL,
        };

        let enumerator = api_call!(unsafe {
            device.CreateVideoProcessorEnumerator(&raw const desc)
        })?;

        let processor = api_call!(unsafe {
            device.CreateVideoProcessor(
                &enumerator,
                /* RateConversionIndex (0 for no rate conversion): */ 0)
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
        &self,
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
            pInputSurface: std::mem::ManuallyDrop::new(Some(input_view)),
            ..default()
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
