//! Lends connected remotes to Dolphin (Source/Core/Core/HW/WiimoteReal/IOBridge.* in
//! the fork): they stay connected to the launcher, in their slots, and their reports
//! go through a local socket.
//!
//! No sensor bar means no pointer for the game, so a remote with a MotionPlus goes
//! through `Proxy`, which makes one from the gyroscope. The game doesn't use the
//! MotionPlus and must not notice it: the remote stays in the report mode that carries
//! both IR and gyroscope data, and the game sees a plain remote in the mode it asked for.

use super::pointer::{AccelCal, ForGame, Pointer, EMPTY};
use super::{pointer, protocol};
use std::time::{Duration, Instant};

/// How long the MotionPlus must say something is plugged into it before the game gets
/// the real remote: a Nunchuk stays plugged, a loose contact glitches for an instant.
const PLUGGED_FOR: Duration = Duration::from_millis(300);
/// How long the answers to our own register writes may take: one that never comes
/// must not swallow one of the game's later on.
const MUTED_FOR: Duration = Duration::from_millis(200);
/// Answer for an address with no device behind it, as on a remote with no extension.
const NO_DEVICE: u8 = 7;
const IR_TRACKING_ON: u8 = 0x08;

#[derive(Default, Debug, PartialEq)]
pub struct Forward {
    pub to_remote: Vec<Vec<u8>>,
    pub to_game: Vec<Vec<u8>>,
}

pub struct Proxy {
    /// Showing the game a plain remote. False: reports go through untouched.
    hiding: bool,
    /// Had to turn the MotionPlus off: the launcher must look for it again afterwards.
    pub motion_plus_off: bool,
    mode: u8,
    continuous: bool,
    plugged_since: Option<Instant>,
    /// Which half of an interleaved full-IR pair (0x3E/0x3F) goes next.
    second_half: bool,
    ir_on: bool,
    tracking: bool,
    ir_mode: u8,
    rumble: u8,
    buttons: [u8; 2],
    last_report: Vec<u8>,
    /// Our own register writes, whose answers aren't for the game, and when they went.
    muted_acks: usize,
    muted_at: Instant,
    /// Where the remote points, from the bar or the gyroscope: the launcher gets it
    /// back (bias learned, bar geometry) when the game lets the remote go.
    pub pointer: Pointer,
}

impl Proxy {
    pub fn new(motion_plus: bool, pointer: Pointer) -> Self {
        Proxy {
            hiding: motion_plus,
            motion_plus_off: false,
            mode: 0x30,
            continuous: false,
            plugged_since: None,
            second_half: false,
            ir_on: false,
            tracking: false,
            ir_mode: 0,
            rumble: 0,
            buttons: [0, 0],
            last_report: Vec::new(),
            muted_acks: 0,
            muted_at: Instant::now(),
            pointer,
        }
    }

    /// Mode the remote stays in: 0x37, continuously, whatever the game asks for.
    fn keep_reporting(&self, flags: u8) -> Vec<u8> {
        vec![protocol::OUT, 0x12, flags | 0x04 | self.rumble, 0x37]
    }

    /// First report to send to the remote.
    pub fn start(&self) -> Option<Vec<u8>> {
        self.hiding.then(|| self.keep_reporting(0))
    }

    /// Something is plugged into the MotionPlus, which the game can't get through this
    /// translation: turns the MotionPlus off and lets the remote speak for itself.
    fn stop_hiding(&mut self, now: Instant, out: &mut Forward) {
        eprintln!("wiimote: something is plugged into the MotionPlus, the game gets the real remote (mode {:02X})", self.mode);
        self.hiding = false;
        self.motion_plus_off = true;
        self.muted_acks += 2;
        self.muted_at = now;
        // Every output report carries the rumble bit: these mustn't stop it.
        let write = |addr, value| {
            let mut w = protocol::write_register(addr, &[value]);
            w[2] |= self.rumble;
            w
        };
        out.to_remote.push(write(protocol::EXTENSION_INIT[0].0, protocol::EXTENSION_INIT[0].1));
        out.to_remote.push(write(0xB0_0033, self.ir_mode));
        out.to_remote.push(vec![protocol::OUT, 0x13, self.rumble | if self.ir_on { 0x04 } else { 0 }]);
        out.to_remote.push(vec![protocol::OUT, 0x12, self.rumble | if self.continuous { 0x04 } else { 0 }, self.mode]);
    }

