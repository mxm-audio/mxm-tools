//! A complex number in `f64`, with the arithmetic the mode analysis needs and nothing else.

use std::ops::{Add, Div, Mul, Neg, Sub};

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Complex {
    pub re: f64,
    pub im: f64,
}

impl Complex {
    pub const ZERO: Self = Self { re: 0.0, im: 0.0 };
    pub const ONE: Self = Self { re: 1.0, im: 0.0 };

    #[must_use]
    pub const fn new(re: f64, im: f64) -> Self {
        Self { re, im }
    }

    /// `r·e^{jθ}`.
    #[must_use]
    pub fn polar(r: f64, theta: f64) -> Self {
        let (sin, cos) = theta.sin_cos();
        Self::new(r * cos, r * sin)
    }

    #[must_use]
    pub fn conj(self) -> Self {
        Self::new(self.re, -self.im)
    }

    /// The modulus, without overflow for large parts.
    #[must_use]
    pub fn abs(self) -> f64 {
        self.re.hypot(self.im)
    }

    #[must_use]
    pub fn norm_sqr(self) -> f64 {
        self.re * self.re + self.im * self.im
    }

    #[must_use]
    pub fn arg(self) -> f64 {
        self.im.atan2(self.re)
    }

    /// The principal natural logarithm.
    #[must_use]
    pub fn ln(self) -> Self {
        Self::new(self.abs().ln(), self.arg())
    }

    #[must_use]
    pub fn scale(self, k: f64) -> Self {
        Self::new(self.re * k, self.im * k)
    }

    #[must_use]
    pub fn is_finite(self) -> bool {
        self.re.is_finite() && self.im.is_finite()
    }
}

impl Add for Complex {
    type Output = Self;
    fn add(self, o: Self) -> Self {
        Self::new(self.re + o.re, self.im + o.im)
    }
}

impl Sub for Complex {
    type Output = Self;
    fn sub(self, o: Self) -> Self {
        Self::new(self.re - o.re, self.im - o.im)
    }
}

impl Mul for Complex {
    type Output = Self;
    fn mul(self, o: Self) -> Self {
        Self::new(
            self.re * o.re - self.im * o.im,
            self.re * o.im + self.im * o.re,
        )
    }
}

impl Div for Complex {
    type Output = Self;
    /// Smith's algorithm, which keeps the intermediate products in range.
    fn div(self, o: Self) -> Self {
        if o.re.abs() >= o.im.abs() {
            let r = o.im / o.re;
            let d = o.re + o.im * r;
            Self::new((self.re + self.im * r) / d, (self.im - self.re * r) / d)
        } else {
            let r = o.re / o.im;
            let d = o.re * r + o.im;
            Self::new((self.re * r + self.im) / d, (self.im * r - self.re) / d)
        }
    }
}

impl Neg for Complex {
    type Output = Self;
    fn neg(self) -> Self {
        Self::new(-self.re, -self.im)
    }
}
