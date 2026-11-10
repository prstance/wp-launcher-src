//! One pointer per remote, fed by its camera, accelerometer and MotionPlus; read by
//! the launcher UI (mod.rs) and turned into sensor-bar dots for the game (bridge.rs).
//!
//! Screen units: x -1..1 left to right, y -1..1 top to bottom (beyond: off screen).

use super::protocol::{self, Gyro, MotionPlus, PITCH, YAW};
use std::time::{Duration, Instant};

pub const EMPTY: (u16, u16) = (1023, 1023);
const MIDDLE: (f32, f32) = (512.0, 384.0);
/// Camera pixels the bar's middle moves while the pointer goes from the centre to an
/// edge: Dolphin's emulated remote (±12.5° yaw, ±10° pitch, 23.8 px/° at 42° FOV).
const REACH: (f32, f32) = (305.0, 244.0);
/// The bar seen this far below (bar on top) or above the camera's middle while the
/// pointer is at the screen's centre: Dolphin's 10 cm vertical offset at 2 m.
const BAR_OFFSET_Y: f32 = 68.0;
/// Pixels between the dots: 0.2 m of bar at 2 m in Dolphin's camera (1362 px focal).
pub const SPACING: f32 = 140.0;
const SPACING_RANGE: (f32, f32) = (40.0, 420.0);
/// Most a pair may lean away from what gravity says (KPAD's own check is cos ≥ 0.9).
const MAX_LEAN: f32 = 25.0;
/// Frames a new pair must hold before it counts as the bar.
const ACQUIRE: u8 = 3;
/// Followed this long, the bar is there: from then on it alone points, as on a Wii, and
/// the MotionPlus not at all (the two are never mixed).
const BAR_CONFIRM: Duration = Duration::from_millis(500);
/// Not seen this long, the bar is taken as gone: the MotionPlus points again.
const BAR_GONE: Duration = Duration::from_secs(30);
/// How long one dot keeps a bar going, and how far it may be from where one was.
const ONE_DOT_FOR: Duration = Duration::from_secs(1);
const ONE_DOT_REACH: f32 = 60.0;
/// Degrees to turn across the whole screen, the same px/° both ways (REACH ratio).
const GYRO_SPAN: (f32, f32) = (36.0, 36.0 * REACH.1 / REACH.0);
/// Below about this rate (°/s) turns are damped, hand tremor and residual drift with them.
const DEADZONE: f32 = 0.8;
/// Gravity pulls the gyroscope's vertical angle back within about this time (s).
const PITCH_TAU: f32 = 1.0;
/// Tilt pointer (no gyroscope, no bar): degrees of twist and of pitch to an edge.
const TILT_SPAN: (f32, f32) = (30.0, 25.0);

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct AccelCal {
    pub zero: [f32; 3],
    pub one_g: [f32; 3],
}

impl Default for AccelCal {
    fn default() -> Self {
        AccelCal { zero: [512.0; 3], one_g: [104.0; 3] }
    }
}

impl AccelCal {
    /// The remote's own, 10 bytes at 0x0016: zero x y z, [low bits], 1 g x y z, [low bits].
    pub fn parse(b: &[u8]) -> Option<AccelCal> {
        if b.len() < 8 {
            return None;
        }
        let zero = [0, 1, 2].map(|i| ((b[i] as u16) << 2 | (b[3] as u16 >> (4 - 2 * i)) & 3) as f32);
        let g = [0, 1, 2].map(|i| ((b[4 + i] as u16) << 2 | (b[7] as u16 >> (4 - 2 * i)) & 3) as f32);
        let one_g = [0, 1, 2].map(|i| g[i] - zero[i]);
        one_g.iter().all(|&s| (40.0..=200.0).contains(&s)).then_some(AccelCal { zero, one_g })
    }

    /// From a read of the remote's memory, if it covers the calibration (games read it
    /// on its own, or within a bigger block).
    pub fn from_read(addr: u16, data: &[u8]) -> Option<AccelCal> {
        let at = protocol::ACCEL_CAL.checked_sub(addr)? as usize;
        AccelCal::parse(data.get(at..at + 8)?)
    }
}

