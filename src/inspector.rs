//! The key editor under the device view (the original's "property inspector"):
//! title and look on the left, the action's own settings on the right.

use crate::actions::{self, BRIGHTNESS, MEDIA};
use crate::app::App;
use crate::catalog;
use crate::icons::{self, Icon};
use crate::profile::{KeyAction, StateDef};
use crate::theme::*;
use egui::{Align2, Color32, FontId, Rect, RichText, pos2, vec2};

#[derive(Default)]
pub struct State {
    /// Which hotkey slot is listening for a key press: (key action id, slot).
    pub recording: Option<(String, usize)>,
    pub step: Option<usize>,
    /// Enter pressed in a Website key's URL field: fetch the site's icon for it.
    pub fetch_icon: Option<String>,
}

struct Ctx {
    profiles: Vec<(String, String)>,
    pages: usize,
    steps: Vec<(String, String, usize)>,
}

pub fn show(app: &mut App, ui: &mut egui::Ui, rect: Rect, now: f64) {
    let p = ui.painter().clone();
    p.rect_filled(rect, 0.0, CANVAS);
    let Some(pos) = app.selected else {
        let mut child = ui.new_child(egui::UiBuilder::new().max_rect(Rect::from_center_size(rect.center(), vec2(400.0, 24.0))));
        child.with_layout(egui::Layout::centered_and_justified(egui::Direction::TopDown), |ui| {
            ui.label(RichText::new("Select a key to configure its action.").color(TEXT_DIM).size(13.0));
        });
        return;
    };
    let Some(original) = app.key_at(pos).cloned() else {
        hint(&p, rect, "Drag an action from the list on the right onto this key.");
        return;
    };
    let ctx = Ctx {
        profiles: app.profiles.iter().map(|p| (p.uuid.clone(), p.name.clone())).collect(),
        pages: app.profile().pages.len(),
        steps: catalog::builtin()
            .into_iter()
            .filter(|c| c.name != "Multi Action")
            .flat_map(|c| c.actions.into_iter())
            // Actions whose manifest says SupportedInMultiActions: false can't be steps.
            .filter(|a| a.runnable && !a.neo_only && catalog::allowed_in_multi_action(&a.uuid))
            .map(|a| (a.uuid, a.name, a.states))
            .chain(std::iter::once(("com.elgato.streamdeck.multiactions.delay".to_string(), "Delay".to_string(), 1)))
            // Actions of running plugins, unless their manifest keeps them out of Multi Actions.
            .chain(
                app.cats
                    .iter()
                    .filter(|c| c.plugin)
                    .flat_map(|c| c.actions.iter())
                    .filter(|a| !a.neo_only && app.plugin_for(&a.uuid).is_some() && app.allowed_in_multi(&a.uuid))
                    .map(|a| (a.uuid.clone(), a.name.clone(), a.states)),
            )
            .collect(),
    };
    let def = catalog::find(&app.cats, &original.uuid).cloned();
    // A plugin's action: its plugin's name, whether it runs, and why not.
    let plugin = app.plugins.as_ref().and_then(|h| {
        let p = h.plugin_of(&original.uuid)?;
        Some((h.info(p).map(|i| i.name.clone()).unwrap_or_else(|| p.to_string()), h.running.contains(p), h.problems.get(p).cloned()))
    });
    let runnable = def.as_ref().map(|d| d.runnable).unwrap_or(original.uuid.starts_with("com.elgato.streamdeck"))
        || plugin.as_ref().is_some_and(|p| p.1);
    let icon = crate::app::key_icon(&app.cats, &original);
    let mut k = original.clone();
    let before = serde_json::to_string(&k).unwrap_or_default();

    let inner = rect.shrink2(vec2(0.0, 18.0));
    let left_x = (rect.center().x - 590.0).max(rect.left() + 24.0);
    let mut action = None;
    let mut child = ui.new_child(egui::UiBuilder::new().max_rect(Rect::from_min_max(pos2(left_x, inner.top()), pos2(rect.right() - 24.0, inner.bottom()))));
    egui::ScrollArea::vertical().id_salt("pi").auto_shrink([false, false]).show(&mut child, |ui| {
        // Header: icon, action name, run and delete.
        ui.horizontal(|ui| {
            let tex = app.icon_texture(ui.ctx(), icon, [0xe6, 0xe6, 0xe6]);
            ui.add(egui::Image::new((tex, vec2(22.0, 22.0))));
            ui.label(RichText::new(&k.name).font(crate::view::bold(15.0)).color(TEXT));
            if let Some(d) = &def {
                ui.label(RichText::new(&d.tooltip).color(TEXT_MUTED));
            }
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                if ui.button("Delete").clicked() {
                    action = Some("delete");
                }
                if ui.add_enabled(runnable, egui::Button::new("Run")).on_hover_text("Run this key now, as if it was pressed").clicked() {
                    action = Some("run");
                }
                if k.uuid == "com.elgato.streamdeck.profile.openchild" && ui.button("Open folder").clicked() {
                    action = Some("folder");
                }
            });
        });
        ui.add_space(10.0);
        ui.horizontal_top(|ui| {
            // Left: key look.
            ui.vertical(|ui| {
                ui.set_width(360.0);
                let tex = app.key_texture(ui.ctx(), &k, 144);
                ui.horizontal(|ui| {
                    ui.add(egui::Image::new((tex, vec2(72.0, 72.0))).corner_radius(9.0));
                    ui.vertical(|ui| {
                        if k.states.len() > 1 {
                            ui.horizontal(|ui| {
                                ui.label("State");
                                for i in 0..k.states.len() {
                                    if ui.selectable_label(k.state == i, format!("{}", i + 1)).clicked() {
                                        k.state = i;
                                    }
                                }
                            });
                        }
                        ui.label(RichText::new("Title").color(TEXT_DIM));
                        let st = k.cur_mut();
                        let mut title = st.title.clone().unwrap_or_default();
                        if ui.add(egui::TextEdit::multiline(&mut title).desired_rows(2).desired_width(260.0)).changed() {
                            st.title = if title.is_empty() { None } else { Some(title) };
                        }
                    });
                });
                ui.add_space(6.0);
                look_ui(ui, k.cur_mut());
            });
            ui.add_space(30.0);
            ui.separator();
            ui.add_space(20.0);
            // Right: the action's settings.
            ui.vertical(|ui| {
                ui.set_width(520.0);
                match &plugin {
                    Some((name, true, _)) => {
                        ui.label(RichText::new(format!("This action comes from the plugin “{name}”, which is running.")).color(TEXT_DIM));
                    }
                    Some((name, false, why)) => {
                        let why = why.clone().unwrap_or_else(|| "it is starting".into());
                        ui.label(RichText::new(format!("This action comes from the plugin “{name}”, which isn't running: {why}.")).color(TEXT_DIM));
                    }
                    None if !runnable => {
                        ui.label(RichText::new("This action comes from a plugin that isn't in LabDeck's Plugins folder. It is kept on the key and saved with the profile.").color(TEXT_DIM));
                    }
                    None => {}
                }
                if plugin.is_some() || !runnable {
                    ui.add_space(6.0);
                    plugin_settings_ui(ui, &mut k);
                }
                let st = &mut app.ins;
                settings_ui(ui, &mut k, &ctx, st, 0);
            });
        });
    });

    if serde_json::to_string(&k).unwrap_or_default() != before {
        if let Some(slot) = app.key_at_mut(pos) {
            *slot = k.clone();
        }
        app.changed(now);
        // A plugin hears about new settings, as from its own settings panel.
        if k.settings != original.settings {
            app.plugin_settings_changed(&k, pos);
        }
    }
    if let Some(url) = app.ins.fetch_icon.take() {
        app.fetch_site_icon(k.action_id.clone(), url, now);
    }
    match action {
        Some("run") => {
            app.press(pos, now);
            app.release(pos, now);
        }
        Some("delete") => app.remove_key(pos, now),
        Some("folder") => app.open_folder(k.setting_str("ProfileUUID"), now),
        _ => {}
    }
}

