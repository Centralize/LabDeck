//! Draws the main window like the captured one: title bar, device view (pickers,
//! toolbar, key grid, pages), splitter, key editor area and the action list.

use crate::app::{App, Drag};
use crate::catalog;
use crate::devices;
use crate::inspector;
use crate::profile::{self, Profile};
use crate::theme::*;
use egui::epaint::RectShape;
use egui::{Align2, Color32, CornerRadius, FontFamily, FontId, Pos2, Rect, Sense, Stroke, StrokeKind, pos2, vec2};
use std::collections::HashSet;
use std::path::PathBuf;
use std::sync::Arc;

pub fn frame(app: &mut App, ui: &mut egui::Ui) {
    let ctx = ui.ctx().clone();
    let now = ctx.input(|i| i.time);
    let full = ctx.content_rect();
    ui.painter().rect_filled(full, 0.0, CANVAS);
    let rx = full.right() - RIGHT_W;
    title_bar(ui, Rect::from_min_max(full.min, pos2(full.right(), full.top() + TITLE_H)));
    let body = Rect::from_min_max(pos2(full.left(), full.top() + TITLE_H), pos2(rx, full.bottom()));
    let split_y = body.top() + (body.height() * app.split).round();
    let view = Rect::from_min_max(body.min, pos2(rx, split_y));
    let pi = Rect::from_min_max(pos2(body.left(), split_y + 2.0), body.max);
    device_view(app, ui, view, now);
    splitter(app, ui, body, split_y);
    inspector::show(app, ui, pi, now);
    right_panel(app, ui, Rect::from_min_max(pos2(rx, full.top() + TITLE_H), full.max), now);
    drag_overlay(app, ui);
    menus(app, ui, now);
    status_line(app, ui, body, now);
    quit_dialog(app, ui);
    resize_edges(ui, full);
    app.handle_screenshot(&ctx);
}

/// "Do you want to quit?" — the same question, text and choices as the original's
/// askQuitApplication (function 0x14018bb80).
fn quit_dialog(app: &mut App, ui: &mut egui::Ui) {
    if !app.confirm_quit {
        return;
    }
    let ctx = ui.ctx().clone();
    let mut choice: Option<bool> = None;
    egui::Window::new("Quit LabDeck")
        .title_bar(false)
        .collapsible(false)
        .resizable(false)
        .anchor(Align2::CENTER_CENTER, [0.0, 0.0])
        .frame(menu_frame().inner_margin(18.0))
        .show(&ctx, |ui| {
            ui.set_width(380.0);
            ui.label(egui::RichText::new("Do you want to quit LabDeck?").font(bold(15.0)).color(TEXT));
            ui.add_space(4.0);
            ui.label(egui::RichText::new("Pressing keys won't have any effect until you launch this app again.").color(TEXT_DIM));
            ui.add_space(10.0);
            ui.checkbox(&mut app.quit_dont_ask_tick, "Don't ask me again");
            ui.add_space(10.0);
            ui.horizontal(|ui| {
                if ui.button("Quit").clicked() {
                    choice = Some(true);
                }
                if ui.button("Cancel").clicked() {
                    choice = Some(false);
                }
            });
        });
    match choice {
        Some(true) => {
            app.confirm_quit = false;
            if app.quit_dont_ask_tick {
                app.settings.dont_ask_quit = true;
                app.save_settings();
            }
            app.quit(&ctx);
        }
        Some(false) => app.confirm_quit = false,
        None => {}
    }
}

/// The window draws its own frame (like the original), so it also provides its own
/// resize handles along the edges and corners.
fn resize_edges(ui: &mut egui::Ui, full: Rect) {
    use egui::{CursorIcon as C, ResizeDirection as D};
    if ui.ctx().input(|i| i.viewport().maximized.unwrap_or(false) || i.viewport().fullscreen.unwrap_or(false)) {
        return;
    }
    const EDGE: f32 = 5.0;
    const CORNER: f32 = 12.0;
    let (l, r, t, b) = (full.left(), full.right(), full.top(), full.bottom());
    let zones = [
        (Rect::from_min_max(pos2(l, t), pos2(l + CORNER, t + CORNER)), D::NorthWest, C::ResizeNorthWest),
        (Rect::from_min_max(pos2(r - CORNER, t), pos2(r, t + CORNER)), D::NorthEast, C::ResizeNorthEast),
        (Rect::from_min_max(pos2(l, b - CORNER), pos2(l + CORNER, b)), D::SouthWest, C::ResizeSouthWest),
        (Rect::from_min_max(pos2(r - CORNER, b - CORNER), pos2(r, b)), D::SouthEast, C::ResizeSouthEast),
        (Rect::from_min_max(pos2(l + CORNER, t), pos2(r - CORNER, t + EDGE)), D::North, C::ResizeNorth),
        (Rect::from_min_max(pos2(l + CORNER, b - EDGE), pos2(r - CORNER, b)), D::South, C::ResizeSouth),
        (Rect::from_min_max(pos2(l, t + CORNER), pos2(l + EDGE, b - CORNER)), D::West, C::ResizeWest),
        (Rect::from_min_max(pos2(r - EDGE, t + CORNER), pos2(r, b - CORNER)), D::East, C::ResizeEast),
    ];
    for (i, (rect, dir, cursor)) in zones.into_iter().enumerate() {
        let resp = ui.interact(rect, ui.scope_id().with(("resize", i)), Sense::drag());
        if resp.hovered() || resp.dragged() {
            ui.ctx().set_cursor_icon(cursor);
        }
        if resp.drag_started() {
            ui.ctx().send_viewport_cmd(egui::ViewportCommand::BeginResize(dir));
        }
    }
}

