//! Directivity of sources and capsules, microphone arrays, and the frame an array is placed in.
//!
//! **Directions are unit vectors from the point outward**: for a capsule, toward where the sound
//! comes from; for a source, the way it leaves. A first-order pattern has pressure gain
//! `a + (1 − a)·cos θ` about its axis, so `a = 1` is omni, `0.5` cardioid and `0` figure-eight.
//! A figure-eight's rear lobe has negative gain, which is its polarity, and is kept.
//!
//! **Tabulated directivity** is measured data (plan §4.4): gains per band on a grid of directions
//! in the source's or capsule's own frame, looked up bilinearly. Only magnitude is represented, so
//! a comparison that depends on a directivity's phase is outside what it can check.
//!
//! **Local frame.** Arrays are written in a frame with `x` forward, `y` left and `z` up, and placed
//! with a [`Frame`]. First-order ambisonics uses ACN channel order (W, Y, Z, X) with SN3D
//! normalisation, so at first order every channel's peak gain is one.

use std::f64::consts::PI;
use std::sync::Arc;

use crate::bands::{Bands, NUM_BANDS};
use crate::error::Error;
use crate::geometry::Vec3;

#[derive(Debug, Clone, PartialEq)]
pub enum Directivity {
    Omni,
    /// `pattern + (1 − pattern)·cos θ` about `axis`, the same in every band.
    FirstOrder {
        axis: Vec3,
        pattern: f64,
    },
    /// A first-order pattern per band: the frequency-dependent directional option (plan §4.4).
    BandFirstOrder {
        axis: Vec3,
        pattern: Bands,
    },
    /// Measured gains on a grid of directions about `frame`.
    Tabulated {
        table: Arc<DirectivityTable>,
        frame: Frame,
    },
}

impl Directivity {
    /// Pressure gain per band toward `direction` (a unit vector).
    pub fn band_gains(&self, direction: Vec3) -> Bands {
        match self {
            Self::Omni => [1.0; NUM_BANDS],
            Self::FirstOrder { axis, pattern } => {
                let g = pattern + (1.0 - pattern) * axis.dot(direction);
                [g; NUM_BANDS]
            }
            Self::BandFirstOrder { axis, pattern } => {
                let c = axis.dot(direction);
                std::array::from_fn(|k| pattern[k] + (1.0 - pattern[k]) * c)
            }
            Self::Tabulated { table, frame } => table.gains_at(Vec3::new(
                direction.dot(frame.forward),
                direction.dot(frame.left),
                direction.dot(frame.up),
            )),
        }
    }

    /// Whether the gain is one in every band and direction.
    pub fn is_omni(&self) -> bool {
        match self {
            Self::Omni => true,
            Self::FirstOrder { pattern, .. } => *pattern == 1.0,
            Self::BandFirstOrder { pattern, .. } => pattern.iter().all(|&p| p == 1.0),
            Self::Tabulated { table, .. } => table.gains.iter().flatten().all(|&g| g == 1.0),
        }
    }

    /// The same pattern with its axis mapped from a frame's local coordinates to the world.
    pub fn placed(&self, frame: &Frame) -> Self {
        match self {
            Self::Omni => Self::Omni,
            Self::FirstOrder { axis, pattern } => Self::FirstOrder {
                axis: frame.to_world(*axis),
                pattern: *pattern,
            },
            Self::BandFirstOrder { axis, pattern } => Self::BandFirstOrder {
                axis: frame.to_world(*axis),
                pattern: *pattern,
            },
            Self::Tabulated { table, frame: own } => Self::Tabulated {
                table: Arc::clone(table),
                frame: Frame {
                    forward: frame.to_world(own.forward),
                    left: frame.to_world(own.left),
                    up: frame.to_world(own.up),
                },
            },
        }
    }

