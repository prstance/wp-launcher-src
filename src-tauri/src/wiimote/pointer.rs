//! One pointer per remote, fed by its camera, accelerometer and MotionPlus; read by
//! the launcher UI (mod.rs) and turned into sensor-bar dots for the game (bridge.rs).
//!
//! Screen units: x -1..1 left to right, y -1..1 top to bottom (beyond: off screen).

use super::journal;
use super::protocol::{self, Gyro, MotionPlus, PITCH, ROLL, YAW};
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
/// Spacing of the dots made for the game: KPAD drops a pair it puts nearer than 0.52 m or
/// farther than 1 m plus the Wii's sensor bar sensitivity (3 in Dolphin: 4 m, 67 px), so
/// 0.7–2.7 m, whatever was learned from a bar.
const GAME_SPACING: (f32, f32) = (100.0, 400.0);
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
/// The MotionPlus pointer is Dolphin's (IMUCursor, EmulateIMUCursor in Dynamics.cpp): degrees
/// from the screen's centre to its side edges ("Total Yaw" 25°) and to its top and bottom (the
/// same px/° both ways, REACH ratio: 10°).
const GYRO_EDGE: (f32, f32) = (12.5, 12.5 * REACH.1 / REACH.0);
/// Slower turns about one axis (°/s) are none: hand tremor and what's left of drift
/// (Dolphin's gyroscope dead zone, IMUGyroscope.cpp).
const DEADZONE: f32 = 2.0;
/// How much gravity straightens the gyroscope's pitch and roll every 5 ms (Dolphin's
/// "Accelerometer Influence", 2 % at its 200 Hz): within a quarter of a second.
const ACCEL_WEIGHT: f32 = 0.02;
const ACCEL_EVERY: f32 = 0.005;
/// Reports come in bursts (two 2 ms apart, then nothing for 12–25 ms: testers' journals)
/// although the remote measures evenly: each counts for the average time between them,
/// followed with this weight.
const PERIOD_WEIGHT: f32 = 0.02;
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

/// A rotation (w, x, y, z), as Dolphin's Common::Quaternion.
#[derive(Clone, Copy, Debug, PartialEq)]
struct Quat([f32; 4]);

fn dot(a: [f32; 3], b: [f32; 3]) -> f32 {
    a[0] * b[0] + a[1] * b[1] + a[2] * b[2]
}

fn cross(a: [f32; 3], b: [f32; 3]) -> [f32; 3] {
    [a[1] * b[2] - a[2] * b[1], a[2] * b[0] - a[0] * b[2], a[0] * b[1] - a[1] * b[0]]
}

impl Quat {
    const IDENTITY: Quat = Quat([1.0, 0.0, 0.0, 0.0]);

    /// `angle` radians about the unit `axis`.
    fn rotate(angle: f32, axis: [f32; 3]) -> Quat {
        let (s, c) = (angle / 2.0).sin_cos();
        Quat([c, axis[0] * s, axis[1] * s, axis[2] * s])
    }

    /// |v| radians about v.
    fn turn(v: [f32; 3]) -> Quat {
        let len = dot(v, v).sqrt();
        if len > 0.0 { Quat::rotate(len, v.map(|c| c / len)) } else { Quat::IDENTITY }
    }

    fn then(self, o: Quat) -> Quat {
        let ([aw, ax, ay, az], [bw, bx, by, bz]) = (self.0, o.0);
        Quat([
            aw * bw - ax * bx - ay * by - az * bz,
            aw * bx + ax * bw + ay * bz - az * by,
            aw * by - ax * bz + ay * bw + az * bx,
            aw * bz + ax * by - ay * bx + az * bw,
        ])
    }

    fn inverse(self) -> Quat {
        let [w, x, y, z] = self.0;
        Quat([w, -x, -y, -z])
    }

    fn apply(self, v: [f32; 3]) -> [f32; 3] {
        let [_, x, y, z] = self.then(Quat([0.0, v[0], v[1], v[2]])).then(self.inverse()).0;
        [x, y, z]
    }

    fn normalized(self) -> Quat {
        let n = self.0.iter().map(|c| c * c).sum::<f32>().sqrt();
        Quat(self.0.map(|c| c / n))
    }

    /// Turned `share` of the way from where it puts gravity to where the accelerometer
    /// feels it (`up`, unit): pitch and roll only (Dolphin's ComplementaryFilter).
    fn toward(self, up: [f32; 3], share: f32) -> Quat {
        let seen = self.apply([0.0, 0.0, 1.0]);
        let axis = cross(seen, up);
        let len = dot(axis, axis).sqrt();
        if len < 1e-6 {
            return self;
        }
        Quat::rotate(dot(seen, up).clamp(-1.0, 1.0).acos() * share, axis.map(|c| c / len)).then(self)
    }

