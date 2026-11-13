//! The main window's state and behaviour: profiles, pages, folders, the device link,
//! timers and saving. Drawing lives in view.rs.

use crate::actions::{self, Effect};
use crate::catalog::{self, Category};
use crate::devices::{self, DeviceModel};
use crate::hid;
use crate::icons::{self, Canvas, Icon, Rgba};
use crate::inspector;
use crate::keyimage::{self, Fonts, ImageCache, Look};
use crate::keys;
use crate::plugins;
use crate::profile::{self, KeyAction, Profile};
use crate::theme::*;
use crate::tray;
use crate::favicon;
use std::collections::{HashMap, HashSet};
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{Receiver, Sender, channel};
use std::sync::Arc;
use std::time::Instant;

pub enum Drag {
    Action(String),
    Key((usize, usize)),
}

pub struct Timer {
    pub end: Instant,
    pub done_at: Option<Instant>,
    /// The key's own sound file (`lastUserFile`, as the original saves it); empty = bell.
    pub sound: String,
    /// Time left while paused (a short press pauses and resumes).
    pub paused: Option<std::time::Duration>,
}

impl Timer {
    pub fn left(&self) -> std::time::Duration {
        self.paused.unwrap_or_else(|| self.end.saturating_duration_since(Instant::now()))
    }
}

/// Hold time that makes a press "long" — the original starts a 500 ms timer at key down and
/// treats a key up before it fires as a short press (KA_OpenApp, functions 0x1402d0b20/0x1402d0b40).
pub const LONG_PRESS_MS: u64 = 500;

pub const DEFAULT_BRIGHTNESS: u8 = 70;
pub const DEFAULT_TIMER_SECONDS: u64 = 300;
pub const TIMER_FLASH_SECONDS: f32 = 3.0;

