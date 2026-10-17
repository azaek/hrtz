//! Many guesses at where the mouse is, weighted by how well each fits the pad's edges.

use super::{Calibration, HELD};

pub(super) const PARTICLES: usize = 300;
/// Pushing this far past an edge (cm) costs nothing: the calibrated edge isn't exact.
const EDGE_SLACK: f64 = 0.5;
/// Beyond the slack, a guess's weight falls by e for every this-many cm it pushes on (squared).
const EDGE_SIGMA: f64 = 1.0;
/// Grip offsets stay within this of the calibrated angle (radians, about 10°).
const MAX_GRIP: f64 = 0.1745;
/// Spread added to a guess's grip offset at each lift (radians, about 1.5°).
const GRIP_STEP: f64 = 0.026;

/// Small seeded generator, so runs (and tests) repeat exactly.
#[derive(Clone)]
pub(super) struct Rng(u64);

impl Rng {
    pub fn new(seed: u64) -> Self {
        Self(seed.max(1))
    }
    pub fn next(&mut self) -> u64 {
        let mut x = self.0;
        x ^= x >> 12;
        x ^= x << 25;
        x ^= x >> 27;
        self.0 = x;
        x.wrapping_mul(0x2545_F491_4F6C_DD1D)
    }
    /// Uniform in [0, 1).
    pub fn unit(&mut self) -> f64 {
        (self.next() >> 11) as f64 / (1u64 << 53) as f64
    }
    pub fn normal(&mut self) -> f64 {
        let u = self.unit().max(1e-12);
        let v = self.unit();
        (-2.0 * u.ln()).sqrt() * (std::f64::consts::TAU * v).cos()
    }
}

#[derive(Clone, Copy)]
pub(super) struct Particle {
    pub x: f64,
    pub y: f64,
    pub w: f64,
    /// This guess's correction to the counts-per-cm scale.
    scale: f64,
    /// This guess's grip angle offset from calibration, with its cosine and sine.
    grip: f64,
    cos: f64,
    sin: f64,
    /// How far it has pushed past an edge since it was last clear of it (cm).
    push: f64,
    /// Which recent pauses this guess treated as lifts, one bit per pause.
    lifts: u32,
    /// Where this guess says each of the held swipes started.
    starts: [(f32, f32); HELD],
}

/// Where a lifted mouse tends to come down.
pub(super) struct Landing {
    pub home: (f64, f64),
    /// Covariance of the put-down spot: xx, xy, yy (cm²).
    pub spread: (f64, f64, f64),
}

pub(super) struct Filter {
    pub p: Vec<Particle>,
    rng: Rng,
}

fn overshoot(v: f64, lo: f64, hi: f64) -> f64 {
    if v < lo {
        v - lo
    } else if v > hi {
        v - hi
    } else {
        0.0
    }
}

impl Filter {
    pub fn new(at: (f64, f64), seed: u64) -> Self {
        let mut rng = Rng::new(seed);
        let p = (0..PARTICLES)
            .map(|_| Particle {
                x: at.0 + rng.normal() * 0.3,
                y: at.1 + rng.normal() * 0.3,
                w: 1.0 / PARTICLES as f64,
                scale: 1.0 + rng.normal() * 0.02,
                grip: 0.0,
                cos: 1.0,
                sin: 0.0,
                push: 0.0,
                lifts: 0,
                starts: [(at.0 as f32, at.1 as f32); HELD],
            })
            .collect();
        Self { p, rng }
    }

