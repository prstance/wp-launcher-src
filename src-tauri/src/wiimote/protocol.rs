//! Wii Remote HID reports, as sent over the L2CAP interrupt channel.
//! Reference: https://wiibrew.org/wiki/Wiimote

use crate::i18n::tr;

/// Output reports start with this HID transaction header (DATA | Output)…
pub const OUT: u8 = 0xA2;
/// …input reports with this one (DATA | Input).
pub const IN: u8 = 0xA1;

/// Bluetooth names a Wii Remote advertises (the "-TR" one has MotionPlus built in).
pub fn is_wiimote_name(name: &str) -> bool {
    name == "Nintendo RVL-CNT-01" || name == "Nintendo RVL-CNT-01-TR"
}

pub fn model(name: &str) -> &'static str {
    if name.ends_with("-TR") {
        "Wiimote Plus"
    } else {
        "Wiimote"
    }
}

/// Player LEDs 1–4 lit for slot 0–3.
pub fn leds(slot: usize) -> Vec<u8> {
    vec![OUT, 0x11, 0x10 << slot]
}

pub fn rumble(on: bool) -> Vec<u8> {
    vec![OUT, 0x10, on as u8]
}

pub fn status_request() -> Vec<u8> {
    vec![OUT, 0x15, 0x00]
}

/// Reports with core buttons, accelerometer, 10 bytes of IR and 6 of extension
/// (0x37, where MotionPlus data lands), sent on change.
pub fn report_mode() -> Vec<u8> {
    vec![OUT, 0x12, 0x00, 0x37]
}

/// Writes up to 16 bytes to the remote's register space (IR camera at 0xB0xxxx,
/// extension controller at 0xA4xxxx).
pub fn write_register(addr: u32, data: &[u8]) -> Vec<u8> {
    let mut r = vec![OUT, 0x16, 0x04, (addr >> 16) as u8, (addr >> 8) as u8, addr as u8, data.len() as u8];
    r.extend_from_slice(data);
    r.resize(2 + 21, 0);
    r
}

/// Turns the IR camera on in basic mode (10 bytes for 4 points), at the Wii's
/// sensitivity level 3. Each report needs a short pause before the next.
pub fn ir_setup() -> [Vec<u8>; 7] {
    [
        vec![OUT, 0x13, 0x04],
        vec![OUT, 0x1A, 0x04],
        write_register(0xB0_0030, &[0x08]),
        write_register(0xB0_0000, &[0x02, 0x00, 0x00, 0x71, 0x01, 0x00, 0xAA, 0x00, 0x64]),
        write_register(0xB0_001A, &[0x63, 0x03]),
        write_register(0xB0_0033, &[0x01]),
        write_register(0xB0_0030, &[0x08]),
    ]
}

pub fn read_register(addr: u32, size: u16) -> Vec<u8> {
    vec![OUT, 0x17, 0x04, (addr >> 16) as u8, (addr >> 8) as u8, addr as u8, (size >> 8) as u8, size as u8]
}

/// Reads the remote's own memory (EEPROM), not its registers.
pub fn read_memory(addr: u16, size: u16) -> Vec<u8> {
    vec![OUT, 0x17, 0x00, 0x00, (addr >> 8) as u8, addr as u8, (size >> 8) as u8, size as u8]
}

/// The accelerometer's calibration in the remote's memory (zero, 1 g, checksum).
pub const ACCEL_CAL: u16 = 0x0016;

/// Steps that wake up an extension (unencrypted) and ask for its 6-byte id.
pub const EXTENSION_INIT: [(u32, u8); 2] = [(0xA4_00F0, 0x55), (0xA4_00FB, 0x00)];
pub const EXTENSION_ID: u32 = 0xA4_00FA;

