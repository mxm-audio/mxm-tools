//! The signal pieces every set shares: seeded noise, slow random curves, filters, convolution.

use mxm_rng::SplitMix64;
use std::f64::consts::PI;

pub fn db_to_gain(db: f64) -> f64 {
    10f64.powf(db / 20.0)
}

pub fn gain_to_db(gain: f64) -> f64 {
    20.0 * gain.abs().max(1e-300).log10()
}

/// A seeded generator: uniform and normal draws from SplitMix64.
pub struct Rng(SplitMix64);

impl Rng {
    pub fn new(seed: u64) -> Self {
        Self(SplitMix64::new(seed))
    }

    /// A generator for one named part of a seed, so adding a part never moves another's stream.
    pub fn stream(seed: u64, part: &str) -> Self {
        // FNV-1a over the part's name, mixed with the seed.
        let mut hash = 0xcbf2_9ce4_8422_2325u64;
        for byte in part.bytes() {
            hash = (hash ^ u64::from(byte)).wrapping_mul(0x0100_0000_01b3);
        }
        Self::new(SplitMix64::mix(seed ^ hash))
    }

    /// Uniform in `[0, 1)`, 53 bits.
    pub fn unit(&mut self) -> f64 {
        (self.0.next_u64() >> 11) as f64 / (1u64 << 53) as f64
    }

    pub fn range(&mut self, lo: f64, hi: f64) -> f64 {
        lo + (hi - lo) * self.unit()
    }

    /// One of `items`, uniformly.
    pub fn pick<'a, T>(&mut self, items: &'a [T]) -> &'a T {
        &items[((self.unit() * items.len() as f64) as usize).min(items.len() - 1)]
    }

    /// Standard normal (Box–Muller).
    pub fn normal(&mut self) -> f64 {
        let u = self.unit().max(1e-300);
        let v = self.unit();
        (-2.0 * u.ln()).sqrt() * (2.0 * PI * v).cos()
    }
}

/// White normal noise, `n` samples.
pub fn white(rng: &mut Rng, n: usize) -> Vec<f64> {
    (0..n).map(|_| rng.normal()).collect()
}

/// A slow random curve: normal noise low-passed at `bandwidth` Hz (two cascaded one-poles, run
/// forward and backward so it has no lag), scaled to unit RMS. `rate` is its sample rate.
pub fn slow_noise(rng: &mut Rng, n: usize, rate: f64, bandwidth: f64) -> Vec<f64> {
    if n == 0 {
        return Vec::new();
    }
    let a = (-2.0 * PI * bandwidth / rate).exp();
    // Warm up so the start is as random as the rest.
    let warm = ((rate / bandwidth) * 4.0) as usize;
    let mut x: Vec<f64> = (0..n + 2 * warm).map(|_| rng.normal()).collect();
    for _ in 0..2 {
        let mut y = 0.0;
        for v in x.iter_mut() {
            y = a * y + (1.0 - a) * *v;
            *v = y;
        }
        let mut y = 0.0;
        for v in x.iter_mut().rev() {
            y = a * y + (1.0 - a) * *v;
            *v = y;
        }
    }
    let x = x[warm..warm + n].to_vec();
    let rms = (x.iter().map(|v| v * v).sum::<f64>() / n as f64)
        .sqrt()
        .max(1e-300);
    x.into_iter().map(|v| v / rms).collect()
}

/// A biquad, transposed direct form II, after Bristow-Johnson's *Audio EQ Cookbook*.
#[derive(Clone, Copy, Debug)]
pub struct Biquad {
    b0: f64,
    b1: f64,
    b2: f64,
    a1: f64,
    a2: f64,
    z1: f64,
    z2: f64,
}

impl Biquad {
    fn new(b: [f64; 3], a: [f64; 3]) -> Self {
        Self {
            b0: b[0] / a[0],
            b1: b[1] / a[0],
            b2: b[2] / a[0],
            a1: a[1] / a[0],
            a2: a[2] / a[0],
            z1: 0.0,
            z2: 0.0,
        }
    }

    pub fn lowpass(rate: f64, f: f64, q: f64) -> Self {
        let w = 2.0 * PI * f / rate;
        let (s, c) = w.sin_cos();
        let alpha = s / (2.0 * q);
        Self::new(
            [(1.0 - c) / 2.0, 1.0 - c, (1.0 - c) / 2.0],
            [1.0 + alpha, -2.0 * c, 1.0 - alpha],
        )
    }

    pub fn highpass(rate: f64, f: f64, q: f64) -> Self {
        let w = 2.0 * PI * f / rate;
        let (s, c) = w.sin_cos();
        let alpha = s / (2.0 * q);
        Self::new(
            [(1.0 + c) / 2.0, -(1.0 + c), (1.0 + c) / 2.0],
            [1.0 + alpha, -2.0 * c, 1.0 - alpha],
        )
    }

    /// Band-pass with a peak gain of 0 dB.
    pub fn bandpass(rate: f64, f: f64, q: f64) -> Self {
        let w = 2.0 * PI * f / rate;
        let (s, c) = w.sin_cos();
        let alpha = s / (2.0 * q);
        Self::new([alpha, 0.0, -alpha], [1.0 + alpha, -2.0 * c, 1.0 - alpha])
    }