    /// Move every guess by one batch of movement (pad cm). Guesses that run past the pad's edges
    /// lose weight. Returns how far the estimate moved because of that, when it happened.
    pub fn motion(&mut self, mx: f64, my: f64, cal: &Calibration) -> Option<f64> {
        let mut pushed = false;
        let (mut old, mut new) = ((0.0, 0.0, 0.0), (0.0, 0.0, 0.0));
        for p in &mut self.p {
            p.x += p.scale * (mx * p.cos - my * p.sin);
            p.y += p.scale * (mx * p.sin + my * p.cos);
            let (ox, oy) = (overshoot(p.x, -cal.left, cal.right), overshoot(p.y, -cal.top, cal.bottom));
            old = (old.0 + p.w * (p.x - ox), old.1 + p.w * (p.y - oy), old.2 + p.w);
            if ox != 0.0 || oy != 0.0 {
                let cost = |push: f64| ((push - EDGE_SLACK).max(0.0) / EDGE_SIGMA).powi(2);
                let before = cost(p.push);
                p.push += ox.hypot(oy);
                p.w *= (before - cost(p.push)).exp();
                p.x -= ox;
                p.y -= oy;
                pushed = true;
            } else {
                p.push = 0.0;
            }
            new = (new.0 + p.w * p.x, new.1 + p.w * p.y, new.2 + p.w);
        }
        if !pushed {
            return None;
        }
        if !(new.2 > 1e-200) {
            // Every guess ran into an edge: the calibration or scale is off. Keep the clamped
            // positions and start the weights over rather than lose the estimate.
            let w = 1.0 / self.p.len() as f64;
            self.p.iter_mut().for_each(|p| p.w = w);
            return None;
        }
        self.normalize();
        if self.effective() < self.p.len() as f64 / 2.0 {
            self.resample();
        }
        let (ax, ay) = (old.0 / old.2, old.1 / old.2);
        let (bx, by) = (new.0 / new.2, new.1 / new.2);
        Some((bx - ax).hypot(by - ay))
    }

    fn normalize(&mut self) {
        let total: f64 = self.p.iter().map(|p| p.w).sum();
        if total > 0.0 {
            self.p.iter_mut().for_each(|p| p.w /= total);
        }
    }

    /// Effective number of guesses: falls as the weight concentrates on a few.
    fn effective(&self) -> f64 {
        1.0 / self.p.iter().map(|p| p.w * p.w).sum::<f64>().max(1e-300)
    }

    /// Redraw the guesses in proportion to their weights (systematic resampling).
    pub fn resample(&mut self) {
        self.normalize();
        let n = self.p.len();
        let step = 1.0 / n as f64;
        let mut target = self.rng.unit() * step;
        let mut cum = 0.0;
        let mut out = Vec::with_capacity(n);
        for p in &self.p {
            cum += p.w;
            while target < cum && out.len() < n {
                out.push(*p);
                target += step;
            }
        }
        while out.len() < n {
            out.push(*self.p.last().unwrap());
        }
        out.iter_mut().for_each(|p| p.w = step);
        self.p = out;
    }

    pub fn mean(&self) -> (f64, f64) {
        self.p.iter().fold((0.0, 0.0), |(x, y), p| (x + p.w * p.x, y + p.w * p.y))
    }

    /// Root of the summed variance in x and y (cm): how unsure the estimate is.
    pub fn spread(&self) -> f64 {
        let (mx, my) = self.mean();
        self.p.iter().map(|p| p.w * ((p.x - mx).powi(2) + (p.y - my).powi(2))).sum::<f64>().sqrt()
    }

    /// Mean grip offset from calibration (radians).
    pub fn grip(&self) -> f64 {
        self.p.iter().map(|p| p.w * p.grip).sum()
    }

    /// At a pause: a `jump` share of the guesses treat it as a lift and come down somewhere new;
    /// the rest stay. `bit` records the choice. Weights should be even (resample first).
    pub fn split(&mut self, jump: f64, bit: u32, dir: (f64, f64), land: &Landing, cal: &Calibration) {
        let mask = 1u32 << bit;
        let (sxx, sxy, syy) = land.spread;
        // Cholesky factor of the put-down spread.
        let l11 = sxx.max(0.01).sqrt();
        let l21 = sxy / l11;
        let l22 = (syy - l21 * l21).max(0.01).sqrt();
        for i in 0..self.p.len() {
            let mut draw = [0.0; 7];
            draw[0] = self.rng.unit();
            draw[1] = self.rng.unit();
            draw[2] = self.rng.normal();
            draw[3] = self.rng.normal();
            draw[4] = self.rng.unit();
            draw[5] = self.rng.normal();
            draw[6] = self.rng.unit();
            let [r, pick, z1, z2, u, g, v] = draw;
            let p = &mut self.p[i];
            p.lifts &= !mask;
            if r >= jump {
                continue;
            }
            p.lifts |= mask;
            let (x, y) = if pick < 0.6 {
                (land.home.0 + l11 * z1, land.home.1 + l21 * z1 + l22 * z2)
            } else if pick < 0.95 {
                // Back against the swipe, from where it ended.
                let d = (8.0 + 4.0 * z1).max(1.0);
                (p.x - dir.0 * d + z2, p.y - dir.1 * d + z2 * 0.5)
            } else {
                (-cal.left + (cal.left + cal.right) * u, -cal.top + (cal.top + cal.bottom) * v)
            };
            p.x = x.clamp(-cal.left, cal.right);
            p.y = y.clamp(-cal.top, cal.bottom);
            p.push = 0.0;
            p.grip = (p.grip + g * GRIP_STEP).clamp(-MAX_GRIP, MAX_GRIP);
            p.cos = p.grip.cos();
            p.sin = p.grip.sin();
        }
    }

