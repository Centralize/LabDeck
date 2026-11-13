//! Website keys: fetch the site's icon and make a key picture from it. Follows the original's
//! ESDFavIconHelper (functions 0x140377810 and 0x140377f30): look for <link rel=…> icons in
//! the order below, first one at least WANTED_PX big, then one without a size; if none,
//! fall back to https://<host>/favicon.ico.

use std::path::PathBuf;
use std::time::Duration;

/// rel values in the original's order of preference.
pub const REL_ORDER: &[&str] = &["apple-touch-icon", "apple-touch-icon-precomposed", "icon", "shortcut icon"];
/// Smallest icon worth taking from a sized <link> (estimated: the caller's size wasn't traced;
/// a 72 px key is the smallest Stream Deck key).
pub const WANTED_PX: u32 = 72;
/// Key picture size made from the icon, and how much of it the icon fills.
pub const KEY_PX: u32 = 144;
pub const ICON_FILL: f32 = 0.62;
pub const TIMEOUT_S: u64 = 8;
/// Below this average brightness a see-through icon gets a light background instead of black.
pub const DARK_ICON_LUMA: f32 = 0.3;
const MAX_BYTES: u64 = 4 * 1024 * 1024;

struct Link {
    rel: String,
    href: String,
    size: u32,
}

/// "example.com/x" → "https://example.com/x" (keeps http:// and other schemes).
pub fn normalize(url: &str) -> String {
    let u = url.trim();
    if u.contains("://") { u.to_string() } else { format!("https://{u}") }
}

fn host_of(url: &str) -> Option<String> {
    let rest = url.split("://").nth(1)?;
    let host = rest.split(['/', '?', '#']).next()?;
    (!host.is_empty()).then(|| host.to_string())
}

fn scheme_of(url: &str) -> &str {
    url.split("://").next().unwrap_or("https")
}

/// The attribute value of `name` inside one tag (quoted or bare).
fn attr(tag: &str, name: &str) -> Option<String> {
    let lower = tag.to_ascii_lowercase();
    let mut from = 0;
    while let Some(i) = lower[from..].find(name) {
        let at = from + i;
        let before_ok = at == 0 || !lower.as_bytes()[at - 1].is_ascii_alphanumeric() && lower.as_bytes()[at - 1] != b'-';
        let mut j = at + name.len();
        while j < tag.len() && tag.as_bytes()[j] == b' ' {
            j += 1;
        }
        if before_ok && j < tag.len() && tag.as_bytes()[j] == b'=' {
            j += 1;
            while j < tag.len() && tag.as_bytes()[j] == b' ' {
                j += 1;
            }
            let q = tag.as_bytes().get(j).copied();
            return Some(if q == Some(b'"') || q == Some(b'\'') {
                let end = tag[j + 1..].find(q.unwrap() as char).map_or(tag.len(), |e| j + 1 + e);
                tag[j + 1..end].to_string()
            } else {
                let end = tag[j..].find([' ', '>', '/']).map_or(tag.len(), |e| j + e);
                tag[j..end].to_string()
            });
        }
        from = at + name.len();
    }
    None
}

fn links(html: &str) -> Vec<Link> {
    let lower = html.to_ascii_lowercase();
    let mut out = Vec::new();
    let mut from = 0;
    while let Some(i) = lower[from..].find("<link") {
        let start = from + i;
        let end = lower[start..].find('>').map_or(lower.len(), |e| start + e + 1);
        let tag = &html[start..end];
        if let (Some(rel), Some(href)) = (attr(tag, "rel"), attr(tag, "href")) {
            // "32x32 64x64" → the biggest; "any" counts as big.
            let size = attr(tag, "sizes")
                .map(|s| {
                    s.split_whitespace()
                        .filter_map(|p| if p.eq_ignore_ascii_case("any") { Some(4096) } else { p.split(['x', 'X']).next()?.parse().ok() })
                        .max()
                        .unwrap_or(0)
                })
                .unwrap_or(0);
            out.push(Link { rel: rel.trim().to_ascii_lowercase(), href: href.trim().to_string(), size });
        }
        from = end;
    }
    out
}

/// The original's pick: by rel order, first a link sized >= WANTED_PX, then one without a size.
fn pick(links: &[Link]) -> Option<&Link> {
    for rel in REL_ORDER {
        if let Some(l) = links.iter().find(|l| l.rel == *rel && l.size >= WANTED_PX) {
            return Some(l);
        }
    }
    for rel in REL_ORDER {
        if let Some(l) = links.iter().find(|l| l.rel == *rel && l.size == 0) {
            return Some(l);
        }
    }
    None
}

/// Make an href absolute ("//x" gets "https:", "/x" and "x" get the page's host).
fn absolute(href: &str, page: &str) -> String {
    let host = host_of(page).unwrap_or_default();
    let scheme = scheme_of(page);
    if href.starts_with("//") {
        format!("https:{href}")
    } else if href.contains("://") {
        href.to_string()
    } else if href.starts_with('/') {
        format!("{scheme}://{host}{href}")
    } else {
        let base = page.rsplit_once('/').filter(|(b, _)| b.contains("://") && b.len() > scheme.len() + 3).map_or(format!("{scheme}://{host}"), |(b, _)| b.to_string());
        format!("{base}/{href}")
    }
}