    /// A report from the game.
    pub fn from_game(&mut self, report: &[u8]) -> Forward {
        let mut out = Forward::default();
        if report.len() < 3 {
            return out;
        }
        if !self.hiding {
            out.to_remote.push(report.to_vec());
            return out;
        }
        let rumble_was = self.rumble;
        self.rumble = report[2] & 0x01;
        let register = report[2] & 0x0C != 0;
        // The low bit of an I2C address is the read/write one: the remote ignores it.
        let device = report.get(3).map(|&a| a & 0xFE);
        // The extension and MotionPlus addresses, which would disturb the MotionPlus.
        let extension = matches!(device, Some(0xA4 | 0xA6));
        // A size the remote refuses gets its own error from it.
        let sized = match report[1] {
            0x16 => report.get(6).is_some_and(|n| (1..=16).contains(n)),
            0x17 => report.len() >= 8 && report[6..8] != [0, 0],
            _ => false,
        };
        match report[1] {
            // A real remote ignores a mode it doesn't have: so does the one forwarded below.
            0x12 if report.len() >= 4 && matches!(report[3], 0x30..=0x37 | 0x3D..=0x3F) => {
                self.mode = report[3];
                self.continuous = report[2] & 0x04 != 0;
                self.second_half = false;
                out.to_remote.push(self.keep_reporting(report[2] & 0x02));
            }
            // The camera stays on: the game only gets its data while it wants it.
            0x13 | 0x1A => {
                if report[1] == 0x13 {
                    self.ir_on = report[2] & 0x04 != 0;
                }
                out.to_remote.push(vec![protocol::OUT, report[1], report[2] | 0x04]);
            }
            0x16 if register && sized && extension => {
                out.to_game.push(vec![protocol::IN, 0x22, self.buttons[0], self.buttons[1], 0x16, NO_DEVICE]);
            }
            0x16 if register && sized && device == Some(0xB0) => {
                let mut write = report.to_vec();
                let start = u16::from_be_bytes([report[4], report[5]]);
                for i in 0..report[6] as usize {
                    let Some(value) = write.get_mut(7 + i) else { break };
                    match start.wrapping_add(i as u16) {
                        0x0030 => self.tracking = *value == IR_TRACKING_ON,
                        0x0033 => {
                            self.ir_mode = *value;
                            *value = 0x01;
                        }
                        _ => {}
                    }
                }
                out.to_remote.push(write);
            }
            0x17 if register && sized && extension => {
                let mut reply = vec![protocol::IN, 0x21, self.buttons[0], self.buttons[1], 0xF0 | NO_DEVICE, report[4], report[5]];
                reply.resize(23, 0);
                out.to_game.push(reply);
            }
            _ => out.to_remote.push(report.to_vec()),
        }
        // Every output report carries the rumble bit, the ones answered here too.
        if out.to_remote.is_empty() && self.rumble != rumble_was {
            out.to_remote.push(vec![protocol::OUT, 0x10, self.rumble]);
        }
        out
    }

    /// A report from the remote.
    pub fn from_remote(&mut self, report: &[u8], now: Instant) -> Forward {
        let mut out = Forward::default();
        if report.len() < 2 {
            return out;
        }
        if report[1] == 0x22 && report.get(4) == Some(&0x16) && self.muted_acks > 0 {
            if now.saturating_duration_since(self.muted_at) < MUTED_FOR {
                self.muted_acks -= 1;
                return out;
            }
            self.muted_acks = 0;
        }
        if !self.hiding {
            out.to_game.push(report.to_vec());
            return out;
        }
        match report[1] {
            0x20 if report.len() >= 8 => {
                self.buttons = [report[2], report[3]];
                let mut status = report.to_vec();
                status[4] &= !0x02;
                status[4] = status[4] & !0x08 | if self.ir_on { 0x08 } else { 0 };
                out.to_game.push(status);
                // A status report stops data reports until the mode is set again.
                out.to_remote.push(self.keep_reporting(0));
            }
            0x21 | 0x22 => {
                if report.len() >= 4 {
                    self.buttons = [report[2], report[3]];
                }
                // The game reading the accelerometer's calibration.
                if let protocol::Input::Read { addr, data: Some(data), .. } = protocol::parse(report) {
                    if let Some(cal) = AccelCal::from_read(addr, &data) {
                        self.pointer.accel_cal = cal;
                    }
                }
                out.to_game.push(report.to_vec());
            }
            0x37 if report.len() >= 23 => self.data(report, now, &mut out),
            _ => {}
        }
        out
    }