/// A MotionPlus (built into "-TR" remotes, or plugged in) only answers at
/// MOTION_PLUS_ID once this byte is written (as Linux's hid-wiimote does).
pub const MOTION_PLUS_INIT: (u32, u8) = (0xA6_00F0, 0x55);
pub const MOTION_PLUS_ID: u32 = 0xA6_00FA;
/// Switches it on in standalone mode: it then takes the extension's place.
pub const MOTION_PLUS_START: (u32, u8) = (0xA6_00FE, 0x04);

/// Once on, its calibration (fast block then slow block) reads at the extension's address.
pub const MOTION_PLUS_CAL: u32 = 0xA4_0020;

/// Its id always ends in 0x05 (0x0005 inactive, 0x0405… once on).
pub fn is_motion_plus(id: &[u8]) -> bool {
    id.len() == 6 && id[5] == 0x05
}

/// The id at the extension's address once it is on in standalone mode.
pub fn is_active_motion_plus(id: &[u8]) -> bool {
    id.len() == 6 && id[2..] == [0xA4, 0x20, 0x04, 0x05]
}

/// Bytes after the report id in each input report, as Dolphin's IOWin trims what
/// Windows hands over (always a whole 22-byte report).
#[cfg_attr(not(windows), allow(dead_code))]
pub fn input_size(id: u8) -> Option<usize> {
    Some(match id {
        0x20 => 6,
        0x21 => 21,
        0x22 => 4,
        0x30 => 2,
        0x31 => 5,
        0x32 => 10,
        0x33 => 17,
        0x34..=0x37 | 0x3D..=0x3F => 21,
        _ => return None,
    })
}

/// An input report as Windows' HID driver hands it over (report id first, padded to the
/// largest report) shaped as it comes over L2CAP on Linux, which is what the launcher and
/// Dolphin's bridge expect: the transaction header Windows leaves out, then only the
/// report's own bytes.
#[cfg_attr(not(windows), allow(dead_code))]
pub fn from_hid(buf: &[u8]) -> Vec<u8> {
    let n = buf.first().and_then(|&id| input_size(id)).map_or(buf.len(), |size| (size + 1).min(buf.len()));
    [IN].into_iter().chain(buf[..n].iter().copied()).collect()
}

/// An output report (L2CAP shape, header first) as Windows' HID driver takes it: it adds
/// the transaction header itself.
#[cfg_attr(not(windows), allow(dead_code))]
pub fn to_hid(report: &[u8]) -> &[u8] {
    report.strip_prefix(&[OUT]).unwrap_or(report)
}

pub enum Input {
    Status { buttons: u16, extension: bool, battery: u8 },
    /// `data` is None when the read failed (nothing at that address).
    Read { buttons: u16, addr: u16, data: Option<Vec<u8>> },
    /// Points the IR camera sees, camera pixels (x 0–1023, y 0–767), and the extension's bytes.
    Data { buttons: u16, dots: Vec<(f32, f32)>, ext: [u8; 6] },
    Buttons(u16),
    Other,
}

/// The 4 points in 10 bytes of basic IR. Two per 5 bytes: x1 y1 [y1 x1 y2 x2 high bits] x2 y2.
pub fn basic_dots(ir: &[u8]) -> [(u16, u16); 4] {
    let pair = |p: &[u8]| {
        let hi = p[2] as u16;
        [(p[0] as u16 | (hi >> 4 & 3) << 8, p[1] as u16 | (hi >> 6 & 3) << 8), (p[3] as u16 | (hi & 3) << 8, p[4] as u16 | (hi >> 2 & 3) << 8)]
    };
    let ([a, b], [c, d]) = (pair(&ir[..5]), pair(&ir[5..10]));
    [a, b, c, d]
}

/// Empty slots read 1023.
pub fn dot_visible(&(x, y): &(u16, u16)) -> bool {
    x < 1023 && y < 768
}