    pub(crate) fn validate(&self) -> Result<(), Error> {
        let check_axis = |axis: &Vec3| {
            if (axis.length() - 1.0).abs() > 1e-6 {
                Err(Error::InvalidOption(format!(
                    "directivity axis {axis:?} is not a unit vector"
                )))
            } else {
                Ok(())
            }
        };
        let check_pattern = |p: f64| {
            if !(0.0..=1.0).contains(&p) {
                Err(Error::InvalidOption(format!(
                    "first-order pattern {p} is outside 0..=1"
                )))
            } else {
                Ok(())
            }
        };
        match self {
            Self::Omni => Ok(()),
            Self::FirstOrder { axis, pattern } => {
                check_axis(axis)?;
                check_pattern(*pattern)
            }
            Self::BandFirstOrder { axis, pattern } => {
                check_axis(axis)?;
                pattern.iter().try_for_each(|&p| check_pattern(p))
            }
            Self::Tabulated { frame, .. } => {
                check_axis(&frame.forward)?;
                check_axis(&frame.left)?;
                if (frame.forward.cross(frame.left) - frame.up).length() > 1e-6 {
                    return Err(Error::InvalidOption(
                        "a tabulated directivity's frame must be orthonormal and right-handed"
                            .into(),
                    ));
                }
                Ok(())
            }
        }
    }

    /// Stable text for scene hashing.
    pub(crate) fn canonical_text(&self) -> String {
        let v = |p: &Vec3| format!("{:.9},{:.9},{:.9}", p.x, p.y, p.z);
        match self {
            Self::Omni => "omni".into(),
            Self::FirstOrder { axis, pattern } => format!("first-order {} {pattern:?}", v(axis)),
            Self::BandFirstOrder { axis, pattern } => {
                format!("band-first-order {} {pattern:?}", v(axis))
            }
            Self::Tabulated { table, frame } => format!(
                "tabulated {} {:016x} {} {} {}",
                table.name,
                table.digest,
                v(&frame.forward),
                v(&frame.left),
                v(&frame.up)
            ),
        }
    }
}

/// Pressure gains per band measured on a regular grid of directions in a source's or capsule's own
/// frame (x forward, y left, z up), the form loudspeaker directivity databases publish.
///
/// A direction's **polar angle** runs from forward (0°) to backward (180°); its **rotation** turns
/// about forward from up (0°) toward left (90°). This is BRAS's "front pole" grid. `rings` polar
/// samples span 0° to 180° inclusive and `rotations` samples one full turn from 0°, so a pole's
/// ring repeats one value. Lookup is bilinear in the two angles.
#[derive(Clone, PartialEq)]
pub struct DirectivityTable {
    pub name: String,
    rings: usize,
    rotations: usize,
    /// `gains[ring * rotations + rotation]`.
    gains: Vec<Bands>,
    /// FNV-1a over the grid and every gain's bits, for scene hashing.
    digest: u64,
}

impl DirectivityTable {
    /// A table from `rings × rotations` gains, ring first. Gains must be finite and non-negative.
    pub fn new(
        name: impl Into<String>,
        rings: usize,
        rotations: usize,
        gains: Vec<Bands>,
    ) -> Result<Self, Error> {
        let name = name.into();
        if rings < 2 || rotations < 1 || gains.len() != rings.saturating_mul(rotations) {
            return Err(Error::InvalidOption(format!(
                "directivity table `{name}` needs two or more rings, a rotation, and rings × rotations gains"
            )));
        }
        if let Some(bad) = gains.iter().flatten().find(|g| !g.is_finite() || **g < 0.0) {
            return Err(Error::InvalidOption(format!(
                "directivity table `{name}` has gain {bad}"
            )));
        }
        let mut digest = 0xcbf2_9ce4_8422_2325_u64;
        let mut eat = |bytes: &[u8]| {
            for &b in bytes {
                digest ^= u64::from(b);
                digest = digest.wrapping_mul(0x0100_0000_01b3);
            }
        };
        eat(&(rings as u64).to_le_bytes());
        eat(&(rotations as u64).to_le_bytes());
        for g in gains.iter().flatten() {
            eat(&g.to_bits().to_le_bytes());
        }
        Ok(Self {
            name,
            rings,
            rotations,
            gains,
            digest,
        })
    }

    pub fn rings(&self) -> usize {
        self.rings
    }

    pub fn rotations(&self) -> usize {
        self.rotations
    }

