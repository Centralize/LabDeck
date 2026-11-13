//! Sends key commands (Hotkey, Hotkey Switch, Timed Hotkey, Multimedia, Multi Action steps) to
//! other programs in a way games notice too.
//!
//! - Windows: SendInput with each key's hardware scan code (KEYEVENTF_SCANCODE, plus
//!   KEYEVENTF_EXTENDEDKEY for the keys a keyboard sends with an E0 prefix), the way a real
//!   keyboard reports keys. Games that read the keyboard through DirectInput or raw input go by
//!   scan codes, so a key sent only as a virtual-key code can go unnoticed there. The scan code
//!   comes from the keyboard layout of the program in front.
//! - Every key stays down at least KEY_MIN_HOLD_MS. Games look at the keyboard once a frame, so a
//!   key that goes down and comes up at the same moment is never seen.
//! - One thread sends everything, in order, so the window never waits, and held keys repeat on
//!   time even while LabDeck is in the tray.

use serde_json::Value;
use std::sync::mpsc::{self, Receiver, RecvTimeoutError, Sender};
use std::sync::OnceLock;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{Duration, Instant};

/// Shortest time a key is held down (LabDeck's own rule: a game running at 30 frames a second
/// looks at the keyboard every 33 ms, so 50 ms is seen by any of them).
pub const KEY_MIN_HOLD_MS: u64 = 50;

/// A key as this system's sender takes it: a Windows virtual-key code, or an enigo key elsewhere.
#[cfg(windows)]
pub type Key = u16;
#[cfg(not(windows))]
pub type Key = enigo::Key;

/// Modifiers and a key, pressed together.
#[derive(Clone, Debug, PartialEq)]
pub struct Combo {
    pub mods: Vec<Key>,
    pub key: Key,
}

pub const VK_SHIFT: u16 = 0x10;
pub const VK_CONTROL: u16 = 0x11;
pub const VK_MENU: u16 = 0x12;
pub const VK_LWIN: u16 = 0x5b;

impl Combo {
    /// From Windows virtual-key codes, the form profiles save hotkeys in.
    pub fn from_vk(mods: &[u16], key: u16) -> Option<Combo> {
        if key == 0 || key > 0xfe {
            return None;
        }
        #[cfg(windows)]
        {
            Some(Combo { mods: mods.to_vec(), key })
        }
        #[cfg(not(windows))]
        {
            let mods = mods
                .iter()
                .filter_map(|m| match *m {
                    VK_CONTROL => Some(enigo::Key::Control),
                    VK_SHIFT => Some(enigo::Key::Shift),
                    VK_MENU => Some(enigo::Key::Alt),
                    VK_LWIN => Some(enigo::Key::Meta),
                    _ => None,
                })
                .collect();
            Some(Combo { mods, key: crate::actions::vk_to_key(key as i64)? })
        }
    }
}

/// One saved Hotkeys entry (VKeyCode with KeyShift, KeyCtrl, KeyOption, KeyCmd), or None when it
/// has no key. Modifiers are kept in the order of their bits in the saved mask (Shift bit 0,
/// Ctrl bit 1, Alt bit 2, Win bit 3), the order the original presses them in (0x1407807c0 goes
/// up from bit 0 on key down, down from the top bit on key up).
pub fn hotkey(hk: &Value) -> Option<Combo> {
    let vk = hk.get("VKeyCode").and_then(|v| v.as_i64()).unwrap_or(-1);
    if !(1..=0xfe).contains(&vk) {
        return None;
    }
    let flag = |n: &str| hk.get(n).and_then(|v| v.as_bool()).unwrap_or(false);
    let mods: Vec<u16> = [("KeyShift", VK_SHIFT), ("KeyCtrl", VK_CONTROL), ("KeyOption", VK_MENU), ("KeyCmd", VK_LWIN)]
        .into_iter()
        .filter(|(n, _)| flag(n))
        .map(|(_, v)| v)
        .collect();
    Combo::from_vk(&mods, vk as u16)
}

enum Job {
    /// Press now; with `repeat` (first, every) in ms, repeat while held.
    Hold { id: u64, combo: Combo, repeat: Option<(u64, u64)> },
    /// Let go, but not before the key has been down KEY_MIN_HOLD_MS.
    LetGo(u64),
    /// Let everything go (each after its KEY_MIN_HOLD_MS) and say when done.
    LetGoAll(Sender<()>),
}