pub fn parse(report: &[u8]) -> Input {
    if report.len() < 4 || report[0] != IN {
        return Input::Other;
    }
    let buttons = u16::from_be_bytes([report[2], report[3]]) & 0x1F9F;
    match report[1] {
        0x20 if report.len() >= 8 => Input::Status { buttons, extension: report[4] & 0x02 != 0, battery: report[7] },
        0x21 if report.len() >= 23 => {
            let size = (report[4] >> 4) as usize + 1;
            let data = (report[4] & 0x0F == 0).then(|| report[7..7 + size].to_vec());
            Input::Read { buttons, addr: u16::from_be_bytes([report[5], report[6]]), data }
        }
        0x37 if report.len() >= 23 => {
            let dots = basic_dots(&report[7..17]).iter().filter(|d| dot_visible(d)).map(|&(x, y)| (x as f32, y as f32)).collect();
            Input::Data { buttons, dots, ext: report[17..23].try_into().unwrap() }
        }
        0x30..=0x3F => Input::Buttons(buttons),
        _ => Input::Other,
    }
}

/// One MotionPlus sample: raw 14-bit rates, [yaw, roll, pitch], and their slow flags.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct MotionPlus {
    pub raw: [u16; 3],
    pub slow: [bool; 3],
    /// Something (Nunchuk…) is plugged into the MotionPlus.
    pub extension: bool,
}

/// Axes in samples and rates (roll is 1).
pub const YAW: usize = 0;
pub const PITCH: usize = 2;

pub fn motion_plus(ext: &[u8; 6]) -> Option<MotionPlus> {
    // Bits 1–0 of byte 5 read 1 0 in MotionPlus data; an empty port (0xFF) doesn't.
    if ext[5] & 0x03 != 0x02 {
        return None;
    }
    let value = |lo: u8, hi: u8| lo as u16 | ((hi >> 2) as u16) << 8;
    Some(MotionPlus {
        raw: [value(ext[0], ext[3]), value(ext[1], ext[4]), value(ext[2], ext[5])],
        slow: [ext[3] & 0x02 != 0, ext[4] & 0x02 != 0, ext[3] & 0x01 != 0],
        extension: ext[4] & 0x01 != 0,
    })
}

/// The MotionPlus's own calibration (32 bytes at 0x20 of its registers): per mode
/// (0 fast, 1 slow) and axis, the raw rest value and raw units per degree/second.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct GyroCal {
    pub zero: [[f32; 3]; 2],
    pub units: [[f32; 3]; 2],
}

impl Default for GyroCal {
    /// Dolphin's emulated MotionPlus: 0x1100 raw for 1200°/s fast, 270°/s slow.
    fn default() -> Self {
        GyroCal { zero: [[8192.0; 3]; 2], units: [[4352.0 / 1200.0; 3], [4352.0 / 270.0; 3]] }
    }
}

impl GyroCal {
    /// Blocks of 16 bytes, fast then slow: zeros yaw roll pitch, scales yaw roll pitch
    /// (big-endian u16, 16-bit = data << 2), degrees / 6, uid, 2 bytes of checksum.
    pub fn parse(b: &[u8]) -> Option<GyroCal> {
        if b.len() < 32 {
            return None;
        }
        let mut cal = GyroCal::default();
        for mode in 0..2 {
            let block = &b[mode * 16..];
            let word = |i: usize| u16::from_be_bytes([block[2 * i], block[2 * i + 1]]) as f32;
            let degrees = block[12] as f32 * 6.0;
            for axis in 0..3 {
                let (zero, scale) = (word(axis), word(3 + axis));
                cal.zero[mode][axis] = zero / 4.0;
                cal.units[mode][axis] = (scale - zero).abs() / 4.0 / degrees;
            }
        }
        let plausible = (0..3).all(|a| {
            (6000.0..=10400.0).contains(&cal.zero[0][a])
                && (6000.0..=10400.0).contains(&cal.zero[1][a])
                && (2.5..=6.0).contains(&cal.units[0][a])
                && (10.0..=22.0).contains(&cal.units[1][a])
        });
        plausible.then_some(cal)
    }
}

