//! Lends connected remotes to Dolphin (Source/Core/Core/HW/WiimoteReal/IOBridge.* in
//! the fork): they stay connected to the launcher, in their slots, and their reports
//! go through a local socket.
//!
//! No sensor bar means no pointer for the game, so a remote with a MotionPlus goes
//! through `Proxy`, which makes one from the gyroscope. The game doesn't use the
//! MotionPlus and must not notice it: the remote stays in the report mode that carries
//! both IR and gyroscope data, and the game sees a plain remote in the mode it asked for.

use super::protocol::{self, Aim, Gyro};
use std::time::{Duration, Instant};

/// How long after losing the bar the gyroscope takes over: the player may just be
/// aiming off-screen.
const BAR_MEMORY: Duration = Duration::from_secs(5);
/// Degrees to turn for the whole width and height of the screen.
const GYRO_SPAN_X: f32 = 40.0;
const GYRO_SPAN_Y: f32 = 25.0;
// Where the camera sees the bar when the pointer is at the screen's edges and centre,
// in camera pixels: as Dolphin's emulated remote, at 2 m with a bar on top of the screen.
const REACH_X: f32 = 305.0;
const REACH_Y: f32 = 244.0;
const CENTRE_Y: f32 = 384.0 + 68.0;
const DOTS_APART: f32 = 140.0;
const EMPTY: (u16, u16) = (1023, 1023);
/// Extended IR reports have a size per point, basic ones don't.
const DOT_SIZE: u8 = 2;
/// Answer for an address with no device behind it, as on a remote with no extension.
const NO_DEVICE: u8 = 7;
const IR_TRACKING_ON: u8 = 0x08;
/// The camera's middle, which the bar turns around when the remote is twisted.
const CAMERA_MIDDLE: (f32, f32) = (512.0, 384.0);
/// An accelerometer's zero and the size of 1 g, in 10-bit units, until the remote's own
/// calibration goes by.
const ACCEL_ZERO: i32 = 0x200;
const ACCEL_ONE_G: i32 = 0x68;

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
    ir_on: bool,
    tracking: bool,
    ir_mode: u8,
    rumble: u8,
    buttons: [u8; 2],
    last_report: Vec<u8>,
    /// Our own register writes, whose answers aren't for the game.
    muted_acks: usize,
    aim: Aim,
    gyro: Gyro,
    bar_seen: Option<Instant>,
    /// Pointer from the gyroscope, -1 to 1 from left to right and top to bottom.
    pointer: Option<(f32, f32)>,
    /// How far the bar turns in the camera, from the twist the accelerometer sees, in
    /// radians: games read the twist from the angle between the two dots.
    twist: f32,
    accel_zero: [i32; 3],
    accel_one_g: i32,
    last_sample: Instant,
}

