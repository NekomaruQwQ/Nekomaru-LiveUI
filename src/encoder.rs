#![expect(clippy::multiple_unsafe_ops_per_block)]

use nkcore::euclid::*;
use nkcore::*;

use windows::core::*;
use windows::{
    Win32::Foundation::*,
    Win32::Graphics::Direct3D11::*,
    Win32::Media::MediaFoundation::*,
    Win32::System::Com::*,
    Win32::System::Variant::*,
};

/// NAL unit types for H.264
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum NALUnitType {
    /// Non-IDR slice
    NonIDR = 1,
    /// Instantaneous Decoder Refresh (keyframe)
    IDR = 5,
    /// Sequence Parameter Set
    SPS = 7,
    /// Picture Parameter Set
    PPS = 8,
}

impl NALUnitType {
    /// Parse NAL unit type from NAL unit header byte
    const fn from_header(header: u8) -> Option<Self> {
        match header & 0x1F {
            1 => Some(Self::NonIDR),
            5 => Some(Self::IDR),
            7 => Some(Self::SPS),
            8 => Some(Self::PPS),
            _ => None,
        }
    }
}

/// Encoded H.264 NAL unit
#[derive(Debug, Clone)]
pub struct NALUnit {
    /// Type of this NAL unit
    pub unit_type: NALUnitType,
    /// Raw NAL unit data (including start code)
    pub data: Vec<u8>,
    /// Timestamp in microseconds
    pub timestamp_us: u64,
}

/// H.264 encoder using Windows Media Foundation
pub struct H264Encoder {
    mf_dxgi_manager: IMFDXGIDeviceManager,
    mf_transform: IMFTransform,
    frame_size: Size2D<u32>,
    frame_count: u64,
}

impl H264Encoder {
    /// Create a new H.264 encoder.
    ///
    /// # Arguments
    /// * `device` - D3D11 device for DXGI integration
    /// * `width` - Video width
    /// * `height` - Video height
    /// * `frame_rate` - Target frame rate (e.g., 60)
    /// * `bitrate` - Target bitrate in bits per second (e.g., 8_000_000 for 8 Mbps)
    pub fn new(
        device: &ID3D11Device,
        frame_size: Size2D<u32>,
        frame_rate: u32,
        bitrate: u32)
        -> anyhow::Result<Self> {
        // Initialize Media Foundation
        api_call!(unsafe { MFStartup(MF_VERSION, MFSTARTUP_NOSOCKET) })?;

        // Create DXGI Device Manager
        let mut reset_token = 0u32;
        let mut mf_dxgi_manager = None;
        api_call!(unsafe {
            MFCreateDXGIDeviceManager(
                &raw mut reset_token,
                &raw mut mf_dxgi_manager)
        })?;

        let mf_dxgi_manager =
            mf_dxgi_manager.ok_or_else(|| anyhow::anyhow!("DXGI device manager is null"))?;

        // Register D3D11 device with DXGI manager
        api_call!(unsafe { mf_dxgi_manager.ResetDevice(device, reset_token) })?;

        // Find H.264 encoder transform
        let mf_transform = Self::find_h264_encoder()
            .context("failed to find H264Encoder")?;

        // Configure input type (NV12) and output type (H.264)
        Self::configure_input_type(&mf_transform, frame_size, frame_rate)
            .context("failed to configure encoder input type")?;
        Self::configure_output_type(&mf_transform, frame_size, frame_rate, bitrate)
            .context("failed to configure encoder output type")?;

        // Attach DXGI manager to transform
        api_call!(unsafe {
            mf_transform.ProcessMessage(
                MFT_MESSAGE_SET_D3D_MANAGER,
                &raw const mf_dxgi_manager as *const _ as usize)
        })?;

        // Start streaming
        api_call!(unsafe {
            mf_transform.ProcessMessage(
                MFT_MESSAGE_NOTIFY_BEGIN_STREAMING,
                0)
        })?;

        api_call!(unsafe {
            mf_transform.ProcessMessage(
                MFT_MESSAGE_NOTIFY_START_OF_STREAM,
                0)
        })?;

        log::info!(
            "H.264 encoder initialized ({}x{} @ {}fps, {} bps)",
            frame_size.width,
            frame_size.height,
            frame_rate,
            bitrate);

        Ok(Self {
            mf_dxgi_manager,
            mf_transform,
            frame_size,
            frame_count: 0,
        })
    }

