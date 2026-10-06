//! Adaptive staircases and a simulated listener: how the owner's thresholds are measured (the plan's
//! §5), and the proof that the measuring works before the owner sits a session.
//!
//! **Technique:** the transformed up-down method (Levitt, *Transformed up-down methods in
//! psychoacoustics*, JASA 1971): two correct answers in a row make the next trial harder, one wrong
//! answer makes it easier, which converges on the size answered correctly 70.7 % of the time. The size
//! walks on a log scale; its step halves (in log terms) after the second and the fourth reversal, and the
//! estimate is the geometric mean of the last [`AVERAGED`] reversals. The task is odd-one-out of three
//! (chance one in three), which has no response bias to correct for.
//!
//! The simulated listener answers from a psychometric function, `P = γ + (1 − γ − λ)·W(p)` with a
//! Weibull `W(p) = 1 − exp(−(p/α)^β)`, chance γ = 1/3 and lapses λ; it is built from the size it answers
//! 70.7 % correct at, which is what the staircase must give back.

/// Reversals after which a staircase stops.
pub const REVERSALS: usize = 8;
/// Reversals averaged for the estimate (the last ones; an even number, so as many turns up as down).
pub const AVERAGED: usize = 6;
/// Trials after which a staircase stops whatever it has.
pub const MAX_TRIALS: usize = 60;
/// Chance in a three-interval odd-one-out.
pub const CHANCE: f64 = 1.0 / 3.0;
/// The proportion correct a 1-up-2-down staircase converges on: `√0.5`.
pub const TARGET: f64 = std::f64::consts::FRAC_1_SQRT_2;

/// One 1-up-2-down staircase on a positive size.
#[derive(Clone, Debug, PartialEq)]
pub struct Staircase {
    /// The size of the next trial.
    pub size: f64,
    min: f64,
    max: f64,
    run: u8,
    /// +1 while the size last rose, −1 while it fell, 0 before it has moved.
    direction: i8,
    /// Sizes at which the direction turned.
    pub reversals: Vec<f64>,
    pub trials: usize,
    /// Every trial: its size and whether it was answered correctly.
    pub history: Vec<(f64, bool)>,
}

impl Staircase {
    #[must_use]
    pub fn new(start: f64, min: f64, max: f64) -> Self {
        Self {
            size: start.clamp(min, max),
            min,
            max,
            run: 0,
            direction: 0,
            reversals: Vec::new(),
            trials: 0,
            history: Vec::new(),
        }
    }

    /// The step factor now: ×2 until the second reversal, ×√2 until the fourth, then ×2^¼.
    fn step(&self) -> f64 {
        match self.reversals.len() {
            0 | 1 => 2.0,
            2 | 3 => std::f64::consts::SQRT_2,
            _ => 2f64.powf(0.25),
        }
    }

    /// Records an answer at the current size and moves it.
    pub fn record(&mut self, correct: bool) {
        self.history.push((self.size, correct));
        self.trials += 1;
        let move_to = if correct {
            self.run += 1;
            if self.run < 2 {
                return;
            }
            self.run = 0;
            -1
        } else {
            self.run = 0;
            1
        };
        if self.direction != 0 && move_to != self.direction {
            self.reversals.push(self.size);
        }
        self.direction = move_to;
        let step = self.step();
        self.size = if move_to < 0 {
            self.size / step
        } else {
            self.size * step
        }
        .clamp(self.min, self.max);
    }

    #[must_use]
    pub fn done(&self) -> bool {
        self.reversals.len() >= REVERSALS || self.trials >= MAX_TRIALS
    }

    /// The size answered correctly 70.7 % of the time: the geometric mean of the last [`AVERAGED`]
    /// reversals; `None` before there are that many.
    #[must_use]
    pub fn estimate(&self) -> Option<f64> {
        if self.reversals.len() < AVERAGED {
            return None;
        }
        let last = &self.reversals[self.reversals.len() - AVERAGED..];
        Some((last.iter().map(|v| v.ln()).sum::<f64>() / AVERAGED as f64).exp())
    }
}

/// A deterministic random source (xorshift64*), so a simulated session can be replayed.
#[derive(Clone, Debug)]
pub struct Rng(u64);

impl Rng {
    #[must_use]
    pub fn new(seed: u64) -> Self {
        Self(seed.max(1))
    }

    /// Uniform in [0, 1).
    pub fn uniform(&mut self) -> f64 {
        self.0 ^= self.0 >> 12;
        self.0 ^= self.0 << 25;
        self.0 ^= self.0 >> 27;
        (self.0.wrapping_mul(0x2545_F491_4F6C_DD1D) >> 11) as f64 / (1u64 << 53) as f64
    }

    /// Uniform in `0..n`.
    pub fn below(&mut self, n: usize) -> usize {
        ((self.uniform() * n as f64) as usize).min(n.saturating_sub(1))
    }
}

/// A listener with a known threshold: the size it answers correctly 70.7 % of the time.
#[derive(Clone, Debug)]
pub struct Simulated {
    alpha: f64,
    beta: f64,
    lapse: f64,
    pub rng: Rng,
}

impl Simulated {
    /// A listener whose 70.7 %-correct size is `threshold`, with slope `beta` and lapse rate `lapse`.
    #[must_use]
    pub fn new(threshold: f64, beta: f64, lapse: f64, seed: u64) -> Self {
        let w = (TARGET - CHANCE) / (1.0 - CHANCE - lapse);
        let alpha = threshold / (-(1.0 - w).ln()).powf(1.0 / beta);
        Self {
            alpha,
            beta,
            lapse,
            rng: Rng::new(seed),
        }
    }

    /// The chance of a correct answer at `size`.
    #[must_use]
    pub fn p_correct(&self, size: f64) -> f64 {
        let w = 1.0 - (-(size.max(0.0) / self.alpha).powf(self.beta)).exp();
        CHANCE + (1.0 - CHANCE - self.lapse) * w
    }

    /// One answer at `size`.
    pub fn answer(&mut self, size: f64) -> bool {
        let p = self.p_correct(size);
        self.rng.uniform() < p
    }
}
