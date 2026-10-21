// SPDX-License-Identifier: GPL-3.0-or-later
//! Spacemap: treemap disk usage viewer.

mod app;
mod filter;
mod layout;
mod scan;
mod theme;
mod tree;
mod util;

use eframe::egui;
use std::path::PathBuf;
use std::sync::Arc;

pub const APP_NAME: &str = "Spacemap";
pub const APP_ID: &str = "spacemap";

fn main() -> eframe::Result<()> {
    let mut start: Option<PathBuf> = None;
    for a in std::env::args().skip(1) {
        match a.as_str() {
            "-h" | "--help" => {
                println!(
                    "{APP_NAME} {} - treemap disk usage viewer\n\n\
                     Usage: spacemap [FOLDER]\n\n\
                     Opens FOLDER, or the drive list if no folder is given.\n\
                     Spacemap only reads files; it does not modify or delete them.",
                    env!("CARGO_PKG_VERSION")
                );
                return Ok(());
            }
            "--scan-helper" => {
                // privileged read-only scanner started by the GUI via pkexec
                let args: Vec<String> = std::env::args().collect();
                let Some(p) = args.get(2) else { std::process::exit(2) };
                let cross = args.iter().any(|a| a == "--cross");
                std::process::exit(scan::run_helper(std::path::Path::new(p), cross));
            }
            "-V" | "--version" => {
                println!("{APP_NAME} {}", env!("CARGO_PKG_VERSION"));
                return Ok(());
            }
            _ => {
                let p = PathBuf::from(&a);
                start = Some(std::fs::canonicalize(&p).unwrap_or(p));
            }
        }
    }

    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_title(APP_NAME)
            .with_app_id(APP_ID)
            .with_inner_size([1280.0, 800.0])
            .with_min_inner_size([640.0, 400.0])
            .with_icon(Arc::new(app_icon()))
            .with_drag_and_drop(true),
        // Don't block on the compositor when presenting a frame. With vsync on,
        // a window on a hidden Wayland workspace never gets its frame callback,
        // so a repaint there would hang the event loop and the compositor would
        // report the app as not responding.
        vsync: false,
        ..Default::default()
    };
    eframe::run_native(
        APP_NAME,
        options,
        Box::new(move |cc| Ok(Box::new(app::App::new(cc, start)))),
    )
}

/// The app icon (128x128 RGBA, pre-decoded so no image library is needed).
pub fn app_icon() -> egui::IconData {
    egui::IconData { rgba: ICON_RGBA.to_vec(), width: 128, height: 128 }
}

pub const ICON_RGBA: &[u8] = include_bytes!("../assets/icon/spacemap-128.rgba");