/// Turns MotionPlus samples into degrees/second, learning each gyroscope's rest value
/// (it drifts with temperature) from moments the remote is still: on a table, or
/// roughly still in a hand.
#[derive(Clone, Debug, Default)]
pub struct Gyro {
    pub cal: GyroCal,
    /// Slow-mode rest values, raw.
    bias: Option<[f32; 3]>,
    /// Learned on a table once at least: in-hand windows only fine-tune it then.
    confident: bool,
    window: Vec<[f32; 3]>,
    /// Every sample of the window had the bar saying the remote wasn't turning.
    window_steady: bool,
    /// In-hand windows in a row that agree (sum of their means, how many): a slow turn
    /// looks still for a moment, only seconds of it make a rest value.
    still: Option<([f32; 3], u32)>,
}

impl Gyro {
    /// ~0.3 s of reports.
    const WINDOW: usize = 32;
    /// Largest swing from the window's mean, raw slow units (16 ≈ 1°/s).
    const TABLE_SPREAD: f32 = 8.0;
    const HAND_SPREAD: f32 = 40.0;
    /// How far an in-hand window may sit from the rest value to move it: a slow,
    /// steady turn mustn't become the rest value.
    const HAND_PULL: f32 = 24.0;
    const ROUGH_PULL: f32 = 160.0;
    /// In-hand windows that must agree, within this, before they move the rest value:
    /// Dolphin's 3 s calibration period within its dead zone (IMUGyroscope.cpp).
    const HAND_STILL: u32 = 10;
    const HAND_AGREE: f32 = 16.0;

    #[cfg_attr(not(unix), allow(dead_code))]
    pub fn ready(&self) -> bool {
        self.bias.is_some()
    }

    /// The rest values (raw, slow) and whether a table set them: for the trace.
    #[cfg(not(test))]
    pub fn rest(&self) -> (Option<[f32; 3]>, bool) {
        (self.bias, self.confident)
    }

    /// Degrees/second [yaw, roll, pitch] (None until a rest value is known).
    /// `steady`: something else (the sensor bar) says the remote isn't turning.
    pub fn update(&mut self, m: &MotionPlus, steady: bool) -> Option<[f32; 3]> {
        self.learn(m, steady);
        let bias = self.bias?;
        let c = &self.cal;
        Some(std::array::from_fn(|a| {
            let raw = m.raw[a] as f32;
            if m.slow[a] {
                (raw - bias[a]) / c.units[1][a]
            } else {
                // Same offset from the reference voltage, before the slow output's
                // amplification: smaller in fast units.
                let fast_bias = c.zero[0][a] + (bias[a] - c.zero[1][a]) * c.units[0][a] / c.units[1][a];
                (raw - fast_bias) / c.units[0][a]
            }
        }))
    }

