//! A Wii Remote with a MotionPlus, made in software for a phone (phone.rs): it answers the
//! reports the launcher and the game send as a real one does (WiiBrew, and Dolphin's own
//! emulated remote), so the rest of the launcher can't tell it from one: slots, LEDs,
//! rumble, the gyroscope's pointer, the bridge to Dolphin, and the speaker, whose sound
//! goes to the phone. Its camera sees no sensor bar: the MotionPlus points.

use super::protocol::{self, IN};

/// The accelerometer's rest value and 1 g, in its 10-bit units (Dolphin's emulated remote).
const ACCEL_ZERO: f32 = 512.0;
const ONE_G: f32 = 104.0;
/// The remote's memory, with the accelerometer's calibration at 0x16 and its copy at 0x20:
/// zero x y z, low bits, 1 g x y z, low bits, volume, checksum.
const EEPROM_SIZE: usize = 0x1700;
const ACCEL_CAL: [u8; 10] = [0x80, 0x80, 0x80, 0x00, 0x9A, 0x9A, 0x9A, 0x00, 0x40, 0xE3];
/// The MotionPlus's ids: waiting at 0xA6, and once on in place of an extension at 0xA4.
const MOTION_PLUS_ID: [u8; 6] = [0x00, 0x00, 0xA6, 0x20, 0x00, 0x05];
const MOTION_PLUS_ACTIVE_ID: [u8; 6] = [0x00, 0x00, 0xA4, 0x20, 0x04, 0x05];
/// Dolphin's emulated MotionPlus: rest at 0x8000, ±0x4400 (16-bit) for 1200°/s fast
/// (200 × 6) and 270°/s slow (45 × 6), yaw and pitch counting down (protocol::GyroCal).
const MOTION_PLUS_CAL: [[u8; 16]; 2] = [
    [0x80, 0, 0x80, 0, 0x80, 0, 0x3C, 0, 0xC4, 0, 0x3C, 0, 200, 0x0B, 0, 0],
    [0x80, 0, 0x80, 0, 0x80, 0, 0x3C, 0, 0xC4, 0, 0x3C, 0, 45, 0x0B, 0, 0],
];
const GYRO_ZERO: f32 = 8192.0;
/// Raw units per °/s, slow and fast.
const GYRO_SLOW: f32 = 4352.0 / 270.0;
const GYRO_FAST: f32 = 4352.0 / 1200.0;
/// Errors in read and write answers.
const NO_DEVICE: u8 = 7;
const BAD_ADDRESS: u8 = 8;
/// The speaker's two formats (register 0x02) and Dolphin's decoder figures (Speaker.cpp).
const SPEAKER_PCM: u8 = 0x40;
const ADPCM_DIFF: [i32; 16] = [1, 3, 5, 7, 9, 11, 13, 15, -1, -3, -5, -7, -9, -11, -13, -15];
const ADPCM_SCALE: [i32; 16] = [230, 230, 230, 230, 307, 409, 512, 614, 230, 230, 230, 230, 307, 409, 512, 614];

/// What is held and felt, in the remote's terms.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Sensors {
    /// Core buttons, as in reports (protocol::button_names).
    pub buttons: u16,
    /// In g, along the accelerometer's x (left), y (back) and z (up): lying face up, +1 z.
    pub accel: [f32; 3],
    /// In °/s, about those axes, right-hand rule.
    pub gyro: [f32; 3],
}

/// What the phone has to show or play.
#[derive(Debug, PartialEq)]
pub enum Event {
    Rumble(bool),
    /// Player LEDs, bit 0 the first.
    Leds(u8),
    /// Samples at `rate` Hz, `volume` 0–1.
    Sound { rate: u32, volume: f32, samples: Vec<i16> },
}

#[derive(Clone, Copy, Debug, PartialEq)]
enum MotionPlus {
    /// Answers at 0xA6 once woken by 0x55 at 0xA600F0, as a real one.
    Asleep,
    Waiting,
    /// On, in the extension's place.
    Active,
}

