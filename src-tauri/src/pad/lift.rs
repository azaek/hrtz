//! How likely a pause was a lift, from how the swipe ended and how the next one began.
//!
//! A logistic score over a few inputs. It starts from hand-set weights and learns from use, with
//! nothing asked of the user: the tracker settles each pause's lift chance from everything that
//! happened after it (the pad's edges above all), and the score is fitted to those settled
//! chances. Pause length is split into ranges, each with its own weight, so the score picks up how
//! long this user's lifts and stops really take.

use std::time::Duration;

use super::segment::Pause;

pub(super) const FEATURES: usize = 13;
/// Inputs known when the pause ends: 0..=8. Inputs from the start of the next swipe: 9..=12.
const PRE: usize = 9;

/// bias; pause 40-150 ms, 150-600 ms, 600-1500 ms, over 1.5 s; cut-off; reverse blip before;
/// swipe length; room left; reverse blip after; mid-swipe dropout; same way toward a near edge;
/// heading back.
///
/// A pause with nothing else to say reads as about one lift in eight: players stop far more often
/// than they lift, and the edges settle the rest. Running low on room only says much once the next
/// swipe carries on the same way, which is what lifting to make room is for.
pub(super) const START_WEIGHTS: [f64; FEATURES] = [-2.0, -1.0, 0.0, -0.8, -2.0, 3.0, 1.0, 0.2, 1.0, 1.0, -4.0, 2.0, -1.0];
const WEIGHT_LIMIT: f64 = 6.0;
const LEARN_RATE: f64 = 0.05;
/// Share of a refit blended into the weights, its step size, and how hard it's pulled back toward
/// the starting weights.
const REFIT_BLEND: f64 = 0.5;
const REFIT_STEP: f64 = 0.5;
const REFIT_PULL: f64 = 0.01;

pub(super) type Inputs = [f64; FEATURES];

/// Which pause-length range a pause falls in: lifts take a few hundred milliseconds, hesitations
/// less, rests more.
pub(super) fn gap_range(gap: Duration) -> usize {
    match gap.as_millis() {
        0..150 => 0,
        150..600 => 1,
        600..1500 => 2,
        _ => 3,
    }
}

/// `room` is how close the swipe was to running out of pad: 1 at the edge, 0 with 12 cm left.
pub(super) fn pre_inputs(p: &Pause, room: f64) -> Inputs {
    let mut x = [0.0; FEATURES];
    x[0] = 1.0;
    x[1 + gap_range(p.gap)] = 1.0;
    x[5] = p.cut.clamp(0.0, 1.2);
    x[6] = (p.reverse * 4.0).clamp(0.0, 2.0);
    x[7] = (p.length / 10.0).clamp(0.0, 2.0);
    x[8] = room.clamp(-1.0, 1.0);
    x
}

/// What the start of the next swipe adds.
pub(super) struct After {
    /// Movement back against the previous swipe in the first moments (cm): landing while still
    /// moving the mouse back.
    pub reverse: f64,
    /// Same speed and direction straight after a short gap: the sensor dropped out mid-swipe.
    pub dropout: bool,
    /// Cosine between the previous swipe's direction and the new one's.
    pub same_way: f64,
}

pub(super) fn add_after(x: &mut Inputs, a: &After) {
    x[9] = (a.reverse * 4.0).clamp(0.0, 2.0);
    x[10] = if a.dropout { 1.0 } else { 0.0 };
    // Carrying on the same way only says "lift" when that way was running out of room.
    x[11] = (a.same_way.max(0.0) * x[8].max(0.0)).min(1.2);
    // Going back the way it came needs no lift: a flick and the flick back.
    x[12] = (-a.same_way).max(0.0);
}

pub(super) fn probability(w: &[f64], x: &Inputs) -> f64 {
    let z: f64 = w.iter().zip(x).map(|(w, x)| w * x).sum();
    1.0 / (1.0 + (-z).exp())
}

/// Probability from the inputs known when the pause ends.
pub(super) fn prior(w: &[f64], x: &Inputs) -> f64 {
    let mut pre = *x;
    pre[PRE..].iter_mut().for_each(|v| *v = 0.0);
    probability(w, &pre)
}