    /// Gains toward `local`, a unit vector in the table's own frame.
    pub fn gains_at(&self, local: Vec3) -> Bands {
        let polar = local.x.clamp(-1.0, 1.0).acos();
        let rotation = local.y.atan2(local.z).rem_euclid(2.0 * PI);
        let fr = polar / PI * (self.rings - 1) as f64;
        let ring = (fr.floor() as usize).min(self.rings - 2);
        let t = (fr - ring as f64).clamp(0.0, 1.0);
        let fq = rotation / (2.0 * PI) * self.rotations as f64;
        let q0 = (fq.floor() as usize) % self.rotations;
        let q1 = (q0 + 1) % self.rotations;
        let u = (fq - fq.floor()).clamp(0.0, 1.0);
        let at = |r: usize, q: usize| &self.gains[r * self.rotations + q];
        std::array::from_fn(|k| {
            let near = at(ring, q0)[k] * (1.0 - u) + at(ring, q1)[k] * u;
            let far = at(ring + 1, q0)[k] * (1.0 - u) + at(ring + 1, q1)[k] * u;
            near * (1.0 - t) + far * t
        })
    }
}

impl std::fmt::Debug for DirectivityTable {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("DirectivityTable")
            .field("name", &self.name)
            .field("rings", &self.rings)
            .field("rotations", &self.rotations)
            .field("digest", &format_args!("{:016x}", self.digest))
            .finish()
    }
}

/// An orthonormal frame: `forward`, `left`, `up`.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Frame {
    pub forward: Vec3,
    pub left: Vec3,
    pub up: Vec3,
}

impl Frame {
    /// The world frame: forward +x, left +y, up +z.
    pub const WORLD: Self = Self {
        forward: Vec3::new(1.0, 0.0, 0.0),
        left: Vec3::new(0.0, 1.0, 0.0),
        up: Vec3::new(0.0, 0.0, 1.0),
    };

    /// A level frame at `from` facing `toward` in the horizontal plane. Falls back to the world
    /// frame when `toward` is straight above or below.
    pub fn facing(from: Vec3, toward: Vec3) -> Self {
        let d = toward - from;
        let horizontal = Vec3::new(d.x, d.y, 0.0);
        if horizontal.length() < 1e-9 {
            return Self::WORLD;
        }
        let forward = horizontal.normalized();
        let up = Vec3::new(0.0, 0.0, 1.0);
        Self {
            forward,
            left: up.cross(forward),
            up,
        }
    }

    /// A frame facing along `direction` with no roll: left stays horizontal. Falls back to the
    /// world's left when `direction` is vertical.
    pub fn pointing(direction: Vec3) -> Self {
        let forward = direction.normalized();
        let side = Vec3::new(0.0, 0.0, 1.0).cross(forward);
        let left = if side.length() < 1e-9 {
            Vec3::new(0.0, 1.0, 0.0)
        } else {
            side.normalized()
        };
        Self {
            forward,
            left,
            up: forward.cross(left),
        }
    }

    pub fn to_world(&self, local: Vec3) -> Vec3 {
        self.forward * local.x + self.left * local.y + self.up * local.z
    }
}

/// One capsule of an array, in the array's local frame.
#[derive(Debug, Clone, PartialEq)]
pub struct Capsule {
    pub name: String,
    /// Offset from the array centre, m.
    pub offset: Vec3,
    pub directivity: Directivity,
}

/// A capsule placed in the world.
#[derive(Debug, Clone, PartialEq)]
pub struct PlacedCapsule {
    pub name: String,
    pub offset: Vec3,
    pub directivity: Directivity,
}

/// A set of capsules recorded together, one channel each.
#[derive(Debug, Clone, PartialEq)]
pub struct Array {
    pub name: String,
    pub capsules: Vec<Capsule>,
}

fn capsule(name: &str, offset: Vec3, directivity: Directivity) -> Capsule {
    Capsule {
        name: name.into(),
        offset,
        directivity,
    }
}

fn horizontal(azimuth_deg: f64) -> Vec3 {
    let a = azimuth_deg.to_radians();
    Vec3::new(a.cos(), a.sin(), 0.0)
}

impl Array {
    /// One omni capsule at the centre.
    pub fn mono() -> Self {
        Self {
            name: "mono omni".into(),
            capsules: vec![capsule("omni", Vec3::default(), Directivity::Omni)],
        }
    }