    fn learn(&mut self, m: &MotionPlus, steady: bool) {
        if !m.slow.iter().all(|&s| s) {
            self.window.clear();
            self.still = None;
            return;
        }
        if self.window.is_empty() {
            self.window_steady = true;
        }
        self.window_steady &= steady;
        self.window.push(m.raw.map(|r| r as f32));
        if self.window.len() < Self::WINDOW {
            return;
        }
        let n = self.window.len() as f32;
        let mean: [f32; 3] = std::array::from_fn(|a| self.window.iter().map(|s| s[a]).sum::<f32>() / n);
        let spread = self.window.iter().flat_map(|s| (0..3).map(move |a| (s[a] - mean[a]).abs())).fold(0f32, f32::max);
        self.window.clear();
        let off = |b: &[f32; 3]| (0..3).map(|a| (mean[a] - b[a]).abs()).fold(0f32, f32::max);
        if spread > Self::TABLE_SPREAD && spread <= Self::HAND_SPREAD && !self.window_steady {
            // In a hand, nothing else saying it's still: count it towards a still spell.
            let (sum, n) = match self.still {
                Some((sum, n)) if off(&sum.map(|v| v / n as f32)) <= Self::HAND_AGREE => (sum, n),
                _ => ([0.0; 3], 0),
            };
            self.still = Some((std::array::from_fn(|a| sum[a] + mean[a]), n + 1));
        } else {
            self.still = None;
        }
        match self.bias {
            _ if spread <= Self::TABLE_SPREAD => {
                self.bias = Some(match (self.bias, self.confident) {
                    (Some(b), true) => std::array::from_fn(|a| b[a] + (mean[a] - b[a]) * 0.3),
                    _ => mean,
                });
                self.confident = true;
            }
            _ if spread > Self::HAND_SPREAD => {}
            // Nothing better yet (as Dolphin, which starts from the first sample).
            None => self.bias = Some(mean),
            // The bar said the remote wasn't turning: as good as a table, roughly.
            Some(b) if self.window_steady => {
                if off(&b) <= Self::ROUGH_PULL {
                    self.bias = Some(std::array::from_fn(|a| b[a] + (mean[a] - b[a]) * 0.2));
                }
            }
            Some(b) => {
                let Some((sum, n)) = self.still.filter(|&(_, n)| n >= Self::HAND_STILL) else { return };
                self.still = None;
                let spell = sum.map(|v| v / n as f32);
                let (pull, weight) = if self.confident { (Self::HAND_PULL, 0.25) } else { (Self::ROUGH_PULL, 0.7) };
                if off(&b) <= pull {
                    self.bias = Some(std::array::from_fn(|a| b[a] + (spell[a] - b[a]) * weight));
                }
            }
        }
    }
}

/// Battery byte to percent (0xC8 is a fresh set of batteries).
pub fn battery_percent(raw: u8) -> u8 {
    (raw as u32 * 100 / 0xC8).min(100) as u8
}

/// Names of the pressed core buttons, in the order they sit on the remote.
pub fn button_names(buttons: u16) -> Vec<&'static str> {
    const BITS: [(u16, &str); 11] = [
        (0x0800, "↑"), (0x0400, "↓"), (0x0100, "←"), (0x0200, "→"),
        (0x0008, "A"), (0x0004, "B"), (0x1000, "+"), (0x0080, "HOME"),
        (0x0010, "−"), (0x0002, "1"), (0x0001, "2"),
    ];
    BITS.iter().filter(|(bit, _)| buttons & bit != 0).map(|(_, name)| *name).collect()
}

pub fn extension_name(id: &[u8]) -> &'static str {
    match id {
        [0x00, 0x00, 0xA4, 0x20, 0x00, 0x00] => "Nunchuk",
        [0x00, 0x00, 0xA4, 0x20, 0x01, 0x01] => "Manette Classique",
        [0x01, 0x00, 0xA4, 0x20, 0x01, 0x01] => "Manette Classique Pro",
        [0x00, 0x00, 0xA4, 0x20, 0x01, 0x03] => "Guitare",
        [0x01, 0x00, 0xA4, 0x20, 0x01, 0x03] => "Batterie",
        _ => "Extension",
    }
}

