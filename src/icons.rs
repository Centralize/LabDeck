//! Simple line-art icons drawn from shapes (no artwork copied from the original), and a
//! tiny anti-aliased rasteriser used both for the window and for the hardware keys.

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Icon {
    MultiAction,
    MultiSwitch,
    Random,
    Navigation,
    Folder,
    Back,
    Profiles,
    PageNext,
    PagePrev,
    PageGoto,
    PageIndicator,
    Speaker,
    StopAudio,
    Deck,
    Brightness,
    Sleep,
    Timer,
    System,
    Globe,
    HotkeySwitch,
    Hotkey,
    TimedHotkey,
    Open,
    Apps,
    Close,
    Text,
    Media,
    MediaPrev,
    MediaNext,
    MediaStop,
    MediaMute,
    MediaVolUp,
    MediaVolDown,
    Custom,
    Plugin,
    Lock,
    ArrowUp,
    ArrowDown,
    ArrowLeft,
    ArrowRight,
    Gamepad,
    Note,
    Clock,
    Dots,
}

pub const ALL: &[Icon] = &[
    Icon::MultiAction, Icon::MultiSwitch, Icon::Random, Icon::Navigation, Icon::Folder, Icon::Back,
    Icon::Profiles, Icon::PageNext, Icon::PagePrev, Icon::PageGoto, Icon::PageIndicator, Icon::Speaker,
    Icon::StopAudio, Icon::Deck, Icon::Brightness, Icon::Sleep, Icon::Timer, Icon::System, Icon::Globe,
    Icon::HotkeySwitch, Icon::Hotkey, Icon::TimedHotkey, Icon::Open, Icon::Apps, Icon::Close, Icon::Text, Icon::Media,
    Icon::MediaPrev, Icon::MediaNext, Icon::MediaStop, Icon::MediaMute, Icon::MediaVolUp, Icon::MediaVolDown,
    Icon::Custom, Icon::Plugin, Icon::Lock, Icon::ArrowUp, Icon::ArrowDown, Icon::ArrowLeft,
    Icon::ArrowRight, Icon::Gamepad, Icon::Note, Icon::Clock, Icon::Dots,
];

impl Icon {
    pub fn key(self) -> &'static str {
        match self {
            Icon::MultiAction => "multi", Icon::MultiSwitch => "multiswitch", Icon::Random => "random",
            Icon::Navigation => "nav", Icon::Folder => "folder", Icon::Back => "back", Icon::Profiles => "profiles",
            Icon::PageNext => "pagenext", Icon::PagePrev => "pageprev", Icon::PageGoto => "pagegoto",
            Icon::PageIndicator => "pageind", Icon::Speaker => "speaker", Icon::StopAudio => "stopaudio",
            Icon::Deck => "deck", Icon::Brightness => "brightness", Icon::Sleep => "sleep", Icon::Timer => "timer",
            Icon::System => "system", Icon::Globe => "globe", Icon::HotkeySwitch => "hotkeyswitch",
            Icon::Hotkey => "hotkey", Icon::TimedHotkey => "timedhotkey", Icon::Open => "open", Icon::Apps => "apps", Icon::Close => "close",
            Icon::Text => "text", Icon::Media => "media", Icon::Custom => "custom", Icon::Plugin => "plugin",
            Icon::MediaPrev => "mediaprev", Icon::MediaNext => "medianext", Icon::MediaStop => "mediastop",
            Icon::MediaMute => "mediamute", Icon::MediaVolUp => "mediavolup", Icon::MediaVolDown => "mediavoldown",
            Icon::Lock => "lock", Icon::ArrowUp => "up", Icon::ArrowDown => "down", Icon::ArrowLeft => "left",
            Icon::ArrowRight => "right", Icon::Gamepad => "gamepad", Icon::Note => "note", Icon::Clock => "clock",
            Icon::Dots => "dots",
        }
    }
    pub fn from_key(k: &str) -> Option<Icon> {
        ALL.iter().copied().find(|i| i.key() == k)
    }
}

/// RGBA canvas with straight (non-premultiplied) colour.
pub struct Canvas {
    pub w: u32,
    pub h: u32,
    pub px: Vec<[f32; 4]>,
}

#[derive(Clone, Copy)]
pub struct Rgba(pub f32, pub f32, pub f32, pub f32);

