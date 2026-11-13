//! Draws one key's picture (background, icon or image, title) at any size. The same
//! picture goes to the window and to the hardware, so both always match.

use crate::icons::{self, Canvas, Icon, Rgba};
use crate::profile::{KeyAction, resolve_image};
use ab_glyph::{Font, FontArc, PxScale, ScaleFont};
use std::collections::HashMap;
use std::path::Path;

/// Title size: pixels per point on a 72 px key (estimated from the captured keys).
pub const TITLE_PX_PER_PT: f32 = 1.5;

pub struct Fonts {
    pub bold: FontArc,
}

const BOLD_CANDIDATES: &[&str] = &[
    "/usr/share/fonts/truetype/dejavu/DejaVuSans-Bold.ttf",
    "/usr/share/fonts/TTF/DejaVuSans-Bold.ttf",
    "/usr/share/fonts/dejavu/DejaVuSans-Bold.ttf",
    "/usr/share/fonts/dejavu-sans-fonts/DejaVuSans-Bold.ttf",
    "/usr/share/fonts/truetype/liberation/LiberationSans-Bold.ttf",
    "/usr/share/fonts/liberation-sans/LiberationSans-Bold.ttf",
    "/usr/share/fonts/noto/NotoSans-Bold.ttf",
    "/usr/share/fonts/truetype/noto/NotoSans-Bold.ttf",
    "C:/Windows/Fonts/arialbd.ttf",
    "C:/Windows/Fonts/segoeuib.ttf",
];

impl Fonts {
    pub fn load() -> Fonts {
        for p in BOLD_CANDIDATES {
            if let Ok(b) = std::fs::read(p) {
                if let Ok(f) = FontArc::try_from_vec(b) {
                    return Fonts { bold: f };
                }
            }
        }
        Fonts { bold: FontArc::try_from_slice(epaint_default_fonts::UBUNTU_LIGHT).expect("built-in font") }
    }
}

/// What the key shows right now, after live values (clock, page number, timer) are filled in.
pub struct Look {
    pub icon: Option<Icon>,
    pub icon_color: Rgba,
    pub background: Rgba,
    pub image: Option<std::path::PathBuf>,
    pub title: String,
    pub title_color: Rgba,
    pub align: Align,
    /// In points, as stored in profiles (default 9).
    pub font_size: f32,
    pub show_title: bool,
    pub big_center: bool,
    /// Number in a white tile (Go to Page without a picture, as in the captured window).
    pub boxed: bool,
}

#[derive(Clone, Copy, PartialEq)]
pub enum Align {
    Top,
    Middle,
    Bottom,
}

pub fn look(k: &KeyAction, default_icon: Icon, page_dir: Option<&Path>, live_title: Option<String>) -> Look {
    let st = k.cur();
    let image = st.image.as_deref().map(|i| resolve_image(page_dir, i)).filter(|p| p.exists());
    let has_title = st.show_title && st.title.as_deref().is_some_and(|t| !t.trim().is_empty());
    let icon = match st.icon.as_deref() {
        Some("") => None,
        Some(s) => Icon::from_key(s).or(Some(default_icon)),
        // A plugin's key without its picture shows just its title (the plugin would draw the rest).
        None if default_icon == Icon::Plugin && has_title => None,
        None if image.is_none() && live_title.is_none() => Some(default_icon),
        None => None,
    };
    let plugin_placeholder = icon == Some(Icon::Plugin) && st.icon.is_none();
    let boxed = k.uuid == "com.elgato.streamdeck.page.goto" && image.is_none() && st.title.is_none();
    let icon = if boxed { None } else { icon };
    let align = match st.title_alignment.as_deref() {
        Some("top") => Align::Top,
        Some("middle") => Align::Middle,
        _ => Align::Bottom,
    };
    let big_center = live_title.is_some() && st.title.is_none();
    Look {
        icon,
        icon_color: st
            .icon_color
            .as_deref()
            .map(Rgba::hex)
            .unwrap_or(if plugin_placeholder { Rgba(0.45, 0.45, 0.45, 1.0) } else { Rgba::WHITE }),
        background: st.background.as_deref().map(Rgba::hex).unwrap_or(Rgba(0.0, 0.0, 0.0, 1.0)),
        image,
        title: live_title.filter(|_| st.title.is_none()).or(st.title.clone()).unwrap_or_default(),
        title_color: st.title_color.as_deref().map(Rgba::hex).unwrap_or(Rgba::WHITE),
        align: if big_center { Align::Middle } else { align },
        font_size: st.font_size.unwrap_or(9.0),
        show_title: st.show_title,
        big_center,
        boxed,
    }
}