fn hint(p: &egui::Painter, rect: Rect, text: &str) {
    p.text(rect.center(), Align2::CENTER_CENTER, text, FontId::proportional(13.0), TEXT_DIM);
}

/// One colour: the swatch opens the colour wheel; beside it the colour's code, which can be
/// typed or pasted into (Ctrl+V), and Paste / Copy buttons. Pasted codes are read in the forms
/// people copy them (see `parse_colour`), including what the wheel's own copy button gives.
/// Drawn in the look grid's value column; `label` is the row's name (also its id).
fn color_field(ui: &mut egui::Ui, label: &str, value: &mut Option<String>, default: &str) {
    ui.horizontal(|ui| {
        let cur = value.clone().unwrap_or_else(|| default.to_string());
        let rgba = icons::Rgba::hex(&cur);
        let byte = |v: f32| (v * 255.0).round() as u8;
        let mut c = Color32::from_rgb(byte(rgba.0), byte(rgba.1), byte(rgba.2));
        if ui.color_edit_button_srgba(&mut c).changed() {
            *value = Some(hex_code(c));
        }

        let text_id = ui.make_persistent_id(("colour code", label));
        let bad_id = text_id.with("not a colour");
        // Ctrl+V in the code field: a pasted colour code replaces the code instead of being
        // inserted into it.
        if ui.memory(|m| m.has_focus(text_id)) {
            let pasted = ui.input(|i| {
                i.events.iter().find_map(|e| match e {
                    egui::Event::Paste(s) => Some(s.clone()),
                    _ => None,
                })
            });
            if let Some(code) = pasted.as_deref().and_then(parse_colour) {
                ui.input_mut(|i| i.events.retain(|e| !matches!(e, egui::Event::Paste(_))));
                *value = Some(code.clone());
                ui.data_mut(|d| d.insert_temp(text_id, code));
            }
        }
        let shown = value.clone().unwrap_or_else(|| default.to_string());
        let mut text = ui.data_mut(|d| d.get_temp::<String>(text_id)).unwrap_or(shown);
        let resp = ui.add(
            egui::TextEdit::singleline(&mut text)
                .id(text_id)
                .desired_width(70.0)
                .font(egui::TextStyle::Monospace),
        );
        if resp.changed() {
            if let Some(code) = parse_colour(&text) {
                *value = Some(code);
            }
        }
        // While typing, keep what was typed; afterwards show the colour's code again.
        if resp.has_focus() {
            ui.data_mut(|d| d.insert_temp(text_id, text));
        } else {
            ui.data_mut(|d| d.remove::<String>(text_id));
        }

        if ui.small_button("Paste").on_hover_text("Use the colour code on the clipboard").clicked() {
            match clipboard_text().as_deref().and_then(parse_colour) {
                Some(code) => {
                    *value = Some(code);
                    ui.data_mut(|d| d.remove::<f64>(bad_id));
                }
                None => {
                    let now = ui.input(|i| i.time);
                    ui.data_mut(|d| d.insert_temp(bad_id, now));
                }
            }
        }
        if ui.small_button("Copy").on_hover_text("Copy this colour's code").clicked() {
            ui.ctx().copy_text(value.clone().unwrap_or_else(|| default.to_string()));
        }
        // For a few seconds after Paste found no colour code, say so.
        if let Some(at) = ui.data(|d| d.get_temp::<f64>(bad_id)) {
            let left = 3.0 - (ui.input(|i| i.time) - at);
            if left > 0.0 {
                ui.label(RichText::new("No colour code on the clipboard").color(TEXT_MUTED));
                ui.ctx().request_repaint_after(std::time::Duration::from_secs_f64(left));
            } else {
                ui.data_mut(|d| d.remove::<f64>(bad_id));
            }
        }
    });
}

