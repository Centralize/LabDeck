//! The action list on the right: built-in categories (function 0x1402221c0) and the
//! actions named in each built-in plugin's manifest.json, plus installed plugins.

use crate::icons::Icon;
use std::collections::HashMap;
use std::path::{Path, PathBuf};

#[derive(Clone, Debug)]
pub struct ActionDef {
    pub uuid: String,
    pub name: String,
    pub tooltip: String,
    pub icon: Icon,
    pub states: usize,
    /// false for third-party plugin actions: they can be placed but this build cannot run them.
    pub runnable: bool,
    /// Only listed for the Stream Deck Neo (manifest `Controllers: ["Neo"]`).
    pub neo_only: bool,
}

#[derive(Clone, Debug)]
pub struct Category {
    pub name: String,
    pub icon: Icon,
    pub actions: Vec<ActionDef>,
    pub plugin: bool,
}

fn a(uuid: &str, name: &str, tooltip: &str, icon: Icon, states: usize) -> ActionDef {
    ActionDef { uuid: uuid.into(), name: name.into(), tooltip: tooltip.into(), icon, states, runnable: true, neo_only: false }
}

/// Listed like the original but not built in LabDeck yet.
fn later(uuid: &str, name: &str, tooltip: &str, icon: Icon) -> ActionDef {
    ActionDef { runnable: false, ..a(uuid, name, &format!("{tooltip}\nNot available in this version yet."), icon, 1) }
}

fn neo(d: ActionDef) -> ActionDef {
    ActionDef { neo_only: true, ..d }
}

/// The five standard groups and their actions, in the original's order (group list in
/// ESDActionsTreeModel::GetAllActionsUUIDs, function 0x14021dfe0). Actions only for dials
/// (Dial Stack, Action Wheel, Action Trigger, Mouse Wheel) are left out — LabDeck drives keys.
/// Parent Folder is not in the list: new folders get it on their first key. Delay only appears
/// inside a Multi Action's steps.
pub fn builtin() -> Vec<Category> {
    use Icon::*;
    vec![
        Category {
            name: "Multi Action".into(),
            icon: MultiAction,
            plugin: false,
            actions: vec![
                a("com.elgato.streamdeck.multiactions.routine", "Multi Action", "Execute multiple actions", MultiAction, 1),
                a("com.elgato.streamdeck.multiactions.routine2", "Multi Action Switch", "Toggle between two Multi Action", MultiSwitch, 2),
                a("com.elgato.streamdeck.multiactions.random", "Random Action", "Execute a random action", Random, 1),
                later("com.elgato.streamdeck.keys.logic", "Key Logic", "Trigger actions with key clicks", Hotkey),
            ],
        },
        Category {
            name: "Navigation".into(),
            icon: Navigation,
            plugin: false,
            actions: vec![
                a("com.elgato.streamdeck.profile.openchild", "Create Folder", "Create a folder", Folder, 1),
                a("com.elgato.streamdeck.profile.rotate", "Switch Profile", "Switch to another preconfigured key profile", Profiles, 1),
                a("com.elgato.streamdeck.page.previous", "Previous Page", "Switch to previous Page", PagePrev, 1),
                a("com.elgato.streamdeck.page.next", "Next Page", "Switch to next Page", PageNext, 1),
                a("com.elgato.streamdeck.page.goto", "Go to Page", "Switch to specified page number", PageGoto, 1),
                a("com.elgato.streamdeck.page.indicator", "Page Indicator", "Displays the page number of the current profile", PageIndicator, 1),
                neo(a("com.elgato.streamdeck.system.pagination", "Page Indicator", "Display your current page selection", PageIndicator, 1)),
            ],
        },
        Category {
            name: "Soundboard".into(),
            icon: Speaker,
            plugin: false,
            actions: vec![
                a("com.elgato.streamdeck.soundboard.playaudio", "Play Audio", "Play a sound bite, audio effect or music clip", Speaker, 1),
                a("com.elgato.streamdeck.soundboard.stopaudioplay", "Stop Audio", "Stop all Soundboard playback", StopAudio, 1),
            ],
        },
        Category {
            name: "Stream Deck".into(),
            icon: Deck,
            plugin: false,
            actions: vec![
                a("com.elgato.streamdeck.system.timer", "Timer", "Timer for Stream Deck (acoustically and visually)", Timer, 1),
                a("com.elgato.streamdeck.system.keybrightness", "Brightness", "Adjust the brightness of your Stream Deck keys", Brightness, 1),
                a("com.elgato.streamdeck.system.sleep", "Sleep", "Put your Stream Deck to sleep", Sleep, 1),
                later("com.elgato.streamdeck.system.actionbar", "Action Bar", "Show/hide Action Bar", Deck),
                later("com.elgato.streamdeck.system.vsdtoggle", "Toggle Virtual Stream Deck", "Show/Hide Virtual Stream Deck", Deck),
            ],
        },
        Category {
            name: "System".into(),
            icon: System,
            plugin: false,
            actions: vec![
                a("com.elgato.streamdeck.system.website", "Website", "Open URL", Globe, 1),
                a("com.elgato.streamdeck.system.hotkeyswitch", "Hotkey Switch", "Toggle between two key commands", HotkeySwitch, 2),
                a("com.elgato.streamdeck.system.hotkey", "Hotkey", "Activate a key command", Hotkey, 1),
                // LabDeck's own addition (not in the original's list).
                a(crate::actions::TIMED_HOTKEY, "Timed Hotkey", "Press a key command after a wait, hold it, and repeat it on a timer", TimedHotkey, 1),
                a("com.elgato.streamdeck.system.open", "Open", "Open an app, file or folder", Open, 1),
                a("com.elgato.streamdeck.system.openapp", "Open Application", "Open an app", Apps, 1),
                a("com.elgato.streamdeck.system.close", "Close", "Close all instances of an app", Close, 1),
                a("com.elgato.streamdeck.system.text", "Text", "Type or paste a precomposed text", Text, 1),
                a("com.elgato.streamdeck.system.multimedia", "Multimedia", "Control your media playback", Media, 1),
                neo(a("com.elgato.streamdeck.system.digitaltime", "Digital Time", "Display a custom digital clock", Clock, 1)),
            ],
        },
        // The original also has an (empty here) "Custom" group; the list shows just these five
        // standard groups, then the plugins that are installed.
    ]
}