pub struct App {
    pub cats: Vec<Category>,
    pub profiles: Vec<Profile>,
    pub cur: usize,
    pub folder: Vec<String>,
    pub layout_model: &'static DeviceModel,
    pub connected: Option<(&'static DeviceModel, String)>,
    pub selected: Option<(usize, usize)>,
    pub drag: Option<Drag>,
    textures: HashMap<String, egui::TextureHandle>,
    icon_tex: HashMap<(Icon, [u8; 3]), egui::TextureHandle>,
    pub fonts: Fonts,
    cache: ImageCache,
    pub search: String,
    pub tab_plugins: bool,
    pub expanded: HashSet<String>,
    pub list_scroll: f32,
    pub split: f32,
    hid_tx: Option<Sender<hid::Cmd>>,
    hid_rx: Option<Receiver<hid::Event>>,
    pub brightness: u8,
    sleeping: bool,
    pub timers: HashMap<String, Timer>,
    runner: actions::Runner,
    fx_tx: Sender<Effect>,
    fx_rx: Receiver<Effect>,
    pub status: Option<(String, f64)>,
    pub dirty: bool,
    last_change: f64,
    screenshot: Option<PathBuf>,
    frames: u32,
    shot_done: Arc<AtomicBool>,
    shot_requested: bool,
    pub persist: bool,
    pub ins: inspector::State,
    pub device_menu: bool,
    /// A menu was opened this frame (so that click doesn't close it again).
    pub menu_opened: bool,
    pub profile_menu: bool,
    pub rename: Option<String>,
    pub import_path: Option<String>,
    pub pressed: HashSet<(usize, usize)>,
    pub clipboard: Option<KeyAction>,
    pub user_initial: String,
    /// Multi Actions pressed and waiting for key up, and running ones (cancel, done flags).
    multi_pending: HashSet<(usize, usize)>,
    multi_running: HashMap<String, (Arc<AtomicBool>, Arc<AtomicBool>)>,
    /// A Brighter/Darker key being held, and when it steps next.
    held_brightness: Option<((usize, usize), f64)>,
    /// A Timer key being held, and when the hold becomes a long press.
    timer_hold: Option<((usize, usize), f64)>,
    /// Hotkey or Multimedia keys held down from here (window clicks, or presses the device
    /// thread left to the window), with their hold ids in keys.rs.
    key_held: HashMap<(usize, usize), u64>,
    /// Time between repeats of a held Brighter/Darker key, read from the keyboard settings.
    repeat_every_ms: u64,
    /// The deck's Hotkey/Multimedia keys, which the device thread presses itself (hid::Direct).
    direct: hid::DirectKeys,
    /// The press being handled was already pressed by the device thread.
    press_direct: bool,
    /// The program last warned about for running as administrator.
    warned_admin: Option<u32>,
    /// Another LabDeck was already running when this one started: window only (no deck,
    /// plugins, tray or saving).
    pub second_copy: bool,
    /// Site icons fetched in the background: (key action id, picture file or error).
    icon_tx: Sender<(String, Result<PathBuf, String>)>,
    icon_rx: Receiver<(String, Result<PathBuf, String>)>,
    /// Look of each key as last sent to the hardware, so only changes are sent.
    device_sent: Vec<String>,
    tray: Option<tray::Tray>,
    tray_rx: Receiver<tray::TrayMsg>,
    quitting: bool,
    hidden: bool,
    /// Showing the "Do you want to quit?" question.
    pub confirm_quit: bool,
    pub quit_dont_ask_tick: bool,
    pub settings: Settings,
    /// The plugins in LabDeck's Plugins folder, running (not in --screenshot mode).
    pub plugins: Option<plugins::Host>,
    /// Titles, pictures and OK / alert flashes plugins set on keys, by key context (action id).
    pub plugin_looks: HashMap<String, PluginLook>,
    /// Plugin keys their plugin was told are shown (willAppear): context → (plugin, key, position).
    plugin_shown: HashMap<String, (String, KeyAction, (usize, usize))>,
}

/// What a plugin set on one key: titles and pictures (per state, or None for every state) and a
/// short OK (true) or alert (false) flash.
#[derive(Default)]
pub struct PluginLook {
    pub titles: HashMap<Option<u64>, String>,
    pub images: HashMap<Option<u64>, PathBuf>,
    pub flash: Option<(Instant, bool)>,
}

/// How long showOk / showAlert colour a key.
pub const PLUGIN_FLASH_SECONDS: f32 = 1.5;

/// The `-info` a plugin is started with.
fn plugin_info_json(p: &plugins::PluginInfo) -> serde_json::Value {
    let platform = match std::env::consts::OS {
        "macos" => "mac",
        "windows" => "windows",
        other => other,
    };
    serde_json::json!({
        "application": {"font": "", "language": "en", "platform": platform, "platformVersion": "", "version": env!("CARGO_PKG_VERSION")},
        "plugin": {"uuid": p.uuid, "version": p.version},
        "devicePixelRatio": 1,
        "colors": {},
        "devices": [],
    })
}

#[derive(serde::Serialize, serde::Deserialize, Default)]
pub struct Settings {
    /// The quit question's "Don't ask me again" (function 0x14018bb80 stores the same choice).
    #[serde(default)]
    pub dont_ask_quit: bool,
    /// The profile that was selected last, opened again at start.
    #[serde(default)]
    pub last_profile: Option<String>,
}

impl Settings {
    fn load() -> Settings {
        std::fs::read_to_string(profile::data_dir().join("settings.json"))
            .ok()
            .and_then(|t| serde_json::from_str(&t).ok())
            .unwrap_or_default()
    }
}

/// How often live keys (clock, timer) and the hardware are brought in sync.
pub const SYNC_INTERVAL_MS: u64 = 100;

/// A key copied as text by `App::copy_key` (the profile format's JSON for one key). Text that
/// isn't a key, with no action id, gives None.
pub fn key_from_text(text: &str) -> Option<KeyAction> {
    let v: serde_json::Value = serde_json::from_str(text.trim()).ok()?;
    if !v.get("UUID").and_then(|u| u.as_str()).is_some_and(|u| u.contains('.')) {
        return None;
    }
    serde_json::from_value(v).ok()
}

/// The picture a key's action gives it: Multimedia keys show their chosen function (the
/// original's table 0x1402ca990 gives each choice its own picture); others their action's icon.
pub fn key_icon(cats: &[Category], k: &KeyAction) -> Icon {
    if k.uuid == "com.elgato.streamdeck.system.multimedia" {
        actions::media_icon(actions::media_index(&k.settings))
    } else {
        catalog::icon_for(cats, &k.uuid)
    }
}

/// Take the lock that says "this LabDeck drives the deck" (a `LabDeck.lock` file in LabDeck's own
/// folder, locked for as long as this copy runs; the system lets go of it when the process ends,
/// even after a crash). False when another copy already holds it.
fn instance_lock() -> bool {
    static LOCK: std::sync::OnceLock<std::fs::File> = std::sync::OnceLock::new();
    if LOCK.get().is_some() {
        return true;
    }
    let dir = profile::data_dir();
    let _ = std::fs::create_dir_all(&dir);
    match lock_file(&dir.join("LabDeck.lock")) {
        Ok(f) => {
            let _ = LOCK.set(f);
            true
        }
        Err(taken) => !taken, // can't tell: behave as the only copy
    }
}

/// Lock a file for this process: Ok(the open file, which holds the lock), Err(true) when another
/// holder has it, Err(false) when locking isn't possible here.
pub fn lock_file(path: &std::path::Path) -> Result<std::fs::File, bool> {
    let f = std::fs::OpenOptions::new().create(true).truncate(false).write(true).open(path).map_err(|_| false)?;
    match f.try_lock() {
        Ok(()) => Ok(f),
        Err(std::fs::TryLockError::WouldBlock) => Err(true),
        Err(_) => Err(false),
    }
}

/// What a second copy says while it runs.
pub const SECOND_COPY_NOTE: &str =
    "LabDeck is already running (look in the tray). This window doesn't use the Stream Deck, and changes made here aren't saved.";

impl App {
    pub fn new(cc: &eframe::CreationContext<'_>, screenshot: Option<PathBuf>, start_hidden: bool) -> Self {
        crate::view::setup_fonts(&cc.egui_ctx);
        crate::view::setup_style(&cc.egui_ctx);
        // In --screenshot mode nothing is written and no device is opened. Only one LabDeck at a
        // time drives the deck: a second copy (started while one runs) opens as a window only,
        // the same way, so it can't draw over the running copy's keys or act on its presses.
        let second_copy = screenshot.is_none() && !instance_lock();
        let persist = screenshot.is_none() && !second_copy;
        let root = profile::profiles_root();
        let model = devices::default_model();
        // LabDeck's own profiles, then those in its Import folder it hasn't saved a copy of.
        // Nothing is read from Elgato's Stream Deck app: LabDeck works on its own.
        let mut profiles = profile::load_all(&root);
        for r in profile::import_roots() {
            for p in profile::load_all(&r) {
                let known = devices::by_model(&p.model).is_some();
                match profiles.iter_mut().find(|q| q.uuid.eq_ignore_ascii_case(&p.uuid)) {
                    // LabDeck's copy wins, but keeps the imported one's default-profile marker.
                    Some(q) => {
                        if q.app.is_none() {
                            q.app = p.app.clone();
                        }
                    }
                    None if known => profiles.push(p),
                    None => {}
                }
            }
        }
        if profiles.iter().any(|p| !p.sample) {
            profiles.retain(|p| !p.sample);
        }
        // An older build's first-run sample gives way to the current one. Its folder is left
        // on disk (only hidden from the list), in case it was edited.
        profiles.retain(|p| !p.sample || p.sample_rev >= profile::SAMPLE_REV);
        if profiles.is_empty() {
            let mut p = profile::sample(model.model);
            if persist {
                let _ = p.save(&root);
            }
            profiles.push(p);
        }
        profiles.sort_by_key(|p| p.name.to_lowercase());
        // Open the device's default profile like the original, else the most recently saved one.
        // The last selected profile, else the device's default profile like the original,
        // else the most recently saved one.
        let last = Settings::load().last_profile;
        let cur = last
            .and_then(|u| profiles.iter().position(|p| p.uuid.eq_ignore_ascii_case(&u)))
            .unwrap_or_else(|| {
                profiles
                    .iter()
                    .enumerate()
                    .max_by_key(|(_, p)| (p.app.as_deref() == Some("*"), p.mtime))
                    .map_or(0, |(i, _)| i)
            });

        // Plugins in LabDeck's own Plugins folder: readable manifests, else the names saved on
        // keys in the profiles.
        let mut plugin_names = HashMap::new();
        let mut action_names = HashMap::new();
        for p in &profiles {
            for pg in p.all.values() {
                for k in pg.keys.values() {
                    if let Some(pl) = k.extra.get("Plugin") {
                        if let (Some(u), Some(n)) = (pl.get("UUID").and_then(|x| x.as_str()), pl.get("Name").and_then(|x| x.as_str())) {
                            plugin_names.insert(u.to_string(), n.to_string());
                        }
                    }
                    action_names.insert(k.uuid.clone(), k.name.clone());
                }
            }
        }
        let mut cats = catalog::builtin();
        let plugin_dirs = vec![catalog::plugins_dir()];
        let plugins = catalog::scan_plugins(&plugin_dirs, &plugin_names, &action_names);
        // Only plugins that are actually installed are listed after the standard groups.
        cats.extend(plugins);

        let (fx_tx, fx_rx) = channel();
        let (tray_tx, tray_rx) = channel();
        let (icon_tx, icon_rx) = channel();
        let mut app = App {
            cats,
            profiles,
            cur,
            folder: Vec::new(),
            layout_model: model,
            connected: None,
            selected: None,
            drag: None,
            textures: HashMap::new(),
            icon_tex: HashMap::new(),
            fonts: Fonts::load(),
            cache: ImageCache::new(),
            search: String::new(),
            tab_plugins: false,
            expanded: ["System", "Autoclicker"].iter().map(|s| s.to_string()).collect(),
            list_scroll: 0.0,
            split: SPLIT,
            hid_tx: None,
            hid_rx: None,
            brightness: DEFAULT_BRIGHTNESS,
            sleeping: false,
            timers: HashMap::new(),
            runner: actions::Runner::new(),
            fx_tx,
            fx_rx,
            status: None,
            dirty: false,
            last_change: 0.0,
            screenshot,
            frames: 0,
            shot_done: Arc::new(AtomicBool::new(false)),
            shot_requested: false,
            persist,
            ins: inspector::State::default(),
            device_menu: false,
            menu_opened: false,
            profile_menu: false,
            rename: None,
            import_path: None,
            pressed: HashSet::new(),
            clipboard: None,
            user_initial: std::env::var("USER")
                .or_else(|_| std::env::var("USERNAME"))
                .ok()
                .and_then(|u| u.chars().next())
                .map(|c| c.to_uppercase().to_string())
                .unwrap_or_else(|| "U".into()),
            multi_pending: HashSet::new(),
            multi_running: HashMap::new(),
            held_brightness: None,
            timer_hold: None,
            key_held: HashMap::new(),
            repeat_every_ms: actions::REPEAT_EVERY_FALLBACK_MS,
            direct: Default::default(),
            press_direct: false,
            warned_admin: None,
            second_copy,
            icon_tx,
            icon_rx,
            device_sent: Vec::new(),
            tray: None,
            tray_rx,
            quitting: false,
            hidden: start_hidden,
            confirm_quit: false,
            quit_dont_ask_tick: false,
            settings: Settings::load(),
            plugins: None,
            plugin_looks: HashMap::new(),
            plugin_shown: HashMap::new(),
        };
        // Start the plugins in LabDeck's own Plugins folder (never in --screenshot mode).
        if app.persist {
            let found = plugins::discover(&catalog::plugins_dir());
            if !found.is_empty() {
                let ctx = cc.egui_ctx.clone();
                app.plugins = plugins::Host::start(found, plugin_info_json, move || ctx.request_repaint());
            }
        }
        if app.persist {
            app.tray = tray::create(cc.egui_ctx.clone(), tray_tx);
            // Keep waking up while the window is hidden, so live keys and the device stay current.
            let ctx = cc.egui_ctx.clone();
            std::thread::Builder::new()
                .name("labdeck-tick".into())
                .spawn(move || loop {
                    std::thread::sleep(std::time::Duration::from_millis(SYNC_INTERVAL_MS));
                    ctx.request_repaint();
                })
                .ok();
        }
        // For checks: LABDECK_MENU=profile|device opens that menu.
        match std::env::var("LABDECK_MENU").as_deref() {
            Ok("profile") => app.profile_menu = true,
            Ok("device") => app.device_menu = true,
            _ => {}
        }
        // For checks: LABDECK_SELECT="col,row" opens the key editor on that key.
        if let Ok(sel) = std::env::var("LABDECK_SELECT") {
            let mut it = sel.split(',').filter_map(|v| v.trim().parse::<usize>().ok());
            if let (Some(c), Some(r)) = (it.next(), it.next()) {
                app.selected = Some((c, r));
            }
        }
        if let Some(m) = devices::by_model(&app.profiles[app.cur].model) {
            app.layout_model = m;
        }
        if app.persist {
            let (cmd_tx, cmd_rx) = channel();
            let (ev_tx, ev_rx) = channel();
            let ctx = cc.egui_ctx.clone();
            hid::spawn(cmd_rx, ev_tx, app.direct.clone(), move || ctx.request_repaint());
            app.hid_tx = Some(cmd_tx);
            app.hid_rx = Some(ev_rx);
        }
        app
    }