fn sender() -> &'static Sender<Job> {
    static TX: OnceLock<Sender<Job>> = OnceLock::new();
    TX.get_or_init(|| {
        let (tx, rx) = mpsc::channel();
        std::thread::Builder::new().name("labdeck-keys".into()).spawn(move || run(rx)).ok();
        tx
    })
}

/// A fresh id for `hold` / `let_go`.
pub fn new_id() -> u64 {
    static NEXT: AtomicU64 = AtomicU64::new(1 << 32);
    NEXT.fetch_add(1, Ordering::Relaxed)
}

/// Press a combination and keep it down until `let_go(id)`; with `repeat` = (first, every) in ms
/// it repeats while held, as a held keyboard key does.
pub fn hold(id: u64, combo: Combo, repeat: Option<(u64, u64)>) {
    let _ = sender().send(Job::Hold { id, combo, repeat });
}

/// Let go of what `hold(id)` pressed (at the earliest KEY_MIN_HOLD_MS after it went down).
pub fn let_go(id: u64) {
    let _ = sender().send(Job::LetGo(id));
}

/// Press and let go, held KEY_MIN_HOLD_MS.
pub fn tap(combo: Combo) {
    let id = new_id();
    hold(id, combo, None);
    let_go(id);
}

/// Let every held key go now (when LabDeck quits), waiting until it's done.
pub fn let_go_all() {
    let (tx, rx) = mpsc::channel();
    if sender().send(Job::LetGoAll(tx)).is_ok() {
        let _ = rx.recv_timeout(Duration::from_millis(500));
    }
}

struct Down {
    id: u64,
    combo: Combo,
    at: Instant,
    next_repeat: Option<Instant>,
    every: Duration,
    release_at: Option<Instant>,
}

fn run(rx: Receiver<Job>) {
    let mut out = Output::new();
    let mut down: Vec<Down> = Vec::new();
    loop {
        let due = down.iter().filter_map(|d| d.release_at.or(d.next_repeat)).min();
        let job = match due {
            Some(t) => match rx.recv_timeout(t.saturating_duration_since(Instant::now())) {
                Ok(j) => Some(j),
                Err(RecvTimeoutError::Timeout) => None,
                Err(RecvTimeoutError::Disconnected) => break,
            },
            None => match rx.recv() {
                Ok(j) => Some(j),
                Err(_) => break,
            },
        };
        let now = Instant::now();
        // Keys whose time is up come up; held keys due a repeat repeat.
        down.retain(|d| {
            let done = d.release_at.is_some_and(|r| r <= now);
            if done {
                out.up(&d.combo);
            }
            !done
        });
        for d in down.iter_mut().filter(|d| d.release_at.is_none()) {
            if d.next_repeat.is_some_and(|n| n <= now) {
                out.repeat(&d.combo);
                d.next_repeat = Some(now + d.every);
            }
        }
        match job {
            Some(Job::Hold { id, combo, repeat }) => {
                // Keep the order: keys waiting to come up do so first.
                finish_releases(&mut down, &mut out);
                if let Some(i) = down.iter().position(|d| d.id == id) {
                    let d = down.remove(i);
                    out.up(&d.combo);
                }
                out.down(&combo);
                let at = Instant::now();
                down.push(Down {
                    id,
                    combo,
                    at,
                    next_repeat: repeat.map(|(first, _)| at + Duration::from_millis(first)),
                    every: Duration::from_millis(repeat.map(|r| r.1).unwrap_or(0).max(1)),
                    release_at: None,
                });
            }
            Some(Job::LetGo(id)) => {
                if let Some(d) = down.iter_mut().find(|d| d.id == id && d.release_at.is_none()) {
                    d.release_at = Some((d.at + Duration::from_millis(KEY_MIN_HOLD_MS)).max(now));
                    d.next_repeat = None;
                }
            }
            Some(Job::LetGoAll(done)) => {
                for d in down.iter_mut().filter(|d| d.release_at.is_none()) {
                    d.release_at = Some((d.at + Duration::from_millis(KEY_MIN_HOLD_MS)).max(now));
                }
                finish_releases(&mut down, &mut out);
                let _ = done.send(());
            }
            None => {}
        }
    }
    for d in down.drain(..).rev() {
        out.up(&d.combo);
    }
}

