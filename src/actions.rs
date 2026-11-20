//! Runs the built-in actions when a key is pressed (on the device or by clicking in the
//! window). Settings use the same names the original saves in its profiles.

use crate::keys;
use crate::profile::KeyAction;
use serde_json::Value;
use std::time::{Duration, Instant};

pub enum Effect {
    NextPage,
    PrevPage,
    GotoPage(usize),
    OpenFolder(String),
    BackToParent,
    SwitchProfile(String),
    BrightnessStep(i32),
    BrightnessSet(u8),
    Sleep,
    ToggleTimer,
    Status(String),
    /// A plugin's action: its plugin gets keyDown and keyUp (see plugins.rs).
    PluginStep(KeyAction),
}

/// Multimedia `actionIdx` values, in the order of the original's table (function 0x1402ca990).
pub const MEDIA: &[&str] = &["Previous Track", "Play / Pause", "Next Track", "Stop", "Mute", "Volume Up", "Volume Down"];
/// The same table's entries 7..=17, named here from their key pictures (`Images/btn_media_…`):
/// Mac keyboard keys with no standard key code that LabDeck can send.
pub const MEDIA_MAC_ONLY: &[&str] = &[
    "Display Brightness Up", "Display Brightness Down", "Contrast Up", "Contrast Down", "Eject",
    "Video Mirror", "Fast Forward", "Rewind", "Keyboard Brightness Up", "Keyboard Brightness Down",
    "Keyboard Backlight",
];

/// The Multimedia choice saved on a key. A key without one acts as Play / Pause, the choice a
/// new key is given.
pub fn media_index(settings: &Value) -> usize {
    settings.get("actionIdx").and_then(|v| v.as_u64()).unwrap_or(1) as usize
}

/// Name of a Multimedia choice, also for the table's Mac-only entries.
pub fn media_name(idx: usize) -> Option<&'static str> {
    MEDIA.get(idx).or_else(|| MEDIA_MAC_ONLY.get(idx.wrapping_sub(MEDIA.len()))).copied()
}

/// Windows key codes of Multimedia choices 0..=6: previous track, play/pause, next track, stop,
/// mute, volume up, volume down.
pub const MEDIA_VK: [u16; 7] = [0xb1, 0xb3, 0xb0, 0xb2, 0xad, 0xaf, 0xae];

/// The key a Multimedia choice sends, or a status line saying why it can't be sent.
fn media_combo(idx: usize) -> Result<keys::Combo, String> {
    match (MEDIA_VK.get(idx).and_then(|vk| keys::Combo::from_vk(&[], *vk)), media_name(idx)) {
        (Some(c), _) => Ok(c),
        (None, Some(name)) => Err(format!("LabDeck can't send “{name}” on this system yet.")),
        (None, None) => Err(format!("Unknown multimedia choice {idx}.")),
    }
}

/// Key picture for each Multimedia choice: the original's table gives every entry its own
/// (btn_media_prev_track, _play_pause, _next_track, _stop, _mute, _volume_up, _volume_down).
/// LabDeck draws its own line icons for them.
pub fn media_icon(idx: usize) -> crate::icons::Icon {
    use crate::icons::Icon;
    match idx {
        0 => Icon::MediaPrev,
        2 => Icon::MediaNext,
        3 => Icon::MediaStop,
        4 => Icon::MediaMute,
        5 => Icon::MediaVolUp,
        6 => Icon::MediaVolDown,
        _ => Icon::Media,
    }
}
/// Brightness `actionIdx` labels, in the order of the original's editor (function 0x14038ff80).
pub const BRIGHTNESS: &[&str] = &["Brighter", "Darker", "Maximum", "High", "Medium", "Low", "Minimum"];
/// Brighter/Darker step and the fixed levels for actionIdx 2..6 (function 0x1402c1e50).
pub const BRIGHTNESS_STEP: i32 = 10;
pub const BRIGHTNESS_LEVELS: [u8; 5] = [100, 75, 50, 25, 0];
/// Keys that repeat while held (Brighter/Darker, Hotkey, Hotkey Switch, Multimedia) follow the
/// computer's own keyboard repeat settings, like a held keyboard key. Before the first repeat
/// they wait REPEAT_DELAY_BASE_MS + REPEAT_DELAY_STEP_MS × the keyboard delay setting (0..3)
/// (start functions 0x1402c6d90 and 0x1402c1230: 250 − v × −750 / 3), or REPEAT_DELAY_FALLBACK_MS
/// when the setting can't be read or the result isn't above 0.
pub const REPEAT_DELAY_BASE_MS: u64 = 250;
pub const REPEAT_DELAY_STEP_MS: u64 = 250;
pub const REPEAT_DELAY_FALLBACK_MS: u64 = 500;
/// Between repeats they wait 1000 / (speed × 27.5 / 31 + 2.5) ms for the keyboard speed setting
/// (0..31, i.e. 2.5 to 30 repeats a second), or REPEAT_EVERY_FALLBACK_MS when it can't be read
/// (repeat ticks 0x1402c6e10, 0x1402c12b0 and 0x1402d1f70; constants at 0x1416df818,
/// 0x1416df770, 0x1416df780 and 0x1416df6d0).
pub const REPEAT_RATE_MS_PER_S: f64 = 1000.0;
pub const REPEAT_RATE_SPAN: f64 = 27.5;
pub const REPEAT_RATE_STEPS: f64 = 31.0;
pub const REPEAT_RATE_SLOWEST: f64 = 2.5;
pub const REPEAT_EVERY_FALLBACK_MS: u64 = 100;