/// A plugin key's settings as JSON text. A plugin's own settings panel is a web page, which
/// LabDeck can't show, so the settings are edited here; text that isn't a JSON object is kept
/// while typing but not saved.
fn plugin_settings_ui(ui: &mut egui::Ui, k: &mut KeyAction) {
    ui.label(RichText::new("Settings (JSON)").color(TEXT_DIM));
    let id = ui.make_persistent_id(("plugin settings", k.action_id.clone()));
    let saved = serde_json::to_string_pretty(&k.settings).unwrap_or_else(|_| "{}".into());
    let mut text = ui.data_mut(|d| d.get_temp::<String>(id)).unwrap_or(saved);
    let resp = ui.add(egui::TextEdit::multiline(&mut text).code_editor().desired_rows(6).desired_width(480.0));
    let parsed = serde_json::from_str::<serde_json::Value>(&text).ok().filter(|v| v.is_object());
    if resp.changed() {
        if let Some(v) = parsed.clone() {
            k.settings = v;
        }
    }
    if resp.has_focus() {
        ui.data_mut(|d| d.insert_temp(id, text));
    } else {
        ui.data_mut(|d| d.remove::<String>(id));
    }
    if parsed.is_none() {
        ui.label(RichText::new("Not a JSON object yet, so not saved.").color(TEXT_MUTED));
    }
}

fn hex_code(c: Color32) -> String {
    format!("#{:02x}{:02x}{:02x}", c.r(), c.g(), c.b())
}

/// The system clipboard's text, read when Paste is pressed.
fn clipboard_text() -> Option<String> {
    arboard::Clipboard::new().ok()?.get_text().ok()
}

