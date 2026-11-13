//! Plugins: LabDeck runs the plugins in its own `Plugins` folder (one `*.sdPlugin` folder each,
//! with a `manifest.json`) and talks to them with the standard plugin protocol: JSON messages
//! such as `keyDown`, `willAppear` or `setTitle`.
//!
//! - Program plugins (a Rust program, or any other program) and JavaScript / TypeScript plugins
//!   (run with Node.js) are started with `-port <port> -pluginUUID <uuid> -registerEvent
//!   registerPlugin -info <json>` (the argument order of 0x140212790) and connect back to a
//!   WebSocket on 127.0.0.1.
//! - Lua plugins run inside LabDeck: the script's global functions named after events are
//!   called, and the `labdeck` table sends messages back.
//!
//! See Plugins.md for writing plugins.

use serde_json::{Value, json};
use std::collections::{HashMap, HashSet};
use std::net::{TcpListener, TcpStream};
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::sync::mpsc::{Receiver, RecvTimeoutError, Sender, channel};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

/// A plugin found in a Plugins folder.
#[derive(Clone, Debug)]
pub struct PluginInfo {
    pub uuid: String,
    pub name: String,
    pub version: String,
    pub dir: PathBuf,
    /// The file to run: `CodePathWin` / `CodePathMac` / `CodePathLin` for this system, else `CodePath`.
    pub code: Option<PathBuf>,
    pub actions: Vec<String>,
    /// Actions whose manifest sets `DisableAutomaticStates`: LabDeck doesn't flip their state.
    pub manual_states: HashSet<String>,
    /// Actions whose manifest sets `SupportedInMultiActions: false`.
    pub not_in_multi: HashSet<String>,
}