    pub fn model(&self) -> &'static DeviceModel {
        self.connected.as_ref().map(|c| c.0).unwrap_or(self.layout_model)
    }

    pub fn profile(&self) -> &Profile {
        &self.profiles[self.cur]
    }

    pub fn page_id(&self) -> String {
        let p = self.profile();
        self.folder.last().cloned().unwrap_or_else(|| p.pages.get(p.current).cloned().unwrap_or_default())
    }

    pub fn key_at(&self, pos: (usize, usize)) -> Option<&KeyAction> {
        self.profile().page(&self.page_id()).and_then(|pg| pg.keys.get(&pos))
    }

    pub fn key_at_mut(&mut self, pos: (usize, usize)) -> Option<&mut KeyAction> {
        let id = self.page_id();
        self.profiles[self.cur].page_mut(&id).keys.get_mut(&pos)
    }

    pub fn changed(&mut self, now: f64) {
        self.dirty = true;
        self.last_change = now;
    }

    pub fn say(&mut self, msg: impl Into<String>, now: f64) {
        self.status = Some((msg.into(), now));
    }

    // ------------------------------------------------------------------ keys

    fn live_title(&self, k: &KeyAction) -> Option<String> {
        match k.uuid.as_str() {
            "com.elgato.streamdeck.system.digitaltime" => {
                let f = k.setting_str("format");
                let f = if f.is_empty() { "HH:mm".to_string() } else { f };
                let f = f.replace("HH", "%H").replace("hh", "%I").replace("mm", "%M").replace("ss", "%S").replace("AP", "%p");
                Some(chrono::Local::now().format(&f).to_string())
            }
            // The Clocks plugin is common enough to show the time natively.
            "com.elgato.clocks.action" | "com.elgato.worldclock.action" => {
                let h12 = k.settings.get("hour12").and_then(|v| v.as_bool()).unwrap_or(false);
                Some(chrono::Local::now().format(if h12 { "%I:%M" } else { "%H:%M" }).to_string())
            }
            "com.elgato.streamdeck.page.indicator" => Some(format!("{}", self.profile().current + 1)),
            "com.elgato.streamdeck.page.goto" => {
                let n = k.settings.get("PageIndex").or(k.settings.get("page")).and_then(|v| v.as_u64()).unwrap_or(1);
                Some(n.to_string())
            }
            "com.elgato.streamdeck.system.timer" => {
                let t = self.timers.get(&k.action_id)?;
                let left = t.left().as_secs();
                Some(format!("{:02}:{:02}", left / 60, left % 60))
            }
            _ => None,
        }
    }

    pub fn look_of(&self, k: &KeyAction) -> Look {
        let icon = key_icon(&self.cats, k);
        let dir = self.profile().page(&self.page_id()).and_then(|p| p.dir.clone());
        let live = self.live_title(k);
        let timer_live = k.uuid == "com.elgato.streamdeck.system.timer" && live.is_some();
        let mut l = keyimage::look(k, icon, dir.as_deref(), if timer_live { None } else { live.clone() });
        if timer_live {
            l.title = live.unwrap_or_default();
            l.align = keyimage::Align::Bottom;
            if self.timers.get(&k.action_id).is_some_and(|t| t.done_at.is_some()) && (Instant::now().elapsed().as_millis() / 400) % 2 == 0 {
                l.background = Rgba::hex("#c0392b");
            }
        }
        // What the key's plugin set: its title and picture (for this state, else for every
        // state) and a short green (OK) or red (alert) flash.
        if let Some(o) = self.plugin_looks.get(&k.action_id) {
            let state = Some(k.state as u64);
            if let Some(t) = o.titles.get(&state).or(o.titles.get(&None)) {
                l.title = t.clone();
            }
            if let Some(p) = o.images.get(&state).or(o.images.get(&None)) {
                l.image = Some(p.clone());
                l.icon = None;
            }
            if let Some((until, ok)) = o.flash {
                if Instant::now() < until {
                    l.background = Rgba::hex(if ok { "#2e7d32" } else { "#c0392b" });
                }
            }
        }
        l
    }

    /// The running plugin a key's action belongs to.
    pub fn plugin_for(&self, action: &str) -> Option<String> {
        self.plugins.as_ref()?.running_plugin_of(action).map(str::to_string)
    }

    /// Whether an action may be a Multi Action step: built-in actions by their list, plugin
    /// actions by their manifest's `SupportedInMultiActions`.
    pub fn allowed_in_multi(&self, action: &str) -> bool {
        catalog::allowed_in_multi_action(action) && self.plugins.as_ref().is_none_or(|h| h.allows_in_multi(action))
    }

    /// The connected device as plugins name it: its id (serial number) and `deviceInfo`.
    fn plugin_device(&self) -> Option<(String, serde_json::Value)> {
        let (m, serial) = self.connected.as_ref()?;
        let id = if serial.is_empty() { m.name.to_string() } else { serial.clone() };
        let info = serde_json::json!({"name": m.name, "type": plugins::device_type(m.name), "size": {"columns": m.cols, "rows": m.rows}});
        Some((id, info))
    }

    fn device_id(&self) -> String {
        self.plugin_device().map(|d| d.0).unwrap_or_default()
    }

    fn plugin_send(&self, plugin: &str, msg: serde_json::Value) {
        if let Some(h) = &self.plugins {
            h.send(plugin, &msg);
        }
    }

    /// A plugin key's settings were changed in the key editor: tell its plugin.
    pub fn plugin_settings_changed(&self, k: &KeyAction, pos: (usize, usize)) {
        if let Some(p) = self.plugin_for(&k.uuid) {
            self.plugin_send(&p, plugins::key_event("didReceiveSettings", k, pos, &self.device_id(), false));
        }
    }

    /// The key (anywhere in the current profile) a plugin means by `context`.
    fn plugin_key_mut(&mut self, context: &str) -> Option<(&mut KeyAction, (usize, usize))> {
        self.profiles[self.cur]
            .all
            .values_mut()
            .flat_map(|pg| pg.keys.iter_mut())
            .find(|(_, k)| k.action_id == context)
            .map(|(pos, k)| (k, *pos))
    }