/// Wait for every key that is waiting to come up, and let it go.
fn finish_releases(down: &mut Vec<Down>, out: &mut Output) {
    let (mut waiting, held): (Vec<Down>, Vec<Down>) = std::mem::take(down).into_iter().partition(|d| d.release_at.is_some());
    *down = held;
    waiting.sort_by_key(|d| d.release_at);
    for d in waiting {
        if let Some(t) = d.release_at {
            let wait = t.saturating_duration_since(Instant::now());
            if !wait.is_zero() {
                std::thread::sleep(wait);
            }
        }
        out.up(&d.combo);
    }
}

#[cfg(windows)]
mod win {
    use super::Combo;

    #[repr(C)]
    #[derive(Clone, Copy)]
    struct KeybdInput {
        vk: u16,
        scan: u16,
        flags: u32,
        time: u32,
        extra: usize,
    }
    #[repr(C)]
    #[derive(Clone, Copy)]
    struct MouseInput {
        dx: i32,
        dy: i32,
        data: u32,
        flags: u32,
        time: u32,
        extra: usize,
    }
    #[repr(C)]
    union InputData {
        ki: KeybdInput,
        mi: MouseInput,
    }
    #[repr(C)]
    struct Input {
        kind: u32,
        data: InputData,
    }
    #[cfg(test)]
    pub const INPUT_SIZE: usize = std::mem::size_of::<Input>();

    #[link(name = "user32")]
    unsafe extern "system" {
        fn SendInput(count: u32, inputs: *const Input, size: i32) -> u32;
        fn MapVirtualKeyExW(code: u32, map: u32, layout: isize) -> u32;
        fn GetKeyboardLayout(thread: u32) -> isize;
        fn GetForegroundWindow() -> isize;
        fn GetWindowThreadProcessId(window: isize, pid: *mut u32) -> u32;
    }
    #[link(name = "kernel32")]
    unsafe extern "system" {
        fn OpenProcess(access: u32, inherit: i32, pid: u32) -> isize;
        fn CloseHandle(handle: isize) -> i32;
        fn GetCurrentProcess() -> isize;
        fn QueryFullProcessImageNameW(process: isize, flags: u32, name: *mut u16, size: *mut u32) -> i32;
    }
    #[link(name = "advapi32")]
    unsafe extern "system" {
        fn OpenProcessToken(process: isize, access: u32, token: *mut isize) -> i32;
        fn GetTokenInformation(token: isize, class: u32, info: *mut core::ffi::c_void, size: u32, returned: *mut u32) -> i32;
    }

    const INPUT_KEYBOARD: u32 = 1;
    const KEYEVENTF_EXTENDEDKEY: u32 = 0x0001;
    const KEYEVENTF_KEYUP: u32 = 0x0002;
    const KEYEVENTF_SCANCODE: u32 = 0x0008;
    const MAPVK_VK_TO_VSC_EX: u32 = 4;

    /// Keys a keyboard sends with an E0 prefix (arrows, the navigation block, right Ctrl/Alt,
    /// the Windows and menu keys, numpad divide, Num Lock, Print Screen, media keys), in case the
    /// layout doesn't say so.
    pub fn is_extended(vk: u16) -> bool {
        matches!(vk, 0x21..=0x28 | 0x2c | 0x2d | 0x2e | 0x5b | 0x5c | 0x5d | 0x6f | 0x90 | 0xa3 | 0xa5 | 0xad..=0xb7)
    }

    /// (scan code, extended) for a virtual key in the layout of the program in front; scan code
    /// 0 when the layout has none for it (it's then sent by its virtual-key code).
    pub fn scan_code(vk: u16) -> (u16, bool) {
        // SAFETY: plain Win32 calls with no pointers kept; a null pid pointer is allowed.
        let sc = unsafe {
            let thread = GetWindowThreadProcessId(GetForegroundWindow(), std::ptr::null_mut());
            MapVirtualKeyExW(vk as u32, MAPVK_VK_TO_VSC_EX, GetKeyboardLayout(thread))
        };
        let prefix = sc & 0xff00;
        // Pause (E1 1D 45) has no single scan code: send it by its virtual-key code.
        if prefix == 0xe100 {
            return (0, false);
        }
        ((sc & 0xff) as u16, prefix == 0xe000 || is_extended(vk))
    }

