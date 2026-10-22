//! Minimal `std::complex<double>` replacement covering just the operations
//! ITM needs (keeps the crate dependency-free and allocation-free).

use std::ops::{Add, Div, Mul, Sub};

#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct Complex {
    pub re: f64,
    pub im: f64,
}

impl Complex {
    #[inline(always)]
    pub const fn new(re: f64, im: f64) -> Self {
        Self { re, im }
    }

    /// `std::abs(complex)` - magnitude.
    #[inline(always)]
    pub fn abs(self) -> f64 {
        self.re.hypot(self.im)
    }

    /// `std::sqrt(complex)` - principal square root (branch cut along the
    /// negative real axis, result has non-negative real part).
    pub fn sqrt(self) -> Self {
        let (x, y) = (self.re, self.im);
        if x == 0.0 && y == 0.0 {
            return Self::new(0.0, y);
        }
        let r = self.abs();
        if x >= 0.0 {
            let t = (0.5 * (r + x)).sqrt();
            Self::new(t, y / (2.0 * t))
        } else {
            let t = (0.5 * (r - x)).sqrt();
            Self::new(y.abs() / (2.0 * t), t.copysign(y))
        }
    }
}

impl Add for Complex {
    type Output = Complex;
    #[inline(always)]
    fn add(self, o: Complex) -> Complex {
        Complex::new(self.re + o.re, self.im + o.im)
    }
}

impl Sub for Complex {
    type Output = Complex;
    #[inline(always)]
    fn sub(self, o: Complex) -> Complex {
        Complex::new(self.re - o.re, self.im - o.im)
    }
}

impl Mul for Complex {
    type Output = Complex;
    #[inline(always)]
    fn mul(self, o: Complex) -> Complex {
        Complex::new(self.re * o.re - self.im * o.im, self.re * o.im + self.im * o.re)
    }
}

impl Mul<f64> for Complex {
    type Output = Complex;
    #[inline(always)]
    fn mul(self, s: f64) -> Complex {
        Complex::new(self.re * s, self.im * s)
    }
}

impl Div for Complex {
    type Output = Complex;
    #[inline(always)]
    fn div(self, o: Complex) -> Complex {
        let denom = o.re * o.re + o.im * o.im;
        Complex::new((self.re * o.re + self.im * o.im) / denom, (self.im * o.re - self.re * o.im) / denom)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sqrt_principal() {
        let z = Complex::new(-4.0, 0.0).sqrt();
        assert!((z.re - 0.0).abs() < 1e-15 && (z.im - 2.0).abs() < 1e-15);
        let z = Complex::new(3.0, 4.0).sqrt();
        assert!((z.re - 2.0).abs() < 1e-15 && (z.im - 1.0).abs() < 1e-15);
        let z = Complex::new(3.0, -4.0).sqrt();
        assert!((z.re - 2.0).abs() < 1e-15 && (z.im + 1.0).abs() < 1e-15);
        let w = Complex::new(1.5, -2.5);
        let s = w.sqrt();
        let back = s * s;
        assert!((back.re - w.re).abs() < 1e-14 && (back.im - w.im).abs() < 1e-14);
    }

    #[test]
    fn division() {
        let a = Complex::new(1.0, 2.0);
        let b = Complex::new(3.0, -1.0);
        let q = a / b;
        let back = q * b;
        assert!((back.re - a.re).abs() < 1e-14 && (back.im - a.im).abs() < 1e-14);
    }
}
