use nkcore::prelude::*;
use nkcore::debug::*;
use nkcore::*;

use windows::Win32::{
    Graphics::Dxgi::IDXGIDevice,
    Media::MediaFoundation::*,
    System::Com::*,
};

pub fn find_h264_encoder(dxgi_device: &IDXGIDevice) -> anyhow::Result<IMFTransform> {
    static INPUT_TYPE: MFT_REGISTER_TYPE_INFO = MFT_REGISTER_TYPE_INFO {
        guidMajorType: MFMediaType_Video,
        guidSubtype: MFVideoFormat_NV12,
    };

    static OUTPUT_TYPE: MFT_REGISTER_TYPE_INFO = MFT_REGISTER_TYPE_INFO {
        guidMajorType: MFMediaType_Video,
        guidSubtype: MFVideoFormat_H264,
    };

    // --- Get the LUID from the Device ---
    let dxgi_adapter = api_call!(unsafe { dxgi_device.GetAdapter() })?;
    let dxgi_adapter_desc = api_call!(unsafe { dxgi_adapter.GetDesc() })?;

    // Search for hardware encoder (async)
    let mut out_activate = std::ptr::null_mut();
    let mut out_count = 0u32;
    api_call!(unsafe {
            MFTEnumEx(
                MFT_CATEGORY_VIDEO_ENCODER,
                MFT_ENUM_FLAG_HARDWARE | MFT_ENUM_FLAG_ASYNCMFT,
                Some(&raw const INPUT_TYPE),
                Some(&raw const OUTPUT_TYPE),
                &raw mut out_activate,
                &raw mut out_count)
        })?;

    if out_activate.is_null() || out_count == 0 {
        anyhow::bail!("no hardware H.264 encoder found");
    }

    log::info!("found {} hardware H.264 encoder(s)", out_count);
    defer(|| unsafe { CoTaskMemFree(Some(out_activate.cast())) });

    // Convert the raw pointer to a slice so we can iterate safely
    let activates = unsafe { std::slice::from_raw_parts(out_activate, out_count as usize) };

    log::info!("scanning {} hardware H.264 encoder(s) for matching adapter...", out_count);
    for (index, activate) in activates.iter().enumerate() {
        // Check if the pointer is valid
        let activate =
            activate
                .as_ref()
                .ok_or_else(|| anyhow::anyhow!("null activate pointer at index {}", index))?;

        // Hardware MFTs expose MFT_ENUM_ADAPTER_LUID (as UINT64).
        // This tells us which physical GPU this encoder belongs to.
        // let mft_luid_val = api_call!(unsafe { activate.GetUINT64(&MFT_ENUM_ADAPTER_LUID) })?;
        //
        // // Convert UINT64 back to LUID struct for comparison
        // // LUID is essentially { LowPart: u32, HighPart: i32 }
        // let mft_luid = LUID {
        //     LowPart: mft_luid_val as u32,
        //     HighPart: (mft_luid_val >> 32) as i32,
        // };
        //
        // log::info!(
        //     "encoder index {} has LUID {{ LowPart: {}, HighPart: {} }}",
        //     index,
        //     mft_luid.LowPart,
        //     mft_luid.HighPart);
        // log::info!(
        //     "current adapter LUID {{ LowPart: {}, HighPart: {} }}",
        //     dxgi_adapter_desc.AdapterLuid.LowPart,
        //     dxgi_adapter_desc.AdapterLuid.HighPart);
        //
        // if mft_luid == dxgi_adapter_desc.AdapterLuid {
        //     log::info!("Found matching encoder at index {}", index);
        //     return Ok(api_call!(unsafe {
        //         activate.ActivateObject::<IMFTransform>()
        //     })?);
        // }

        let mut buf = [0u16; 256];
        let mut len = 0u32;
        api_call!(unsafe {
                activate.GetString(
                    &MFT_FRIENDLY_NAME_Attribute,
                    &mut buf,
                    Some(&raw mut len))
            })?;

        let name = unsafe {
            widestring::U16Str::from_ptr(
                buf.as_ptr(),
                len as _)
        };

        let name = name.to_string_lossy();
        log::info!("Encoder #{}: '{}'", index, name);

        // Activate this encoder
        if name.to_ascii_lowercase().contains("nvidia") {
            log::info!("Selecting encoder #{} ('{}')", index, name);
            return Ok(api_call!(unsafe {
                    activate.ActivateObject::<IMFTransform>()
                })?);
        }
    }

    anyhow::bail!("no matching hardware H.264 encoder found for the current adapter");
}