/// How a plugin's code is run.
#[derive(Debug, PartialEq)]
pub enum Kind {
    Program,
    Node,
    Lua,
    Unsupported(&'static str),
}

pub fn kind_of(code: &Path) -> Kind {
    match code.extension().and_then(|e| e.to_str()).map(|e| e.to_ascii_lowercase()).as_deref() {
        Some("js" | "mjs" | "cjs" | "ts" | "mts" | "cts") => Kind::Node,
        Some("lua") => Kind::Lua,
        Some("html" | "htm") => Kind::Unsupported("plugins written as a web page need a browser, which LabDeck doesn't have"),
        _ => Kind::Program,
    }
}

/// Reads one plugin folder's manifest.json (None if it can't be read, e.g. an encrypted one).
pub fn read_info(dir: &Path) -> Option<PluginInfo> {
    let text = std::fs::read_to_string(dir.join("manifest.json")).ok()?;
    let v: Value = serde_json::from_str(text.trim_start_matches('\u{feff}')).ok()?;
    let stem = dir.file_stem()?.to_string_lossy().to_string();
    let s = |k: &str| v.get(k).and_then(|x| x.as_str()).map(str::to_string);
    let uuid = s("UUID").unwrap_or(stem);
    let own = if cfg!(target_os = "windows") {
        "CodePathWin"
    } else if cfg!(target_os = "macos") {
        "CodePathMac"
    } else {
        "CodePathLin"
    };
    let code = s(own).or_else(|| s("CodePath")).map(|c| dir.join(c));
    let mut actions = Vec::new();
    let mut manual_states = HashSet::new();
    let mut not_in_multi = HashSet::new();
    for a in v.get("Actions").and_then(|a| a.as_array()).into_iter().flatten() {
        if let Some(u) = a.get("UUID").and_then(|u| u.as_str()) {
            actions.push(u.to_string());
            if a.get("DisableAutomaticStates").and_then(|b| b.as_bool()) == Some(true) {
                manual_states.insert(u.to_string());
            }
            if a.get("SupportedInMultiActions").and_then(|b| b.as_bool()) == Some(false) {
                not_in_multi.insert(u.to_string());
            }
        }
    }
    Some(PluginInfo {
        name: s("Name").unwrap_or_else(|| uuid.clone()),
        version: s("Version").unwrap_or_default(),
        uuid,
        dir: dir.to_path_buf(),
        code,
        actions,
        manual_states,
        not_in_multi,
    })
}

/// Every readable plugin in a folder.
pub fn discover(dir: &Path) -> Vec<PluginInfo> {
    let Ok(rd) = std::fs::read_dir(dir) else { return Vec::new() };
    let mut v: Vec<PluginInfo> = rd
        .flatten()
        .map(|e| e.path())
        .filter(|p| p.extension().is_some_and(|x| x == "sdPlugin"))
        .filter_map(|p| read_info(&p))
        .collect();
    v.sort_by(|a, b| a.uuid.cmp(&b.uuid));
    v
}

/// What a plugin did: connected, sent a message, or stopped.
pub enum FromPlugin {
    Registered(String),
    Message(String, Value),
    Gone(String, String),
}

type Outboxes = Arc<Mutex<HashMap<String, Sender<String>>>>;

pub struct Host {
    /// The WebSocket port plugins connect to (passed as `-port`).
    #[allow(dead_code)]
    pub port: u16,
    pub plugins: Vec<PluginInfo>,
    /// Plugins connected right now.
    pub running: HashSet<String>,
    /// Why a plugin isn't running (it couldn't start, it stopped, …).
    pub problems: HashMap<String, String>,
    rx: Receiver<FromPlugin>,
    out: Outboxes,
    children: Vec<Child>,
    action_plugin: HashMap<String, String>,
}

impl Drop for Host {
    fn drop(&mut self) {
        self.stop();
    }
}

impl Host {
    /// Opens the WebSocket on 127.0.0.1 (a free port) and starts every plugin. `info` is the
    /// `-info` JSON for a plugin; `wake` is called whenever a plugin has sent something.
    pub fn start(plugins: Vec<PluginInfo>, info: impl Fn(&PluginInfo) -> Value, wake: impl Fn() + Send + Sync + 'static) -> Option<Host> {
        let listener = TcpListener::bind("127.0.0.1:0").ok()?;
        let port = listener.local_addr().ok()?.port();
        let (tx, rx) = channel();
        let out: Outboxes = Arc::new(Mutex::new(HashMap::new()));
        let wake: Arc<dyn Fn() + Send + Sync> = Arc::new(wake);
        {
            let (tx, out, wake) = (tx.clone(), out.clone(), wake.clone());
            std::thread::Builder::new()
                .name("labdeck-plugins".into())
                .spawn(move || {
                    for stream in listener.incoming().flatten() {
                        let (tx, out, wake) = (tx.clone(), out.clone(), wake.clone());
                        std::thread::spawn(move || serve(stream, tx, out, wake));
                    }
                })
                .ok()?;
        }
        let mut host = Host {
            port,
            action_plugin: plugins.iter().flat_map(|p| p.actions.iter().map(|a| (a.clone(), p.uuid.clone()))).collect(),
            plugins,
            running: HashSet::new(),
            problems: HashMap::new(),
            rx,
            out,
            children: Vec::new(),
        };
        for p in host.plugins.clone() {
            let Some(code) = p.code.clone() else {
                host.problems.insert(p.uuid.clone(), "its manifest names no CodePath".into());
                continue;
            };
            match kind_of(&code) {
                Kind::Unsupported(why) => {
                    host.problems.insert(p.uuid.clone(), why.into());
                }
                Kind::Lua => {
                    let (otx, orx) = channel();
                    host.out.lock().unwrap().insert(p.uuid.clone(), otx);
                    let (tx, wake, p2) = (tx.clone(), wake.clone(), p.clone());
                    std::thread::spawn(move || run_lua(p2, code, tx, orx, wake));
                }
                kind => match start_process(&p, &code, kind, port, &info(&p)) {
                    Ok(child) => host.children.push(child),
                    Err(e) => {
                        host.problems.insert(p.uuid.clone(), e);
                    }
                },
            }
        }
        Some(host)
    }