/// Wait before the first repeat for keyboard delay setting `v` (None: unreadable).
pub fn repeat_delay_ms(v: Option<u32>) -> u64 {
    match v {
        Some(v) => {
            let ms = REPEAT_DELAY_BASE_MS as i64 + REPEAT_DELAY_STEP_MS as i64 * v as i64;
            if ms > 0 { ms as u64 } else { REPEAT_DELAY_FALLBACK_MS }
        }
        None => REPEAT_DELAY_FALLBACK_MS,
    }
}

/// Wait between repeats for keyboard speed setting `s` (None: unreadable).
pub fn repeat_every_ms(s: Option<u32>) -> u64 {
    match s {
        Some(s) => {
            let ms = (REPEAT_RATE_MS_PER_S / (s as f64 * REPEAT_RATE_SPAN / REPEAT_RATE_STEPS + REPEAT_RATE_SLOWEST)) as i64;
            if ms > 0 { ms as u64 } else { REPEAT_EVERY_FALLBACK_MS }
        }
        None => REPEAT_EVERY_FALLBACK_MS,
    }
}

/// (first repeat, between repeats) in ms from this computer's keyboard settings. Windows reads
/// them with SystemParametersInfo (SPI_GETKEYBOARDDELAY 0x16, SPI_GETKEYBOARDSPEED 0x0A), as the
/// original does each time a repeat timer starts; elsewhere the fallbacks are used.
pub fn keyboard_repeat() -> (u64, u64) {
    (repeat_delay_ms(system_param(0x16)), repeat_every_ms(system_param(0x0A)))
}

#[cfg(windows)]
fn system_param(action: u32) -> Option<u32> {
    #[link(name = "user32")]
    unsafe extern "system" {
        fn SystemParametersInfoW(action: u32, param: u32, value: *mut core::ffi::c_void, win_ini: u32) -> i32;
    }
    let mut v: u32 = 0;
    // SAFETY: both settings write one 32-bit value into `v`.
    let ok = unsafe { SystemParametersInfoW(action, 0, &mut v as *mut u32 as *mut core::ffi::c_void, 0) };
    (ok != 0).then_some(v)
}

#[cfg(not(windows))]
fn system_param(_action: u32) -> Option<u32> {
    None
}

pub struct Runner {
    enigo: Option<enigo::Enigo>,
    audio: Option<rodio::MixerDeviceSink>,
    players: Vec<rodio::Player>,
    /// Set by a second press while a Multi Action runs (function 0x1402c9c40 moves a running
    /// Multi Action to its canceling state on key down).
    pub cancel: Option<std::sync::Arc<std::sync::atomic::AtomicBool>>,
}

/// What a key that is held down presses (Hotkey, Hotkey Switch, Multimedia).
pub enum HeldKeys {
    /// Not an action whose keys are held.
    NotHeld,
    Keys(keys::Combo),
    /// A Hotkey with no key set.
    Empty,
    /// A choice that can't be sent here (said in the status line).
    Unsendable(String),
}

/// The keys an action holds while its key is held: a Hotkey or Hotkey Switch sends the Hotkeys
/// entry for its state (KA_HotKey::ProcessKeyDown 0x1402c1540 reads entry `state`, 24 bytes
/// each), Multimedia its media key (KA_Multimedia 0x1402d20c0).
pub fn held_keys(k: &KeyAction) -> HeldKeys {
    match k.uuid.as_str() {
        "com.elgato.streamdeck.system.hotkey" | "com.elgato.streamdeck.system.hotkeyswitch" => {
            let slot = if k.uuid.ends_with("hotkeyswitch") { k.state.min(1) } else { 0 };
            match k.settings.get("Hotkeys").and_then(|h| h.as_array()).and_then(|l| l.get(slot)).and_then(keys::hotkey) {
                Some(c) => HeldKeys::Keys(c),
                None => HeldKeys::Empty,
            }
        }
        "com.elgato.streamdeck.system.multimedia" => match media_combo(media_index(&k.settings)) {
            Ok(c) => HeldKeys::Keys(c),
            Err(msg) => HeldKeys::Unsendable(msg),
        },
        _ => HeldKeys::NotHeld,
    }
}

impl Runner {
    pub fn new() -> Self {
        Runner { enigo: None, audio: None, players: Vec::new(), cancel: None }
    }

    fn sleep_cancellable(&self, ms: u64) {
        let end = Instant::now() + Duration::from_millis(ms);
        while Instant::now() < end {
            if self.cancel.as_ref().is_some_and(|c| c.load(std::sync::atomic::Ordering::SeqCst)) {
                return;
            }
            std::thread::sleep(Duration::from_millis(10));
        }
    }

    fn input(&mut self) -> Option<&mut enigo::Enigo> {
        if self.enigo.is_none() {
            self.enigo = enigo::Enigo::new(&enigo::Settings::default()).ok();
        }
        self.enigo.as_mut()
    }