    /// Turning right is +, in radians (Dolphin's GetYaw: the remote's back, y, in the world).
    fn yaw(self) -> f32 {
        let back = self.inverse().apply([0.0, 1.0, 0.0]);
        back[0].atan2(back[1])
    }

    /// Raising the nose is − (Dolphin's GetPitch).
    fn pitch(self) -> f32 {
        let up = self.apply([0.0, 0.0, 1.0]);
        up[1].atan2(up[0].hypot(up[2]))
    }

    /// The twist, as the accelerometer's (Dolphin's GetRoll).
    fn roll(self) -> f32 {
        let up = self.apply([0.0, 0.0, 1.0]);
        up[0].atan2(up[2])
    }

    /// Turned about the vertical to that yaw.
    fn with_yaw(self, yaw: f32) -> Quat {
        self.then(Quat::rotate(yaw - self.yaw(), [0.0, 0.0, 1.0]))
    }
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
    /// From gravity alone, unfiltered: for the bar's lean and the tilt pointer.
    twist: f32,
    /// How the world sits around the remote for the MotionPlus pointer (Dolphin's
    /// IMUCursorState): world vectors into the accelerometer's axes, x left, y back, z up.
    orient: Option<Quat>,
    /// Average time between reports (s).
    period: f32,
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
            orient: None,
            period: 0.01,
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

    /// The dots the game gets for a pointer made without the bar.
    fn for_game(&self, pos: (f32, f32), twist: f32) -> ForGame {
        let spacing = self.spacing.clamp(GAME_SPACING.0, GAME_SPACING.1);
        ForGame::Dots(self.dots_for(pos, spacing, twist), Self::dot_size(spacing))
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
        #[cfg(not(test))]
        trace::line(self, dots, accel, motion, &seen);
        seen
    }

    fn step(&mut self, dots: &[(f32, f32)], accel: [u16; 3], motion: Option<&MotionPlus>, now: Instant) -> Seen {
        if let Some(since) = self.last.map(|t| now.saturating_duration_since(t).as_secs_f32()).filter(|&s| s < 0.1) {
            self.period += (since - self.period) * PERIOD_WEIGHT;
        }
        self.last = Some(now);

        // Gravity: twist (about the pointing axis) and pitch, when not being shaken.
        let a: [f32; 3] = std::array::from_fn(|i| (accel[i] as f32 - self.accel_cal.zero[i]) / self.accel_cal.one_g[i]);
        let g = (a[0] * a[0] + a[1] * a[1] + a[2] * a[2]).sqrt();
        let level = (g - 1.0).abs() < 0.15;
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
            self.pair = Some(Pair { at: if dots.len() >= 2 { now } else { p.at }, ..p });
            self.bar_seen = Some(now);
            let since = *self.bar_since.get_or_insert(now);
            if !self.bar_mode && now.duration_since(since) >= BAR_CONFIRM {
                self.bar_mode = true;
                journal::note!("sensor bar found, it alone points now");
            }
            // Only a bar tells how far it is: two lamps paired for an instant while the remote
            // swept past them had made the game's bar too far for KPAD, so no pointer at all.
            if self.bar_mode {
                let spacing = 2.0 * p.half.0.hypot(p.half.1);
                self.spacing += (spacing - self.spacing) * 0.2;
            }
        } else {
            self.bar_since = None;
            if self.pair.is_some_and(|p| now.duration_since(p.at) > ONE_DOT_FOR) {
                self.pair = None;
            }
        }
        if self.bar_mode && self.bar_seen.is_some_and(|t| now.duration_since(t) > BAR_GONE) {
            self.bar_mode = false;
            self.orient = None;
            journal::note!("no sensor bar for {} s, the MotionPlus points again", BAR_GONE.as_secs());
        }
        if self.bar_mode {
            // As on a Wii: the game sees what the camera sees, and no pointer without the bar.
            self.pos = bar_pos;
            return Seen { pos: bar_pos, source: bar_pos.map(|_| Source::Bar), game: ForGame::Real };
        }