    fn data(&mut self, report: &[u8], now: Instant, out: &mut Forward) {
        self.buttons = [report[2], report[3]];
        let ext: [u8; 6] = report[17..23].try_into().unwrap();
        let sample = protocol::motion_plus(&ext);
        if let Some(m) = &sample {
            self.plugged_since = m.extension.then(|| self.plugged_since.unwrap_or(now));
        }
        if self.plugged_since.is_some_and(|t| now.saturating_duration_since(t) >= PLUGGED_FOR) {
            return self.stop_hiding(now, out);
        }
        let raw = protocol::basic_dots(&report[7..17]);
        let visible: Vec<(f32, f32)> = raw.iter().filter(|d| protocol::dot_visible(d)).map(|d| (d.0 as f32, d.1 as f32)).collect();
        // No gyroscope data in this report: no pointer made from it, rather than a frozen one.
        let seen = self.pointer.update(&visible, pointer::accel(report), sample.as_ref(), now);
        let (dots, size) = match seen.game {
            ForGame::Real => (raw, self.pointer.dot_size_now()),
            ForGame::Dots(dots, size) => (dots, size),
            ForGame::Nothing => ([EMPTY; 4], 0),
        };
        let dots = if self.ir_on && self.tracking { dots } else { [EMPTY; 4] };

        let accel = &report[4..7];
        // The accelerometer's low bits share the button bytes, only in modes that carry it.
        let core = match self.mode {
            0x31 | 0x33 | 0x35 | 0x37 => [report[2], report[3]],
            _ => [report[2] & 0x1F, report[3] & 0x9F],
        };
        let mut data = vec![protocol::IN, self.mode, core[0], core[1]];
        // No extension: the port reads 0xFF.
        let no_extension = |n: usize| std::iter::repeat_n(0xFFu8, n);
        match self.mode {
            0x30 => {}
            0x31 => data.extend_from_slice(accel),
            0x32 => data.extend(no_extension(8)),
            0x33 => {
                data.extend_from_slice(accel);
                data.extend(dots.iter().flat_map(|&d| extended_dot(d, size)));
            }
            0x34 => data.extend(no_extension(19)),
            0x35 => {
                data.extend_from_slice(accel);
                data.extend(no_extension(16));
            }
            0x36 => {
                data.extend(basic_ir(&dots));
                data.extend(no_extension(9));
            }
            0x37 => {
                data.extend_from_slice(accel);
                data.extend(basic_ir(&dots));
                data.extend(no_extension(6));
            }
            0x3D => data = [protocol::IN, 0x3D].into_iter().chain(no_extension(21)).collect(),
            // Full IR comes in two halves, sent in turn as a real remote does (Dolphin keeps
            // only the newest data report it finds queued, so not both at once).
            0x3E | 0x3F => {
                let [x, y, z] = pointer::accel(report);
                let (id, axis, z_shift, pair) = if self.second_half { (0x3F, y, 2, &dots[2..]) } else { (0x3E, x, 6, &dots[..2]) };
                self.second_half = !self.second_half;
                data = vec![
                    protocol::IN,
                    id,
                    report[2] & 0x1F | ((z >> z_shift & 3) as u8) << 5,
                    report[3] & 0x9F | ((z >> (z_shift + 2) & 3) as u8) << 5,
                    (axis >> 2) as u8,
                ];
                data.extend(pair.iter().flat_map(|&d| full_dot(d, size)));
            }
            _ => return,
        }
        // Without continuous reporting, a remote only speaks when something changes.
        if self.continuous || data != self.last_report {
            self.last_report.clone_from(&data);
            out.to_game.push(data);
        }
    }
}

fn extended_dot((x, y): (u16, u16), size: u8) -> [u8; 3] {
    if x >= EMPTY.0 {
        return [0xFF; 3];
    }
    [x as u8, y as u8, ((y >> 8) << 6 | (x >> 8) << 4) as u8 | size.min(15)]
}

/// As Dolphin's camera: a box `size` around the dot (7 bits a side), no intensity.
fn full_dot((x, y): (u16, u16), size: u8) -> [u8; 9] {
    if x >= EMPTY.0 {
        return [0xFF; 9];
    }
    let [a, b, c] = extended_dot((x, y), size);
    let s = size as u16;
    let seven = |v: u16| (v & 0x7F) as u8;
    [a, b, c, seven(x.saturating_sub(s)), seven(y.saturating_sub(s)), seven((x + s).min(1024)), seven((y + s).min(768)), 0, 0]
}