fn title_bar(ui: &mut egui::Ui, r: Rect) {
    let p = ui.painter().clone();
    p.rect_filled(r, 0.0, TITLE_BG);
    // App badge: a purple tile with a key grid (drawn, not the original logo).
    let badge = Rect::from_min_size(pos2(r.left() + 8.0, r.top() + 7.0), vec2(16.0, 16.0));
    p.rect_filled(badge, 3.0, hex(0x6a40ff));
    for i in 0..3 {
        for j in 0..3 {
            let c = pos2(badge.left() + 4.0 + i as f32 * 4.0, badge.top() + 4.0 + j as f32 * 4.0);
            p.rect_filled(Rect::from_center_size(c, vec2(2.4, 2.4)), 0.5, hex(0x1d1440));
        }
    }
    p.text(pos2(r.left() + 31.0, r.center().y), Align2::LEFT_CENTER, "LabDeck", FontId::proportional(12.0), hex(0xbdbdbd));
    let drag = ui.interact(Rect::from_min_max(r.min, pos2(r.right() - 140.0, r.bottom())), ui.scope_id().with("titlebar"), Sense::click_and_drag());
    if drag.drag_started() {
        ui.ctx().send_viewport_cmd(egui::ViewportCommand::StartDrag);
    }
    let maximized = ui.ctx().input(|i| i.viewport().maximized.unwrap_or(false));
    if drag.double_clicked() {
        ui.ctx().send_viewport_cmd(egui::ViewportCommand::Maximized(!maximized));
    }
    let w = 46.0;
    for (name, x) in [("min", r.right() - 3.0 * w), ("max", r.right() - 2.0 * w), ("close", r.right() - w)] {
        let br = Rect::from_min_size(pos2(x, r.top()), vec2(w, r.height()));
        let resp = ui.interact(br, ui.scope_id().with(name), Sense::click());
        let names = crate::theme::caption_names();
        let label = match name { "min" => names.minimize, "max" if maximized => names.restore, "max" => names.maximize, _ => names.close };
        resp.widget_info(|| egui::WidgetInfo::labeled(egui::WidgetType::Button, true, label));
        if resp.hovered() {
            p.rect_filled(br, 0.0, if name == "close" { CLOSE_HOVER } else { hex(0x3a3a3a) });
        }
        let c = br.center();
        let s = Stroke::new(1.0, hex(0xc8c8c8));
        match name {
            "min" => {
                p.line_segment([pos2(c.x - 5.0, c.y), pos2(c.x + 5.0, c.y)], s);
            }
            "max" => {
                p.rect_stroke(Rect::from_center_size(c, vec2(10.0, 10.0)), 0.0, s, StrokeKind::Inside);
            }
            _ => {
                p.line_segment([pos2(c.x - 5.0, c.y - 5.0), pos2(c.x + 5.0, c.y + 5.0)], s);
                p.line_segment([pos2(c.x + 5.0, c.y - 5.0), pos2(c.x - 5.0, c.y + 5.0)], s);
            }
        }
        if resp.clicked() {
            let cmd = match name {
                "min" => egui::ViewportCommand::Minimized(true),
                "max" => egui::ViewportCommand::Maximized(!maximized),
                _ => egui::ViewportCommand::Close,
            };
            ui.ctx().send_viewport_cmd(cmd);
        }
    }
}

fn splitter(app: &mut App, ui: &mut egui::Ui, body: Rect, y: f32) {
    let p = ui.painter();
    p.line_segment([pos2(body.left(), y + 0.5), pos2(body.right(), y + 0.5)], Stroke::new(1.0, SPLIT_A));
    p.line_segment([pos2(body.left(), y + 1.5), pos2(body.right(), y + 1.5)], Stroke::new(1.0, SPLIT_B));
    let handle = Rect::from_min_max(pos2(body.left(), y - 4.0), pos2(body.right(), y + 6.0));
    let r = ui.interact(handle, ui.scope_id().with("split"), Sense::drag());
    if r.hovered() || r.dragged() {
        ui.ctx().set_cursor_icon(egui::CursorIcon::ResizeVertical);
    }
    if r.dragged() {
        let ny = (y + r.drag_delta().y - body.top()) / body.height();
        app.split = ny.clamp(0.35, 0.85);
    }
}

fn top_bar(view_left: f32, view_right: f32, top: f32) -> Rect {
    let cx = (view_left + view_right) / 2.0;
    let w = (view_right - view_left - 40.0).min(1190.0);
    Rect::from_min_size(pos2(cx - w / 2.0, top + 34.0), vec2(w, 50.0))
}

