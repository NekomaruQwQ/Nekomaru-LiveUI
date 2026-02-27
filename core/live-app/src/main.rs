//! Minimal wry webview host for Nekomaru LiveUI.
//!
//! Opens a non-resizable window at the stream resolution and loads the
//! LiveServer frontend. This is a thin shell — all capture, encoding, and
//! stream management lives in `live-capture.exe` and LiveServer.

use nkcore::prelude::*;
use nkcore::os::windows::winit::{
    AppEvent,
    EventLoopExt as _,
};

use clap::Parser;

use winit::{
    dpi::LogicalSize,
    dpi::PhysicalPosition,
    dpi::Size as WindowSize,
    event_loop::EventLoop,
    event::WindowEvent,
    window::Window,
    window::WindowButtons,
};

const WINDOW_SIZE: LogicalSize<u32> = LogicalSize::new(1280, 720);

/// CLI arguments for the webview host.
#[derive(Parser)]
#[command(name = "live-app")]
struct LiveAppArgs {
    /// URL to load in the webview.
    pub url: Option<String>,

    /// Window title. Defaults to the URL if not provided.
    #[arg(long, short = 'm')]
    pub title: Option<String>,

    /// Device scaling factor to use for the webview. If not provided, the
    /// webview will use the system scaling factor, which may cause issues
    /// on high-DPI displays.
    #[arg(long, short = 's')]
    pub scale_factor: Option<f32>,
}

/// Reads `LIVE_PORT` from the environment, panics if not set or invalid,
/// and constructs the server URL.
fn get_server_url() -> String {
    let port = std::env::var("LIVE_PORT")
        .ok()
        .and_then(|port| port.parse::<u16>().ok())
        .expect("LIVE_PORT not set or is not a valid port number");
    format!("http://localhost:{port}")
}

/// Parse CLI arguments and launch the webview. Convenience entry point for
/// binaries that don't need programmatic control over the URL or title.
fn main() {
    pretty_env_logger::init();

    let args = LiveAppArgs::parse();
    let window_title =
        if let Some(title) = args.title.as_ref() {
            Owned(format!("{title} - Nekomaru LiveUI v2"))
        } else {
            Borrowed("Nekomaru LiveUI v2")
        };
    let url = args.url.unwrap_or_else(get_server_url);
    let scale_factor = args.scale_factor;

    log::info!("starting frontend at {url}, scale factor: {scale_factor:?}");

    let mut webview_args = vec![
        // Disable WebView2's background throttling features to prevent the webview
        // from freezing when the window is not in the foreground. This is necessary
        // for streaming.
        Borrowed("--disable-backgrounding-occluded-windows"),
        Borrowed("--disable-renderer-backgrounding"),
    ];

    if let Some(scale_factor) = scale_factor {
        webview_args.push(Owned(format!("--force-device-scale-factor={scale_factor}")));
    }

    // SAFETY: Single-threaded access to environment variable, set before
    // any threads are spawned.
    unsafe {
        std::env::set_var("WEBVIEW2_ADDITIONAL_BROWSER_ARGUMENTS", webview_args.join(" "));
    }

    EventLoop::<()>::new()
        .expect("failed to create event loop")
        .run_app_with(move |event_loop| {
            let window_size: WindowSize =
                if let Some(scale_factor) = scale_factor {
                    WINDOW_SIZE.to_physical::<f64>(scale_factor as f64).into()
                } else {
                    WINDOW_SIZE.into()
                };

            let window =
                event_loop.create_window(
                    Window::default_attributes()
                        .with_title(window_title)
                        .with_inner_size(window_size)
                        .with_resizable(false)
                        .with_enabled_buttons(
                            WindowButtons::CLOSE |
                            WindowButtons::MINIMIZE))
                    .expect("failed to create window");

            let webview =
                wry::WebViewBuilder::new()
                    .with_url(&url)
                    .build(&window)
                    .expect("failed to create webview");

            move |event_loop, event| {
                if let AppEvent::WindowEvent(window_id, event) = event &&
                    window_id == window.id() {
                    match event {
                        WindowEvent::CloseRequested =>
                            event_loop.exit(),
                        WindowEvent::Resized(new_size) => {
                            let _ = webview.set_bounds(wry::Rect {
                                position: PhysicalPosition::new(0, 0).into(),
                                size: new_size.into(),
                            });
                        }
                        _ => {}
                    }
                }
            }
        })
        .expect("failed to run event loop");
}