    /// Peaking EQ: `gain_db` at `f`, bandwidth set by `q`.
    pub fn peaking(rate: f64, f: f64, q: f64, gain_db: f64) -> Self {
        let a = 10f64.powf(gain_db / 40.0);
        let w = 2.0 * PI * f / rate;
        let (s, c) = w.sin_cos();
        let alpha = s / (2.0 * q);
        Self::new(
            [1.0 + alpha * a, -2.0 * c, 1.0 - alpha * a],
            [1.0 + alpha / a, -2.0 * c, 1.0 - alpha / a],
        )
    }

    #[inline]
    pub fn run(&mut self, x: f64) -> f64 {
        let y = self.b0 * x + self.z1;
        self.z1 = self.b1 * x - self.a1 * y + self.z2;
        self.z2 = self.b2 * x - self.a2 * y;
        y
    }

    pub fn run_all(&mut self, x: &mut [f64]) {
        for v in x.iter_mut() {
            *v = self.run(*v);
        }
    }

    /// The filter's gain at `f`.
    pub fn gain_at(&self, rate: f64, f: f64) -> f64 {
        let w = 2.0 * PI * f / rate;
        let z1 = (w.cos(), -w.sin());
        let z2 = ((2.0 * w).cos(), -(2.0 * w).sin());
        let num = (
            self.b0 + self.b1 * z1.0 + self.b2 * z2.0,
            self.b1 * z1.1 + self.b2 * z2.1,
        );
        let den = (
            1.0 + self.a1 * z1.0 + self.a2 * z2.0,
            self.a1 * z1.1 + self.a2 * z2.1,
        );
        (num.0.hypot(num.1)) / (den.0.hypot(den.1))
    }
}

/// `x` filtered forward and then backward through `filter`: zero phase, the magnitude squared.
pub fn zero_phase(x: &[f64], filter: Biquad) -> Vec<f64> {
    let mut y = x.to_vec();
    let mut f = filter;
    f.run_all(&mut y);
    y.reverse();
    let mut f = filter;
    f.run_all(&mut y);
    y.reverse();
    y
}

/// Linear convolution of `x` with `h`, through the kit's FFT.
pub fn convolve(x: &[f64], h: &[f64]) -> Vec<f64> {
    if x.is_empty() || h.is_empty() {
        return Vec::new();
    }
    let out_len = x.len() + h.len() - 1;
    let n = out_len.next_power_of_two();
    let mut planner = mxm_fft::Planner::<f64>::new();
    let fft = planner.real(n);
    let mut a = vec![0.0; n];
    a[..x.len()].copy_from_slice(x);
    let mut b = vec![0.0; n];
    b[..h.len()].copy_from_slice(h);
    let zero = mxm_fft::Complex::new(0.0, 0.0);
    let mut sa = vec![zero; fft.spectrum_len()];
    let mut sb = vec![zero; fft.spectrum_len()];
    let mut scratch = fft.make_scratch();
    fft.forward(&mut a, &mut sa, &mut scratch)
        .expect("planned lengths");
    fft.forward(&mut b, &mut sb, &mut scratch)
        .expect("planned lengths");
    for (p, q) in sa.iter_mut().zip(&sb) {
        *p *= *q;
    }
    let mut out = vec![0.0; n];
    fft.inverse(&mut sa, &mut out, &mut scratch)
        .expect("planned lengths");
    out.truncate(out_len);
    out
}

/// A raised-cosine step from 0 to 1 as `x` goes from 0 to 1 (clamped).
pub fn smoothstep(x: f64) -> f64 {
    let x = x.clamp(0.0, 1.0);
    0.5 - 0.5 * (PI * x).cos()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn convolution_matches_the_direct_sum() {
        let x = [1.0, 2.0, 3.0, 0.5];
        let h = [0.5, -1.0, 0.25];
        let y = convolve(&x, &h);
        let mut direct = vec![0.0; 6];
        for (i, a) in x.iter().enumerate() {
            for (j, b) in h.iter().enumerate() {
                direct[i + j] += a * b;
            }
        }
        for (a, b) in y.iter().zip(&direct) {
            assert!((a - b).abs() < 1e-12);
        }
    }

    #[test]
    fn a_peaking_filter_has_its_gain_at_its_centre() {
        let f = Biquad::peaking(48_000.0, 200.0, 4.0, -20.0);
        assert!((gain_to_db(f.gain_at(48_000.0, 200.0)) + 20.0).abs() < 1e-9);
        assert!(gain_to_db(f.gain_at(48_000.0, 2_000.0)).abs() < 0.2);
    }

    #[test]
    fn slow_noise_is_unit_rms_and_seeded() {
        let a = slow_noise(&mut Rng::new(7), 4800, 1000.0, 2.0);
        let b = slow_noise(&mut Rng::new(7), 4800, 1000.0, 2.0);
        assert_eq!(a, b);
        let rms = (a.iter().map(|v| v * v).sum::<f64>() / a.len() as f64).sqrt();
        assert!((rms - 1.0).abs() < 1e-9);
    }
}