/// One step toward the lift chance the tracker settled on, sized by how far the evidence moved it
/// from what the score expected.
pub(super) fn learn(w: &mut [f64], x: &Inputs, settled: f64, expected: f64) {
    let err = settled - probability(w, x);
    let trust = (settled - expected).abs();
    for (w, x) in w.iter_mut().zip(x) {
        *w = (*w + LEARN_RATE * trust * err * x).clamp(-WEIGHT_LIMIT, WEIGHT_LIMIT);
    }
}

/// Refit the weights to the lift chances the tracker settled on for recorded pauses. Those are
/// posteriors: the score's own guess corrected by everything that happened after, edges included,
/// so fitting them is the learning half of expectation–maximisation. The fit is pulled toward the
/// starting weights, so a stretch with little evidence can't drag the model off.
pub(super) fn refit(w: &mut [f64], xs: &[Vec<f32>], ps: &[f32]) {
    let n = xs.len().min(ps.len());
    if n < 100 {
        return;
    }
    let xs: Vec<Inputs> = xs[..n].iter().map(|x| std::array::from_fn(|j| x.get(j).map_or(0.0, |v| *v as f64))).collect();
    let mut fit = w.to_vec();
    for _ in 0..300 {
        let mut grad = [0.0; FEATURES];
        for (x, &p) in xs.iter().zip(ps) {
            let err = probability(&fit, x) - p as f64;
            for (g, xj) in grad.iter_mut().zip(x) {
                *g += err * xj;
            }
        }
        for (j, f) in fit.iter_mut().enumerate() {
            let pull = REFIT_PULL * (*f - START_WEIGHTS[j]);
            *f = (*f - REFIT_STEP * (grad[j] / n as f64 + pull)).clamp(-WEIGHT_LIMIT, WEIGHT_LIMIT);
        }
    }
    for (w, f) in w.iter_mut().zip(fit) {
        *w = (1.0 - REFIT_BLEND) * *w + REFIT_BLEND * f;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pause_lengths_fall_in_ranges() {
        assert_eq!(gap_range(Duration::from_millis(60)), 0);
        assert_eq!(gap_range(Duration::from_millis(300)), 1);
        assert_eq!(gap_range(Duration::from_millis(1000)), 2);
        assert_eq!(gap_range(Duration::from_secs(5)), 3);
    }

    #[test]
    fn learning_moves_toward_the_label() {
        let mut w = START_WEIGHTS.to_vec();
        let x: Inputs = [1.0, 0.0, 1.0, 0.0, 0.0, 0.1, 0.0, 0.5, 0.0, 0.0, 0.0, 0.0, 0.0];
        let before = probability(&w, &x);
        for _ in 0..50 {
            learn(&mut w, &x, 1.0, 0.0);
        }
        assert!(probability(&w, &x) > before + 0.2);
    }

    #[test]
    fn refit_follows_settled_chances() {
        // Lifts: long pauses near the edge. Stops: short pauses with room to spare.
        let mut xs = Vec::new();
        let mut ps = Vec::new();
        for i in 0..400 {
            let lift = i % 2 == 0;
            let jitter = (i % 7) as f32 * 0.02;
            xs.push(if lift {
                vec![1.0, 0.0, 1.0, 0.0, 0.0, 0.2 + jitter, 0.0, 1.0, 0.8 - jitter, 0.0, 0.0, 0.6, 0.0]
            } else {
                vec![1.0, 0.0, 0.0, 1.0, 0.0, 0.05, 0.0, 0.3, -0.5 + jitter, 0.0, 0.0, 0.0, 1.0]
            });
            ps.push(if lift { 0.95 } else { 0.05 });
        }
        let mut w = START_WEIGHTS.to_vec();
        refit(&mut w, &xs, &ps);
        let lift: Inputs = [1.0, 0.0, 1.0, 0.0, 0.0, 0.2, 0.0, 1.0, 0.8, 0.0, 0.0, 0.6, 0.0];
        let stop: Inputs = [1.0, 0.0, 0.0, 1.0, 0.0, 0.05, 0.0, 0.3, -0.5, 0.0, 0.0, 0.0, 1.0];
        assert!(probability(&w, &lift) > 0.8, "{}", probability(&w, &lift));
        assert!(probability(&w, &stop) < 0.2, "{}", probability(&w, &stop));
    }
}