    /// Find a hardware H.264 encoder transform
    fn find_h264_encoder() -> anyhow::Result<IMFTransform> {
        static INPUT_TYPE: MFT_REGISTER_TYPE_INFO = MFT_REGISTER_TYPE_INFO {
            guidMajorType: MFMediaType_Video,
            guidSubtype: MFVideoFormat_NV12,
        };

        static OUTPUT_TYPE: MFT_REGISTER_TYPE_INFO = MFT_REGISTER_TYPE_INFO {
            guidMajorType: MFMediaType_Video,
            guidSubtype: MFVideoFormat_H264,
        };

        // Search for hardware encoder
        let mut out_activate = std::ptr::null_mut();
        let mut out_count = 0u32;
        api_call!(unsafe {
            MFTEnumEx(
                MFT_CATEGORY_VIDEO_ENCODER,
                MFT_ENUM_FLAG_HARDWARE,
                Some(&raw const INPUT_TYPE),
                Some(&raw const OUTPUT_TYPE),
                &raw mut out_activate,
                &raw mut out_count)
        })?;

        if out_activate.is_null() || out_count == 0 {
            anyhow::bail!("no hardware H.264 encoder found");
        }

        log::info!("hardware H.264 encoder found");
        defer(|| unsafe { CoTaskMemFree(Some(out_activate.cast())) });
        let activate =
            unsafe { std::ptr::read(out_activate) }
                .ok_or_else(|| anyhow::anyhow!("unexpected null pointer"))?;
        Ok(api_call!(unsafe {
            activate.ActivateObject::<IMFTransform>()
        })?)
    }

    /// Configure input media type (NV12)
    fn configure_input_type(
        transform: &IMFTransform,
        frame_size: Size2D<u32>,
        frame_rate: u32)
        -> anyhow::Result<()> {
        let input_type = api_call!(unsafe { MFCreateMediaType() })?;

        api_call!(unsafe { input_type.SetGUID(&MF_MT_MAJOR_TYPE, &MFMediaType_Video) })
            .with_context(|| context!("failed to set input major type to video"))?;
        api_call!(unsafe { input_type.SetGUID(&MF_MT_SUBTYPE, &MFVideoFormat_NV12) })
            .with_context(|| context!("failed to set input subtype to NV12"))?;

        let frame_size =
            ((frame_size.width as u64) << 32) |
            (frame_size.height as u64);
        api_call!(unsafe { input_type.SetUINT64(&MF_MT_FRAME_SIZE, frame_size) })
            .with_context(|| context!("failed to set input frame size"))?;

        let frame_rate_ratio = ((frame_rate as u64) << 32) | 1u64;
        api_call!(unsafe { input_type.SetUINT64(&MF_MT_FRAME_RATE, frame_rate_ratio) })
            .with_context(|| context!("failed to set input frame rate"))?;

        api_call!(unsafe { transform.SetInputType(0, &input_type, 0) })
            .with_context(|| context!("failed to set encoder input type"))?;
        Ok(())
    }

