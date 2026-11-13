//! Profiles, pages and keys, stored in the same folder layout the original uses
//! (`<UUID>.sdProfile/manifest.json` + `Profiles/<page>/manifest.json`), so exported
//! `.streamDeckProfile` files from Windows can be imported unchanged.

use serde::{Deserialize, Serialize};
use serde_json::{Map, Value, json};
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

pub fn data_dir() -> PathBuf {
    if let Ok(p) = std::env::var("LABDECK_HOME") {
        return PathBuf::from(p);
    }
    #[cfg(target_os = "windows")]
    {
        if let Ok(a) = std::env::var("APPDATA") {
            return PathBuf::from(a).join("LabDeck");
        }
    }
    if let Ok(x) = std::env::var("XDG_DATA_HOME") {
        return PathBuf::from(x).join("LabDeck");
    }
    let home = std::env::var("HOME").unwrap_or_else(|_| ".".into());
    PathBuf::from(home).join(".local/share/LabDeck")
}

fn yes() -> bool {
    true
}

#[derive(Serialize, Deserialize, Clone, Debug, Default)]
pub struct StateDef {
    #[serde(rename = "Image", default, skip_serializing_if = "Option::is_none")]
    pub image: Option<String>,
    #[serde(rename = "Title", default, skip_serializing_if = "Option::is_none")]
    pub title: Option<String>,
    #[serde(rename = "ShowTitle", default = "yes")]
    pub show_title: bool,
    #[serde(rename = "TitleColor", default, skip_serializing_if = "Option::is_none")]
    pub title_color: Option<String>,
    #[serde(rename = "TitleAlignment", default, skip_serializing_if = "Option::is_none")]
    pub title_alignment: Option<String>,
    #[serde(rename = "FontSize", default, skip_serializing_if = "Option::is_none")]
    pub font_size: Option<f32>,
    /// This app's own fields for keys without a picture: a drawn icon and a background colour.
    #[serde(rename = "LabDeckIcon", default, skip_serializing_if = "Option::is_none")]
    pub icon: Option<String>,
    #[serde(rename = "LabDeckBackground", default, skip_serializing_if = "Option::is_none")]
    pub background: Option<String>,
    #[serde(rename = "LabDeckIconColor", default, skip_serializing_if = "Option::is_none")]
    pub icon_color: Option<String>,
    #[serde(flatten)]
    pub extra: Map<String, Value>,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct KeyAction {
    #[serde(rename = "ActionID", default)]
    pub action_id: String,
    #[serde(rename = "LinkedTitle", default = "yes")]
    pub linked_title: bool,
    #[serde(rename = "Name", default)]
    pub name: String,
    #[serde(rename = "Settings", default)]
    pub settings: Value,
    #[serde(rename = "State", default)]
    pub state: usize,
    #[serde(rename = "States", default)]
    pub states: Vec<StateDef>,
    #[serde(rename = "UUID")]
    pub uuid: String,
    #[serde(flatten)]
    pub extra: Map<String, Value>,
}

impl KeyAction {
    pub fn new(uuid: &str, name: &str, states: usize) -> Self {
        KeyAction {
            action_id: new_uuid(),
            linked_title: true,
            name: name.into(),
            settings: json!({}),
            state: 0,
            states: vec![StateDef { show_title: true, ..Default::default() }; states.max(1)],
            uuid: uuid.into(),
            extra: Map::new(),
        }
    }
    pub fn cur(&self) -> StateDef {
        self.states.get(self.state).or(self.states.first()).cloned().unwrap_or_default()
    }
    pub fn cur_mut(&mut self) -> &mut StateDef {
        if self.states.is_empty() {
            self.states.push(StateDef { show_title: true, ..Default::default() });
        }
        let i = self.state.min(self.states.len() - 1);
        &mut self.states[i]
    }
    pub fn setting_str(&self, k: &str) -> String {
        self.settings.get(k).and_then(|v| v.as_str()).unwrap_or("").to_string()
    }
}

#[derive(Clone, Debug, Default)]
pub struct Page {
    pub id: String,
    pub name: String,
    /// key "col,row" like the original's manifest.
    pub keys: BTreeMap<(usize, usize), KeyAction>,
    /// The folder the page was loaded from, for relative image paths.
    pub dir: Option<PathBuf>,
}

#[derive(Clone, Debug)]
pub struct Profile {
    pub uuid: String,
    pub name: String,
    pub model: String,
    pub pages: Vec<String>,
    pub current: usize,
    pub default_page: String,
    /// Every page, including folder pages that are not in `pages`.
    pub all: BTreeMap<String, Page>,
    pub dir: Option<PathBuf>,
    /// The first-run sample profile (left out when there are real profiles).
    pub sample: bool,
    /// Which layout of the first-run profile it is (0 for older builds' sample).
    pub sample_rev: u64,
    /// When its manifest was last written (seconds), to open the most recent one first.
    pub mtime: u64,
    /// The original's "AppIdentifier": "*" marks the device's default profile (shown when no
    /// app-specific profile applies); other values tie a profile to an app.
    pub app: Option<String>,
}

/// Page folder names: the page UUID's 16 bytes in base-32 with the alphabet below,
/// then a 'Z' (worked out from the default profiles shipped with the original).
const B32: &[u8] = b"0123456789ABCDEFGHIJKLMNOPQRSTVW";

pub fn page_folder_name(uuid: &str) -> String {
    let hex: String = uuid.chars().filter(|c| c.is_ascii_hexdigit()).collect();
    let mut bytes = Vec::new();
    for i in (0..hex.len().min(32)).step_by(2) {
        bytes.push(u8::from_str_radix(&hex[i..i + 2], 16).unwrap_or(0));
    }
    let mut out = String::new();
    let mut acc: u32 = 0;
    let mut bits = 0;
    for b in bytes {
        acc = (acc << 8) | b as u32;
        bits += 8;
        while bits >= 5 {
            out.push(B32[((acc >> (bits - 5)) & 31) as usize] as char);
            bits -= 5;
        }
    }
    if bits > 0 {
        out.push(B32[((acc << (5 - bits)) & 31) as usize] as char);
    }
    out.push('Z');
    out
}

pub fn new_uuid() -> String {
    use std::time::{SystemTime, UNIX_EPOCH};
    static COUNTER: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(1);
    let t = SystemTime::now().duration_since(UNIX_EPOCH).map(|d| d.as_nanos()).unwrap_or(0);
    let c = COUNTER.fetch_add(1, std::sync::atomic::Ordering::Relaxed) as u128;
    let mut x = t ^ (c.wrapping_mul(0x9E37_79B9_7F4A_7C15_F39C_C060_5CED_C835)) ^ (std::process::id() as u128) << 64;
    let mut b = [0u8; 16];
    for v in &mut b {
        x ^= x << 13;
        x ^= x >> 7;
        x ^= x << 17;
        *v = (x & 0xff) as u8;
        x = x.rotate_left(11).wrapping_add(c);
    }
    b[6] = (b[6] & 0x0f) | 0x40;
    b[8] = (b[8] & 0x3f) | 0x80;
    let h: String = b.iter().map(|v| format!("{v:02x}")).collect();
    format!("{}-{}-{}-{}-{}", &h[0..8], &h[8..12], &h[12..16], &h[16..20], &h[20..32])
}

impl Profile {
    pub fn new(name: &str, model: &str) -> Self {
        let first = new_uuid();
        let mut all = BTreeMap::new();
        all.insert(first.clone(), Page { id: first.clone(), ..Default::default() });
        Profile {
            uuid: new_uuid().to_uppercase(),
            name: name.into(),
            model: model.into(),
            pages: vec![first.clone()],
            current: 0,
            default_page: new_uuid(),
            all,
            dir: None,
            sample: false,
            sample_rev: 0,
            mtime: 0,
            app: None,
        }
    }

