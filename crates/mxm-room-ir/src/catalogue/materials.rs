//! The catalogue's surfaces, built from the research page's absorption rows
//! (`research:effects/room-acoustics-simulation.md` §8.1). Each row is one published source, quoted
//! with its tier and number; nothing here is tuned to a room.
//!
//! **A surface is a blend of rows by area fraction**, because one polygon of a generic room stands
//! for a wall with a window, a floor with a rug. Absorption blends by area. Scattering follows the
//! page's rule, `s(f) = 0.5·d·f/c` from a part's characteristic depth `d`, clamped to 0.05–0.99
//! (BRAS documentation §3.2.2, after Postma & Katz 2015), and blends weighted by each part's
//! reflected energy. On top of the rule, every surface scatters [`DETAIL_SCATTERING`] of its
//! remaining specular reflection, for the detail no row, depth or solid stands for. A blend is
//! labelled by its parts, so the sidecar says what the wall was.

use crate::bands::{Bands, NOMINAL_CENTRES_HZ, NUM_BANDS};
use crate::error::Error;
use crate::material::Material;

/// One published absorption row, random incidence, 125 Hz–4 kHz.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Row {
    pub name: &'static str,
    /// Tier and number, as the research page gives them.
    pub source: &'static str,
    pub absorption: [f64; 6],
}

/// Speed of sound in the scattering rule, m/s. **Chosen:** 20 °C air; the rule is an estimate.
const RULE_SPEED: f64 = 343.0;

pub const HARD: Row = Row {
    name: "plaster, masonry or hard floor",
    source: "[S-PTB] No. 11, Heckl–Müller 1994",
    absorption: [0.02, 0.02, 0.03, 0.03, 0.04, 0.04],
};
pub const CONCRETE_FLOOR: Row = Row {
    name: "concrete or terrazzo floor",
    source: "[S-PTB] No. 9, Harris 1991",
    absorption: [0.01, 0.01, 0.015, 0.02, 0.02, 0.02],
};
pub const BLOCK_PAINTED: Row = Row {
    name: "concrete block, painted",
    source: "[S-PTB] No. 8, Harris 1991",
    absorption: [0.10, 0.05, 0.06, 0.07, 0.09, 0.08],
};
pub const BRICK: Row = Row {
    name: "brick, exposed",
    source: "[S-PTB] No. 12, Meyer",
    absorption: [0.15, 0.13, 0.15, 0.15, 0.13, 0.14],
};
pub const PLASTER: Row = Row {
    name: "smooth plaster on masonry",
    source: "[S-PTB] No. 328, Meyer",
    absorption: [0.02, 0.02, 0.03, 0.04, 0.05, 0.05],
};
pub const PLASTERBOARD: Row = Row {
    name: "plasterboard on studs, empty cavity",
    source: "[S-PTB] No. 304, DNA 1968",
    absorption: [0.08, 0.11, 0.04, 0.03, 0.03, 0.0],
};
pub const TILE: Row = Row {
    name: "marble or glazed tile",
    source: "[S-PTB] No. 10, Harris 1991",
    absorption: [0.01, 0.01, 0.01, 0.01, 0.02, 0.02],
};
pub const SANDSTONE: Row = Row {
    name: "rough sandstone",
    source: "[S-PTB] No. 15, Meyer",
    absorption: [0.02, 0.02, 0.03, 0.04, 0.05, 0.05],
};
pub const LEADED_GLAZING: Row = Row {
    name: "leaded church glazing",
    source: "[S-PTB] No. 198, Meyer",
    absorption: [0.30, 0.20, 0.14, 0.10, 0.05, 0.05],
};
pub const GLASS: Row = Row {
    name: "ordinary window glass",
    source: "[S-PTB] No. 193, Harris 1991",
    absorption: [0.35, 0.25, 0.18, 0.12, 0.07, 0.04],
};
pub const WOOD_FLOOR: Row = Row {
    name: "wood floor over cavity",
    source: "[S-PTB] No. 352, Harris 1991",
    absorption: [0.15, 0.11, 0.10, 0.07, 0.06, 0.07],
};
pub const WOOD_LINING: Row = Row {
    name: "wood lining over air gap",
    source: "[S-PTB] No. 354, Harris 1991",
    absorption: [0.28, 0.22, 0.17, 0.09, 0.10, 0.11],
};
pub const CARPET: Row = Row {
    name: "carpet, heavy, on concrete",
    source: "[S-PTB] No. 116, Harris 1991",
    absorption: [0.02, 0.06, 0.14, 0.37, 0.60, 0.65],
};
pub const VELOUR: Row = Row {
    name: "heavy velour, draped to half area",
    source: "[S-PTB] No. 129, Harris 1991",
    absorption: [0.14, 0.35, 0.55, 0.72, 0.70, 0.65],
};
pub const SEATS: Row = Row {
    name: "upholstered seats, unoccupied",
    source: "[S-PTB] No. 517, Beranek–Hidaka 1998",
    absorption: [0.70, 0.76, 0.81, 0.84, 0.84, 0.81],
};
pub const PEWS: Row = Row {
    name: "pews, no cushions",
    source: "[S-PTB] No. 463, Meyer",
    absorption: [0.10, 0.15, 0.18, 0.20, 0.20, 0.20],
};
pub const GLASS_WOOL: Row = Row {
    name: "glass wool 51 mm",
    source: "[S-PTB] No. 222, Harris 1991",
    absorption: [0.17, 0.55, 0.80, 0.90, 0.85, 0.80],
};
pub const BALLAST: Row = Row {
    name: "crushed-stone ballast, 30 cm",
    source: "[S-PTB] No. 2, Harris 1991",
    absorption: [0.27, 0.58, 0.48, 0.54, 0.73, 0.63],
};
pub const WATER: Row = Row {
    name: "water surface",
    source: "[S-PTB] No. 669, Harris 1991",
    absorption: [0.008, 0.008, 0.013, 0.015, 0.020, 0.025],
};