impl Rgba {
    pub fn hex(s: &str) -> Rgba {
        let s = s.trim_start_matches('#');
        let v = u32::from_str_radix(s, 16).unwrap_or(0xffffff);
        Rgba(((v >> 16) & 255) as f32 / 255.0, ((v >> 8) & 255) as f32 / 255.0, (v & 255) as f32 / 255.0, 1.0)
    }
    pub const WHITE: Rgba = Rgba(1.0, 1.0, 1.0, 1.0);
}

impl Canvas {
    pub fn new(w: u32, h: u32) -> Self {
        Canvas { w, h, px: vec![[0.0; 4]; (w * h) as usize] }
    }

    pub fn fill(&mut self, c: Rgba) {
        for p in &mut self.px {
            *p = [c.0, c.1, c.2, c.3];
        }
    }

    pub fn blend(&mut self, x: u32, y: u32, c: Rgba, cov: f32) {
        if x >= self.w || y >= self.h || cov <= 0.0 {
            return;
        }
        let a = (c.3 * cov.min(1.0)).clamp(0.0, 1.0);
        let d = &mut self.px[(y * self.w + x) as usize];
        let out_a = a + d[3] * (1.0 - a);
        if out_a <= 0.0 {
            return;
        }
        for i in 0..3 {
            let s = [c.0, c.1, c.2][i];
            d[i] = (s * a + d[i] * d[3] * (1.0 - a)) / out_a;
        }
        d[3] = out_a;
    }

    /// Paint any shape given by a signed distance function (negative inside), in pixels.
    pub fn sdf(&mut self, bbox: (f32, f32, f32, f32), c: Rgba, f: impl Fn(f32, f32) -> f32) {
        let x0 = bbox.0.floor().max(0.0) as u32;
        let y0 = bbox.1.floor().max(0.0) as u32;
        let x1 = (bbox.2.ceil() as i64).clamp(0, self.w as i64) as u32;
        let y1 = (bbox.3.ceil() as i64).clamp(0, self.h as i64) as u32;
        for y in y0..y1 {
            for x in x0..x1 {
                let d = f(x as f32 + 0.5, y as f32 + 0.5);
                let cov = (0.5 - d).clamp(0.0, 1.0);
                self.blend(x, y, c, cov);
            }
        }
    }

    pub fn round_rect(&mut self, x: f32, y: f32, w: f32, h: f32, r: f32, c: Rgba) {
        let (cx, cy, hx, hy) = (x + w / 2.0, y + h / 2.0, w / 2.0, h / 2.0);
        self.sdf((x - 1.0, y - 1.0, x + w + 1.0, y + h + 1.0), c, |px, py| sd_round_box(px - cx, py - cy, hx, hy, r));
    }

    pub fn to_rgba8(&self) -> Vec<u8> {
        let mut v = Vec::with_capacity(self.px.len() * 4);
        for p in &self.px {
            for i in 0..4 {
                v.push((p[i].clamp(0.0, 1.0) * 255.0 + 0.5) as u8);
            }
        }
        v
    }

    pub fn to_rgb8_on(&self, bg: [f32; 3]) -> Vec<u8> {
        let mut v = Vec::with_capacity(self.px.len() * 3);
        for p in &self.px {
            for i in 0..3 {
                let c = p[i] * p[3] + bg[i] * (1.0 - p[3]);
                v.push((c.clamp(0.0, 1.0) * 255.0 + 0.5) as u8);
            }
        }
        v
    }
}

pub fn sd_round_box(px: f32, py: f32, hx: f32, hy: f32, r: f32) -> f32 {
    let qx = px.abs() - hx + r;
    let qy = py.abs() - hy + r;
    let ox = qx.max(0.0);
    let oy = qy.max(0.0);
    (ox * ox + oy * oy).sqrt() + qx.max(qy).min(0.0) - r
}

fn sd_segment(px: f32, py: f32, ax: f32, ay: f32, bx: f32, by: f32) -> f32 {
    let (pax, pay, bax, bay) = (px - ax, py - ay, bx - ax, by - ay);
    let h = ((pax * bax + pay * bay) / (bax * bax + bay * bay).max(1e-6)).clamp(0.0, 1.0);
    let (dx, dy) = (pax - bax * h, pay - bay * h);
    (dx * dx + dy * dy).sqrt()
}

