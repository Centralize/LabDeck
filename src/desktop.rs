//! Starting LabDeck like any other desktop program, without a terminal:
//!
//! - its own icon on the window and in the taskbar or dock;
//! - on Linux, an entry in the app menu that LabDeck writes itself at start, so after the first
//!   start it opens from the menu (and pins to the dock) like other apps;
//! - on Linux, a start from a terminal hands over to a copy of LabDeck of its own, so the
//!   terminal comes back at once and can be closed without stopping LabDeck;
//! - on Windows, no console window, while the options that print (`--help`...) still print
//!   into the terminal they were typed in;
//! - the "Start LabDeck when I log in" switch (Windows, Linux and macOS).
//!
//! None of this touches Elgato's Stream Deck app or its folders.

use std::path::{Path, PathBuf};

/// The name the desktop knows LabDeck by: the Linux menu entry is `labdeck.desktop`, and the
/// window's app id (Wayland) and class (X11) are the same, so the dock shows its icon.
pub const APP_ID: &str = "labdeck";

/// The option a background start (autostart) is given.
pub const BACKGROUND_ARG: &str = "--background";

/// LabDeck's icon, `px` × `px` RGBA: the purple tile with a 3×3 key grid (also the tray icon).
pub fn icon_rgba(px: u32) -> Vec<u8> {
    crate::tray::badge(px)
}

/// The window's icon (title bar, taskbar, Alt+Tab, dock).
pub fn window_icon() -> egui::IconData {
    let px = 256;
    egui::IconData { rgba: icon_rgba(px), width: px, height: px }
}

/// Windows: LabDeck is a window program, so it has no console of its own. When it's started from
/// a terminal with an option that prints, it writes into that terminal.
pub fn attach_terminal() {
    #[cfg(target_os = "windows")]
    {
        #[link(name = "kernel32")]
        unsafe extern "system" {
            safe fn AttachConsole(process: u32) -> i32;
        }
        const ATTACH_PARENT_PROCESS: u32 = u32::MAX;
        let _ = AttachConsole(ATTACH_PARENT_PROCESS);
    }
}

/// Linux: started from a terminal, start a copy of LabDeck in a session of its own (no terminal;
/// what it prints goes to `Logs/labdeck.log` in LabDeck's folder) and say so. True when that copy
/// is running and this one should end. Started from the menu, by autostart or with `--foreground`,
/// nothing happens (false).
pub fn hand_over_terminal() -> bool {
    #[cfg(target_os = "linux")]
    {
        linux_hand_over()
    }
    #[cfg(not(target_os = "linux"))]
    {
        false
    }
}

#[cfg(target_os = "linux")]
fn linux_hand_over() -> bool {
    use std::io::IsTerminal;
    use std::os::unix::process::CommandExt;
    use std::process::{Command, Stdio};
    if std::env::var_os(FOREGROUND_ENV).is_some() {
        return false;
    }
    if !(std::io::stdin().is_terminal() || std::io::stdout().is_terminal() || std::io::stderr().is_terminal()) {
        return false;
    }
    let Ok(exe) = std::env::current_exe() else { return false };
    let dir = crate::profile::data_dir().join("Logs");
    let _ = std::fs::create_dir_all(&dir);
    let (out, err) = match std::fs::File::create(dir.join("labdeck.log")) {
        Ok(f) => match f.try_clone() {
            Ok(g) => (Stdio::from(f), Stdio::from(g)),
            Err(_) => (Stdio::from(f), Stdio::null()),
        },
        Err(_) => (Stdio::null(), Stdio::null()),
    };
    let mut cmd = Command::new(&exe);
    cmd.args(std::env::args_os().skip(1)).env(FOREGROUND_ENV, "1").stdin(Stdio::null()).stdout(out).stderr(err);
    unsafe extern "C" {
        safe fn setsid() -> i32;
    }
    // SAFETY: setsid is async-signal-safe and touches nothing in this process's memory.
    unsafe {
        cmd.pre_exec(|| {
            setsid();
            Ok(())
        });
    }
    match cmd.spawn() {
        Ok(_) => {
            println!("LabDeck is running. You can close this terminal.");
            println!("From now on you can also start LabDeck from your app menu.");
            true
        }
        Err(_) => false,
    }
}

/// Set for the copy a terminal start handed over to (and by `--foreground`): stay in this process.
#[allow(dead_code)]
pub const FOREGROUND_ENV: &str = "LABDECK_FOREGROUND";