    /// Plugin connections, messages and which of their keys are shown.
    fn poll_plugins(&mut self, now: f64) {
        let msgs = match &mut self.plugins {
            Some(h) => h.drain(),
            None => return,
        };
        for m in msgs {
            match m {
                plugins::FromPlugin::Registered(p) => {
                    if let Some((id, info)) = self.plugin_device() {
                        self.plugin_send(&p, serde_json::json!({"event": "deviceDidConnect", "device": id, "deviceInfo": info}));
                    }
                    // Its keys are announced again below.
                    self.plugin_shown.retain(|_, v| v.0 != p);
                }
                plugins::FromPlugin::Gone(p, why) => {
                    self.plugin_shown.retain(|_, v| v.0 != p);
                    let name = self.plugins.as_ref().and_then(|h| h.info(&p)).map(|i| i.name.clone()).unwrap_or(p.clone());
                    self.say(format!("Plugin “{name}” isn't running: {why}"), now);
                }
                plugins::FromPlugin::Message(p, v) => self.plugin_command(&p, v, now),
            }
        }
        // willAppear / willDisappear for the plugin keys on the shown page.
        let Some(host) = &self.plugins else { return };
        let page = self.profile().page(&self.page_id()).cloned().unwrap_or_default();
        let mut shown = HashMap::new();
        for (pos, k) in &page.keys {
            if let Some(p) = host.running_plugin_of(&k.uuid) {
                shown.insert(k.action_id.clone(), (p.to_string(), k.clone(), *pos));
            }
        }
        let dev = self.device_id();
        for (ctx, (p, k, pos)) in &self.plugin_shown {
            if !shown.get(ctx).is_some_and(|(_, k2, pos2)| k2.uuid == k.uuid && pos2 == pos) {
                host.send(p, &plugins::key_event("willDisappear", k, *pos, &dev, false));
            }
        }
        for (ctx, (p, k, pos)) in &shown {
            if !self.plugin_shown.get(ctx).is_some_and(|(_, k2, pos2)| k2.uuid == k.uuid && pos2 == pos) {
                host.send(p, &plugins::key_event("willAppear", k, *pos, &dev, false));
            }
        }
        self.plugin_shown = shown;
    }

    /// One command from a plugin (the standard plugin protocol's names).
    fn plugin_command(&mut self, plugin: &str, v: serde_json::Value, now: f64) {
        let event = v.get("event").and_then(|e| e.as_str()).unwrap_or_default().to_string();
        let context = v.get("context").and_then(|c| c.as_str()).unwrap_or_default().to_string();
        let payload = v.get("payload").cloned().unwrap_or(serde_json::Value::Null);
        let state = payload.get("state").and_then(|s| s.as_u64());
        let dir = self.plugins.as_ref().and_then(|h| h.info(plugin)).map(|i| i.dir.clone()).unwrap_or_default();
        match event.as_str() {
            "setTitle" => {
                let o = self.plugin_looks.entry(context).or_default();
                match payload.get("title").and_then(|t| t.as_str()) {
                    Some(t) => o.titles.insert(state, t.to_string()),
                    None => o.titles.remove(&state),
                };
            }
            "setImage" => match payload.get("image").and_then(|i| i.as_str()).filter(|i| !i.is_empty()) {
                Some(img) => match plugins::image_file(img, &dir) {
                    Ok(path) => {
                        self.plugin_looks.entry(context).or_default().images.insert(state, path);
                    }
                    Err(e) => plugins::log(plugin, &format!("setImage: {e}")),
                },
                None => {
                    self.plugin_looks.entry(context).or_default().images.remove(&state);
                }
            },
            "showOk" | "showAlert" => {
                let until = Instant::now() + std::time::Duration::from_secs_f32(PLUGIN_FLASH_SECONDS);
                self.plugin_looks.entry(context).or_default().flash = Some((until, event == "showOk"));
            }
            "setState" => {
                if let Some(s) = state {
                    if let Some((k, _)) = self.plugin_key_mut(&context) {
                        while k.states.len() <= s as usize {
                            k.states.push(profile::StateDef { show_title: true, ..Default::default() });
                        }
                        k.state = s as usize;
                        self.changed(now);
                    }
                }
            }
            "setSettings" => {
                if let Some((k, _)) = self.plugin_key_mut(&context) {
                    k.settings = if payload.is_object() { payload } else { serde_json::json!({}) };
                    self.changed(now);
                }
            }
            "getSettings" => {
                let dev = self.device_id();
                if let Some((k, pos)) = self.plugin_key_mut(&context) {
                    let msg = plugins::key_event("didReceiveSettings", k, pos, &dev, false);
                    self.plugin_send(plugin, msg);
                }
            }
            "setGlobalSettings" => plugins::save_globals(plugin, &payload),
            "getGlobalSettings" => {
                let msg = serde_json::json!({"event": "didReceiveGlobalSettings", "payload": {"settings": plugins::load_globals(plugin)}});
                self.plugin_send(plugin, msg);
            }
            "openUrl" => {
                if let Some(url) = payload.get("url").and_then(|u| u.as_str()) {
                    let _ = open::that(url);
                }
            }
            "logMessage" => plugins::log(plugin, payload.get("message").and_then(|m| m.as_str()).unwrap_or_default()),
            "switchToProfile" => {
                let name = payload.get("profile").and_then(|p| p.as_str()).unwrap_or_default();
                if let Some(i) = self.profiles.iter().position(|p| p.name == name) {
                    self.switch_profile(i);
                    if let Some(page) = payload.get("page").and_then(|p| p.as_u64()) {
                        if (page as usize) < self.profile().pages.len() {
                            self.goto_page(page as usize);
                        }
                    }
                }
            }
            // Dial, touch-strip and property-inspector messages have nothing to act on here.
            _ => {}
        }
    }

    fn sig(l: &Look, px: u32) -> String {
        format!(
            "{px}|{:?}|{:?}|{:?}|{:?}|{}|{:?}|{}|{}|{}|{}|{}",
            l.icon,
            (l.icon_color.0, l.icon_color.1, l.icon_color.2),
            (l.background.0, l.background.1, l.background.2),
            l.image,
            l.title,
            (l.title_color.0, l.title_color.1, l.title_color.2),
            l.align as u8,
            l.font_size,
            l.show_title,
            l.big_center,
            l.boxed
        )
    }

    pub fn key_texture(&mut self, ctx: &egui::Context, k: &KeyAction, px: u32) -> egui::TextureId {
        let l = self.look_of(k);
        let sig = Self::sig(&l, px);
        if let Some(t) = self.textures.get(&sig) {
            return t.id();
        }
        if self.textures.len() > 400 {
            self.textures.clear();
        }
        let cv = keyimage::render(&l, px, &self.fonts, &mut self.cache);
        let img = egui::ColorImage::from_rgba_unmultiplied([px as usize, px as usize], &cv.to_rgba8());
        let t = ctx.load_texture(&sig, img, egui::TextureOptions::LINEAR);
        let id = t.id();
        self.textures.insert(sig, t);
        id
    }

    pub fn icon_texture(&mut self, ctx: &egui::Context, icon: Icon, color: [u8; 3]) -> egui::TextureId {
        if let Some(t) = self.icon_tex.get(&(icon, color)) {
            return t.id();
        }
        let px = 64;
        let mut cv = Canvas::new(px, px);
        let c = Rgba(color[0] as f32 / 255.0, color[1] as f32 / 255.0, color[2] as f32 / 255.0, 1.0);
        icons::draw_icon(&mut cv, icon, 2.0, 2.0, px as f32 - 4.0, c, 0.075);
        let img = egui::ColorImage::from_rgba_unmultiplied([px as usize, px as usize], &cv.to_rgba8());
        let t = ctx.load_texture(format!("icon-{icon:?}-{color:?}"), img, egui::TextureOptions::LINEAR);
        let id = t.id();
        self.icon_tex.insert((icon, color), t);
        id
    }

    /// Forget what was sent, so every key picture and the brightness go to the deck again.
    pub fn redraw_device(&mut self) {
        self.device_sent.clear();
        if let Some(tx) = &self.hid_tx {
            let _ = tx.send(hid::Cmd::Refresh);
            let _ = tx.send(hid::Cmd::Brightness(if self.sleeping { 0 } else { self.brightness }));
        }
    }

