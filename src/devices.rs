//! Stream Deck hardware models, from the device registry built in function 0x1401348f0
//! and the per-model constructors it points at (see MECHANICS.md).

pub const ELGATO_VID: u16 = 0x0fd9;

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Protocol {
    /// Original 15-key deck: BMP images, 1 report per page with a 16-byte header (estimated).
    Gen1,
    /// Mini: BMP images rotated, 16-byte header (estimated).
    Mini,
    /// MK.2 / XL / + / Neo and later: JPEG images, 8-byte header (estimated).
    Gen2,
    /// Pedal: no screens.
    NoScreen,
}

#[derive(Clone, Copy, Debug)]
pub struct DeviceModel {
    pub name: &'static str,
    /// Elgato model code; profiles store it in manifest.json as Device.Model.
    pub model: &'static str,
    pub pid: u16,
    pub cols: usize,
    pub rows: usize,
    /// Key picture size on the hardware, in pixels.
    pub key_px: u32,
    pub protocol: Protocol,
}

pub const MODELS: &[DeviceModel] = &[
    DeviceModel { name: "Stream Deck", model: "20GAA9901", pid: 0x0060, cols: 5, rows: 3, key_px: 72, protocol: Protocol::Gen1 },
    DeviceModel { name: "Stream Deck", model: "20GAA9902", pid: 0x006d, cols: 5, rows: 3, key_px: 72, protocol: Protocol::Gen2 },
    DeviceModel { name: "Stream Deck MK.2", model: "20GBA9901", pid: 0x0080, cols: 5, rows: 3, key_px: 72, protocol: Protocol::Gen2 },
    DeviceModel { name: "Stream Deck", model: "20GBL9901", pid: 0x00a5, cols: 5, rows: 3, key_px: 72, protocol: Protocol::Gen2 },
    DeviceModel { name: "Stream Deck Mini", model: "20GAI9901", pid: 0x0063, cols: 3, rows: 2, key_px: 80, protocol: Protocol::Mini },
    DeviceModel { name: "Stream Deck Mini", model: "20GAI9902", pid: 0x0090, cols: 3, rows: 2, key_px: 80, protocol: Protocol::Mini },
    DeviceModel { name: "Stream Deck XL", model: "20GAT9901", pid: 0x006c, cols: 8, rows: 4, key_px: 96, protocol: Protocol::Gen2 },
    DeviceModel { name: "Stream Deck XL", model: "20GAT9902", pid: 0x008f, cols: 8, rows: 4, key_px: 96, protocol: Protocol::Gen2 },
    DeviceModel { name: "Stream Deck +", model: "20GBD9901", pid: 0x0084, cols: 4, rows: 2, key_px: 120, protocol: Protocol::Gen2 },
    DeviceModel { name: "Stream Deck Neo", model: "20GBJ9901", pid: 0x009a, cols: 4, rows: 2, key_px: 96, protocol: Protocol::Gen2 },
    DeviceModel { name: "Stream Deck Module 6", model: "20GBH9901", pid: 0x00b8, cols: 3, rows: 2, key_px: 80, protocol: Protocol::Mini },
    DeviceModel { name: "Stream Deck Module 15", model: "20GBG9901", pid: 0x00b9, cols: 5, rows: 3, key_px: 72, protocol: Protocol::Gen2 },
    DeviceModel { name: "Stream Deck Module 32", model: "20GBK9901", pid: 0x00ba, cols: 8, rows: 4, key_px: 96, protocol: Protocol::Gen2 },
    DeviceModel { name: "Stream Deck Pedal", model: "20GBF9901", pid: 0x0086, cols: 3, rows: 1, key_px: 72, protocol: Protocol::NoScreen },
];

pub fn by_pid(pid: u16) -> Option<&'static DeviceModel> {
    MODELS.iter().find(|m| m.pid == pid)
}

pub fn by_model(code: &str) -> Option<&'static DeviceModel> {
    MODELS.iter().find(|m| m.model == code)
}

/// Per-model value each constructor stores at offset 0xbc: 0x5a (90) for the XL, + and
/// 20GAA9902 (0x1405f0a80, 0x1405f0c10, 0x1405f00f0), 0x61 (97) for MK.2, 20GBL9901 and Neo
/// (0x1405f0280, 0x1405f1300, 0x1405f1160). Its use as the JPEG quality is inferred: the
/// encoder call (tjCompress2, 0x140b51280) is only reached through a pointer.
pub fn jpeg_quality(m: &DeviceModel) -> u8 {
    match m.model {
        "20GBA9901" | "20GBL9901" | "20GBJ9901" | "20GBG9901" => 97,
        _ => 90,
    }
}

/// The XL: the model in the captured window.
pub fn default_model() -> &'static DeviceModel {
    by_pid(0x008f).unwrap()
}