/// Where installed plugins live. On Linux: ~/.local/share/LabDeck/Plugins; a Windows
/// Stream Deck plugin folder can be copied there as-is.
pub fn plugins_dir() -> PathBuf {
    crate::profile::data_dir().join("Plugins")
}

/// Reads every `*.sdPlugin/manifest.json` so the person's plugins show in the list,
/// sorted by name like the original. Their actions are marked not runnable.
pub fn scan_plugins(dirs: &[PathBuf], names: &HashMap<String, String>, actions: &HashMap<String, String>) -> Vec<Category> {
    let mut out: Vec<Category> = Vec::new();
    for dir in dirs {
        let Ok(rd) = std::fs::read_dir(dir) else { continue };
        for e in rd.flatten() {
            let p = e.path();
            if !p.extension().is_some_and(|x| x == "sdPlugin") {
                continue;
            }
            // A readable manifest decides (plugins with no actions for the list are left out);
            // only encrypted ones are named from the profiles and the original's cache.
            let readable = manifest_json(&p).is_some();
            let cat = if readable { read_plugin(&p) } else { guess_plugin(&p, names, actions) };
            if let Some(c) = cat {
                if !out.iter().any(|o| o.name == c.name) {
                    out.push(c);
                }
            }
        }
    }
    out.sort_by_key(|c| c.name.to_lowercase());
    out
}

/// For plugins whose manifest is encrypted (store plugins): the name comes from `names`
/// (by plugin id) or the folder name, and the actions from the person's profiles.
fn guess_plugin(dir: &Path, names: &HashMap<String, String>, actions: &HashMap<String, String>) -> Option<Category> {
    let id = dir.file_stem()?.to_str()?.to_string();
    let name = names.get(&id).cloned().unwrap_or_else(|| {
        let last = id.rsplit('.').next().unwrap_or(&id);
        last.split(['-', '_'])
            .filter(|w| !w.is_empty())
            .map(|w| {
                let mut c = w.chars();
                c.next().map(|f| f.to_uppercase().collect::<String>() + c.as_str()).unwrap_or_default()
            })
            .collect::<Vec<_>>()
            .join(" ")
    });
    let prefix = format!("{id}.");
    let mut acts: Vec<ActionDef> = actions
        .iter()
        .filter(|(u, _)| u.starts_with(&prefix))
        .map(|(u, n)| ActionDef { uuid: u.clone(), name: n.clone(), tooltip: String::new(), icon: Icon::Plugin, states: 1, runnable: false, neo_only: false })
        .collect();
    acts.sort_by(|a, b| a.name.cmp(&b.name));
    if acts.is_empty() {
        acts.push(ActionDef { uuid: format!("{id}.action"), name: name.clone(), tooltip: String::new(), icon: Icon::Plugin, states: 1, runnable: false, neo_only: false });
    }
    Some(Category { name, icon: Icon::Plugin, actions: acts, plugin: true })
}

