//! The system tray icon: LabDeck keeps running (and keeps the Stream Deck working) when
//! its window is closed; the tray brings the window back or quits.

use crate::icons::{Canvas, Rgba};
use std::sync::mpsc::Sender;
use tray_icon::menu::{Menu, MenuEvent, MenuItem, PredefinedMenuItem};
use tray_icon::{MouseButton, MouseButtonState, TrayIcon, TrayIconBuilder, TrayIconEvent};

/// The original's tray menu (ESDSystemTrayMenu, function 0x1401dd4d0): "Configure Stream
/// Deck", "Preferences…", "Quit Stream Deck" (asks first, 0x14018bb80), "Elgato Marketplace…"
/// (left out here). A single or double click on the icon configures (0x1401de060).
pub enum TrayMsg {
    Show,
    Preferences,
    Quit,
}

pub struct Tray {
    _icon: TrayIcon,
}

/// A small purple tile with a 3×3 key grid, like the window's title-bar badge.
fn badge(px: u32) -> Vec<u8> {
    let mut cv = Canvas::new(px, px);
    let s = px as f32;
    cv.round_rect(s * 0.06, s * 0.06, s * 0.88, s * 0.88, s * 0.18, Rgba::hex("#6a40ff"));
    for i in 0..3 {
        for j in 0..3 {
            let k = s * 0.17;
            let x = s * 0.2 + i as f32 * s * 0.22;
            let y = s * 0.2 + j as f32 * s * 0.22;
            cv.round_rect(x, y, k, k, s * 0.03, Rgba::hex("#1d1440"));
        }
    }
    cv.to_rgba8()
}

/// Creates the tray icon. On Windows this must run on the window's thread (it is called
/// from App::new); on Linux the KSNI backend runs its own D-Bus thread.
pub fn create(ctx: egui::Context, tx: Sender<TrayMsg>) -> Option<Tray> {
    let show = MenuItem::new("Configure LabDeck", true, None);
    let prefs = MenuItem::new("Preferences…", true, None);
    let quit = MenuItem::new("Quit LabDeck", true, None);
    let menu = Menu::new();
    menu.append(&show).ok()?;
    menu.append(&prefs).ok()?;
    menu.append(&PredefinedMenuItem::separator()).ok()?;
    menu.append(&quit).ok()?;
    let (show_id, prefs_id, quit_id) = (show.id().clone(), prefs.id().clone(), quit.id().clone());

    let (tx_menu, ctx_menu) = (tx.clone(), ctx.clone());
    MenuEvent::set_event_handler(Some(move |e: MenuEvent| {
        let msg = if e.id == show_id {
            Some(TrayMsg::Show)
        } else if e.id == prefs_id {
            Some(TrayMsg::Preferences)
        } else if e.id == quit_id {
            Some(TrayMsg::Quit)
        } else {
            None
        };
        if let Some(m) = msg {
            let _ = tx_menu.send(m);
            ctx_menu.request_repaint();
        }
    }));
    TrayIconEvent::set_event_handler(Some(move |e: TrayIconEvent| {
        let show = matches!(
            e,
            TrayIconEvent::Click { button: MouseButton::Left, button_state: MouseButtonState::Up, .. }
                | TrayIconEvent::DoubleClick { button: MouseButton::Left, .. }
        );
        if show {
            let _ = tx.send(TrayMsg::Show);
            ctx.request_repaint();
        }
    }));

    let px = 32;
    let icon = tray_icon::Icon::from_rgba(badge(px), px, px).ok()?;
    let tray = TrayIconBuilder::new()
        .with_tooltip("LabDeck")
        .with_icon(icon)
        .with_menu(Box::new(menu))
        .with_menu_on_left_click(false)
        .build()
        .ok()?;
    Some(Tray { _icon: tray })
}