    /// Send every key picture of the shown page to the hardware (the HID thread skips unchanged ones).
    fn sync_device(&mut self) {
        let Some(tx) = self.hid_tx.clone() else { return };
        let Some((m, _)) = self.connected else { return };
        if m.protocol == devices::Protocol::NoScreen {
            return;
        }
        let n = m.cols * m.rows;
        if self.device_sent.len() != n {
            self.device_sent = vec![String::new(); n];
        }
        let page = self.profile().page(&self.page_id()).cloned().unwrap_or_default();
        for r in 0..m.rows {
            for c in 0..m.cols {
                let i = r * m.cols + c;
                // Only keys whose look changed are drawn and sent again.
                let look = page.keys.get(&(c, r)).map(|k| self.look_of(k));
                let sig = look.as_ref().map_or_else(|| "empty".to_string(), |l| Self::sig(l, m.key_px));
                if self.device_sent[i] == sig {
                    continue;
                }
                let rgb = match &look {
                    Some(l) => keyimage::render(l, m.key_px, &self.fonts, &mut self.cache).to_rgb8_on([0.0; 3]),
                    None => vec![0u8; (m.key_px * m.key_px * 3) as usize],
                };
                let _ = tx.send(hid::Cmd::Image(i, rgb));
                self.device_sent[i] = sig;
            }
        }
    }

    // ------------------------------------------------------------- behaviour

    /// Key down. Multi Actions wait for the key to come up (function 0x1402c9f30 starts them
    /// in ProcessKeyUp); pressing one that is still running cancels it (0x1402c9c40).
    /// Fetch a website's icon for a Website key in the background (see favicon.rs).
    pub fn fetch_site_icon(&mut self, action_id: String, url: String, now: f64) {
        self.say(format!("Getting the icon of {}…", favicon::normalize(&url)), now);
        let tx = self.icon_tx.clone();
        std::thread::spawn(move || {
            let res = favicon::fetch(&url).and_then(|img| favicon::make_key_picture(&img, &url));
            let _ = tx.send((action_id, res));
        });
    }

    fn apply_site_icons(&mut self, now: f64) {
        while let Ok((id, res)) = self.icon_rx.try_recv() {
            match res {
                Ok(path) => {
                    let mut found = false;
                    for pr in &mut self.profiles {
                        for pg in pr.all.values_mut() {
                            for k in pg.keys.values_mut().filter(|k| k.action_id == id) {
                                let st = k.cur_mut();
                                st.image = Some(path.to_string_lossy().to_string());
                                st.icon = Some(String::new());
                                found = true;
                            }
                        }
                    }
                    if found {
                        self.changed(now);
                        self.say("Icon added from the website", now);
                    }
                }
                Err(e) => self.say(format!("Couldn't get the website's icon: {e}"), now),
            }
        }
    }

    /// Short press on a Timer: start it, pause it, resume it; once it has rung, start over.
    fn timer_short_press(&mut self, k: &KeyAction) {
        match self.timers.get_mut(&k.action_id) {
            Some(t) if t.done_at.is_some() => {
                self.timers.remove(&k.action_id);
            }
            Some(t) => match t.paused.take() {
                Some(left) => t.end = Instant::now() + left,
                None => t.paused = Some(t.end.saturating_duration_since(Instant::now())),
            },
            None => {
                let secs = k.settings.get("duration").and_then(|v| v.as_u64()).unwrap_or(DEFAULT_TIMER_SECONDS);
                self.timers.insert(
                    k.action_id.clone(),
                    Timer {
                        end: Instant::now() + std::time::Duration::from_secs(secs),
                        done_at: None,
                        sound: k.setting_str("lastUserFile"),
                        paused: None,
                    },
                );
            }
        }
    }

    /// Long press on a Timer: stop it and go back to its preset time.
    fn timer_long_press(&mut self, pos: (usize, usize), now: f64) {
        if let Some(k) = self.key_at(pos).cloned() {
            if self.timers.remove(&k.action_id).is_some() {
                self.runner.stop_audio();
            }
            let secs = k.settings.get("duration").and_then(|v| v.as_u64()).unwrap_or(DEFAULT_TIMER_SECONDS);
            self.say(format!("Timer reset to {:02}:{:02}", secs / 60, secs % 60), now);
        }
    }

    pub fn press(&mut self, pos: (usize, usize), now: f64) {
        let Some(k) = self.key_at(pos).cloned() else { return };
        // A plugin's key: its plugin gets keyDown.
        if let Some(p) = self.plugin_for(&k.uuid) {
            self.plugin_send(&p, plugins::key_event("keyDown", &k, pos, &self.device_id(), false));
            return;
        }
        // Timers decide on key up (short) or after LONG_PRESS_MS held (long).
        if k.uuid == "com.elgato.streamdeck.system.timer" {
            self.timer_hold = Some((pos, now + LONG_PRESS_MS as f64 / 1000.0));
            return;
        }
        // Timed Hotkey: runs on its own thread; pressing it again while it runs stops it.
        if k.uuid == actions::TIMED_HOTKEY {
            if let Some((cancel, done)) = self.multi_running.get(&k.action_id) {
                if !done.load(Ordering::SeqCst) {
                    cancel.store(true, Ordering::SeqCst);
                    self.say("Timed hotkey stopped", now);
                    return;
                }
            }
            let cancel = Arc::new(AtomicBool::new(false));
            let done = Arc::new(AtomicBool::new(false));
            self.multi_running.insert(k.action_id.clone(), (cancel.clone(), done.clone()));
            let settings = k.settings.clone();
            std::thread::spawn(move || {
                let mut r = actions::Runner::new();
                r.cancel = Some(cancel);
                r.timed_hotkey(&settings);
                done.store(true, Ordering::SeqCst);
            });
            return;
        }
        if k.uuid.starts_with("com.elgato.streamdeck.multiactions.") {
            if let Some((cancel, done)) = self.multi_running.get(&k.action_id) {
                if !done.load(Ordering::SeqCst) {
                    cancel.store(true, Ordering::SeqCst);
                    return;
                }
            }
            self.multi_pending.insert(pos);
            return;
        }
        // Held keys repeat like a held keyboard key, at this computer's keyboard repeat
        // settings (repeat timers 0x1402c6d90 / 0x1402c1230).
        let (first_repeat, every) = actions::keyboard_repeat();
        // Hotkey / Hotkey Switch / Multimedia: keys go down now and stay held until the key
        // comes up, repeating while held (KA_HotKey 0x1402c1540, KA_Multimedia 0x1402d20c0).
        // Deck presses are usually pressed already by the device thread (press_direct).
        let held = actions::held_keys(&k);
        if !matches!(held, actions::HeldKeys::NotHeld) {
            if !self.press_direct {
                if let Some(id) = self.key_held.remove(&pos) {
                    keys::let_go(id);
                }
                match held {
                    actions::HeldKeys::Keys(c) => {
                        let id = keys::new_id();
                        keys::hold(id, c, Some((first_repeat, every)));
                        self.key_held.insert(pos, id);
                    }
                    actions::HeldKeys::Unsendable(msg) => self.say(msg, now),
                    _ => {}
                }
            }
            self.warn_if_blocked(now);
            self.flip_state(pos, &k, now);
            return;
        }
        let fx = self.runner.press(&k);
        for e in fx {
            self.apply(e, Some(&k), now);
        }
        // Brighter/Darker keep stepping while the key is held (KA_KeyBrightness 0x1402c6f20).
        if k.uuid == "com.elgato.streamdeck.system.keybrightness" && k.settings.get("actionIdx").and_then(|v| v.as_u64()).unwrap_or(0) < 2 {
            self.repeat_every_ms = every;
            self.held_brightness = Some((pos, now + first_repeat as f64 / 1000.0));
        }
        self.flip_state(pos, &k, now);
    }