fn basic_ir(dots: &[(u16, u16); 4]) -> Vec<u8> {
    dots.chunks(2)
        .flat_map(|pair| {
            let ((x1, y1), (x2, y2)) = (pair[0], pair[1]);
            [x1 as u8, y1 as u8, ((y1 >> 8) << 6 | (x1 >> 8) << 4 | (y2 >> 8) << 2 | x2 >> 8) as u8, x2 as u8, y2 as u8]
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A remote at rest, its MotionPlus at the slow rate with nothing plugged in.
    fn report(ir: [u8; 10]) -> Vec<u8> {
        let mut r = vec![0xA1, 0x37, 0x00, 0x08, 0x80, 0x81, 0x82];
        r.extend_from_slice(&ir);
        // Yaw, roll and pitch at 0x2000, all three slow.
        r.extend_from_slice(&[0x00, 0x00, 0x00, 0x83, 0x82, 0x82]);
        r
    }
    const NO_IR: [u8; 10] = [0xFF; 10];
    const BAR: [u8; 10] = [100, 200, 0x00, 44, 88, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF];

    fn game_set_up(p: &mut Proxy, mode: u8) {
        p.from_game(&[0xA2, 0x12, 0x04, mode]);
        p.from_game(&[0xA2, 0x13, 0x04]);
        p.from_game(&protocol::write_register(0xB0_0030, &[0x08]));
        p.from_game(&protocol::write_register(0xB0_0033, &[0x03]));
    }

    fn calibrated(p: &mut Proxy, at: Instant) -> Instant {
        let mut now = at;
        for _ in 0..40 {
            now += Duration::from_millis(10);
            p.from_remote(&report(NO_IR), now);
        }
        now
    }

    fn plugged() -> Vec<u8> {
        let mut r = report(NO_IR);
        r[21] |= 0x01;
        r
    }

    #[test]
    fn without_motion_plus_everything_goes_through() {
        let mut p = Proxy::new(false, Pointer::default());
        assert_eq!(p.start(), None);
        let setting = vec![0xA2, 0x12, 0x00, 0x33];
        assert_eq!(p.from_game(&setting).to_remote, [setting]);
        let data = vec![0xA1, 0x33, 0, 8, 1, 2, 3];
        assert_eq!(p.from_remote(&data, Instant::now()).to_game, [data]);
    }

    #[test]
    fn keeps_the_remote_in_the_mode_with_gyroscope_data() {
        let mut p = Proxy::new(true, Pointer::default());
        assert_eq!(p.start(), Some(vec![0xA2, 0x12, 0x04, 0x37]));
        assert_eq!(p.from_game(&[0xA2, 0x12, 0x03, 0x33]).to_remote, [vec![0xA2, 0x12, 0x07, 0x37]]);
    }

    #[test]
    fn keeps_the_camera_in_basic_mode() {
        let mut p = Proxy::new(true, Pointer::default());
        let out = p.from_game(&protocol::write_register(0xB0_0033, &[0x03]));
        assert_eq!(out.to_remote[0][7], 0x01);
        // …and on when the game turns it off.
        assert_eq!(p.from_game(&[0xA2, 0x13, 0x00]).to_remote, [vec![0xA2, 0x13, 0x04]]);
    }

    #[test]
    fn hides_the_motion_plus() {
        let mut p = Proxy::new(true, Pointer::default());
        p.from_remote(&report(NO_IR), Instant::now());
        // Nothing answers at its address, or the extension's.
        for addr in [0xA6_00FA, 0xA4_00FA] {
            let out = p.from_game(&protocol::read_register(addr, 6));
            assert!(out.to_remote.is_empty());
            assert_eq!(out.to_game[0][1..5], [0x21, 0x00, 0x08, 0xF7]);
            assert_eq!(out.to_game[0].len(), 23);
        }
        let out = p.from_game(&protocol::write_register(0xA4_00F0, &[0x55]));
        assert!(out.to_remote.is_empty());
        assert_eq!(out.to_game, [vec![0xA1, 0x22, 0x00, 0x08, 0x16, 7]]);
        // The rest of the remote still answers.
        assert_eq!(p.from_game(&protocol::read_register(0x00_0016, 8)).to_remote.len(), 1);
        // No extension in the status, and the mode is set again.
        let out = p.from_remote(&[0xA1, 0x20, 0, 8, 0x0A, 0, 0, 0x90], Instant::now());
        assert_eq!(out.to_game, [vec![0xA1, 0x20, 0, 8, 0x00, 0, 0, 0x90]]);
        assert_eq!(out.to_remote, [vec![0xA2, 0x12, 0x04, 0x37]]);
    }

    #[test]
    fn an_address_with_its_read_bit_is_the_same_device() {
        let mut p = Proxy::new(true, Pointer::default());
        let out = p.from_game(&protocol::read_register(0xA5_00FA, 6));
        assert!(out.to_remote.is_empty());
        assert_eq!(out.to_game[0][4], 0xF7);
        assert!(p.from_game(&protocol::write_register(0xA7_00F0, &[0x55])).to_remote.is_empty());
        assert_eq!(p.from_game(&protocol::write_register(0xB1_0033, &[0x03])).to_remote[0][7], 0x01);
    }

    #[test]
    fn sizes_the_remote_refuses_get_its_own_answer() {
        let mut p = Proxy::new(true, Pointer::default());
        let empty = protocol::write_register(0xA4_00F0, &[]);
        assert_eq!(p.from_game(&empty).to_remote, [empty]);
        let mut too_big = protocol::write_register(0xA4_00F0, &[0x55]);
        too_big[6] = 17;
        assert_eq!(p.from_game(&too_big).to_remote, [too_big]);
        let none = protocol::read_register(0xA4_00FA, 0);
        assert_eq!(p.from_game(&none).to_remote, [none]);
        let mut camera = protocol::write_register(0xB0_0033, &[0x03]);
        camera[6] = 0;
        assert_eq!(p.from_game(&camera).to_remote, [camera]);
    }

    #[test]
    fn translates_to_the_mode_the_game_wants() {
        let mut p = Proxy::new(true, Pointer::default());
        let now = Instant::now();
        p.from_game(&[0xA2, 0x12, 0x04, 0x31]);
        assert_eq!(p.from_remote(&report(NO_IR), now).to_game, [vec![0xA1, 0x31, 0x00, 0x08, 0x80, 0x81, 0x82]]);
        p.from_game(&[0xA2, 0x12, 0x04, 0x30]);
        assert_eq!(p.from_remote(&report(NO_IR), now).to_game, [vec![0xA1, 0x30, 0x00, 0x08]]);
        for (mode, size) in [(0x32, 12), (0x33, 19), (0x34, 23), (0x35, 23), (0x36, 23), (0x37, 23), (0x3D, 23)] {
            p.from_game(&[0xA2, 0x12, 0x04, mode]);
            assert_eq!(p.from_remote(&report(NO_IR), now).to_game[0].len(), size, "{mode:x}");
        }
    }

    #[test]
    fn an_invalid_mode_changes_nothing() {
        let mut p = Proxy::new(true, Pointer::default());
        p.from_game(&[0xA2, 0x12, 0x04, 0x33]);
        let invalid = vec![0xA2, 0x12, 0x04, 0x38];
        assert_eq!(p.from_game(&invalid).to_remote, [invalid]);
        let out = p.from_remote(&report(NO_IR), Instant::now());
        assert_eq!(out.to_game.len(), 1);
        assert_eq!(out.to_game[0][1], 0x33);
    }

    #[test]
    fn extension_bytes_read_as_an_empty_port() {
        let mut p = Proxy::new(true, Pointer::default());
        for (mode, from) in [(0x32, 4), (0x34, 4), (0x35, 7), (0x36, 14), (0x37, 17), (0x3D, 2)] {
            p.from_game(&[0xA2, 0x12, 0x04, mode]);
            let out = p.from_remote(&report(NO_IR), Instant::now()).to_game.remove(0);
            assert!(out[from..].iter().all(|&b| b == 0xFF), "{mode:x} {out:02X?}");
        }
    }

    #[test]
    fn modes_without_accelerometer_have_no_accelerometer_bits() {
        let mut p = Proxy::new(true, Pointer::default());
        p.from_game(&[0xA2, 0x12, 0x00, 0x30]);
        let mut r = report(NO_IR);
        r[2] = 0x60;
        r[3] = 0x68;
        assert_eq!(p.from_remote(&r, Instant::now()).to_game, [vec![0xA1, 0x30, 0x00, 0x08]]);
        // Only the accelerometer moved: nothing new for the game.
        r[2] = 0x20;
        assert!(p.from_remote(&r, Instant::now()).to_game.is_empty());
    }

    #[test]
    fn stays_quiet_without_continuous_reporting_until_something_changes() {
        let mut p = Proxy::new(true, Pointer::default());
        p.from_game(&[0xA2, 0x12, 0x00, 0x30]);
        let now = Instant::now();
        assert_eq!(p.from_remote(&report(NO_IR), now).to_game.len(), 1);
        assert_eq!(p.from_remote(&report(NO_IR), now).to_game.len(), 0);
        let mut press = report(NO_IR);
        press[3] = 0x00;
        assert_eq!(p.from_remote(&press, now).to_game.len(), 1);
    }

    #[test]
    fn passes_the_real_bar_on_in_the_format_the_game_wants() {
        let mut p = Proxy::new(true, Pointer::default());
        game_set_up(&mut p, 0x33);
        let now = Instant::now();
        // A pair counts as the bar once it has held for a few reports.
        for i in 0..3 {
            p.from_remote(&report(BAR), now + Duration::from_millis(10 * i));
        }
        let out = p.from_remote(&report(BAR), now + Duration::from_millis(30));
        // x=100 y=200, then x=44 y=88, each with a size (125 px apart: far, size 1).
        assert_eq!(out.to_game[0][7..13], [100, 200, 1, 44, 88, 1]);
        assert_eq!(out.to_game[0][13..19], [0xFF; 6]);

        game_set_up(&mut p, 0x37);
        let out = p.from_remote(&report(BAR), now + Duration::from_millis(40));
        assert_eq!(out.to_game[0][7..17], BAR);
    }

    #[test]
    fn full_ir_comes_in_two_halves_made_from_the_basic_dots() {
        let mut p = Proxy::new(true, Pointer::default());
        game_set_up(&mut p, 0x3E);
        let now = Instant::now();
        let mut r = report(BAR);
        // x 0x200 | 0b10, z 0x208 | 0b10: z = 0b10_0000_1010.
        r[2] = 0x40;
        r[3] = 0x48;
        r[6] = 0x82;
        for i in 0..3 {
            p.from_remote(&r, now + Duration::from_millis(10 * i));
        }
        // Setting the mode again starts from the first half.
        p.from_game(&[0xA2, 0x12, 0x04, 0x3E]);
        let first = p.from_remote(&r, now + Duration::from_millis(30)).to_game.remove(0);
        let second = p.from_remote(&r, now + Duration::from_millis(40)).to_game.remove(0);
        assert!(!p.motion_plus_off);
        assert_eq!((first[1], first.len(), second[1], second.len()), (0x3E, 23, 0x3F, 23));
        // Buttons in both; X then Y; Z's bits 7-6 and 9-8, then 3-2 and 5-4, in the spare button bits.
        assert_eq!(first[2..5], [0x00, 0x08 | 0b10 << 5, 0x80]);
        assert_eq!(second[2..5], [0b10 << 5, 0x08, 0x81]);
        // Dots 1 and 2 in the first half, with their box (size 1); nothing in the second.
        assert_eq!(first[5..8], [100, 200, 1]);
        assert_eq!(first[8..12], [99 & 0x7F, 199 & 0x7F, 101 & 0x7F, 201 & 0x7F]);
        assert_eq!(first[14..17], [44, 88, 1]);
        assert!(second[5..].iter().all(|&b| b == 0xFF));
    }

    #[test]
    fn no_ir_data_while_the_game_has_not_asked_for_it() {
        let mut p = Proxy::new(true, Pointer::default());
        p.from_game(&[0xA2, 0x12, 0x04, 0x33]);
        let out = p.from_remote(&report(BAR), Instant::now());
        assert_eq!(out.to_game[0][7..19], [0xFF; 12]);
    }

    #[test]
    fn without_a_bar_the_gyroscope_points() {
        let mut p = Proxy::new(true, Pointer::default());
        game_set_up(&mut p, 0x33);
        let now = calibrated(&mut p, Instant::now());
        let dots = |p: &mut Proxy, r: &[u8], now| {
            let out = p.from_remote(r, now).to_game.remove(0);
            let x = |i: usize| out[7 + i] as u16 | ((out[9 + i] as u16 >> 4) & 3) << 8;
            let y = |i: usize| out[8 + i] as u16 | ((out[9 + i] as u16 >> 6) & 3) << 8;
            (x(0), y(0), x(3), y(3))
        };
        // In the middle to start with: two dots side by side, a bar on top of the screen.
        let centre = dots(&mut p, &report(NO_IR), now);
        assert_eq!(centre, (442, 452, 582, 452));
        // Turning moves them, and they stay in the camera's view however far it goes.
        let mut turned = report(NO_IR);
        turned[17] = 0xFF;
        turned[20] = 0x8F;
        let mut last = centre;
        let mut now = now;
        for _ in 0..300 {
            now += Duration::from_millis(10);
            last = dots(&mut p, &turned, now);
        }
        assert_ne!(last.0, centre.0);
        assert_eq!(last.1, centre.1);
        assert!(last.0 < 1023 && last.2 < 1023);
    }

    /// The two dots the gyroscope makes, as (x, y) pairs.
    fn gyro_dots(p: &mut Proxy, r: &[u8], now: Instant) -> [(u16, u16); 2] {
        let out = p.from_remote(r, now).to_game.remove(0);
        let x = |i: usize| out[7 + i] as u16 | ((out[9 + i] as u16 >> 4) & 3) << 8;
        let y = |i: usize| out[8 + i] as u16 | ((out[9 + i] as u16 >> 6) & 3) << 8;
        [(x(0), y(0)), (x(3), y(3))]
    }

    /// A remote at rest, its face up (z at 1 g), or twisted (gravity partly along x).
    fn held(x: u8, z: u8) -> Vec<u8> {
        let mut r = report(NO_IR);
        r[4] = x;
        r[6] = z;
        r
    }

    #[test]
    fn the_twist_turns_the_dots_the_gyroscope_makes() {
        let mut p = Proxy::new(true, Pointer::default());
        game_set_up(&mut p, 0x33);
        let now = calibrated(&mut p, Instant::now());
        // Face up: level, as before.
        let level = gyro_dots(&mut p, &held(0x80, 0x9A), now);
        assert_eq!(level, [(442, 452), (582, 452)]);
        // Twisted as Dolphin's emulated remote is with a positive roll (x below its zero):
        // the left dot goes down the camera and the right one up, around the camera's middle.
        let [left, right] = gyro_dots(&mut p, &held(0x78, 0x98), now);
        assert!(left.1 > right.1, "{left:?} {right:?}");
        let apart = ((right.0 as f32 - left.0 as f32).powi(2) + (right.1 as f32 - left.1 as f32).powi(2)).sqrt();
        assert!((apart - pointer::SPACING).abs() < 2.0, "{apart}");
        // Pointing up or down, gravity says nothing about the twist: it stays.
        let [l, r] = gyro_dots(&mut p, &held(0x80, 0x80), now);
        assert!(l.1 > r.1);
        // Twisted the other way.
        let [left, right] = gyro_dots(&mut p, &held(0x88, 0x98), now);
        assert!(left.1 < right.1);
    }

    #[test]
    fn reads_the_accelerometer_calibration_the_game_asks_for() {
        let mut p = Proxy::new(true, Pointer::default());
        // Zero at 0x84 on each axis, 1 g at 0x9E: 0x68 in 10 bits.
        let mut reply = vec![0xA1, 0x21, 0x00, 0x08, 0x90, 0x00, 0x16, 0x84, 0x84, 0x84, 0x00, 0x9E, 0x9E, 0x9E, 0x00];
        reply.resize(23, 0);
        assert_eq!(p.from_remote(&reply, Instant::now()).to_game, [reply.clone()]);
        assert_eq!(p.pointer.accel_cal.zero, [528.0; 3]);
        assert_eq!(p.pointer.accel_cal.one_g, [104.0; 3]);
        // Another address is something else.
        let mut other = reply.clone();
        other[6] = 0x20;
        let mut q = Proxy::new(true, Pointer::default());
        q.from_remote(&other, Instant::now());
        assert_eq!(q.pointer.accel_cal, AccelCal::default());
        // A bigger read from 0x0010 has it 6 bytes in.
        let mut block = vec![0xA1, 0x21, 0x00, 0x08, 0xF0, 0x00, 0x10, 0, 0, 0, 0, 0, 0, 0x84, 0x84, 0x84, 0x00, 0x9E, 0x9E, 0x9E, 0x00];
        block.resize(23, 0);
        q.from_remote(&block, Instant::now());
        assert_eq!(q.pointer.accel_cal, p.pointer.accel_cal);
    }

    #[test]
    fn the_gyroscope_carries_on_from_the_real_bar() {
        let mut p = Proxy::new(true, Pointer::default());
        game_set_up(&mut p, 0x33);
        let mut now = calibrated(&mut p, Instant::now());
        let mut last = vec![];
        for _ in 0..4 {
            now += Duration::from_millis(10);
            last = p.from_remote(&report(BAR), now).to_game.remove(0);
        }
        // Bar gone: the same two dots, made by the gyroscope now, not an empty camera.
        now += Duration::from_millis(10);
        let [a, b] = gyro_dots(&mut p, &report(NO_IR), now);
        let x = |i: usize| last[7 + i] as u16 | ((last[9 + i] as u16 >> 4) & 3) << 8;
        let y = |i: usize| last[8 + i] as u16 | ((last[9 + i] as u16 >> 6) & 3) << 8;
        let mut real = [(x(0), y(0)), (x(3), y(3))];
        let mut made = [a, b];
        real.sort();
        made.sort();
        for (r, m) in real.iter().zip(made) {
            assert!(r.0.abs_diff(m.0) <= 1 && r.1.abs_diff(m.1) <= 1, "{real:?} {made:?}");
        }
    }

    #[test]
    fn one_stray_dot_does_not_hide_the_gyroscope_pointer() {
        let mut p = Proxy::new(true, Pointer::default());
        game_set_up(&mut p, 0x33);
        let mut now = calibrated(&mut p, Instant::now());
        // A lamp: one dot at (300, 200), for seconds.
        let lamp = [44, 200, 0x1F, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF];
        for _ in 0..300 {
            now += Duration::from_millis(10);
            let [a, b] = gyro_dots(&mut p, &report(lamp), now);
            assert_eq!([a, b], [(442, 452), (582, 452)]);
        }
    }

    #[test]
    fn a_motion_plus_gone_quiet_leaves_no_frozen_pointer() {
        let mut p = Proxy::new(true, Pointer::default());
        game_set_up(&mut p, 0x33);
        let mut now = calibrated(&mut p, Instant::now());
        assert!(p.pointer.gyro.ready());
        let mut gone = report(NO_IR);
        gone[17..23].fill(0xFF);
        now += Duration::from_millis(10);
        assert_eq!(p.from_remote(&gone, now).to_game[0][7..19], [0xFF; 12], "pointer still shown without a gyroscope");
    }

    #[test]
    fn lending_again_keeps_the_gyroscope_calibrated() {
        let mut p = Proxy::new(true, Pointer::default());
        game_set_up(&mut p, 0x33);
        let now = calibrated(&mut p, Instant::now());
        // What take_back gets back, then lends to the next game: it points at once.
        let mut q = Proxy::new(true, p.pointer);
        game_set_up(&mut q, 0x33);
        assert_eq!(gyro_dots(&mut q, &report(NO_IR), now + Duration::from_millis(10)), [(442, 452), (582, 452)]);
    }

    #[test]
    fn a_glitch_in_the_plugged_bit_changes_nothing() {
        let mut p = Proxy::new(true, Pointer::default());
        game_set_up(&mut p, 0x33);
        let now = Instant::now();
        assert_eq!(p.from_remote(&plugged(), now).to_game.len(), 1);
        p.from_remote(&report(NO_IR), now + Duration::from_millis(10));
        p.from_remote(&plugged(), now + PLUGGED_FOR);
        assert!(!p.motion_plus_off);
    }

    #[test]
    fn stands_aside_when_something_is_plugged_in() {
        let mut p = Proxy::new(true, Pointer::default());
        game_set_up(&mut p, 0x33);
        let now = Instant::now();
        // A moment's contact isn't something plugged in.
        assert!(p.from_remote(&plugged(), now).to_remote.is_empty());
        let out = p.from_remote(&plugged(), now + PLUGGED_FOR);
        assert!(out.to_game.is_empty());
        assert_eq!(out.to_remote.len(), 4);
        assert_eq!(out.to_remote[0][3..6], [0xA4, 0x00, 0xF0]);
        assert_eq!(out.to_remote[1][7], 0x03);
        assert_eq!(out.to_remote[2], [0xA2, 0x13, 0x04]);
        assert_eq!(out.to_remote[3], [0xA2, 0x12, 0x04, 0x33]);
        assert!(p.motion_plus_off);
        // Those writes' answers stay here, then everything goes through.
        for _ in 0..2 {
            assert!(p.from_remote(&[0xA1, 0x22, 0, 0, 0x16, 0], now + PLUGGED_FOR).to_game.is_empty());
        }
        let ack = vec![0xA1, 0x22, 0, 0, 0x16, 0];
        assert_eq!(p.from_remote(&ack, now + PLUGGED_FOR).to_game, std::slice::from_ref(&ack));
        let nunchuk = vec![0xA2, 0x16, 0x04, 0xA4, 0x00, 0xF0, 0x01, 0x55];
        assert_eq!(p.from_game(&nunchuk).to_remote, [nunchuk]);
    }

    #[test]
    fn a_lost_answer_does_not_swallow_the_games() {
        let mut p = Proxy::new(true, Pointer::default());
        game_set_up(&mut p, 0x33);
        let now = Instant::now();
        p.from_remote(&plugged(), now);
        p.from_remote(&plugged(), now + PLUGGED_FOR);
        let ack = vec![0xA1, 0x22, 0, 0, 0x16, 0];
        assert!(p.from_remote(&ack, now + PLUGGED_FOR).to_game.is_empty());
        // The second never came: the game's own, much later, is the game's.
        assert_eq!(p.from_remote(&ack, now + PLUGGED_FOR + MUTED_FOR).to_game, std::slice::from_ref(&ack));
        assert_eq!(p.from_remote(&ack, now + PLUGGED_FOR + MUTED_FOR).to_game, [ack]);
    }

    #[test]
    fn standing_aside_keeps_the_rumble() {
        let mut p = Proxy::new(true, Pointer::default());
        p.from_game(&[0xA2, 0x10, 0x01]);
        let now = Instant::now();
        p.from_remote(&plugged(), now);
        let out = p.from_remote(&plugged(), now + PLUGGED_FOR);
        assert_eq!(out.to_remote.len(), 4);
        assert!(out.to_remote.iter().all(|r| r[2] & 0x01 == 0x01), "{:02X?}", out.to_remote);
    }

    #[test]
    fn a_rumble_change_in_an_answered_report_reaches_the_remote() {
        let mut p = Proxy::new(true, Pointer::default());
        let mut w = protocol::write_register(0xA6_00F0, &[0x55]);
        w[2] |= 0x01;
        assert_eq!(p.from_game(&w).to_remote, [vec![0xA2, 0x10, 0x01]]);
    }
}