        // No bar: the MotionPlus, as Dolphin's IMU pointer. Its rates are turned into ones
        // about the accelerometer's axes (MotionPlus.cpp: the calibration's sign, then pitch
        // and yaw counted against the right-hand rule, sign_fix −1 +1 −1), slow ones dropped;
        // the remote's orientation follows them, gravity straightens its pitch and roll, and
        // a turn past a side edge brings the screen along. Yaw and pitch then point, the roll
        // only turns the game's dots, as a real bar's would.
        let turn = self.rate.map(|r| {
            let dt = self.period;
            let w = [-r[PITCH], r[ROLL], -r[YAW]].map(|v| if v.abs() > DEADZONE { v.to_radians() } else { 0.0 });
            let up = (0.5..1.5).contains(&g).then(|| a.map(|v| v / g));
            let mut q = match self.orient {
                Some(q) => Quat::turn(w.map(|v| -v * dt)).then(q),
                // Facing the screen's centre, tilted as gravity says.
                None => up.map_or(Quat::IDENTITY, |up| Quat::IDENTITY.toward(up, 1.0)).with_yaw(0.0),
            };
            if let Some(up) = up {
                q = q.toward(up, 1.0 - (1.0 - ACCEL_WEIGHT).powf(dt / ACCEL_EVERY));
            }
            let edge = GYRO_EDGE.0.to_radians();
            let yaw = q.yaw();
            if yaw.abs() > edge {
                q = q.with_yaw(yaw.clamp(-edge, edge));
            }
            let q = q.normalized();
            self.orient = Some(q);
            ((q.yaw() / edge).clamp(-1.0, 1.0), q.pitch() / GYRO_EDGE.1.to_radians(), q.roll())
        });

        if let Some((x, y, roll)) = turn {
            let pos = (x, y);
            self.pos = Some(pos);
            return Seen { pos: Some(pos), source: Some(Source::Gyro), game: self.for_game(pos, roll) };
        }

        if self.tilt && motion.is_none() {
            let zero = *self.tilt_zero.get_or_insert(tilt_up);
            let pos = (
                (self.twist.to_degrees() / TILT_SPAN.0).clamp(-1.0, 1.0),
                (-(tilt_up - zero) / TILT_SPAN.1).clamp(-1.0, 1.0),
            );
            self.pos = Some(pos);
            return Seen { pos: Some(pos), source: Some(Source::Tilt), game: self.for_game(pos, self.twist) };
        }
        self.tilt_zero = None;
        Seen { pos: None, source: None, game: ForGame::Nothing }
    }
}

/// Each report's inputs and pointer, one line each, in the remotes' journal (journal.rs),
/// to see on someone else's remote what it gave: `tail -f /tmp/wp-wiimote.log`.
#[cfg(not(test))]
mod trace {
    use super::{ForGame, MotionPlus, Pointer, Seen};
    use crate::wiimote::journal;
    use std::sync::atomic::{AtomicU32, Ordering};

    static LINES: AtomicU32 = AtomicU32::new(0);