    /// Key up: stops a held brightness key and starts a waiting Multi Action.
    pub fn release(&mut self, pos: (usize, usize), now: f64) {
        // A plugin's key: keyUp, then a key with several states moves to the next one (unless
        // its manifest sets DisableAutomaticStates).
        if let Some(k) = self.key_at(pos).cloned() {
            if let Some(p) = self.plugin_for(&k.uuid) {
                self.plugin_send(&p, plugins::key_event("keyUp", &k, pos, &self.device_id(), false));
                let manual = self.plugins.as_ref().and_then(|h| h.info(&p)).is_some_and(|i| i.manual_states.contains(&k.uuid));
                if k.states.len() > 1 && !manual {
                    self.flip_state(pos, &k, now);
                }
                return;
            }
        }
        if self.held_brightness.is_some_and(|h| h.0 == pos) {
            self.held_brightness = None;
        }
        if let Some(id) = self.key_held.remove(&pos) {
            keys::let_go(id);
        }
        // Let go before the long-press time: a short press.
        if self.timer_hold.is_some_and(|h| h.0 == pos) {
            self.timer_hold = None;
            if let Some(k) = self.key_at(pos).cloned() {
                self.timer_short_press(&k);
            }
            return;
        }
        if !self.multi_pending.remove(&pos) {
            return;
        }
        let Some(k) = self.key_at(pos).cloned() else { return };
        // Multi Actions may wait between steps, so they run on their own thread.
        let cancel = Arc::new(AtomicBool::new(false));
        let done = Arc::new(AtomicBool::new(false));
        self.multi_running.insert(k.action_id.clone(), (cancel.clone(), done.clone()));
        let tx = self.fx_tx.clone();
        let k2 = k.clone();
        std::thread::spawn(move || {
            let mut r = actions::Runner::new();
            r.cancel = Some(cancel);
            for e in r.press(&k2) {
                let _ = tx.send(e);
            }
            done.store(true, Ordering::SeqCst);
        });
        self.flip_state(pos, &k, now);
    }

    /// Two-state actions flip after each use.
    fn flip_state(&mut self, pos: (usize, usize), k: &KeyAction, now: f64) {
        let two_state = matches!(k.uuid.as_str(), "com.elgato.streamdeck.system.hotkeyswitch" | "com.elgato.streamdeck.multiactions.routine2");
        if k.states.len() > 1 || two_state {
            if let Some(km) = self.key_at_mut(pos) {
                let n = km.states.len().max(2);
                while km.states.len() < n {
                    km.states.push(profile::StateDef { show_title: true, ..Default::default() });
                }
                km.state = (km.state + 1) % n;
            }
            self.changed(now);
        }
    }

    fn apply(&mut self, e: Effect, from: Option<&KeyAction>, now: f64) {
        match e {
            Effect::NextPage => self.goto_page((self.profile().current + 1) % self.profile().pages.len()),
            Effect::PrevPage => {
                let n = self.profile().pages.len();
                self.goto_page((self.profile().current + n - 1) % n)
            }
            Effect::GotoPage(i) => {
                if i < self.profile().pages.len() {
                    self.goto_page(i)
                }
            }
            Effect::OpenFolder(id) => self.open_folder(id, now),
            Effect::BackToParent => {
                self.folder.pop();
                self.selected = None;
            }
            Effect::SwitchProfile(id) => {
                let idx = self
                    .profiles
                    .iter()
                    .position(|p| !id.is_empty() && p.uuid.eq_ignore_ascii_case(&id))
                    .unwrap_or((self.cur + 1) % self.profiles.len());
                self.switch_profile(idx);
            }
            Effect::BrightnessStep(d) => self.set_brightness((self.brightness as i32 + d).clamp(0, 100) as u8),
            Effect::BrightnessSet(v) => self.set_brightness(v),
            Effect::Sleep => {
                self.sleeping = true;
                if let Some(tx) = &self.hid_tx {
                    let _ = tx.send(hid::Cmd::Brightness(0));
                }
            }
            Effect::ToggleTimer => {
                if let Some(k) = from {
                    self.timer_short_press(k);
                }
            }
            Effect::Status(s) => self.say(s, now),
            // A plugin action pressed without its plugin's own key handling (a Multi Action
            // step, or a key whose plugin isn't running).
            Effect::PluginStep(k) => match self.plugin_for(&k.uuid) {
                Some(p) => {
                    let dev = self.device_id();
                    self.plugin_send(&p, plugins::key_event("keyDown", &k, (0, 0), &dev, true));
                    self.plugin_send(&p, plugins::key_event("keyUp", &k, (0, 0), &dev, true));
                }
                None => {
                    let why = self
                        .plugins
                        .as_ref()
                        .and_then(|h| h.plugin_of(&k.uuid).and_then(|p| h.problems.get(p).cloned()))
                        .unwrap_or_else(|| "its plugin isn't in LabDeck's Plugins folder".into());
                    self.say(format!("“{}” can't run: {why}", k.name), now);
                }
            },
        }
    }

    pub fn goto_page(&mut self, i: usize) {
        self.folder.clear();
        self.selected = None;
        self.profiles[self.cur].current = i;
        self.dirty = true;
    }

    pub fn open_folder(&mut self, id: String, now: f64) {
        if id.is_empty() {
            self.say("This folder has no page yet.", now);
            return;
        }
        self.profiles[self.cur].page_mut(&id);
        self.folder.push(id);
        self.selected = None;
    }

    /// Switch to another profile. The one being left is saved first, and the choice is
    /// remembered for the next start.
    pub fn switch_profile(&mut self, i: usize) {
        if i >= self.profiles.len() {
            return;
        }
        if self.persist && self.dirty && i != self.cur {
            let root = profile::profiles_root();
            let cur = self.cur;
            let _ = self.profiles[cur].save(&root);
            self.dirty = false;
        }
        self.cur = i;
        self.settings.last_profile = Some(self.profiles[i].uuid.clone());
        self.save_settings();
        self.folder.clear();
        self.selected = None;
        if let Some(m) = devices::by_model(&self.profiles[i].model) {
            self.layout_model = m;
        }
    }

    pub fn set_brightness(&mut self, v: u8) {
        self.brightness = v;
        self.sleeping = false;
        if let Some(tx) = &self.hid_tx {
            let _ = tx.send(hid::Cmd::Brightness(v));
        }
    }

    /// Put an action from the list on a key.
    pub fn place(&mut self, uuid: &str, pos: (usize, usize), now: f64) {
        let Some(def) = catalog::find(&self.cats, uuid).cloned() else { return };
        let mut k = KeyAction::new(&def.uuid, &def.name, def.states);
        match def.uuid.as_str() {
            "com.elgato.streamdeck.system.hotkey" | "com.elgato.streamdeck.system.hotkeyswitch" => {
                k.settings = serde_json::json!({"Coalesce": true, "Hotkeys": vec![actions::empty_hotkey(); 4]});
            }
            actions::TIMED_HOTKEY => {
                k.settings = serde_json::json!({
                    "Hotkeys": vec![actions::empty_hotkey(); 1],
                    "delayMs": 0, "holdMs": actions::TIMED_HOLD_MS, "repeatMs": 0, "repeatCount": 0,
                });
            }
            "com.elgato.streamdeck.profile.openchild" => {
                // A new folder gets its own page with a "back" key in the corner, like the original.
                let child = profile::new_uuid();
                let back = KeyAction::new("com.elgato.streamdeck.profile.backtoparent", "Parent Folder", 1);
                self.profiles[self.cur].page_mut(&child).keys.insert((0, 0), back);
                k.settings = serde_json::json!({"ProfileUUID": child});
            }
            "com.elgato.streamdeck.system.multimedia" => k.settings = serde_json::json!({"actionIdx": 1}),
            "com.elgato.streamdeck.system.keybrightness" => k.settings = serde_json::json!({"actionIdx": 0}),
            "com.elgato.streamdeck.system.timer" => k.settings = serde_json::json!({"duration": DEFAULT_TIMER_SECONDS}),
            "com.elgato.streamdeck.page.goto" => k.settings = serde_json::json!({"PageIndex": 1}),
            "com.elgato.streamdeck.system.website" => k.settings = serde_json::json!({"openInBrowser": true, "path": ""}),
            "com.elgato.streamdeck.system.text" => {
                k.settings = serde_json::json!({"pastedText": "", "isSendingEnter": false, "isTypingMode": true})
            }
            _ => {}
        }
        let id = self.page_id();
        self.profiles[self.cur].page_mut(&id).keys.insert(pos, k);
        self.selected = Some(pos);
        self.changed(now);
    }

    pub fn remove_key(&mut self, pos: (usize, usize), now: f64) {
        let id = self.page_id();
        self.profiles[self.cur].page_mut(&id).keys.remove(&pos);
        if self.selected == Some(pos) {
            self.selected = None;
        }
        self.changed(now);
    }