/// Draws an icon in a unit square mapped to (x, y, size). `lw` is the line width in units.
pub struct Pen<'a> {
    pub cv: &'a mut Canvas,
    pub x: f32,
    pub y: f32,
    pub s: f32,
    pub c: Rgba,
    pub lw: f32,
}

impl Pen<'_> {
    fn p(&self, u: f32, v: f32) -> (f32, f32) {
        (self.x + u * self.s, self.y + v * self.s)
    }
    pub fn line(&mut self, a: (f32, f32), b: (f32, f32)) {
        let (ax, ay) = self.p(a.0, a.1);
        let (bx, by) = self.p(b.0, b.1);
        let hw = self.lw * self.s / 2.0;
        let bb = (ax.min(bx) - hw - 1.0, ay.min(by) - hw - 1.0, ax.max(bx) + hw + 1.0, ay.max(by) + hw + 1.0);
        self.cv.sdf(bb, self.c, |px, py| sd_segment(px, py, ax, ay, bx, by) - hw);
    }
    pub fn poly(&mut self, pts: &[(f32, f32)], closed: bool) {
        for w in pts.windows(2) {
            self.line(w[0], w[1]);
        }
        if closed && pts.len() > 2 {
            self.line(pts[pts.len() - 1], pts[0]);
        }
    }
    pub fn rect(&mut self, x: f32, y: f32, w: f32, h: f32, r: f32) {
        let (cx, cy) = self.p(x + w / 2.0, y + h / 2.0);
        let (hx, hy, rr, hw) = (w * self.s / 2.0, h * self.s / 2.0, r * self.s, self.lw * self.s / 2.0);
        let bb = (cx - hx - hw - 1.0, cy - hy - hw - 1.0, cx + hx + hw + 1.0, cy + hy + hw + 1.0);
        self.cv.sdf(bb, self.c, |px, py| sd_round_box(px - cx, py - cy, hx, hy, rr).abs() - hw);
    }
    pub fn fill_rect(&mut self, x: f32, y: f32, w: f32, h: f32, r: f32) {
        let (cx, cy) = self.p(x + w / 2.0, y + h / 2.0);
        let (hx, hy, rr) = (w * self.s / 2.0, h * self.s / 2.0, r * self.s);
        self.cv.sdf((cx - hx - 1.0, cy - hy - 1.0, cx + hx + 1.0, cy + hy + 1.0), self.c, |px, py| {
            sd_round_box(px - cx, py - cy, hx, hy, rr)
        });
    }
    pub fn circle(&mut self, u: f32, v: f32, r: f32) {
        let (cx, cy) = self.p(u, v);
        let (rr, hw) = (r * self.s, self.lw * self.s / 2.0);
        let e = rr + hw + 1.0;
        self.cv.sdf((cx - e, cy - e, cx + e, cy + e), self.c, |px, py| {
            (((px - cx).powi(2) + (py - cy).powi(2)).sqrt() - rr).abs() - hw
        });
    }
    pub fn disc(&mut self, u: f32, v: f32, r: f32) {
        let (cx, cy) = self.p(u, v);
        let rr = r * self.s;
        self.cv.sdf((cx - rr - 1.0, cy - rr - 1.0, cx + rr + 1.0, cy + rr + 1.0), self.c, |px, py| {
            ((px - cx).powi(2) + (py - cy).powi(2)).sqrt() - rr
        });
    }
    pub fn arc(&mut self, u: f32, v: f32, r: f32, a0: f32, a1: f32) {
        let n = 24;
        let pts: Vec<(f32, f32)> = (0..=n)
            .map(|i| {
                let a = a0 + (a1 - a0) * i as f32 / n as f32;
                (u + r * a.cos(), v + r * a.sin())
            })
            .collect();
        self.poly(&pts, false);
    }
    pub fn tri(&mut self, a: (f32, f32), b: (f32, f32), c: (f32, f32)) {
        let pa = self.p(a.0, a.1);
        let pb = self.p(b.0, b.1);
        let pc = self.p(c.0, c.1);
        let bb = (pa.0.min(pb.0).min(pc.0) - 1.0, pa.1.min(pb.1).min(pc.1) - 1.0, pa.0.max(pb.0).max(pc.0) + 1.0, pa.1.max(pb.1).max(pc.1) + 1.0);
        self.cv.sdf(bb, self.c, |x, y| {
            let edge = |p: (f32, f32), q: (f32, f32)| {
                let (ex, ey) = (q.0 - p.0, q.1 - p.1);
                let len = (ex * ex + ey * ey).sqrt().max(1e-6);
                ((x - p.0) * ey - (y - p.1) * ex) / len
            };
            let s = edge(pa, pb).max(edge(pb, pc)).max(edge(pc, pa));
            let s2 = (-edge(pa, pb)).max(-edge(pb, pc)).max(-edge(pc, pa));
            s.min(s2)
        });
    }
}