    /// Configure output media type (H.264) with low-latency settings
    fn configure_output_type(
        transform: &IMFTransform,
        frame_size: Size2D<u32>,
        frame_rate: u32,
        bitrate: u32)
        -> anyhow::Result<()> {
        let output_type = api_call!(unsafe { MFCreateMediaType() })?;

        api_call!(unsafe { output_type.SetGUID(&MF_MT_MAJOR_TYPE, &MFMediaType_Video) })
            .with_context(|| context!("setting output major type"))?;
        api_call!(unsafe { output_type.SetGUID(&MF_MT_SUBTYPE, &MFVideoFormat_H264) })
            .with_context(|| context!("setting output subtype to H.264"))?;

        api_call!(unsafe { output_type.SetUINT32(&MF_MT_AVG_BITRATE, bitrate) })
            .with_context(|| context!("setting output bitrate"))?;

        let frame_size =
            ((frame_size.width as u64) << 32) |
            (frame_size.height as u64);
        api_call!(unsafe { output_type.SetUINT64(&MF_MT_FRAME_SIZE, frame_size) })
            .with_context(|| context!("setting output frame size"))?;

        let frame_rate_ratio = ((frame_rate as u64) << 32) | 1u64;
        api_call!(unsafe { output_type.SetUINT64(&MF_MT_FRAME_RATE, frame_rate_ratio) })
            .with_context(|| context!("setting output frame rate"))?;

        // Baseline profile for maximum compatibility
        api_call!(unsafe { output_type.SetUINT32(&MF_MT_MPEG2_PROFILE, eAVEncH264VProfile_Base.0 as u32) })
            .with_context(|| context!("setting H.264 profile to baseline"))?;

        api_call!(unsafe { transform.SetOutputType(0, &output_type, 0) })
            .with_context(|| context!("setting encoder output type"))?;

        // Configure low-latency settings via ICodecAPI
        let codec_api = api_call!(transform.cast::<ICodecAPI>())?;

        fn make_variant_u32(value: u32) -> VARIANT {
            let mut variant = VARIANT::default();
            let inner = unsafe { &mut *variant.Anonymous.Anonymous };
            inner.vt = VT_UI4;
            inner.Anonymous.ulVal = value;
            variant
        }

        fn make_variant_bool(value: bool) -> VARIANT {
            let mut variant = VARIANT::default();
            let inner = unsafe { &mut *variant.Anonymous.Anonymous };
            inner.vt = VT_BOOL;
            inner.Anonymous.boolVal = VARIANT_BOOL::from(value);
            variant
        }

        for (api, value) in [
            // No B-frames for low latency
            (CODECAPI_AVEncMPVDefaultBPictureCount,
                make_variant_u32(0)),
            // GOP size = 2 seconds
            (CODECAPI_AVEncMPVGOPSize,
                make_variant_u32(frame_rate * 2)),
            // Low latency mode
            (CODECAPI_AVLowLatencyMode,
                make_variant_bool(true)),
            // CBR rate control
            (CODECAPI_AVEncCommonRateControlMode,
                make_variant_u32(eAVEncCommonRateControlMode_CBR.0 as _)),
        ] {
            api_call!(unsafe { codec_api.SetValue(&api, &value) })
                .with_context(|| context!("failed to set codec API value"))?;
        }

        Ok(())
    }

    /// Encode a single NV12 frame to H.264 NAL units.
    ///
    /// # Arguments
    /// * `nv12_texture` - Input NV12 texture
    /// * `timestamp_us` - Timestamp in microseconds
    ///
    /// # Returns
    /// Vector of encoded NAL units. May be empty if encoder is buffering.
    pub fn encode_frame(
        &mut self,
        nv12_texture: &ID3D11Texture2D,
        timestamp_us: u64)
        -> anyhow::Result<Vec<NALUnit>> {
        // Create DXGI surface buffer from texture
        let buffer = api_call!(unsafe {
            MFCreateDXGISurfaceBuffer(
                &ID3D11Texture2D::IID,
                nv12_texture,
                0,
                false)
        })?;

        // Create MF sample
        let sample = api_call!(unsafe { MFCreateSample() })?;

        // Add buffer to sample
        api_call!(unsafe { sample.AddBuffer(&buffer) })?;

        // Set sample time (convert μs to 100ns units)
        api_call!(unsafe { sample.SetSampleTime((timestamp_us * 10) as i64) })?;

        // Set sample duration (frame duration at target fps)
        let duration_100ns = (1_000_000 * 10) / 60;  // ~16.666ms for 60fps
        api_call!(unsafe { sample.SetSampleDuration(duration_100ns) })?;

        // Feed sample to encoder
        api_call!(unsafe { self.mf_transform.ProcessInput(0, &sample, 0) })?;

        // Drain encoded output
        let nal_units = self.drain_encoder_output(timestamp_us)?;

        self.frame_count += 1;

        Ok(nal_units)
    }