fn device_view(app: &mut App, ui: &mut egui::Ui, view: Rect, now: f64) {
    let ctx = ui.ctx().clone();
    let p = ui.painter().clone();
    let cx = view.center().x;
    let bar = top_bar(view.left(), view.right(), view.top());

    // Device and profile pickers.
    let dev_name = app.model().name.to_string();
    let g = p.text(pos2(bar.left() + 3.0, bar.top() + 12.0), Align2::LEFT_CENTER, &dev_name, bold(17.0), TEXT);
    chevron(&p, pos2(g.right() + 8.0, g.center().y + 1.0), 4.0, TEXT_SOFT);
    let dev_hit = ui.interact(g.expand2(vec2(4.0, 3.0)).union(Rect::from_min_size(g.right_top(), vec2(16.0, g.height()))), ui.scope_id().with("devpick"), Sense::click());
    dev_hit.widget_info(|| egui::WidgetInfo::labeled(egui::WidgetType::Button, true, &dev_name));
    if dev_hit.clicked() {
        app.device_menu = !app.device_menu;
        app.profile_menu = false;
        app.menu_opened = true;
    }
    let mut prof_name = app.profile().name.clone();
    if !app.folder.is_empty() {
        prof_name.push_str("  ›  Folder");
    }
    let g2 = p.text(pos2(bar.left() + 3.0, bar.top() + 37.0), Align2::LEFT_CENTER, &prof_name, FontId::proportional(13.5), TEXT_SOFT);
    chevron(&p, pos2(g2.right() + 7.0, g2.center().y + 1.0), 3.5, TEXT_SOFT);
    let prof_hit = ui.interact(g2.expand2(vec2(4.0, 3.0)).union(Rect::from_min_size(g2.right_top(), vec2(14.0, g2.height()))), ui.scope_id().with("profpick"), Sense::click());
    // The button is named after the profile alone, also while a folder is open.
    let prof_label = app.profile().name.clone();
    prof_hit.widget_info(|| egui::WidgetInfo::labeled(egui::WidgetType::Button, true, &prof_label));
    if prof_hit.clicked() {
        app.profile_menu = !app.profile_menu;
        app.device_menu = false;
        app.menu_opened = true;
    }

    // Toolbar: notifications, store, preferences, account.
    let ty = bar.top() + 11.0;
    let xs = [bar.right() - 113.0, bar.right() - 81.0, bar.right() - 49.0, bar.right() - 17.0];
    bell(&p, pos2(xs[0], ty), TEXT_SOFT);
    for (i, c) in [hex(0xf05a5a), hex(0x3ec46d), hex(0x4f8dff), hex(0xf5c542)].iter().enumerate() {
        let a = i as f32 * std::f32::consts::FRAC_PI_2 + 0.785;
        p.circle_filled(pos2(xs[1] + 4.5 * a.cos(), ty + 4.5 * a.sin()), 3.6, *c);
    }
    gear(&p, pos2(xs[2], ty), TEXT_SOFT);
    p.circle_filled(pos2(xs[3], ty), 11.0, AVATAR);
    p.text(pos2(xs[3], ty), Align2::CENTER_CENTER, &app.user_initial, bold(14.0), TEXT);
    for (i, x) in xs.iter().enumerate() {
        let hit = ui.interact(Rect::from_center_size(pos2(*x, ty), vec2(24.0, 24.0)), ui.scope_id().with(("tool", i)), Sense::click());
        hit.widget_info(|| egui::WidgetInfo::labeled(egui::WidgetType::Button, true, ["Notifications", "Marketplace", "Preferences", "Account"][i]));
        if hit.clicked() {
            match i {
                0 => app.say("No new notifications.", now),
                1 => app.say("The plugin store is not part of this version.", now),
                2 => {
                    app.device_menu = true;
                    app.profile_menu = false;
                }
                _ => app.say("Accounts are not part of this version.", now),
            }
        }
        hit.on_hover_text(["Notifications", "Marketplace", "Preferences", "Account"][i]);
    }

    // Key grid.
    let m = app.model();
    let grid_w = KEY_PITCH * (m.cols as f32 - 1.0) + KEY;
    let grid_h = KEY_PITCH * (m.rows as f32 - 1.0) + KEY;
    let gx = (cx - grid_w / 2.0).round();
    let gy = (view.top() + (view.height() - grid_h) / 2.0 + 8.0).round();
    let page = app.profile().page(&app.page_id()).cloned().unwrap_or_default();
    let pointer = ctx.input(|i| i.pointer.interact_pos());
    let released = ctx.input(|i| i.pointer.any_released());
    for r in 0..m.rows {
        for c in 0..m.cols {
            let kr = Rect::from_min_size(pos2(gx + c as f32 * KEY_PITCH, gy + r as f32 * KEY_PITCH), vec2(KEY, KEY));
            let resp = ui.interact(kr, ui.scope_id().with(("key", c, r)), Sense::click_and_drag());
            let key = page.keys.get(&(c, r));
            let key_label = key.map(|k| k.cur().title.filter(|t| !t.is_empty()).unwrap_or_else(|| k.name.clone())).unwrap_or_default();
            resp.widget_info(|| egui::WidgetInfo::selected(egui::WidgetType::Button, true, app.selected == Some((c, r)), &key_label));
            let selected = app.selected == Some((c, r));
            let hovered_drop = app.drag.is_some() && pointer.is_some_and(|pt| kr.contains(pt));
            let ring = if selected {
                KEY_SELECTED
            } else if hovered_drop || resp.hovered() {
                hex(0x6a6a6a)
            } else {
                KEY_RING
            };
            p.rect_filled(kr, KEY_RADIUS + 1.0, ring);
            let inner = kr.shrink(KEY_BORDER);
            match key {
                Some(k) => {
                    let tex = app.key_texture(&ctx, k, 144);
                    let tint = if app.pressed.contains(&(c, r)) { hex(0xb0b0b0) } else { Color32::WHITE };
                    let shape = RectShape::filled(inner, CornerRadius::same((KEY_RADIUS - 2.0) as u8), tint).with_texture(tex, uv01());
                    p.add(shape);
                }
                None => {
                    p.rect_filled(inner, KEY_RADIUS - 2.0, KEY_INNER);
                    p.rect_filled(inner.shrink(1.0), KEY_RADIUS - 3.0, KEY_EMPTY);
                }
            }
            if resp.clicked() {
                app.selected = Some((c, r));
                app.device_menu = false;
                app.profile_menu = false;
            }
            if resp.double_clicked() {
                if let Some(k) = key.filter(|k| k.uuid == "com.elgato.streamdeck.profile.openchild") {
                    app.open_folder(k.setting_str("ProfileUUID"), now);
                }
            }
            if resp.drag_started() && key.is_some() {
                app.drag = Some(Drag::Key((c, r)));
            }
            let resp = match key {
                Some(k) => resp.on_hover_text(k.name.clone()),
                None => resp,
            };
            // Right-click menu, built like the original's CanvasContextMenu (function 0x140862d20):
            // Copy, Paste, Create Folder / Move to Folder, Create Multi Action / Move to Multi
            // Action, Delete, Trigger Action — each only when it applies (the original's flag
            // bits 1, 3-6). On an empty key, Create Folder comes first as the default.
            resp.context_menu(|ui| {
                let paste = |app: &mut App| {
                    app.paste_key((c, r), None, now);
                };
                if key.is_none() {
                    let default = egui::Button::new(egui::RichText::new("Create Folder").strong());
                    if ui.add(default).clicked() {
                        app.place("com.elgato.streamdeck.profile.openchild", (c, r), now);
                        ui.close();
                    }
                    if app.clipboard.is_some() && ui.add(egui::Button::new("Paste").shortcut_text("Ctrl+V")).clicked() {
                        paste(app);
                        ui.close();
                    }
                    if ui.button("Create Multi Action").clicked() {
                        app.place("com.elgato.streamdeck.multiactions.routine", (c, r), now);
                        ui.close();
                    }
                } else {
                    if ui.add(egui::Button::new("Copy").shortcut_text("Ctrl+C")).clicked() {
                        if let Some(text) = app.copy_key((c, r), now) {
                            ui.ctx().copy_text(text);
                        }
                        ui.close();
                    }
                    if app.clipboard.is_some() && ui.add(egui::Button::new("Paste").shortcut_text("Ctrl+V")).clicked() {
                        paste(app);
                        ui.close();
                    }
                    let uuid = key.map(|k| k.uuid.as_str()).unwrap_or_default();
                    if uuid != "com.elgato.streamdeck.profile.backtoparent" && ui.button("Move to Folder").clicked() {
                        app.move_to_folder((c, r), now);
                        ui.close();
                    }
                    if app.allowed_in_multi(uuid) && ui.button("Move to Multi Action").clicked() {
                        app.move_to_multi_action((c, r), now);
                        ui.close();
                    }
                    ui.separator();
                    if ui.button("Delete").clicked() {
                        app.remove_key((c, r), now);
                        ui.close();
                    }
                    if ui.button("Trigger Action").clicked() {
                        app.press((c, r), now);
                        app.release((c, r), now);
                        ui.close();
                    }
                }
            });
            if released && hovered_drop {
                match app.drag.take() {
                    Some(Drag::Action(u)) => app.place(&u, (c, r), now),
                    Some(Drag::Key(from)) => app.move_key(from, (c, r), now),
                    None => {}
                }
            }
        }
    }
    // Delete removes the selected key, like the original.
    if ctx.input(|i| i.key_pressed(egui::Key::Delete)) && !ctx.egui_wants_keyboard_input() {
        if let Some(s) = app.selected {
            app.remove_key(s, now);
        }
    }
    // Ctrl+C / Ctrl+V copy the selected key and paste onto it (unless a text field is being
    // typed in, where they copy and paste text as usual). The window turns these shortcuts into
    // copy and paste events.
    if !ctx.egui_wants_keyboard_input() {
        if let Some(s) = app.selected {
            let (copy, pasted) = ctx.input(|i| {
                let copy = i.events.iter().any(|e| matches!(e, egui::Event::Copy));
                let pasted = i.events.iter().find_map(|e| match e {
                    egui::Event::Paste(t) => Some(t.clone()),
                    _ => None,
                });
                (copy, pasted)
            });
            if copy && app.key_at(s).is_some() {
                if let Some(text) = app.copy_key(s, now) {
                    ctx.copy_text(text);
                }
            }
            if let Some(text) = pasted {
                app.paste_key(s, Some(&text), now);
            }
        }
    }
    // A picture file dragged onto a key becomes its icon. While files are dragged in, the
    // window gets no pointer moves on some systems, so the real cursor position is asked for.
    let key_under = |pt: Pos2| -> Option<(usize, usize)> {
        let c = ((pt.x - gx) / KEY_PITCH).floor();
        let r = ((pt.y - gy) / KEY_PITCH).floor();
        let inside = (pt.x - gx) - c * KEY_PITCH <= KEY && (pt.y - gy) - r * KEY_PITCH <= KEY;
        (c >= 0.0 && r >= 0.0 && (c as usize) < m.cols && (r as usize) < m.rows && inside).then_some((c as usize, r as usize))
    };
    let (hovering, dropped) = ctx.input(|i| (!i.raw.hovered_files.is_empty(), i.raw.dropped_files.clone()));
    if hovering || !dropped.is_empty() {
        let target = file_drag_pos(&ctx).and_then(key_under);
        if hovering {
            if let Some((c, r)) = target {
                let kr = Rect::from_min_size(pos2(gx + c as f32 * KEY_PITCH, gy + r as f32 * KEY_PITCH), vec2(KEY, KEY));
                p.rect_stroke(kr.expand(2.0), KEY_RADIUS + 3.0, Stroke::new(2.0, ACCENT), StrokeKind::Outside);
            }
            ctx.request_repaint();
        }
        if let Some(f) = dropped.first() {
            let path = f.path().to_path_buf();
            let is_picture = path
                .extension()
                .and_then(|e| e.to_str())
                .is_some_and(|e| PICTURE_TYPES.contains(&e.to_ascii_lowercase().as_str()));
            // Fall back to the selected key when the position isn't known (e.g. on Wayland).
            let target = target.or(app.selected);
            match (is_picture, target) {
                (false, _) => app.say("Only pictures (PNG, JPG, GIF, BMP, WebP, ICO) can be dropped on a key.", now),
                (true, None) => app.say("Drop the picture right on a key.", now),
                (true, Some(pos)) => match app.key_at_mut(pos) {
                    Some(k) => {
                        let st = k.cur_mut();
                        st.image = Some(path.to_string_lossy().to_string());
                        st.icon = Some(String::new());
                        app.selected = Some(pos);
                        app.changed(now);
                        app.say("Icon set", now);
                    }
                    None => app.say("Put an action on this key first, then drop the picture on it.", now),
                },
            }
        }
    }

    // Pages bar: pin, page numbers, plus.
    let pages_n = app.profile().pages.len();
    let seg = 34.0;
    let pill_w = seg * (pages_n as f32 + 1.0);
    let total = 32.0 + 12.0 + pill_w;
    let py = gy + grid_h + 35.0;
    let left = (cx - total / 2.0).round();
    let pin = Rect::from_min_size(pos2(left + 2.0, py), vec2(32.0, 16.0));
    p.rect_filled(pin, 8.0, PILL);
    pin_icon(&p, pin.center(), hex(0xc9c9c9));
    ui.interact(pin, ui.scope_id().with("pin"), Sense::click()).on_hover_text("Pinned actions");
    let pill = Rect::from_min_size(pos2(left + 44.0, py), vec2(pill_w, 16.0));
    p.rect_filled(pill, 8.0, PILL);
    let in_folder = !app.folder.is_empty();
    let current = app.profile().current;
    let mut goto = None;
    for i in 0..=pages_n {
        let sr = Rect::from_min_size(pos2(pill.left() + i as f32 * seg, py), vec2(seg, 16.0));
        let resp = ui.interact(sr, ui.scope_id().with(("page", i)), Sense::click());
        if i < pages_n {
            let on = i == current && !in_folder;
            resp.widget_info(|| egui::WidgetInfo::selected(egui::WidgetType::CheckBox, true, on, format!("{}", i + 1)));
        } else {
            resp.widget_info(|| egui::WidgetInfo::labeled(egui::WidgetType::Button, true, "Add page"));
        }
        if i == pages_n {
            let c = sr.center();
            let s = Stroke::new(1.5, hex(0x979797));
            p.line_segment([pos2(c.x - 4.5, c.y), pos2(c.x + 4.5, c.y)], s);
            p.line_segment([pos2(c.x, c.y - 4.5), pos2(c.x, c.y + 4.5)], s);
            if resp.clicked() {
                goto = Some(app.profiles[app.cur].add_page());
                app.changed(now);
            }
            resp.on_hover_text("Add page");
            continue;
        }
        let on = i == current && !in_folder;
        if on {
            p.rect_filled(sr.shrink2(vec2(1.0, 0.0)), 8.0, PILL_ON);
        } else if resp.hovered() {
            p.rect_filled(sr.shrink2(vec2(1.0, 0.0)), 8.0, hex(0x454545));
        }
        p.text(sr.center(), Align2::CENTER_CENTER, format!("{}", i + 1), FontId::proportional(11.5), if on { hex(0x2d2d2d) } else { hex(0xd0d0d0) });
        if resp.clicked() {
            goto = Some(i);
        }
        resp.context_menu(|ui| {
            if pages_n > 1 && ui.button("Delete page").clicked() {
                let pr = &mut app.profiles[app.cur];
                let id = pr.pages.remove(i);
                pr.all.remove(&id);
                pr.current = pr.current.min(pr.pages.len() - 1);
                app.dirty = true;
                ui.close();
            }
        });
    }
    if let Some(i) = goto {
        app.goto_page(i);
    }

    if in_folder {
        let br = Rect::from_min_size(pos2(gx, gy - 30.0), vec2(130.0, 20.0));
        let resp = ui.interact(br, ui.scope_id().with("folderback"), Sense::click());
        p.text(br.left_center(), Align2::LEFT_CENTER, "‹  Back to parent", FontId::proportional(12.5), if resp.hovered() { TEXT } else { TEXT_DIM });
        if resp.clicked() {
            app.folder.pop();
            app.selected = None;
        }
    }
}

