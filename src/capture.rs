#![expect(clippy::multiple_unsafe_ops_per_block)]

use nkcore::euclid::*;
use nkcore::*;

use windows::core::Interface as _;
use windows::{
    Graphics::*,
    Graphics::Capture::*,
    Graphics::DirectX::*,
    Graphics::DirectX::Direct3D11::*,
    UI::*,
    Win32::Foundation::*,
    Win32::Graphics::Dxgi::Common::*,
    Win32::Graphics::Dxgi::*,
    Win32::Graphics::Direct3D11::*,
    Win32::System::WinRT::Direct3D11::*,
};

pub struct CaptureSession {
    device: ID3D11Device,
    device_context: ID3D11DeviceContext,
    frame_texture: ID3D11Texture2D,
    winrt_device: IDirect3DDevice,
    frame_pool: Direct3D11CaptureFramePool,
    frame_pool_size: SizeInt32,

    _session: GraphicsCaptureSession, // The GraphicsCaptureSession object must be kept alive.
}

impl CaptureSession {
    pub fn new(
        device: &ID3D11Device,
        device_context: &ID3D11DeviceContext,
        capture_item: &GraphicsCaptureItem)
        -> anyhow::Result<Self> {
        let frame_texture =
            Self::create_texture(
                device,
                DXGI_FORMAT_B8G8R8A8_UNORM_SRGB,
                SizeInt32 { Width: 1, Height: 1 })?;
        let dxgi_device =
            api_call!(device.cast::<IDXGIDevice>())?;
        let winrt_device =
            api_call!(unsafe { CreateDirect3D11DeviceFromDXGIDevice(&dxgi_device) })?;
        let winrt_device =
            api_call!(winrt_device.cast::<IDirect3DDevice>())?;
        let frame_pool = api_call! {
            Direct3D11CaptureFramePool::CreateFreeThreaded(
                &winrt_device,
                DirectXPixelFormat::B8G8R8A8UIntNormalized,
                2,
                SizeInt32 { Width: 1, Height: 1 })
        }?;
        let session =
            api_call!(frame_pool.CreateCaptureSession(capture_item))?;
        api_call!(session.StartCapture())?;

        Ok(Self {
            device: device.clone(),
            device_context: device_context.clone(),
            frame_texture,
            winrt_device,
            frame_pool,
            frame_pool_size: SizeInt32 { Width: 1, Height: 1 },
            _session: session,
        })
    }

    pub fn capture_window(
        device: &ID3D11Device,
        device_context: &ID3D11DeviceContext,
        hwnd: HWND)
        -> anyhow::Result<Self> {
        let capture_item =
            api_call!(GraphicsCaptureItem::TryCreateFromWindowId(WindowId { Value: hwnd.0 as _ }))?;
        Self::new(device, device_context, &capture_item)
    }

    pub const fn frame_buffer(&self) -> &ID3D11Texture2D {
        &self.frame_texture
    }

    pub const fn frame_buffer_size(&self) -> Size2D<u32> {
        Size2D::new(
            self.frame_pool_size.Width as _,
            self.frame_pool_size.Height as _)
    }

    pub fn update(&mut self) {
        if let Err(err) = self.update_internal() {
            log::error!("{} failed: {err:?}", pretty_name::of_method!(Self::update));
        }
    }

    fn update_internal(&mut self) -> anyhow::Result<()> {
        let mut last_frame = None;
        while let Ok(frame) = self.frame_pool.TryGetNextFrame() {
            last_frame = Some(frame);
        }

        let Some(frame) = last_frame else {
            return Ok(());
        };

        let new_size = frame.ContentSize()?;
        if new_size != self.frame_pool_size {
            self.frame_pool_size = new_size;
            self.frame_pool.Recreate(
                &self.winrt_device,
                DirectXPixelFormat::B8G8R8A8UIntNormalized,
                2,
                new_size)?;
            self.frame_texture =
                Self::create_texture(
                    &self.device,
                    DXGI_FORMAT_B8G8R8A8_UNORM,
                    new_size)?;
            log::info!(
                "{} resized to {}x{}",
                pretty_name::of_field!(Self::frame_texture),
                new_size.Width,
                new_size.Height);
            return Ok(());
        }

        let frame_texture = unsafe {
            frame
                .pipe(|frame| api_call!(frame.Surface()))?
                .pipe(|frame| api_call!(frame.cast::<IDirect3DDxgiInterfaceAccess>()))?
                .pipe(|frame| api_call!(frame.GetInterface::<ID3D11Texture2D>()))?
        };

        unsafe {
            self.device_context
                .CopyResource(&self.frame_texture, &frame_texture);
        }

        Ok(())
    }

    fn create_texture(device: &ID3D11Device, format: DXGI_FORMAT, size: SizeInt32)
                      -> anyhow::Result<ID3D11Texture2D> {
        let desc = D3D11_TEXTURE2D_DESC {
            Width: size.Width as _,
            Height: size.Height as _,
            MipLevels: 1,
            ArraySize: 1,
            Format: format,
            SampleDesc: DXGI_SAMPLE_DESC { Count: 1, Quality: 0 },
            Usage: D3D11_USAGE_DEFAULT,
            BindFlags: D3D11_BIND_SHADER_RESOURCE.0 as _,
            CPUAccessFlags: 0,
            MiscFlags: 0,
        };

        let mut texture = None;
        api_call!(unsafe {
            device.CreateTexture2D(
                &raw const desc,
                None,
                Some(&raw mut texture))
        })?;

        Ok(texture.expect("unexpected null pointer"))
    }
}