/// Configure input media type (NV12)
#[cfg(false)]
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
#[cfg(false)]
fn configure_output_type(
    transform: &IMFTransform,
    frame_size: Size2D<u32>,
    frame_rate: u32,
    bitrate: u32)
    -> anyhow::Result<()> {
    let output_type = api_call!(unsafe { MFCreateMediaType() })?;

    let frame_size_packed =
        ((frame_size.width as u64) << 32) | (frame_size.height as u64);
    let frame_rate_packed =
        ((frame_rate as u64) << 32) | 1u64;

    api_call!(unsafe { output_type.SetGUID(&MF_MT_MAJOR_TYPE, &MFMediaType_Video) })
        .context("failed to set output major type")?;
    api_call!(unsafe { output_type.SetGUID(&MF_MT_SUBTYPE, &MFVideoFormat_H264) })
        .context("failed to set output subtype to H.264")?;
    api_call!(unsafe { output_type.SetUINT64(&MF_MT_FRAME_SIZE, frame_size_packed) })
        .context("failed to set output frame size")?;
    api_call!(unsafe { output_type.SetUINT64(&MF_MT_FRAME_RATE, frame_rate_packed) })
        .context("failed to set output frame rate")?;
    api_call!(unsafe { output_type.SetUINT32(&MF_MT_AVG_BITRATE, bitrate) })
        .context("failed to set output bitrate")?;

    // Baseline profile for maximum compatibility
    api_call!(unsafe { output_type.SetUINT32(&MF_MT_MPEG2_PROFILE, eAVEncH264VProfile_Base.0 as u32) })
        .context("failed to set H.264 profile to baseline")?;

    // Hardware encoders fail if you don't explicitly say "Progressive"
    api_call!(unsafe {
            output_type.SetUINT32(
                &MF_MT_INTERLACE_MODE,
                MFVideoInterlace_Progressive.0 as u32)
        })
        .context("failed to set interlace mode to progressive")?;

    // --- [FIX] Explicitly Set Profile and Level ---
    // eAVEncH264VProfile_High = {54041196-23BB-45F7-9684-8073A642E325}
    // eAVEncH264VLevel5_1     = 51 (decimal)

    // // Note: You might need to define these GUIDs/Constants if windows-rs
    // // doesn't export them conveniently in the version you are using.
    // // eAVEncH264VProfile_High
    // api_call!(unsafe {
    //     output_type.SetUINT32(&MF_MT_MPEG2_PROFILE, eAVEncH264VProfile_High.0 as _)
    // })?;
    //
    // // eAVEncH264VLevel5_1 (Enum value 51)
    // // This unlocks 4K resolution and higher macroblock throughput.
    // api_call!(unsafe {
    //     output_type.SetUINT32(&MF_MT_MPEG2_LEVEL, eAVEncH264VLevel5_1.0 as _)
    // })?;

    api_call!(unsafe { transform.SetOutputType(0, &output_type, 0) })
        .context("failed to set encoder output type")?;


    api_call!(unsafe { transform.SetOutputType(0, &output_type, 0) })
        .context("failed to set encoder output type")?;

    // Configure low-latency settings via ICodecAPI
    let codec_api = api_call!(transform.cast::<ICodecAPI>())?;

    for (api, value) in [
        // No B-frames for low latency
        (CODECAPI_AVEncMPVDefaultBPictureCount, VARIANT::from(0)),
        // GOP size = 2 seconds
        (CODECAPI_AVEncMPVGOPSize, VARIANT::from(frame_rate * 2)),
        // Low latency mode
        (CODECAPI_AVLowLatencyMode, VARIANT::from(true)),
        // CBR rate control
        (CODECAPI_AVEncCommonRateControlMode,
         VARIANT::from(eAVEncCommonRateControlMode_CBR.0 as u32)),
    ] {
        api_call!(unsafe { codec_api.SetValue(&api, &value) })
            .context("failed to set codec API value")?;
    }

    Ok(())
}