    /// Everything plugins sent since the last call.
    pub fn drain(&mut self) -> Vec<FromPlugin> {
        let v: Vec<FromPlugin> = self.rx.try_iter().collect();
        for m in &v {
            match m {
                FromPlugin::Registered(u) => {
                    self.running.insert(u.clone());
                    self.problems.remove(u);
                }
                FromPlugin::Gone(u, why) => {
                    self.running.remove(u);
                    self.problems.insert(u.clone(), why.clone());
                }
                FromPlugin::Message(..) => {}
            }
        }
        v
    }

    /// The plugin an action belongs to, when that plugin is running.
    pub fn running_plugin_of(&self, action: &str) -> Option<&str> {
        self.action_plugin.get(action).map(String::as_str).filter(|p| self.running.contains(*p))
    }

    pub fn plugin_of(&self, action: &str) -> Option<&str> {
        self.action_plugin.get(action).map(String::as_str)
    }

    /// False for a plugin action whose manifest says `SupportedInMultiActions: false`.
    pub fn allows_in_multi(&self, action: &str) -> bool {
        !self.plugins.iter().any(|p| p.not_in_multi.contains(action))
    }

    pub fn info(&self, plugin: &str) -> Option<&PluginInfo> {
        self.plugins.iter().find(|p| p.uuid == plugin)
    }

    /// Send one protocol message to a plugin.
    pub fn send(&self, plugin: &str, msg: &Value) {
        if let Some(tx) = self.out.lock().unwrap().get(plugin) {
            let _ = tx.send(msg.to_string());
        }
    }

    pub fn send_all(&self, msg: &Value) {
        for tx in self.out.lock().unwrap().values() {
            let _ = tx.send(msg.to_string());
        }
    }