pub struct Emulated {
    pub sensors: Sensors,
    /// Status reports' battery byte (0xC8 full).
    pub battery: u8,
    eeprom: Vec<u8>,
    mode: u8,
    continuous: bool,
    rumble: bool,
    leds: u8,
    ir: bool,
    camera: [u8; 0x100],
    speaker: [u8; 0x100],
    speaker_on: bool,
    speaker_muted: bool,
    adpcm: (i32, i32),
    motion_plus: MotionPlus,
    /// Which half of an interleaved pair (0x3E/0x3F) goes next.
    second_half: bool,
    /// The last data report: one that changes nothing isn't sent again outside continuous mode.
    last: Vec<u8>,
}

impl Default for Emulated {
    fn default() -> Self {
        let mut eeprom = vec![0; EEPROM_SIZE];
        eeprom[0x16..0x20].copy_from_slice(&ACCEL_CAL);
        eeprom[0x20..0x2A].copy_from_slice(&ACCEL_CAL);
        Emulated {
            sensors: Sensors { accel: [0.0, 0.0, 1.0], ..Default::default() },
            battery: 0xC8,
            eeprom,
            mode: 0x30,
            continuous: false,
            rumble: false,
            leds: 0,
            ir: false,
            camera: [0; 0x100],
            speaker: [0; 0x100],
            speaker_on: false,
            speaker_muted: false,
            adpcm: (0, 127),
            motion_plus: MotionPlus::Asleep,
            second_half: false,
            last: Vec::new(),
        }
    }
}

impl Emulated {
    /// The two button bytes, with the accelerometer's low bits where the remote puts them.
    fn core(&self, accel: [u16; 3]) -> [u8; 2] {
        let b = self.sensors.buttons & 0x1F9F;
        [(b >> 8) as u8 | ((accel[0] & 3) as u8) << 5, b as u8 | ((accel[1] >> 1 & 1) as u8) << 5 | ((accel[2] >> 1 & 1) as u8) << 6]
    }

    fn accel(&self) -> [u16; 3] {
        self.sensors.accel.map(|g| (ACCEL_ZERO + g * ONE_G).round().clamp(0.0, 1023.0) as u16)
    }

    /// Six bytes of MotionPlus data: yaw, roll, pitch (about z, y, x), each slow while it fits.
    fn motion_plus_data(&self) -> [u8; 6] {
        let axis = |rate: f32| {
            let slow = (rate * GYRO_SLOW).abs() < 8000.0;
            let raw = (GYRO_ZERO + rate * if slow { GYRO_SLOW } else { GYRO_FAST }).round().clamp(0.0, 16383.0) as u16;
            (raw, slow)
        };
        let [x, y, z] = self.sensors.gyro;
        let ((yaw, yaw_slow), (roll, roll_slow), (pitch, pitch_slow)) = (axis(z), axis(y), axis(x));
        let hi = |v: u16| ((v >> 8) as u8) << 2;
        [
            yaw as u8,
            roll as u8,
            pitch as u8,
            hi(yaw) | (yaw_slow as u8) << 1 | pitch_slow as u8,
            hi(roll) | (roll_slow as u8) << 1,
            hi(pitch) | 0x02,
        ]
    }

    fn extension(&self, size: usize) -> Vec<u8> {
        let mut ext = vec![0; size];
        if self.motion_plus == MotionPlus::Active {
            let data = self.motion_plus_data();
            let n = size.min(6);
            ext[..n].copy_from_slice(&data[..n]);
        }
        ext
    }

    /// The data report for the current mode (the camera sees nothing: no bar).
    pub fn data(&mut self) -> Vec<u8> {
        let accel = self.accel();
        let core = self.core(accel);
        let mut r = vec![IN, self.mode, core[0], core[1]];
        let accel_bytes = accel.map(|v| (v >> 2) as u8);
        let ir = |n: usize| vec![0xFF; n];
        match self.mode {
            0x31 => r.extend(accel_bytes),
            0x32 => r.extend(self.extension(8)),
            0x33 => r.extend(accel_bytes.into_iter().chain(ir(12))),
            0x34 => r.extend(self.extension(19)),
            0x35 => r.extend(accel_bytes.into_iter().chain(self.extension(16))),
            0x36 => r.extend(ir(10).into_iter().chain(self.extension(9))),
            0x37 => r.extend(accel_bytes.into_iter().chain(ir(10)).chain(self.extension(6))),
            0x3D => {
                r.truncate(2);
                r.extend(self.extension(21));
            }
            0x3E | 0x3F => {
                // Interleaved: each half has one accelerometer byte and half the camera's.
                r[1] = if self.second_half { 0x3F } else { 0x3E };
                r.push(accel_bytes[if self.second_half { 2 } else { 0 }]);
                r.extend(ir(18));
                self.second_half = !self.second_half;
            }
            _ => {}
        }
        r
    }