    /// One key event. The original (0x140780600, scan codes from 0x1407818e0) sends ordinary
    /// keys as virtual-key code plus scan code, and only E0 keys (media keys, the navigation
    /// block, right Ctrl/Alt) in scan-code mode. LabDeck sends every key that has a scan code in
    /// scan-code mode, so games that read scan codes see all of them (LabDeck's own rule).
    fn event(vk: u16, up: bool) -> Input {
        let (scan, extended) = scan_code(vk);
        let mut flags = if up { KEYEVENTF_KEYUP } else { 0 };
        if extended {
            flags |= KEYEVENTF_EXTENDEDKEY;
        }
        if scan != 0 {
            flags |= KEYEVENTF_SCANCODE;
        }
        Input { kind: INPUT_KEYBOARD, data: InputData { ki: KeybdInput { vk, scan, flags, time: 0, extra: 0 } } }
    }

    /// Events Windows has taken so far (SendInput's count), for the checks.
    pub static TAKEN: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);

    fn send(events: &[Input]) {
        if events.is_empty() {
            return;
        }
        // SAFETY: `events` is a valid array of INPUT structures of the size given.
        let n = unsafe { SendInput(events.len() as u32, events.as_ptr(), std::mem::size_of::<Input>() as i32) };
        TAKEN.fetch_add(n as u64, std::sync::atomic::Ordering::Relaxed);
    }

    pub struct Output;

    impl Output {
        pub fn new() -> Self {
            Output
        }
        /// Like the original's hotkey sender (0x14077e2e0): on key down the modifiers go down
        /// together in one SendInput (0x1407807c0), then the key in its own (0x140780600).
        pub fn down(&mut self, c: &Combo) {
            let mods: Vec<Input> = c.mods.iter().map(|m| event(*m, false)).collect();
            send(&mods);
            send(&[event(c.key, false)]);
        }
        /// On key up the key comes up first, then the modifiers, last one first.
        pub fn up(&mut self, c: &Combo) {
            send(&[event(c.key, true)]);
            let mods: Vec<Input> = c.mods.iter().rev().map(|m| event(*m, true)).collect();
            send(&mods);
        }
        /// A held key repeating: the key goes down again and the modifiers stay as they are.
        /// The original's repeat (0x14077e360) asks for down and up with its repeat flag, and
        /// its key sender (0x140780600) skips an up that carries that flag.
        pub fn repeat(&mut self, c: &Combo) {
            send(&[event(c.key, false)]);
        }
    }

    const PROCESS_QUERY_LIMITED_INFORMATION: u32 = 0x1000;
    const TOKEN_QUERY: u32 = 0x0008;
    const TOKEN_ELEVATION: u32 = 20;

    fn elevated(process: isize) -> Option<bool> {
        let mut token = 0isize;
        let mut value = 0u32;
        let mut len = 0u32;
        // SAFETY: the token handle is closed below; `value` is the 4-byte TOKEN_ELEVATION.
        unsafe {
            if OpenProcessToken(process, TOKEN_QUERY, &mut token) == 0 {
                return None;
            }
            let ok = GetTokenInformation(token, TOKEN_ELEVATION, &mut value as *mut u32 as *mut core::ffi::c_void, 4, &mut len);
            CloseHandle(token);
            (ok != 0).then_some(value != 0)
        }
    }

    /// The program in front, when it runs as administrator and LabDeck doesn't: Windows then
    /// drops the keys LabDeck sends to it.
    pub fn front_program_above_us() -> Option<(u32, String)> {
        // SAFETY: handles are closed; the name buffer is passed with its length.
        unsafe {
            if elevated(GetCurrentProcess()) != Some(false) {
                return None;
            }
            let mut pid = 0u32;
            GetWindowThreadProcessId(GetForegroundWindow(), &mut pid);
            if pid == 0 || pid == std::process::id() {
                return None;
            }
            let p = OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, 0, pid);
            if p == 0 {
                return None;
            }
            let above = elevated(p) == Some(true);
            let mut buf = [0u16; 520];
            let mut n = buf.len() as u32;
            let name = if QueryFullProcessImageNameW(p, 0, buf.as_mut_ptr(), &mut n) != 0 {
                let full = String::from_utf16_lossy(&buf[..n as usize]);
                full.rsplit('\\').next().unwrap_or(&full).to_string()
            } else {
                "The program in front".to_string()
            };
            CloseHandle(p);
            above.then_some((pid, name))
        }
    }
}