// The rows below are not among the page's representative rows; they are from the full tables of
// its working notes (`research:sources/room-acoustics-simulation/notes/r0-materials.md`), which the
// page names as holding every row, quoted as the notes give them.

pub const BLOCK_COARSE: Row = Row {
    name: "concrete block, coarse, unpainted",
    source: "[S-PTB] No. 7, Harris 1991",
    absorption: [0.36, 0.44, 0.31, 0.29, 0.39, 0.25],
};
pub const PLATE_GLASS: Row = Row {
    name: "large panes of heavy plate glass",
    source: "[S-PTB] No. 192, Harris 1991",
    absorption: [0.18, 0.06, 0.04, 0.03, 0.02, 0.02],
};
pub const PARQUET: Row = Row {
    name: "wood parquet on concrete",
    source: "[S-PTB] No. 353, Harris 1991",
    absorption: [0.04, 0.04, 0.07, 0.06, 0.06, 0.07],
};
pub const BASS_TRAP: Row = Row {
    name: "6 mm plywood, 100 mm gap, mineral-fibre board",
    source: "[S-PTB] No. 388, DNA 1968",
    absorption: [0.75, 0.30, 0.12, 0.05, 0.04, 0.03],
};
pub const PERFORATED_WOOD: Row = Row {
    name: "wood panels, 6 % perforated, 5 cm cavity",
    source: "[S-PTB] No. 364, Meyer",
    absorption: [0.35, 0.80, 0.65, 0.45, 0.25, 0.20],
};
pub const PERFORATED_METAL: Row = Row {
    name: "perforated metal with fleece, 5 cm cavity",
    source: "[S-PTB] No. 575, Meyer",
    absorption: [0.26, 0.75, 0.95, 0.97, 0.79, 0.67],
};
pub const MINERAL_BOARD: Row = Row {
    name: "mineral-fibre board 50 mm, 40 kg/m³",
    source: "[S-PTB] No. 252, DNA 1968",
    absorption: [0.20, 0.60, 0.87, 0.93, 0.98, 0.97],
};
pub const CARPET_ON_PAD: Row = Row {
    name: "carpet, heavy, on foam rubber",
    source: "[S-PTB] No. 117, Harris 1991",
    absorption: [0.08, 0.24, 0.57, 0.69, 0.71, 0.73],
};
pub const CARPET_THIN: Row = Row {
    name: "carpet, thin, cemented to concrete",
    source: "[S-PTB] No. 115, Beranek–Hidaka 1998",
    absorption: [0.02, 0.04, 0.08, 0.20, 0.35, 0.40],
};
pub const CURTAIN: Row = Row {
    name: "curtain fabric, 25 cm in front of the wall",
    source: "[S-PTB] No. 151, Meyer",
    absorption: [0.30, 0.60, 0.75, 0.60, 0.70, 0.75],
};
pub const MEDIUM_SEATS: Row = Row {
    name: "medium upholstered seats, unoccupied",
    source: "[S-PTB] No. 486, Beranek–Hidaka 1998",
    absorption: [0.54, 0.62, 0.68, 0.70, 0.68, 0.66],
};
pub const WOODEN_CHAIRS: Row = Row {
    name: "wooden chairs, no upholstery",
    source: "[S-PTB] No. 465, Meyer",
    absorption: [0.05, 0.08, 0.10, 0.12, 0.12, 0.12],
};

