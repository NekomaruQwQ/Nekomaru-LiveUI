/// egui application for the LiveServer control panel.
///
/// `ControlApp` implements `eframe::App`. It holds all UI state and a blocking
/// HTTP client. Periodic polling refreshes stream list and auto-selector status
/// every ~2 seconds. All other calls are on-demand (button clicks).
///
/// When the server is unreachable, the UI shows a disconnected state instead
/// of the normal controls. Connectivity is re-checked every poll interval.
use std::time::Instant;

use eframe::egui;

use crate::client::Client;
use crate::model::*;

/// How often to poll the server for stream list and auto-selector status.
const POLL_INTERVAL: std::time::Duration = std::time::Duration::from_secs(2);

/// Capture mode selection in the "New Capture" section.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum CaptureMode {
    Resample,
    Crop,
}

/// All available crop alignment options, mirroring the server's accepted values.
const CROP_ALIGNMENTS: &[&str] = &[
    "center",
    "top-left",
    "top",
    "top-right",
    "left",
    "right",
    "bottom-left",
    "bottom",
    "bottom-right",
];

pub struct ControlApp {
    client: Client,
    server_url: String,

    // ── Connection state ────────────────────────────────────────────────

    /// Whether the last poll succeeded. Controls which UI is shown.
    connected: bool,
    /// Last error message from a failed poll (shown in disconnected UI).
    last_error: Option<String>,

    // ── Cached server state (refreshed by polling) ──────────────────────

    streams: Vec<StreamInfo>,
    auto_status: Option<AutoStatus>,
    windows: Vec<WindowInfo>,

    // ── Polling ─────────────────────────────────────────────────────────

    last_poll: Instant,

    // ── UI state: new capture form ──────────────────────────────────────

    /// Index into `self.windows` for the selected window.
    selected_window: Option<usize>,
    capture_mode: CaptureMode,
    resample_width: String,
    resample_height: String,
    crop_width: String,
    crop_height: String,
    crop_width_full: bool,
    crop_height_full: bool,
    /// Index into `CROP_ALIGNMENTS`.
    crop_align_idx: usize,

    // ── Status bar ──────────────────────────────────────────────────────

    /// Transient feedback message and when it was set (clears after 5s).
    status: Option<(String, Instant)>,
}

impl ControlApp {
    pub fn new(_cc: &eframe::CreationContext<'_>, server_url: &str) -> Self {
        Self {
            client: Client::new(server_url),
            server_url: server_url.to_owned(),
            connected: false,
            last_error: None,
            streams: Vec::new(),
            auto_status: None,
            windows: Vec::new(),
            last_poll: Instant::now() - POLL_INTERVAL, // trigger immediate first poll
            selected_window: None,
            capture_mode: CaptureMode::Resample,
            resample_width: "1920".into(),
            resample_height: "1200".into(),
            crop_width: "1280".into(),
            crop_height: "720".into(),
            crop_width_full: false,
            crop_height_full: false,
            crop_align_idx: 0, // "center"
            status: None,
        }
    }

    /// Set a transient status message shown at the bottom of the panel.
    fn set_status(&mut self, msg: impl Into<String>) {
        self.status = Some((msg.into(), Instant::now()));
    }

    /// Poll the server for stream list and auto-selector status.
    /// Updates `self.connected` based on whether the requests succeed.
    fn poll(&mut self) {
        if self.last_poll.elapsed() < POLL_INTERVAL {
            return;
        }
        self.last_poll = Instant::now();

        // Use list_streams as the connectivity probe — if it fails, we're disconnected.
        match self.client.list_streams() {
            Ok(streams) => {
                self.streams = streams;
                self.connected = true;
                self.last_error = None;
            }
            Err(e) => {
                self.connected = false;
                self.last_error = Some(e);
                return; // skip auto-status fetch if server is down
            }
        }
        match self.client.get_auto_status() {
            Ok(status) => self.auto_status = Some(status),
            Err(e) => self.set_status(e),
        }
    }

    // ── UI: disconnected state ──────────────────────────────────────────

