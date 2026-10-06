//! Complex demodulation: the amplitude of one frequency over a sliding Hann window, read at every hop
//! from running sums.
//!
//! The Hann window `w(n) = ½ − ½·cos(2πn/N)` is three complex exponentials, so its windowed sum of
//! `x·e^(−iωn)` is three rectangular sums, at ω and at ω ± 2π/N:
//! `X = S(ω)/2 − e^(−iθa)·S(ω − θ)/4 − e^(iθa)·S(ω + θ)/4`, θ = 2π/N, `a` the window's first sample.
//! A rectangular sum over any stretch is the difference of two running sums, so once the three
//! running sums are kept at every hop boundary, any hop-aligned window's amplitude costs a few
//! multiplications, whatever N. The Hann window's sidelobes are −31 dB and fall 18 dB an octave: a
//! line four bins away (4/N Hz) is about 40 dB down. A rectangular window alone was tried for the
//! splitter: its sidelobes passed the semitone below at about −23 dB, which beat against a quiet note.

/// The running sums of one frequency over a signal, kept at hop boundaries.
#[derive(Clone, Debug)]
pub struct Tracker {
    /// `grid[p][k]`: the sum of `x·e^(−iω_k·n)` over the samples before boundary `p`, for ω, ω − θ
    /// and ω + θ.
    grid: Vec<[(f64, f64); 3]>,
    /// The sample index of boundary 0; negative when the first windows start before the signal.
    origin: i64,
    hop: usize,
    span: usize,
    theta: f64,
}

impl Tracker {
    /// The running sums of `x` at `hz` for windows of `span` hops, at boundaries
    /// `origin + p·hop` for `p` in `0..=count`. Samples outside `x` are silent. `None` for a zero
    /// hop, span or rate.
    #[must_use]
    pub fn new<T: Copy + Into<f64>>(
        x: &[T],
        rate: f64,
        hz: f64,
        origin: i64,
        hop: usize,
        span: usize,
        count: usize,
    ) -> Option<Self> {
        if hop == 0 || span == 0 || rate <= 0.0 {
            return None;
        }
        let len = span * hop;
        let w = std::f64::consts::TAU * hz / rate;
        let theta = std::f64::consts::TAU / len as f64;
        let omegas = [w, w - theta, w + theta];
        let mut grid: Vec<[(f64, f64); 3]> = Vec::with_capacity(count + 1);
        let mut sums = [(0.0f64, 0.0f64); 3];
        let mut n = 0usize;
        for p in 0..=count {
            let boundary = origin + (p * hop) as i64;
            let end = usize::try_from(boundary.max(0)).map_or(x.len(), |b| b.min(x.len()));
            if end > n {
                for (k, &om) in omegas.iter().enumerate() {
                    // The phasor set exactly at each boundary and turned sample by sample after it.
                    let (mut s, mut c) = (om * n as f64).sin_cos();
                    let (ds, dc) = om.sin_cos();
                    let (mut re, mut im) = sums[k];
                    for v in &x[n..end] {
                        let v: f64 = (*v).into();
                        re += v * c;
                        im -= v * s;
                        (c, s) = (c * dc - s * ds, s * dc + c * ds);
                    }
                    sums[k] = (re, im);
                }
                n = end;
            }
            grid.push(sums);
        }
        Some(Self {
            grid,
            origin,
            hop,
            span,
            theta,
        })
    }

    /// How many windows fit: window `p` runs from boundary `p` to boundary `p + span`.
    #[must_use]
    pub fn windows(&self) -> usize {
        self.grid.len().saturating_sub(self.span)
    }

    /// The amplitude of the tracked frequency over window `p`: a steady sine of amplitude `A` at that
    /// frequency reads `A`. `None` past the last window.
    #[must_use]
    pub fn amplitude(&self, p: usize) -> Option<f64> {
        let (a, b) = (self.grid.get(p)?, self.grid.get(p + self.span)?);
        let part = |k: usize| (b[k].0 - a[k].0, b[k].1 - a[k].1);
        let start = (self.origin + (p * self.hop) as i64) as f64;
        let (sin, cos) = (self.theta * start).sin_cos();
        let (s0, s1, s2) = (part(0), part(1), part(2));
        // e^(−iθa)·S(ω − θ) and e^(iθa)·S(ω + θ).
        let lower = (cos * s1.0 + sin * s1.1, cos * s1.1 - sin * s1.0);
        let upper = (cos * s2.0 - sin * s2.1, cos * s2.1 + sin * s2.0);
        let re = 0.5 * s0.0 - 0.25 * (lower.0 + upper.0);
        let im = 0.5 * s0.1 - 0.25 * (lower.1 + upper.1);
        // A sine of amplitude A sums to A/2 times the window's weight, N/2.
        let len = (self.span * self.hop) as f64;
        Some(re.hypot(im) * 4.0 / len)
    }
}