pub fn draw_icon(cv: &mut Canvas, icon: Icon, x: f32, y: f32, size: f32, c: Rgba, lw: f32) {
    let mut p = Pen { cv, x, y, s: size, c, lw };
    use std::f32::consts::PI;
    match icon {
        Icon::MultiAction => {
            p.poly(&[(0.3, 0.2), (0.5, 0.5), (0.3, 0.8)], false);
            p.poly(&[(0.5, 0.2), (0.7, 0.5), (0.5, 0.8)], false);
            p.poly(&[(0.12, 0.5), (0.38, 0.25), (0.88, 0.25), (0.88, 0.75), (0.38, 0.75)], true);
        }
        Icon::MultiSwitch => {
            p.rect(0.15, 0.2, 0.45, 0.45, 0.06);
            p.rect(0.4, 0.4, 0.45, 0.45, 0.06);
        }
        Icon::Random => {
            p.poly(&[(0.15, 0.3), (0.4, 0.3), (0.6, 0.7), (0.85, 0.7)], false);
            p.poly(&[(0.15, 0.7), (0.4, 0.7), (0.6, 0.3), (0.85, 0.3)], false);
            p.poly(&[(0.75, 0.2), (0.85, 0.3), (0.75, 0.4)], false);
            p.poly(&[(0.75, 0.6), (0.85, 0.7), (0.75, 0.8)], false);
        }
        Icon::Navigation => {
            p.rect(0.38, 0.08, 0.24, 0.24, 0.03);
            p.rect(0.08, 0.62, 0.24, 0.24, 0.03);
            p.rect(0.38, 0.62, 0.24, 0.24, 0.03);
            p.rect(0.68, 0.62, 0.24, 0.24, 0.03);
            p.poly(&[(0.44, 0.24), (0.5, 0.16), (0.56, 0.24)], false);
            p.line((0.5, 0.32), (0.5, 0.62));
        }
        Icon::Folder => {
            p.poly(&[(0.12, 0.25), (0.4, 0.25), (0.48, 0.35), (0.88, 0.35), (0.88, 0.78), (0.12, 0.78)], true);
            p.line((0.12, 0.45), (0.88, 0.45));
        }
        Icon::Back | Icon::ArrowLeft => {
            p.line((0.15, 0.5), (0.85, 0.5));
            p.poly(&[(0.45, 0.2), (0.15, 0.5), (0.45, 0.8)], false);
        }
        Icon::ArrowRight | Icon::PageNext => {
            p.line((0.15, 0.5), (0.85, 0.5));
            p.poly(&[(0.55, 0.2), (0.85, 0.5), (0.55, 0.8)], false);
        }
        Icon::ArrowUp => {
            p.line((0.5, 0.15), (0.5, 0.85));
            p.poly(&[(0.2, 0.45), (0.5, 0.15), (0.8, 0.45)], false);
        }
        Icon::ArrowDown => {
            p.line((0.5, 0.15), (0.5, 0.85));
            p.poly(&[(0.2, 0.55), (0.5, 0.85), (0.8, 0.55)], false);
        }
        Icon::Profiles => {
            p.rect(0.12, 0.3, 0.5, 0.5, 0.06);
            p.poly(&[(0.3, 0.3), (0.3, 0.15), (0.88, 0.15), (0.88, 0.62), (0.62, 0.62)], false);
        }
        Icon::PagePrev => {
            p.rect(0.3, 0.15, 0.55, 0.7, 0.05);
            p.poly(&[(0.3, 0.3), (0.12, 0.5), (0.3, 0.7)], false);
        }
        Icon::PageGoto => {
            p.rect(0.2, 0.15, 0.6, 0.7, 0.05);
            p.line((0.35, 0.4), (0.65, 0.4));
            p.line((0.35, 0.6), (0.65, 0.6));
        }
        Icon::PageIndicator => {
            p.rect(0.15, 0.3, 0.7, 0.4, 0.2);
            p.disc(0.35, 0.5, 0.06);
            p.disc(0.5, 0.5, 0.06);
            p.disc(0.65, 0.5, 0.06);
        }
        Icon::Speaker => {
            p.poly(&[(0.12, 0.38), (0.3, 0.38), (0.5, 0.2), (0.5, 0.8), (0.3, 0.62), (0.12, 0.62)], true);
            p.arc(0.5, 0.5, 0.17, -PI / 3.0, PI / 3.0);
            p.arc(0.5, 0.5, 0.33, -PI / 3.0, PI / 3.0);
        }
        Icon::StopAudio | Icon::MediaMute => {
            p.poly(&[(0.12, 0.38), (0.3, 0.38), (0.5, 0.2), (0.5, 0.8), (0.3, 0.62), (0.12, 0.62)], true);
            p.line((0.62, 0.38), (0.86, 0.62));
            p.line((0.86, 0.38), (0.62, 0.62));
        }
        Icon::MediaVolUp | Icon::MediaVolDown => {
            p.poly(&[(0.12, 0.38), (0.3, 0.38), (0.5, 0.2), (0.5, 0.8), (0.3, 0.62), (0.12, 0.62)], true);
            p.line((0.62, 0.5), (0.88, 0.5));
            if icon == Icon::MediaVolUp {
                p.line((0.75, 0.37), (0.75, 0.63));
            }
        }
        Icon::MediaPrev => {
            p.line((0.22, 0.25), (0.22, 0.75));
            p.poly(&[(0.8, 0.25), (0.35, 0.5), (0.8, 0.75)], true);
        }
        Icon::MediaNext => {
            p.poly(&[(0.2, 0.25), (0.65, 0.5), (0.2, 0.75)], true);
            p.line((0.78, 0.25), (0.78, 0.75));
        }
        Icon::MediaStop => {
            p.rect(0.25, 0.25, 0.5, 0.5, 0.04);
        }
        Icon::Deck => {
            p.rect(0.08, 0.22, 0.84, 0.56, 0.06);
            for i in 0..3 {
                for j in 0..2 {
                    p.disc(0.3 + 0.2 * i as f32, 0.4 + 0.2 * j as f32, 0.04);
                }
            }
        }
        Icon::Brightness => {
            p.circle(0.5, 0.5, 0.16);
            for i in 0..8 {
                let a = i as f32 * PI / 4.0;
                p.line((0.5 + 0.28 * a.cos(), 0.5 + 0.28 * a.sin()), (0.5 + 0.4 * a.cos(), 0.5 + 0.4 * a.sin()));
            }
        }
        Icon::Sleep => {
            p.arc(0.5, 0.5, 0.32, PI * 0.35, PI * 1.85);
            p.arc(0.68, 0.36, 0.25, PI * 0.62, PI * 1.12);
        }
        Icon::Timer | Icon::Clock => {
            p.circle(0.5, 0.55, 0.33);
            p.line((0.5, 0.55), (0.5, 0.35));
            p.line((0.5, 0.55), (0.65, 0.62));
            if icon == Icon::Timer {
                p.line((0.42, 0.12), (0.58, 0.12));
            }
        }
        Icon::System => {
            p.rect(0.3, 0.1, 0.4, 0.8, 0.06);
            p.line((0.42, 0.25), (0.58, 0.25));
            p.disc(0.5, 0.72, 0.04);
        }
        Icon::Globe => {
            p.circle(0.5, 0.5, 0.36);
            p.line((0.14, 0.5), (0.86, 0.5));
            p.line((0.5, 0.14), (0.5, 0.86));
            p.arc(0.5, 0.5, 0.36, -PI / 2.0, PI / 2.0);
            let pts: Vec<(f32, f32)> = (0..=20).map(|i| {
                let a = -PI / 2.0 + PI * i as f32 / 20.0;
                (0.5 + 0.18 * a.cos(), 0.5 + 0.36 * a.sin())
            }).collect();
            p.poly(&pts, false);
            let pts: Vec<(f32, f32)> = (0..=20).map(|i| {
                let a = -PI / 2.0 + PI * i as f32 / 20.0;
                (0.5 - 0.18 * a.cos(), 0.5 + 0.36 * a.sin())
            }).collect();
            p.poly(&pts, false);
        }
        Icon::HotkeySwitch => {
            p.rect(0.1, 0.25, 0.8, 0.5, 0.06);
            p.fill_rect(0.18, 0.33, 0.3, 0.34, 0.04);
        }
        Icon::Hotkey => {
            p.rect(0.15, 0.15, 0.7, 0.32, 0.05);
            p.rect(0.15, 0.55, 0.32, 0.3, 0.05);
            p.rect(0.53, 0.55, 0.32, 0.3, 0.05);
        }
        Icon::TimedHotkey => {
            p.rect(0.08, 0.38, 0.5, 0.42, 0.06);
            p.line((0.2, 0.59), (0.46, 0.59));
            p.circle(0.68, 0.34, 0.22);
            p.line((0.68, 0.34), (0.68, 0.22));
            p.line((0.68, 0.34), (0.77, 0.4));
        }
        Icon::Open => {
            p.poly(&[(0.55, 0.15), (0.85, 0.15), (0.85, 0.45)], false);
            p.line((0.85, 0.15), (0.5, 0.5));
            p.poly(&[(0.4, 0.2), (0.15, 0.2), (0.15, 0.85), (0.8, 0.85), (0.8, 0.6)], false);
        }
        Icon::Apps => {
            for i in 0..3 {
                for j in 0..3 {
                    p.disc(0.25 + 0.25 * i as f32, 0.25 + 0.25 * j as f32, 0.065);
                }
            }
        }
        Icon::Close => {
            p.rect(0.15, 0.15, 0.7, 0.7, 0.05);
            p.line((0.35, 0.35), (0.65, 0.65));
            p.line((0.65, 0.35), (0.35, 0.65));
        }
        Icon::Text => {
            p.line((0.2, 0.18), (0.8, 0.18));
            p.line((0.5, 0.18), (0.5, 0.85));
            p.line((0.2, 0.18), (0.2, 0.28));
            p.line((0.8, 0.18), (0.8, 0.28));
        }
        Icon::Media => {
            p.poly(&[(0.15, 0.22), (0.55, 0.5), (0.15, 0.78)], true);
            p.line((0.7, 0.22), (0.7, 0.78));
            p.line((0.85, 0.22), (0.85, 0.78));
        }
        Icon::Custom => {
            p.circle(0.5, 0.5, 0.36);
            p.tri((0.4, 0.3), (0.7, 0.5), (0.4, 0.7));
        }
        Icon::Plugin => {
            p.rect(0.15, 0.15, 0.7, 0.7, 0.12);
            p.line((0.5, 0.32), (0.5, 0.68));
            p.line((0.32, 0.5), (0.68, 0.5));
        }
        Icon::Lock => {
            p.rect(0.22, 0.45, 0.56, 0.42, 0.06);
            p.arc(0.5, 0.45, 0.18, PI, 2.0 * PI);
            p.line((0.32, 0.45), (0.32, 0.4));
            p.line((0.68, 0.45), (0.68, 0.4));
            p.disc(0.5, 0.64, 0.05);
        }
        Icon::Gamepad => {
            p.rect(0.08, 0.3, 0.84, 0.42, 0.2);
            p.line((0.22, 0.51), (0.38, 0.51));
            p.line((0.3, 0.43), (0.3, 0.59));
            p.disc(0.66, 0.45, 0.04);
            p.disc(0.76, 0.56, 0.04);
        }
        Icon::Note => {
            p.line((0.4, 0.2), (0.4, 0.7));
            p.line((0.75, 0.15), (0.75, 0.62));
            p.line((0.4, 0.2), (0.75, 0.15));
            p.disc(0.32, 0.72, 0.09);
            p.disc(0.67, 0.64, 0.09);
        }
        Icon::Dots => {
            p.disc(0.32, 0.32, 0.13);
            p.disc(0.68, 0.32, 0.13);
            p.disc(0.32, 0.68, 0.13);
            p.disc(0.68, 0.68, 0.13);
        }
    }
}