/// Reads a colour code the way people copy them and gives it back as `#rrggbb`: `#ff8800`,
/// `ff8800`, `#f80`, `0xff8800`, `#ff8800cc` (the alpha is dropped), `255, 136, 0` (what the
/// colour wheel's own copy button gives), `255 136 0`, `rgb(255, 136, 0)`, `rgba(…)`, and
/// fractions such as `1.0, 0.5, 0.0`. Anything else is not a colour code.
pub fn parse_colour(text: &str) -> Option<String> {
    let t = text.trim().trim_matches(|c| c == '"' || c == '\'').trim().to_ascii_lowercase();
    if t.is_empty() {
        return None;
    }
    let (hex, marked) = match t.strip_prefix('#').or_else(|| t.strip_prefix("0x")) {
        Some(h) => (h, true),
        None => (t.as_str(), false),
    };
    if !hex.is_empty() && hex.chars().all(|c| c.is_ascii_hexdigit()) {
        return match hex.len() {
            3 | 4 if marked => Some(format!("#{}", hex.chars().take(3).flat_map(|c| [c, c]).collect::<String>())),
            6 | 8 => Some(format!("#{}", &hex[..6])),
            _ => None,
        };
    }
    let inner = t.strip_prefix("rgba").or_else(|| t.strip_prefix("rgb")).unwrap_or(t.as_str());
    let inner = inner.trim().trim_start_matches('(').trim_end_matches(')');
    let parts: Vec<&str> = inner.split(|c: char| c == ',' || c == ';' || c.is_whitespace()).filter(|s| !s.is_empty()).collect();
    if parts.len() != 3 && parts.len() != 4 {
        return None;
    }
    let nums: Vec<f64> = parts.iter().map(|p| p.parse::<f64>()).collect::<Result<_, _>>().ok()?;
    let fractions = parts[..3].iter().any(|p| p.contains('.')) && nums[..3].iter().all(|v| (0.0..=1.0).contains(v));
    let mut out = String::from("#");
    for v in &nums[..3] {
        let b = if fractions { v * 255.0 } else { *v };
        if !(0.0..=255.0).contains(&b) || (!fractions && b.fract() != 0.0) {
            return None;
        }
        out.push_str(&format!("{:02x}", b.round() as u8));
    }
    Some(out)
}

#[cfg(test)]
mod tests {
    use super::parse_colour;

    #[test]
    fn colour_codes_paste_in_the_forms_people_copy() {
        for (text, want) in [
            ("#ff8800", "#ff8800"),
            ("  #FF8800 ", "#ff8800"),
            ("ff8800", "#ff8800"),
            ("#f80", "#ff8800"),
            ("0xff8800", "#ff8800"),
            ("#ff8800cc", "#ff8800"),
            ("255, 136, 0", "#ff8800"), // the colour wheel's own copy button
            ("255, 136, 0, 255", "#ff8800"),
            ("255 136 0", "#ff8800"),
            ("rgb(255, 136, 0)", "#ff8800"),
            ("rgba(255,136,0,0.5)", "#ff8800"),
            ("1.0, 0.5333, 0.0", "#ff8800"),
            ("\"#ff8800\"", "#ff8800"),
        ] {
            assert_eq!(parse_colour(text).as_deref(), Some(want), "{text}");
        }
        assert_eq!(parse_colour("#f808").as_deref(), Some("#ff8800")); // #rgba short form
        for text in ["", "hello", "#ff88800", "fff", "256, 0, 0", "1, 2", "12.5, 3, 4"] {
            assert_eq!(parse_colour(text), None, "{text}");
        }
    }
}

