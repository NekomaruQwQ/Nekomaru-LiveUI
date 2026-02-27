/// egui application for the LiveServer control panel.
///
/// `ControlApp` implements `eframe::App`. It holds all UI state and a blocking
/// HTTP client. Periodic polling refreshes stream list, window list, auto-selector
/// status/config, and string store every ~2 seconds. All other calls are on-demand
/// (button clicks).
///
/// When the server is unreachable, the UI shows a disconnected state instead
/// of the normal controls. Connectivity is re-checked every poll interval.
use std::collections::HashMap;
use std::time::Instant;

use eframe::egui;

use crate::api::Client;
use crate::data::*;

/// How often to poll the server for stream list and auto-selector status.
const POLL_INTERVAL: std::time::Duration = std::time::Duration::from_secs(2);

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
    /// Window list, cross-referenced with streams to resolve title/executable.
    windows: Vec<WindowInfo>,
    auto_status: Option<AutoStatus>,
    /// String store fetched from server. Not updated while `strings_dirty`.
    strings: HashMap<String, String>,

    // ── Polling ─────────────────────────────────────────────────────────

    last_poll: Instant,

    // ── UI state: auto-selector config editing ──────────────────────────

    /// Local copy of the config, edited in the UI.
    config_include: Vec<String>,
    config_exclude: Vec<String>,
    /// True when the user has made unsaved edits to the config lists.
    config_dirty: bool,
    /// Text input for adding a new include pattern.
    config_include_input: String,
    /// Text input for adding a new exclude pattern.
    config_exclude_input: String,

    // ── UI state: string store editing ───────────────────────────────────

    string_key_input: String,
    string_value_input: String,

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
            windows: Vec::new(),
            auto_status: None,
            strings: HashMap::new(),
            last_poll: Instant::now().checked_sub(POLL_INTERVAL).unwrap(), // trigger immediate first poll
            config_include: Vec::new(),
            config_exclude: Vec::new(),
            config_dirty: false,
            config_include_input: String::new(),
            config_exclude_input: String::new(),
            string_key_input: String::new(),
            string_value_input: String::new(),
            status: None,
        }
    }

    /// Set a transient status message shown at the bottom of the panel.
    fn set_status(&mut self, msg: impl Into<String>) {
        self.status = Some((msg.into(), Instant::now()));
    }

    /// Poll the server for stream list, window list, auto-selector status/config,
    /// and string store. Updates `self.connected` based on whether the primary
    /// request succeeds.
    fn poll(&mut self) {
        if self.last_poll.elapsed() < POLL_INTERVAL {
            return;
        }
        self.last_poll = Instant::now();

        // Use list_streams as the connectivity probe.
        match self.client.list_streams() {
            Ok(streams) => {
                self.streams = streams;
                self.connected = true;
                self.last_error = None;
            }
            Err(e) => {
                self.connected = false;
                self.last_error = Some(e);
                return;
            }
        }

        // Window list — for cross-referencing stream hwnd → title/executable.
        if let Ok(w) = self.client.list_windows() {
            self.windows = w;
        }

        if let Ok(status) = self.client.get_auto_status() {
            self.auto_status = Some(status);
        }

        // Only overwrite config from server when the user has no unsaved edits.
        if !self.config_dirty
            && let Ok(config) = self.client.get_auto_config() {
            self.config_include = config.include_list;
            self.config_exclude = config.exclude_list;
        }

        if let Ok(s) = self.client.get_strings() {
            self.strings = s;
        }
    }

    /// Build a lookup from hex hwnd string → WindowInfo for cross-referencing.
    fn window_lookup(&self) -> HashMap<String, &WindowInfo> {
        self.windows.iter()
            .map(|w| (format!("0x{:X}", w.hwnd), w))
            .collect()
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

    fn ui_streams(&self, ui: &mut egui::Ui) {
        ui.heading("Streams");
        ui.add_space(4.0);

        if self.streams.is_empty() {
            ui.label("No active streams.");
            return;
        }

        let lookup = self.window_lookup();

        egui::Grid::new("streams_grid")
            .num_columns(6)
            .striped(true)
            .show(ui, |ui| {
                ui.strong("ID");
                ui.strong("Gen");
                ui.strong("HWND");
                ui.strong("Executable");
                ui.strong("Status");
                ui.strong("Title");
                ui.end_row();

                for stream in &self.streams {
                    let win = lookup.get(&stream.hwnd);

                    ui.label(&stream.id);
                    ui.label(stream.generation.to_string());
                    ui.label(&stream.hwnd);

                    // Executable — extract filename from full path.
                    let exe = win
                        .map_or("?", |w| w.executable_path.rsplit('\\').next().unwrap_or(&w.executable_path));
                    ui.label(exe);

                    ui.label(&stream.status);

                    // Window title last — clips at window width instead of overflowing.
                    let title = win
                        .map_or("?", |w| if w.title.is_empty() { "<untitled>" } else { &w.title });
                    ui.add(egui::Label::new(title).truncate());
                    ui.end_row();
                }
            });
    }

    #[expect(clippy::too_many_lines, reason = "single UI section with include/exclude list editors; splitting would fragment the layout flow")]
    fn ui_auto_selector(&mut self, ui: &mut egui::Ui) {
        ui.heading("Auto-Selector");
        ui.add_space(4.0);

        // ── Status + Start/Stop ──────────────────────────────────────────

        if let Some(ref status) = self.auto_status {
            ui.horizontal(|ui| {
                ui.label("Status:");
                if status.active {
                    ui.colored_label(egui::Color32::LIGHT_GREEN, "Active");
                } else {
                    ui.colored_label(egui::Color32::GRAY, "Inactive");
                }
            });
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

        ui.add_space(8.0);

        // ── Include list ─────────────────────────────────────────────────

        ui.strong("Include Patterns");
        ui.add_space(2.0);
        ui.label("Substring match on executable path.");
        ui.add_space(2.0);

        let mut include_remove: Option<usize> = None;
        for (i, pattern) in self.config_include.iter().enumerate() {
            ui.horizontal(|ui| {
                let mut text = pattern.clone();
                ui.add(egui::TextEdit::singleline(&mut text)
                    .desired_width(ui.available_width() - 24.0)
                    .interactive(false));
                if ui.small_button("\u{d7}").clicked() {
                    include_remove = Some(i);
                }
            });
        }
        if let Some(i) = include_remove {
            self.config_include.remove(i);
            self.config_dirty = true;
        }

        ui.horizontal(|ui| {
            ui.add(egui::TextEdit::singleline(&mut self.config_include_input)
                .desired_width(ui.available_width() - 40.0)
                .hint_text("path or substring..."));
            if ui.small_button("Add").clicked() && !self.config_include_input.is_empty() {
                self.config_include.push(self.config_include_input.drain(..).collect());
                self.config_dirty = true;
            }
        });

        ui.add_space(8.0);

        // ── Exclude list ─────────────────────────────────────────────────

        ui.strong("Exclude Patterns");
        ui.add_space(2.0);
        ui.label("Case-insensitive substring match.");
        ui.add_space(2.0);

        let mut exclude_remove: Option<usize> = None;
        for (i, pattern) in self.config_exclude.iter().enumerate() {
            ui.horizontal(|ui| {
                let mut text = pattern.clone();
                ui.add(egui::TextEdit::singleline(&mut text)
                    .desired_width(ui.available_width() - 24.0)
                    .interactive(false));
                if ui.small_button("\u{d7}").clicked() {
                    exclude_remove = Some(i);
                }
            });
        }
        if let Some(i) = exclude_remove {
            self.config_exclude.remove(i);
            self.config_dirty = true;
        }

        ui.horizontal(|ui| {
            ui.add(egui::TextEdit::singleline(&mut self.config_exclude_input)
                .desired_width(ui.available_width() - 40.0)
                .hint_text("path or substring..."));
            if ui.small_button("Add").clicked() && !self.config_exclude_input.is_empty() {
                self.config_exclude.push(self.config_exclude_input.drain(..).collect());
                self.config_dirty = true;
            }
        });

        ui.add_space(8.0);

        // ── Save button ──────────────────────────────────────────────────

        ui.horizontal(|ui| {
            if ui.add_enabled(self.config_dirty, egui::Button::new("Save Config")).clicked() {
                let config = SelectorConfig {
                    include_list: self.config_include.clone(),
                    exclude_list: self.config_exclude.clone(),
                };
                match self.client.set_auto_config(&config) {
                    Ok(()) => {
                        self.config_dirty = false;
                        self.set_status("Config saved");
                    }
                    Err(e) => self.set_status(e),
                }
            }
            if self.config_dirty {
                ui.colored_label(egui::Color32::from_rgb(255, 200, 100), "unsaved changes");
            }
        });
    }

    fn ui_string_store(&mut self, ui: &mut egui::Ui) {
        ui.heading("String Store");
        ui.add_space(4.0);

        if self.strings.is_empty() {
            ui.label("No entries.");
        }

        // Collect into sorted vec for stable display order.
        let mut entries: Vec<_> = self.strings.iter().collect();
        entries.sort_by_key(|&(k, _)| k.as_str());

        let mut delete_key: Option<String> = None;
        for &(key, value) in &entries {
            ui.horizontal(|ui| {
                ui.label(format!("{key} ="));
                let mut val = value.clone();
                ui.add(egui::TextEdit::singleline(&mut val)
                    .desired_width(ui.available_width() - 24.0)
                    .interactive(false));
                if ui.small_button("\u{d7}").clicked() {
                    delete_key = Some(key.clone());
                }
            });
        }

        if let Some(ref key) = delete_key {
            match self.client.delete_string(key) {
                Ok(()) => {
                    self.strings.remove(key);
                    self.set_status(format!("Deleted string \"{key}\""));
                }
                Err(e) => self.set_status(e),
            }
        }

        ui.add_space(4.0);
        ui.horizontal(|ui| {
            ui.add(egui::TextEdit::singleline(&mut self.string_key_input)
                .desired_width(100.0)
                .hint_text("key"));
            ui.add(egui::TextEdit::singleline(&mut self.string_value_input)
                .desired_width(ui.available_width() - 40.0)
                .hint_text("value"));
            if ui.small_button("Set").clicked() && !self.string_key_input.is_empty() {
                let key: String = self.string_key_input.drain(..).collect();
                let value: String = self.string_value_input.drain(..).collect();
                match self.client.set_string(&key, &value) {
                    Ok(()) => {
                        self.set_status(format!("Set \"{key}\""));
                        self.strings.insert(key, value);
                    }
                    Err(e) => self.set_status(e),
                }
            }
        });
    }

    fn ui_status_bar(&mut self, ui: &mut egui::Ui) {
        // Clear stale status messages after 5 seconds.
        if self.status.as_ref().is_some_and(|&(_, ref when)| when.elapsed() > std::time::Duration::from_secs(5)) {
            self.status = None;
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
                self.ui_streams(ui);
                ui.separator();
                ui.columns(2, |cols| {
                    assert!(cols.len() >= 2, "ui.columns(2, ...) must yield at least 2 columns");
                    self.ui_string_store(&mut cols[0]);
                    self.ui_auto_selector(&mut cols[1]);
                });
            });
        });
    }
}
