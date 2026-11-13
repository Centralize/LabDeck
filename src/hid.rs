//! Talks to the Stream Deck over USB HID (hidapi, like the original). Runs on its own
//! thread; the window sends pictures and brightness, and gets key presses back.
//! The report layouts are the publicly documented ones (marked estimated in MECHANICS.md).

use crate::devices::{self, DeviceModel, Protocol};
use crate::keys;
use std::collections::HashMap;
use std::sync::mpsc::{Receiver, Sender};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

/// Keys this thread presses itself the moment the deck reports them, without waiting for the
/// window (which only looks every 100 ms while it's minimised or in the tray): the Hotkey,
/// Hotkey Switch and Multimedia keys of the page on the deck, filled in by the window.
#[derive(Default)]
pub struct Direct {
    /// Key index (row-major) → what it holds down.
    pub keys: HashMap<usize, keys::Combo>,
    /// (first repeat, between repeats) in ms while a key is held.
    pub repeat: (u64, u64),
    /// Other key presses sent to the window so far. A press may change the page, so the
    /// table is only used while the window has caught up with all of them (`seen` equals this).
    pub sent: u64,
    pub seen: u64,
}

pub type DirectKeys = Arc<Mutex<Direct>>;

/// Hold ids for keys pressed here (one per key index).
const DIRECT_ID: u64 = 1 << 40;

pub enum Cmd {
    /// Key index (row-major), RGB pixels at the model's key size.
    Image(usize, Vec<u8>),
    Brightness(u8),
    /// Forget which pictures were sent, so everything is sent again.
    Refresh,
}