    pub fn add_page(&mut self) -> usize {
        let id = new_uuid();
        self.all.insert(id.clone(), Page { id: id.clone(), ..Default::default() });
        self.pages.push(id);
        self.pages.len() - 1
    }

    pub fn page(&self, id: &str) -> Option<&Page> {
        self.all.get(id)
    }

    pub fn page_mut(&mut self, id: &str) -> &mut Page {
        self.all.entry(id.to_string()).or_insert_with(|| Page { id: id.to_string(), ..Default::default() })
    }

    pub fn load(dir: &Path) -> Option<Profile> {
        let text = std::fs::read_to_string(dir.join("manifest.json")).ok()?;
        let v: Value = serde_json::from_str(text.trim_start_matches('\u{feff}')).ok()?;
        let name = v.get("Name").and_then(|x| x.as_str()).unwrap_or("Profile").to_string();
        let model = v.pointer("/Device/Model").and_then(|x| x.as_str()).unwrap_or("").to_string();
        let mut pages: Vec<String> = v
            .pointer("/Pages/Pages")
            .and_then(|x| x.as_array())
            .map(|a| a.iter().filter_map(|s| s.as_str().map(String::from)).collect())
            .unwrap_or_default();
        let current_id = v.pointer("/Pages/Current").and_then(|x| x.as_str()).unwrap_or("").to_string();
        let default_page = v.pointer("/Pages/Default").and_then(|x| x.as_str()).unwrap_or("").to_string();
        let stem = dir.file_stem().and_then(|s| s.to_str()).unwrap_or("").to_string();

        // Read every page folder, then give each its UUID by matching folder names.
        let mut by_folder: BTreeMap<String, Page> = BTreeMap::new();
        if let Ok(rd) = std::fs::read_dir(dir.join("Profiles")) {
            for e in rd.flatten() {
                let p = e.path();
                if let Some(pg) = load_page(&p) {
                    by_folder.insert(p.file_name().unwrap().to_string_lossy().to_string(), pg);
                }
            }
        }
        let mut ids: Vec<String> = pages.clone();
        ids.push(default_page.clone());
        for pg in by_folder.values() {
            for k in pg.keys.values() {
                if let Some(u) = k.settings.get("ProfileUUID").and_then(|x| x.as_str()) {
                    ids.push(u.to_string());
                }
            }
        }
        let mut all = BTreeMap::new();
        // Version 3.0 profiles name page folders by the upper-case UUID, 2.0 by base-32.
        for id in ids {
            let found = by_folder.remove(&id.to_uppercase()).or_else(|| by_folder.remove(&page_folder_name(&id)));
            if let Some(mut pg) = found {
                pg.id = id.clone();
                all.insert(id, pg);
            }
        }
        // Folders we could not name keep their folder name as id.
        for (f, mut pg) in by_folder {
            if f == page_folder_name(&default_page) || f.eq_ignore_ascii_case(&default_page) {
                continue;
            }
            let f = if f.len() == 36 && f.contains('-') { f.to_lowercase() } else { f };
            pg.id = f.clone();
            if pages.is_empty() {
                pages.push(f.clone());
            }
            all.insert(f, pg);
        }
        pages.retain(|p| all.contains_key(p));
        if pages.is_empty() {
            let id = new_uuid();
            all.insert(id.clone(), Page { id: id.clone(), ..Default::default() });
            pages.push(id);
        }
        let current = pages.iter().position(|p| *p == current_id).unwrap_or(0);
        // Sample profiles carry a flag; earlier builds saved them without it, but only the
        // sample uses the "sample." placeholder plugin ids.
        let sample = v.get("LabDeckSample").and_then(|x| x.as_bool()).unwrap_or(false)
            || all.values().any(|pg| pg.keys.values().any(|k| k.uuid.starts_with("sample.")));
        let mtime = std::fs::metadata(dir.join("manifest.json"))
            .and_then(|m| m.modified())
            .ok()
            .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
            .map_or(0, |d| d.as_secs());
        let app = v.get("AppIdentifier").and_then(|x| x.as_str()).map(String::from);
        let sample_rev = if sample { v.get("LabDeckSampleRev").and_then(|x| x.as_u64()).unwrap_or(0) } else { 0 };
        Some(Profile { uuid: stem, name, model, pages, current, default_page, all, dir: Some(dir.to_path_buf()), sample, sample_rev, mtime, app })
    }

