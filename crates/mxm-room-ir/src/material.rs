//! Surface materials: per-band random-incidence absorption and scattering.
//!
//! The coefficients are energy quantities: absorption is the absorbed fraction of incident energy,
//! and scattering (ISO 17497-1:2004 §3.3) is one minus the specular fraction of the reflected
//! energy. So the specular pressure reflection a specular path picks up at a wall is
//! `sqrt((1 − α)(1 − s))`. Values come from published tables with their provenance in
//! `research:effects/room-acoustics-simulation.md` §8.1. A random-incidence α above 1 is a
//! finite-sample measurement artefact and is rejected here, never clipped.

use crate::bands::{self, Bands, NUM_BANDS};
use crate::boundary::Boundary;
use crate::error::Error;

#[derive(Debug, Clone, PartialEq)]
pub struct Material {
    pub name: String,
    pub absorption: Bands,
    pub scattering: Bands,
    /// The boundary model a wave-solved scene uses (plan §4.1). `None` means one is fitted to the
    /// band absorption when needed, and labelled fitted.
    pub boundary: Option<Boundary>,
}

impl Material {
    /// A material from full nine-band data. Every coefficient must be finite and in 0..=1.
    pub fn new(
        name: impl Into<String>,
        absorption: Bands,
        scattering: Bands,
    ) -> Result<Self, Error> {
        let name = name.into();
        for (label, values) in [("absorption", &absorption), ("scattering", &scattering)] {
            for (k, &v) in values.iter().enumerate() {
                if !v.is_finite() || !(0.0..=1.0).contains(&v) {
                    return Err(Error::InvalidMaterial(format!(
                        "{name}: {label} in band {k} is {v}, outside 0..=1"
                    )));
                }
            }
        }
        Ok(Self {
            name,
            absorption,
            scattering,
            boundary: None,
        })
    }

    /// The same material with an explicit boundary model.
    pub fn with_boundary(mut self, boundary: Boundary) -> Result<Self, Error> {
        if !boundary.is_passive() {
            return Err(Error::InvalidMaterial(format!(
                "{}: boundary model is not passive",
                self.name
            )));
        }
        self.boundary = Some(boundary);
        Ok(self)
    }

    /// The boundary model a wave-solved scene uses: the explicit one, or one fitted to the band
    /// absorption.
    pub fn boundary_model(&self) -> Result<Boundary, Error> {
        match &self.boundary {
            Some(b) => Ok(b.clone()),
            None => Boundary::fitted_to_absorption(&self.absorption),
        }
    }

    /// A material from the 125 Hz–4 kHz octaves published tables give, extended by
    /// [`bands::from_125_to_4k`].
    pub fn from_125_to_4k(
        name: impl Into<String>,
        absorption: [f64; 6],
        scattering: [f64; 6],
    ) -> Result<Self, Error> {
        Self::new(
            name,
            bands::from_125_to_4k(absorption),
            bands::from_125_to_4k(scattering),
        )
    }

    /// No absorption, no scattering: the rigid, specular wall of Allen & Berkley's lossless case.
    pub fn rigid() -> Self {
        Self {
            name: "rigid".into(),
            absorption: [0.0; NUM_BANDS],
            scattering: [0.0; NUM_BANDS],
            boundary: None,
        }
    }

    /// Total absorption: nothing is reflected.
    pub fn anechoic() -> Self {
        Self {
            name: "anechoic".into(),
            absorption: [1.0; NUM_BANDS],
            scattering: [0.0; NUM_BANDS],
            boundary: None,
        }
    }

    /// Pressure amplitude a specular path keeps at one reflection: `sqrt((1 − α)(1 − s))` per band.
    pub fn specular_pressure_reflection(&self) -> Bands {
        std::array::from_fn(|k| ((1.0 - self.absorption[k]) * (1.0 - self.scattering[k])).sqrt())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rejects_out_of_range() {
        assert!(Material::from_125_to_4k("x", [0.1, 0.2, 1.14, 0.5, 0.5, 0.5], [0.0; 6]).is_err());
        assert!(Material::from_125_to_4k("x", [0.1; 6], [f64::NAN; 6]).is_err());
        assert!(Material::from_125_to_4k("x", [-0.01; 6], [0.0; 6]).is_err());
    }

    #[test]
    fn specular_reflection_is_energy_consistent() {
        let m = Material::from_125_to_4k("plaster", [0.02, 0.02, 0.03, 0.04, 0.05, 0.05], [0.1; 6])
            .unwrap();
        let r = m.specular_pressure_reflection();
        for (k, rk) in r.iter().enumerate() {
            let energy = rk * rk;
            let expected = (1.0 - m.absorption[k]) * (1.0 - m.scattering[k]);
            assert!((energy - expected).abs() < 1e-12);
        }
        assert_eq!(
            Material::rigid().specular_pressure_reflection(),
            [1.0; NUM_BANDS]
        );
        assert_eq!(
            Material::anechoic().specular_pressure_reflection(),
            [0.0; NUM_BANDS]
        );
    }
}