    /// Stop every plugin program.
    pub fn stop(&mut self) {
        for c in &mut self.children {
            let _ = c.kill();
            let _ = c.wait();
        }
        self.children.clear();
        self.out.lock().unwrap().clear();
    }
}

/// The Node.js program: `LABDECK_NODE` if set, else `node` from the PATH.
fn node_program() -> String {
    std::env::var("LABDECK_NODE").ok().filter(|s| !s.trim().is_empty()).unwrap_or_else(|| "node".into())
}

fn start_process(p: &PluginInfo, code: &Path, kind: Kind, port: u16, info: &Value) -> Result<Child, String> {
    let mut code = code.to_path_buf();
    if kind == Kind::Program && !code.exists() && cfg!(target_os = "windows") {
        let exe = code.with_extension("exe");
        if exe.exists() {
            code = exe;
        }
    }
    if !code.exists() {
        return Err(format!("{} wasn't found", code.display()));
    }
    let mut cmd = match kind {
        Kind::Node => {
            let mut c = Command::new(node_program());
            c.arg(&code);
            c
        }
        _ => Command::new(&code),
    };
    cmd.args(["-port", &port.to_string(), "-pluginUUID", &p.uuid, "-registerEvent", "registerPlugin", "-info", &info.to_string()]);
    cmd.current_dir(&p.dir).stdin(Stdio::null());
    // The plugin's printed output goes to its log file in LabDeck's Logs folder.
    let log = log_dir().join(format!("{}.log", p.uuid));
    let _ = std::fs::create_dir_all(log_dir());
    match std::fs::OpenOptions::new().create(true).append(true).open(&log) {
        Ok(f) => {
            let f2 = f.try_clone().map(Stdio::from).unwrap_or_else(|_| Stdio::null());
            cmd.stdout(Stdio::from(f)).stderr(f2);
        }
        Err(_) => {
            cmd.stdout(Stdio::null()).stderr(Stdio::null());
        }
    }
    #[cfg(target_os = "windows")]
    {
        // No console window for each plugin program.
        use std::os::windows::process::CommandExt;
        cmd.creation_flags(0x0800_0000);
    }
    cmd.spawn().map_err(|e| match kind {
        Kind::Node => format!("Node.js couldn't be started ({e}); install it from nodejs.org"),
        _ => format!("{} couldn't be started: {e}", code.display()),
    })
}

/// One plugin's WebSocket: the first message registers it, then messages go both ways.
fn serve(stream: TcpStream, tx: Sender<FromPlugin>, out: Outboxes, wake: Arc<dyn Fn() + Send + Sync>) {
    use tungstenite::Message;
    let Ok(mut ws) = tungstenite::accept(stream) else { return };
    let uuid = loop {
        match ws.read() {
            Ok(Message::Text(t)) => {
                let v: Value = serde_json::from_str(&t).unwrap_or(Value::Null);
                match v.get("event").and_then(|e| e.as_str()) {
                    Some("registerPlugin") => break v.get("uuid").and_then(|u| u.as_str()).unwrap_or_default().to_string(),
                    // Property inspectors are web pages; LabDeck has none to connect.
                    _ => return,
                }
            }
            Ok(Message::Close(_)) | Err(_) => return,
            Ok(_) => {}
        }
    };
    if uuid.is_empty() {
        return;
    }
    let _ = ws.get_ref().set_read_timeout(Some(Duration::from_millis(15)));
    let (otx, orx) = channel::<String>();
    out.lock().unwrap().insert(uuid.clone(), otx);
    let _ = tx.send(FromPlugin::Registered(uuid.clone()));
    wake();
    let why = 'outer: loop {
        match ws.read() {
            Ok(Message::Text(t)) => {
                if let Ok(v) = serde_json::from_str::<Value>(&t) {
                    let _ = tx.send(FromPlugin::Message(uuid.clone(), v));
                    wake();
                }
            }
            Ok(Message::Close(_)) => break "it closed its connection".to_string(),
            Ok(_) => {}
            Err(tungstenite::Error::Io(e)) if matches!(e.kind(), std::io::ErrorKind::WouldBlock | std::io::ErrorKind::TimedOut) => {}
            Err(e) => break format!("its connection ended ({e})"),
        }
        while let Ok(text) = orx.try_recv() {
            if let Err(e) = ws.send(Message::text(text)) {
                break 'outer format!("its connection ended ({e})");
            }
        }
    };
    out.lock().unwrap().remove(&uuid);
    let _ = tx.send(FromPlugin::Gone(uuid, format!("it stopped: {why}")));
    wake();
}

/// Lua helpers on top of `labdeck.send`, one per command.
const LUA_PRELUDE: &str = r#"
function labdeck.setTitle(context, title, state) labdeck.send({event = "setTitle", context = context, payload = {title = title, state = state, target = 0}}) end
function labdeck.setImage(context, image, state) labdeck.send({event = "setImage", context = context, payload = {image = image, state = state, target = 0}}) end
function labdeck.setState(context, state) labdeck.send({event = "setState", context = context, payload = {state = state}}) end
function labdeck.showOk(context) labdeck.send({event = "showOk", context = context}) end
function labdeck.showAlert(context) labdeck.send({event = "showAlert", context = context}) end
function labdeck.setSettings(context, settings) labdeck.send({event = "setSettings", context = context, payload = settings}) end
function labdeck.getSettings(context) labdeck.send({event = "getSettings", context = context}) end
function labdeck.setGlobalSettings(settings) labdeck.send({event = "setGlobalSettings", context = labdeck.uuid, payload = settings}) end
function labdeck.getGlobalSettings() labdeck.send({event = "getGlobalSettings", context = labdeck.uuid}) end
function labdeck.openUrl(url) labdeck.send({event = "openUrl", payload = {url = url}}) end
function labdeck.log(message) labdeck.send({event = "logMessage", payload = {message = tostring(message)}}) end
"#;