    /// Key went down. Returns what the window has to do (page changes etc.).
    pub fn press(&mut self, k: &KeyAction) -> Vec<Effect> {
        let s = &k.settings;
        let mut fx = Vec::new();
        match k.uuid.as_str() {
            "com.elgato.streamdeck.system.hotkey" | "com.elgato.streamdeck.system.hotkeyswitch" | "com.elgato.streamdeck.system.multimedia" => {
                fx.extend(self.tap_held(k));
            }
            // Inside a Multi Action the timed hotkey runs in place (the step thread waits).
            TIMED_HOTKEY => self.timed_hotkey(s),
            "com.elgato.streamdeck.system.text" => {
                let text = k.setting_str("pastedText");
                let enter = s.get("isSendingEnter").and_then(|v| v.as_bool()).unwrap_or(false);
                if let Some(e) = self.input() {
                    use enigo::Keyboard;
                    let _ = e.text(&text);
                    if enter {
                        let _ = e.key(enigo::Key::Return, enigo::Direction::Click);
                    }
                }
            }
            "com.elgato.streamdeck.system.website" => {
                let mut url = k.setting_str("path");
                if !url.is_empty() && !url.contains(':') {
                    url = format!("https://{url}");
                }
                if open::that_detached(&url).is_err() {
                    fx.push(Effect::Status(format!("Could not open {url}")));
                }
            }
            "com.elgato.streamdeck.system.open" => {
                let path = k.setting_str("path");
                fx.extend(launch(path.trim().trim_matches('"'), ""));
            }
            "com.elgato.streamdeck.system.openapp" => {
                let exec = k.setting_str("exec");
                fx.extend(launch(&exec, &k.setting_str("args")));
            }
            "com.elgato.streamdeck.system.close" => {
                let app = k.setting_str("app_name");
                let app = if app.is_empty() { k.setting_str("path") } else { app };
                fx.extend(close_app(&app));
            }
            "com.elgato.streamdeck.page.next" => fx.push(Effect::NextPage),
            "com.elgato.streamdeck.page.previous" => fx.push(Effect::PrevPage),
            "com.elgato.streamdeck.page.goto" => {
                let n = s.get("PageIndex").or(s.get("page")).and_then(|v| v.as_u64()).unwrap_or(1) as usize;
                fx.push(Effect::GotoPage(n.max(1) - 1));
            }
            "com.elgato.streamdeck.profile.openchild" => {
                fx.push(Effect::OpenFolder(k.setting_str("ProfileUUID")));
            }
            "com.elgato.streamdeck.profile.backtoparent" => fx.push(Effect::BackToParent),
            "com.elgato.streamdeck.profile.rotate" => fx.push(Effect::SwitchProfile(k.setting_str("ProfileUUID"))),
            "com.elgato.streamdeck.system.keybrightness" => {
                let idx = s.get("actionIdx").and_then(|v| v.as_u64()).unwrap_or(0);
                fx.push(match idx {
                    0 => Effect::BrightnessStep(BRIGHTNESS_STEP),
                    1 => Effect::BrightnessStep(-BRIGHTNESS_STEP),
                    i => Effect::BrightnessSet(BRIGHTNESS_LEVELS[(i as usize - 2).min(4)]),
                });
            }
            "com.elgato.streamdeck.system.sleep" => fx.push(Effect::Sleep),
            "com.elgato.streamdeck.system.timer" => fx.push(Effect::ToggleTimer),
            "com.elgato.streamdeck.soundboard.playaudio" => {
                let path = k.setting_str("path");
                fx.extend(self.play(&path, s.get("volume").and_then(|v| v.as_f64()).unwrap_or(100.0) as f32 / 100.0));
            }
            "com.elgato.streamdeck.soundboard.stopaudioplay" => self.stop_audio(),
            "com.elgato.streamdeck.multiactions.routine" | "com.elgato.streamdeck.multiactions.routine2" => {
                let group = if k.uuid.ends_with("routine2") { k.state.min(1) } else { 0 };
                for sub in sub_actions(k, group) {
                    if self.cancel.as_ref().is_some_and(|c| c.load(std::sync::atomic::Ordering::SeqCst)) {
                        break;
                    }
                    if sub.uuid == "com.elgato.streamdeck.multiactions.delay" {
                        // A Delay step's timings are (0, delay) (function 0x14084af80); the wait
                        // after it is at least MULTI_STEP_MIN_WAIT_MS (step runner 0x140252290).
                        let ms = sub.settings.get("delay").and_then(|v| v.as_i64()).unwrap_or(1000);
                        self.sleep_cancellable((ms.max(0) as u64).max(MULTI_STEP_MIN_WAIT_MS));
                    } else {
                        fx.extend(self.step(&sub));
                    }
                }
            }
            "com.elgato.streamdeck.multiactions.random" => {
                let subs = sub_actions(k, 0);
                if !subs.is_empty() {
                    fx.extend(self.press(&subs[random_index(random_u32(), subs.len())]));
                }
            }
            "com.elgato.streamdeck.system.digitaltime" | "com.elgato.streamdeck.page.indicator" => {}
            other if other.starts_with("sample.") || !other.starts_with("com.elgato.streamdeck") => {
                fx.push(Effect::PluginStep(k.clone()));
            }
            _ => {}
        }
        fx
    }

    /// Timed Hotkey (a LabDeck action): wait `delayMs`, hold the key command for `holdMs`, then
    /// repeat every `repeatMs` (0 = once) for `repeatCount` times (0 = until pressed again).
    /// Runs on its own thread; `cancel` (a second press) stops it and always releases the keys.
    pub fn timed_hotkey(&mut self, s: &Value) {
        let num = |k: &str, d: u64| s.get(k).and_then(|v| v.as_u64()).unwrap_or(d);
        let (delay, hold, every, count) = (num("delayMs", 0), num("holdMs", TIMED_HOLD_MS), num("repeatMs", 0), num("repeatCount", 0));
        let Some(combo) = s.get("Hotkeys").and_then(|h| h.as_array()).and_then(|l| l.first()).and_then(keys::hotkey) else { return };
        let stopped = |r: &Runner| r.cancel.as_ref().is_some_and(|c| c.load(std::sync::atomic::Ordering::SeqCst));
        if delay > 0 {
            self.sleep_cancellable(delay);
        }
        let mut done = 0u64;
        while !stopped(self) {
            // Held at least KEY_MIN_HOLD_MS, even when stopped early.
            let id = keys::new_id();
            keys::hold(id, combo.clone(), None);
            self.sleep_cancellable(hold.max(keys::KEY_MIN_HOLD_MS));
            keys::let_go(id);
            done += 1;
            if every == 0 || (count > 0 && done >= count) {
                break;
            }
            self.sleep_cancellable(every.saturating_sub(hold));
        }
    }

