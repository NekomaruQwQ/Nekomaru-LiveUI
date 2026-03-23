//! `enumerate-windows.exe` — list capturable windows as JSON.
//!
//! Lightweight utility for Nushell scripts to discover window handles.
//!
//! ## Usage
//!
//! ```text
//! # List all capturable windows
//! enumerate-windows
//!
//! # Get the current foreground window
//! enumerate-windows --foreground
//! ```

use clap::Parser;

/// List capturable windows as JSON.
#[derive(Parser)]
#[command(name = "enumerate-windows")]
struct CliArgs {
    /// Print only the current foreground window instead of all windows.
    #[arg(long)]
    foreground: bool,
}

fn main() {
    let _ = set_dpi_awareness::per_monitor_v2();
    let args = CliArgs::parse();

    if args.foreground {
        let window = enumerate_windows::get_foreground_window();
        println!("{}", serde_json::to_string(&window).expect("JSON serialization failed"));
    } else {
        let windows = enumerate_windows::enumerate_windows();
        println!("{}", serde_json::to_string(&windows).expect("JSON serialization failed"));
    }
}