    pub fn move_key(&mut self, from: (usize, usize), to: (usize, usize), now: f64) {
        if from == to {
            return;
        }
        let id = self.page_id();
        let pg = self.profiles[self.cur].page_mut(&id);
        let a = pg.keys.remove(&from);
        let b = pg.keys.remove(&to);
        if let Some(a) = a {
            pg.keys.insert(to, a);
        }
        if let Some(b) = b {
            pg.keys.insert(from, b);
        }
        self.selected = Some(to);
        self.changed(now);
    }

    /// Copy a key (menu Copy or Ctrl+C). LabDeck keeps the copy, and the key is also given back as
    /// text for the system clipboard: the window only reports Ctrl+V when the clipboard holds
    /// something, and that way a copy can also be pasted after LabDeck restarts.
    pub fn copy_key(&mut self, pos: (usize, usize), now: f64) -> Option<String> {
        let id = self.page_id();
        let pg = self.profile().page(&id)?;
        // Pictures keep working when the copy is pasted on another page.
        let k = profile::with_absolute_images(pg.keys.get(&pos)?, pg.dir.as_deref());
        let text = serde_json::to_string(&k).ok();
        self.clipboard = Some(k);
        self.say("Key copied", now);
        text
    }

    /// Paste onto a key (menu Paste or Ctrl+V), replacing what is there. `text` is what the
    /// system clipboard held: a key LabDeck copied (also in another LabDeck window, or before a
    /// restart) is pasted from it; otherwise LabDeck's own last copy is used.
    pub fn paste_key(&mut self, pos: (usize, usize), text: Option<&str>, now: f64) -> bool {
        let Some(mut k) = text.and_then(key_from_text).or_else(|| self.clipboard.clone()) else { return false };
        k.action_id = profile::new_uuid();
        let id = self.page_id();
        self.profiles[self.cur].page_mut(&id).keys.insert(pos, k);
        self.selected = Some(pos);
        self.changed(now);
        self.say("Key pasted", now);
        true
    }

    /// "Move to Folder" (the key menu's flag bit 4 in function 0x140862d20): a new folder takes
    /// the key's place and the key goes inside it, at the same position unless that is the
    /// folder's back-key corner (then the first free one).
    pub fn move_to_folder(&mut self, pos: (usize, usize), now: f64) {
        const FOLDER: &str = "com.elgato.streamdeck.profile.openchild";
        let id = self.page_id();
        let dir = self.profile().page(&id).and_then(|p| p.dir.clone());
        let Some(k) = self.key_at(pos).cloned() else { return };
        if k.uuid == "com.elgato.streamdeck.profile.backtoparent" {
            return;
        }
        let moved = profile::with_absolute_images(&k, dir.as_deref());
        self.place(FOLDER, pos, now);
        let Some(child) = self.key_at(pos).filter(|f| f.uuid == FOLDER).map(|f| f.setting_str("ProfileUUID")) else { return };
        let m = self.model();
        let page = self.profiles[self.cur].page_mut(&child);
        let spot = if pos != (0, 0) {
            Some(pos)
        } else {
            (0..m.rows).flat_map(|r| (0..m.cols).map(move |c| (c, r))).find(|p| !page.keys.contains_key(p))
        };
        match spot {
            Some(s) => {
                page.keys.insert(s, moved);
            }
            // No room in the folder (a one-key device): leave the key where it was.
            None => {
                let here = self.page_id();
                self.profiles[self.cur].page_mut(&here).keys.insert(pos, k);
            }
        }
        self.changed(now);
    }

    /// "Move to Multi Action" (flag bit 6 in function 0x140862d20): a new Multi Action takes the
    /// key's place with the key's action as its first step. Only for actions a Multi Action may
    /// hold (`SupportedInMultiActions`).
    pub fn move_to_multi_action(&mut self, pos: (usize, usize), now: f64) {
        const MULTI: &str = "com.elgato.streamdeck.multiactions.routine";
        let Some(k) = self.key_at(pos).cloned() else { return };
        if !self.allowed_in_multi(&k.uuid) {
            return;
        }
        self.place(MULTI, pos, now);
        if let Some(m) = self.key_at_mut(pos).filter(|m| m.uuid == MULTI) {
            actions::set_sub_actions(m, 0, &[k]);
        }
        self.changed(now);
    }

    pub fn first_free(&self) -> Option<(usize, usize)> {
        let m = self.model();
        let pg = self.profile().page(&self.page_id());
        (0..m.rows)
            .flat_map(|r| (0..m.cols).map(move |c| (c, r)))
            .find(|pos| pg.is_none_or(|p| !p.keys.contains_key(pos)))
    }

    /// Tell the device thread which keys on the deck's page it presses itself: Hotkey, Hotkey
    /// Switch (for its current state) and Multimedia keys. None while the deck sleeps, so the
    /// first press only wakes it.
    fn publish_direct(&mut self) {
        let mut table = HashMap::new();
        if self.connected.is_some() && !self.sleeping && !self.quitting {
            let m = self.model();
            for r in 0..m.rows {
                for c in 0..m.cols {
                    if let Some(actions::HeldKeys::Keys(combo)) = self.key_at((c, r)).map(actions::held_keys) {
                        table.insert(r * m.cols + c, combo);
                    }
                }
            }
        }
        let repeat = actions::keyboard_repeat();
        if let Ok(mut d) = self.direct.lock() {
            d.keys = table;
            d.repeat = repeat;
        }
    }

    /// Windows drops key presses sent to a program that runs as administrator when LabDeck
    /// doesn't; say so once per program instead of failing silently.
    fn warn_if_blocked(&mut self, now: f64) {
        if let Some((pid, name)) = keys::front_program_above_us() {
            if self.warned_admin != Some(pid) {
                self.warned_admin = Some(pid);
                self.say(
                    format!("{name} runs as administrator, so Windows doesn't let LabDeck press keys in it. Start LabDeck as administrator to use hotkeys there."),
                    now,
                );
            }
        }
    }

