// A window program on Windows in every build, so no console window opens next to LabDeck
// (desktop::attach_terminal lets the options that print still print in a terminal).
#![cfg_attr(not(test), windows_subsystem = "windows")]

mod actions;
mod app;
mod catalog;
mod desktop;
mod devices;
mod favicon;
mod hid;
mod icons;
mod inspector;
mod keyimage;
mod keys;
mod plugins;
mod profile;
mod theme;
mod tray;
mod view;

use std::path::PathBuf;

fn main() -> eframe::Result<()> {
    let mut screenshot: Option<PathBuf> = None;
    let mut background = false;
    let mut foreground = false;
    let mut args = std::env::args().skip(1);
    if std::env::args().skip(1).any(|a| matches!(a.as_str(), "--help" | "-h" | "--favicon" | "--icons" | "--screenshot")) {
        desktop::attach_terminal();
    }
    while let Some(a) = args.next() {
        match a.as_str() {
            "--screenshot" => screenshot = args.next().map(PathBuf::from),
            "--background" => background = true,
            "--foreground" => foreground = true,
            // Check: fetch a site's icon, make the key picture, print where it went.
            "--favicon" => {
                let url = args.next().unwrap_or_default();
                match favicon::fetch(&url).and_then(|img| favicon::make_key_picture(&img, &url)) {
                    Ok(p) => println!("{}", p.display()),
                    Err(e) => eprintln!("error: {e}"),
                }
                return Ok(());
            }
            // Check: draw every built-in icon on one sheet, as on a key, save it as PNG.
            "--icons" => {
                let out = args.next().map(PathBuf::from).unwrap_or_else(|| PathBuf::from("icons.png"));
                let (cell, cols) = (72u32, 8u32);
                let rows = (icons::ALL.len() as u32).div_ceil(cols);
                let mut cv = icons::Canvas::new(cols * cell, rows * cell);
                cv.fill(icons::Rgba::hex("#1f1f1f"));
                for (i, ic) in icons::ALL.iter().enumerate() {
                    let (x, y) = ((i as u32 % cols) * cell, (i as u32 / cols) * cell);
                    icons::draw_icon(&mut cv, *ic, x as f32 + 14.0, y as f32 + 14.0, 44.0, icons::Rgba::WHITE, 0.085);
                }
                match image::save_buffer(&out, &cv.to_rgba8(), cv.w, cv.h, image::ColorType::Rgba8) {
                    Ok(()) => println!("{}", out.display()),
                    Err(e) => eprintln!("error: {e}"),
                }
                return Ok(());
            }
            "--help" | "-h" => {
                println!(
                    "LabDeck: configure and run your Stream Deck on Linux, Windows and macOS.\n\n  --background             start in the tray, without the window (for autostart)\n  --foreground             Linux: stay in this terminal instead of handing over to a copy of its own\n  --screenshot <file.png>  draw ~120 frames, save the window as PNG, exit\n  --favicon <address>      fetch a site's icon, make a key picture, print where it went\n  --icons <file.png>       draw every built-in icon on one sheet, save it, exit\n\nSee README.md for building and usage."
                );
                return Ok(());
            }
            _ => {}
        }
    }
    // Linux: started from a terminal, a copy of LabDeck of its own takes over, so the terminal
    // can be closed (checks and --foreground stay here).
    if screenshot.is_none() && !foreground && desktop::hand_over_terminal() {
        return Ok(());
    }
    // Linux: LabDeck's entry in the app menu (written or brought up to date), so from now on it
    // starts from the menu like any other app.
    if screenshot.is_none() {
        desktop::add_to_app_menu();
    }
    let start_hidden = background && screenshot.is_none();
    let checking = screenshot.is_some();
    let mut viewport = egui::ViewportBuilder::default()
        .with_title("LabDeck")
        .with_app_id(desktop::APP_ID)
        .with_icon(desktop::window_icon())
        .with_inner_size([theme::WIN_W, theme::WIN_H])
        .with_min_inner_size([1100.0, 860.0])
        .with_decorations(false)
        .with_resizable(true)
        .with_visible(!start_hidden);
    if checking {
        // A check's picture: open without taking focus, and on Windows behind other windows
        // (a covered window still draws there), so it never gets in the person's way.
        viewport = viewport.with_active(false);
        #[cfg(target_os = "windows")]
        {
            viewport = viewport.with_window_level(egui::WindowLevel::AlwaysOnBottom);
        }
    }
    #[allow(unused_mut)]
    let mut options = eframe::NativeOptions {
        viewport,
        // Keep running (in the tray) when the window is hidden.
        run_and_return: false,
        ..Default::default()
    };
    #[cfg(target_os = "macos")]
    if checking {
        options.event_loop_builder = Some(Box::new(|b| {
            use winit::platform::macos::EventLoopBuilderExtMacOS;
            b.with_activate_ignoring_other_apps(false);
        }));
    }
    eframe::run_native("LabDeck", options, Box::new(move |cc| Ok(Box::new(app::App::new(cc, screenshot, start_hidden)))))
}