/// Picture files a key accepts as its icon.
const PICTURE_TYPES: &[&str] = &["png", "jpg", "jpeg", "gif", "bmp", "webp", "ico"];

/// Where the pointer is during a file drag, in window points: the system cursor position
/// mapped into the window (egui gets no pointer moves while files are dragged on Windows),
/// else egui's own pointer position.
fn file_drag_pos(ctx: &egui::Context) -> Option<Pos2> {
    let (inner, ppp, egui_pos) = ctx.input(|i| (i.viewport().inner_rect, i.pixels_per_point, i.pointer.latest_pos()));
    if let (mouse_position::mouse_position::Mouse::Position { x, y }, Some(inner)) =
        (mouse_position::mouse_position::Mouse::get_mouse_position(), inner)
    {
        let pt = pos2(x as f32 / ppp, y as f32 / ppp) - inner.min.to_vec2();
        if ctx.content_rect().contains(pt) {
            return Some(pt);
        }
    }
    egui_pos
}

fn right_panel(app: &mut App, ui: &mut egui::Ui, r: Rect, now: f64) {
    let ctx = ui.ctx().clone();
    let p = ui.painter().clone();
    p.line_segment([pos2(r.left() + 0.5, r.top()), pos2(r.left() + 0.5, r.bottom())], Stroke::new(1.0, DIVIDER_A));
    p.line_segment([pos2(r.left() + 1.5, r.top()), pos2(r.left() + 1.5, r.bottom())], Stroke::new(1.0, DIVIDER_B));
    p.rect_filled(Rect::from_min_max(pos2(r.left() + 2.0, r.top()), r.max), 0.0, PANEL);
    let x0 = r.left();

    // Search field.
    let sr = Rect::from_min_size(pos2(x0 + 17.0, r.top() + 12.0), vec2(231.0, 24.0));
    p.rect_filled(sr, 3.0, FIELD);
    let mc = pos2(sr.left() + 15.0, sr.center().y - 1.0);
    p.circle_stroke(mc, 4.0, Stroke::new(1.3, hex(0x9a9a9a)));
    p.line_segment([mc + vec2(3.0, 3.0), mc + vec2(6.0, 6.0)], Stroke::new(1.3, hex(0x9a9a9a)));
    let edit_r = Rect::from_min_max(pos2(sr.left() + 27.0, sr.top() + 2.0), pos2(sr.right() - 6.0, sr.bottom() - 2.0));
    let te = egui::TextEdit::singleline(&mut app.search)
        .frame(egui::Frame::NONE)
        .hint_text(egui::RichText::new("Search").color(hex(0x8c8c8c)))
        .font(FontId::proportional(12.5))
        .text_color(TEXT_SOFT)
        .vertical_align(egui::Align::Center);
    ui.put(edit_r, te);
    // Expand/collapse all.
    let lt = Rect::from_min_size(pos2(x0 + 260.0, r.top() + 18.0), vec2(16.0, 13.0));
    for i in 0..3 {
        let y = lt.top() + 1.5 + i as f32 * 5.0;
        p.circle_filled(pos2(lt.left() + 1.5, y), 1.3, hex(0xa0a0a0));
        p.line_segment([pos2(lt.left() + 5.0, y), pos2(lt.right(), y)], Stroke::new(1.3, hex(0xa0a0a0)));
    }
    if ui.interact(lt.expand(4.0), ui.scope_id().with("listtoggle"), Sense::click()).on_hover_text("Expand or collapse all").clicked() {
        if app.cats.iter().all(|c| app.expanded.contains(&c.name)) {
            app.expanded.clear();
        } else {
            app.expanded = app.cats.iter().map(|c| c.name.clone()).collect();
        }
    }

    // Keys / Plugins segments.
    let seg_y = r.top() + 48.0;
    for (i, sr) in [Rect::from_min_size(pos2(x0 + 17.0, seg_y), vec2(133.0, 24.0)), Rect::from_min_size(pos2(x0 + 151.0, seg_y), vec2(133.0, 24.0))]
        .into_iter()
        .enumerate()
    {
        let on = (i == 1) == app.tab_plugins;
        if on {
            p.rect_filled(sr, 12.0, SEGMENT_ON);
        }
        let col = if on { TEXT_SOFT } else { TEXT_DIM };
        if i == 0 {
            let ir = Rect::from_center_size(pos2(sr.center().x - 18.0, sr.center().y), vec2(11.0, 13.0));
            p.rect_stroke(ir, 2.0, Stroke::new(1.2, col), StrokeKind::Inside);
            p.text(pos2(sr.center().x - 8.0, sr.center().y), Align2::LEFT_CENTER, "Keys", FontId::proportional(13.0), col);
        } else {
            p.text(sr.center(), Align2::CENTER_CENTER, "Plugins", FontId::proportional(13.0), col);
        }
        let seg_resp = ui.interact(sr, ui.scope_id().with(("seg", i)), Sense::click());
        seg_resp.widget_info(|| egui::WidgetInfo::selected(egui::WidgetType::CheckBox, true, on, if i == 0 { "Keys" } else { "Plugins" }));
        if seg_resp.clicked() {
            app.tab_plugins = i == 1;
        }
    }

    // The action tree.
    let tree = Rect::from_min_max(pos2(x0 + 2.0, r.top() + 80.0), r.max);
    let q = app.search.trim().to_lowercase();
    let is_neo = app.model().model == "20GBJ9901";
    let mut rows: Vec<(bool, usize, usize)> = Vec::new();
    for (ci, c) in app.cats.iter().enumerate() {
        if app.tab_plugins && !c.plugin {
            continue;
        }
        let hits: Vec<usize> = c
            .actions
            .iter()
            .enumerate()
            .filter(|(_, a)| !a.neo_only || is_neo)
            .filter(|(_, a)| q.is_empty() || a.name.to_lowercase().contains(&q) || c.name.to_lowercase().contains(&q))
            .map(|(i, _)| i)
            .collect();
        if hits.is_empty() && q.is_empty() {
            continue;
        }
        if !q.is_empty() && hits.is_empty() {
            continue;
        }
        rows.push((true, ci, 0));
        if app.expanded.contains(&c.name) || !q.is_empty() {
            rows.extend(hits.into_iter().map(|i| (false, ci, i)));
        }
    }
    let content_h: f32 = rows.iter().map(|r| if r.0 { ROW_CAT } else { ROW_ACTION }).sum();
    let max_scroll = (content_h - tree.height()).max(0.0);
    if ctx.input(|i| i.pointer.hover_pos()).is_some_and(|pt| tree.contains(pt)) {
        app.list_scroll -= ctx.input(|i| i.smooth_scroll_delta.y);
    }
    app.list_scroll = app.list_scroll.clamp(0.0, max_scroll);
    let clip = p.with_clip_rect(tree);
    let row_w = tree.width() - 11.0;
    let mut y = tree.top() - app.list_scroll;
    let mut toggle: Option<String> = None;
    let mut prev_child = false;
    for (is_cat, ci, ai) in rows {
        let h = if is_cat { ROW_CAT } else { ROW_ACTION };
        let rr = Rect::from_min_size(pos2(tree.left(), y), vec2(row_w, h));
        y += h;
        if rr.bottom() < tree.top() || rr.top() > tree.bottom() {
            prev_child = !is_cat;
            continue;
        }
        let vis = rr.intersect(tree);
        let sense = if is_cat { Sense::click() } else { Sense::click_and_drag() };
        let resp = ui.interact(vis, ui.scope_id().with(("row", ci, ai, is_cat)), sense);
        let row_label = if is_cat { app.cats[ci].name.clone() } else { app.cats[ci].actions[ai].name.clone() };
        resp.widget_info(|| egui::WidgetInfo::labeled(egui::WidgetType::TreeItem, true, &row_label));
        if is_cat {
            if !prev_child {
                clip.line_segment([pos2(rr.left(), rr.top() + 0.5), pos2(rr.right(), rr.top() + 0.5)], Stroke::new(1.0, ROW_LINE));
            }
            if resp.hovered() {
                clip.rect_filled(rr.shrink2(vec2(0.0, 1.0)), 0.0, hex(0x2f2f2f));
            }
            let name = app.cats[ci].name.clone();
            let icon = app.cats[ci].icon;
            let open = app.expanded.contains(&name) || !q.is_empty();
            let cc = pos2(rr.left() + 22.0, rr.center().y);
            let s = Stroke::new(1.3, hex(0xb4b4b4));
            if open {
                clip.line_segment([cc + vec2(-4.5, -2.5), cc + vec2(0.0, 2.0)], s);
                clip.line_segment([cc + vec2(0.0, 2.0), cc + vec2(4.5, -2.5)], s);
            } else {
                clip.line_segment([cc + vec2(-2.0, -4.5), cc + vec2(2.5, 0.0)], s);
                clip.line_segment([cc + vec2(2.5, 0.0), cc + vec2(-2.0, 4.5)], s);
            }
            let tex = app.icon_texture(&ctx, icon, [0xe6, 0xe6, 0xe6]);
            clip.image(tex, Rect::from_center_size(pos2(rr.left() + 52.0, rr.center().y), vec2(26.0, 26.0)), uv01(), Color32::WHITE);
            clip.text(pos2(rr.left() + 74.0, rr.center().y), Align2::LEFT_CENTER, &name, bold(13.0), hex(0xf0f0f0));
            if resp.clicked() {
                toggle = Some(name);
            }
            prev_child = false;
        } else {
            clip.rect_filled(rr, 0.0, if resp.hovered() { hex(0x2c2c2c) } else { PANEL_OPEN });
            let a = app.cats[ci].actions[ai].clone();
            let tex = app.icon_texture(&ctx, a.icon, [0xb4, 0xb4, 0xb4]);
            clip.image(tex, Rect::from_center_size(pos2(rr.left() + 53.0, rr.center().y), vec2(20.0, 20.0)), uv01(), Color32::WHITE);
            let plugin_running = app.plugin_for(&a.uuid).is_some();
            let col = if a.runnable || plugin_running { hex(0xcfcfcf) } else { hex(0x9a9a9a) };
            clip.text(pos2(rr.left() + 74.0, rr.center().y), Align2::LEFT_CENTER, &a.name, FontId::proportional(12.5), col);
            if resp.drag_started() {
                app.drag = Some(Drag::Action(a.uuid.clone()));
            }
            if resp.double_clicked() {
                // Double-click puts the action on the selected key if it's free, else the first free key.
                let target = app.selected.filter(|s| app.key_at(*s).is_none()).or_else(|| app.first_free());
                if let Some(t) = target {
                    app.place(&a.uuid, t, now);
                }
            }
            let plugin_problem = app.plugins.as_ref().and_then(|h| h.plugin_of(&a.uuid).map(|p| h.problems.get(p).cloned()));
            let tip = if a.runnable || plugin_running {
                a.tooltip.clone()
            } else if let Some(why) = plugin_problem {
                format!("{}\nIts plugin isn't running: {}", a.tooltip, why.unwrap_or_else(|| "it is starting".into()))
            } else if a.icon == crate::icons::Icon::Plugin {
                format!("{}\nIts plugin's manifest can't be read (store plugins are encrypted), so it can't run.", a.tooltip)
            } else {
                a.tooltip.clone()
            };
            resp.on_hover_text(tip);
            prev_child = true;
        }
    }
    if let Some(t) = toggle {
        if !app.expanded.remove(&t) {
            app.expanded.insert(t);
        }
    }
    if max_scroll > 0.0 {
        let track = Rect::from_min_max(pos2(r.right() - 11.0, tree.top() + 1.0), pos2(r.right() - 1.0, tree.bottom() - 1.0));
        let th = (tree.height() / content_h * track.height()).max(30.0);
        let ty = track.top() + (track.height() - th) * (app.list_scroll / max_scroll);
        p.rect_filled(Rect::from_min_size(pos2(track.left(), ty), vec2(track.width(), th)), 5.0, SCROLL);
        let resp = ui.interact(track, ui.scope_id().with("scroll"), Sense::drag());
        if resp.dragged() {
            app.list_scroll += resp.drag_delta().y * content_h / track.height();
        }
    }
}