    /// The next data report, if one is due: always in continuous mode, otherwise when
    /// something changed.
    pub fn tick(&mut self) -> Option<Vec<u8>> {
        let report = self.data();
        if !self.continuous && report == self.last {
            return None;
        }
        self.last = report.clone();
        Some(report)
    }

    /// The buttons alone, as status reports and answers carry them.
    fn buttons(&self) -> [u8; 2] {
        (self.sensors.buttons & 0x1F9F).to_be_bytes()
    }

    pub fn status(&self) -> Vec<u8> {
        let core = self.buttons();
        let flags = (self.battery <= 0x14) as u8
            | ((self.motion_plus == MotionPlus::Active) as u8) << 1
            | (self.speaker_on as u8) << 2
            | (self.ir as u8) << 3
            | self.leds << 4;
        vec![IN, 0x20, core[0], core[1], flags, 0, 0, self.battery]
    }

    fn ack(&self, report: u8, error: u8) -> Vec<u8> {
        let core = self.buttons();
        vec![IN, 0x22, core[0], core[1], report, error]
    }

    /// A register's 256 bytes, for the device at `device` (0xA2, 0xA4, 0xA6, 0xB0).
    fn registers(&self, device: u8) -> Option<[u8; 0x100]> {
        let motion_plus = |id: [u8; 6]| {
            let mut regs = [0u8; 0x100];
            regs[0x20..0x30].copy_from_slice(&MOTION_PLUS_CAL[0]);
            regs[0x30..0x40].copy_from_slice(&MOTION_PLUS_CAL[1]);
            regs[0xFA..].copy_from_slice(&id);
            regs
        };
        match device {
            0xA2 => Some(self.speaker),
            0xB0 => Some(self.camera),
            0xA4 if self.motion_plus == MotionPlus::Active => Some(motion_plus(MOTION_PLUS_ACTIVE_ID)),
            0xA6 if self.motion_plus == MotionPlus::Waiting => Some(motion_plus(MOTION_PLUS_ID)),
            _ => None,
        }
    }

    /// A write to the remote's memory or registers: the error to answer with.
    fn write(&mut self, register: bool, addr: u32, data: &[u8], replies: &mut Vec<Vec<u8>>) -> u8 {
        if !register {
            let at = addr as usize & 0xFFFF;
            let Some(slot) = self.eeprom.get_mut(at..at + data.len()) else { return BAD_ADDRESS };
            slot.copy_from_slice(data);
            return 0;
        }
        let device = (addr >> 16) as u8 & 0xFE;
        let offset = addr as usize & 0xFF;
        let was_on = self.motion_plus == MotionPlus::Active;
        match device {
            0xA2 => {
                for (i, &v) in data.iter().enumerate() {
                    self.speaker[(offset + i) & 0xFF] = v;
                }
                // A new configuration starts a new sound.
                if offset <= 0x01 {
                    self.adpcm = (0, 127);
                }
            }
            0xB0 => {
                for (i, &v) in data.iter().enumerate() {
                    self.camera[(offset + i) & 0xFF] = v;
                }
            }
            0xA6 if self.motion_plus != MotionPlus::Active => {
                for (i, &v) in data.iter().enumerate() {
                    match (offset + i, v) {
                        (0xF0, 0x55) => self.motion_plus = MotionPlus::Waiting,
                        (0xFE, 0x04..=0x07) if self.motion_plus == MotionPlus::Waiting => self.motion_plus = MotionPlus::Active,
                        _ => {}
                    }
                }
            }
            // Waking the extension port up switches the MotionPlus off.
            0xA4 if was_on => {
                if data.first() == Some(&0x55) && offset == 0xF0 {
                    self.motion_plus = MotionPlus::Asleep;
                }
            }
            _ => return NO_DEVICE,
        }
        // Something came into or left the extension port: the remote says so on its own.
        if was_on != (self.motion_plus == MotionPlus::Active) {
            replies.push(self.status());
        }
        0
    }