    pub fn poll(&mut self, ctx: &egui::Context, now: f64) {
        let mut events = Vec::new();
        if let Some(rx) = &self.hid_rx {
            while let Ok(e) = rx.try_recv() {
                events.push(e);
            }
        }
        for e in events {
            match e {
                hid::Event::Connected { model, serial } => {
                    self.connected = Some((model, serial));
                    self.device_sent.clear();
                    self.say(format!("{} connected", model.name), now);
                    if let (Some(h), Some((id, info))) = (&self.plugins, self.plugin_device()) {
                        h.send_all(&serde_json::json!({"event": "deviceDidConnect", "device": id, "deviceInfo": info}));
                    }
                    if let Some(tx) = &self.hid_tx {
                        let _ = tx.send(hid::Cmd::Refresh);
                        let _ = tx.send(hid::Cmd::Brightness(self.brightness));
                    }
                }
                hid::Event::Disconnected => {
                    if let (Some(h), Some((id, _))) = (&self.plugins, self.plugin_device()) {
                        h.send_all(&serde_json::json!({"event": "deviceDidDisconnect", "device": id}));
                    }
                    self.connected = None;
                    self.pressed.clear();
                    self.say("Stream Deck disconnected", now);
                }
                hid::Event::Key(i, down, handled) => {
                    let m = self.model();
                    let pos = (i % m.cols, i / m.cols);
                    self.press_direct = handled;
                    if down {
                        self.pressed.insert(pos);
                        if self.sleeping && !handled {
                            self.set_brightness(self.brightness);
                        } else {
                            self.press(pos, now);
                        }
                    } else {
                        self.pressed.remove(&pos);
                        self.release(pos, now);
                    }
                    self.press_direct = false;
                    // The device thread waits for the window to catch up with presses it
                    // didn't handle itself (they may change the page).
                    if !handled {
                        if let Ok(mut d) = self.direct.lock() {
                            d.seen += 1;
                        }
                    }
                }
                hid::Event::Problem(p) => self.say(p, now),
            }
        }
        while let Ok(e) = self.fx_rx.try_recv() {
            self.apply(e, None, now);
        }
        self.poll_plugins(now);
        self.apply_site_icons(now);
        self.multi_running.retain(|_, (_, done)| !done.load(Ordering::SeqCst));
        if let Some((pos, next)) = self.held_brightness {
            if now >= next {
                if let Some(k) = self.key_at(pos).cloned() {
                    for e in self.runner.press(&k) {
                        self.apply(e, Some(&k), now);
                    }
                }
                self.held_brightness = Some((pos, now + self.repeat_every_ms as f64 / 1000.0));
            }
            if let Some((_, next)) = self.held_brightness {
                ctx.request_repaint_after(std::time::Duration::from_secs_f64((next - now).max(0.001)));
            }
        }
        // (Held Hotkey and Multimedia keys repeat on the key sender's own thread, keys.rs.)
        self.publish_direct();
        // A Timer key held past LONG_PRESS_MS: stop and reset it (fires while still held, as
        // the original's long-press timer does).
        if let Some((pos, at)) = self.timer_hold {
            if now >= at {
                self.timer_hold = None;
                self.timer_long_press(pos, now);
            } else {
                ctx.request_repaint_after(std::time::Duration::from_millis(20));
            }
        }
        // Finished timers flash for a few seconds, then reset.
        let mut done = Vec::new();
        let mut ring: Vec<String> = Vec::new();
        for (id, t) in self.timers.iter_mut() {
            if t.paused.is_none() && Instant::now() >= t.end && t.done_at.is_none() {
                t.done_at = Some(Instant::now());
                ring.push(t.sound.clone());
            }
            if t.done_at.is_some_and(|d| d.elapsed().as_secs_f32() > TIMER_FLASH_SECONDS) {
                done.push(id.clone());
            }
        }
        for d in done {
            self.timers.remove(&d);
        }
        // Time's up: ring the bell (or the key's own sound file).
        for sound in ring {
            for e in self.runner.timer_bell(&sound) {
                self.apply(e, None, now);
            }
        }
        if self.persist && self.dirty && now - self.last_change > 0.6 {
            let root = profile::profiles_root();
            let cur = self.cur;
            if let Err(e) = self.profiles[cur].save(&root) {
                self.say(format!("Could not save: {e}"), now);
            }
            self.dirty = false;
        }
        if self.connected.is_some() {
            self.sync_device();
        }
        let fast = !self.timers.is_empty() || self.connected.is_some();
        ctx.request_repaint_after(std::time::Duration::from_millis(if fast { SYNC_INTERVAL_MS } else { 1000 }));
    }

    pub fn quit(&mut self, ctx: &egui::Context) {
        self.quitting = true;
        // No key stays held down after LabDeck closes.
        self.key_held.clear();
        if let Ok(mut d) = self.direct.lock() {
            d.keys.clear();
        }
        keys::let_go_all();
        // Plugin programs end with LabDeck.
        if let Some(h) = &mut self.plugins {
            h.stop();
        }
        ctx.send_viewport_cmd(egui::ViewportCommand::Close);
    }

    pub fn save_settings(&self) {
        if self.persist {
            let _ = std::fs::create_dir_all(profile::data_dir());
            if let Ok(t) = serde_json::to_string_pretty(&self.settings) {
                let _ = std::fs::write(profile::data_dir().join("settings.json"), t);
            }
        }
    }

    /// Tray clicks, and closing the window to the tray instead of quitting.
    fn background(&mut self, ctx: &egui::Context) {
        while let Ok(m) = self.tray_rx.try_recv() {
            // Opening the window draws every key on the deck again, so the deck and the window
            // agree even if something else drew on the deck meanwhile.
            self.redraw_device();
            let show = |ctx: &egui::Context| {
                ctx.send_viewport_cmd(egui::ViewportCommand::Visible(true));
                ctx.send_viewport_cmd(egui::ViewportCommand::Minimized(false));
                ctx.send_viewport_cmd(egui::ViewportCommand::Focus);
            };
            match m {
                tray::TrayMsg::Show => {
                    self.hidden = false;
                    show(ctx);
                }
                tray::TrayMsg::Preferences => {
                    self.hidden = false;
                    self.device_menu = true;
                    self.profile_menu = false;
                    show(ctx);
                }
                tray::TrayMsg::Quit => {
                    // Asks first unless "Don't ask me again" was ticked (function 0x14018bb80).
                    if self.settings.dont_ask_quit {
                        self.quit(ctx);
                    } else {
                        self.hidden = false;
                        self.confirm_quit = true;
                        show(ctx);
                    }
                }
            }
        }
        if self.tray.is_some() && !self.quitting && ctx.input(|i| i.viewport().close_requested()) {
            ctx.send_viewport_cmd(egui::ViewportCommand::CancelClose);
            ctx.send_viewport_cmd(egui::ViewportCommand::Visible(false));
            if !self.hidden {
                self.hidden = true;
            }
        }
    }

    pub fn handle_screenshot(&mut self, ctx: &egui::Context) {
        let Some(path) = self.screenshot.clone() else { return };
        self.frames += 1;
        ctx.request_repaint();
        if self.shot_done.load(Ordering::SeqCst) {
            ctx.send_viewport_cmd(egui::ViewportCommand::Close);
            return;
        }
        if self.frames >= 120 && !self.shot_requested {
            self.shot_requested = true;
            let done = self.shot_done.clone();
            let ctx2 = ctx.clone();
            ctx.request_screenshot(move |img| {
                let [w, h] = img.size;
                let mut buf = Vec::with_capacity(w * h * 4);
                for p in &img.pixels {
                    buf.extend_from_slice(&p.to_srgba_unmultiplied());
                }
                if let Some(dir) = path.parent() {
                    let _ = std::fs::create_dir_all(dir);
                }
                if let Some(im) = image::RgbaImage::from_raw(w as u32, h as u32, buf) {
                    if let Err(e) = im.save(&path) {
                        eprintln!("could not save screenshot: {e}");
                    }
                }
                done.store(true, Ordering::SeqCst);
                ctx2.request_repaint();
            });
        }
    }
}

impl eframe::App for App {
    /// Runs before every frame and also while the window is hidden in the tray, so the
    /// Stream Deck keeps working and stays in sync without the window.
    fn logic(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        let now = ctx.input(|i| i.time);
        if self.second_copy {
            // Started in the tray (autostart) while another copy runs: there's nothing to show.
            if self.hidden {
                ctx.send_viewport_cmd(egui::ViewportCommand::Close);
                return;
            }
            if self.status.as_ref().is_none_or(|(m, _)| m != SECOND_COPY_NOTE) {
                self.say(SECOND_COPY_NOTE, now);
            } else if let Some(s) = self.status.as_mut() {
                s.1 = now; // keep it showing
            }
        }
        self.poll(ctx, now);
        self.background(ctx);
    }

    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        crate::view::frame(self, ui);
    }

    fn raw_input_hook(&mut self, _ctx: &egui::Context, raw: &mut egui::RawInput) {
        // The check picture must not depend on where the mouse happens to be.
        if self.screenshot.is_some() {
            raw.events.retain(|e| !matches!(e, egui::Event::PointerMoved(_) | egui::Event::PointerButton { .. } | egui::Event::MouseWheel { .. }));
            raw.events.push(egui::Event::PointerGone);
        }
    }

    fn clear_color(&self, _v: &egui::Visuals) -> [f32; 4] {
        CANVAS.to_normalized_gamma_f32()
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn only_one_copy_drives_the_deck() {
        // A second holder of the lock is turned away while the first runs, and gets it once the
        // first is gone (as when that copy of LabDeck quits or crashes).
        let dir = std::env::temp_dir().join(format!("labdeck-lock-test-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("LabDeck.lock");
        let first = super::lock_file(&path).expect("first copy gets the lock");
        assert_eq!(super::lock_file(&path).err(), Some(true), "second copy must see it taken");
        drop(first);
        assert!(super::lock_file(&path).is_ok(), "free again once the first copy is gone");
        let _ = std::fs::remove_dir_all(&dir);
    }
}