/// A Lua plugin, on its own thread: events arrive as JSON text and call the script's global
/// function of the same name with the message as a table; `tick(seconds)` is called every second
/// when the script defines it.
fn run_lua(p: PluginInfo, code: PathBuf, tx: Sender<FromPlugin>, inbox: Receiver<String>, wake: Arc<dyn Fn() + Send + Sync>) {
    use mlua::LuaSerdeExt;
    let lua = mlua::Lua::new();
    let uuid = p.uuid.clone();
    let setup = || -> mlua::Result<()> {
        let t = lua.create_table()?;
        t.set("uuid", uuid.clone())?;
        t.set("folder", p.dir.to_string_lossy().to_string())?;
        let (tx1, u1, w1) = (tx.clone(), uuid.clone(), wake.clone());
        t.set(
            "send",
            lua.create_function(move |lua, msg: mlua::Value| {
                let v: Value = lua.from_value(msg)?;
                let _ = tx1.send(FromPlugin::Message(u1.clone(), v));
                w1();
                Ok(())
            })?,
        )?;
        lua.globals().set("labdeck", t)?;
        lua.load(LUA_PRELUDE).set_name("labdeck").exec()?;
        let src = std::fs::read_to_string(&code).map_err(mlua::Error::external)?;
        let name = code.file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or_default();
        lua.load(src).set_name(name).exec()
    };
    if let Err(e) = setup() {
        let _ = tx.send(FromPlugin::Gone(uuid, format!("its Lua script failed: {e}")));
        wake();
        return;
    }
    let _ = tx.send(FromPlugin::Registered(uuid.clone()));
    wake();
    let log_error = |e: mlua::Error| {
        let _ = tx.send(FromPlugin::Message(uuid.clone(), json!({"event": "logMessage", "payload": {"message": format!("Lua error: {e}")}})));
    };
    let started = Instant::now();
    let mut next_tick = Instant::now() + Duration::from_secs(1);
    loop {
        match inbox.recv_timeout(next_tick.saturating_duration_since(Instant::now())) {
            Ok(text) => {
                let Ok(v) = serde_json::from_str::<Value>(&text) else { continue };
                let Some(event) = v.get("event").and_then(|e| e.as_str()) else { continue };
                if let Ok(Some(f)) = lua.globals().get::<Option<mlua::Function>>(event) {
                    let r = lua.to_value(&v).and_then(|arg| f.call::<()>(arg));
                    if let Err(e) = r {
                        log_error(e);
                    }
                }
            }
            Err(RecvTimeoutError::Timeout) => {
                if let Ok(Some(f)) = lua.globals().get::<Option<mlua::Function>>("tick") {
                    if let Err(e) = f.call::<()>(started.elapsed().as_secs_f64()) {
                        log_error(e);
                    }
                }
                next_tick += Duration::from_secs(1);
            }
            Err(RecvTimeoutError::Disconnected) => break,
        }
    }
}

pub fn log_dir() -> PathBuf {
    crate::profile::data_dir().join("Logs")
}

/// `logMessage`: one line in the plugin's log file.
pub fn log(plugin: &str, message: &str) {
    let _ = std::fs::create_dir_all(log_dir());
    if let Ok(mut f) = std::fs::OpenOptions::new().create(true).append(true).open(log_dir().join(format!("{plugin}.log"))) {
        use std::io::Write;
        let _ = writeln!(f, "{} {message}", chrono::Local::now().format("%Y-%m-%d %H:%M:%S"));
    }
}

fn globals_file(plugin: &str) -> PathBuf {
    crate::profile::data_dir().join("PluginSettings").join(format!("{plugin}.json"))
}

/// A plugin's global settings (`setGlobalSettings` / `getGlobalSettings`).
pub fn load_globals(plugin: &str) -> Value {
    std::fs::read_to_string(globals_file(plugin)).ok().and_then(|t| serde_json::from_str(&t).ok()).unwrap_or_else(|| json!({}))
}

pub fn save_globals(plugin: &str, v: &Value) {
    let f = globals_file(plugin);
    if let Some(d) = f.parent() {
        let _ = std::fs::create_dir_all(d);
    }
    let _ = std::fs::write(f, serde_json::to_string_pretty(v).unwrap_or_default());
}