/// Linux: write LabDeck's app menu entry and its icon (only when they're missing or different, for
/// example after LabDeck was moved), and keep the "start when I log in" entry pointing at this
/// program. Nothing on other systems.
pub fn add_to_app_menu() {
    #[cfg(target_os = "linux")]
    {
        let Ok(exe) = std::env::current_exe() else { return };
        let icon = crate::profile::data_dir().join("labdeck.png");
        if !icon.exists() {
            let _ = std::fs::create_dir_all(crate::profile::data_dir());
            let px = 256;
            let _ = image::save_buffer(&icon, &icon_rgba(px), px, px, image::ColorType::Rgba8);
        }
        write_if_changed(&linux_menu_file(), &desktop_entry(&exe, &icon, false));
        let auto = linux_autostart_file();
        if auto.exists() {
            write_if_changed(&auto, &desktop_entry(&exe, &icon, true));
        }
    }
}

#[cfg(target_os = "linux")]
fn write_if_changed(path: &Path, text: &str) {
    if std::fs::read_to_string(path).is_ok_and(|t| t == text) {
        return;
    }
    if let Some(dir) = path.parent() {
        let _ = std::fs::create_dir_all(dir);
    }
    let _ = std::fs::write(path, text);
}

#[allow(dead_code)]
fn home() -> PathBuf {
    PathBuf::from(std::env::var("HOME").unwrap_or_else(|_| ".".into()))
}

/// `$XDG_DATA_HOME/applications/labdeck.desktop` (normally `~/.local/share/applications`).
#[allow(dead_code)]
fn linux_menu_file() -> PathBuf {
    let base = std::env::var("XDG_DATA_HOME").map(PathBuf::from).unwrap_or_else(|_| home().join(".local/share"));
    base.join("applications").join(format!("{APP_ID}.desktop"))
}

/// `$XDG_CONFIG_HOME/autostart/labdeck.desktop` (normally `~/.config/autostart`).
#[allow(dead_code)]
fn linux_autostart_file() -> PathBuf {
    let base = std::env::var("XDG_CONFIG_HOME").map(PathBuf::from).unwrap_or_else(|_| home().join(".config"));
    base.join("autostart").join(format!("{APP_ID}.desktop"))
}

/// `~/Library/LaunchAgents/com.labdeck.LabDeck.plist`.
#[allow(dead_code)]
fn mac_launch_agent() -> PathBuf {
    home().join("Library/LaunchAgents/com.labdeck.LabDeck.plist")
}

/// A desktop entry (freedesktop.org format) that starts `exe`: the app menu's, or the autostart
/// one, which starts LabDeck in the tray.
#[allow(dead_code)]
pub fn desktop_entry(exe: &Path, icon: &Path, autostart: bool) -> String {
    let mut exec = exec_word(&exe.to_string_lossy());
    if autostart {
        exec.push(' ');
        exec.push_str(BACKGROUND_ARG);
    }
    let mut t = String::from("[Desktop Entry]\nType=Application\nVersion=1.0\nName=LabDeck\n");
    t.push_str("GenericName=Stream Deck setup\nComment=Set up the keys of your Stream Deck and run them\n");
    t.push_str(&format!("Exec={exec}\nIcon={}\nTerminal=false\n", escape_value(&icon.to_string_lossy())));
    t.push_str(&format!("StartupWMClass={APP_ID}\nStartupNotify=true\nCategories=Utility;\nKeywords=stream;deck;keys;hotkey;macro;\n"));
    if autostart {
        t.push_str("X-GNOME-Autostart-enabled=true\n");
    }
    t
}

/// One word of an `Exec` line: quoted when it has characters other than plain path ones, with the
/// spec's escapes (`"`, `` ` ``, `$` and `\` inside quotes, `%` doubled, then `\` doubled again
/// as for every string value).
#[allow(dead_code)]
fn exec_word(s: &str) -> String {
    let plain = !s.is_empty() && s.chars().all(|c| c.is_ascii_alphanumeric() || "/._-+,:@".contains(c));
    let word = if plain {
        s.to_string()
    } else {
        let mut q = String::from("\"");
        for c in s.chars() {
            if matches!(c, '"' | '`' | '$' | '\\') {
                q.push('\\');
            }
            q.push(c);
        }
        q.push('"');
        q
    };
    escape_value(&word).replace('%', "%%")
}

/// A desktop entry string value: backslash, new line, tab and carriage return escaped.
#[allow(dead_code)]
fn escape_value(s: &str) -> String {
    s.replace('\\', "\\\\").replace('\n', "\\n").replace('\t', "\\t").replace('\r', "\\r")
}