/// The share of a relief part's area its left-out geometry covers. **Chosen:** piers, pilasters,
/// ribs, trusses and window recesses stand proud of a quarter of a wall or ceiling; the rest
/// reflects as the flat row. Before 2026-09-15 the depth covered the whole part, which put a big
/// room's walls at the rule's 0.99 ceiling from 2 kHz and turned every early reflection there into
/// the same diffuse cloud.
pub const RELIEF_SHARE: f64 = 0.25;

/// The share of a surface's specular reflection that detail scatters, in every band: fittings,
/// fixings, mouldings, ducts, uneven masonry — whatever no row, depth or solid stands for. It is an
/// independent mechanism beside the depth rule, so the two combine as
/// `s = 1 − (1 − s_rule)(1 − DETAIL_SCATTERING)`, held under the rule's 0.99 ceiling.
/// **Chosen by measurement, not read** (2026-09-16): renders of ten spaces set against their recorded
/// impulse responses were too specular — early fields too sparse and early decays well short of the
/// late — and this share over every surface corrected both where walls were flat. The measurement is
/// in this crate's `NOTES.md`, with the one space it made worse.
pub const DETAIL_SCATTERING: f64 = 0.5;

/// The depth rule's bounds, which the combined scattering also keeps under.
const RULE_FLOOR: f64 = 0.05;
const RULE_CEILING: f64 = 0.99;

/// One part of a surface: a row, the fraction of the surface's area it covers, its characteristic
/// depth for the scattering rule, m, and the share of the part that depth covers.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Part {
    pub row: Row,
    pub fraction: f64,
    pub depth_m: f64,
    pub relief_share: f64,
}

/// A part covering `fraction` of a surface, all of it with characteristic depth `depth_m`: a
/// texture, or a surface whose depth is the surface itself (seats, pews, drapes).
pub const fn part(row: Row, fraction: f64, depth_m: f64) -> Part {
    Part {
        row,
        fraction,
        depth_m,
        relief_share: 1.0,
    }
}

/// A flat part covering `fraction` of a surface, with left-out geometry of depth `depth_m` over
/// [`RELIEF_SHARE`] of it.
pub const fn relief(row: Row, fraction: f64, depth_m: f64) -> Part {
    Part {
        row,
        fraction,
        depth_m,
        relief_share: RELIEF_SHARE,
    }
}

/// The scattering rule's value in band `k` for depth `d`.
pub fn rule_scattering(depth_m: f64, k: usize) -> f64 {
    (0.5 * depth_m * NOMINAL_CENTRES_HZ[k] / RULE_SPEED).clamp(RULE_FLOOR, RULE_CEILING)
}

/// `rule`, a blend of the depth rule's values, with [`DETAIL_SCATTERING`] combined into it.
pub fn with_detail_scattering(rule: f64) -> f64 {
    (1.0 - (1.0 - rule) * (1.0 - DETAIL_SCATTERING)).min(RULE_CEILING)
}