fn look_ui(ui: &mut egui::Ui, st: &mut StateDef) {
    egui::Grid::new("look").num_columns(2).spacing(vec2(12.0, 6.0)).show(ui, |ui| {
        ui.label("Show title");
        ui.checkbox(&mut st.show_title, "");
        ui.end_row();
        ui.label("Title position");
        let cur = st.title_alignment.clone().unwrap_or_else(|| "bottom".into());
        egui::ComboBox::from_id_salt("align").selected_text(cur.clone()).show_ui(ui, |ui| {
            for a in ["top", "middle", "bottom"] {
                if ui.selectable_label(cur == a, a).clicked() {
                    st.title_alignment = Some(a.into());
                }
            }
        });
        ui.end_row();
        ui.label("Font size");
        let mut fs = st.font_size.unwrap_or(9.0);
        if ui.add(egui::DragValue::new(&mut fs).range(6.0..=40.0)).changed() {
            st.font_size = Some(fs);
        }
        ui.end_row();
        ui.label("Icon");
        let cur = st.icon.clone().unwrap_or_default();
        let label = if cur.is_empty() { "(action default)".to_string() } else { cur.clone() };
        egui::ComboBox::from_id_salt("icon").selected_text(label).height(300.0).show_ui(ui, |ui| {
            if ui.selectable_label(st.icon.is_none(), "(action default)").clicked() {
                st.icon = None;
            }
            if ui.selectable_label(cur.is_empty() && st.icon.is_some(), "(none)").clicked() {
                st.icon = Some(String::new());
            }
            for i in icons::ALL {
                if ui.selectable_label(cur == i.key(), i.key()).clicked() {
                    st.icon = Some(i.key().into());
                }
            }
        });
        ui.end_row();
        ui.label("Picture file");
        let mut img = st.image.clone().unwrap_or_default();
        ui.horizontal(|ui| {
            if ui.add(egui::TextEdit::singleline(&mut img).desired_width(170.0).hint_text("drop a file on the key")).changed() {
                st.image = if img.is_empty() { None } else { Some(img.clone()) };
            }
            if st.image.is_some() && ui.small_button("Remove").on_hover_text("Remove picture").clicked() {
                st.image = None;
            }
        });
        ui.end_row();
        ui.label("Title colour");
        color_field(ui, "Title colour", &mut st.title_color, "#ffffff");
        ui.end_row();
        ui.label("Background");
        color_field(ui, "Background", &mut st.background, "#000000");
        ui.end_row();
        ui.label("Icon colour");
        color_field(ui, "Icon colour", &mut st.icon_color, "#ffffff");
        ui.end_row();
    });
}

fn text_setting(ui: &mut egui::Ui, k: &mut KeyAction, key: &str, label: &str, hint: &str) {
    ui.label(RichText::new(label).color(TEXT_DIM));
    let mut v = k.setting_str(key);
    if ui.add(egui::TextEdit::singleline(&mut v).desired_width(420.0).hint_text(hint)).changed() {
        set(k, key, serde_json::Value::String(v));
    }
}

fn set(k: &mut KeyAction, key: &str, v: serde_json::Value) {
    if !k.settings.is_object() {
        k.settings = serde_json::json!({});
    }
    k.settings[key] = v;
}

fn idx_setting(k: &KeyAction) -> usize {
    k.settings.get("actionIdx").and_then(|v| v.as_u64()).unwrap_or(0) as usize
}