    /// Scale the weight of guesses that took the lift branch at `bit` by `ratio`.
    pub fn reweight(&mut self, bit: u32, ratio: f64) {
        let mask = 1u32 << bit;
        for p in &mut self.p {
            if p.lifts & mask != 0 {
                p.w *= ratio;
            }
        }
        self.normalize();
        if self.effective() < self.p.len() as f64 / 2.0 {
            self.resample();
        }
    }

    /// Weighted share of guesses that took the lift branch at `bit`.
    pub fn lift_share(&self, bit: u32) -> f64 {
        let mask = 1u32 << bit;
        self.p.iter().filter(|p| p.lifts & mask != 0).map(|p| p.w).sum()
    }

    pub fn record_start(&mut self, slot: usize) {
        for p in &mut self.p {
            p.starts[slot] = (p.x as f32, p.y as f32);
        }
    }

    /// Where the swipe in `slot` started, and the summed variance of that (cm²).
    pub fn start(&self, slot: usize) -> ((f64, f64), f64) {
        let (mx, my) = self.p.iter().fold((0.0, 0.0), |(x, y), p| (x + p.w * p.starts[slot].0 as f64, y + p.w * p.starts[slot].1 as f64));
        let var = self
            .p
            .iter()
            .map(|p| p.w * ((p.starts[slot].0 as f64 - mx).powi(2) + (p.starts[slot].1 as f64 - my).powi(2)))
            .sum();
        ((mx, my), var)
    }

    /// `n` start points for the swipe in `slot`, drawn in proportion to the guesses' weights
    /// (evenly spaced through the cumulative weight, so the draw doesn't add noise of its own).
    pub fn sample_starts(&self, slot: usize, n: usize) -> Vec<(f64, f64)> {
        let mut out = Vec::with_capacity(n);
        let step = 1.0 / n as f64;
        let mut target = step / 2.0;
        let mut cum = 0.0;
        for p in &self.p {
            cum += p.w;
            while target < cum && out.len() < n {
                out.push((p.starts[slot].0 as f64, p.starts[slot].1 as f64));
                target += step;
            }
        }
        if out.is_empty() {
            out.push(self.start(slot).0);
        }
        out
    }

    /// Chance that the swipe in `slot`, with extents `bbox` (min x, max x, min y, max y relative
    /// to its start), came within `near` cm of each edge: top, bottom, left, right. `reached`
    /// says whether the swipe moved toward that side at all.
    pub fn edge_chances(&self, slot: usize, bbox: [f64; 4], reached: [bool; 4], near: f64, cal: &Calibration) -> [f64; 4] {
        let mut out = [0.0; 4];
        for p in &self.p {
            let (sx, sy) = (p.starts[slot].0 as f64, p.starts[slot].1 as f64);
            let hits = [
                reached[0] && sy + bbox[2] <= -cal.top + near,
                reached[1] && sy + bbox[3] >= cal.bottom - near,
                reached[2] && sx + bbox[0] <= -cal.left + near,
                reached[3] && sx + bbox[1] >= cal.right - near,
            ];
            for (o, h) in out.iter_mut().zip(hits) {
                if h {
                    *o += p.w;
                }
            }
        }
        out
    }
}
