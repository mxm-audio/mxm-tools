//! A small complex number for boundary models, reflection coefficients and filter design.

use std::ops::{Add, Div, Mul, Neg, Sub};

#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct C64 {
    pub re: f64,
    pub im: f64,
}

impl C64 {
    pub const ZERO: Self = Self { re: 0.0, im: 0.0 };
    pub const ONE: Self = Self { re: 1.0, im: 0.0 };

    pub const fn new(re: f64, im: f64) -> Self {
        Self { re, im }
    }

    pub const fn real(re: f64) -> Self {
        Self { re, im: 0.0 }
    }

    pub fn from_polar(magnitude: f64, phase: f64) -> Self {
        Self::new(magnitude * phase.cos(), magnitude * phase.sin())
    }

    pub fn norm_sqr(self) -> f64 {
        self.re * self.re + self.im * self.im
    }

    pub fn abs(self) -> f64 {
        self.re.hypot(self.im)
    }

    pub fn arg(self) -> f64 {
        self.im.atan2(self.re)
    }

    pub fn conj(self) -> Self {
        Self::new(self.re, -self.im)
    }

    pub fn inv(self) -> Self {
        let d = self.norm_sqr();
        Self::new(self.re / d, -self.im / d)
    }

    pub fn exp(self) -> Self {
        Self::from_polar(self.re.exp(), self.im)
    }

    pub fn ln(self) -> Self {
        Self::new(self.abs().ln(), self.arg())
    }

    pub fn powf(self, p: f64) -> Self {
        Self::from_polar(self.abs().powf(p), self.arg() * p)
    }

    /// `coth z`, written for `Re z > 0` so a thick layer does not overflow.
    pub fn coth(self) -> Self {
        let e = (self * -2.0).exp();
        (Self::ONE + e) / (Self::ONE - e)
    }

    pub fn is_finite(self) -> bool {
        self.re.is_finite() && self.im.is_finite()
    }
}

impl Add for C64 {
    type Output = Self;
    fn add(self, o: Self) -> Self {
        Self::new(self.re + o.re, self.im + o.im)
    }
}
impl Sub for C64 {
    type Output = Self;
    fn sub(self, o: Self) -> Self {
        Self::new(self.re - o.re, self.im - o.im)
    }
}
impl Mul for C64 {
    type Output = Self;
    fn mul(self, o: Self) -> Self {
        Self::new(
            self.re * o.re - self.im * o.im,
            self.re * o.im + self.im * o.re,
        )
    }
}
impl Div for C64 {
    type Output = Self;
    #[allow(clippy::suspicious_arithmetic_impl)]
    fn div(self, o: Self) -> Self {
        self * o.inv()
    }
}
impl Mul<f64> for C64 {
    type Output = Self;
    fn mul(self, s: f64) -> Self {
        Self::new(self.re * s, self.im * s)
    }
}
impl Div<f64> for C64 {
    type Output = Self;
    fn div(self, s: f64) -> Self {
        Self::new(self.re / s, self.im / s)
    }
}
impl Neg for C64 {
    type Output = Self;
    fn neg(self) -> Self {
        Self::new(-self.re, -self.im)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn arithmetic_and_coth() {
        let a = C64::new(1.0, 2.0);
        let b = C64::new(-0.5, 0.25);
        let q = (a * b) / b;
        assert!((q - a).abs() < 1e-12);
        assert!((C64::new(0.0, std::f64::consts::PI).exp() + C64::ONE).abs() < 1e-12);
        // coth(x) for real x.
        let x = 0.7f64;
        assert!((C64::real(x).coth().re - x.cosh() / x.sinh()).abs() < 1e-12);
        assert!((C64::real(40.0).coth().re - 1.0).abs() < 1e-12);
    }
}