    /// One Multi Action step (step runner 0x140252290): key down, wait MULTI_STEP_DOWN_MS, key
    /// up, wait MULTI_STEP_UP_MS. A Hotkey or Multimedia step holds its keys for the down time.
    /// A second press of the Multi Action cuts the waits short, but the keys always come back up.
    fn step(&mut self, k: &KeyAction) -> Vec<Effect> {
        let mut fx = Vec::new();
        match held_keys(k) {
            HeldKeys::Keys(c) => {
                let id = keys::new_id();
                keys::hold(id, c, None);
                self.sleep_cancellable(MULTI_STEP_DOWN_MS);
                keys::let_go(id);
            }
            HeldKeys::Empty => self.sleep_cancellable(MULTI_STEP_DOWN_MS),
            HeldKeys::Unsendable(msg) => fx.push(Effect::Status(msg)),
            HeldKeys::NotHeld => {
                fx = self.press(k);
                self.sleep_cancellable(MULTI_STEP_DOWN_MS);
            }
        }
        self.sleep_cancellable(MULTI_STEP_UP_MS.max(MULTI_STEP_MIN_WAIT_MS));
        fx
    }

    /// A Hotkey, Hotkey Switch or Multimedia action run as one press (from a Random Action, or
    /// Trigger Action): its keys go down and come up again KEY_MIN_HOLD_MS later.
    fn tap_held(&mut self, k: &KeyAction) -> Vec<Effect> {
        match held_keys(k) {
            HeldKeys::Keys(c) => keys::tap(c),
            HeldKeys::Unsendable(msg) => return vec![Effect::Status(msg)],
            HeldKeys::Empty | HeldKeys::NotHeld => {}
        }
        Vec::new()
    }

    fn sink(&mut self) -> Result<&rodio::MixerDeviceSink, String> {
        if self.audio.is_none() {
            let mut s = rodio::DeviceSinkBuilder::open_default_sink().map_err(|e| format!("No sound output: {e}"))?;
            s.log_on_drop(false);
            self.audio = Some(s);
        }
        Ok(self.audio.as_ref().unwrap())
    }

    /// The Timer's sound when time is up, by its choice (`actionIdx`, see `timer_sound`):
    /// 0..=3 one of LabDeck's own chimes, 4 the person's file (`lastUserFile`, only if it
    /// exists, as 0x1402df610 checks), 6 none. A chosen file that's missing plays nothing and is
    /// reported.
    pub fn timer_bell(&mut self, choice: u64, user_file: &str) -> Vec<Effect> {
        let file = user_file.trim();
        let chime = match choice {
            TIMER_SOUND_FILE => {
                if !file.is_empty() && std::path::Path::new(file).exists() {
                    return self.play(file, 1.0);
                }
                let what = if file.is_empty() { "no file is chosen".to_string() } else { format!("{file} wasn't found") };
                return vec![Effect::Status(format!("Time's up. No sound: {what}."))];
            }
            c => match CHIMES.get(c as usize) {
                Some(ch) => *ch,
                None => return Vec::new(), // None (6), or a choice with no sound
            },
        };
        let sink = match self.sink() {
            Ok(s) => s,
            Err(e) => return vec![Effect::Status(e)],
        };
        let player = rodio::Player::connect_new(sink.mixer());
        player.append(bell_samples(chime));
        self.players.retain(|p| !p.empty());
        self.players.push(player);
        Vec::new()
    }

    fn play(&mut self, path: &str, volume: f32) -> Vec<Effect> {
        if path.is_empty() {
            return vec![Effect::Status("Choose a sound file for this key first.".into())];
        }
        let sink = match self.sink() {
            Ok(s) => s,
            Err(e) => return vec![Effect::Status(e)],
        };
        let file = match std::fs::File::open(path) {
            Ok(f) => f,
            Err(e) => return vec![Effect::Status(format!("Could not open {path}: {e}"))],
        };
        match rodio::play(sink.mixer(), std::io::BufReader::new(file)) {
            Ok(p) => {
                p.set_volume(volume);
                self.players.retain(|p| !p.empty());
                self.players.push(p);
                vec![]
            }
            Err(e) => vec![Effect::Status(format!("Could not play {path}: {e}"))],
        }
    }

    pub fn stop_audio(&mut self) {
        for p in self.players.drain(..) {
            p.stop();
        }
    }
}

/// Timer bell (made here, not the original's sound): BELL_STRIKES strikes BELL_GAP_S apart,
/// each a struck-bell tone of inharmonic partials that die away over BELL_DECAY_S.
pub const BELL_HZ: f32 = 880.0;
pub const BELL_STRIKES: usize = 3;
pub const BELL_GAP_S: f32 = 0.6;
pub const BELL_DECAY_S: f32 = 1.4;

/// One of LabDeck's own Timer chimes: pitch, strikes and seconds between strikes.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Chime {
    pub hz: f32,
    pub strikes: usize,
    pub gap_s: f32,
}