    pub fn line(p: &Pointer, dots: &[(f32, f32)], accel: [u16; 3], motion: Option<&MotionPlus>, seen: &Seen) {
        if LINES.fetch_add(1, Ordering::Relaxed) % 500 == 0 {
            let (c, a) = (&p.gyro.cal, &p.accel_cal);
            journal::write(&format!(
                "# calibration: MotionPlus rest slow {:?} fast {:?}, units/°/s slow {:?}; accelerometer zero {:?} 1 g {:?}; tilt pointer {}",
                c.zero[1], c.zero[0], c.units[1], a.zero, a.one_g, p.tilt
            ));
        }
        let mut l = format!("dots={}", dots.len());
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
        if let Some(q) = p.orient {
            l += &format!(" ori={:+.1},{:+.1},{:+.1}", q.yaw().to_degrees(), q.pitch().to_degrees(), q.roll().to_degrees());
        }
        l += &format!(" every={:.1}", p.period * 1000.0);
        l += &format!(" mode={} src={}", if p.bar_mode { "bar" } else { "motion" }, seen.source.map_or("-", |s| s.name()));
        if let Some((x, y)) = seen.pos {
            l += &format!(" pos={x:+.3},{y:+.3}");
        }
        match seen.game {
            ForGame::Real => l += " game=real",
            ForGame::Dots(d, size) => l += &format!(" game=({},{})({},{})s{size}", d[0].0, d[0].1, d[1].0, d[1].1),
            ForGame::Nothing => l += " game=-",
        }
        journal::write(&l);
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

    /// The tester's journal: swept past two lamps 50 px apart, held three frames, the game's
    /// bar had stayed that far ever since (5 m: KPAD shows no pointer).
    #[test]
    fn lamps_swept_past_leave_the_game_bar_where_it_was() {
        let mut now = Instant::now();
        let mut p = calibrated(&mut now);
        for _ in 0..10 {
            now += Duration::from_millis(10);
            p.update(&[(800.0, 300.0), (850.0, 300.0)], LEVEL, Some(&mp([8192; 3])), now);
        }
        now += Duration::from_millis(10);
        let seen = p.update(&[], LEVEL, Some(&mp([8192; 3])), now);
        let ForGame::Dots(d, _) = seen.game else { panic!("{seen:?}") };
        assert_eq!(d[1].0 - d[0].0, SPACING as u16);
        // A bar learned too far for KPAD still gives it one it takes.
        p.spacing = 50.0;
        let ForGame::Dots(d, _) = p.for_game((0.0, 0.0), 0.0) else { unreachable!() };
        assert_eq!(d[1].0 - d[0].0, GAME_SPACING.0 as u16);
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
        // Turning right spins about −x now, which the pitch gyroscope sees.
        for _ in 0..20 {
            now += Duration::from_millis(10);
            p.update(&[], rolled, Some(&mp_for([-10.0, 0.0, 0.0])), now);
        }
        let end = p.pos.unwrap();
        assert!(end.0 - start.0 > 0.05 && (end.1 - start.1).abs() < 0.02, "{start:?} {end:?}");
    }

    /// A MotionPlus sample for an angular velocity (°/s) about the accelerometer's x (left)
    /// y (back) z (up), right-hand rule, as one with Dolphin's calibration gives it
    /// (MotionPlus.cpp: sign_fix −1 +1 −1 against yaw and pitch scales below their zero).
    fn mp_for(w: [f32; 3]) -> MotionPlus {
        let raw = |rate: f32| (8192.0 + rate * 4352.0 / 270.0).round() as u16;
        mp([raw(w[2]), raw(w[1]), raw(w[0])])
    }

    #[test]
    fn a_remote_held_twisted_goes_where_it_turns() {
        for deg in [-30.0f32, 15.0, 25.0, 45.0] {
            let (s, c) = deg.to_radians().sin_cos();
            // Gravity u = (sin t, 0, cos t); across the remote h = (cos t, 0, -sin t). Turning
            // right spins about −u, raising the nose about −h (a lamp in a tester's camera).
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

    /// A tester's external MotionPlus counts yaw and pitch the other way from another remote's:
    /// its calibration says so, as Dolphin reads it, and both point the same.
    #[test]
    fn a_motion_plus_counting_the_other_way_says_so_in_its_calibration() {
        let mut now = Instant::now();
        let (mut usual, mut other) = (Pointer::default(), Pointer::default());
        other.gyro.cal.units = other.gyro.cal.units.map(|u| u.map(f32::abs));
        let flipped = |m: MotionPlus| MotionPlus { raw: [16384 - m.raw[0], m.raw[1], 16384 - m.raw[2]], ..m };
        for _ in 0..40 {
            now += Duration::from_millis(10);
            usual.update(&[], LEVEL, Some(&mp([8192; 3])), now);
            other.update(&[], LEVEL, Some(&flipped(mp([8192; 3]))), now);
        }
        // Right and up at once, then left.
        for w in [[-15.0, 0.0, -20.0], [0.0, 0.0, 30.0]] {
            for _ in 0..30 {
                now += Duration::from_millis(10);
                let a = usual.update(&[], LEVEL, Some(&mp_for(w)), now).pos.unwrap();
                let b = other.update(&[], LEVEL, Some(&flipped(mp_for(w))), now).pos.unwrap();
                assert!((a.0 - b.0).abs() < 1e-3 && (a.1 - b.1).abs() < 1e-3, "{a:?} {b:?}");
            }
        }
        // 6° right and 4.5° up (gravity, which feels the remote level, pulling that back), then 9° left.
        let end = usual.pos.unwrap();
        assert!((end.0 + 3.0 / 12.5).abs() < 0.02 && end.1 < -0.05, "{end:?}");
    }

    /// Reports come two at a time on some machines: the pointer still moves evenly.
    #[test]
    fn reports_in_bursts_move_it_evenly() {
        let mut now = Instant::now();
        let mut p = calibrated(&mut now);
        let mut steps = Vec::new();
        // Fewer than a calibration window: a synthetic turn is steadier than any hand.
        for i in 0..30 {
            now += Duration::from_millis(if i % 2 == 0 { 2 } else { 18 });
            let x = p.pos.unwrap().0;
            p.update(&[], LEVEL, Some(&mp_for([0.0, 0.0, -20.0])), now);
            steps.push(p.pos.unwrap().0 - x);
        }
        let (lo, hi) = steps.iter().fold((f32::MAX, f32::MIN), |(lo, hi), &d| (lo.min(d), hi.max(d)));
        assert!(lo > 0.0 && hi < lo * 1.05, "{steps:?}");
        // As far as 0.3 s at 20°/s takes it.
        assert!((p.pos.unwrap().0 - 6.0 / 12.5).abs() < 0.02, "{:?}", p.pos);
    }

    /// Replays a remote's journal (wp-wiimote.log, from someone's machine) through the pointer:
    /// `WP_JOURNAL=path cargo test replays_a_journal -- --ignored --nocapture`.
    #[test]
    #[ignore]
    fn replays_a_journal() {
        let path = std::env::var("WP_JOURNAL").expect("WP_JOURNAL=path to a wp-wiimote.log");
        let start = Instant::now();
        let mut p = Pointer::default();
        let mut second = -1.0;
        for line in std::fs::read_to_string(path).unwrap().lines() {
            // "# calibration: … units/°/s slow [y, r, p]; accelerometer zero [x, y, z] 1 g [x, y, z]; …"
            let list = |key: &str| -> Option<[f32; 3]> {
                let v: Vec<f32> = line.split(key).nth(1)?.split(['[', ']']).nth(1)?.split(", ").filter_map(|v| v.parse().ok()).collect();
                v.try_into().ok()
            };
            if let (Some(slow), Some(zero), Some(one_g)) = (list("units/°/s slow "), list("accelerometer zero "), list(" 1 g ")) {
                // The journal has them unsigned: the signs Dolphin's and the testers' remotes have.
                p.gyro.cal.units[1] = [-slow[0], slow[1], -slow[2]];
                p.accel_cal = AccelCal { zero, one_g };
                continue;
            }
            let field = |key: &str| line.split(' ').find_map(|w| w.strip_prefix(key));
            let (Some(acc), Some(mp)) = (field("acc="), field("mp=")) else { continue };
            let t: f32 = line.split_whitespace().next().unwrap().parse().unwrap();
            let accel: Vec<u16> = acc.split(',').map(|v| v.parse().unwrap()).collect();
            let motion = (mp != "-").then(|| {
                let v: Vec<&str> = mp.split(',').collect();
                MotionPlus {
                    raw: std::array::from_fn(|i| v[i][..v[i].len() - 1].parse().unwrap()),
                    slow: std::array::from_fn(|i| v[i].ends_with('s')),
                    extension: false,
                }
            });
            let dots: Vec<(f32, f32)> = line
                .split(' ')
                .filter_map(|w| w.strip_prefix('(')?.strip_suffix(')')?.split_once(','))
                .filter_map(|(x, y)| Some((x.parse().ok()?, y.parse().ok()?)))
                .collect();
            let seen = p.update(&dots, [accel[0], accel[1], accel[2]], motion.as_ref(), start + Duration::from_secs_f32(t));
            if t.floor() != second {
                second = t.floor();
                let ori = p.orient.map(|q| [q.yaw(), q.pitch(), q.roll()].map(f32::to_degrees));
                let a: [f32; 3] = std::array::from_fn(|i| (accel[i] as f32 - p.accel_cal.zero[i]) / p.accel_cal.one_g[i]);
                let gravity = [a[1].atan2(a[0].hypot(a[2])), a[0].atan2(a[2])].map(f32::to_degrees);
                println!("{t:9.3} {:?} {:?} ori {ori:.1?} gravity {gravity:.1?} every {:.1} ms", seen.source, seen.pos, p.period * 1000.0);
            }
        }
        println!("end: spacing {}", p.spacing);
    }

    #[test]
    fn vertical_drift_is_pulled_back_by_gravity() {
        let mut now = Instant::now();
        let mut p = calibrated(&mut now);
        // Pitch gyroscope 3°/s off (warming up), remote level and still: y stays put.
        for _ in 0..1000 {
            now += Duration::from_millis(10);
            p.update(&[], LEVEL, Some(&mp([8192, 8192, 8192 + 48])), now);
        }
        assert!(p.pos.unwrap().1.abs() < 0.1, "{:?}", p.pos);
    }

    #[test]
    fn without_a_bar_the_pointer_stays_on_screen() {
        let mut now = Instant::now();
        let mut p = calibrated(&mut now);
        for _ in 0..300 {
            now += Duration::from_millis(10);
            p.update(&[], LEVEL, Some(&mp_for([0.0, 0.0, -60.0])), now);
        }
        assert!((p.pos.unwrap().0 - 1.0).abs() < 1e-4, "{:?}", p.pos);
        // Turning back moves it at once ("lifting the mouse").
        now += Duration::from_millis(10);
        p.update(&[], LEVEL, Some(&mp_for([0.0, 0.0, 60.0])), now);
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
