mod app;
mod client;
mod model;

fn main() -> eframe::Result {
    let port: u16 = std::env::var("LIVE_PORT")
        .ok()
        .and_then(|value| value.parse().ok())
        .expect("LIVE_PORT not set or is not a valid port number");
    let server_url = format!("http://localhost:{port}");

    let options = eframe::NativeOptions {
        viewport: eframe::egui::ViewportBuilder::default()
            .with_inner_size([480.0, 640.0])
            .with_title("LiveUI Control"),
        ..Default::default()
    };

    eframe::run_native(
        "live-control",
        options,
        Box::new(|cc| Ok(Box::new(app::ControlApp::new(cc, &server_url)))))
}