/// The accelerometer's 10-bit values from a data report (buttons at 2–3, axes at 4–6).
pub fn accel(report: &[u8]) -> [u16; 3] {
    [
        (report[4] as u16) << 2 | (report[2] as u16 >> 5 & 3),
        (report[5] as u16) << 2 | (report[3] as u16 >> 5 & 1) << 1,
        (report[6] as u16) << 2 | (report[3] as u16 >> 6 & 1) << 1,
    ]
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Source {
    Bar,
    Gyro,
    Tilt,
}

impl Source {
    pub fn name(self) -> &'static str {
        match self {
            Source::Bar => "bar",
            Source::Gyro => "gyro",
            Source::Tilt => "tilt",
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum ForGame {
    /// The camera's own dots: a real bar.
    Real,
    Dots([(u16, u16); 4], u8),
    Nothing,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Seen {
    pub pos: Option<(f32, f32)>,
    pub source: Option<Source>,
    pub game: ForGame,
}

#[derive(Clone, Copy, Debug)]
struct Pair {
    mid: (f32, f32),
    /// Half the vector from the left dot to the right one.
    half: (f32, f32),
    at: Instant,
}

#[derive(Clone, Debug)]
pub struct Pointer {
    pub gyro: Gyro,
    pub accel_cal: AccelCal,
    pub bar_top: bool,
    /// Plain remotes without a bar point by tilting.
    pub tilt: bool,
    twist: f32,
    pitch: Option<f32>,
    pair: Option<Pair>,
    candidate: Option<(Pair, u8)>,
    spacing: f32,
    /// Since when the bar has been followed without a break.
    bar_since: Option<Instant>,
    bar_seen: Option<Instant>,
    /// There is a bar: it alone points.
    bar_mode: bool,
    pos: Option<(f32, f32)>,
    steady_at: Option<(f32, f32)>,
    tilt_zero: Option<f32>,
    last: Option<Instant>,
    /// The last report's gyroscope rates (°/s yaw roll pitch), for the trace.
    rate: Option<[f32; 3]>,
}

impl Default for Pointer {
    fn default() -> Self {
        Pointer {
            gyro: Gyro::default(),
            accel_cal: AccelCal::default(),
            bar_top: true,
            tilt: false,
            twist: 0.0,
            pitch: None,
            pair: None,
            candidate: None,
            spacing: SPACING,
            bar_since: None,
            bar_seen: None,
            bar_mode: false,
            pos: None,
            steady_at: None,
            tilt_zero: None,
            last: None,
            rate: None,
        }
    }
}

fn rotate((x, y): (f32, f32), angle: f32) -> (f32, f32) {
    let (s, c) = angle.sin_cos();
    (x * c - y * s, x * s + y * c)
}

impl Pointer {
    fn bar_y(&self) -> f32 {
        if self.bar_top { BAR_OFFSET_Y } else { -BAR_OFFSET_Y }
    }

    /// Where a real pair puts the pointer, the remote's roll taken out.
    fn pointer_of(&self, mid: (f32, f32), half: (f32, f32)) -> (f32, f32) {
        let (ox, oy) = rotate((mid.0 - MIDDLE.0, mid.1 - MIDDLE.1), -half.1.atan2(half.0));
        (-ox / REACH.0, (oy - self.bar_y()) / REACH.1)
    }

    /// The dots a real bar would show for this pointer, spacing and roll; a dot
    /// outside the camera's view isn't seen (never clamped to its edge).
    pub fn dots_for(&self, (x, y): (f32, f32), spacing: f32, twist: f32) -> [(u16, u16); 4] {
        let mid = (-x * REACH.0, self.bar_y() + y * REACH.1);
        let dot = |side: f32| {
            let (dx, dy) = rotate((mid.0 + side * spacing / 2.0, mid.1), twist);
            let (px, py) = ((MIDDLE.0 + dx).round(), (MIDDLE.1 + dy).round());
            if (0.0..1023.0).contains(&px) && (0.0..768.0).contains(&py) { (px as u16, py as u16) } else { EMPTY }
        };
        [dot(-1.0), dot(1.0), EMPTY, EMPTY]
    }

    /// Point size in extended IR reports, as Dolphin's camera gives it for the distance
    /// the spacing means (2.37·d^-0.778, d = 272/spacing metres).
    fn dot_size(spacing: f32) -> u8 {
        let d = 272.4 / spacing.max(1.0);
        (2.37 * d.powf(-0.778)).round().clamp(1.0, 15.0) as u8
    }

    pub fn dot_size_now(&self) -> u8 {
        Self::dot_size(self.spacing)
    }

    /// The pair among the dots that looks like the bar, if any.
    fn pick(&self, dots: &[(f32, f32)], level: bool, now: Instant) -> Option<Pair> {
        let mut best: Option<(f32, Pair)> = None;
        for i in 0..dots.len() {
            for j in i + 1..dots.len() {
                let (a, b) = (dots[i], dots[j]);
                let mut half = ((b.0 - a.0) / 2.0, (b.1 - a.1) / 2.0);
                if half.0 < 0.0 {
                    half = (-half.0, -half.1);
                }
                let spacing = 2.0 * half.0.hypot(half.1);
                if !(SPACING_RANGE.0..=SPACING_RANGE.1).contains(&spacing) {
                    continue;
                }
                let lean = if level {
                    let d = (half.1.atan2(half.0) - self.twist).to_degrees().rem_euclid(180.0);
                    d.min(180.0 - d)
                } else {
                    0.0
                };
                if lean > MAX_LEAN {
                    continue;
                }
                let score = lean + 30.0 * (spacing / self.spacing).ln().abs();
                let pair = Pair { mid: ((a.0 + b.0) / 2.0, (a.1 + b.1) / 2.0), half, at: now };
                if best.is_none_or(|(s, _)| score < s) {
                    best = Some((score, pair));
                }
            }
        }
        if let Some((_, pair)) = best {
            return Some(pair);
        }
        // One dot: only while a bar was just there, and only near one of its dots.
        let (&[d], Some(last)) = (dots, self.pair) else { return None };
        if now.duration_since(last.at) > ONE_DOT_FOR {
            return None;
        }
        let near = |p: (f32, f32)| (d.0 - p.0).hypot(d.1 - p.1);
        let (left, right) = ((last.mid.0 - last.half.0, last.mid.1 - last.half.1), (last.mid.0 + last.half.0, last.mid.1 + last.half.1));
        let mid = if near(left) <= near(right) {
            (near(left) <= ONE_DOT_REACH).then_some((d.0 + last.half.0, d.1 + last.half.1))
        } else {
            (near(right) <= ONE_DOT_REACH).then_some((d.0 - last.half.0, d.1 - last.half.1))
        }?;
        Some(Pair { mid, half: last.half, at: last.at })
    }

    /// One report's worth: visible dots (camera px), raw accelerometer, MotionPlus sample.
    pub fn update(&mut self, dots: &[(f32, f32)], accel: [u16; 3], motion: Option<&MotionPlus>, now: Instant) -> Seen {
        let seen = self.step(dots, accel, motion, now);
        #[cfg(all(debug_assertions, not(test)))]
        trace::line(self, dots, accel, motion, &seen, now);
        seen
    }

    fn step(&mut self, dots: &[(f32, f32)], accel: [u16; 3], motion: Option<&MotionPlus>, now: Instant) -> Seen {
        let dt = self.last.map_or(0.01, |t| now.saturating_duration_since(t).as_secs_f32().min(0.1));
        self.last = Some(now);

        // Gravity: twist (about the pointing axis) and pitch, when not being shaken.
        let a: [f32; 3] = std::array::from_fn(|i| (accel[i] as f32 - self.accel_cal.zero[i]) / self.accel_cal.one_g[i]);
        let level = ((a[0] * a[0] + a[1] * a[1] + a[2] * a[2]).sqrt() - 1.0).abs() < 0.15;
        let across = a[0].hypot(a[2]);
        if across > 0.5 {
            self.twist = a[0].atan2(a[2]);
        }
        // Raising the nose lowers y (checked against the gyroscope on a real remote).
        let tilt_up = (-a[1]).atan2(across).to_degrees();

        // Real bar.
        let mut pair = self.pick(dots, level, now);
        if self.pair.is_none_or(|p| now.duration_since(p.at) > ONE_DOT_FOR) {
            // Not tracking one: a new pair must hold still-ish for a few frames.
            pair = pair.and_then(|p| {
                let held = match self.candidate {
                    Some((c, n)) if (c.mid.0 - p.mid.0).hypot(c.mid.1 - p.mid.1) < ONE_DOT_REACH => n + 1,
                    _ => 1,
                };
                self.candidate = Some((p, held));
                (held >= ACQUIRE).then_some(p)
            });
            if pair.is_none() && dots.len() < 2 {
                self.candidate = None;
            }
        }
        let bar_pos = pair.map(|p| self.pointer_of(p.mid, p.half));
        let steady = match (bar_pos, self.steady_at) {
            (Some(p), Some(s)) if (p.0 - s.0).abs() < 0.015 && (p.1 - s.1).abs() < 0.015 => true,
            (p, _) => {
                self.steady_at = p;
                false
            }
        };

        // Still learning its rest value while the bar points.
        self.rate = motion.and_then(|m| self.gyro.update(m, steady));

        if let Some(p) = pair {
            let spacing = 2.0 * p.half.0.hypot(p.half.1);
            self.spacing = if self.bar_since.is_none() { spacing } else { self.spacing + (spacing - self.spacing) * 0.2 };
            self.pair = Some(Pair { at: if dots.len() >= 2 { now } else { p.at }, ..p });
            self.bar_seen = Some(now);
            let since = *self.bar_since.get_or_insert(now);
            if !self.bar_mode && now.duration_since(since) >= BAR_CONFIRM {
                self.bar_mode = true;
                eprintln!("wiimote: sensor bar found, it alone points now");
            }
        } else {
            self.bar_since = None;
            if self.pair.is_some_and(|p| now.duration_since(p.at) > ONE_DOT_FOR) {
                self.pair = None;
            }
        }
        if self.bar_mode && self.bar_seen.is_some_and(|t| now.duration_since(t) > BAR_GONE) {
            self.bar_mode = false;
            self.pitch = None;
            eprintln!("wiimote: no sensor bar for {} s, the MotionPlus points again", BAR_GONE.as_secs());
        }
        if self.bar_mode {
            // As on a Wii: the game sees what the camera sees, and no pointer without the bar.
            self.pos = bar_pos;
            return Seen { pos: bar_pos, source: bar_pos.map(|_| Source::Bar), game: ForGame::Real };
        }

        // No bar: the gyroscope, turns about the screen's vertical and horizontal, roll
        // taken out. It turns about the accelerometer's axes: pitch about x, yaw about z,
        // both counted the same way (Dolphin's MotionPlus.cpp, sign_fix -1 +1 -1); held
        // level, + yaw turns right and + pitch raises the nose (checked on a real remote).
        // Twisted by t, gravity is (sin t, ·, cos t) in x and z: a turn about it shows as
        // yaw·cos t + pitch·sin t, one about the horizontal as pitch·cos t − yaw·sin t.
        let turn = self.rate.map(|r| {
            let (s, c) = self.twist.sin_cos();
            let (right, up) = (r[YAW] * c + r[PITCH] * s, r[PITCH] * c - r[YAW] * s);
            let m2 = right * right + up * up;
            let k = m2 / (m2 + DEADZONE * DEADZONE);
            let mut dv = up * k * dt;
            // The vertical angle drifts back to gravity's, unless swinging.
            let pitch = self.pitch.unwrap_or(tilt_up);
            let mut next = pitch + dv;
            if level && up.abs() < 30.0 {
                next += (tilt_up - next) * (dt / PITCH_TAU).min(1.0);
            }
            dv = next - pitch;
            self.pitch = Some(next);
            (right * k * dt * 2.0 / GYRO_SPAN.0, -dv * 2.0 / GYRO_SPAN.1)
        });

        if let Some((dx, dy)) = turn {
            let (x, y) = self.pos.unwrap_or((0.0, 0.0));
            let pos = ((x + dx).clamp(-1.0, 1.0), (y + dy).clamp(-1.0, 1.0));
            self.pos = Some(pos);
            return Seen { pos: Some(pos), source: Some(Source::Gyro), game: ForGame::Dots(self.dots_for(pos, self.spacing, self.twist), Self::dot_size(self.spacing)) };
        }

        if self.tilt && motion.is_none() {
            let zero = *self.tilt_zero.get_or_insert(tilt_up);
            let pos = (
                (self.twist.to_degrees() / TILT_SPAN.0).clamp(-1.0, 1.0),
                (-(tilt_up - zero) / TILT_SPAN.1).clamp(-1.0, 1.0),
            );
            self.pos = Some(pos);
            return Seen { pos: Some(pos), source: Some(Source::Tilt), game: ForGame::Dots(self.dots_for(pos, self.spacing, self.twist), Self::dot_size(self.spacing)) };
        }
        self.tilt_zero = None;
        Seen { pos: None, source: None, game: ForGame::Nothing }
    }
}

/// Development builds: each report's inputs and pointer, one line each, in
/// <temp dir>/wp-wiimote-trace.log (emptied when the launcher starts), to tune on real
/// remotes: `tail -f /tmp/wp-wiimote-trace.log`.
#[cfg(all(debug_assertions, not(test)))]
mod trace {
    use super::{ForGame, MotionPlus, Pointer, Seen};
    use std::fs::File;
    use std::io::{BufWriter, Write};
    use std::sync::Mutex;
    use std::time::Instant;

    static OUT: Mutex<Option<(BufWriter<File>, Instant, u32)>> = Mutex::new(None);

    pub fn line(p: &Pointer, dots: &[(f32, f32)], accel: [u16; 3], motion: Option<&MotionPlus>, seen: &Seen, now: Instant) {
        let Ok(mut out) = OUT.lock() else { return };
        if out.is_none() {
            let Ok(file) = File::create(std::env::temp_dir().join("wp-wiimote-trace.log")) else { return };
            *out = Some((BufWriter::new(file), now, 0));
        }
        let Some((file, start, lines)) = out.as_mut() else { return };
        let mut l = format!("{:9.3} dots={}", now.saturating_duration_since(*start).as_secs_f32(), dots.len());
        for d in dots {
            l += &format!(" ({:.0},{:.0})", d.0, d.1);
        }
        l += &format!(" acc={},{},{} twist={:+.1}", accel[0], accel[1], accel[2], p.twist.to_degrees());
        match motion {
            Some(m) => {
                let mode = |i: usize| if m.slow[i] { 's' } else { 'F' };
                l += &format!(" mp={}{},{}{},{}{}", m.raw[0], mode(0), m.raw[1], mode(1), m.raw[2], mode(2));
            }
            None => l += " mp=-",
        }
        if let Some(r) = p.rate {
            l += &format!(" rate={:+.2},{:+.2},{:+.2}", r[0], r[1], r[2]);
        }
        match p.gyro.rest() {
            (Some(b), sure) => l += &format!(" rest={:.1},{:.1},{:.1}{}", b[0], b[1], b[2], if sure { "T" } else { "H" }),
            (None, _) => l += " rest=-",
        }
        if let Some(pitch) = p.pitch {
            l += &format!(" pitch={pitch:+.1}");
        }
        l += &format!(" mode={} src={}", if p.bar_mode { "bar" } else { "motion" }, seen.source.map_or("-", |s| s.name()));
        if let Some((x, y)) = seen.pos {
            l += &format!(" pos={x:+.3},{y:+.3}");
        }
        match seen.game {
            ForGame::Real => l += " game=real",
            ForGame::Dots(d, size) => l += &format!(" game=({},{})({},{})s{size}", d[0].0, d[0].1, d[1].0, d[1].1),
            ForGame::Nothing => l += " game=-",
        }
        if *lines % 500 == 0 {
            let c = &p.gyro.cal;
            let _ = writeln!(file, "# MotionPlus calibration: rest slow {:?} fast {:?}, units/°/s slow {:?}", c.zero[1], c.zero[0], c.units[1]);
        }
        let _ = writeln!(file, "{l}");
        *lines += 1;
        if *lines % 25 == 0 {
            let _ = file.flush();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use super::super::protocol::motion_plus;

    const LEVEL: [u16; 3] = [512, 512, 616];

    /// Raw [yaw, roll, pitch], all slow.
    fn mp(raw: [u16; 3]) -> MotionPlus {
        let hi = |v: u16| ((v >> 8) as u8) << 2;
        motion_plus(&[raw[0] as u8, raw[1] as u8, raw[2] as u8, hi(raw[0]) | 3, hi(raw[1]) | 2, hi(raw[2]) | 2]).unwrap()
    }

    /// A pointer with its gyroscope calibrated on a table.
    fn calibrated(now: &mut Instant) -> Pointer {
        let mut p = Pointer::default();
        for _ in 0..40 {
            *now += Duration::from_millis(10);
            p.update(&[], LEVEL, Some(&mp([8192; 3])), *now);
        }
        p
    }

    #[test]
    fn dots_and_pointer_go_both_ways() {
        for bar_top in [true, false] {
            let p = Pointer { bar_top, ..Default::default() };
            for pos in [(0.0, 0.0), (0.7, -0.4), (-1.0, 1.0)] {
                for twist in [0.0f32, 0.3, -0.5] {
                    let d = p.dots_for(pos, SPACING, twist);
                    if d[0] == EMPTY || d[1] == EMPTY {
                        continue; // out of the camera's view, as with a real bar
                    }
                    let (a, b) = ((d[0].0 as f32, d[0].1 as f32), (d[1].0 as f32, d[1].1 as f32));
                    let back = p.pointer_of(((a.0 + b.0) / 2.0, (a.1 + b.1) / 2.0), ((b.0 - a.0) / 2.0, (b.1 - a.1) / 2.0));
                    assert!((back.0 - pos.0).abs() < 0.01 && (back.1 - pos.1).abs() < 0.01, "{pos:?} {twist} {back:?}");
                }
            }
        }
        // Centre, bar on top: as the bridge always made it (442, 452), (582, 452).
        assert_eq!(Pointer::default().dots_for((0.0, 0.0), SPACING, 0.0)[..2], [(442, 452), (582, 452)]);
    }

    #[test]
    fn dots_leaving_the_camera_are_dropped_not_clamped() {
        let p = Pointer::default();
        let d = p.dots_for((1.5, 0.0), SPACING, 0.0);
        assert_eq!(d[0], EMPTY);
        assert!(d[1].0 < 1023);
    }

    #[test]
    fn one_stray_dot_is_never_a_bar() {
        let mut now = Instant::now();
        let mut p = Pointer::default();
        for _ in 0..50 {
            now += Duration::from_millis(10);
            let seen = p.update(&[(300.0, 200.0)], LEVEL, None, now);
            assert_eq!(seen.source, None);
        }
    }

    const BAR: [(f32, f32); 2] = [(442.0, 452.0), (582.0, 452.0)];

    /// Followed long enough to be taken as there.
    fn see_bar(p: &mut Pointer, now: &mut Instant, motion: Option<&MotionPlus>) {
        for _ in 0..60 {
            *now += Duration::from_millis(10);
            p.update(&BAR, LEVEL, motion, *now);
        }
    }

    #[test]
    fn a_pair_needs_half_a_second_and_the_right_lean() {
        let mut now = Instant::now();
        let mut p = Pointer::default();
        let seen: Vec<_> = (0..60).map(|_| { now += Duration::from_millis(10); p.update(&BAR, LEVEL, None, now).source }).collect();
        assert!(seen[..48].iter().all(Option::is_none), "{seen:?}");
        assert_eq!(seen[59], Some(Source::Bar));
        // Two lamps one above the other (90° off what gravity says): no bar.
        let mut q = Pointer::default();
        for _ in 0..10 {
            now += Duration::from_millis(10);
            assert_eq!(q.update(&[(500.0, 300.0), (500.0, 450.0)], LEVEL, None, now).source, None);
        }
        // Too close together (a lamp's reflection): no bar.
        for _ in 0..10 {
            now += Duration::from_millis(10);
            assert_eq!(q.update(&[(500.0, 300.0), (520.0, 300.0)], LEVEL, None, now).source, None);
        }
    }

    #[test]
    fn one_dot_carries_a_bar_briefly_and_only_where_a_dot_was() {
        let mut now = Instant::now();
        let mut p = Pointer::default();
        see_bar(&mut p, &mut now, None);
        now += Duration::from_millis(10);
        let seen = p.update(&[(592.0, 452.0)], LEVEL, None, now);
        assert_eq!(seen.source, Some(Source::Bar));
        let x = seen.pos.unwrap().0;
        assert!((x - -10.0 / REACH.0).abs() < 1e-3, "{x}");
        // Somewhere else entirely: not that bar.
        now += Duration::from_millis(10);
        assert_eq!(p.update(&[(100.0, 100.0)], LEVEL, None, now).source, None);
        // Nor after a second alone.
        let mut q = Pointer::default();
        see_bar(&mut q, &mut now, None);
        for _ in 0..150 {
            now += Duration::from_millis(10);
            q.update(&[(592.0, 452.0)], LEVEL, None, now);
        }
        assert_eq!(q.update(&[(592.0, 452.0)], LEVEL, None, now).source, None);
    }

    #[test]
    fn with_a_bar_the_motion_plus_never_points() {
        let mut now = Instant::now();
        let mut p = calibrated(&mut now);
        see_bar(&mut p, &mut now, Some(&mp([8192; 3])));
        now += Duration::from_millis(10);
        let seen = p.update(&BAR, LEVEL, Some(&mp([8192; 3])), now);
        assert_eq!((seen.source, seen.game), (Some(Source::Bar), ForGame::Real));
        // Off the bar, turning right 10°/s for two seconds: no pointer, as on a Wii.
        for _ in 0..200 {
            now += Duration::from_millis(10);
            let seen = p.update(&[], LEVEL, Some(&mp([8192 + 161, 8192, 8192])), now);
            assert_eq!((seen.pos, seen.game), (None, ForGame::Real));
        }
        // The bar back: where it says, as soon as the pair has held a few reports.
        let mut seen = None;
        for _ in 0..ACQUIRE {
            now += Duration::from_millis(10);
            seen = Some(p.update(&BAR, LEVEL, Some(&mp([8192; 3])), now));
        }
        let seen = seen.unwrap();
        assert_eq!(seen.source, Some(Source::Bar));
        let pos = seen.pos.unwrap();
        assert!(pos.0.abs() < 1e-3 && pos.1.abs() < 1e-3, "{pos:?}");
    }

    #[test]
    fn a_pair_seen_briefly_leaves_the_motion_plus_pointing() {
        let mut now = Instant::now();
        let mut p = calibrated(&mut now);
        for _ in 0..20 {
            now += Duration::from_millis(10);
            assert_eq!(p.update(&BAR, LEVEL, Some(&mp([8192; 3])), now).source, Some(Source::Gyro));
        }
        now += Duration::from_millis(10);
        assert_eq!(p.update(&[], LEVEL, Some(&mp([8192; 3])), now).source, Some(Source::Gyro));
    }

    #[test]
    fn a_bar_gone_for_good_hands_back_to_the_motion_plus() {
        let mut now = Instant::now();
        let mut p = calibrated(&mut now);
        see_bar(&mut p, &mut now, Some(&mp([8192; 3])));
        let mut last = None;
        for _ in 0..3100 {
            now += Duration::from_millis(10);
            last = p.update(&[], LEVEL, Some(&mp([8192 + 161, 8192, 8192])), now).source;
        }
        assert_eq!(last, Some(Source::Gyro));
    }

    #[test]
    fn a_rolled_remote_still_turns_right_to_go_right() {
        let mut now = Instant::now();
        let mut p = Pointer::default();
        // Rolled 90°: gravity along x (twist ±90°).
        let rolled = [616, 512, 512];
        for _ in 0..40 {
            now += Duration::from_millis(10);
            p.update(&[], rolled, Some(&mp([8192; 3])), now);
        }
        let start = p.pos.unwrap();
        // The turn shows on the pitch gyroscope now (twist +90°: + pitch is right).
        for _ in 0..20 {
            now += Duration::from_millis(10);
            p.update(&[], rolled, Some(&mp([8192, 8192, 8192 + 161])), now);
        }
        let end = p.pos.unwrap();
        assert!(end.0 - start.0 > 0.05 && (end.1 - start.1).abs() < 0.02, "{start:?} {end:?}");
    }

    /// A MotionPlus sample for an angular velocity (°/s) about the accelerometer's x y z,
    /// as a real one gives it (Dolphin's MotionPlus.cpp: pitch and yaw counted against
    /// that frame's right-hand rule): independent of how the pointer reads it.
    fn mp_for(w: [f32; 3]) -> MotionPlus {
        let raw = |rate: f32| (8192.0 + rate * 4352.0 / 270.0).round() as u16;
        mp([raw(-w[2]), raw(w[1]), raw(-w[0])])
    }

    #[test]
    fn a_remote_held_twisted_goes_where_it_turns() {
        for deg in [-30.0f32, 15.0, 25.0, 45.0] {
            let (s, c) = deg.to_radians().sin_cos();
            // Gravity u = (sin t, 0, cos t); across the remote h = (cos t, 0, -sin t). Turning
            // right spins about −u (+ yaw, level), raising the nose about −h (+ pitch, level:
            // measured on a real remote).
            let held = [(512.0 + 104.0 * s).round() as u16, 512, (512.0 + 104.0 * c).round() as u16];
            let turn = |right: f32, up: f32| mp_for([-right * s - up * c, 0.0, -right * c + up * s]);
            let mut now = Instant::now();
            let mut p = Pointer::default();
            for _ in 0..40 {
                now += Duration::from_millis(10);
                p.update(&[], held, Some(&turn(0.0, 0.0)), now);
            }
            let moved = |p: &mut Pointer, now: &mut Instant, right: f32, up: f32| {
                let a = p.pos.unwrap();
                for _ in 0..30 {
                    *now += Duration::from_millis(10);
                    p.update(&[], held, Some(&turn(right, up)), *now);
                }
                let b = p.pos.unwrap();
                (b.0 - a.0, b.1 - a.1)
            };
            let (dx, dy) = moved(&mut p, &mut now, 20.0, 0.0);
            assert!(dx > 0.25 && dy.abs() < 0.1 * dx, "{deg}° right: {dx} {dy}");
            let (dx, dy) = moved(&mut p, &mut now, 0.0, 20.0);
            assert!(dy < -0.1 && dx.abs() < 0.1 * -dy, "{deg}° up: {dx} {dy}");
        }
    }

    #[test]
    fn raising_the_nose_moves_up_and_gravity_agrees() {
        let mut now = Instant::now();
        let mut p = calibrated(&mut now);
        // The accelerometer of a remote raised by `deg`: y goes down (measured).
        let raised = |deg: f32| {
            let (s, c) = deg.to_radians().sin_cos();
            [512, (512.0 - 104.0 * s).round() as u16, (512.0 + 104.0 * c).round() as u16]
        };
        let start = p.pos.unwrap().1;
        // 10° up in half a second, + pitch (measured).
        for i in 0..50 {
            now += Duration::from_millis(10);
            p.update(&[], raised(i as f32 * 0.2), Some(&mp_for([-20.0, 0.0, 0.0])), now);
        }
        let up = p.pos.unwrap().1;
        assert!(up < start - 0.3, "{start} {up}");
        // Held there: gravity says the same, nothing slides back.
        for _ in 0..300 {
            now += Duration::from_millis(10);
            p.update(&[], raised(10.0), Some(&mp_for([0.0; 3])), now);
        }
        let held = p.pos.unwrap().1;
        assert!((held - up).abs() < 0.05, "{up} {held}");
    }

    #[test]
    fn vertical_drift_is_pulled_back_by_gravity() {
        let mut now = Instant::now();
        let mut p = calibrated(&mut now);
        // Pitch gyroscope 1.5°/s off (warming up), remote level and still: y stays put.
        for _ in 0..1000 {
            now += Duration::from_millis(10);
            p.update(&[], LEVEL, Some(&mp([8192, 8192, 8192 + 24])), now);
        }
        assert!(p.pos.unwrap().1.abs() < 0.12, "{:?}", p.pos);
    }

    #[test]
    fn without_a_bar_the_pointer_stays_on_screen() {
        let mut now = Instant::now();
        let mut p = calibrated(&mut now);
        for _ in 0..300 {
            now += Duration::from_millis(10);
            p.update(&[], LEVEL, Some(&mp([8192 + 1000, 8192, 8192])), now);
        }
        assert_eq!(p.pos.unwrap().0, 1.0);
        // Turning back moves it at once ("lifting the mouse").
        now += Duration::from_millis(10);
        p.update(&[], LEVEL, Some(&mp([8192 - 1000, 8192, 8192])), now);
        assert!(p.pos.unwrap().0 < 1.0);
    }

    #[test]
    fn a_plain_remote_can_point_by_tilting() {
        let mut now = Instant::now();
        let mut p = Pointer { tilt: true, ..Default::default() };
        now += Duration::from_millis(10);
        assert_eq!(p.update(&[], LEVEL, None, now).pos, Some((0.0, 0.0)));
        // Twisted 15° one way: half way to an edge.
        let (s, c) = 15f32.to_radians().sin_cos();
        let twisted = [(512.0 + 104.0 * s) as u16, 512, (512.0 + 104.0 * c) as u16];
        now += Duration::from_millis(10);
        let x = p.update(&[], twisted, None, now).pos.unwrap().0;
        assert!((x.abs() - 0.5).abs() < 0.02, "{x}");
    }

    #[test]
    fn dot_size_follows_distance() {
        assert_eq!(Pointer::dot_size(SPACING), 1);
        assert_eq!(Pointer::dot_size(272.0), 2);
    }

    #[test]
    fn reads_accelerometer_calibration_and_bits() {
        let cal = AccelCal::parse(&[0x80, 0x80, 0x80, 0x00, 0x9A, 0x9A, 0x9A, 0x00]).unwrap();
        assert_eq!(cal.zero, [512.0; 3]);
        assert_eq!(cal.one_g, [104.0; 3]);
        assert!(AccelCal::parse(&[0xFF; 8]).is_none());
        // Within a bigger read, or cut short.
        let block = [[0u8; 6].as_slice(), &[0x80, 0x80, 0x80, 0x00, 0x9A, 0x9A, 0x9A, 0x00, 0, 0]].concat();
        assert_eq!(AccelCal::from_read(0x0010, &block), Some(cal));
        assert_eq!(AccelCal::from_read(0x0016, &block[6..]), Some(cal));
        assert_eq!(AccelCal::from_read(0x0010, &block[..10]), None);
        assert_eq!(AccelCal::from_read(0x0020, &block), None);
        // x low bits in byte 2 bits 5–6, y's bit 1 in byte 3 bit 5, z's in byte 3 bit 6.
        assert_eq!(accel(&[0xA1, 0x37, 0x60, 0x60, 0x80, 0x80, 0x80]), [515, 514, 514]);
    }
}
