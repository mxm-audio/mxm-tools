//! Air: atmospheric absorption after ISO 9613-1:1993, and the speed of sound.
//!
//! The absorption formula is the standard's eqs. (3)–(5), §6.2, read from its free preview
//! (`research:effects/room-acoustics-simulation.md` §3). It is an outdoor standard applied indoors
//! by convention, and it is accurate to ±10 % within clause 7's ranges.
//!
//! **Not read in a primary source, and so marked:**
//! - The conversion from relative humidity to molar concentration of water vapour is the
//!   standard's Annex B, which the preview omits. The expression here (`psat/pr = 10^C`, with
//!   `C = −6.8346·(T01/T)^1.261 + 4.6151` and `T01 = 273.16 K`) is as reproduced by secondary
//!   pages that cite ISO 9613-1.
//! - The speed of sound is the ideal-gas approximation `c = 331.3·sqrt(1 + θ/273.15)` m/s, a
//!   chosen textbook form.
//! - No tabulated ISO value was available to check against. The tests check the formula's
//!   structure and a loose magnitude only.

use crate::bands::{self, Bands, NUM_BANDS};

/// Reference pressure, kPa (ISO 9613-1 §6.2).
const P_REF_KPA: f64 = 101.325;
/// Reference temperature, K (ISO 9613-1 §6.2).
const T0_K: f64 = 293.15;
/// Triple-point temperature used by the humidity conversion, K.
const T01_K: f64 = 273.16;

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Air {
    pub temperature_c: f64,
    pub relative_humidity_pct: f64,
    pub pressure_kpa: f64,
}

impl Default for Air {
    fn default() -> Self {
        Self::standard()
    }
}

impl Air {
    /// 20 °C, 50 % relative humidity, 101.325 kPa.
    pub const fn standard() -> Self {
        Self {
            temperature_c: 20.0,
            relative_humidity_pct: 50.0,
            pressure_kpa: P_REF_KPA,
        }
    }

    fn temperature_k(&self) -> f64 {
        self.temperature_c + 273.15
    }

    /// Speed of sound in m/s (ideal-gas approximation; see the module note).
    pub fn speed_of_sound(&self) -> f64 {
        331.3 * (1.0 + self.temperature_c / 273.15).sqrt()
    }

    /// Density of dry air at this temperature and pressure, kg/m³: the ideal-gas law with the
    /// specific gas constant of dry air, 287.05 J/(kg·K). **Chosen:** humidity's small effect on
    /// density is ignored.
    pub fn density(&self) -> f64 {
        self.pressure_kpa * 1000.0 / (287.05 * self.temperature_k())
    }

    /// Characteristic impedance of air, `ρc`, Pa·s/m.
    pub fn characteristic_impedance(&self) -> f64 {
        self.density() * self.speed_of_sound()
    }

    /// Molar concentration of water vapour, in percent (the `h` of ISO 9613-1 eqs. 3–4).
    pub fn molar_concentration_water_pct(&self) -> f64 {
        let c = -6.8346 * (T01_K / self.temperature_k()).powf(1.261) + 4.6151;
        let psat_over_pref = 10f64.powf(c);
        self.relative_humidity_pct * psat_over_pref / (self.pressure_kpa / P_REF_KPA)
    }

    /// Pure-tone attenuation coefficient, dB per metre (ISO 9613-1 eq. 5).
    pub fn attenuation_db_per_m(&self, frequency_hz: f64) -> f64 {
        let t = self.temperature_k();
        let pa = self.pressure_kpa / P_REF_KPA;
        let h = self.molar_concentration_water_pct();
        let tr = t / T0_K;
        // Relaxation frequencies of oxygen and nitrogen, eqs. (3) and (4).
        let fr_o = pa * (24.0 + 4.04e4 * h * (0.02 + h) / (0.391 + h));
        let fr_n =
            pa * tr.powf(-0.5) * (9.0 + 280.0 * h * (-4.170 * (tr.powf(-1.0 / 3.0) - 1.0)).exp());
        let f2 = frequency_hz * frequency_hz;
        8.686
            * f2
            * (1.84e-11 / pa * tr.sqrt()
                + tr.powf(-2.5)
                    * (0.01275 * (-2239.1 / t).exp() / (fr_o + f2 / fr_o)
                        + 0.1068 * (-3352.0 / t).exp() / (fr_n + f2 / fr_n)))
    }

    /// Pressure gain after `distance_m` of air, per band, evaluated at each band's exact centre.
    /// **Chosen:** centre-frequency evaluation; the standard's band method (clause 8) was not read.
    pub fn band_pressure_gain(&self, distance_m: f64) -> Bands {
        let mut out = [0.0; NUM_BANDS];
        for (k, g) in out.iter_mut().enumerate() {
            let db = self.attenuation_db_per_m(bands::exact_centre_hz(k)) * distance_m;
            *g = 10f64.powf(-db / 20.0);
        }
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn speed_of_sound_near_343_at_20c() {
        let c = Air::standard().speed_of_sound();
        assert!((c - 343.2).abs() < 0.3, "{c}");
    }

    #[test]
    fn humidity_conversion_magnitude() {
        // Saturation vapour pressure at 20 °C is about 2.34 kPa, so 50 % RH is about 1.15 % molar.
        let h = Air::standard().molar_concentration_water_pct();
        assert!((1.0..1.3).contains(&h), "{h}");
    }

    #[test]
    fn attenuation_structure() {
        let air = Air::standard();
        let mut previous = 0.0;
        for f in [
            63.0, 125.0, 250.0, 500.0, 1000.0, 2000.0, 4000.0, 8000.0, 16000.0,
        ] {
            let a = air.attenuation_db_per_m(f);
            assert!(a.is_finite() && a > previous, "{f} Hz: {a}");
            previous = a;
        }
        // Loose magnitude only (see the module note): a few dB per km at 1 kHz, tens at 4 kHz.
        let a1k = air.attenuation_db_per_m(1000.0) * 1000.0;
        let a4k = air.attenuation_db_per_m(4000.0) * 1000.0;
        assert!((2.0..10.0).contains(&a1k), "1 kHz {a1k} dB/km");
        assert!((15.0..60.0).contains(&a4k), "4 kHz {a4k} dB/km");
        // Well above both relaxation frequencies, the classical term dominates and grows as f².
        let hi = air.attenuation_db_per_m(1.0e6) / air.attenuation_db_per_m(5.0e5);
        assert!((hi - 4.0).abs() < 0.2, "{hi}");
    }

    #[test]
    fn band_gain_is_one_at_zero_distance() {
        assert_eq!(Air::standard().band_pressure_gain(0.0), [1.0; NUM_BANDS]);
    }
}
