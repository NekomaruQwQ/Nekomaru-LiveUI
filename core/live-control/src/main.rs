mod app;
mod api;
mod data;

fn main() -> eframe::Result {
    use egui::*;
    use eframe::*;

    let port: u16 = std::env::var("LIVE_PORT")
        .ok()
        .and_then(|value| value.parse().ok())
        .expect("LIVE_PORT not set or is not a valid port number");
    let server_url = format!("http://localhost:{port}");

    let options = NativeOptions {
        viewport: ViewportBuilder::default()
            .with_title("Control Panel - Nekomaru LiveUI v2")
            .with_inner_size([960.0, 600.0])
            .with_resizable(false)
            .with_maximize_button(false),
        ..Default::default()
    };

    run_native("live-control", options, Box::new(|cc| {
        setup_fonts(cc);
        cc.egui_ctx.set_visuals(Visuals::dark());
        cc.egui_ctx.set_zoom_factor(1.125);
        Ok(Box::new(app::ControlApp::new(cc, &server_url)))
    }))
}

fn setup_fonts(cc: &eframe::CreationContext<'_>) {
    use std::fs;
    use std::sync::Arc;
    use egui::*;
    use eframe::*;

    // Load Microsoft YaHei UI for CJK character support.
    // msyh.ttc index 1 = Microsoft YaHei UI (UI-optimized variant).
    let font_bytes =
        fs::read("C:/Windows/Fonts/msyh.ttc")
            .expect("Failed to read Microsoft YaHei UI font (msyh.ttc)");
    let mut font_data = FontData::from_owned(font_bytes);
    font_data.index = 1;
    let font_data = Arc::new(font_data);

    let mut fonts = FontDefinitions::default();
    fonts.font_data
        .insert("msyahei_ui".to_owned(), font_data);
    // Primary proportional font — CJK + Latin.
    fonts.families
        .entry(FontFamily::Proportional)
        .or_default()
        .push("msyahei_ui".to_owned());

    cc.egui_ctx.set_fonts(fonts);
}