/// The picture of a `setImage`: a `data:image/...;base64,` address (PNG, JPEG, GIF, BMP or WebP)
/// is written to LabDeck's PluginImages folder; a file path (absolute or inside the plugin's
/// folder) is used as it is.
pub fn image_file(image: &str, plugin_dir: &Path) -> Result<PathBuf, String> {
    if let Some(rest) = image.strip_prefix("data:") {
        let (head, data) = rest.split_once(',').ok_or("a data address without a comma")?;
        if head.starts_with("image/svg") {
            return Err("SVG pictures aren't supported yet; send a PNG".into());
        }
        if !head.ends_with(";base64") {
            return Err("only base64 data addresses are supported".into());
        }
        let bytes = base64_decode(data).ok_or("the base64 picture data couldn't be read")?;
        let format = image::guess_format(&bytes).map_err(|_| "not a picture LabDeck can read")?;
        let ext = format.extensions_str().first().copied().unwrap_or("png");
        let dir = crate::profile::data_dir().join("PluginImages");
        let _ = std::fs::create_dir_all(&dir);
        let path = dir.join(format!("{:016x}.{ext}", fnv1a(&bytes)));
        if !path.exists() {
            std::fs::write(&path, &bytes).map_err(|e| e.to_string())?;
        }
        return Ok(path);
    }
    let p = PathBuf::from(image);
    let p = if p.is_absolute() { p } else { plugin_dir.join(p) };
    if p.exists() { Ok(p) } else { Err(format!("{} wasn't found", p.display())) }
}

fn fnv1a(bytes: &[u8]) -> u64 {
    let mut h: u64 = 0xcbf2_9ce4_8422_2325;
    for b in bytes {
        h ^= *b as u64;
        h = h.wrapping_mul(0x0000_0100_0000_01b3);
    }
    h
}

/// Standard base64 (padding and whitespace allowed).
pub fn base64_decode(s: &str) -> Option<Vec<u8>> {
    let mut out = Vec::with_capacity(s.len() * 3 / 4);
    let mut buf = 0u32;
    let mut bits = 0;
    for c in s.bytes() {
        let v = match c {
            b'A'..=b'Z' => c - b'A',
            b'a'..=b'z' => c - b'a' + 26,
            b'0'..=b'9' => c - b'0' + 52,
            b'+' | b'-' => 62,
            b'/' | b'_' => 63,
            b'=' => break,
            b' ' | b'\n' | b'\r' | b'\t' => continue,
            _ => return None,
        } as u32;
        buf = (buf << 6) | v;
        bits += 6;
        if bits >= 8 {
            bits -= 8;
            out.push((buf >> bits) as u8);
            buf &= (1 << bits) - 1;
        }
    }
    Some(out)
}

/// A key event for a plugin (`keyDown`, `keyUp`, `willAppear`, `willDisappear`, `didReceiveSettings`).
pub fn key_event(event: &str, k: &crate::profile::KeyAction, pos: (usize, usize), device: &str, in_multi: bool) -> Value {
    let settings = if k.settings.is_object() { k.settings.clone() } else { json!({}) };
    json!({
        "event": event,
        "action": k.uuid,
        "context": k.action_id,
        "device": device,
        "payload": {
            "settings": settings,
            "coordinates": {"column": pos.0, "row": pos.1},
            "state": k.state,
            "isInMultiAction": in_multi,
            "controller": "Keypad",
        }
    })
}