fn drag_overlay(app: &mut App, ui: &mut egui::Ui) {
    let ctx = ui.ctx().clone();
    if app.drag.is_none() {
        return;
    }
    if !ctx.input(|i| i.pointer.any_down() || i.pointer.any_released()) {
        app.drag = None;
        return;
    }
    let Some(pt) = ctx.input(|i| i.pointer.hover_pos()) else { return };
    let p = ctx.layer_painter(egui::LayerId::new(egui::Order::Tooltip, egui::Id::new("dragimg")));
    let r = Rect::from_center_size(pt, vec2(64.0, 64.0));
    match &app.drag {
        Some(Drag::Action(u)) => {
            let icon = catalog::icon_for(&app.cats, u);
            let tex = app.icon_texture(&ctx, icon, [255, 255, 255]);
            p.rect_filled(r, 10.0, Color32::from_black_alpha(215));
            p.image(tex, r.shrink(14.0), uv01(), Color32::WHITE);
        }
        Some(Drag::Key(pos)) => {
            if let Some(k) = app.key_at(*pos).cloned() {
                let tex = app.key_texture(&ctx, &k, 144);
                p.add(RectShape::filled(r, CornerRadius::same(9), Color32::from_white_alpha(220)).with_texture(tex, uv01()));
            }
        }
        None => {}
    }
    // Let go outside a key: nothing changes.
    if ctx.input(|i| i.pointer.any_released()) {
        app.drag = None;
    }
}