#[cfg(windows)]
use win::Output;
#[cfg(windows)]
pub use win::front_program_above_us;
#[cfg(all(windows, test))]
use win::scan_code;

#[cfg(not(windows))]
struct Output {
    enigo: Option<enigo::Enigo>,
}

#[cfg(not(windows))]
impl Output {
    fn new() -> Self {
        Output { enigo: enigo::Enigo::new(&enigo::Settings::default()).ok() }
    }
    fn down(&mut self, c: &Combo) {
        use enigo::{Direction::Press, Keyboard};
        if let Some(e) = self.enigo.as_mut() {
            for m in &c.mods {
                let _ = e.key(*m, Press);
            }
            let _ = e.key(c.key, Press);
        }
    }
    fn up(&mut self, c: &Combo) {
        use enigo::{Direction::Release, Keyboard};
        if let Some(e) = self.enigo.as_mut() {
            let _ = e.key(c.key, Release);
            for m in c.mods.iter().rev() {
                let _ = e.key(*m, Release);
            }
        }
    }
    fn repeat(&mut self, c: &Combo) {
        use enigo::{Direction::Press, Keyboard};
        if let Some(e) = self.enigo.as_mut() {
            let _ = e.key(c.key, Press);
        }
    }
}

/// Elsewhere there's no such check.
#[cfg(not(windows))]
pub fn front_program_above_us() -> Option<(u32, String)> {
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hotkey_entries_become_combinations() {
        let alt1 = serde_json::json!({"KeyCmd": false, "KeyCtrl": false, "KeyOption": true, "KeyShift": false, "VKeyCode": 0x31});
        let c = hotkey(&alt1).unwrap();
        #[cfg(windows)]
        assert_eq!((c.mods.clone(), c.key), (vec![VK_MENU], 0x31));
        // An empty slot (VKeyCode -1) presses nothing.
        assert!(hotkey(&serde_json::json!({"VKeyCode": -1})).is_none());
        // Modifiers in the order of their mask bits: Shift, Ctrl, Alt, Win (0x1407807c0).
        let all = serde_json::json!({"KeyCmd": true, "KeyCtrl": true, "KeyOption": true, "KeyShift": true, "VKeyCode": 0x41});
        #[cfg(windows)]
        assert_eq!(hotkey(&all).unwrap().mods, vec![VK_SHIFT, VK_CONTROL, VK_MENU, VK_LWIN]);
        assert_eq!(hotkey(&all).unwrap().mods.len(), 4);
    }

    #[cfg(windows)]
    #[test]
    fn keys_go_by_their_hardware_scan_codes() {
        // Scan codes that are the same on every common layout.
        assert_eq!(scan_code(VK_MENU), (0x38, false));
        assert_eq!(scan_code(VK_CONTROL), (0x1d, false));
        assert_eq!(scan_code(VK_SHIFT), (0x2a, false));
        assert_eq!(scan_code(0x31), (0x02, false)); // the 1 key
        assert_eq!(scan_code(0x70), (0x3b, false)); // F1
        assert_eq!(scan_code(0x25), (0x4b, true)); // left arrow, extended
        assert_eq!(scan_code(VK_LWIN), (0x5b, true));
        // The structure SendInput reads must have the size Windows expects (40 bytes on x64).
        #[cfg(target_pointer_width = "64")]
        assert_eq!(win::INPUT_SIZE, 40);
    }

    /// Watches the keys as Windows itself receives them (a low-level keyboard hook, which sees
    /// what raw-input and DirectInput games see): Shift+F23 (nothing reacts to it, and the hook
    /// keeps it from every program) must arrive with the hardware scan codes, in order, with F23
    /// held down KEY_MIN_HOLD_MS even though press and let-go were asked for at once.
    #[cfg(windows)]
    #[test]
    fn windows_receives_scan_codes_held_long_enough() {
        use std::sync::Mutex;
        #[repr(C)]
        struct KbdLl {
            vk: u32,
            scan: u32,
            flags: u32,
            time: u32,
            extra: usize,
        }
        #[link(name = "user32")]
        unsafe extern "system" {
            fn SetWindowsHookExW(id: i32, hook: extern "system" fn(i32, usize, isize) -> isize, module: isize, thread: u32) -> isize;
            fn UnhookWindowsHookEx(hook: isize) -> i32;
            fn CallNextHookEx(hook: isize, code: i32, w: usize, l: isize) -> isize;
            fn PeekMessageW(msg: *mut [u64; 8], window: isize, min: u32, max: u32, remove: u32) -> i32;
        }
        #[link(name = "kernel32")]
        unsafe extern "system" {
            fn GetModuleHandleW(name: *const u16) -> isize;
        }
        // (scan code, flags, message, when)
        static SEEN: Mutex<Vec<(u32, u32, usize, Instant)>> = Mutex::new(Vec::new());
        static WATCH: Mutex<Vec<u32>> = Mutex::new(Vec::new());
        extern "system" fn hook(code: i32, w: usize, l: isize) -> isize {
            if code >= 0 {
                // SAFETY: for WH_KEYBOARD_LL, l points to a KBDLLHOOKSTRUCT.
                let k = unsafe { &*(l as *const KbdLl) };
                if WATCH.lock().unwrap().contains(&k.scan) && k.flags & 0x10 != 0 {
                    SEEN.lock().unwrap().push((k.scan, k.flags, w, Instant::now()));
                    return 1; // keep it from every program
                }
            }
            // SAFETY: passing the event on unchanged.
            unsafe { CallNextHookEx(0, code, w, l) }
        }
        let (shift_sc, _) = scan_code(VK_SHIFT);
        let (f23_sc, _) = scan_code(0x86);
        assert!(shift_sc != 0 && f23_sc != 0);
        *WATCH.lock().unwrap() = vec![shift_sc as u32, f23_sc as u32];

        let (ready_tx, ready_rx) = mpsc::channel();
        let watcher = std::thread::spawn(move || {
            // SAFETY: the hook is removed before the thread ends; messages are pumped meanwhile.
            unsafe {
                let h = SetWindowsHookExW(13, hook, GetModuleHandleW(std::ptr::null()), 0);
                let _ = ready_tx.send(h != 0);
                let end = Instant::now() + Duration::from_millis(800);
                let mut msg = [0u64; 8];
                while Instant::now() < end {
                    while PeekMessageW(&mut msg, 0, 0, 0, 1) != 0 {}
                    std::thread::sleep(Duration::from_millis(1));
                }
                UnhookWindowsHookEx(h);
            }
        });
        assert!(ready_rx.recv().unwrap(), "could not watch the keyboard");
        std::thread::sleep(Duration::from_millis(50));
        let taken_before = win::TAKEN.load(std::sync::atomic::Ordering::Relaxed);
        tap(Combo::from_vk(&[VK_SHIFT], 0x86).unwrap());
        let_go_all();
        watcher.join().unwrap();
        let taken = win::TAKEN.load(std::sync::atomic::Ordering::Relaxed) - taken_before;

        let seen = SEEN.lock().unwrap().clone();
        // Windows took all four events, but none reached even the keyboard hook: a program in
        // front is throwing away keys sent by programs (some games' anti-cheat does this while
        // the game has focus). Nothing LabDeck sends can get through then, so there's nothing
        // to check about the order or the timing.
        if seen.is_empty() && taken >= 4 {
            eprintln!("skipped: the program in front throws away keys sent by programs ({taken} events taken, none seen)");
            return;
        }
        let order: Vec<(u32, bool)> = seen.iter().map(|(sc, flags, _, _)| (*sc, flags & 0x80 != 0)).collect();
        // Shift down, F23 down, F23 up, Shift up — by scan code, as a keyboard sends them.
        assert_eq!(
            order,
            vec![(shift_sc as u32, false), (f23_sc as u32, false), (f23_sc as u32, true), (shift_sc as u32, true)],
            "{seen:?}"
        );
        let held = seen[2].3 - seen[1].3;
        assert!(held >= Duration::from_millis(KEY_MIN_HOLD_MS - 5), "held {held:?}");
    }

    #[test]
    fn short_presses_are_held_long_enough() {
        // A press and let-go sent at the same moment still keep the key down KEY_MIN_HOLD_MS:
        // the let-go only takes effect once that time has passed (checked through the queue,
        // with a key no program reacts to: F24 with no modifiers).
        let c = Combo::from_vk(&[], 0x87).unwrap();
        let t = Instant::now();
        tap(c);
        let_go_all();
        assert!(t.elapsed() >= Duration::from_millis(KEY_MIN_HOLD_MS), "{:?}", t.elapsed());
    }
}