    /// Answers to a read, 16 bytes each.
    fn read(&self, register: bool, addr: u32, size: u16, replies: &mut Vec<Vec<u8>>) {
        let core = self.buttons();
        let answer = |addr: u16, data: &[u8], error: u8| {
            let mut r = vec![IN, 0x21, core[0], core[1], (data.len().max(1) as u8 - 1) << 4 | error, (addr >> 8) as u8, addr as u8];
            r.extend_from_slice(data);
            r.resize(23, 0);
            r
        };
        let start = addr as usize & 0xFFFF;
        let bytes: Option<Vec<u8>> = if register {
            let offset = addr as usize & 0xFF;
            self.registers((addr >> 16) as u8 & 0xFE).map(|regs| (0..size as usize).map(|i| regs[(offset + i) & 0xFF]).collect())
        } else {
            self.eeprom.get(start..start + size as usize).map(<[u8]>::to_vec)
        };
        let Some(bytes) = bytes else {
            let error = if register { NO_DEVICE } else { BAD_ADDRESS };
            replies.push(answer(start as u16, &[0; 16], error));
            return;
        };
        for (i, chunk) in bytes.chunks(16).enumerate() {
            replies.push(answer((start + i * 16) as u16, chunk, 0));
        }
    }

    /// Speaker data: decoded to samples when the speaker is on.
    fn sound(&mut self, data: &[u8], events: &mut Vec<Event>) {
        let rate_divisor = u16::from_le_bytes([self.speaker[3], self.speaker[4]]) as u32;
        if !self.speaker_on || self.speaker_muted || rate_divisor == 0 || data.is_empty() {
            return;
        }
        let pcm = self.speaker[2] == SPEAKER_PCM;
        let (samples, rate, full) = if pcm {
            (data.iter().map(|&b| (b as i8 as i16) << 8).collect(), 12_000_000 / rate_divisor, 0xFF)
        } else {
            let mut samples = Vec::with_capacity(data.len() * 2);
            for &b in data {
                for nibble in [b >> 4, b & 0x0F] {
                    let (predictor, step) = &mut self.adpcm;
                    *predictor = (*predictor + *step * ADPCM_DIFF[nibble as usize] / 8).clamp(-32768, 32767);
                    *step = (*step * ADPCM_SCALE[nibble as usize] >> 8).clamp(127, 24576);
                    samples.push(*predictor as i16);
                }
            }
            (samples, 6_000_000 / rate_divisor, 0x7F)
        };
        let volume = (self.speaker[5] as f32 / full as f32).min(1.0);
        events.push(Event::Sound { rate, volume, samples });
    }