/// The Timer's built-in sounds 0..=3. The original's choices 0..=3 are four sound files of its
/// own (0x1402df610); LabDeck plays four chimes it makes itself instead.
pub const CHIMES: [Chime; 4] = [
    Chime { hz: BELL_HZ, strikes: BELL_STRIKES, gap_s: BELL_GAP_S },
    Chime { hz: 523.25, strikes: 2, gap_s: 0.45 },
    Chime { hz: 1174.66, strikes: 3, gap_s: 0.25 },
    Chime { hz: 1567.98, strikes: 1, gap_s: 0.0 },
];
/// Timer sound choices (`actionIdx`, function 0x1402df610): 0..=3 built in, 4 the person's
/// file (`lastUserFile`), 6 none. 5 has no sound there either, so it isn't offered.
pub const TIMER_SOUNDS: [(u64, &str); 6] =
    [(0, "Bell"), (1, "Low bell"), (2, "Quick chime"), (3, "Ping"), (TIMER_SOUND_FILE, "Your own sound file"), (TIMER_SOUND_NONE, "No sound")];
pub const TIMER_SOUND_FILE: u64 = 4;
pub const TIMER_SOUND_NONE: u64 = 6;

/// A Timer's sound choice: `actionIdx`, 0 when missing (settings reader 0x1402ddec0). Timer keys
/// made by earlier LabDeck builds saved only `lastUserFile`; those keep playing their file.
pub fn timer_sound(s: &Value) -> u64 {
    match s.get("actionIdx").and_then(|v| v.as_u64()) {
        Some(i) => i,
        None if !s.get("lastUserFile").and_then(|v| v.as_str()).unwrap_or("").trim().is_empty() => TIMER_SOUND_FILE,
        None => 0,
    }
}

fn bell_samples(ch: Chime) -> rodio::buffer::SamplesBuffer {
    let rate = 44_100u32;
    let total = ch.gap_s * (ch.strikes as f32 - 1.0) + BELL_DECAY_S * 1.6;
    let n = (total * rate as f32) as usize;
    // Partial ratios and loudness of a small bell.
    let partials = [(1.0f32, 1.0f32), (2.0, 0.5), (2.76, 0.35), (5.4, 0.18), (8.93, 0.08)];
    let mut out = vec![0f32; n];
    for s in 0..ch.strikes {
        let start = (s as f32 * ch.gap_s * rate as f32) as usize;
        for (i, v) in out.iter_mut().enumerate().skip(start) {
            let t = (i - start) as f32 / rate as f32;
            let attack = (t / 0.004).min(1.0);
            let mut x = 0.0;
            for (k, (ratio, amp)) in partials.iter().enumerate() {
                // Higher partials fade faster, as on a real bell.
                let decay = (-t / (BELL_DECAY_S / (1.0 + k as f32 * 0.8))).exp();
                x += amp * decay * (std::f32::consts::TAU * ch.hz * ratio * t).sin();
            }
            *v += 0.22 * attack * x;
        }
    }
    for v in &mut out {
        *v = v.clamp(-1.0, 1.0);
    }
    rodio::buffer::SamplesBuffer::new(
        std::num::NonZero::new(1u16).unwrap(),
        std::num::NonZero::new(rate).unwrap(),
        out,
    )
}

/// LabDeck's own Timed Hotkey action (not in the original).
pub const TIMED_HOTKEY: &str = "com.labdeck.system.timedhotkey";
/// Default hold time (the shortest is keys::KEY_MIN_HOLD_MS, as for every key LabDeck sends).
pub const TIMED_HOLD_MS: u64 = 100;

/// Multi Action step timing (step runner 0x140252290 with the default step timings of
/// 0x1403bcdc0): each step goes down, stays down MULTI_STEP_DOWN_MS, comes up, and the next
/// step starts MULTI_STEP_UP_MS later. A Delay step's timings are (0, its delay) (0x14084af80),
/// and the wait after a key comes up is at least MULTI_STEP_MIN_WAIT_MS.
pub const MULTI_STEP_DOWN_MS: u64 = 100;
pub const MULTI_STEP_UP_MS: u64 = 100;
pub const MULTI_STEP_MIN_WAIT_MS: u64 = 1;

/// A fresh 32-bit random number from the system (std's randomly keyed hasher, rekeyed per call).
fn random_u32() -> u32 {
    use std::hash::{BuildHasher, Hasher};
    let mut h = std::collections::hash_map::RandomState::new().build_hasher();
    h.write_u128(std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_nanos()).unwrap_or(0));
    let v = h.finish();
    (v ^ (v >> 32)) as u32
}

/// Random Action's pick (KA_MultiActions::ProcessKeyUp 0x1402c9f30): one 32-bit random value
/// scaled to the entry count, `(r * count) >> 32`, so every entry is equally likely.
pub fn random_index(r: u32, count: usize) -> usize {
    ((r as u64 * count as u64) >> 32) as usize
}

/// Multi Actions keep their steps in "Actions": [[step, step, ...], [...]], one list per state.
pub fn sub_actions(k: &KeyAction, group: usize) -> Vec<KeyAction> {
    k.extra
        .get("Actions")
        .and_then(|a| a.as_array())
        .and_then(|groups| groups.get(group))
        .and_then(|g| g.as_array())
        .map(|g| g.iter().filter_map(|x| serde_json::from_value(x.clone()).ok()).collect())
        .unwrap_or_default()
}

pub fn set_sub_actions(k: &mut KeyAction, group: usize, list: &[KeyAction]) {
    let mut groups: Vec<Value> = k.extra.get("Actions").and_then(|a| a.as_array()).cloned().unwrap_or_default();
    while groups.len() <= group {
        groups.push(Value::Array(vec![]));
    }
    groups[group] = Value::Array(list.iter().filter_map(|a| serde_json::to_value(a).ok()).collect());
    k.extra.insert("Actions".into(), Value::Array(groups));
}