    /// Two omnis `spacing` apart on the left–right axis.
    pub fn spaced_omni(spacing_m: f64) -> Self {
        let h = spacing_m / 2.0;
        Self {
            name: format!("spaced omni pair, {spacing_m} m"),
            capsules: vec![
                capsule("left", Vec3::new(0.0, h, 0.0), Directivity::Omni),
                capsule("right", Vec3::new(0.0, -h, 0.0), Directivity::Omni),
            ],
        }
    }

    /// Two cardioids `spacing` apart, each turned `half_angle_deg` outward. ORTF is 0.17 m, 55°.
    pub fn near_coincident_cardioids(spacing_m: f64, half_angle_deg: f64) -> Self {
        let h = spacing_m / 2.0;
        Self {
            name: format!("near-coincident cardioids, {spacing_m} m, ±{half_angle_deg}°"),
            capsules: vec![
                capsule(
                    "left",
                    Vec3::new(0.0, h, 0.0),
                    Directivity::FirstOrder {
                        axis: horizontal(half_angle_deg),
                        pattern: 0.5,
                    },
                ),
                capsule(
                    "right",
                    Vec3::new(0.0, -h, 0.0),
                    Directivity::FirstOrder {
                        axis: horizontal(-half_angle_deg),
                        pattern: 0.5,
                    },
                ),
            ],
        }
    }

    /// Coincident figure-eights at ±45° (Blumlein pair).
    pub fn coincident_figure_eights() -> Self {
        Self {
            name: "coincident figure-eights, ±45°".into(),
            capsules: vec![
                capsule(
                    "left",
                    Vec3::default(),
                    Directivity::FirstOrder {
                        axis: horizontal(45.0),
                        pattern: 0.0,
                    },
                ),
                capsule(
                    "right",
                    Vec3::default(),
                    Directivity::FirstOrder {
                        axis: horizontal(-45.0),
                        pattern: 0.0,
                    },
                ),
            ],
        }
    }

    /// Mid–side, decoded: a forward cardioid `M` and a sideways figure-eight `S` give left
    /// `(M + S)/√2`-style channels. Emitted here undecoded, as `mid` and `side`.
    pub fn mid_side() -> Self {
        Self {
            name: "mid-side, cardioid mid".into(),
            capsules: vec![
                capsule(
                    "mid",
                    Vec3::default(),
                    Directivity::FirstOrder {
                        axis: horizontal(0.0),
                        pattern: 0.5,
                    },
                ),
                capsule(
                    "side",
                    Vec3::default(),
                    Directivity::FirstOrder {
                        axis: horizontal(90.0),
                        pattern: 0.0,
                    },
                ),
            ],
        }
    }

    /// First-order ambisonics, ACN order (W, Y, Z, X), SN3D.
    pub fn first_order_ambisonics() -> Self {
        let eight = |axis: Vec3| Directivity::FirstOrder { axis, pattern: 0.0 };
        Self {
            name: "first-order ambisonics, ACN/SN3D".into(),
            capsules: vec![
                capsule("W", Vec3::default(), Directivity::Omni),
                capsule("Y", Vec3::default(), eight(Vec3::new(0.0, 1.0, 0.0))),
                capsule("Z", Vec3::default(), eight(Vec3::new(0.0, 0.0, 1.0))),
                capsule("X", Vec3::default(), eight(Vec3::new(1.0, 0.0, 0.0))),
            ],
        }
    }