    /// An output report (header first): the input reports it gets in answer, and what the
    /// phone has to do.
    pub fn output(&mut self, report: &[u8], events: &mut Vec<Event>) -> Vec<Vec<u8>> {
        let mut replies = Vec::new();
        if report.len() < 3 || report[0] != protocol::OUT {
            return replies;
        }
        let (id, flags) = (report[1], report[2]);
        // Every output report carries the rumble bit.
        let rumble = flags & 0x01 != 0;
        if rumble != self.rumble {
            self.rumble = rumble;
            events.push(Event::Rumble(rumble));
        }
        let on = flags & 0x04 != 0;
        let asked_ack = flags & 0x02 != 0;
        match id {
            0x11 => {
                let leds = flags >> 4;
                if leds != self.leds {
                    self.leds = leds;
                    events.push(Event::Leds(leds));
                }
            }
            0x12 if report.len() >= 4 => {
                self.continuous = on;
                self.mode = report[3];
                self.second_half = false;
                self.last.clear();
            }
            0x13 | 0x1A => self.ir = on,
            0x14 => {
                self.speaker_on = on;
                self.adpcm = (0, 127);
            }
            0x19 => self.speaker_muted = on,
            0x15 => replies.push(self.status()),
            0x16 if report.len() >= 7 => {
                let addr = u32::from_be_bytes([0, report[3], report[4], report[5]]);
                let size = (report[6] as usize).min(16);
                let data = report.get(7..7 + size).unwrap_or(&[]);
                let mut after = Vec::new();
                let error = self.write(flags & 0x0C != 0, addr, data, &mut after);
                replies.push(self.ack(0x16, error));
                replies.extend(after);
            }
            0x17 if report.len() >= 8 => {
                let addr = u32::from_be_bytes([0, report[3], report[4], report[5]]);
                let size = u16::from_be_bytes([report[6], report[7]]);
                self.read(flags & 0x0C != 0, addr, size, &mut replies);
            }
            0x18 => {
                let size = (flags >> 3) as usize;
                let data = report.get(3..3 + size.min(20)).unwrap_or(&[]).to_vec();
                self.sound(&data, events);
            }
            _ => {}
        }
        if asked_ack && matches!(id, 0x11..=0x14 | 0x19 | 0x1A) {
            replies.push(self.ack(id, 0));
        }
        replies
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::wiimote::pointer::{self, AccelCal, Pointer};
    use crate::wiimote::protocol::{GyroCal, Input};
    use std::time::{Duration, Instant};

    fn send(e: &mut Emulated, report: &[u8]) -> Vec<Vec<u8>> {
        e.output(report, &mut Vec::new())
    }

    /// What the launcher's set_up does: it finds and starts a MotionPlus, reads both calibrations.
    #[test]
    fn the_launcher_finds_a_remote_with_a_motion_plus() {
        let mut e = Emulated::default();
        let r = send(&mut e, &protocol::read_memory(protocol::ACCEL_CAL, 10));
        let Input::Read { addr, data: Some(data), .. } = protocol::parse(&r[0]) else { panic!("{r:?}") };
        assert_eq!(AccelCal::from_read(addr, &data), Some(AccelCal { zero: [512.0; 3], one_g: [104.0; 3] }));

        // Asleep, the MotionPlus doesn't answer; woken, it gives its id.
        let id = protocol::read_register(protocol::MOTION_PLUS_ID, 6);
        assert!(matches!(protocol::parse(&send(&mut e, &id)[0]), Input::Read { data: None, .. }));
        let (addr, value) = protocol::MOTION_PLUS_INIT;
        assert_eq!(send(&mut e, &protocol::write_register(addr, &[value])), [vec![IN, 0x22, 0, 0, 0x16, 0]]);
        let Input::Read { data: Some(data), .. } = protocol::parse(&send(&mut e, &id)[0]) else { panic!() };
        assert!(protocol::is_motion_plus(&data));
        assert!(matches!(protocol::parse(&e.status()), Input::Status { extension: false, .. }));

        // Started: it says so in a status report and takes the extension's place.
        let (addr, value) = protocol::MOTION_PLUS_START;
        let r = send(&mut e, &protocol::write_register(addr, &[value]));
        assert!(matches!(protocol::parse(&r[1]), Input::Status { extension: true, .. }));
        let r = send(&mut e, &protocol::read_register(protocol::EXTENSION_ID, 6));
        let Input::Read { data: Some(data), .. } = protocol::parse(&r[0]) else { panic!() };
        assert!(protocol::is_active_motion_plus(&data));
        let r = send(&mut e, &protocol::read_register(protocol::MOTION_PLUS_CAL, 32));
        let blocks: Vec<u8> = r.iter().flat_map(|r| match protocol::parse(r) {
            Input::Read { data: Some(d), .. } => d,
            _ => panic!(),
        }).collect();
        assert_eq!(GyroCal::parse(&blocks), Some(GyroCal::default()));

        // Its data in the 0x37 reports.
        send(&mut e, &protocol::report_mode());
        e.sensors.gyro = [0.0, 0.0, 30.0];
        let Input::Data { dots, ext, .. } = protocol::parse(&e.tick().unwrap()) else { panic!() };
        assert!(dots.is_empty());
        let m = protocol::motion_plus(&ext).unwrap();
        assert!(m.slow.iter().all(|&s| s) && !m.extension);
        // Waking the extension port switches it off again.
        let (addr, value) = protocol::EXTENSION_INIT[0];
        let r = send(&mut e, &protocol::write_register(addr, &[value]));
        assert!(matches!(protocol::parse(&r[1]), Input::Status { extension: false, .. }));
    }

    fn started() -> Emulated {
        let mut e = Emulated::default();
        for (addr, value) in [protocol::MOTION_PLUS_INIT, protocol::MOTION_PLUS_START] {
            send(&mut e, &protocol::write_register(addr, &[value]));
        }
        send(&mut e, &protocol::report_mode());
        e
    }

    #[test]
    fn buttons_and_accelerometer_read_back() {
        let mut e = started();
        e.sensors.buttons = 0x0808; // A and ↑
        e.sensors.accel = [0.5, -1.0, 2.0];
        let r = e.tick().unwrap();
        assert_eq!(protocol::button_names(u16::from_be_bytes([r[2], r[3]]) & 0x1F9F), ["↑", "A"]);
        assert_eq!(pointer::accel(&r), [564, 408, 720]);
    }

    /// Turning the phone right, then raising its nose, moves the launcher's pointer the same way.
    #[test]
    fn the_gyroscope_points_like_a_motion_plus() {
        let mut e = started();
        let mut p = Pointer::default();
        let mut now = Instant::now();
        let mut run = |e: &mut Emulated, p: &mut Pointer, gyro: [f32; 3], n: usize| {
            e.sensors.gyro = gyro;
            let mut pos = None;
            for _ in 0..n {
                now += Duration::from_millis(10);
                let r = e.tick().unwrap();
                let Input::Data { dots, ext, .. } = protocol::parse(&r) else { panic!() };
                pos = p.update(&dots, pointer::accel(&r), protocol::motion_plus(&ext).as_ref(), now).pos;
            }
            pos.unwrap()
        };
        // Still on a table: the gyroscope's rest value is learned.
        let start = run(&mut e, &mut p, [0.0; 3], 40);
        // Turning right is turning clockwise seen from above: about −z.
        let right = run(&mut e, &mut p, [0.0, 0.0, -20.0], 30);
        assert!(right.0 - start.0 > 0.25 && (right.1 - start.1).abs() < 0.05, "{start:?} {right:?}");
        // Raising the nose: about −x (x points left), right-hand rule.
        let up = run(&mut e, &mut p, [-20.0, 0.0, 0.0], 30);
        assert!(up.1 - right.1 < -0.2, "{right:?} {up:?}");
    }

    #[test]
    fn writes_are_acknowledged_and_unknown_devices_refused() {
        let mut e = Emulated::default();
        assert_eq!(send(&mut e, &protocol::write_register(0xB0_0030, &[0x08])), [vec![IN, 0x22, 0, 0, 0x16, 0]]);
        assert_eq!(send(&mut e, &protocol::write_register(0xA4_00F0, &[0x55])), [vec![IN, 0x22, 0, 0, 0x16, NO_DEVICE]]);
        // Acknowledgement asked for.
        assert_eq!(send(&mut e, &[0xA2, 0x11, 0x12]), [vec![IN, 0x22, 0, 0, 0x11, 0]]);
        let mut events = Vec::new();
        e.output(&[0xA2, 0x11, 0x41], &mut events);
        assert_eq!(events, [Event::Rumble(true), Event::Leds(4)]);
    }

    #[test]
    fn speaker_data_becomes_sound() {
        let mut e = Emulated::default();
        // Wii Party's configuration: ADPCM, 3000 Hz (divisor 2000), volume 0x40.
        send(&mut e, &[0xA2, 0x14, 0x04]);
        send(&mut e, &protocol::write_register(0xA2_0001, &[0x00, 0x00, 0xD0, 0x07, 0x40, 0x00, 0x00]));
        let mut events = Vec::new();
        let mut data = vec![0xA2, 0x18, 20 << 3];
        data.extend([0x17; 20]);
        e.output(&data, &mut events);
        let [Event::Sound { rate, volume, samples }] = &events[..] else { panic!("{events:?}") };
        assert_eq!(*rate, 3000);
        assert!((volume - 0x40 as f32 / 127.0).abs() < 1e-6);
        assert_eq!(samples.len(), 40);
        assert!(samples.iter().any(|&s| s != 0));
        // Muted: nothing.
        send(&mut e, &[0xA2, 0x19, 0x04]);
        events.clear();
        e.output(&data, &mut events);
        assert!(events.is_empty());
    }

    #[test]
    fn reports_only_what_changed_unless_continuous() {
        let mut e = Emulated::default();
        send(&mut e, &[0xA2, 0x12, 0x00, 0x30]);
        assert!(e.tick().is_some());
        assert!(e.tick().is_none());
        e.sensors.buttons = 0x0004;
        assert_eq!(e.tick().unwrap(), [IN, 0x30, 0x00, 0x04]);
        send(&mut e, &[0xA2, 0x12, 0x04, 0x31]);
        assert!(e.tick().is_some() && e.tick().is_some());
    }
}