/// An extension_name() in the launcher's language, when the snapshot is drawn.
pub fn extension_label(name: &str) -> String {
    match name {
        "Manette Classique" => tr!("Manette Classique"),
        "Manette Classique Pro" => tr!("Manette Classique Pro"),
        "Guitare" => tr!("Guitare"),
        "Batterie" => tr!("Batterie"),
        "Extension" => tr!("Extension"),
        other => other.to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_status_and_buttons() {
        // Status: A held, extension plugged, battery 0x64.
        match parse(&[0xA1, 0x20, 0x00, 0x08, 0x02, 0x00, 0x00, 0x64]) {
            Input::Status { buttons, extension, battery } => {
                assert_eq!(button_names(buttons), ["A"]);
                assert!(extension);
                assert_eq!(battery_percent(battery), 50);
            }
            _ => panic!("not a status report"),
        }
        match parse(&[0xA1, 0x30, 0x08, 0x80]) {
            Input::Buttons(b) => assert_eq!(button_names(b), ["↑", "HOME"]),
            _ => panic!("not a buttons report"),
        }
    }

    #[test]
    fn parses_extension_id() {
        let mut r = vec![0xA1, 0x21, 0x00, 0x00, 0x50, 0x00, 0xFA, 0x00, 0x00, 0xA4, 0x20, 0x00, 0x00];
        r.resize(23, 0);
        match parse(&r) {
            Input::Read { addr, data, .. } => {
                assert_eq!(addr, 0x00FA);
                assert_eq!(extension_name(&data.unwrap()), "Nunchuk");
            }
            _ => panic!("not a read report"),
        }
    }

    #[test]
    fn builds_reports() {
        assert_eq!(leds(2), [0xA2, 0x11, 0x40]);
        assert_eq!(write_register(0xA4_00F0, &[0x55])[..8], [0xA2, 0x16, 0x04, 0xA4, 0x00, 0xF0, 0x01, 0x55]);
        assert_eq!(write_register(0xB0_001A, &[0x63, 0x03])[6..9], [0x02, 0x63, 0x03]);
        assert!(ir_setup().iter().skip(2).all(|r| r.len() == 23));
    }

    #[test]
    fn parses_ir() {
        // Dots at (412, 384) and (612, 384), the second pair empty, then 6 extension bytes.
        let mut r = vec![0xA1, 0x37, 0x00, 0x00, 0x80, 0x80, 0x80];
        r.extend([0x9C, 0x80, 0x56, 0x64, 0x80, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF]);
        r.extend([0; 6]);
        let Input::Data { dots, .. } = parse(&r) else { panic!("not a data report") };
        assert_eq!(dots, [(412.0, 384.0), (612.0, 384.0)]);
    }

    #[test]
    fn an_empty_port_is_no_motion_plus_data() {
        // What a remote sends right after the MotionPlus is switched on, before it wakes up.
        assert!(motion_plus(&[0xFF; 6]).is_none());
        assert!(is_motion_plus(&[0x00, 0x00, 0xA6, 0x20, 0x00, 0x05]));
        assert!(is_active_motion_plus(&[0x00, 0x00, 0xA4, 0x20, 0x04, 0x05]));
        assert!(!is_active_motion_plus(&[0x00, 0x00, 0xA6, 0x20, 0x00, 0x05]));
    }

    #[test]
    fn input_sizes_trim_what_windows_hands_over() {
        assert_eq!(input_size(0x20), Some(6));
        assert_eq!(input_size(0x37), Some(21));
        assert_eq!(input_size(0x3F), Some(21));
        assert_eq!(input_size(0x38), None);
        // A status report, header included, is what parse() expects.
        let windows = [0x20, 0x00, 0x08, 0x02, 0x00, 0x00, 0x64, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0];
        let report = from_hid(&windows);
        assert_eq!(report.len(), 8);
        assert!(matches!(parse(&report), Input::Status { extension: true, battery: 0x64, .. }));
    }

    /// What reaches the proxy and Dolphin's bridge from a remote, and what goes to it,
    /// has the same shape on Windows (HID device) as on Linux (L2CAP).
    #[test]
    fn windows_reports_have_the_l2cap_shape() {
        use super::super::bridge::Proxy;
        // Over L2CAP: header, id, the report's own bytes.
        let l2cap: [Vec<u8>; 3] = [
            vec![0xA1, 0x30, 0x00, 0x08],
            vec![0xA1, 0x20, 0x00, 0x08, 0x02, 0x00, 0x00, 0x64],
            [0xA1, 0x37].into_iter().chain(1..=21).collect(),
        ];
        for report in &l2cap {
            // From Windows: no header, padded to 22 bytes.
            let mut hid = report[1..].to_vec();
            hid.resize(22, 0);
            assert_eq!(&from_hid(&hid), report);
            let mut proxy = Proxy::new(false, Default::default());
            assert_eq!(proxy.from_remote(&from_hid(&hid), std::time::Instant::now()).to_game, std::slice::from_ref(report));
        }
        // An id the trimming doesn't know passes whole.
        assert_eq!(from_hid(&[0x3B, 1, 2]), [0xA1, 0x3B, 1, 2]);
        assert_eq!(from_hid(&[]), [0xA1]);
        assert_eq!(to_hid(&leds(0)), [0x11, 0x10]);
        assert_eq!(to_hid(&[0x11, 0x10]), [0x11, 0x10]);
    }

    #[test]
    fn builds_memory_reads() {
        assert_eq!(read_memory(ACCEL_CAL, 10), [0xA2, 0x17, 0x00, 0x00, 0x00, 0x16, 0x00, 0x0A]);
    }

    /// Raw [yaw, roll, pitch] and their slow flags, nothing plugged in.
    fn ext(raw: [u16; 3], slow: [bool; 3]) -> [u8; 6] {
        let hi = |v: u16| ((v >> 8) as u8) << 2;
        [
            raw[0] as u8,
            raw[1] as u8,
            raw[2] as u8,
            hi(raw[0]) | (slow[0] as u8) << 1 | slow[2] as u8,
            hi(raw[1]) | (slow[1] as u8) << 1,
            hi(raw[2]) | 0x02,
        ]
    }

    #[test]
    fn decodes_three_axes_and_their_modes() {
        // The bridge tests' "turned" fixture: yaw 0x23FF and pitch 0x2000 slow, but roll
        // 0x0020 *fast* (byte 4 = 0x00), a value a still remote never sends.
        let m = motion_plus(&[0xFF, 0x20, 0x00, 0x8F, 0x00, 0x82]).unwrap();
        assert_eq!(m.raw, [0x23FF, 0x0020, 0x2000]);
        assert_eq!(m.slow, [true, false, true]);
        assert!(!m.extension);
        // A real sample from today's log (RVL-CNT-01-TR, slot 1).
        let m = motion_plus(&[0xC8, 0x84, 0x1F, 0xA7, 0xA2, 0x86]).unwrap();
        assert_eq!(m.raw, [0x29C8, 0x2884, 0x211F]);
        assert_eq!(m.slow, [true, true, true]);
        assert!(motion_plus(&[0xFF; 6]).is_none());
        assert!(motion_plus(&[0x00; 6]).is_none());
        assert_eq!(motion_plus(&ext([8000, 8100, 8200], [true, false, true])).unwrap().raw, [8000, 8100, 8200]);
    }

    #[test]
    fn reads_the_remotes_own_calibration() {
        // Dolphin's emulated block: zeros 0x8000, scales 0x8000 ± 0x4400, 1200°/s and 270°/s.
        let block = |deg: u8| {
            let mut b = vec![0x80, 0, 0x80, 0, 0x80, 0, 0x3C, 0, 0xC4, 0, 0x3C, 0, deg, 0x0B, 0, 0];
            b.truncate(16);
            b
        };
        let bytes = [block(200), block(45)].concat();
        let cal = GyroCal::parse(&bytes).unwrap();
        let d = GyroCal::default();
        for m in 0..2 {
            for a in 0..3 {
                assert!((cal.units[m][a] - d.units[m][a]).abs() < 1e-3);
                assert_eq!(cal.zero[m][a], 8192.0);
            }
        }
        assert!(GyroCal::parse(&[0xFF; 32]).is_none());
        assert!(GyroCal::parse(&[0x00; 32]).is_none());
    }

    #[test]
    fn learns_rest_on_a_table_then_measures_turns() {
        let mut g = Gyro::default();
        for i in 0..31 {
            let wobble = (i % 3) as u16; // ±1–2 counts of noise
            assert!(g.update(&motion_plus(&ext([8063 + wobble, 8190, 8210], [true; 3])).unwrap(), false).is_none());
        }
        assert!(g.update(&motion_plus(&ext([8064, 8190, 8210], [true; 3])).unwrap(), false).is_some());
        // 10°/s to the right, slow mode.
        let r = g.update(&motion_plus(&ext([8064 + 161, 8190, 8210], [true; 3])).unwrap(), false).unwrap();
        assert!((r[YAW] - 10.0).abs() < 0.1 && r[PITCH].abs() < 0.1, "{r:?}");
        // 500°/s in fast mode: the rest offset shrinks with the gain (8064 → 8163).
        let fast_rest: f32 = 8192.0 + (8064.0 - 8192.0) * (4352.0 / 1200.0) / (4352.0 / 270.0);
        let raw = (fast_rest + 500.0 * 4352.0 / 1200.0).round() as u16;
        let r = g.update(&motion_plus(&ext([raw, 8190, 8210], [false, true, true])).unwrap(), false).unwrap();
        assert!((r[YAW] - 500.0).abs() < 0.5, "{r:?}");
    }

    #[test]
    fn learns_rest_in_a_hand_and_ignores_slow_turns_once_sure() {
        let mut g = Gyro::default();
        // Held: ±20 counts of tremor around 8100.
        let held = |i: usize| 8100 + [0i32, 20, -20, 10, -10][i % 5];
        for i in 0..64 {
            g.update(&motion_plus(&ext([held(i) as u16, 8192, 8192], [true; 3])).unwrap(), false);
        }
        let r = g.update(&motion_plus(&ext([8100, 8192, 8192], [true; 3])).unwrap(), false).unwrap();
        assert!(r[YAW].abs() < 0.3, "{r:?}");
        // On a table: sure of it now.
        for _ in 0..32 {
            g.update(&motion_plus(&ext([8100, 8192, 8192], [true; 3])).unwrap(), false);
        }
        // A slow steady turn (3°/s, +48 counts) held for seconds stays a turn.
        for i in 0..320 {
            g.update(&motion_plus(&ext([(held(i) + 48) as u16, 8192, 8192], [true; 3])).unwrap(), false);
        }
        let r = g.update(&motion_plus(&ext([8148, 8192, 8192], [true; 3])).unwrap(), false).unwrap();
        assert!((r[YAW] - 3.0).abs() < 0.2, "{r:?}");
    }

    #[test]
    fn a_slow_turn_in_a_hand_never_set_down_stays_a_turn() {
        let mut g = Gyro::default();
        let held = |i: usize| 8100 + [0i32, 20, -20, 10, -10][i % 5];
        for i in 0..64 {
            g.update(&motion_plus(&ext([held(i) as u16, 8192, 8192], [true; 3])).unwrap(), false);
        }
        // A slow steady turn (3°/s) for a second, as when easing onto a button.
        for i in 0..100 {
            g.update(&motion_plus(&ext([(held(i) + 48) as u16, 8192, 8192], [true; 3])).unwrap(), false);
        }
        // Still again: still, no drift back.
        let r = g.update(&motion_plus(&ext([8100, 8192, 8192], [true; 3])).unwrap(), false).unwrap();
        assert!(r[YAW].abs() < 0.3, "{r:?}");
    }

    #[test]
    fn a_hand_held_still_for_seconds_corrects_the_rest_value() {
        let mut g = Gyro::default();
        let held = |i: usize| 8100 + [0i32, 20, -20, 10, -10][i % 5];
        for i in 0..32 {
            g.update(&motion_plus(&ext([held(i) as u16, 8192, 8192], [true; 3])).unwrap(), false);
        }
        // The rest value warms up by 2°/s: once held still that way for a few seconds,
        // it is the new rest value.
        for i in 0..800 {
            g.update(&motion_plus(&ext([(held(i) + 32) as u16, 8192, 8192], [true; 3])).unwrap(), false);
        }
        let r = g.update(&motion_plus(&ext([8132, 8192, 8192], [true; 3])).unwrap(), false).unwrap();
        assert!(r[YAW].abs() < 0.5, "{r:?}");
    }
}