fn settings_ui(ui: &mut egui::Ui, k: &mut KeyAction, ctx: &Ctx, st: &mut State, depth: usize) {
    match k.uuid.as_str() {
        "com.elgato.streamdeck.system.hotkey" => {
            ui.label(RichText::new("Hotkey").color(TEXT_DIM));
            hotkey_slot(ui, k, 0, st);
        }
        actions::TIMED_HOTKEY => {
            ui.label(RichText::new("Hotkey").color(TEXT_DIM));
            hotkey_slot(ui, k, 0, st);
            let field = |ui: &mut egui::Ui, k: &mut KeyAction, key: &str, label: &str, unit: &str, max: u64, default: u64| {
                let mut v = k.settings.get(key).and_then(|v| v.as_u64()).unwrap_or(default);
                ui.horizontal(|ui| {
                    ui.add_sized([150.0, 20.0], egui::Label::new(label));
                    if ui.add(egui::DragValue::new(&mut v).range(0..=max).speed(10).suffix(unit)).changed() {
                        set(k, key, v.into());
                    }
                });
            };
            field(ui, k, "delayMs", "Wait before", " ms", 3_600_000, 0);
            field(ui, k, "holdMs", "Hold keys for", " ms", 600_000, actions::TIMED_HOLD_MS);
            field(ui, k, "repeatMs", "Repeat every", " ms", 3_600_000, 0);
            field(ui, k, "repeatCount", "Times", "", 100_000, 0);
            ui.label(
                RichText::new("Repeat every 0 = press once. Times 0 = keep repeating until the key is pressed again. Pressing the key while it runs stops it.")
                    .color(TEXT_MUTED),
            );
        }
        "com.elgato.streamdeck.system.hotkeyswitch" => {
            ui.label(RichText::new("Hotkey for state 1").color(TEXT_DIM));
            hotkey_slot(ui, k, 0, st);
            ui.label(RichText::new("Hotkey for state 2").color(TEXT_DIM));
            hotkey_slot(ui, k, 1, st);
        }
        "com.elgato.streamdeck.system.text" => {
            ui.label(RichText::new("Text").color(TEXT_DIM));
            let mut v = k.setting_str("pastedText");
            if ui.add(egui::TextEdit::multiline(&mut v).desired_rows(4).desired_width(420.0)).changed() {
                set(k, "pastedText", v.into());
            }
            let mut enter = k.settings.get("isSendingEnter").and_then(|v| v.as_bool()).unwrap_or(false);
            if ui.checkbox(&mut enter, "Press Enter afterwards").changed() {
                set(k, "isSendingEnter", enter.into());
            }
        }
        "com.elgato.streamdeck.system.website" => {
            ui.label(RichText::new("URL").color(TEXT_DIM));
            let mut v = k.setting_str("path");
            let resp = ui.add(egui::TextEdit::singleline(&mut v).desired_width(420.0).hint_text("https://"));
            if resp.changed() {
                set(k, "path", serde_json::Value::String(v.clone()));
            }
            // Enter: fetch the site's icon and make it the key's picture.
            if resp.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter)) && !v.trim().is_empty() && depth == 0 {
                st.fetch_icon = Some(v.trim().to_string());
            }
            ui.horizontal(|ui| {
                if depth == 0 && ui.button("Get icon from website").clicked() && !v.trim().is_empty() {
                    st.fetch_icon = Some(v.trim().to_string());
                }
                ui.label(RichText::new("or press Enter after typing the address").color(TEXT_MUTED));
            });
            let mut b = k.settings.get("openInBrowser").and_then(|v| v.as_bool()).unwrap_or(true);
            if ui.checkbox(&mut b, "Open in browser").changed() {
                set(k, "openInBrowser", b.into());
            }
        }
        "com.elgato.streamdeck.system.open" => text_setting(ui, k, "path", "App / File / Folder", "/usr/bin/firefox, ~/Music, xdg-open target…"),
        "com.elgato.streamdeck.system.openapp" => {
            text_setting(ui, k, "exec", "Application", "program name or full path");
            text_setting(ui, k, "args", "Arguments", "");
        }
        "com.elgato.streamdeck.system.close" => text_setting(ui, k, "app_name", "Application to close", "process name, e.g. firefox"),
        "com.elgato.streamdeck.system.multimedia" => {
            // Shows the saved choice by name, also one LabDeck can't send (the original's table
            // has Mac keyboard keys after Volume Down), instead of clamping it to the last item.
            ui.label(RichText::new("Action").color(TEXT_DIM));
            let cur = actions::media_index(&k.settings);
            let shown = actions::media_name(cur).unwrap_or("Unknown");
            egui::ComboBox::from_id_salt("media").selected_text(shown).width(220.0).show_ui(ui, |ui| {
                for (i, it) in MEDIA.iter().enumerate() {
                    if ui.selectable_label(i == cur, *it).clicked() {
                        set(k, "actionIdx", i.into());
                    }
                }
            });
            if cur >= MEDIA.len() {
                ui.label(RichText::new("LabDeck can't send this one on this system yet.").color(TEXT_MUTED));
            }
        }
        "com.elgato.streamdeck.system.keybrightness" => {
            combo(ui, k, "Action", BRIGHTNESS, "bright");
        }
        "com.elgato.streamdeck.system.timer" => {
            ui.label(RichText::new("Duration").color(TEXT_DIM));
            let mut secs = crate::app::timer_seconds(k).unwrap_or(0);
            let (mut m, mut s) = (secs / 60, secs % 60);
            ui.horizontal(|ui| {
                let a = ui.add(egui::DragValue::new(&mut m).range(0..=999).suffix(" min")).changed();
                let b = ui.add(egui::DragValue::new(&mut s).range(0..=59).suffix(" s")).changed();
                if a || b {
                    secs = m * 60 + s;
                    set(k, "duration", secs.into());
                }
            });
            // The sound when time is up (actionIdx): LabDeck's chimes, the person's file, or none.
            ui.label(RichText::new("Sound when time is up").color(TEXT_DIM));
            let cur = actions::timer_sound(&k.settings);
            let label = actions::TIMER_SOUNDS.iter().find(|(i, _)| *i == cur).map_or("Bell", |(_, l)| *l);
            egui::ComboBox::from_id_salt("timer-sound").selected_text(label).width(220.0).show_ui(ui, |ui| {
                for (i, l) in actions::TIMER_SOUNDS {
                    if ui.selectable_label(i == cur, l).clicked() {
                        set(k, "actionIdx", i.into());
                    }
                }
            });
            if cur == actions::TIMER_SOUND_FILE {
                text_setting(ui, k, "lastUserFile", "Sound file", "/home/you/sounds/alarm.wav");
            }
            ui.label(
                RichText::new("Press to start, press again to pause or resume. Hold for half a second to stop and reset to this time. A time of 0 doesn't start.")
                    .color(TEXT_MUTED),
            );
        }
        "com.elgato.streamdeck.soundboard.playaudio" => {
            text_setting(ui, k, "path", "Sound file", "/home/you/sounds/clip.mp3");
            let mut vol = k.settings.get("volume").and_then(|v| v.as_f64()).unwrap_or(100.0);
            ui.horizontal(|ui| {
                ui.label("Volume");
                if ui.add(egui::Slider::new(&mut vol, 0.0..=100.0).suffix("%")).changed() {
                    set(k, "volume", vol.into());
                }
            });
        }
        "com.elgato.streamdeck.page.goto" => {
            ui.label(RichText::new("Page").color(TEXT_DIM));
            let mut n = k.settings.get("PageIndex").and_then(|v| v.as_u64()).unwrap_or(1);
            if ui.add(egui::DragValue::new(&mut n).range(1..=ctx.pages.max(1) as u64)).changed() {
                set(k, "PageIndex", n.into());
            }
        }
        "com.elgato.streamdeck.profile.rotate" => {
            ui.label(RichText::new("Profile").color(TEXT_DIM));
            let cur = k.setting_str("ProfileUUID");
            let name = ctx.profiles.iter().find(|p| p.0.eq_ignore_ascii_case(&cur)).map(|p| p.1.clone()).unwrap_or_else(|| "Next profile".into());
            egui::ComboBox::from_id_salt(("prof", depth)).selected_text(name).show_ui(ui, |ui| {
                if ui.selectable_label(cur.is_empty(), "Next profile").clicked() {
                    set(k, "ProfileUUID", "".into());
                }
                for (id, n) in &ctx.profiles {
                    if ui.selectable_label(cur.eq_ignore_ascii_case(id), n).clicked() {
                        set(k, "ProfileUUID", id.clone().into());
                    }
                }
            });
        }
        "com.elgato.streamdeck.system.digitaltime" => text_setting(ui, k, "format", "Format", "HH:mm"),
        "com.elgato.streamdeck.profile.openchild" => {
            ui.label(RichText::new("Double-click the key, or use “Open folder”, to fill the folder's own keys.").color(TEXT_MUTED));
        }
        "com.elgato.streamdeck.multiactions.delay" => {
            ui.label(RichText::new("Delay").color(TEXT_DIM));
            let mut ms = k.settings.get("delay").and_then(|v| v.as_u64()).unwrap_or(1000);
            if ui.add(egui::DragValue::new(&mut ms).range(0..=10_000).suffix(" ms")).changed() {
                set(k, "delay", ms.into());
            }
        }
        "com.elgato.streamdeck.multiactions.routine" | "com.elgato.streamdeck.multiactions.random" if depth == 0 => {
            steps_ui(ui, k, 0, ctx, st);
        }
        "com.elgato.streamdeck.multiactions.routine2" if depth == 0 => {
            ui.label(RichText::new("Steps for state 1").font(crate::view::bold(13.0)));
            steps_ui(ui, k, 0, ctx, st);
            ui.add_space(8.0);
            ui.label(RichText::new("Steps for state 2").font(crate::view::bold(13.0)));
            steps_ui(ui, k, 1, ctx, st);
        }
        _ => {
            ui.label(RichText::new("This action has no settings.").color(TEXT_MUTED));
        }
    }
}

