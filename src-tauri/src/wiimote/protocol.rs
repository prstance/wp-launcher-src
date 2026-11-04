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

/// Steps that wake up an extension (unencrypted) and ask for its 6-byte id.
pub const EXTENSION_INIT: [(u32, u8); 2] = [(0xA4_00F0, 0x55), (0xA4_00FB, 0x00)];
pub const EXTENSION_ID: u32 = 0xA4_00FA;

/// A MotionPlus (built into "-TR" remotes, or plugged in) only answers at
/// MOTION_PLUS_ID once this byte is written (as Linux's hid-wiimote does).
pub const MOTION_PLUS_INIT: (u32, u8) = (0xA6_00F0, 0x55);
pub const MOTION_PLUS_ID: u32 = 0xA6_00FA;
/// Switches it on in standalone mode: it then takes the extension's place.
pub const MOTION_PLUS_START: (u32, u8) = (0xA6_00FE, 0x04);

/// Its id always ends in 0x05 (0x0005 inactive, 0x0405… once on).
pub fn is_motion_plus(id: &[u8]) -> bool {
    id.len() == 6 && id[5] == 0x05
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

/// Where the remote points, from the two sensor-bar dots: offset of their middle
/// from the camera centre, in camera pixels, with the remote's roll taken out.
/// Remembers the last pair so it keeps working while only one dot is in view.
#[derive(Default)]
pub struct Aim {
    last: Option<((f32, f32), (f32, f32))>, // (middle, half the gap between the dots)
}

impl Aim {
    pub fn update(&mut self, dots: &[(f32, f32)]) -> Option<(f32, f32)> {
        let (mid, half) = match *dots {
            [a, b, ..] => {
                let (mut dx, mut dy) = ((b.0 - a.0) / 2.0, (b.1 - a.1) / 2.0);
                if dx < 0.0 {
                    (dx, dy) = (-dx, -dy);
                }
                (((a.0 + b.0) / 2.0, (a.1 + b.1) / 2.0), (dx, dy))
            }
            [d] => {
                let (last_mid, half) = self.last?;
                let dist = |m: (f32, f32)| (m.0 - last_mid.0).powi(2) + (m.1 - last_mid.1).powi(2);
                let (left, right) = ((d.0 + half.0, d.1 + half.1), (d.0 - half.0, d.1 - half.1));
                (if dist(left) < dist(right) { left } else { right }, half)
            }
            [] => return None,
        };
        self.last = Some((mid, half));
        let (sin, cos) = half.1.atan2(half.0).sin_cos();
        let (ox, oy) = (mid.0 - 512.0, mid.1 - 384.0);
        Some((ox * cos + oy * sin, oy * cos - ox * sin))
    }
}

/// One MotionPlus sample (from the extension bytes once it is active).
pub struct MotionPlus {
    yaw: f32,
    pitch: f32,
    yaw_slow: bool,
    pitch_slow: bool,
    /// Something (Nunchuk…) is plugged into the MotionPlus.
    pub extension: bool,
}

pub fn motion_plus(ext: &[u8; 6]) -> Option<MotionPlus> {
    if ext[5] & 0x02 == 0 {
        return None;
    }
    Some(MotionPlus {
        yaw: (ext[0] as u16 | (ext[3] as u16 >> 2) << 8) as f32,
        pitch: (ext[2] as u16 | (ext[5] as u16 >> 2) << 8) as f32,
        yaw_slow: ext[3] & 0x02 != 0,
        pitch_slow: ext[3] & 0x01 != 0,
        extension: ext[4] & 0x01 != 0,
    })
}

/// Turns MotionPlus rates into degrees turned. Each gyroscope rests at a slightly
/// different value, so it learns that from moments when the remote lies still.
#[derive(Default)]
pub struct Gyro {
    bias: Option<(f32, f32)>,
    still: Vec<(f32, f32)>,
}

impl Gyro {
    // Raw units per degree/second (Dolphin's MotionPlus calibration).
    const SLOW: f32 = 16.118;
    const FAST: f32 = 3.6266;
    const STILL_SAMPLES: usize = 32;
    const STILL_SPREAD: f32 = 6.0;

    pub fn ready(&self) -> bool {
        self.bias.is_some()
    }

    /// Degrees turned (yaw, pitch) over `dt` seconds; None until calibrated.
    pub fn update(&mut self, m: &MotionPlus, dt: f32) -> Option<(f32, f32)> {
        if m.yaw_slow && m.pitch_slow {
            self.still.push((m.yaw, m.pitch));
            if self.still.len() == Self::STILL_SAMPLES {
                let n = Self::STILL_SAMPLES as f32;
                let mean = self.still.iter().fold((0.0, 0.0), |a, s| (a.0 + s.0 / n, a.1 + s.1 / n));
                let spread = self.still.iter().fold(0f32, |a, s| a.max((s.0 - mean.0).abs()).max((s.1 - mean.1).abs()));
                if spread < Self::STILL_SPREAD {
                    self.bias = Some(match self.bias {
                        Some(b) => (b.0 * 0.8 + mean.0 * 0.2, b.1 * 0.8 + mean.1 * 0.2),
                        None => mean,
                    });
                }
                self.still.clear();
            }
        } else {
            self.still.clear();
        }
        let (by, bp) = self.bias?;
        let rate = |raw: f32, bias: f32, slow: bool| (raw - bias) / if slow { Self::SLOW } else { Self::FAST };
        Some((rate(m.yaw, by, m.yaw_slow) * dt, rate(m.pitch, bp, m.pitch_slow) * dt))
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
    fn parses_ir_and_aims() {
        // Dots at (412, 384) and (612, 384), the second pair empty, then 6 extension bytes.
        let mut r = vec![0xA1, 0x37, 0x00, 0x00, 0x80, 0x80, 0x80];
        r.extend([0x9C, 0x80, 0x56, 0x64, 0x80, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF]);
        r.extend([0; 6]);
        let Input::Data { dots, .. } = parse(&r) else { panic!("not a data report") };
        assert_eq!(dots, [(412.0, 384.0), (612.0, 384.0)]);

        let mut aim = Aim::default();
        assert_eq!(aim.update(&dots), Some((0.0, 0.0)));
        // Only the right dot left in view: the middle is guessed from the last gap.
        assert_eq!(aim.update(&[(662.0, 384.0)]), Some((50.0, 0.0)));
        // Same aim (50, 0) with the remote rolled 30°: the whole picture turns with it.
        let mut aim = Aim::default();
        let (x, y) = aim.update(&[(468.69873, 359.0), (641.90381, 459.0)]).unwrap();
        assert!((x - 50.0).abs() < 1e-2 && y.abs() < 1e-2, "{x} {y}");
    }

    #[test]
    fn calibrates_gyro_while_still() {
        // yaw 8100 / pitch 8200, both in slow mode, MotionPlus data bit set.
        let sample = |yaw: u16, pitch: u16| {
            [yaw as u8, 0, pitch as u8, ((yaw >> 8) as u8) << 2 | 0x03, 0, ((pitch >> 8) as u8) << 2 | 0x02]
        };
        let mut gyro = Gyro::default();
        for _ in 0..31 {
            assert!(gyro.update(&motion_plus(&sample(8100, 8200)).unwrap(), 0.01).is_none());
        }
        // The 32nd still sample sets the resting point.
        assert_eq!(gyro.update(&motion_plus(&sample(8100, 8200)).unwrap(), 0.01), Some((0.0, 0.0)));
        // Turning at 10°/s for 0.1 s.
        let (yaw, pitch) = gyro.update(&motion_plus(&sample(8100 + 161, 8200)).unwrap(), 0.1).unwrap();
        assert!((yaw - 1.0).abs() < 0.01 && pitch.abs() < 1e-6, "{yaw} {pitch}");
        assert!(motion_plus(&[0; 6]).is_none());
        assert!(is_motion_plus(&[0x00, 0x00, 0xA6, 0x20, 0x00, 0x05]));
    }
}