    fn ui_disconnected(&self, ui: &mut egui::Ui) {
        ui.vertical_centered(|ui| {
            ui.add_space(40.0);
            ui.heading("Server Unreachable");
            ui.add_space(12.0);
            ui.label(format!("Cannot connect to {}", self.server_url));
            if let Some(ref err) = self.last_error {
                ui.add_space(8.0);
                ui.colored_label(egui::Color32::from_rgb(255, 120, 120), err);
            }
            ui.add_space(16.0);
            ui.label("Retrying every 2 seconds...");
        });
    }

    // ── UI sections ─────────────────────────────────────────────────────

    fn ui_auto_selector(&mut self, ui: &mut egui::Ui) {
        ui.heading("Auto-Selector");
        ui.add_space(4.0);

        if let Some(ref status) = self.auto_status {
            ui.horizontal(|ui| {
                ui.label("Status:");
                if status.active {
                    ui.colored_label(egui::Color32::LIGHT_GREEN, "Active");
                } else {
                    ui.colored_label(egui::Color32::GRAY, "Inactive");
                }
            });
            if let Some(ref id) = status.current_stream_id {
                ui.label(format!("  Stream: {id}"));
            }
            if let Some(ref hwnd) = status.current_hwnd {
                ui.label(format!("  HWND: {hwnd}"));
            }
        } else {
            ui.label("Status: unknown");
        }

        ui.add_space(4.0);
        ui.horizontal(|ui| {
            if ui.button("Start").clicked() {
                match self.client.start_auto() {
                    Ok(s) => {
                        self.auto_status = Some(s);
                        self.set_status("Auto-selector started");
                    }
                    Err(e) => self.set_status(e),
                }
            }
            if ui.button("Stop").clicked() {
                match self.client.stop_auto() {
                    Ok(()) => {
                        self.auto_status = None;
                        self.set_status("Auto-selector stopped");
                    }
                    Err(e) => self.set_status(e),
                }
            }
        });
    }

    fn ui_active_streams(&mut self, ui: &mut egui::Ui) {
        ui.heading("Active Streams");
        ui.add_space(4.0);

        if self.streams.is_empty() {
            ui.label("No active streams.");
            return;
        }

        // Find the stream to destroy (if any) after iterating,
        // so we don't borrow self.streams while calling self.client.
        let mut destroy_id: Option<String> = None;

        egui::Grid::new("streams_grid")
            .num_columns(4)
            .striped(true)
            .show(ui, |ui| {
                ui.strong("ID");
                ui.strong("HWND");
                ui.strong("Status");
                ui.strong("");
                ui.end_row();

                for stream in &self.streams {
                    ui.label(&stream.id);
                    ui.label(&stream.hwnd);
                    ui.label(&stream.status);
                    if ui.small_button("Destroy").clicked() {
                        destroy_id = Some(stream.id.clone());
                    }
                    ui.end_row();
                }
            });

        if let Some(id) = destroy_id {
            match self.client.destroy_stream(&id) {
                Ok(()) => self.set_status(format!("Destroyed stream {id}")),
                Err(e) => self.set_status(e),
            }
        }
    }