fn menus(app: &mut App, ui: &mut egui::Ui, now: f64) {
    let ctx = ui.ctx().clone();
    let full = ctx.content_rect();
    let bar = top_bar(full.left(), full.right() - RIGHT_W, full.top() + TITLE_H);
    // The click that opened a menu must not also close it.
    let clicked = ctx.input(|i| i.pointer.any_click()) && !std::mem::take(&mut app.menu_opened);
    if app.device_menu {
        let id = egui::Id::new("devmenu");
        let mut close = false;
        let resp = egui::Area::new(id).fixed_pos(pos2(bar.left() + 4.0, bar.top() + 24.0)).order(egui::Order::Foreground).show(&ctx, |ui| {
            menu_frame().show(ui, |ui| {
                ui.set_min_width(280.0);
                match &app.connected {
                    Some((m, serial)) => {
                        ui.label(egui::RichText::new(format!("● {} connected", m.name)).color(hex(0x4cd964)));
                        if !serial.is_empty() {
                            ui.label(egui::RichText::new(format!("Serial {serial}")).color(TEXT_MUTED).small());
                        }
                    }
                    None => {
                        ui.label(egui::RichText::new("No Stream Deck connected").color(TEXT_DIM));
                        ui.label(egui::RichText::new("Plug one in — it is found automatically.").color(TEXT_MUTED).small());
                    }
                }
                ui.separator();
                ui.label(egui::RichText::new("Show the layout of").color(TEXT_MUTED).small());
                let mut seen = HashSet::new();
                for m in devices::MODELS {
                    if !seen.insert((m.name, m.cols, m.rows)) {
                        continue;
                    }
                    let on = app.model().name == m.name && app.model().cols == m.cols;
                    let resp = ui.add_enabled(app.connected.is_none(), egui::Button::selectable(on, format!("{}  ({}×{})", m.name, m.cols, m.rows)));
                    if resp.clicked() {
                        app.layout_model = m;
                        app.profiles[app.cur].model = m.model.to_string();
                        app.dirty = true;
                        close = true;
                    }
                }
                ui.separator();
                ui.horizontal(|ui| {
                    ui.label("Key brightness");
                    let mut b = app.brightness as f32;
                    if ui.add(egui::Slider::new(&mut b, 0.0..=100.0).suffix("%")).changed() {
                        app.set_brightness(b as u8);
                    }
                });
                ui.separator();
                // Starts in the tray at login, so LabDeck never needs a terminal (a second copy
                // changes nothing, like its other settings).
                let mut on = app.login_start;
                let resp = ui.add_enabled(app.persist, egui::Checkbox::new(&mut on, "Start LabDeck when I log in"));
                if resp.changed() {
                    app.set_login_start(on, now);
                }
            });
        });
        let over = ctx.pointer_hover_pos().is_some_and(|pt| resp.response.rect.contains(pt));
        if close || (clicked && !over) {
            app.device_menu = false;
        }
    }
    if app.profile_menu {
        let id = egui::Id::new("profmenu");
        let mut close = false;
        let resp = egui::Area::new(id).fixed_pos(pos2(bar.left() + 4.0, bar.top() + 48.0)).order(egui::Order::Foreground).show(&ctx, |ui| {
            menu_frame().show(ui, |ui| {
                ui.set_min_width(300.0);
                ui.label(egui::RichText::new("Profiles").color(TEXT_MUTED).small());
                let mut switch = None;
                egui::ScrollArea::vertical().max_height(360.0).show(ui, |ui| {
                    for (i, p) in app.profiles.iter().enumerate() {
                        let on = i == app.cur;
                        let device = devices::by_model(&p.model).map_or("Stream Deck", |m| m.name);
                        let pages = p.pages.len();
                        let detail = format!("{device} · {pages} page{}", if pages == 1 { "" } else { "s" });
                        let text = egui::RichText::new(&p.name).strong();
                        let resp = ui.add(egui::Button::selectable(on, text).min_size(vec2(290.0, 22.0)));
                        ui.label(egui::RichText::new(format!("      {detail}")).small().color(TEXT_MUTED));
                        if resp.clicked() {
                            switch = Some(i);
                        }
                    }
                });
                if let Some(i) = switch {
                    app.switch_profile(i);
                    close = true;
                }
                ui.separator();
                if let Some(name) = &mut app.rename {
                    let mut done = false;
                    ui.horizontal(|ui| {
                        ui.add(egui::TextEdit::singleline(name).desired_width(180.0));
                        done = ui.button("OK").clicked();
                    });
                    if done {
                        let n = name.trim().to_string();
                        if !n.is_empty() {
                            app.profiles[app.cur].name = n;
                            app.dirty = true;
                        }
                        app.rename = None;
                    }
                } else if ui.button("Rename profile…").clicked() {
                    app.rename = Some(app.profile().name.clone());
                }
                if ui.button("New profile").clicked() {
                    let p = Profile::new(&format!("Profile {}", app.profiles.len() + 1), app.model().model);
                    app.profiles.push(p);
                    app.switch_profile(app.profiles.len() - 1);
                    app.dirty = true;
                    close = true;
                }
                if ui.button("Duplicate profile").clicked() {
                    let mut p = app.profile().clone();
                    p.uuid = profile::new_uuid().to_uppercase();
                    p.name = format!("{} copy", p.name);
                    p.app = None;
                    app.profiles.push(p);
                    app.switch_profile(app.profiles.len() - 1);
                    app.dirty = true;
                    close = true;
                }
                let mut do_import = None;
                match &mut app.import_path {
                    Some(path) => {
                        ui.label(egui::RichText::new("Path of a .streamDeckProfile file:").small().color(TEXT_MUTED));
                        ui.horizontal(|ui| {
                            ui.add(egui::TextEdit::singleline(path).desired_width(200.0));
                            if ui.button("Import").clicked() {
                                do_import = Some(PathBuf::from(path.trim().trim_matches('"')));
                            }
                        });
                    }
                    None => {
                        if ui.button("Import profile…").clicked() {
                            app.import_path = Some(String::new());
                        }
                    }
                }
                if let Some(path) = do_import {
                    app.import_path = None;
                    match profile::import(&path, &profile::profiles_root()) {
                        Ok(p) => {
                            app.profiles.push(p);
                            app.switch_profile(app.profiles.len() - 1);
                            app.say("Profile imported", now);
                            close = true;
                        }
                        Err(e) => app.say(format!("Import failed: {e}"), now),
                    }
                }
                if app.profiles.len() > 1 && ui.button("Delete profile").clicked() {
                    let p = app.profiles.remove(app.cur);
                    if let Some(d) = p.dir {
                        if app.persist && d.starts_with(profile::profiles_root()) {
                            let _ = std::fs::remove_dir_all(d);
                        }
                    }
                    app.switch_profile(0);
                    close = true;
                }
            });
        });
        let over = ctx.pointer_hover_pos().is_some_and(|pt| resp.response.rect.contains(pt));
        if close || (clicked && !over && app.rename.is_none() && app.import_path.is_none()) {
            app.profile_menu = false;
        }
    }
}

