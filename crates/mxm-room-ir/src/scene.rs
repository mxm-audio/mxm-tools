//! A scene: a room, its air, a source and a receiver — and the hash that names it.
//!
//! **Non-diffuse scenes.** A scene declared non-diffuse (a long tunnel, a corridor) keeps its
//! specular structure for the whole response: specular paths found by rays past the image-source
//! order stay discrete arrivals, and only scattered energy becomes noise (plan §4.2).

use crate::air::Air;
use crate::directivity::Directivity;
use crate::error::Error;
use crate::geometry::{Room, Vec3};

#[derive(Debug, Clone, PartialEq)]
pub struct Scene {
    pub name: String,
    pub room: Room,
    pub air: Air,
    pub source: Vec3,
    pub receiver: Vec3,
    /// The source's radiation pattern, in world coordinates.
    pub source_directivity: Directivity,
    /// Whether late specular structure is kept discrete (see the module note).
    pub non_diffuse: bool,
}

impl Scene {
    /// Builds a scene, rejecting a source or receiver that is not strictly inside the room.
    pub fn new(
        name: impl Into<String>,
        room: Room,
        air: Air,
        source: Vec3,
        receiver: Vec3,
    ) -> Result<Self, Error> {
        for (label, p) in [("source", source), ("receiver", receiver)] {
            if !room.contains(p) {
                return Err(Error::PointOutsideRoom(format!("{label} at {p:?}")));
            }
        }
        Ok(Self {
            name: name.into(),
            room,
            air,
            source,
            receiver,
            source_directivity: Directivity::Omni,
            non_diffuse: false,
        })
    }

    /// The same scene with a directional source.
    pub fn with_source_directivity(mut self, directivity: Directivity) -> Result<Self, Error> {
        directivity.validate()?;
        self.source_directivity = directivity;
        Ok(self)
    }

    /// The same scene declared non-diffuse.
    pub fn non_diffuse(mut self) -> Self {
        self.non_diffuse = true;
        self
    }

    /// Every input that decides the result, as text with fixed precision.
    pub fn canonical_text(&self) -> String {
        use std::fmt::Write;
        let mut s = String::new();
        let v = |p: Vec3| format!("{:.9},{:.9},{:.9}", p.x, p.y, p.z);
        let _ = writeln!(s, "name {}", self.name);
        let _ = writeln!(
            s,
            "air {:.6} {:.6} {:.6}",
            self.air.temperature_c, self.air.relative_humidity_pct, self.air.pressure_kpa
        );
        let _ = writeln!(s, "source {}", v(self.source));
        let _ = writeln!(s, "receiver {}", v(self.receiver));
        let _ = writeln!(
            s,
            "source-directivity {}",
            self.source_directivity.canonical_text()
        );
        let _ = writeln!(s, "non-diffuse {}", self.non_diffuse);
        for m in &self.room.materials {
            let _ = writeln!(
                s,
                "material {} a {:?} s {:?} boundary {}",
                m.name,
                m.absorption,
                m.scattering,
                m.boundary
                    .as_ref()
                    .map_or("fitted-on-demand".to_string(), |b| b.canonical_text())
            );
        }
        for p in &self.room.polygons {
            let verts: Vec<String> = p.vertices.iter().map(|&x| v(x)).collect();
            let _ = writeln!(s, "face m{} {}", p.material, verts.join(" "));
        }
        s
    }

    /// FNV-1a 64 of [`Scene::canonical_text`], as 16 hex digits.
    pub fn hash_hex(&self) -> String {
        format!("{:016x}", fnv1a64(self.canonical_text().as_bytes()))
    }
}

/// FNV-1a, 64-bit (Fowler, Noll & Vo).
pub fn fnv1a64(bytes: &[u8]) -> u64 {
    let mut h: u64 = 0xcbf2_9ce4_8422_2325;
    for &b in bytes {
        h ^= u64::from(b);
        h = h.wrapping_mul(0x0000_0100_0000_01b3);
    }
    h
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::material::Material;

    #[test]
    fn fnv_reference_vector() {
        assert_eq!(fnv1a64(b""), 0xcbf2_9ce4_8422_2325);
        assert_eq!(fnv1a64(b"a"), 0xaf63_dc4c_8601_ec8c);
    }

    #[test]
    fn hash_changes_with_inputs_and_rejects_outside_points() {
        let room = Room::shoebox(
            Vec3::new(4.0, 3.0, 2.5),
            std::array::from_fn(|_| Material::rigid()),
        )
        .unwrap();
        let a = Scene::new(
            "a",
            room.clone(),
            Air::standard(),
            Vec3::new(1.0, 1.0, 1.0),
            Vec3::new(3.0, 2.0, 1.2),
        )
        .unwrap();
        let mut b = a.clone();
        b.receiver.x += 0.001;
        assert_ne!(a.hash_hex(), b.hash_hex());
        assert_eq!(a.hash_hex(), a.clone().hash_hex());
        assert!(
            Scene::new(
                "c",
                room,
                Air::standard(),
                Vec3::new(5.0, 1.0, 1.0),
                Vec3::new(1.0, 1.0, 1.0)
            )
            .is_err()
        );
    }
}