pub struct ImageCache {
    map: HashMap<std::path::PathBuf, Option<image::RgbaImage>>,
}

impl ImageCache {
    pub fn new() -> Self {
        ImageCache { map: HashMap::new() }
    }
    fn get(&mut self, p: &Path) -> Option<&image::RgbaImage> {
        self.map
            .entry(p.to_path_buf())
            .or_insert_with(|| image::open(p).ok().map(|i| i.to_rgba8()))
            .as_ref()
    }
}

pub fn render(look: &Look, px: u32, fonts: &Fonts, cache: &mut ImageCache) -> Canvas {
    let mut cv = Canvas::new(px, px);
    cv.fill(look.background);
    let s = px as f32;
    if let Some(p) = &look.image {
        if let Some(img) = cache.get(p) {
            let scaled = image::imageops::resize(img, px, px, image::imageops::FilterType::Triangle);
            for (x, y, c) in scaled.enumerate_pixels() {
                cv.blend(x, y, Rgba(c[0] as f32 / 255.0, c[1] as f32 / 255.0, c[2] as f32 / 255.0, 1.0), c[3] as f32 / 255.0);
            }
        }
    }
    let has_title = look.show_title && !look.title.trim().is_empty();
    if let Some(icon) = look.icon {
        // Leave room for a bottom title like the original's icon-over-title keys.
        let (size, y) = if has_title && look.align == Align::Bottom { (s * 0.46, s * 0.14) } else { (s * 0.56, s * 0.22) };
        let x = (s - size) / 2.0;
        if icon == Icon::Dots {
            let cols = ["#4285f4", "#ea4335", "#fbbc05", "#34a853"];
            let pos = [(0.32, 0.32), (0.68, 0.32), (0.32, 0.68), (0.68, 0.68)];
            for (c, (u, v)) in cols.iter().zip(pos) {
                let mut p = icons::Pen { cv: &mut cv, x, y, s: size, c: Rgba::hex(c), lw: 0.1 };
                p.disc(u, v, 0.15);
            }
        } else {
            icons::draw_icon(&mut cv, icon, x, y, size, look.icon_color, 0.085);
        }
    }
    if look.boxed {
        let b = s * 0.4;
        cv.round_rect((s - b) / 2.0, (s - b) / 2.0, b, b, s * 0.06, Rgba::WHITE);
        draw_title(&mut cv, &look.title, s * 0.26, Rgba(0.0, 0.0, 0.0, 1.0), Align::Middle, fonts);
        return cv;
    }
    if has_title {
        let em = if look.big_center { s * 0.3 } else { look.font_size * TITLE_PX_PER_PT * s / 72.0 };
        draw_title(&mut cv, &look.title, em, look.title_color, look.align, fonts);
    }
    cv
}

fn draw_title(cv: &mut Canvas, text: &str, em: f32, color: Rgba, align: Align, fonts: &Fonts) {
    let font = fonts.bold.as_scaled(PxScale::from(em));
    let lines: Vec<&str> = text.lines().collect();
    let line_h = font.height() * 1.0;
    let total = line_h * lines.len() as f32;
    let s = cv.w as f32;
    let top = match align {
        Align::Top => s * 0.06,
        Align::Middle => (s - total) / 2.0,
        Align::Bottom => s - total - s * 0.06,
    };
    for (i, line) in lines.iter().enumerate() {
        let width: f32 = line.chars().map(|c| font.h_advance(font.glyph_id(c))).sum();
        let mut x = (s - width) / 2.0;
        let base = top + i as f32 * line_h + font.ascent();
        let mut glyphs = Vec::new();
        for c in line.chars() {
            let id = font.glyph_id(c);
            glyphs.push(id.with_scale_and_position(PxScale::from(em), ab_glyph::point(x, base)));
            x += font.h_advance(id);
        }
        // A thin dark outline keeps titles readable on pictures, like the original's outline.
        for pass in 0..2 {
            for g in &glyphs {
                if let Some(o) = fonts.bold.outline_glyph(g.clone()) {
                    let b = o.px_bounds();
                    let offs: &[(i32, i32)] = if pass == 0 { &[(-1, 0), (1, 0), (0, -1), (0, 1)] } else { &[(0, 0)] };
                    for (dx, dy) in offs {
                        let col = if pass == 0 { Rgba(0.0, 0.0, 0.0, 0.55) } else { color };
                        o.draw(|gx, gy, cov| {
                            let px = b.min.x as i32 + gx as i32 + dx;
                            let py = b.min.y as i32 + gy as i32 + dy;
                            if px >= 0 && py >= 0 {
                                cv.blend(px as u32, py as u32, col, cov);
                            }
                        });
                    }
                }
            }
        }
    }
}