impl Proxy {
    pub fn new(motion_plus: bool, gyro: Gyro) -> Self {
        Proxy {
            hiding: motion_plus,
            motion_plus_off: false,
            mode: 0x30,
            continuous: false,
            ir_on: false,
            tracking: false,
            ir_mode: 0,
            rumble: 0,
            buttons: [0, 0],
            last_report: Vec::new(),
            muted_acks: 0,
            aim: Aim::default(),
            gyro,
            bar_seen: None,
            pointer: None,
            twist: 0.0,
            accel_zero: [ACCEL_ZERO; 3],
            accel_one_g: ACCEL_ONE_G,
            last_sample: Instant::now(),
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

    /// Something the game can't get through this translation is needed (an extension,
    /// full IR data): turns the MotionPlus off and lets the remote speak for itself.
    fn stop_hiding(&mut self, out: &mut Forward) {
        self.hiding = false;
        self.motion_plus_off = true;
        self.muted_acks += 2;
        out.to_remote.push(protocol::write_register(protocol::EXTENSION_INIT[0].0, &[protocol::EXTENSION_INIT[0].1]));
        out.to_remote.push(protocol::write_register(0xB0_0033, &[self.ir_mode]));
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
        self.rumble = report[2] & 0x01;
        let register = report[2] & 0x0C != 0;
        // The extension and MotionPlus addresses, which would disturb the MotionPlus.
        let extension = report.get(3).is_some_and(|&a| a == 0xA4 || a == 0xA6);
        match report[1] {
            0x12 if report.len() >= 4 => {
                self.mode = report[3];
                self.continuous = report[2] & 0x04 != 0;
                if matches!(self.mode, 0x3E | 0x3F) {
                    self.stop_hiding(&mut out);
                } else {
                    out.to_remote.push(self.keep_reporting(report[2] & 0x02));
                }
            }
            // The camera stays on: the game only gets its data while it wants it.
            0x13 | 0x1A => {
                if report[1] == 0x13 {
                    self.ir_on = report[2] & 0x04 != 0;
                }
                out.to_remote.push(vec![protocol::OUT, report[1], report[2] | 0x04]);
            }
            0x16 if register && extension => {
                out.to_game.push(vec![protocol::IN, 0x22, self.buttons[0], self.buttons[1], 0x16, NO_DEVICE]);
            }
            0x16 if register && report.len() >= 7 && report[3] == 0xB0 => {
                let mut write = report.to_vec();
                let start = u16::from_be_bytes([report[4], report[5]]);
                for i in 0..report[6].min(16) as usize {
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
            0x17 if register && extension && report.len() >= 6 => {
                let mut reply = vec![protocol::IN, 0x21, self.buttons[0], self.buttons[1], 0xF0 | NO_DEVICE, report[4], report[5]];
                reply.resize(23, 0);
                out.to_game.push(reply);
            }
            _ => out.to_remote.push(report.to_vec()),
        }
        out
    }

    /// A report from the remote.
    pub fn from_remote(&mut self, report: &[u8], now: Instant) -> Forward {
        let mut out = Forward::default();
        if report.len() < 2 {
            return out;
        }
        if report[1] == 0x22 && self.muted_acks > 0 && report.get(4) == Some(&0x16) {
            self.muted_acks -= 1;
            return out;
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
                if report[1] == 0x21 && report.len() >= 14 && report[5..7] == [0x00, 0x16] && report[4] & 0x0F == 0 {
                    self.calibrate(&report[7..14]);
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
        if sample.as_ref().is_some_and(|m| m.extension) {
            return self.stop_hiding(out);
        }
        let dt = now.duration_since(self.last_sample).as_secs_f32().min(0.05);
        self.last_sample = now;
        let turn = sample.and_then(|m| self.gyro.update(&m, dt));

        let raw = protocol::basic_dots(&report[7..17]);
        let visible: Vec<(f32, f32)> = raw.iter().filter(|d| protocol::dot_visible(d)).map(|d| (d.0 as f32, d.1 as f32)).collect();
        let bar = self.aim.update(&visible).is_some();
        if bar {
            self.bar_seen = Some(now);
            self.pointer = None;
        }
        self.follow_twist(report);
        let gyro_time = !bar && self.gyro.ready() && self.bar_seen.is_none_or(|t| now.duration_since(t) > BAR_MEMORY);
        let dots = if bar {
            raw
        } else if gyro_time {
            self.point(turn.unwrap_or((0.0, 0.0)))
        } else {
            [EMPTY; 4]
        };
        let dots = if self.ir_on && self.tracking { dots } else { [EMPTY; 4] };

        let accel = &report[4..7];
        let mut data = vec![protocol::IN, self.mode, report[2], report[3]];
        let zeros = |n: usize| std::iter::repeat_n(0u8, n);
        match self.mode {
            0x30 => {}
            0x31 => data.extend_from_slice(accel),
            0x32 => data.extend(zeros(8)),
            0x33 => {
                data.extend_from_slice(accel);
                data.extend(dots.iter().flat_map(|&d| extended_dot(d)));
            }
            0x34 => data.extend(zeros(19)),
            0x35 => {
                data.extend_from_slice(accel);
                data.extend(zeros(16));
            }
            0x36 => {
                data.extend(basic_ir(&dots));
                data.extend(zeros(9));
            }
            0x37 => {
                data.extend_from_slice(accel);
                data.extend(basic_ir(&dots));
                data.extend(zeros(6));
            }
            0x3D => data = [protocol::IN, 0x3D].into_iter().chain(zeros(21)).collect(),
            _ => return,
        }
        // Without continuous reporting, a remote only speaks when something changes.
        if self.continuous || data != self.last_report {
            self.last_report.clone_from(&data);
            out.to_game.push(data);
        }
    }

    /// Zero and 1 g of each axis, from the remote's memory at 0x16 (their 8 high bits are
    /// enough here).
    fn calibrate(&mut self, cal: &[u8]) {
        let zero = [0, 1, 2].map(|i| (cal[i] as i32) << 2);
        let size = (0..3).map(|i| ((cal[4 + i] as i32) << 2) - zero[i]).sum::<i32>() / 3;
        if size > 0 {
            self.accel_zero = zero;
            self.accel_one_g = size;
        }
    }

    /// The twist from where gravity pulls, while the remote isn't pointing up or down.
    fn follow_twist(&mut self, report: &[u8]) {
        let x = ((report[4] as i32) << 2 | (report[2] as i32 >> 5 & 3)) - self.accel_zero[0];
        let z = ((report[6] as i32) << 2 | (report[3] as i32 >> 6 & 1) << 1) - self.accel_zero[2];
        let (x, z) = (x as f32, z as f32);
        if (x * x + z * z).sqrt() > self.accel_one_g as f32 * 0.5 {
            self.twist = x.atan2(z);
        }
    }

    /// Follows the turns; pushing past an edge is how you re-centre it, like lifting a mouse.
    fn point(&mut self, (yaw, pitch): (f32, f32)) -> [(u16, u16); 4] {
        let (x, y) = self.pointer.unwrap_or((0.0, 0.0));
        let x = (x + yaw * 2.0 / GYRO_SPAN_X).clamp(-1.0, 1.0);
        let y = (y - pitch * 2.0 / GYRO_SPAN_Y).clamp(-1.0, 1.0);
        self.pointer = Some((x, y));
        // The camera sees the bar move the other way from the pointer, horizontally.
        let (mid_x, mid_y) = (512.0 - x * REACH_X, CENTRE_Y + y * REACH_Y);
        let (sin, cos) = self.twist.sin_cos();
        let dot = |side: f32| {
            let (dx, dy) = (mid_x + side * DOTS_APART / 2.0 - CAMERA_MIDDLE.0, mid_y - CAMERA_MIDDLE.1);
            let x = CAMERA_MIDDLE.0 + dx * cos - dy * sin;
            let y = CAMERA_MIDDLE.1 + dx * sin + dy * cos;
            (x.clamp(0.0, 1022.0) as u16, y.clamp(0.0, 767.0) as u16)
        };
        [dot(-1.0), dot(1.0), EMPTY, EMPTY]
    }
}

fn extended_dot((x, y): (u16, u16)) -> [u8; 3] {
    if x >= EMPTY.0 {
        return [0xFF; 3];
    }
    [x as u8, y as u8, ((y >> 8) << 6 | (x >> 8) << 4) as u8 | DOT_SIZE]
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
        r.extend_from_slice(&[0x00, 0x20, 0x00, 0x83, 0x00, 0x82]);
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

    #[test]
    fn without_motion_plus_everything_goes_through() {
        let mut p = Proxy::new(false, Gyro::default());
        assert_eq!(p.start(), None);
        let setting = vec![0xA2, 0x12, 0x00, 0x33];
        assert_eq!(p.from_game(&setting).to_remote, [setting]);
        let data = vec![0xA1, 0x33, 0, 8, 1, 2, 3];
        assert_eq!(p.from_remote(&data, Instant::now()).to_game, [data]);
    }

    #[test]
    fn keeps_the_remote_in_the_mode_with_gyroscope_data() {
        let mut p = Proxy::new(true, Gyro::default());
        assert_eq!(p.start(), Some(vec![0xA2, 0x12, 0x04, 0x37]));
        assert_eq!(p.from_game(&[0xA2, 0x12, 0x03, 0x33]).to_remote, [vec![0xA2, 0x12, 0x07, 0x37]]);
    }

    #[test]
    fn keeps_the_camera_in_basic_mode() {
        let mut p = Proxy::new(true, Gyro::default());
        let out = p.from_game(&protocol::write_register(0xB0_0033, &[0x03]));
        assert_eq!(out.to_remote[0][7], 0x01);
        // …and on when the game turns it off.
        assert_eq!(p.from_game(&[0xA2, 0x13, 0x00]).to_remote, [vec![0xA2, 0x13, 0x04]]);
    }

    #[test]
    fn hides_the_motion_plus() {
        let mut p = Proxy::new(true, Gyro::default());
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
    fn translates_to_the_mode_the_game_wants() {
        let mut p = Proxy::new(true, Gyro::default());
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
    fn stays_quiet_without_continuous_reporting_until_something_changes() {
        let mut p = Proxy::new(true, Gyro::default());
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
        let mut p = Proxy::new(true, Gyro::default());
        game_set_up(&mut p, 0x33);
        let out = p.from_remote(&report(BAR), Instant::now());
        // x=100 y=200, then x=44 y=88, each with a size; the others empty.
        assert_eq!(out.to_game[0][7..13], [100, 200, DOT_SIZE, 44, 88, DOT_SIZE]);
        assert_eq!(out.to_game[0][13..19], [0xFF; 6]);

        game_set_up(&mut p, 0x37);
        let out = p.from_remote(&report(BAR), Instant::now());
        assert_eq!(out.to_game[0][7..17], BAR);
    }

    #[test]
    fn no_ir_data_while_the_game_has_not_asked_for_it() {
        let mut p = Proxy::new(true, Gyro::default());
        p.from_game(&[0xA2, 0x12, 0x04, 0x33]);
        let out = p.from_remote(&report(BAR), Instant::now());
        assert_eq!(out.to_game[0][7..19], [0xFF; 12]);
    }

    #[test]
    fn without_a_bar_the_gyroscope_points() {
        let mut p = Proxy::new(true, Gyro::default());
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
        let mut p = Proxy::new(true, Gyro::default());
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
        assert!((apart - DOTS_APART).abs() < 2.0, "{apart}");
        // Pointing up or down, gravity says nothing about the twist: it stays.
        let [l, r] = gyro_dots(&mut p, &held(0x80, 0x80), now);
        assert!(l.1 > r.1);
        // Twisted the other way.
        let [left, right] = gyro_dots(&mut p, &held(0x88, 0x98), now);
        assert!(left.1 < right.1);
    }

    #[test]
    fn reads_the_accelerometer_calibration_the_game_asks_for() {
        let mut p = Proxy::new(true, Gyro::default());
        // Zero at 0x84 on each axis, 1 g at 0x9E: 0x68 in 10 bits.
        let mut reply = vec![0xA1, 0x21, 0x00, 0x08, 0x90, 0x00, 0x16, 0x84, 0x84, 0x84, 0x00, 0x9E, 0x9E, 0x9E, 0x00];
        reply.resize(23, 0);
        assert_eq!(p.from_remote(&reply, Instant::now()).to_game, [reply.clone()]);
        assert_eq!(p.accel_zero, [0x210; 3]);
        assert_eq!(p.accel_one_g, 0x68);
        // Another address is something else.
        let mut other = reply.clone();
        other[6] = 0x20;
        let mut q = Proxy::new(true, Gyro::default());
        q.from_remote(&other, Instant::now());
        assert_eq!(q.accel_zero, [ACCEL_ZERO; 3]);
    }

    #[test]
    fn the_real_bar_wins_and_the_gyroscope_waits_after_losing_it() {
        let mut p = Proxy::new(true, Gyro::default());
        game_set_up(&mut p, 0x33);
        let now = calibrated(&mut p, Instant::now());
        p.from_remote(&report(BAR), now);
        let soon = now + Duration::from_secs(1);
        assert_eq!(p.from_remote(&report(NO_IR), soon).to_game[0][7..19], [0xFF; 12]);
        let later = now + BAR_MEMORY + Duration::from_secs(1);
        assert_ne!(p.from_remote(&report(NO_IR), later).to_game[0][7..10], [0xFF; 3]);
    }

    #[test]
    fn stands_aside_when_something_is_plugged_in() {
        let mut p = Proxy::new(true, Gyro::default());
        game_set_up(&mut p, 0x33);
        let mut plugged = report(NO_IR);
        plugged[21] = 0x01;
        let out = p.from_remote(&plugged, Instant::now());
        assert!(out.to_game.is_empty());
        assert_eq!(out.to_remote.len(), 3);
        assert_eq!(out.to_remote[0][3..6], [0xA4, 0x00, 0xF0]);
        assert_eq!(out.to_remote[1][7], 0x03);
        assert_eq!(out.to_remote[2], [0xA2, 0x12, 0x04, 0x33]);
        assert!(p.motion_plus_off);
        // Those writes' answers stay here, then everything goes through.
        for _ in 0..2 {
            assert!(p.from_remote(&[0xA1, 0x22, 0, 0, 0x16, 0], Instant::now()).to_game.is_empty());
        }
        let ack = vec![0xA1, 0x22, 0, 0, 0x16, 0];
        assert_eq!(p.from_remote(&ack, Instant::now()).to_game, [ack.clone()]);
        let nunchuk = vec![0xA2, 0x16, 0x04, 0xA4, 0x00, 0xF0, 0x01, 0x55];
        assert_eq!(p.from_game(&nunchuk).to_remote, [nunchuk]);
    }

    #[test]
    fn full_ir_needs_the_real_remote() {
        let mut p = Proxy::new(true, Gyro::default());
        let out = p.from_game(&[0xA2, 0x12, 0x04, 0x3E]);
        assert_eq!(out.to_remote.last().unwrap(), &vec![0xA2, 0x12, 0x04, 0x3E]);
        assert!(p.motion_plus_off);
    }
}