/// macOS: the launch agent that starts LabDeck in the tray when the user logs in.
#[allow(dead_code)]
fn launch_agent(exe: &Path) -> String {
    let x = |s: &str| s.replace('&', "&amp;").replace('<', "&lt;").replace('>', "&gt;");
    format!(
        "<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n<!DOCTYPE plist PUBLIC \"-//Apple//DTD PLIST 1.0//EN\" \"http://www.apple.com/DTDs/PropertyList-1.0.dtd\">\n<plist version=\"1.0\">\n<dict>\n  <key>Label</key>\n  <string>com.labdeck.LabDeck</string>\n  <key>ProgramArguments</key>\n  <array>\n    <string>{}</string>\n    <string>{BACKGROUND_ARG}</string>\n  </array>\n  <key>RunAtLoad</key>\n  <true/>\n  <key>ProcessType</key>\n  <string>Interactive</string>\n</dict>\n</plist>\n",
        x(&exe.to_string_lossy())
    )
}

/// Windows: the command the Run key holds, `"C:\…\labdeck.exe" --background`.
#[allow(dead_code)]
fn run_command(exe: &Path) -> String {
    format!("\"{}\" {BACKGROUND_ARG}", exe.display())
}

#[cfg(target_os = "windows")]
mod run_key {
    //! `HKEY_CURRENT_USER\Software\Microsoft\Windows\CurrentVersion\Run`, value `LabDeck`.
    #[link(name = "advapi32")]
    unsafe extern "system" {
        fn RegSetKeyValueW(key: isize, sub: *const u16, value: *const u16, ty: u32, data: *const u8, len: u32) -> i32;
        fn RegDeleteKeyValueW(key: isize, sub: *const u16, value: *const u16) -> i32;
        fn RegGetValueW(key: isize, sub: *const u16, value: *const u16, flags: u32, ty: *mut u32, data: *mut u8, len: *mut u32) -> i32;
    }
    const HKEY_CURRENT_USER: isize = 0x8000_0001u32 as i32 as isize;
    const REG_SZ: u32 = 1;
    const RRF_RT_REG_SZ: u32 = 0x2;
    const ERROR_FILE_NOT_FOUND: i32 = 2;
    const SUB: &str = "Software\\Microsoft\\Windows\\CurrentVersion\\Run";
    const NAME: &str = "LabDeck";

    fn wide(s: &str) -> Vec<u16> {
        s.encode_utf16().chain(std::iter::once(0)).collect()
    }

    /// The command saved under LabDeck's name, if any.
    pub fn get() -> Option<String> {
        get_named(NAME)
    }

    pub fn set(command: &str) -> Result<(), String> {
        set_named(NAME, command)
    }

    pub fn remove() -> Result<(), String> {
        remove_named(NAME)
    }

    pub(super) fn get_named(name: &str) -> Option<String> {
        let (sub, name) = (wide(SUB), wide(name));
        let mut buf = vec![0u16; 2048];
        let mut len = (buf.len() * 2) as u32;
        // SAFETY: the pointers are valid for the lengths given.
        let r = unsafe { RegGetValueW(HKEY_CURRENT_USER, sub.as_ptr(), name.as_ptr(), RRF_RT_REG_SZ, std::ptr::null_mut(), buf.as_mut_ptr().cast(), &mut len) };
        if r != 0 {
            return None;
        }
        let n = (len as usize / 2).min(buf.len());
        Some(String::from_utf16_lossy(&buf[..n]).trim_end_matches('\0').to_string())
    }

    pub(super) fn set_named(name: &str, command: &str) -> Result<(), String> {
        let (sub, name, data) = (wide(SUB), wide(name), wide(command));
        // SAFETY: the pointers are valid for the lengths given.
        let r = unsafe { RegSetKeyValueW(HKEY_CURRENT_USER, sub.as_ptr(), name.as_ptr(), REG_SZ, data.as_ptr().cast(), (data.len() * 2) as u32) };
        if r == 0 { Ok(()) } else { Err(format!("Windows refused it (error {r})")) }
    }

    pub(super) fn remove_named(name: &str) -> Result<(), String> {
        let (sub, name) = (wide(SUB), wide(name));
        // SAFETY: the pointers are valid, null-terminated strings.
        let r = unsafe { RegDeleteKeyValueW(HKEY_CURRENT_USER, sub.as_ptr(), name.as_ptr()) };
        if r == 0 || r == ERROR_FILE_NOT_FOUND { Ok(()) } else { Err(format!("Windows refused it (error {r})")) }
    }
}