pub enum Event {
    Connected { model: &'static DeviceModel, serial: String },
    Disconnected,
    /// Key index, down, and whether this thread already pressed (or let go of) its keys.
    Key(usize, bool, bool),
    Problem(String),
}

pub fn spawn(cmds: Receiver<Cmd>, events: Sender<Event>, direct: DirectKeys, wake: impl Fn() + Send + 'static) {
    std::thread::Builder::new()
        .name("streamdeck-hid".into())
        .spawn(move || run(cmds, events, direct, wake))
        .ok();
}

fn run(cmds: Receiver<Cmd>, events: Sender<Event>, direct: DirectKeys, wake: impl Fn()) {
    let mut api = match hidapi::HidApi::new() {
        Ok(a) => a,
        Err(e) => {
            let _ = events.send(Event::Problem(format!("USB access is not available: {e}")));
            return;
        }
    };
    let mut last_problem = String::new();
    loop {
        let _ = api.refresh_devices();
        let found = api
            .device_list()
            .filter(|d| d.vendor_id() == devices::ELGATO_VID)
            .find_map(|d| devices::by_pid(d.product_id()).map(|m| (d.clone(), m)));
        if let Some((info, model)) = found {
            match info.open_device(&api) {
                Ok(dev) => {
                    let serial = info.serial_number().unwrap_or("").to_string();
                    let _ = events.send(Event::Connected { model, serial });
                    wake();
                    session(&dev, model, &cmds, &events, &direct, &wake);
                    let _ = events.send(Event::Disconnected);
                    wake();
                }
                Err(e) => {
                    let msg = format!(
                        "Found a {} but could not open it ({e}). On Linux, install the udev rule from the app folder and replug the device.",
                        model.name
                    );
                    if msg != last_problem {
                        let _ = events.send(Event::Problem(msg.clone()));
                        wake();
                        last_problem = msg;
                    }
                }
            }
        }
        // Drain commands while nothing is connected, then look again.
        let until = Instant::now() + Duration::from_millis(1500);
        while Instant::now() < until {
            match cmds.recv_timeout(Duration::from_millis(100)) {
                Ok(_) => {}
                Err(std::sync::mpsc::RecvTimeoutError::Disconnected) => return,
                Err(_) => {}
            }
        }
    }
}

fn session(dev: &hidapi::HidDevice, m: &DeviceModel, cmds: &Receiver<Cmd>, events: &Sender<Event>, direct: &DirectKeys, wake: &impl Fn()) {
    let n = m.cols * m.rows;
    let mut pressed = vec![false; n];
    // Keys whose combination this thread is holding down.
    let mut held_here = vec![false; n];
    session_loop(dev, m, cmds, events, direct, wake, &mut pressed, &mut held_here);
    // Unplugged (or stopped) with keys held: let them go.
    for (i, h) in held_here.iter().enumerate() {
        if *h {
            keys::let_go(DIRECT_ID + i as u64);
        }
    }
}

/// A key changed on the deck: press or let go of its keys here when the table says so, and
/// say whether that was done. Everything else goes to the window, and the table waits until
/// the window has handled it (it may have changed the page).
fn direct_key(direct: &DirectKeys, held_here: &mut [bool], i: usize, down: bool) -> bool {
    if !down && held_here[i] {
        held_here[i] = false;
        keys::let_go(DIRECT_ID + i as u64);
        return true;
    }
    let Ok(mut d) = direct.lock() else { return false };
    if down && d.seen == d.sent {
        if let Some(c) = d.keys.get(&i).cloned() {
            keys::hold(DIRECT_ID + i as u64, c, Some(d.repeat));
            held_here[i] = true;
            return true;
        }
    }
    d.sent += 1;
    false
}

#[allow(clippy::too_many_arguments)]
fn session_loop(
    dev: &hidapi::HidDevice,
    m: &DeviceModel,
    cmds: &Receiver<Cmd>,
    events: &Sender<Event>,
    direct: &DirectKeys,
    wake: &impl Fn(),
    pressed: &mut [bool],
    held_here: &mut [bool],
) {
    let n = m.cols * m.rows;
    let mut sent: Vec<Option<u64>> = vec![None; n];
    let mut buf = [0u8; 512];
    // Reset the deck's screens first: 32-byte feature report 03 02 (function 0x14062da40).
    if m.protocol == Protocol::Gen2 {
        let mut r = [0u8; 32];
        r[0] = 0x03;
        r[1] = 0x02;
        let _ = dev.send_feature_report(&r);
    }
    // Like the original's device thread (ESDDeviceComm::run, 0x140595c70): queued work
    // (pictures, brightness) is done as soon as it arrives, and the "Read Keys" task runs
    // every KEY_POLL_MS with a non-blocking read (0x140594250 re-queues it 50 ms ahead;
    // 0x1406154e0 reads with timeout 0).
    let mut next_poll = Instant::now();
    loop {
        let wait = next_poll.saturating_duration_since(Instant::now());
        let first = match cmds.recv_timeout(wait) {
            Ok(c) => Some(c),
            Err(std::sync::mpsc::RecvTimeoutError::Timeout) => None,
            Err(std::sync::mpsc::RecvTimeoutError::Disconnected) => return,
        };
        let mut pending: Vec<Cmd> = first.into_iter().collect();
        while let Ok(c) = cmds.try_recv() {
            pending.push(c);
        }
        for c in pending {
            match c {
                Cmd::Image(i, rgb) if i < n && m.protocol != Protocol::NoScreen => {
                    let h = hash(&rgb);
                    if sent[i] == Some(h) {
                        continue;
                    }
                    if write_image(dev, m, i, &rgb).is_err() {
                        return;
                    }
                    sent[i] = Some(h);
                }
                Cmd::Brightness(p) => {
                    let _ = set_brightness(dev, m, p);
                }
                Cmd::Refresh => sent.iter_mut().for_each(|s| *s = None),
                _ => {}
            }
        }
        if Instant::now() < next_poll {
            continue;
        }
        next_poll = Instant::now() + Duration::from_millis(KEY_POLL_MS);
        // Drain every report that arrived since the last poll.
        loop {
            match dev.read_timeout(&mut buf, 0) {
                Ok(0) => break,
                Ok(len) => {
                    let states = key_states(m, &buf[..len]);
                    for (i, down) in states.into_iter().enumerate().take(n) {
                        if down != pressed[i] {
                            pressed[i] = down;
                            let handled = direct_key(direct, held_here, i, down);
                            let _ = events.send(Event::Key(i, down, handled));
                            wake();
                        }
                    }
                }
                Err(_) => return,
            }
        }
    }
}

/// The original's key polling period (function 0x140594250: next run = now + 50 ms).
pub const KEY_POLL_MS: u64 = 50;

fn hash(v: &[u8]) -> u64 {
    let mut h: u64 = 0xcbf2_9ce4_8422_2325;
    for b in v {
        h = (h ^ *b as u64).wrapping_mul(0x100_0000_01b3);
    }
    h
}

/// The original 15-key deck numbers its keys right-to-left.
fn origin_key(m: &DeviceModel, i: usize) -> usize {
    if m.protocol == Protocol::Gen1 {
        let col = i % m.cols;
        i - col + (m.cols - 1 - col)
    } else {
        i
    }
}

fn key_states(m: &DeviceModel, data: &[u8]) -> Vec<bool> {
    let n = m.cols * m.rows;
    // Newer decks (function 0x14061abe0): 512-byte input reports, byte 1 = event type
    // (0 = keys), bytes 2..3 = payload length, key states from byte 4.
    // Original/Mini (function 0x1406185e0): 64-byte reports, key states from byte 1.
    let (off, ok) = match m.protocol {
        Protocol::Gen2 | Protocol::NoScreen => (4, data.first() == Some(&0x01) && data.get(1) == Some(&0x00)),
        Protocol::Gen1 | Protocol::Mini => (1, data.first() == Some(&0x01)),
    };
    let mut out = vec![false; n];
    if !ok {
        return out;
    }
    for i in 0..n {
        if let Some(&b) = data.get(off + i) {
            out[origin_key(m, i)] = b != 0;
        }
    }
    out
}

fn set_brightness(dev: &hidapi::HidDevice, m: &DeviceModel, pct: u8) -> hidapi::HidResult<()> {
    let pct = pct.min(100);
    match m.protocol {
        Protocol::Gen2 | Protocol::NoScreen => {
            let mut r = [0u8; 32];
            r[0] = 0x03;
            r[1] = 0x08;
            r[2] = pct;
            dev.send_feature_report(&r)
        }
        Protocol::Gen1 | Protocol::Mini => {
            let mut r = [0u8; 17];
            r[..6].copy_from_slice(&[0x05, 0x55, 0xaa, 0xd1, 0x01, pct]);
            dev.send_feature_report(&r)
        }
    }
}

/// Turns a picture the way the hardware expects (most models show it rotated 180°).
fn orient(m: &DeviceModel, rgb: &[u8]) -> image::RgbImage {
    let px = m.key_px;
    let img = image::RgbImage::from_raw(px, px, rgb.to_vec()).unwrap_or_else(|| image::RgbImage::new(px, px));
    match (m.protocol, m.pid) {
        (Protocol::Gen2, 0x0084) => img,
        (Protocol::Gen2, _) => image::imageops::rotate180(&img),
        (Protocol::Gen1, _) => image::imageops::flip_horizontal(&img),
        (Protocol::Mini, _) => image::imageops::rotate90(&img),
        _ => img,
    }
}

fn write_image(dev: &hidapi::HidDevice, m: &DeviceModel, key: usize, rgb: &[u8]) -> hidapi::HidResult<()> {
    let img = orient(m, rgb);
    match m.protocol {
        Protocol::Gen2 => {
            let mut jpg = Vec::new();
            let enc = image::codecs::jpeg::JpegEncoder::new_with_quality(&mut jpg, devices::jpeg_quality(m));
            let _ = img.write_with_encoder(enc);
            // Function 0x14062dcf0: 1024-byte reports, 8-byte header, 1016 (0x3f8) bytes of JPEG each.
            const LEN: usize = 1024;
            const HDR: usize = 8;
            const PAYLOAD: usize = 1016;
            for (page, chunk) in jpg.chunks(PAYLOAD).enumerate() {
                let last = (page + 1) * PAYLOAD >= jpg.len();
                let mut r = [0u8; LEN];
                r[..HDR].copy_from_slice(&[
                    0x02, 0x07, key as u8, last as u8,
                    (chunk.len() & 0xff) as u8, (chunk.len() >> 8) as u8,
                    (page & 0xff) as u8, (page >> 8) as u8,
                ]);
                r[HDR..HDR + chunk.len()].copy_from_slice(chunk);
                dev.write(&r)?;
            }
            Ok(())
        }
        Protocol::Gen1 | Protocol::Mini => {
            let bmp = bmp(&img);
            let key = origin_key(m, key);
            let len = if m.protocol == Protocol::Gen1 { 8191 } else { 1024 };
            const HDR: usize = 16;
            for (page, chunk) in bmp.chunks(len - HDR).enumerate() {
                let last = (page + 1) * (len - HDR) >= bmp.len();
                let mut r = vec![0u8; len];
                let pno = if m.protocol == Protocol::Gen1 { page + 1 } else { page };
                r[..6].copy_from_slice(&[0x02, 0x01, pno as u8, 0x00, last as u8, key as u8 + 1]);
                r[HDR..HDR + chunk.len()].copy_from_slice(chunk);
                dev.write(&r)?;
            }
            Ok(())
        }
        Protocol::NoScreen => Ok(()),
    }
}

/// 24-bit bottom-up BMP, as the first-generation decks take.
fn bmp(img: &image::RgbImage) -> Vec<u8> {
    let (w, h) = img.dimensions();
    let row = (w * 3).div_ceil(4) * 4;
    let size = 54 + row * h;
    let mut v = Vec::with_capacity(size as usize);
    v.extend_from_slice(b"BM");
    v.extend_from_slice(&size.to_le_bytes());
    v.extend_from_slice(&[0, 0, 0, 0]);
    v.extend_from_slice(&54u32.to_le_bytes());
    v.extend_from_slice(&40u32.to_le_bytes());
    v.extend_from_slice(&(w as i32).to_le_bytes());
    v.extend_from_slice(&(h as i32).to_le_bytes());
    v.extend_from_slice(&1u16.to_le_bytes());
    v.extend_from_slice(&24u16.to_le_bytes());
    v.extend_from_slice(&[0; 4]);
    v.extend_from_slice(&(row * h).to_le_bytes());
    v.extend_from_slice(&[0x13, 0x0b, 0, 0, 0x13, 0x0b, 0, 0]);
    v.extend_from_slice(&[0; 8]);
    for y in (0..h).rev() {
        let start = v.len();
        for x in 0..w {
            let p = img.get_pixel(x, y);
            v.extend_from_slice(&[p[2], p[1], p[0]]);
        }
        while v.len() - start < row as usize {
            v.push(0);
        }
    }
    v
}