fn launch(target: &str, args: &str) -> Vec<Effect> {
    if target.is_empty() {
        return vec![Effect::Status("Choose what this key should open first.".into())];
    }
    let p = std::path::Path::new(target);
    // A program (file that can run, or a bare command name) starts with its arguments;
    // anything else opens with the desktop's default app.
    let is_cmd = !p.exists() && !target.contains('/') && !target.contains('\\');
    let runnable = is_cmd || is_executable(p);
    let res = if runnable {
        let mut c = std::process::Command::new(target);
        if !args.trim().is_empty() {
            c.args(args.split_whitespace());
        }
        c.spawn().map(|_| ())
    } else {
        open::that_detached(target)
    };
    match res {
        Ok(()) => vec![],
        Err(e) => vec![Effect::Status(format!("Could not open {target}: {e}"))],
    }
}

#[cfg(unix)]
fn is_executable(p: &std::path::Path) -> bool {
    use std::os::unix::fs::PermissionsExt;
    p.is_file() && p.metadata().map(|m| m.permissions().mode() & 0o111 != 0).unwrap_or(false)
}

#[cfg(not(unix))]
fn is_executable(p: &std::path::Path) -> bool {
    p.extension().is_some_and(|e| e.eq_ignore_ascii_case("exe"))
}

fn close_app(name: &str) -> Vec<Effect> {
    let name = std::path::Path::new(name.trim_matches('"'))
        .file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or(name)
        .to_string();
    if name.is_empty() {
        return vec![Effect::Status("Choose which app this key closes first.".into())];
    }
    #[cfg(unix)]
    let r = std::process::Command::new("pkill").arg("-x").arg(&name).status();
    #[cfg(not(unix))]
    let r = {
        // No console window flashing up for taskkill.
        use std::os::windows::process::CommandExt;
        std::process::Command::new("taskkill").args(["/IM", &format!("{name}.exe")]).creation_flags(0x0800_0000).status()
    };
    match r {
        Ok(_) => vec![],
        Err(e) => vec![Effect::Status(format!("Could not close {name}: {e}"))],
    }
}

/// Windows virtual-key codes (what the original stores as VKeyCode) to keys.
pub fn vk_to_key(vk: i64) -> Option<enigo::Key> {
    use enigo::Key::*;
    // On Windows the stored code is already the key's own code, so it's sent as is: going
    // through a character would pick whatever key makes that character in the current layout
    // (and break for characters that need Shift there, like "=" on a Nordic keyboard).
    #[cfg(target_os = "windows")]
    if matches!(vk, 0x30..=0x39 | 0x41..=0x5a | 0x60..=0x6f | 0xba..=0xc0 | 0xdb..=0xdf | 0xe2) {
        return Some(Other(vk as u32));
    }
    Some(match vk {
        #[cfg(not(target_os = "macos"))]
        0x2d => Insert,
        0x41..=0x5a => Unicode((b'a' + (vk - 0x41) as u8) as char),
        0x30..=0x39 => Unicode((b'0' + (vk - 0x30) as u8) as char),
        0x60..=0x69 => Unicode((b'0' + (vk - 0x60) as u8) as char),
        0x70 => F1, 0x71 => F2, 0x72 => F3, 0x73 => F4, 0x74 => F5, 0x75 => F6,
        0x76 => F7, 0x77 => F8, 0x78 => F9, 0x79 => F10, 0x7a => F11, 0x7b => F12,
        0x7c => F13, 0x7d => F14, 0x7e => F15, 0x7f => F16, 0x80 => F17, 0x81 => F18,
        0x82 => F19, 0x83 => F20,
        0x25 => LeftArrow, 0x26 => UpArrow, 0x27 => RightArrow, 0x28 => DownArrow,
        0x0d => Return, 0x1b => Escape, 0x20 => Space, 0x09 => Tab, 0x08 => Backspace,
        0x2e => Delete, 0x24 => Home, 0x23 => End, 0x21 => PageUp, 0x22 => PageDown,
        0xb3 => MediaPlayPause, 0xb0 => MediaNextTrack, 0xb1 => MediaPrevTrack, 0xb2 => MediaStop,
        0xad => VolumeMute, 0xae => VolumeDown, 0xaf => VolumeUp,
        _ => Unicode(vk_symbol(vk)?),
    })
}

/// The key a recorded hotkey stores. With Shift held, the window reports the symbol the
/// keyboard layout makes ("!" for Shift+1 on most layouts, "¤" or "$" for Shift+4...), and
/// not every symbol is a key the editor knows. Like the original, a hotkey stores the key
/// itself, not the symbol: letters come from the layout (so A stays A on AZERTY), every
/// other key from its place on the keyboard (Shift+1 is Shift + 1 on any layout).
pub fn recorded_vk(logical: egui::Key, physical: Option<egui::Key>) -> Option<i64> {
    use egui::Key as K;
    // The number row: always the number on the key, whatever Shift turns it into.
    if let Some(p @ (K::Num0 | K::Num1 | K::Num2 | K::Num3 | K::Num4 | K::Num5 | K::Num6 | K::Num7 | K::Num8 | K::Num9)) = physical {
        return egui_to_vk(p);
    }
    // Letters, named keys and unshifted punctuation: what the layout says the key is.
    egui_to_vk(logical)
        // A symbol the editor has no name for: the key's place on the keyboard.
        .or_else(|| physical.and_then(egui_to_vk))
        .or_else(|| shifted_symbol_vk(logical))
}

