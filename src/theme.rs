//! Colours and measurements taken from the captured window (capture/window.png).

use egui::Color32;

pub const WIN_W: f32 = 1705.0;
pub const WIN_H: f32 = 1380.0;

pub const TITLE_H: f32 = 31.0;
pub const RIGHT_W: f32 = 295.0;

pub const KEY: f32 = 78.0;
pub const KEY_PITCH: f32 = 93.0;
pub const KEY_RADIUS: f32 = 12.0;
pub const KEY_BORDER: f32 = 3.0;

pub const ROW_CAT: f32 = 50.0;
pub const ROW_ACTION: f32 = 40.0;
/// Device view share of the space under the title bar (806 of 1349 px).
pub const SPLIT: f32 = 0.5975;

pub const fn hex(v: u32) -> Color32 {
    Color32::from_rgb((v >> 16) as u8, (v >> 8) as u8, v as u8)
}

pub const TITLE_BG: Color32 = hex(0x20201f);
pub const CANVAS: Color32 = hex(0x2d2d2d);
pub const PANEL: Color32 = hex(0x292929);
pub const PANEL_OPEN: Color32 = hex(0x222222);
pub const DIVIDER_A: Color32 = hex(0x272727);
pub const DIVIDER_B: Color32 = hex(0x232323);
pub const SPLIT_A: Color32 = hex(0x262626);
pub const SPLIT_B: Color32 = hex(0x212121);
pub const ROW_LINE: Color32 = hex(0x242424);
pub const FIELD: Color32 = hex(0x3b3b3b);
pub const SEGMENT_ON: Color32 = hex(0x343434);
pub const PILL: Color32 = hex(0x373737);
pub const PILL_ON: Color32 = hex(0xc8c8c8);
pub const KEY_EMPTY: Color32 = hex(0x222222);
pub const KEY_INNER: Color32 = hex(0x1f1f1f);
pub const KEY_RING: Color32 = hex(0x484848);
pub const KEY_SELECTED: Color32 = hex(0xd9d9d9);
pub const SCROLL: Color32 = hex(0x666666);
pub const TEXT: Color32 = hex(0xffffff);
pub const TEXT_SOFT: Color32 = hex(0xdedede);
pub const TEXT_DIM: Color32 = hex(0xa2a09c);
pub const TEXT_MUTED: Color32 = hex(0x8a8a8a);
pub const ACCENT: Color32 = hex(0x7841ff);
pub const AVATAR: Color32 = hex(0x22837d);
pub const CLOSE_HOVER: Color32 = hex(0xe81123);

/// Names of the window buttons. The original keeps the frame Windows draws, so its buttons are
/// named in the display language (the capture: "Minimer", "Maksimer", "Luk" on Danish Windows;
/// "Minimize", "Maximize", "Close" on English). LabDeck draws its own and names them the same way.
pub struct CaptionNames {
    pub minimize: &'static str,
    pub maximize: &'static str,
    pub restore: &'static str,
    pub close: &'static str,
}

const fn names(minimize: &'static str, maximize: &'static str, restore: &'static str, close: &'static str) -> CaptionNames {
    CaptionNames { minimize, maximize, restore, close }
}

/// Two-letter code of the display language ("da", "en"...).
fn display_language() -> String {
    #[cfg(target_os = "windows")]
    {
        #[link(name = "kernel32")]
        unsafe extern "system" {
            safe fn GetUserDefaultUILanguage() -> u16;
        }
        // Primary language id: the low 10 bits of the LANGID.
        return match GetUserDefaultUILanguage() & 0x3ff {
            0x06 => "da",
            0x07 => "de",
            0x0a => "es",
            0x0c => "fr",
            0x13 => "nl",
            0x14 => "nb",
            0x1d => "sv",
            _ => "en",
        }
        .to_string();
    }
    #[cfg(not(target_os = "windows"))]
    {
        // Same order as gettext: LANGUAGE (a list), then LC_ALL, LC_MESSAGES, LANG.
        for var in ["LANGUAGE", "LC_ALL", "LC_MESSAGES", "LANG"] {
            if let Ok(v) = std::env::var(var) {
                let first = v.split(':').next().unwrap_or("").trim().to_lowercase();
                if !first.is_empty() && first != "c" && first != "posix" && !first.starts_with("c.") {
                    return first.chars().take_while(|c| c.is_ascii_alphabetic()).collect();
                }
            }
        }
        "en".to_string()
    }
}

pub fn caption_names_for(lang: &str) -> CaptionNames {
    match lang {
        "da" => names("Minimer", "Maksimer", "Gendan", "Luk"),
        "de" => names("Minimieren", "Maximieren", "Verkleinern", "Schließen"),
        "es" => names("Minimizar", "Maximizar", "Restaurar", "Cerrar"),
        "fr" => names("Réduire", "Agrandir", "Restaurer", "Fermer"),
        "nl" => names("Minimaliseren", "Maximaliseren", "Vorige grootte", "Sluiten"),
        "nb" | "no" | "nn" => names("Minimer", "Maksimer", "Gjenopprett", "Lukk"),
        "sv" => names("Minimera", "Maximera", "Återställ nedåt", "Stäng"),
        _ => names("Minimize", "Maximize", "Restore", "Close"),
    }
}

/// The window buttons' names for this computer's display language (read once).
pub fn caption_names() -> &'static CaptionNames {
    static NAMES: std::sync::OnceLock<CaptionNames> = std::sync::OnceLock::new();
    NAMES.get_or_init(|| caption_names_for(&display_language()))
}