/// A surface blended from `parts`, whose fractions must sum to 1.
pub fn surface(label: &str, parts: &[Part]) -> Result<Material, Error> {
    let total: f64 = parts.iter().map(|p| p.fraction).sum();
    if parts.is_empty()
        || (total - 1.0).abs() > 1e-9
        || parts.iter().any(|p| {
            !(p.fraction > 0.0
                && p.depth_m >= 0.0
                && p.depth_m.is_finite()
                && (0.0..=1.0).contains(&p.relief_share))
        })
    {
        return Err(Error::InvalidMaterial(format!(
            "{label}: parts need positive fractions summing to 1 and a finite depth"
        )));
    }
    let mut absorption: Bands = [0.0; NUM_BANDS];
    let mut scattering: Bands = [0.0; NUM_BANDS];
    for k in 0..NUM_BANDS {
        let row_band = k.clamp(1, 6) - 1;
        let mut reflected = 0.0;
        for p in parts {
            let a = p.row.absorption[row_band];
            absorption[k] += p.fraction * a;
            reflected += p.fraction * (1.0 - a);
            let s = p.relief_share * rule_scattering(p.depth_m, k)
                + (1.0 - p.relief_share) * rule_scattering(0.0, k);
            scattering[k] += p.fraction * (1.0 - a) * s;
        }
        let rule = if reflected > 0.0 {
            scattering[k] / reflected
        } else {
            RULE_FLOOR
        };
        scattering[k] = with_detail_scattering(rule);
    }
    let described: Vec<String> = parts
        .iter()
        .map(|p| {
            if p.depth_m > 0.0 && p.relief_share < 1.0 {
                format!(
                    "{:.0} % {} (relief {} m deep over {:.0} %)",
                    100.0 * p.fraction,
                    p.row.name,
                    p.depth_m,
                    100.0 * p.relief_share
                )
            } else if p.depth_m > 0.0 {
                format!(
                    "{:.0} % {} ({} m deep)",
                    100.0 * p.fraction,
                    p.row.name,
                    p.depth_m
                )
            } else {
                format!("{:.0} % {}", 100.0 * p.fraction, p.row.name)
            }
        })
        .collect();
    Material::new(
        format!("{label}: {}", described.join(", ")),
        absorption,
        scattering,
    )
}

/// The research sources of `parts`, for a sidecar.
pub fn sources(parts: &[Part]) -> Vec<&'static str> {
    let mut out: Vec<&'static str> = parts.iter().map(|p| p.row.source).collect();
    out.dedup();
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_blend_weights_absorption_by_area_and_scattering_by_reflected_energy() {
        let wall = surface(
            "wall with window",
            &[part(PLASTER, 0.75, 0.0), part(GLASS, 0.25, 0.0)],
        )
        .unwrap();
        // 125 Hz: 0.75·0.02 + 0.25·0.35; 63 Hz holds the 125 Hz value.
        assert!((wall.absorption[1] - 0.1025).abs() < 1e-12);
        assert_eq!(wall.absorption[0], wall.absorption[1]);
        // Flat parts scatter at the rule's floor, with detail, whatever their absorption.
        let flat = with_detail_scattering(0.05);
        assert!(wall.scattering.iter().all(|&s| (s - flat).abs() < 1e-12));
        // A deep part dominates only in proportion to what it reflects.
        let seats = surface("seats", &[part(SEATS, 0.5, 0.45), part(HARD, 0.5, 0.0)]).unwrap();
        let k = 4;
        let deep = rule_scattering(0.45, k);
        let blended = (0.5 * (1.0 - 0.84) * deep + 0.5 * (1.0 - 0.03) * 0.05)
            / (0.5 * (1.0 - 0.84) + 0.5 * (1.0 - 0.03));
        assert!((seats.scattering[k] - with_detail_scattering(blended)).abs() < 1e-12);
        assert!((0.6..0.7).contains(&deep), "{deep}");
        // Relief covers its share; the rest of the part scatters at the floor.
        let piers = surface("piers", &[relief(BRICK, 1.0, 0.6)]).unwrap();
        let blended = RELIEF_SHARE * rule_scattering(0.6, 6) + (1.0 - RELIEF_SHARE) * 0.05;
        assert!((piers.scattering[6] - with_detail_scattering(blended)).abs() < 1e-12);
        assert!(surface("bad", &[part(HARD, 0.5, 0.0)]).is_err());
    }

    #[test]
    fn detail_scattering_combines_with_the_rule_as_an_independent_mechanism() {
        // Half of what the rule leaves specular is scattered: 0.05 becomes 0.525, 0.6 becomes 0.8.
        assert!((with_detail_scattering(0.05) - 0.525).abs() < 1e-12);
        assert!((with_detail_scattering(0.6) - 0.8).abs() < 1e-12);
        // Never above the rule's ceiling, and never below where the rule alone put it.
        assert_eq!(with_detail_scattering(RULE_CEILING), RULE_CEILING);
        for i in 0..=100 {
            let rule = RULE_FLOOR + (RULE_CEILING - RULE_FLOOR) * f64::from(i) / 100.0;
            let s = with_detail_scattering(rule);
            assert!(s >= rule && s <= RULE_CEILING, "{rule} -> {s}");
        }
        // A deep surface already at the ceiling at 16 kHz stays there.
        let pews = surface("pews", &[part(PEWS, 1.0, 0.3)]).unwrap();
        assert_eq!(pews.scattering[8], RULE_CEILING);
    }
}
