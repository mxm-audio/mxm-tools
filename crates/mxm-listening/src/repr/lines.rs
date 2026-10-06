//! Steady lines at known frequencies: the amplitude of a sine at a stated frequency, read through a
//! four-term Blackman–Harris window (Harris, Proc. IEEE 1978: sidelobes at −92 dB), and the mean.
//!
//! Where the frequencies are known — a stimulus's tone, its harmonics, its intermodulation products —
//! a windowed projection reads each one without a spectrum's bin grid: a line off a bin is read at its
//! own frequency, and the window's main lobe (±4 bins of the window's length) keeps a neighbour four
//! bins away out. A one-second window reads lines 4 Hz apart; distortion products 90 dB down are read
//! beside a full-scale tone.

/// A Blackman–Harris window of one length, and its sum.
#[derive(Clone, Debug)]
pub struct Lines {
    w: Vec<f64>,
    sum: f64,
}

impl Lines {
    /// The window for `n` samples.
    #[must_use]
    pub fn new(n: usize) -> Self {
        let a = [0.35875, 0.48829, 0.14128, 0.01168];
        let m = n.max(2) as f64 - 1.0;
        let w: Vec<f64> = (0..n)
            .map(|k| {
                let x = std::f64::consts::TAU * k as f64 / m;
                a[0] - a[1] * x.cos() + a[2] * (2.0 * x).cos() - a[3] * (3.0 * x).cos()
            })
            .collect();
        let sum = w.iter().sum();
        Self { w, sum }
    }

    /// The window's length.
    #[must_use]
    pub fn len(&self) -> usize {
        self.w.len()
    }

    /// Whether the window is empty.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.w.is_empty()
    }

    /// The amplitude and phase (radians, of a cosine at the window's first sample) of the line at
    /// `hz` in `x`, which must be the window's length: a steady sine of amplitude `A` reads `A`.
    /// `None` for a wrong length or an empty window.
    #[must_use]
    pub fn phasor(&self, x: &[f64], rate: f64, hz: f64) -> Option<(f64, f64)> {
        if x.len() != self.w.len() || self.sum <= 0.0 {
            return None;
        }
        let step = std::f64::consts::TAU * hz / rate;
        let (ds, dc) = step.sin_cos();
        let (mut s, mut c) = (0.0f64, 1.0f64);
        let (mut re, mut im) = (0.0, 0.0);
        for (k, (v, w)) in x.iter().zip(&self.w).enumerate() {
            re += v * w * c;
            im -= v * w * s;
            (c, s) = (c * dc - s * ds, s * dc + c * ds);
            // The rotation drifts by rounding; reset it exactly every 4096 samples.
            if k % 4096 == 4095 {
                (s, c) = (step * (k + 1) as f64).sin_cos();
            }
        }
        let scale = 2.0 / self.sum;
        Some((re.hypot(im) * scale, im.atan2(re)))
    }

    /// The amplitude of the line at `hz`; see [`Self::phasor`].
    #[must_use]
    pub fn amplitude(&self, x: &[f64], rate: f64, hz: f64) -> Option<f64> {
        self.phasor(x, rate, hz).map(|p| p.0)
    }

    /// The window-weighted mean of `x`: its DC.
    #[must_use]
    pub fn mean(&self, x: &[f64]) -> Option<f64> {
        (x.len() == self.w.len() && self.sum > 0.0)
            .then(|| x.iter().zip(&self.w).map(|(v, w)| v * w).sum::<f64>() / self.sum)
    }
}
