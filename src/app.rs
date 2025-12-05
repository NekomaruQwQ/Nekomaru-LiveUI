use nkprelude::tap::*;
use nkprelude::*;

use wry::{WebView, WebViewBuilder};

use winit::{
    application::ApplicationHandler,
    dpi::LogicalSize,
    dpi::PhysicalSize,
    event::WindowEvent,
    event_loop::EventLoop,
    event_loop::ActiveEventLoop,
    raw_window_handle::HasWindowHandle as _,
    raw_window_handle::RawWindowHandle,
    window::Window,
    window::WindowId,
    window::WindowButtons,
};

use windows::core::*;
use windows::{
    Win32::Foundation::*,
    Win32::Graphics::{
        Dxgi::*,
        Direct3D::*,
        Direct3D11::*,
    },
};

use crate::capture::CaptureSession;

pub fn run() {
    EventLoop::<()>::new()
        .expect("failed to create event loop")
        .pipe(|event_loop| event_loop.run_app(&mut AppWrapper::<LiveApp>(None)))
        .expect("failed to run event loop");
}

fn create_device() -> anyhow::Result<(IDXGIFactory6, ID3D11Device, ID3D11DeviceContext)> {
    let dxgi_factory =
        api_call!(unsafe { CreateDXGIFactory::<IDXGIFactory6>() })?;
    let dxgi_adapter =
        api_call!(unsafe {
            dxgi_factory.EnumAdapterByGpuPreference::<IDXGIAdapter>(
                0,
                DXGI_GPU_PREFERENCE_HIGH_PERFORMANCE)
        })?;

    let DXGI_ADAPTER_DESC { Description: adapter_name, .. } =
        api_call!(unsafe { dxgi_adapter.GetDesc() })?;
    let adapter_name =
        unsafe { widestring::U16CString::from_ptr_str(adapter_name.as_ptr()) }
            .to_string_lossy();
    log::info!("device: {adapter_name}");

    let mut device = None;
    let mut device_context = None;
    api_call!(unsafe {
        D3D11CreateDevice(
            &dxgi_adapter,
            D3D_DRIVER_TYPE_UNKNOWN,
            HMODULE::default(),
            cfg!(debug_assertions)
                .then_some(D3D11_CREATE_DEVICE_DEBUG)
                .unwrap_or_default(),
            Some(&[D3D_FEATURE_LEVEL_11_0]),
            D3D11_SDK_VERSION,
            Some(&raw mut device),
            None,
            Some(&raw mut device_context))
    })?;

    let device =
        device
            .ok_or_else(|| anyhow::anyhow!("failed to create D3D11 device"))?;
    let device_context =
        device_context
            .ok_or_else(|| anyhow::anyhow!("failed to create D3D11 device context"))?;

    Ok((dxgi_factory, device, device_context))
}

#[expect(
    clippy::panic_in_result_fn,
    reason = "running on an unexpected platform is always an unrecoverable error")]
fn get_hwnd_from_window(window: &Window) -> anyhow::Result<HWND> {
    if let RawWindowHandle::Win32(hwnd) = window.window_handle()?.as_raw() {
        Ok(HWND(hwnd.hwnd.get() as _))
    } else {
        panic!("unexpected platform");
    }
}

struct AppWrapper<T>(Option<T>);

// noinspection RsSortImplTraitMembers
impl ApplicationHandler for AppWrapper<LiveApp> {
    fn suspended(&mut self, _: &ActiveEventLoop) {
        let _ = self.0.take();
    }

    fn resumed(&mut self, event_loop: &ActiveEventLoop) {
        let _ = self.0.replace(LiveApp::new(event_loop).expect("fatal error creating app"));
    }

    fn window_event(&mut self, event_loop: &ActiveEventLoop, window_id: WindowId, event: WindowEvent) {
        if event == WindowEvent::CloseRequested {
            event_loop.exit();
        }

        if let Some(app) = self.0.as_mut() {
            app.on_window_event(window_id, event);
        }
    }
}

#[expect(dead_code, reason = "to keep various resources alive")]
struct LiveApp {
    frontend_window: Window,
    frontend_webview: WebView,
    frontend_capture: CaptureSession,

    control_window: Window,
    output_window: Window,
}

impl LiveApp {
    fn new(event_loop: &ActiveEventLoop) -> anyhow::Result<Self> {
        let (dxgi_factory, device, device_context) =
            create_device()
                .context("failed to create graphics context")?;

        let frontend_window = api_call! {
            event_loop.create_window(
                Window::default_attributes()
                    .with_title("Nekomaru LiveUI Web Frontend")
                    .with_inner_size(PhysicalSize::<u32>::new(1920, 1200))
                    .with_resizable(false)
                    .with_enabled_buttons(WindowButtons::CLOSE))
        }?;

        let frontend_webview =
            WebViewBuilder::new()
                .with_url("http://localhost:9688/")
                .build(&frontend_window)
                .context("failed to create webview for frontend window")?;

        let frontend_hwnd =
            get_hwnd_from_window(&frontend_window)
                .context("failed to get window handle for frontend window")?;
        let frontend_capture =
            CaptureSession::capture_window(
                &device,
                &device_context,
                frontend_hwnd)
                .context("failed to start capture session for frontend window")?;

        let control_window = api_call! {
            event_loop.create_window(
                Window::default_attributes()
                    .with_title("Nekomaru LiveUI Control Panel")
                    .with_inner_size(LogicalSize::<u32>::new(960, 600))
                    .with_resizable(false)
                    .with_enabled_buttons(WindowButtons::CLOSE))
        }?;

        let output_window = api_call! {
            event_loop.create_window(
                Window::default_attributes()
                    .with_title("Nekomaru LiveUI Renderer Output")
                    .with_inner_size(PhysicalSize::<u32>::new(1920, 1080))
                    .with_resizable(false)
                    .with_enabled_buttons(WindowButtons::CLOSE))
        }?;

        control_window.set_visible(false); // currently not used

        Ok(Self {
            frontend_window,
            frontend_webview,
            frontend_capture,
            control_window,
            output_window,
        })
    }

    fn on_window_event(&mut self, window_id: WindowId, event: WindowEvent) {
    }
}