/// The device type number plugins expect: 0 Stream Deck, 1 Mini, 2 XL, 5 Pedal, 7 +, 9 Neo.
pub fn device_type(name: &str) -> u32 {
    if name.contains("Mini") {
        1
    } else if name.contains("XL") {
        2
    } else if name.contains("Pedal") {
        5
    } else if name.contains('+') {
        7
    } else if name.contains("Neo") {
        9
    } else {
        0
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn plugin_code_kinds() {
        assert_eq!(kind_of(Path::new("bin/plugin.js")), Kind::Node);
        assert_eq!(kind_of(Path::new("plugin.ts")), Kind::Node);
        assert_eq!(kind_of(Path::new("main.lua")), Kind::Lua);
        assert_eq!(kind_of(Path::new("my-plugin.exe")), Kind::Program);
        assert_eq!(kind_of(Path::new("my-plugin")), Kind::Program);
        assert!(matches!(kind_of(Path::new("index.html")), Kind::Unsupported(_)));
    }

    #[test]
    fn base64_pictures_decode() {
        assert_eq!(base64_decode("aGVsbG8=").unwrap(), b"hello");
        assert_eq!(base64_decode("aGVsbG8gd29ybGQ").unwrap(), b"hello world");
        assert!(base64_decode("not*base64").is_none());
    }

    /// Waits up to 10 s for a message from `host` that `want` accepts.
    fn wait_for(host: &mut Host, mut want: impl FnMut(&FromPlugin) -> bool) -> bool {
        let until = Instant::now() + Duration::from_secs(10);
        while Instant::now() < until {
            if host.drain().iter().any(&mut want) {
                return true;
            }
            std::thread::sleep(Duration::from_millis(20));
        }
        false
    }

    fn is_command(m: &FromPlugin, event: &str, check: impl Fn(&Value) -> bool) -> bool {
        matches!(m, FromPlugin::Message(_, v) if v["event"] == event && check(v))
    }

    fn counter_key(uuid: &str, count: u64) -> crate::profile::KeyAction {
        let mut k = crate::profile::KeyAction::new(uuid, "Counter", 1);
        k.settings = json!({"count": count});
        k
    }

    #[test]
    fn websocket_plugin_round_trip() {
        use tungstenite::Message;
        let mut host = Host::start(Vec::new(), |_| json!({}), || {}).unwrap();
        let (mut ws, _) = tungstenite::connect(format!("ws://127.0.0.1:{}", host.port)).unwrap();
        ws.send(Message::text(json!({"event": "registerPlugin", "uuid": "com.test.ws"}).to_string())).unwrap();
        assert!(wait_for(&mut host, |m| matches!(m, FromPlugin::Registered(u) if u == "com.test.ws")));
        // LabDeck → plugin
        let k = counter_key("com.test.ws.count", 3);
        host.send("com.test.ws", &key_event("keyDown", &k, (2, 1), "DEV1", false));
        let got: Value = match ws.read().unwrap() {
            Message::Text(t) => serde_json::from_str(&t).unwrap(),
            other => panic!("unexpected {other:?}"),
        };
        assert_eq!(got["event"], "keyDown");
        assert_eq!(got["context"], k.action_id.as_str());
        assert_eq!(got["payload"]["coordinates"]["column"], 2);
        assert_eq!(got["payload"]["settings"]["count"], 3);
        // plugin → LabDeck
        ws.send(Message::text(json!({"event": "setTitle", "context": k.action_id, "payload": {"title": "4"}}).to_string())).unwrap();
        assert!(wait_for(&mut host, |m| is_command(m, "setTitle", |v| v["payload"]["title"] == "4")));
        ws.close(None).unwrap();
        assert!(wait_for(&mut host, |m| matches!(m, FromPlugin::Gone(u, _) if u == "com.test.ws")));
        assert!(!host.running.contains("com.test.ws"));
    }

    /// The Lua example from plugin-examples runs inside LabDeck and counts.
    #[test]
    fn lua_example_counts() {
        let src = Path::new(env!("CARGO_MANIFEST_DIR")).join("plugin-examples/counter-lua/com.labdeck.example.counter-lua.sdPlugin");
        let info = read_info(&src).unwrap();
        let mut host = Host::start(vec![info.clone()], |_| json!({}), || {}).unwrap();
        assert!(wait_for(&mut host, |m| matches!(m, FromPlugin::Registered(u) if *u == info.uuid)));
        let action = &info.actions[0];
        assert_eq!(host.running_plugin_of(action), Some(info.uuid.as_str()));
        let k = counter_key(action, 41);
        host.send(&info.uuid, &key_event("willAppear", &k, (0, 0), "DEV1", false));
        assert!(wait_for(&mut host, |m| is_command(m, "setTitle", |v| v["payload"]["title"] == "41")));
        host.send(&info.uuid, &key_event("keyDown", &k, (0, 0), "DEV1", false));
        let (mut settings, mut title) = (false, false);
        let until = Instant::now() + Duration::from_secs(10);
        while Instant::now() < until && !(settings && title) {
            for m in host.drain() {
                settings |= is_command(&m, "setSettings", |v| v["payload"]["count"] == 42);
                title |= is_command(&m, "setTitle", |v| v["payload"]["title"] == "42");
            }
            std::thread::sleep(Duration::from_millis(20));
        }
        assert!(settings && title, "settings {settings}, title {title}");
    }

    /// The Rust example from plugin-examples, as a real program LabDeck starts. Build it first:
    /// `cargo build --release --manifest-path plugin-examples/counter-rust/Cargo.toml --target-dir target/plugin-examples`
    #[test]
    #[ignore]
    fn rust_example_counts() {
        let root = Path::new(env!("CARGO_MANIFEST_DIR"));
        let exe = root.join("target/plugin-examples/release").join(if cfg!(windows) { "counter-rust.exe" } else { "counter-rust" });
        assert!(exe.exists(), "build the Rust example first");
        let mut info = read_info(&root.join("plugin-examples/counter-rust/com.labdeck.example.counter-rust.sdPlugin")).unwrap();
        info.code = Some(exe);
        let mut host = Host::start(vec![info.clone()], |p| json!({"plugin": {"uuid": p.uuid}}), || {}).unwrap();
        assert!(host.problems.is_empty(), "{:?}", host.problems);
        assert!(wait_for(&mut host, |m| matches!(m, FromPlugin::Registered(u) if *u == info.uuid)));
        let k = counter_key(&info.actions[0], 9);
        host.send(&info.uuid, &key_event("keyDown", &k, (1, 0), "DEV1", false));
        // The 10th press: new settings, the title "10" and an OK flash, in that order.
        let (mut settings, mut title, mut ok) = (false, false, false);
        let until = Instant::now() + Duration::from_secs(10);
        while Instant::now() < until && !(settings && title && ok) {
            for m in host.drain() {
                settings |= is_command(&m, "setSettings", |v| v["payload"]["count"] == 10);
                title |= is_command(&m, "setTitle", |v| v["payload"]["title"] == "10");
                ok |= is_command(&m, "showOk", |_| true);
            }
            std::thread::sleep(Duration::from_millis(20));
        }
        assert!(settings && title && ok, "settings {settings}, title {title}, OK {ok}");
        host.stop();
    }

    #[test]
    fn manifest_is_read() {
        let dir = std::env::temp_dir().join(format!("labdeck-test-{}.sdPlugin", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(
            dir.join("manifest.json"),
            r#"{"UUID": "com.example.counter", "Name": "Counter", "Version": "1.0", "CodePath": "main.lua",
                "Actions": [{"UUID": "com.example.counter.count", "Name": "Count"},
                            {"UUID": "com.example.counter.toggle", "DisableAutomaticStates": true}]}"#,
        )
        .unwrap();
        let p = read_info(&dir).unwrap();
        assert_eq!(p.uuid, "com.example.counter");
        assert_eq!(p.code.as_deref(), Some(dir.join("main.lua").as_path()));
        assert_eq!(p.actions, ["com.example.counter.count", "com.example.counter.toggle"]);
        assert!(p.manual_states.contains("com.example.counter.toggle"));
        let _ = std::fs::remove_dir_all(&dir);
    }
}