    /// The capsules mapped into the world with `frame`.
    pub fn placed(&self, frame: &Frame) -> Result<Vec<PlacedCapsule>, Error> {
        if self.capsules.is_empty() {
            return Err(Error::InvalidOption("an array needs a capsule".into()));
        }
        self.capsules
            .iter()
            .map(|c| {
                c.directivity.validate()?;
                Ok(PlacedCapsule {
                    name: c.name.clone(),
                    offset: frame.to_world(c.offset),
                    directivity: c.directivity.placed(frame),
                })
            })
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn patterns_have_their_textbook_gains() {
        let axis = Vec3::new(1.0, 0.0, 0.0);
        let cardioid = Directivity::FirstOrder { axis, pattern: 0.5 };
        assert_eq!(cardioid.band_gains(axis)[3], 1.0);
        assert_eq!(cardioid.band_gains(-axis)[3], 0.0);
        let eight = Directivity::FirstOrder { axis, pattern: 0.0 };
        assert_eq!(eight.band_gains(-axis)[0], -1.0);
        assert_eq!(eight.band_gains(Vec3::new(0.0, 1.0, 0.0))[0], 0.0);
    }

    #[test]
    fn frame_facing_is_orthonormal_and_ambisonics_follow_it() {
        let f = Frame::facing(Vec3::new(1.0, 1.0, 1.5), Vec3::new(1.0, 5.0, 1.0));
        assert!((f.forward - Vec3::new(0.0, 1.0, 0.0)).length() < 1e-12);
        assert!((f.left - Vec3::new(-1.0, 0.0, 0.0)).length() < 1e-12);
        let foa = Array::first_order_ambisonics().placed(&f).unwrap();
        // A sound from the front reaches X fully and Y not at all.
        let front = f.forward;
        assert_eq!(foa[3].directivity.band_gains(front)[4], 1.0);
        assert!(foa[1].directivity.band_gains(front)[4].abs() < 1e-12);
        assert_eq!(foa[0].directivity.band_gains(front)[4], 1.0);
    }

    #[test]
    fn tabulated_gains_follow_the_front_pole_grid_and_their_frame() {
        // Five rings (0°, 45°, 90°, 135°, 180°) × four rotations (up, left, down, right). Band 0
        // holds the ring index and band 1 the rotation index, so a lookup reads back where it landed.
        let gains = (0..5)
            .flat_map(|r| {
                (0..4).map(move |q| {
                    let mut g = [0.0; NUM_BANDS];
                    g[0] = r as f64;
                    g[1] = q as f64;
                    g
                })
            })
            .collect();
        let table = Arc::new(DirectivityTable::new("grid", 5, 4, gains).unwrap());
        let d = Directivity::Tabulated {
            table: Arc::clone(&table),
            frame: Frame::pointing(Vec3::new(0.0, 1.0, 0.0)),
        };
        d.validate().unwrap();
        assert!(d.band_gains(Vec3::new(0.0, 1.0, 0.0))[0].abs() < 1e-12);
        let up = d.band_gains(Vec3::new(0.0, 0.0, 1.0));
        assert!((up[0] - 2.0).abs() < 1e-12 && up[1].abs() < 1e-12);
        // Facing +y, the frame's left is −x.
        let left = d.band_gains(Vec3::new(-1.0, 0.0, 0.0));
        assert!((left[0] - 2.0).abs() < 1e-12 && (left[1] - 1.0).abs() < 1e-12);
        let halfway = d.band_gains(Vec3::new(0.0, 1.0, 1.0).normalized());
        assert!((halfway[0] - 1.0).abs() < 1e-9);
        // Placing turns the frame: the world frame's forward becomes the placement's.
        let placed = Directivity::Tabulated {
            table: Arc::clone(&table),
            frame: Frame::WORLD,
        }
        .placed(&Frame::pointing(Vec3::new(0.0, 0.0, -1.0)));
        assert!(placed.band_gains(Vec3::new(0.0, 0.0, -1.0))[0].abs() < 1e-12);
        let skew = Directivity::Tabulated {
            table,
            frame: Frame {
                forward: Vec3::new(1.0, 0.0, 0.0),
                left: Vec3::new(1.0, 0.0, 0.0),
                up: Vec3::new(0.0, 0.0, 1.0),
            },
        };
        assert!(skew.validate().is_err());
        assert!(DirectivityTable::new("short", 5, 4, vec![[1.0; NUM_BANDS]; 3]).is_err());
    }

    #[test]
    fn ortf_capsules_sit_left_and_right() {
        let placed = Array::near_coincident_cardioids(0.17, 55.0)
            .placed(&Frame::WORLD)
            .unwrap();
        assert!((placed[0].offset.y - 0.085).abs() < 1e-12);
        assert!((placed[1].offset.y + 0.085).abs() < 1e-12);
    }
}