fn combo(ui: &mut egui::Ui, k: &mut KeyAction, label: &str, items: &[&str], id: &str) {
    ui.label(RichText::new(label).color(TEXT_DIM));
    let cur = idx_setting(k).min(items.len() - 1);
    egui::ComboBox::from_id_salt(id).selected_text(items[cur]).width(220.0).show_ui(ui, |ui| {
        for (i, it) in items.iter().enumerate() {
            if ui.selectable_label(i == cur, *it).clicked() {
                set(k, "actionIdx", i.into());
            }
        }
    });
}

fn hotkey_text(hk: &serde_json::Value) -> String {
    let vk = hk.get("VKeyCode").and_then(|v| v.as_i64()).unwrap_or(-1);
    if vk < 0 {
        return "Not set".into();
    }
    let f = |n: &str| hk.get(n).and_then(|v| v.as_bool()).unwrap_or(false);
    let mut s = String::new();
    for (n, l) in [("KeyCtrl", "Ctrl + "), ("KeyShift", "Shift + "), ("KeyOption", "Alt + "), ("KeyCmd", "Super + ")] {
        if f(n) {
            s.push_str(l);
        }
    }
    s + &actions::vk_name(vk)
}

fn hotkey_slot(ui: &mut egui::Ui, k: &mut KeyAction, slot: usize, st: &mut State) {
    if k.settings.get("Hotkeys").and_then(|h| h.as_array()).is_none_or(|a| a.len() <= slot) {
        let mut list: Vec<serde_json::Value> = k.settings.get("Hotkeys").and_then(|h| h.as_array()).cloned().unwrap_or_default();
        while list.len() < 4.max(slot + 1) {
            list.push(actions::empty_hotkey());
        }
        set(k, "Hotkeys", serde_json::Value::Array(list));
    }
    let me = (k.action_id.clone(), slot);
    let listening = st.recording.as_ref() == Some(&me);
    let text = if listening { "Press a key combination…".to_string() } else { hotkey_text(&k.settings["Hotkeys"][slot]) };
    ui.horizontal(|ui| {
        let b = ui.add_sized([260.0, 26.0], egui::Button::new(text).selected(listening));
        if b.clicked() {
            st.recording = if listening { None } else { Some(me.clone()) };
        }
        if ui.small_button("Clear").clicked() {
            k.settings["Hotkeys"][slot] = actions::empty_hotkey();
        }
    });
    if listening {
        let got = ui.input(|i| {
            i.events.iter().find_map(|e| match e {
                egui::Event::Key { key, physical_key, pressed: true, modifiers, .. } => Some((*key, *physical_key, *modifiers)),
                _ => None,
            })
        });
        if let Some((key, physical, m)) = got {
            if key == egui::Key::Escape && !m.any() {
                st.recording = None;
            } else if let Some(vk) = actions::recorded_vk(key, physical) {
                k.settings["Hotkeys"][slot] = actions::hotkey_json(vk, m.ctrl, m.shift, m.alt, m.mac_cmd || m.command && cfg!(target_os = "macos"));
                st.recording = None;
            }
        }
    }
}

