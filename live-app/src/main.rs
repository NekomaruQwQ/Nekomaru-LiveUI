//! Minimal wry webview host for Nekomaru LiveUI.
//!
//! Opens a non-resizable window at the stream resolution and loads the
//! LiveServer frontend. This is a thin shell — all capture, encoding, and
//! stream management lives in `live-video.exe` and LiveServer.

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

/// CLI arguments for the webview host.
#[derive(Parser)]
#[command(name = "live-app", about = "Minimal wry webview host for Nekomaru LiveUI")]
struct LiveAppArgs {
    /// URL to load in the webview. If not provided, defaults to
    /// `http://localhost:{LIVE_PORT}`.
    pub url: String,

    /// Initial width of the window in logical pixels. Must be provided if
    /// `height` is provided.
    ///
    /// Logical pixels are converted to physical pixels using the provided
    /// `scale_factor` or the system scaling factor if `scale_factor` is not
    /// provided.
    #[arg(long, short = 'x', requires = "height", default_value = "1280")]
    pub width: u32,

    /// Initial height of the window in logical pixels. Must be provided if
    /// `width` is provided.
    ///
    /// Logical pixels are converted to physical pixels using the provided
    /// `scale_factor` or the system scaling factor if `scale_factor` is not
    /// provided.
    #[arg(long, short = 'y', requires = "width", default_value = "800")]
    pub height: u32,

    /// Window title to use for the webview. If not provided, defaults to
    /// "Nekomaru LiveUI v2".
    #[arg(long, short = 't')]
    pub title: Option<String>,

    /// Device scaling factor to use for the webview. If not provided, the
    /// webview will use the system scaling factor, which may cause issues
    /// on high-DPI displays.
    #[arg(long, short = 's')]
    pub scale_factor: Option<f32>,
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
    let window_size =
        LogicalSize::new(args.width, args.height);
    let window_size =
        if let Some(scale_factor) = args.scale_factor {
            WindowSize::from(window_size.to_physical::<f64>(scale_factor as f64))
        } else {
            WindowSize::from(window_size)
        };

    let url = args.url;
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