fn manifest_json(dir: &Path) -> Option<serde_json::Value> {
    let text = std::fs::read_to_string(dir.join("manifest.json")).ok()?;
    serde_json::from_str(text.trim_start_matches('\u{feff}')).ok()
}

fn read_plugin(dir: &Path) -> Option<Category> {
    let v = manifest_json(dir)?;
    let name = v
        .get("Category")
        .and_then(|x| x.as_str())
        .filter(|s| !s.trim().is_empty())
        .or_else(|| v.get("Name").and_then(|x| x.as_str()))?
        .trim()
        .to_string();
    let actions = v
        .get("Actions")?
        .as_array()?
        .iter()
        .filter(|x| x.get("VisibleInActionsList").and_then(|b| b.as_bool()) != Some(false))
        .filter_map(|x| {
            // Controllers defaults to Keypad; dial-only actions are left out.
            let ctrls: Vec<&str> = x
                .get("Controllers")
                .and_then(|c| c.as_array())
                .map(|c| c.iter().filter_map(|v| v.as_str()).collect())
                .unwrap_or_else(|| vec!["Keypad"]);
            let keypad = ctrls.contains(&"Keypad");
            let neo = ctrls.contains(&"Neo");
            if !keypad && !neo {
                return None;
            }
            Some(ActionDef {
                uuid: x.get("UUID")?.as_str()?.to_string(),
                name: x.get("Name")?.as_str()?.to_string(),
                tooltip: x.get("Tooltip").and_then(|t| t.as_str()).unwrap_or("").to_string(),
                icon: Icon::Plugin,
                states: x.get("States").and_then(|s| s.as_array()).map_or(1, |s| s.len().max(1)),
                runnable: false,
                neo_only: !keypad,
            })
        })
        .collect::<Vec<_>>();
    if actions.is_empty() {
        return None;
    }
    Some(Category { name, icon: Icon::Plugin, actions, plugin: true })
}

/// Built-in actions whose manifest sets `SupportedInMultiActions: false`.
const NOT_IN_MULTI: &[&str] = &[
    "com.elgato.streamdeck.keys.logic",
    "com.elgato.streamdeck.multiactions.routine",
    "com.elgato.streamdeck.multiactions.routine2",
    "com.elgato.streamdeck.multiactions.random",
    "com.elgato.streamdeck.page.previous",
    "com.elgato.streamdeck.page.next",
    "com.elgato.streamdeck.page.goto",
    "com.elgato.streamdeck.page.indicator",
    "com.elgato.streamdeck.profile.backtoparent",
    "com.elgato.streamdeck.profile.openchild",
    "com.elgato.streamdeck.system.hotkeyswitch",
    "com.elgato.streamdeck.system.timer",
];

pub fn allowed_in_multi_action(uuid: &str) -> bool {
    !NOT_IN_MULTI.contains(&uuid)
}

pub fn find<'a>(cats: &'a [Category], uuid: &str) -> Option<&'a ActionDef> {
    cats.iter().flat_map(|c| c.actions.iter()).find(|a| a.uuid == uuid)
}

/// Icon for an action id, also for ids that are not in the list (e.g. delays inside Multi Actions).
pub fn icon_for(cats: &[Category], uuid: &str) -> Icon {
    if let Some(a) = find(cats, uuid) {
        return a.icon;
    }
    match uuid {
        "com.elgato.streamdeck.multiactions.delay" => Icon::Timer,
        "com.elgato.streamdeck.profile.backtoparent" => Icon::Back,
        "com.elgato.streamdeck.system.digitaltime" => Icon::Timer,
        _ => Icon::Plugin,
    }
}