fn status_line(app: &mut App, ui: &mut egui::Ui, body: Rect, now: f64) {
    let Some((msg, t)) = &app.status else { return };
    if now - t > 6.0 {
        app.status = None;
        return;
    }
    let p = ui.ctx().layer_painter(egui::LayerId::new(egui::Order::Foreground, egui::Id::new("status")));
    let galley = p.layout(msg.clone(), FontId::proportional(12.5), TEXT_SOFT, 700.0);
    let size = galley.size() + vec2(28.0, 14.0);
    let r = Rect::from_center_size(pos2(body.center().x, body.top() + 150.0), size);
    p.rect_filled(r, 6.0, hex(0x3a3a3a));
    p.galley(r.min + vec2(14.0, 7.0), galley, TEXT_SOFT);
    ui.ctx().request_repaint_after(std::time::Duration::from_millis(500));
}

pub fn uv01() -> Rect {
    Rect::from_min_max(pos2(0.0, 0.0), pos2(1.0, 1.0))
}

pub fn bold(size: f32) -> FontId {
    FontId::new(size, FontFamily::Name("bold".into()))
}

fn menu_frame() -> egui::Frame {
    egui::Frame::new()
        .fill(hex(0x353535))
        .stroke(Stroke::new(1.0, hex(0x474747)))
        .corner_radius(6.0)
        .inner_margin(10.0)
        .shadow(egui::epaint::Shadow { offset: [0, 4], blur: 12, spread: 0, color: Color32::from_black_alpha(120) })
}

