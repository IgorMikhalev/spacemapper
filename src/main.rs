// SPDX-License-Identifier: GPL-3.0-or-later
//! Spacemapper: treemap disk usage viewer.

mod app;
mod dupes;
mod export;
mod history;
mod filter;
mod layout;
mod scan;
mod theme;
mod tree;
mod util;

use eframe::egui;
use std::path::PathBuf;
use std::sync::Arc;

pub const APP_NAME: &str = "Spacemapper";
pub const APP_ID: &str = "spacemapper";

fn main() -> eframe::Result<()> {
    let mut start: Option<PathBuf> = None;
    for a in std::env::args().skip(1) {
        match a.as_str() {
            "-h" | "--help" => {
                println!(
                    "{APP_NAME} {} - treemap disk usage viewer\n\n\
                     Usage:\n  \
                     spacemapper [FOLDER]\n      \
                     Open FOLDER, or the drive list if no folder is given.\n  \
                     spacemapper --json FOLDER [--max-depth N] [--cross]\n      \
                     Scan FOLDER and print the result as JSON.\n      \
                     --max-depth N  list entries only N levels deep (default: all)\n      \
                     --cross        also scan other filesystems mounted inside\n\n\
                     Spacemapper only reads files; it does not modify or delete them.",
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
            "--json" => {
                let args: Vec<String> = std::env::args().skip(2).collect();
                let mut path: Option<PathBuf> = None;
                let mut max_depth = u32::MAX;
                let mut cross = false;
                let mut it = args.iter();
                while let Some(a) = it.next() {
                    match a.as_str() {
                        "--cross" => cross = true,
                        "--max-depth" => match it.next().and_then(|v| v.parse().ok()) {
                            Some(d) => max_depth = d,
                            None => {
                                eprintln!("--max-depth needs a number");
                                std::process::exit(2);
                            }
                        },
                        _ if path.is_none() => path = Some(PathBuf::from(a)),
                        _ => {
                            eprintln!("Unexpected argument: {a}");
                            std::process::exit(2);
                        }
                    }
                }
                let Some(p) = path else {
                    eprintln!("Usage: spacemapper --json FOLDER [--max-depth N] [--cross]");
                    std::process::exit(2);
                };
                let p = std::fs::canonicalize(&p).unwrap_or(p);
                std::process::exit(export::run(&p, max_depth, cross));
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

pub const ICON_RGBA: &[u8] = include_bytes!("../assets/icon/spacemapper-128.rgba");