fn agent() -> ureq::Agent {
    ureq::Agent::config_builder()
        .timeout_global(Some(Duration::from_secs(TIMEOUT_S)))
        .user_agent("Mozilla/5.0 (X11; Linux x86_64) LabDeck")
        .build()
        .into()
}

fn get_bytes(agent: &ureq::Agent, url: &str) -> Result<Vec<u8>, String> {
    let mut resp = agent.get(url).call().map_err(|e| e.to_string())?;
    resp.body_mut().with_config().limit(MAX_BYTES).read_to_vec().map_err(|e| e.to_string())
}

fn decode(bytes: &[u8]) -> Option<image::RgbaImage> {
    image::load_from_memory(bytes).ok().map(|i| i.to_rgba8()).filter(|i| i.width() > 0)
}

/// Fetch the site's icon. Returns the decoded picture.
pub fn fetch(url: &str) -> Result<image::RgbaImage, String> {
    let page = normalize(url);
    let host = host_of(&page).ok_or("That doesn't look like a web address.")?;
    let agent = agent();
    let mut candidates = Vec::new();
    if let Ok(bytes) = get_bytes(&agent, &page) {
        let html = String::from_utf8_lossy(&bytes);
        let all = links(&html);
        if let Some(l) = pick(&all) {
            candidates.push(absolute(&l.href, &page));
        }
        // Other icons on the page as backups (e.g. when the preferred one is an SVG).
        for rel in REL_ORDER {
            for l in all.iter().filter(|l| l.rel == *rel) {
                let u = absolute(&l.href, &page);
                if !candidates.contains(&u) {
                    candidates.push(u);
                }
            }
        }
    }
    candidates.push(format!("https://{host}/favicon.ico"));
    let mut last = String::from("no icon found");
    for c in candidates {
        match get_bytes(&agent, &c) {
            Ok(b) => match decode(&b) {
                Some(img) => return Ok(img),
                None => last = format!("{c} isn't a picture LabDeck can read"),
            },
            Err(e) => last = e,
        }
    }
    Err(last)
}

/// A key picture: the icon centred on a background taken from the icon's edge (or dark when
/// the edge is see-through), saved as PNG. Returns the file path.
pub fn make_key_picture(icon: &image::RgbaImage, url: &str) -> Result<PathBuf, String> {
    let (w, h) = icon.dimensions();
    // Average colour of the opaque edge pixels.
    let (mut r, mut g, mut b, mut n) = (0u64, 0u64, 0u64, 0u64);
    for (x, y, p) in icon.enumerate_pixels() {
        if (x == 0 || y == 0 || x == w - 1 || y == h - 1) && p[3] > 200 {
            r += p[0] as u64;
            g += p[1] as u64;
            b += p[2] as u64;
            n += 1;
        }
    }
    let edge_opaque = n * 2 > (2 * (w + h) as u64);
    // See-through icons go on black, unless the icon itself is dark (then on a light tile).
    let (mut lum, mut cnt) = (0f32, 0f32);
    for p in icon.pixels().filter(|p| p[3] > 128) {
        lum += (0.2126 * p[0] as f32 + 0.7152 * p[1] as f32 + 0.0722 * p[2] as f32) / 255.0;
        cnt += 1.0;
    }
    let dark_icon = cnt > 0.0 && lum / cnt < DARK_ICON_LUMA;
    let bg = if edge_opaque {
        image::Rgba([(r / n) as u8, (g / n) as u8, (b / n) as u8, 255])
    } else if dark_icon {
        image::Rgba([0xf2, 0xf2, 0xf2, 255])
    } else {
        image::Rgba([0, 0, 0, 255])
    };
    let mut out = image::RgbaImage::from_pixel(KEY_PX, KEY_PX, bg);
    let target = (KEY_PX as f32 * if edge_opaque { 1.0 } else { ICON_FILL }) as u32;
    let scale = target as f32 / w.max(h) as f32;
    let (nw, nh) = (((w as f32 * scale).round() as u32).max(1), ((h as f32 * scale).round() as u32).max(1));
    // Tiny pixel icons stay crisp (nearest); anything bigger is smoothed.
    let filter = if w.max(h) <= 16 { image::imageops::FilterType::Nearest } else { image::imageops::FilterType::CatmullRom };
    let scaled = image::imageops::resize(icon, nw, nh, filter);
    image::imageops::overlay(&mut out, &scaled, ((KEY_PX - nw) / 2) as i64, ((KEY_PX - nh) / 2) as i64);
    let dir = crate::profile::data_dir().join("Icons");
    std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    let name: String = host_of(&normalize(url)).unwrap_or_else(|| "site".into()).chars().map(|c| if c.is_ascii_alphanumeric() || c == '.' { c } else { '_' }).collect();
    let path = dir.join(format!("{name}-{}.png", crate::profile::new_uuid().split('-').next().unwrap_or("0")));
    out.save(&path).map_err(|e| e.to_string())?;
    Ok(path)
}