fn chevron(p: &egui::Painter, c: Pos2, s: f32, col: Color32) {
    let st = Stroke::new(1.3, col);
    p.line_segment([pos2(c.x - s, c.y - s * 0.5), pos2(c.x, c.y + s * 0.5)], st);
    p.line_segment([pos2(c.x, c.y + s * 0.5), pos2(c.x + s, c.y - s * 0.5)], st);
}

fn bell(p: &egui::Painter, c: Pos2, col: Color32) {
    let st = Stroke::new(1.4, col);
    let pts: Vec<Pos2> = (0..=16)
        .map(|i| {
            let a = std::f32::consts::PI * (1.0 + i as f32 / 16.0);
            pos2(c.x + 5.5 * a.cos(), c.y - 2.0 + 5.5 * a.sin())
        })
        .collect();
    p.add(egui::Shape::line(pts, st));
    p.line_segment([pos2(c.x - 5.5, c.y - 2.0), pos2(c.x - 6.5, c.y + 5.0)], st);
    p.line_segment([pos2(c.x + 5.5, c.y - 2.0), pos2(c.x + 6.5, c.y + 5.0)], st);
    p.line_segment([pos2(c.x - 8.0, c.y + 5.0), pos2(c.x + 8.0, c.y + 5.0)], st);
    p.line_segment([pos2(c.x - 2.0, c.y + 8.0), pos2(c.x + 2.0, c.y + 8.0)], st);
}

fn gear(p: &egui::Painter, c: Pos2, col: Color32) {
    let st = Stroke::new(1.4, col);
    p.circle_stroke(c, 3.2, st);
    let pts: Vec<Pos2> = (0..=48)
        .map(|i| {
            let a = i as f32 / 48.0 * std::f32::consts::TAU;
            let r = if (i / 3) % 2 == 0 { 8.5 } else { 6.6 };
            pos2(c.x + r * a.cos(), c.y + r * a.sin())
        })
        .collect();
    p.add(egui::Shape::line(pts, st));
}

fn pin_icon(p: &egui::Painter, c: Pos2, col: Color32) {
    p.line_segment([pos2(c.x - 4.0, c.y + 4.0), pos2(c.x - 1.0, c.y + 1.0)], Stroke::new(1.6, col));
    p.add(egui::Shape::convex_polygon(
        vec![pos2(c.x - 2.5, c.y - 1.0), pos2(c.x + 1.0, c.y - 4.5), pos2(c.x + 4.5, c.y - 1.0), pos2(c.x + 1.0, c.y + 2.5)],
        col,
        Stroke::NONE,
    ));
}

fn font_bytes(paths: &[&str]) -> Option<Vec<u8>> {
    paths.iter().find_map(|p| std::fs::read(p).ok())
}

pub fn setup_fonts(ctx: &egui::Context) {
    let mut defs = egui::FontDefinitions::default();
    let regular = font_bytes(&[
        "C:/Windows/Fonts/segoeui.ttf",
        "/usr/share/fonts/truetype/noto/NotoSans-Regular.ttf",
        "/usr/share/fonts/noto/NotoSans-Regular.ttf",
        "/usr/share/fonts/google-noto/NotoSans-Regular.ttf",
        "/usr/share/fonts/truetype/dejavu/DejaVuSans.ttf",
        "/usr/share/fonts/TTF/DejaVuSans.ttf",
        "/usr/share/fonts/dejavu/DejaVuSans.ttf",
        "/usr/share/fonts/truetype/liberation/LiberationSans-Regular.ttf",
    ]);
    let boldf = font_bytes(&[
        "C:/Windows/Fonts/segoeuib.ttf",
        "/usr/share/fonts/truetype/noto/NotoSans-Bold.ttf",
        "/usr/share/fonts/noto/NotoSans-Bold.ttf",
        "/usr/share/fonts/google-noto/NotoSans-Bold.ttf",
        "/usr/share/fonts/truetype/dejavu/DejaVuSans-Bold.ttf",
        "/usr/share/fonts/TTF/DejaVuSans-Bold.ttf",
        "/usr/share/fonts/dejavu/DejaVuSans-Bold.ttf",
        "/usr/share/fonts/truetype/liberation/LiberationSans-Bold.ttf",
    ]);
    if let Some(b) = regular {
        defs.font_data.insert("ui-regular".into(), Arc::new(egui::FontData::from_owned(b)));
        defs.families.entry(FontFamily::Proportional).or_default().insert(0, "ui-regular".into());
    }
    let mut bold_family: Vec<String> = Vec::new();
    if let Some(b) = boldf {
        defs.font_data.insert("ui-bold".into(), Arc::new(egui::FontData::from_owned(b)));
        bold_family.push("ui-bold".into());
    }
    bold_family.extend(defs.families.get(&FontFamily::Proportional).cloned().unwrap_or_default());
    defs.families.insert(FontFamily::Name("bold".into()), bold_family);
    ctx.set_fonts(defs);
}

pub fn setup_style(ctx: &egui::Context) {
    ctx.set_theme(egui::Theme::Dark);
    ctx.all_styles_mut(|s| {
        s.visuals = egui::Visuals::dark();
        s.visuals.panel_fill = CANVAS;
        s.visuals.window_fill = hex(0x353535);
        s.visuals.extreme_bg_color = FIELD;
        s.visuals.widgets.inactive.bg_fill = hex(0x3b3b3b);
        s.visuals.widgets.inactive.weak_bg_fill = hex(0x3b3b3b);
        s.visuals.widgets.hovered.weak_bg_fill = hex(0x464646);
        s.visuals.widgets.active.weak_bg_fill = hex(0x505050);
        s.visuals.selection.bg_fill = ACCENT;
        s.visuals.override_text_color = Some(TEXT_SOFT);
        s.visuals.widgets.noninteractive.fg_stroke.color = TEXT_SOFT;
        s.visuals.widgets.inactive.fg_stroke.color = TEXT_SOFT;
        s.visuals.widgets.noninteractive.bg_stroke.color = hex(0x3c3c3c);
        s.text_styles.insert(egui::TextStyle::Body, FontId::proportional(13.0));
        s.text_styles.insert(egui::TextStyle::Button, FontId::proportional(13.0));
        s.spacing.item_spacing = vec2(8.0, 6.0);
    });
}