    /// Drain encoded NAL units from encoder
    fn drain_encoder_output(&self, timestamp_us: u64) -> anyhow::Result<Vec<NALUnit>> {
        let mut nal_units = Vec::new();

        loop {
            let mut output_buffers = [MFT_OUTPUT_DATA_BUFFER::default()];
            let mut status = 0u32;
            let result = unsafe {
                self.mf_transform.ProcessOutput(
                    0,
                    &mut output_buffers,
                    &raw mut status)
            };

            match result {
                Ok(_) => {
                    if let Some(sample) = output_buffers[0].pSample.take() {
                        // Convert to contiguous buffer and parse NAL units
                        match api_call!(unsafe { sample.ConvertToContiguousBuffer() })
                            .with_context(|| context!("converting to contiguous buffer"))
                        {
                            Ok(buffer) => {
                                match Self::parse_nal_units_from_buffer(&buffer, timestamp_us) {
                                    Ok(units) => nal_units.extend(units),
                                    Err(e) => {
                                        log::warn!("Failed to parse NAL units: {:?}", e);
                                        // Continue - skip this frame's NAL units
                                    }
                                }
                            }
                            Err(e) => {
                                log::warn!("Failed to convert buffer: {:?}", e);
                                // Continue - skip this frame
                            }
                        }
                    }
                }
                Err(err) if err.code() == MF_E_TRANSFORM_NEED_MORE_INPUT => {
                    // Normal condition - encoder needs more input
                    break;
                }
                Err(err) => {
                    log::error!("ProcessOutput failed: {:?}", err);
                    break;
                }
            }
        }

        Ok(nal_units)
    }

    /// Parse NAL units from encoder output buffer
    fn parse_nal_units_from_buffer(buffer: &IMFMediaBuffer, timestamp_us: u64)
        -> anyhow::Result<Vec<NALUnit>> {
        // Lock buffer to access raw data
        let buffer_lock = BufferLock::lock(buffer)?;
        let data = buffer_lock.as_slice();

        let mut nal_units = Vec::new();
        let mut i = 0;

        while i < data.len() {
            // Look for start code (00 00 00 01 or 00 00 01)
            let start_code_len = if i + 3 < data.len()
                && data[i] == 0x00
                && data[i + 1] == 0x00
                && data[i + 2] == 0x00
                && data[i + 3] == 0x01 {
                4
            } else if i + 2 < data.len()
                && data[i] == 0x00
                && data[i + 1] == 0x00
                && data[i + 2] == 0x01 {
                3
            } else {
                i += 1;
                continue;
            };

            // Parse NAL unit header
            let nal_header_pos = i + start_code_len;
            if nal_header_pos >= data.len() {
                break;
            }

            let nal_header = data[nal_header_pos];
            let nal_type = NALUnitType::from_header(nal_header);

            // Find next start code
            let mut next_start = data.len();
            for j in (nal_header_pos + 1)..data.len() {
                if j + 3 < data.len()
                    && data[j] == 0x00
                    && data[j + 1] == 0x00
                    && data[j + 2] == 0x00
                    && data[j + 3] == 0x01 {
                    next_start = j;
                    break;
                }
                if j + 2 < data.len()
                    && data[j] == 0x00
                    && data[j + 1] == 0x00
                    && data[j + 2] == 0x01 {
                    next_start = j;
                    break;
                }
            }

            // Extract NAL unit data (including start code)
            if let Some(unit_type) = nal_type {
                let data = data[i..next_start].to_vec();
                nal_units.push(NALUnit {
                    unit_type,
                    data,
                    timestamp_us,
                });
            }

            i = next_start;
        }

        Ok(nal_units)
    }
}

/// RAII guard for IMFMediaBuffer::Lock/Unlock
struct BufferLock<'a> {
    buffer: &'a IMFMediaBuffer,
    ptr: *mut u8,
    len: usize,
}

impl<'a> BufferLock<'a> {
    fn lock(buffer: &'a IMFMediaBuffer) -> anyhow::Result<Self> {
        let mut ptr = std::ptr::null_mut();
        let mut len = 0u32;

        api_call!(unsafe {
            buffer.Lock(
                &raw mut ptr,
                None,
                Some(&raw mut len))
        })?;

        Ok(Self { buffer, ptr, len: len as _ })
    }

    const fn as_slice(&self) -> &[u8] {
        unsafe { std::slice::from_raw_parts(self.ptr, self.len) }
    }
}

impl Drop for BufferLock<'_> {
    fn drop(&mut self) {
        unsafe {
            // Ignore error - we're in Drop, can't propagate
            let _ = self.buffer.Unlock();
        }
    }
}