/// Whether LabDeck starts (in the tray) when the user logs in.
pub fn starts_at_login() -> bool {
    #[cfg(target_os = "windows")]
    return run_key::get().is_some();
    #[cfg(target_os = "macos")]
    return mac_launch_agent().exists();
    #[cfg(not(any(target_os = "windows", target_os = "macos")))]
    return linux_autostart_file().exists();
}

/// Turn starting at login on (pointing at this program) or off.
pub fn set_start_at_login(on: bool) -> Result<(), String> {
    let exe = std::env::current_exe().map_err(|e| format!("can't tell where LabDeck is ({e})"))?;
    #[cfg(target_os = "windows")]
    {
        if on { run_key::set(&run_command(&exe)) } else { run_key::remove() }
    }
    #[cfg(not(target_os = "windows"))]
    {
        #[cfg(target_os = "macos")]
        let (path, text) = (mac_launch_agent(), launch_agent(&exe));
        #[cfg(not(target_os = "macos"))]
        let (path, text) = (linux_autostart_file(), desktop_entry(&exe, &crate::profile::data_dir().join("labdeck.png"), true));
        if on {
            if let Some(dir) = path.parent() {
                std::fs::create_dir_all(dir).map_err(|e| e.to_string())?;
            }
            std::fs::write(&path, text).map_err(|e| e.to_string())
        } else {
            match std::fs::remove_file(&path) {
                Err(e) if e.kind() != std::io::ErrorKind::NotFound => Err(e.to_string()),
                _ => Ok(()),
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn menu_entry_starts_labdeck_without_a_terminal() {
        let t = desktop_entry(Path::new("/home/sam/.local/bin/labdeck"), Path::new("/home/sam/.local/share/LabDeck/labdeck.png"), false);
        assert!(t.starts_with("[Desktop Entry]\n"));
        assert!(t.contains("\nExec=/home/sam/.local/bin/labdeck\n"));
        assert!(t.contains("\nIcon=/home/sam/.local/share/LabDeck/labdeck.png\n"));
        assert!(t.contains("\nTerminal=false\n"));
        assert!(t.contains(&format!("\nStartupWMClass={APP_ID}\n")));
        assert!(!t.contains(BACKGROUND_ARG));
        // The autostart entry starts it in the tray.
        let a = desktop_entry(Path::new("/opt/lab deck/labdeck"), Path::new("/i.png"), true);
        assert!(a.contains("\nExec=\"/opt/lab deck/labdeck\" --background\n"), "{a}");
    }

    #[test]
    fn exec_words_follow_the_desktop_entry_rules() {
        assert_eq!(exec_word("/usr/bin/labdeck"), "/usr/bin/labdeck");
        assert_eq!(exec_word("/a b/c"), "\"/a b/c\"");
        // `$` gets a backslash inside quotes, and that backslash is doubled as a string value.
        assert_eq!(exec_word("/a$b"), "\"/a\\\\$b\"");
        assert_eq!(exec_word("/100%/x"), "\"/100%%/x\"");
    }

    #[test]
    fn login_start_commands_point_at_this_program() {
        assert_eq!(run_command(Path::new("C:\\Tools\\labdeck.exe")), "\"C:\\Tools\\labdeck.exe\" --background");
        let p = launch_agent(Path::new("/Applications/LabDeck & Co/labdeck"));
        assert!(p.contains("<string>/Applications/LabDeck &amp; Co/labdeck</string>"));
        assert!(p.contains("<string>--background</string>"));
        assert!(p.contains("<key>RunAtLoad</key>\n  <true/>"));
    }

    /// Windows: the Run key is written, read back and removed again (under a test name, so
    /// LabDeck's own entry is never touched).
    #[cfg(target_os = "windows")]
    #[test]
    fn windows_login_start_round_trip() {
        let name = format!("LabDeckTest{}", std::process::id());
        let cmd = run_command(Path::new("C:\\Program Files\\LabDeck\\labdeck.exe"));
        assert_eq!(run_key::get_named(&name), None);
        run_key::set_named(&name, &cmd).unwrap();
        assert_eq!(run_key::get_named(&name).as_deref(), Some(cmd.as_str()));
        run_key::remove_named(&name).unwrap();
        assert_eq!(run_key::get_named(&name), None);
        // Removing what isn't there is fine (switching off twice).
        run_key::remove_named(&name).unwrap();
    }
}