    pub fn save(&mut self, root: &Path) -> std::io::Result<()> {
        let dir = root.join(format!("{}.sdProfile", self.uuid));
        std::fs::create_dir_all(dir.join("Profiles"))?;
        let manifest = json!({
            "Device": {"Model": self.model, "UUID": ""},
            "Name": self.name,
            "Pages": {
                "Current": self.pages.get(self.current).cloned().unwrap_or_default(),
                "Default": self.default_page,
                "Pages": self.pages,
            },
            "Version": "3.0",
        });
        let mut manifest = manifest;
        if self.sample {
            manifest["LabDeckSample"] = json!(true);
            manifest["LabDeckSampleRev"] = json!(self.sample_rev);
        }
        if let Some(a) = &self.app {
            manifest["AppIdentifier"] = json!(a);
        }
        std::fs::write(dir.join("manifest.json"), serde_json::to_string(&manifest)?)?;
        for (id, pg) in &mut self.all {
            // Saved like the current original (version 3.0): page folders named by the upper-case id.
            let pdir = dir.join("Profiles").join(id.to_uppercase());
            std::fs::create_dir_all(pdir.join("Images"))?;
            let mut actions = Map::new();
            for ((c, r), k) in &mut pg.keys {
                // Copy pictures that live elsewhere into the page's Images folder; one already in
                // it (a key pasted back onto its own page) is saved by its relative path.
                for st in &mut k.states {
                    if let Some(img) = st.image.clone() {
                        let src = resolve_image(pg.dir.as_deref(), &img);
                        if src.starts_with(&pdir) {
                            if Path::new(&img).is_absolute() {
                                if let Ok(rel) = src.strip_prefix(&pdir) {
                                    st.image = Some(rel.to_string_lossy().replace('\\', "/"));
                                }
                            }
                        } else if src.exists() {
                            let ext = src.extension().and_then(|e| e.to_str()).unwrap_or("png");
                            let name = format!("Images/{}.{}", page_folder_name(&new_uuid()), ext);
                            if std::fs::copy(&src, pdir.join(&name)).is_ok() {
                                st.image = Some(name);
                            }
                        }
                    }
                }
                actions.insert(format!("{c},{r}"), serde_json::to_value(&*k)?);
            }
            pg.dir = Some(pdir.clone());
            let m = json!({
                "Controllers": [{"Actions": actions, "Type": "Keypad"}],
                "Icon": "",
                "Name": pg.name,
            });
            std::fs::write(pdir.join("manifest.json"), serde_json::to_string(&m)?)?;
        }
        self.dir = Some(dir);
        Ok(())
    }
}

pub fn resolve_image(page_dir: Option<&Path>, img: &str) -> PathBuf {
    let p = PathBuf::from(img);
    if p.is_absolute() {
        p
    } else {
        page_dir.map(|d| d.join(&p)).unwrap_or(p)
    }
}

/// A copy of `k` whose pictures point at their files directly (resolved against the folder of
/// the page it comes from), so it can go on another page: saving then copies the pictures into
/// that page's own folder. Used by Copy, Move to Folder and Move to Multi Action.
pub fn with_absolute_images(k: &KeyAction, page_dir: Option<&Path>) -> KeyAction {
    let mut k = k.clone();
    if let Some(dir) = page_dir {
        for st in &mut k.states {
            if let Some(img) = st.image.as_deref().filter(|i| !i.is_empty()) {
                st.image = Some(resolve_image(Some(dir), img).to_string_lossy().to_string());
            }
        }
    }
    k
}

fn load_page(dir: &Path) -> Option<Page> {
    let text = std::fs::read_to_string(dir.join("manifest.json")).ok()?;
    let v: Value = serde_json::from_str(text.trim_start_matches('\u{feff}')).ok()?;
    let mut keys = BTreeMap::new();
    if let Some(ctrls) = v.get("Controllers").and_then(|c| c.as_array()) {
        for c in ctrls {
            if c.get("Type").and_then(|t| t.as_str()).is_some_and(|t| t != "Keypad") {
                continue;
            }
            if let Some(acts) = c.get("Actions").and_then(|a| a.as_object()) {
                for (pos, a) in acts {
                    let mut it = pos.split(',').map(|s| s.trim().parse::<usize>());
                    if let (Some(Ok(col)), Some(Ok(row))) = (it.next(), it.next()) {
                        if let Ok(k) = serde_json::from_value::<KeyAction>(a.clone()) {
                            keys.insert((col, row), k);
                        }
                    }
                }
            }
        }
    }
    Some(Page {
        id: String::new(),
        name: v.get("Name").and_then(|x| x.as_str()).unwrap_or("").to_string(),
        keys,
        dir: Some(dir.to_path_buf()),
    })
}

pub fn profiles_root() -> PathBuf {
    data_dir().join("ProfilesV3")
}

/// LabDeck's own read-only import folder (`Import/ProfilesV3` in LabDeck's folder): profiles in
/// the same format placed there, for example copied from another computer, are listed, and one
/// edited here is saved as LabDeck's own copy, which then takes precedence. LabDeck is a
/// standalone program: it never reads Elgato's Stream Deck app or its folders.
pub fn import_roots() -> Vec<PathBuf> {
    vec![data_dir().join("Import").join("ProfilesV3")]
}

pub fn load_all(root: &Path) -> Vec<Profile> {
    let mut v = Vec::new();
    if let Ok(rd) = std::fs::read_dir(root) {
        for e in rd.flatten() {
            let p = e.path();
            if p.extension().is_some_and(|x| x == "sdProfile") {
                if let Some(pr) = Profile::load(&p) {
                    v.push(pr);
                }
            }
        }
    }
    v.sort_by_key(|p| p.name.to_lowercase());
    v
}

/// Import an exported `.streamDeckProfile` (a zip with one `.sdProfile` folder inside).
pub fn import(file: &Path, root: &Path) -> Result<Profile, String> {
    let f = std::fs::File::open(file).map_err(|e| e.to_string())?;
    let mut z = zip::ZipArchive::new(f).map_err(|e| e.to_string())?;
    std::fs::create_dir_all(root).map_err(|e| e.to_string())?;
    let staging = root.join(format!(".import-{}", new_uuid()));
    z.extract(&staging).map_err(|e| e.to_string())?;
    let inner = std::fs::read_dir(&staging)
        .map_err(|e| e.to_string())?
        .flatten()
        .map(|e| e.path())
        .find(|p| p.extension().is_some_and(|x| x == "sdProfile"))
        .ok_or("no profile inside the file")?;
    let mut pr = Profile::load(&inner).ok_or("could not read the profile")?;
    pr.uuid = new_uuid().to_uppercase();
    pr.save(root).map_err(|e| e.to_string())?;
    let _ = std::fs::remove_dir_all(&staging);
    Ok(pr)
}

fn key(uuid: &str, name: &str, title: &str, icon: &str, bg: &str) -> KeyAction {
    let mut k = KeyAction::new(uuid, name, 1);
    let st = k.cur_mut();
    st.title = if title.is_empty() { None } else { Some(title.into()) };
    st.icon = Some(icon.into());
    st.background = Some(bg.into());
    k
}

/// Layout of the first-run profile; older builds saved revision 0 (the six-page sample).
pub const SAMPLE_REV: u64 = 1;

/// The first-run profile, as the person drew it: one page named "Version" with four Text
/// keys in the middle of the grid spelling LAB / DECK over 0 / 4, white bold titles on black.
pub fn sample(model: &str) -> Profile {
    let mut p = Profile::new("Version", model);
    p.sample = true;
    p.sample_rev = SAMPLE_REV;
    let (cols, rows) = crate::devices::by_model(model).map_or((8, 4), |m| (m.cols, m.rows));
    // The two middle columns and the two middle rows (on an XL: columns 4-5, rows 2-3).
    let c0 = (cols / 2).saturating_sub(1);
    let r0 = (rows / 2).saturating_sub(1);
    let r1 = if rows > 1 { r0 + 1 } else { r0 };
    let id = p.pages[0].clone();
    let pg = p.page_mut(&id);
    for (c, r, t, size) in [(c0, r0, "LAB", 16.0), (c0 + 1, r0, "DECK", 16.0), (c0, r1, "0", 32.0), (c0 + 1, r1, "4", 32.0)] {
        if r1 == r0 && t.len() == 1 {
            continue;
        }
        let mut k = key("com.elgato.streamdeck.system.text", "Text", t, "", "#000000");
        k.settings = json!({"pastedText": t, "isSendingEnter": false});
        let st = k.cur_mut();
        st.title_alignment = Some("middle".into());
        st.title_color = Some("#ffffff".into());
        st.font_size = Some(size);
        pg.keys.insert((c, r), k);
    }
    p
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn moved_keys_keep_their_pictures() {
        let mut k = KeyAction::new("com.elgato.streamdeck.system.open", "Open", 1);
        k.states[0].image = Some("Images/abc.png".into());
        let dir = std::env::temp_dir().join("page");
        let moved = with_absolute_images(&k, Some(&dir));
        let want = dir.join("Images/abc.png").to_string_lossy().to_string();
        assert_eq!(moved.states[0].image.as_deref(), Some(want.as_str()));
        // A picture that already points at its file stays as it is.
        let again = with_absolute_images(&moved, Some(&std::env::temp_dir().join("other")));
        assert_eq!(again.states[0].image, moved.states[0].image);
        // A key without a picture is unchanged.
        let plain = KeyAction::new("com.elgato.streamdeck.system.open", "Open", 1);
        assert_eq!(with_absolute_images(&plain, Some(&dir)).states[0].image, None);
    }
}