/// Shifted symbols, used only when the key's place isn't known: the key they're on in the
/// US layout.
fn shifted_symbol_vk(k: egui::Key) -> Option<i64> {
    use egui::Key as K;
    Some(match k {
        K::Exclamationmark => 0x31, K::Questionmark => 0xbf, K::Colon => 0xba, K::Pipe => 0xdc,
        K::OpenCurlyBracket => 0xdb, K::CloseCurlyBracket => 0xdd,
        _ => return None,
    })
}

/// The character on a numpad operator or punctuation key, by its Windows virtual-key code.
fn vk_symbol(vk: i64) -> Option<char> {
    Some(match vk {
        0x6a => '*', 0x6b => '+', 0x6d => '-', 0x6e => '.', 0x6f => '/',
        0xba => ';', 0xbb => '=', 0xbc => ',', 0xbd => '-', 0xbe => '.', 0xbf => '/', 0xc0 => '`',
        0xdb => '[', 0xdc => '\\', 0xdd => ']', 0xde => '\'',
        _ => return None,
    })
}

/// egui key to Windows virtual-key code, for recording hotkeys in the editor.
pub fn egui_to_vk(k: egui::Key) -> Option<i64> {
    use egui::Key as K;
    let name = k.name();
    if name.len() == 1 {
        let c = name.chars().next()?.to_ascii_uppercase();
        if c.is_ascii_alphanumeric() {
            return Some(c as i64);
        }
    }
    Some(match k {
        K::F1 => 0x70, K::F2 => 0x71, K::F3 => 0x72, K::F4 => 0x73, K::F5 => 0x74, K::F6 => 0x75,
        K::F7 => 0x76, K::F8 => 0x77, K::F9 => 0x78, K::F10 => 0x79, K::F11 => 0x7a, K::F12 => 0x7b,
        K::ArrowLeft => 0x25, K::ArrowUp => 0x26, K::ArrowRight => 0x27, K::ArrowDown => 0x28,
        K::Enter => 0x0d, K::Escape => 0x1b, K::Space => 0x20, K::Tab => 0x09, K::Backspace => 0x08,
        K::Delete => 0x2e, K::Home => 0x24, K::End => 0x23, K::PageUp => 0x21, K::PageDown => 0x22,
        K::Semicolon => 0xba, K::Equals => 0xbb, K::Comma => 0xbc, K::Minus => 0xbd, K::Period => 0xbe,
        K::Slash => 0xbf, K::Backtick => 0xc0, K::OpenBracket => 0xdb, K::Backslash => 0xdc,
        K::CloseBracket => 0xdd, K::Quote => 0xde, K::Insert => 0x2d,
        K::F13 => 0x7c, K::F14 => 0x7d, K::F15 => 0x7e, K::F16 => 0x7f, K::F17 => 0x80, K::F18 => 0x81,
        K::F19 => 0x82, K::F20 => 0x83, K::Plus => 0xbb,
        _ => return None,
    })
}

pub fn vk_name(vk: i64) -> String {
    match vk {
        0x41..=0x5a | 0x30..=0x39 => ((vk as u8) as char).to_string(),
        0x70..=0x83 => format!("F{}", vk - 0x6f),
        0x25 => "Left".into(), 0x26 => "Up".into(), 0x27 => "Right".into(), 0x28 => "Down".into(),
        0x0d => "Enter".into(), 0x1b => "Esc".into(), 0x20 => "Space".into(), 0x09 => "Tab".into(),
        0x08 => "Backspace".into(), 0x2e => "Delete".into(), 0x24 => "Home".into(), 0x23 => "End".into(),
        0x21 => "Page Up".into(), 0x22 => "Page Down".into(), 0x2d => "Insert".into(),
        0x60..=0x69 => format!("Num {}", vk - 0x60),
        _ if vk_symbol(vk).is_some() => vk_symbol(vk).unwrap_or_default().to_string(),
        _ => vk_to_key(vk).map(|k| match k {
            enigo::Key::Unicode(c) => c.to_string(),
            other => format!("{other:?}"),
        }).unwrap_or_default(),
    }
}

/// A hotkey entry in the original's format.
pub fn hotkey_json(vk: i64, ctrl: bool, shift: bool, alt: bool, meta: bool) -> Value {
    let mods = (shift as i64) | (ctrl as i64) << 1 | (alt as i64) << 2 | (meta as i64) << 3;
    serde_json::json!({
        "KeyCmd": meta, "KeyCtrl": ctrl, "KeyModifiers": mods, "KeyOption": alt, "KeyShift": shift,
        "NativeCode": vk, "QTKeyCode": vk, "VKeyCode": vk,
    })
}

pub fn empty_hotkey() -> Value {
    serde_json::json!({
        "KeyCmd": false, "KeyCtrl": false, "KeyModifiers": 0, "KeyOption": false, "KeyShift": false,
        "NativeCode": 146, "QTKeyCode": 33554431, "VKeyCode": -1,
    })
}

#[cfg(test)]
mod tests {
    use super::recorded_vk;
    use egui::Key as K;

    #[test]
    fn copied_keys_paste_back_from_text() {
        let mut k = crate::profile::KeyAction::new("com.elgato.streamdeck.system.website", "Website", 1);
        k.settings = serde_json::json!({"path": "https://example.com"});
        let text = serde_json::to_string(&k).unwrap();
        let back = crate::app::key_from_text(&text).expect("a copied key reads back");
        assert_eq!(back.uuid, k.uuid);
        assert_eq!(back.setting_str("path"), "https://example.com");
        // Ordinary clipboard text is not a key.
        for t in ["hello", "#ff8800", "{}", "{\"UUID\": \"nope\"}", "[1, 2]"] {
            assert!(crate::app::key_from_text(t).is_none(), "{t}");
        }
    }