fn steps_ui(ui: &mut egui::Ui, k: &mut KeyAction, group: usize, ctx: &Ctx, st: &mut State) {
    let mut steps = actions::sub_actions(k, group);
    let mut changed = false;
    let mut remove = None;
    let mut swap = None;
    let base = group * 1000;
    for (i, s) in steps.iter().enumerate() {
        ui.horizontal(|ui| {
            let on = st.step == Some(base + i);
            let label = match s.uuid.as_str() {
                "com.elgato.streamdeck.multiactions.delay" => format!("{}.  Delay {} ms", i + 1, s.settings.get("delay").and_then(|v| v.as_u64()).unwrap_or(1000)),
                _ => format!("{}.  {}", i + 1, s.name),
            };
            if ui.add_sized([300.0, 22.0], egui::Button::selectable(on, label)).clicked() {
                st.step = if on { None } else { Some(base + i) };
            }
            if i > 0 && ui.small_button("Up").clicked() {
                swap = Some((i, i - 1));
            }
            if i + 1 < steps.len() && ui.small_button("Down").clicked() {
                swap = Some((i, i + 1));
            }
            if ui.small_button("Remove").clicked() {
                remove = Some(i);
            }
        });
    }
    if let Some((a, b)) = swap {
        steps.swap(a, b);
        changed = true;
    }
    if let Some(i) = remove {
        steps.remove(i);
        st.step = None;
        changed = true;
    }
    egui::ComboBox::from_id_salt(("addstep", group)).selected_text("Add a step…").width(300.0).show_ui(ui, |ui| {
        for (uuid, name, n) in &ctx.steps {
            if ui.selectable_label(false, name).clicked() {
                let mut a = KeyAction::new(uuid, name, *n);
                if uuid.ends_with("hotkey") {
                    a.settings = serde_json::json!({"Coalesce": true, "Hotkeys": vec![actions::empty_hotkey(); 4]});
                }
                if uuid.ends_with("delay") {
                    a.settings = serde_json::json!({"delay": 1000});
                }
                steps.push(a);
                st.step = Some(base + steps.len() - 1);
                changed = true;
            }
        }
    });
    if let Some(sel) = st.step.filter(|s| *s >= base && *s < base + 1000).map(|s| s - base) {
        if let Some(s) = steps.get_mut(sel) {
            ui.add_space(6.0);
            egui::Frame::new().fill(hex(0x333333)).corner_radius(6.0).inner_margin(10.0).show(ui, |ui| {
                ui.label(RichText::new(format!("Step {}: {}", sel + 1, s.name)).font(crate::view::bold(13.0)));
                let before = serde_json::to_string(&*s).unwrap_or_default();
                settings_ui(ui, s, ctx, st, 1);
                if serde_json::to_string(&*s).unwrap_or_default() != before {
                    changed = true;
                }
            });
        }
    }
    if changed {
        actions::set_sub_actions(k, group, &steps);
    }
}

#[allow(dead_code)]
fn icon_name(i: Icon) -> &'static str {
    i.key()
}