    fn ui_new_capture(&mut self, ui: &mut egui::Ui) {
        ui.heading("New Capture");
        ui.add_space(4.0);

        // ── Window picker ───────────────────────────────────────────────

        if ui.button("Refresh Windows").clicked() {
            match self.client.list_windows() {
                Ok(w) => {
                    self.windows = w;
                    self.selected_window = None;
                    self.set_status("Window list refreshed");
                }
                Err(e) => self.set_status(e),
            }
        }

        if !self.windows.is_empty() {
            ui.add_space(4.0);
            egui::ScrollArea::vertical()
                .max_height(150.0)
                .show(ui, |ui| {
                    for (i, win) in self.windows.iter().enumerate() {
                        let label = format!(
                            "{} (0x{:X})",
                            if win.title.is_empty() { "<untitled>" } else { &win.title },
                            win.hwnd);
                        let selected = self.selected_window == Some(i);
                        if ui.selectable_label(selected, label).clicked() {
                            self.selected_window = Some(i);
                        }
                    }
                });
        }

        ui.add_space(8.0);

        // ── Capture mode ────────────────────────────────────────────────

        ui.horizontal(|ui| {
            ui.label("Mode:");
            ui.radio_value(&mut self.capture_mode, CaptureMode::Resample, "Resample");
            ui.radio_value(&mut self.capture_mode, CaptureMode::Crop, "Crop");
        });

        ui.add_space(4.0);

        match self.capture_mode {
            CaptureMode::Resample => {
                ui.horizontal(|ui| {
                    ui.label("Width:");
                    ui.add(egui::TextEdit::singleline(&mut self.resample_width).desired_width(60.0));
                    ui.label("Height:");
                    ui.add(egui::TextEdit::singleline(&mut self.resample_height).desired_width(60.0));
                });
            }
            CaptureMode::Crop => {
                ui.horizontal(|ui| {
                    ui.label("Crop W:");
                    ui.add_enabled(
                        !self.crop_width_full,
                        egui::TextEdit::singleline(&mut self.crop_width).desired_width(60.0));
                    ui.checkbox(&mut self.crop_width_full, "Full");
                });
                ui.horizontal(|ui| {
                    ui.label("Crop H:");
                    ui.add_enabled(
                        !self.crop_height_full,
                        egui::TextEdit::singleline(&mut self.crop_height).desired_width(60.0));
                    ui.checkbox(&mut self.crop_height_full, "Full");
                });
                ui.horizontal(|ui| {
                    ui.label("Align:");
                    egui::ComboBox::from_id_salt("crop_align")
                        .selected_text(CROP_ALIGNMENTS[self.crop_align_idx])
                        .show_ui(ui, |ui| {
                            for (i, &align) in CROP_ALIGNMENTS.iter().enumerate() {
                                ui.selectable_value(&mut self.crop_align_idx, i, align);
                            }
                        });
                });
            }
        }

        ui.add_space(8.0);

        // ── Create button ───────────────────────────────────────────────

        let can_create = self.selected_window.is_some();
        if ui.add_enabled(can_create, egui::Button::new("Create Capture")).clicked() {
            if let Some(idx) = self.selected_window {
                let hwnd = format!("0x{:X}", self.windows[idx].hwnd);
                let result = match self.capture_mode {
                    CaptureMode::Resample => {
                        let w = self.resample_width.parse::<u32>().unwrap_or(1920);
                        let h = self.resample_height.parse::<u32>().unwrap_or(1200);
                        self.client.create_stream_resample(&hwnd, w, h)
                    }
                    CaptureMode::Crop => {
                        let cw = if self.crop_width_full { "full" } else { &self.crop_width };
                        let ch = if self.crop_height_full { "full" } else { &self.crop_height };
                        let align = CROP_ALIGNMENTS[self.crop_align_idx];
                        self.client.create_stream_crop(&hwnd, cw, ch, align)
                    }
                };
                match result {
                    Ok(id) => self.set_status(format!("Created stream {id}")),
                    Err(e) => self.set_status(e),
                }
            }
        }
    }

    fn ui_status_bar(&mut self, ui: &mut egui::Ui) {
        // Clear stale status messages after 5 seconds.
        if let Some((_, when)) = &self.status {
            if when.elapsed() > std::time::Duration::from_secs(5) {
                self.status = None;
            }
        }

        if let Some((ref msg, _)) = self.status {
            ui.label(msg);
        }
    }
}

impl eframe::App for ControlApp {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        self.poll();

        // Schedule the next repaint so polling continues even without user input.
        ctx.request_repaint_after(POLL_INTERVAL);

        if !self.connected {
            egui::CentralPanel::default().show(ctx, |ui| self.ui_disconnected(ui));
            return;
        }

        egui::TopBottomPanel::bottom("status_bar")
            .show(ctx, |ui| self.ui_status_bar(ui));

        egui::CentralPanel::default().show(ctx, |ui| {
            egui::ScrollArea::vertical().show(ui, |ui| {
                self.ui_auto_selector(ui);
                ui.separator();
                self.ui_active_streams(ui);
                ui.separator();
                self.ui_new_capture(ui);
            });
        });
    }
}