    #[test]
    fn multimedia_table_matches_the_original() {
        use crate::icons::Icon;
        // Table 0x1402ca990: 0 prev, 1 play/pause, 2 next, 3 stop, 4 mute, 5 vol up, 6 vol down.
        let icons: Vec<Icon> = (0..7).map(super::media_icon).collect();
        assert_eq!(icons, [Icon::MediaPrev, Icon::Media, Icon::MediaNext, Icon::MediaStop, Icon::MediaMute, Icon::MediaVolUp, Icon::MediaVolDown]);
        assert_eq!(super::media_name(3), Some("Stop"));
        assert_eq!(super::media_name(13), Some("Fast Forward"));
        assert_eq!(super::media_name(17), Some("Keyboard Backlight"));
        assert_eq!(super::media_name(18), None);
        // No saved choice: Play / Pause, as a new key gets.
        assert_eq!(super::media_index(&serde_json::json!({})), 1);
    }

    #[test]
    fn window_buttons_follow_the_display_language() {
        let da = crate::theme::caption_names_for("da");
        assert_eq!((da.minimize, da.maximize, da.close), ("Minimer", "Maksimer", "Luk"));
        let en = crate::theme::caption_names_for("en");
        assert_eq!((en.minimize, en.maximize, en.close), ("Minimize", "Maximize", "Close"));
        assert_eq!(crate::theme::caption_names_for("xx").close, "Close");
    }

    #[test]
    fn held_keys_repeat_at_the_keyboard_settings() {
        use super::{repeat_delay_ms, repeat_every_ms};
        // First repeat: 250 ms per keyboard delay step, starting at 250 ms; 500 ms unreadable.
        assert_eq!(repeat_delay_ms(Some(0)), 250);
        assert_eq!(repeat_delay_ms(Some(1)), 500);
        assert_eq!(repeat_delay_ms(Some(3)), 1000);
        assert_eq!(repeat_delay_ms(None), 500);
        // Between repeats: 2.5 a second at speed 0 up to 30 a second at speed 31; 100 ms unreadable.
        assert_eq!(repeat_every_ms(Some(0)), 400);
        assert_eq!(repeat_every_ms(Some(15)), 63);
        assert_eq!(repeat_every_ms(Some(31)), 33);
        assert_eq!(repeat_every_ms(None), 100);
        // Whatever this computer is set to stays inside those ranges.
        let (first, every) = super::keyboard_repeat();
        assert!((250..=1000).contains(&first), "{first}");
        assert!((33..=400).contains(&every), "{every}");
    }

    #[test]
    fn multi_action_steps_take_their_timings() {
        // Each step: down 100 ms, up, then 100 ms before the next.
        assert_eq!((super::MULTI_STEP_DOWN_MS, super::MULTI_STEP_UP_MS), (100, 100));
        // A Delay with no time still waits the shortest wait.
        let k: crate::profile::KeyAction = serde_json::from_value(serde_json::json!({
            "UUID": "com.elgato.streamdeck.multiactions.routine",
            "Settings": {},
            "Actions": [[
                {"UUID": "com.elgato.streamdeck.multiactions.delay", "Settings": {"delay": 0}},
                {"UUID": "com.elgato.streamdeck.system.digitaltime", "Settings": {}}
            ]]
        }))
        .unwrap();
        let t = std::time::Instant::now();
        let mut r = super::Runner::new();
        r.press(&k);
        let ms = t.elapsed().as_millis();
        // 1 ms delay + one 200 ms step (sleeps poll every 10 ms, so allow some slack).
        assert!((200..600).contains(&ms), "{ms}");
    }

    #[test]
    fn random_pick_covers_every_entry_evenly() {
        assert_eq!(super::random_index(0, 3), 0);
        assert_eq!(super::random_index(u32::MAX, 3), 2);
        // Equal slices of the 32-bit range for each entry.
        let mut counts = [0u32; 3];
        for i in 0..3000u32 {
            counts[super::random_index(i.wrapping_mul(1_431_655), 3)] += 1;
        }
        assert!(counts.iter().all(|&c| (900..=1100).contains(&c)), "{counts:?}");
    }

    #[test]
    fn shift_number_row_records_the_number() {
        // Shift+1..4 as the window reports them: (symbol the layout makes, key position).
        // US: ! @ # $   Danish: ! " # ¤   (symbols egui has no name for fall back to the position)
        assert_eq!(recorded_vk(K::Exclamationmark, Some(K::Num1)), Some(0x31));
        assert_eq!(recorded_vk(K::Num2, Some(K::Num2)), Some(0x32));
        assert_eq!(recorded_vk(K::Num3, Some(K::Num3)), Some(0x33));
        assert_eq!(recorded_vk(K::Num4, Some(K::Num4)), Some(0x34));
        // Some layouts make a named symbol with Shift+4 (e.g. "+" or "?"); still the 4 key.
        assert_eq!(recorded_vk(K::Plus, Some(K::Num4)), Some(0x34));
        assert_eq!(recorded_vk(K::Questionmark, Some(K::Num4)), Some(0x34));
        // No position known: the US key for the symbol.
        assert_eq!(recorded_vk(K::Exclamationmark, None), Some(0x31));
    }

    #[test]
    fn letters_follow_the_layout() {
        // The A key on an AZERTY keyboard sits where Q is on US.
        assert_eq!(recorded_vk(K::A, Some(K::Q)), Some(0x41));
        assert_eq!(recorded_vk(K::Plus, Some(K::Minus)), Some(0xbb));
    }
}
