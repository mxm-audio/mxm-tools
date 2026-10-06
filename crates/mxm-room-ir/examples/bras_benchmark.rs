//! V8, the measured benchmark (plan R4): renders the BRAS reference scenes and compares each with
//! its measurement, octave by octave, the way the round robin compared simulators.
//!
//! **Inputs are BRAS's, as published**, read from the research repository and never copied here
//! (CC BY-SA 4.0): the measured impulse responses, the positions and dimensions on the scenes'
//! drawings, the surface data (third-octave absorption and scattering, averaged into octaves) and
//! the Genelec 8020c directivity, reduced to octaves by
//! `research:sources/room-acoustics-simulation/bras/derive_genelec_octaves.py`. Nothing is fitted
//! to a measurement. Geometry simplifications are named where the scene is built.
//!
//! **Comparison.** Every response is cut from 3 ms before its first arrival to 43 ms after, with a
//! 3 ms Hann fade-in and a 10 ms fade-out (Brinkmann et al., JASA 145, 2019, §II D). Time is
//! absolute on both sides, because the measurements start at emission. Octave levels are compared
//! two ways:
//! - **level**, with the source calibrated as BRAS states it (80 dB SPL at 2 m on axis at 1 kHz,
//!   so 0.4 Pa·m);
//! - **shape**, with each response normalised to its own energy in the gated bands, 250 Hz–4 kHz.
//!   Below that the chambers and the loudspeaker data are not valid; above it the surface data are
//!   extrapolated.
//!
//! A band gates only where its centre lies inside the published valid range of every surface
//! dataset shaping the response (BRAS documentation, Table 5): the tube-measured data end at
//! 4 kHz, and the absorber's angle-dependent data start at 300 Hz. A gated group passes when, in
//! every gated band, the median shape error over its responses is within one JND of 1 dB. The round robin publishes spectra, not a numeric spread for these
//! scenes, so the JND is the gate (plan revision 12). Level errors are reported, not gated.
//!
//! ```bash
//! cargo run -p mxm-room-ir --release --example bras_benchmark
//! ```
//!
//! Set `MXM_RESEARCH_DIR` when the research checkout is not beside this repository; since the split
//! (2026-10-06) it is kept in the private archive, not beside mxm-tools. The report goes to
//! `target/mxm-room-ir/bras-benchmark.md`; the process exits 1 if a gate fails. `BRAS_ONLY=RS5`
//! runs only the groups whose name starts with it, and `BRAS_DIFFRACTION=0` renders without edge
//! diffraction, to bisect a mechanism.

use std::error::Error;
use std::f64::consts::PI;
use std::fmt::Write as _;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::Instant;

use mxm_room_ir::analysis::band_energies;
use mxm_room_ir::bands::{NOMINAL_CENTRES_HZ, NUM_BANDS};
use mxm_room_ir::diffraction::DiffractionOptions;
use mxm_room_ir::directivity::{Array, Directivity, DirectivityTable, Frame};
use mxm_room_ir::geometry::Prism;
use mxm_room_ir::rays::RayOptions;
use mxm_room_ir::render::RenderOptions;
use mxm_room_ir::simulation::{SimulationOptions, render_set, simulate};
use mxm_room_ir::{Air, ImageSourceOptions, Material, Room, Scene, Vec3, wav};

type Result<T> = std::result::Result<T, Box<dyn Error>>;

const FS: u32 = 44_100;
/// BRAS's calibration: 80 dB SPL (0.2 Pa) at 2 m on axis, at 1 kHz.
const SOURCE_PA_M: f64 = 0.4;
/// 250 Hz to 4 kHz.
const GATED: [usize; 5] = [2, 3, 4, 5, 6];
const JND_DB: f64 = 1.0;
/// The hemi-anechoic chamber as the scenes use it: a tiled floor, everything else absorbing. The
/// drawings put the scenes' axis at y = 2.985 m.
const HEMI: Vec3 = Vec3::new(11.5, 5.97, 5.0);
const AXIS_Y: f64 = 2.985;

struct Case {
    label: String,
    scene: Scene,
    measured: PathBuf,
}

struct Group {
    name: String,
    gated: bool,
    note: &'static str,
    /// The published valid range of every surface dataset shaping the response, Hz.
    valid_hz: (f64, f64),
    cases: Vec<Case>,
}

struct Outcome {
    label: String,
    paths: usize,
    level: [f64; NUM_BANDS],
    shape: [f64; NUM_BANDS],
}

struct Inputs {
    bras: PathBuf,
    genelec: Arc<DirectivityTable>,
}

fn chamber_air(temperature_c: f64, relative_humidity_pct: f64) -> Air {
    Air {
        temperature_c,
        relative_humidity_pct,
        ..Air::standard()
    }
}

/// A loudspeaker aim as the drawings give it: azimuth φ from +x toward +y, elevation θ.
fn aimed(azimuth_deg: f64, elevation_deg: f64) -> Frame {
    let (p, t) = (azimuth_deg.to_radians(), elevation_deg.to_radians());
    Frame::pointing(Vec3::new(t.cos() * p.cos(), t.cos() * p.sin(), t.sin()))
}

/// Angle from the normal of a specular reflection off the plane through the origin with unit
/// `normal`, degrees.
fn incidence_deg(source: Vec3, receiver: Vec3, normal: Vec3, plane_offset: f64) -> f64 {
    let image = source - normal * (2.0 * (normal.dot(source) - plane_offset));
    let d = receiver - image;
    (normal.dot(d).abs() / d.length()).acos().to_degrees()
}

fn anechoic6(floor: Material) -> [Material; 6] {
    let a = Material::anechoic;
    [a(), a(), a(), a(), floor, a()]
}

impl Inputs {
    fn load() -> Result<Self> {
        let research = std::env::var_os("MXM_RESEARCH_DIR")
            .map(PathBuf::from)
            .unwrap_or_else(|| {
                PathBuf::from(env!("CARGO_MANIFEST_DIR"))
                    .join("../../../01-mxm-collection-research")
            });
        let bras = research.join("sources/room-acoustics-simulation/bras");
        let text =
            std::fs::read_to_string(bras.join("genelec-8020c-octaves.txt")).map_err(|e| {
                format!(
                    "{}: {e}; set MXM_RESEARCH_DIR to the research checkout",
                    bras.display()
                )
            })?;
        // BRAS's front pole grid: polar angle T from forward, rotation P from up toward left, 1°.
        let (rings, rotations) = (181, 360);
        let mut gains = vec![[f64::NAN; NUM_BANDS]; rings * rotations];
        for line in text
            .lines()
            .filter(|l| !l.starts_with('#') && !l.trim().is_empty())
        {
            let v: Vec<f64> = line
                .split_whitespace()
                .map(str::parse)
                .collect::<std::result::Result<_, _>>()?;
            if v.len() != 2 + NUM_BANDS {
                return Err(format!("directivity line `{line}`").into());
            }
            let (t, p) = (v[0] as usize, v[1] as usize);
            let g: [f64; NUM_BANDS] = std::array::from_fn(|k| 10f64.powf(v[2 + k] / 20.0));
            if t == 0 || t == 180 {
                gains[t * rotations..(t + 1) * rotations].fill(g);
            } else {
                gains[t * rotations + p] = g;
            }
        }
        if gains.iter().flatten().any(|g| g.is_nan()) {
            return Err("the Genelec directivity grid is incomplete".into());
        }
        let genelec = Arc::new(DirectivityTable::new(
            "Genelec 8020c, BRAS v3, octaves",
            rings,
            rotations,
            gains,
        )?);
        Ok(Self { bras, genelec })
    }

    /// A BRAS surface as octaves: each octave the mean of its three third-octaves.
    fn material(&self, file: &str) -> Result<Material> {
        let path = self.bras.join("materials").join(format!("{file}.csv"));
        let text = std::fs::read_to_string(&path)?;
        let rows: Vec<Vec<f64>> = text
            .lines()
            .filter(|l| !l.trim().is_empty())
            .map(|l| {
                l.split(',')
                    .map(str::trim)
                    .filter(|s| !s.is_empty())
                    .map(str::parse)
                    .collect::<std::result::Result<_, _>>()
            })
            .collect::<std::result::Result<_, _>>()?;
        if rows.len() < 3 || rows[1].len() != 31 || rows[2].len() != 31 {
            return Err(format!("{}: expected 31 thirds on lines 2 and 3", path.display()).into());
        }
        // Third-octaves from 20 Hz: the 63 Hz octave is 50, 63 and 80 Hz, indices 4 to 6.
        let octaves = |row: &[f64]| {
            std::array::from_fn(|k| row[4 + 3 * k..7 + 3 * k].iter().sum::<f64>() / 3.0)
        };
        Ok(Material::new(file, octaves(&rows[1]), octaves(&rows[2]))?)
    }

    fn case(
        &self,
        label: String,
        room: &Room,
        air: Air,
        (source, aim): (Vec3, Frame),
        receiver: Vec3,
        measured: &str,
    ) -> Result<Case> {
        let scene = Scene::new(label.clone(), room.clone(), air, source, receiver)?
            .with_source_directivity(Directivity::Tabulated {
                table: Arc::clone(&self.genelec),
                frame: aim,
            })?;
        Ok(Case {
            label,
            scene,
            measured: self.bras.join(measured),
        })
    }
}

/// RS1: the floor alone, then a RockFon absorber laid on it. The absorber's scene sits 20 mm higher
/// on the drawing; here everything is lowered by that, so the absorber's face is the floor.
fn rs1(inputs: &Inputs) -> Result<Vec<Group>> {
    let air = || chamber_air(20.3, 41.5);
    let tiles = inputs.material("mat_Tiles")?;
    let ls = [
        (2.902, 1.500, -30.0),
        (3.379, 2.121, -45.0),
        (4.000, 2.598, -60.0),
    ];
    let mp = [
        (8.098, 1.500),
        (7.621, 2.121),
        (7.000, 2.598),
        (8.098, 1.573),
    ];
    let rigid_room = Room::box_with_prisms(Vec3::default(), HEMI, anechoic6(tiles.clone()), &[])?;
    let angles = ["00", "30", "45", "60"];
    let rockfon: Vec<Material> = angles
        .iter()
        .map(|a| inputs.material(&format!("mat_RockFonSonarG_{a}deg")))
        .collect::<Result<_>>()?;
    let (xs, ys) = ([0.0, 3.397, 7.597, HEMI.x], [0.0, 0.881, 5.081, HEMI.y]);
    let patched = |patch: &Material| -> Result<Room> {
        let mut regions = Vec::new();
        for i in 0..3 {
            for j in 0..3 {
                let floor = if i == 1 && j == 1 { patch } else { &tiles };
                regions.push((
                    vec![
                        (xs[i], ys[j]),
                        (xs[i + 1], ys[j]),
                        (xs[i + 1], ys[j + 1]),
                        (xs[i], ys[j + 1]),
                    ],
                    [floor.clone(), Material::anechoic(), Material::anechoic()],
                ));
            }
        }
        Ok(Room::extruded_regions(&regions, HEMI.z)?)
    };
    let (mut rigid, mut absorbing) = (Vec::new(), Vec::new());
    for (i, &(lx, lz, elevation)) in ls.iter().enumerate() {
        let source = Vec3::new(lx, AXIS_Y, lz);
        for (j, &(mx, mz)) in mp.iter().enumerate() {
            let receiver = Vec3::new(mx, AXIS_Y, mz);
            let label = format!("LS{}-MP{}", i + 1, j + 1);
            if j < 3 || i == 1 {
                rigid.push(inputs.case(
                    label.clone(),
                    &rigid_room,
                    air(),
                    (source, aimed(0.0, elevation)),
                    receiver,
                    &format!("RS1/RS1_RIR_Rigid_LS{}_MP{}.wav", i + 1, j + 1),
                )?);
            }
            if j < 3 {
                // BRAS measured the absorber at 0°, 30°, 45° and 60°: take the nearest to this path.
                let angle = incidence_deg(source, receiver, Vec3::new(0.0, 0.0, 1.0), 0.0);
                let nearest = [0.0, 30.0, 45.0, 60.0]
                    .iter()
                    .enumerate()
                    .min_by(|a, b| (a.1 - angle).abs().total_cmp(&(b.1 - angle).abs()))
                    .map_or(0, |(k, _)| k);
                absorbing.push(inputs.case(
                    format!("{label} (absorber at {}°)", angles[nearest]),
                    &patched(&rockfon[nearest])?,
                    air(),
                    (source, aimed(0.0, elevation)),
                    receiver,
                    &format!("RS1/RS1_RIR_Absorbing_LS{}_MP{}.wav", i + 1, j + 1),
                )?);
            }
        }
    }
    Ok(vec![
        Group {
            name: "RS1 single reflection, tiled floor".into(),
            gated: true,
            note: "specular reflection with scattering",
            // The tiles' data are estimates with no stated range.
            valid_hz: (0.0, f64::INFINITY),
            cases: rigid,
        },
        Group {
            name: "RS1 single reflection, RockFon absorber".into(),
            gated: true,
            note: "angle-dependent absorption as published; the absorber's 20 mm step at its edges is ignored",
            valid_hz: (300.0, 15_000.0),
            cases: absorbing,
        },
    ])
}

/// RS2: square MDF plates of 1 m and 2 m, bare and faced with absorber, in the free field. The
/// plate is 25 mm thick with its face at x = 0; the stand is left out, as BRAS allows.
fn rs2(inputs: &Inputs) -> Result<Vec<Group>> {
    let air = || chamber_air(17.6, 47.0);
    let mdf = inputs.material("mat_MDF25mmA_plane_00deg")?;
    let rockfon: Vec<Material> = ["00", "30", "45", "60"]
        .iter()
        .map(|a| inputs.material(&format!("mat_RockFonSonarG_{a}deg")))
        .collect::<Result<_>>()?;
    let ls = [
        (3.46, 2.00, -150.0),
        (2.83, 2.83, -135.0),
        (2.00, 3.46, -120.0),
        (3.46, -2.00, 150.0),
        (2.83, -2.83, 135.0),
        (2.00, -3.46, 120.0),
    ];
    let mp = [
        (3.46, -2.00),
        (2.83, -2.83),
        (2.00, -3.46),
        (-0.71, -0.71),
        (-0.71, 0.71),
    ];
    let z = 1.02;
    let mut groups = Vec::new();
    for (size, half) in [("1m", 0.5), ("2m", 1.0)] {
        for surface in ["rigid", "absorbing"] {
            let mut cases = Vec::new();
            for (i, &(lx, ly, azimuth)) in ls.iter().enumerate() {
                for (j, &(mx, my)) in mp.iter().enumerate() {
                    let front = i < 3 && j < 3;
                    let behind = i >= 3 && j >= 3 && !(size == "2m" && surface == "absorbing");
                    if !(front || behind) {
                        continue;
                    }
                    let (source, receiver) = (Vec3::new(lx, ly, z), Vec3::new(mx, my, z));
                    let (material, label) = if surface == "rigid" {
                        (mdf.clone(), format!("LS{}-MP{}", i + 1, j + 1))
                    } else {
                        let angle = incidence_deg(source, receiver, Vec3::new(1.0, 0.0, 0.0), 0.0);
                        let k = ((angle / 15.0).round() as usize).clamp(0, 4);
                        let k = [0, 1, 1, 2, 3][k];
                        (
                            rockfon[k].clone(),
                            format!(
                                "LS{}-MP{} (absorber at {}°)",
                                i + 1,
                                j + 1,
                                [0, 30, 45, 60][k]
                            ),
                        )
                    };
                    let plate = Prism {
                        section: vec![
                            (-0.025, z - half),
                            (0.0, z - half),
                            (0.0, z + half),
                            (-0.025, z + half),
                        ],
                        y: (-half, half),
                        material,
                    };
                    let room = Room::box_with_prisms(
                        Vec3::new(-2.0, -4.5, -1.0),
                        Vec3::new(4.5, 4.5, 4.0),
                        anechoic6(Material::anechoic()),
                        &[plate],
                    )?;
                    cases.push(inputs.case(
                        label,
                        &room,
                        air(),
                        (source, aimed(azimuth, 0.0)),
                        receiver,
                        &format!(
                            "RS2/RS2_RIR_{size}Plate_{surface}_LS{}_MP{}.wav",
                            i + 1,
                            j + 1
                        ),
                    )?);
                }
            }
            groups.push(Group {
                name: format!("RS2 finite plate, {size}, {surface}"),
                gated: true,
                note: "specular reflection and first-order edge diffraction; transmission ignored",
                valid_hz: if surface == "rigid" {
                    (100.0, 4_000.0)
                } else {
                    (300.0, 15_000.0)
                },
                cases,
            });
        }
    }
    Ok(groups)
}

/// RS3: two 2 m plates 10 m apart. Within the 46 ms window: the direct sound and one reflection
/// off each plate.
fn rs3(inputs: &Inputs) -> Result<Vec<Group>> {
    let mdf = inputs.material("mat_MDF25mmA_plane_00deg")?;
    let plate = |x0: f64| Prism {
        section: vec![
            (x0, 0.02),
            (x0 + 0.025, 0.02),
            (x0 + 0.025, 2.02),
            (x0, 2.02),
        ],
        y: (-1.0, 1.0),
        material: mdf.clone(),
    };
    let room = Room::box_with_prisms(
        Vec3::new(-1.0, -2.0, -1.0),
        Vec3::new(11.0, 2.0, 4.0),
        anechoic6(Material::anechoic()),
        &[plate(-0.025), plate(10.0)],
    )?;
    Ok(vec![Group {
        name: "RS3 parallel plates, first window".into(),
        gated: true,
        note: "46 ms: direct sound and the first reflection off each plate; the flutter's decay is not compared",
        valid_hz: (100.0, 4_000.0),
        cases: vec![inputs.case(
            "LS1-MP1".into(),
            &room,
            chamber_air(17.3, 49.5),
            (Vec3::new(3.0, 0.0, 1.02), aimed(0.0, 0.0)),
            Vec3::new(7.0, 0.0, 1.02),
            "RS3/RS3_RIR_LS1_MP1.wav",
        )?],
    }])
}

/// RS5: a 2.066 m MDF partition, 4.75 m wide and 25 mm thick, on the tiled floor. Modelled as a
/// knife edge tapering from 25 mm at the floor to its crest: v1 diffracts once, and a flat 25 mm
/// top would need two diffractions to reach a shadowed microphone.
fn rs5(inputs: &Inputs) -> Result<Vec<Group>> {
    let partition = Prism {
        section: vec![(5.487, 0.0), (5.4995, 2.066), (5.512, 0.0)],
        y: (AXIS_Y - 2.375, AXIS_Y + 2.375),
        material: inputs.material("mat_MDF25mmB_plane_00deg")?,
    };
    let room = Room::box_with_prisms(
        Vec3::default(),
        HEMI,
        anechoic6(inputs.material("mat_Tiles")?),
        &[partition],
    )?;
    let ls = [(1.235, 0.0), (2.0, 0.0), (3.0, 0.0), (0.135, 34.6)];
    let mp = [1.235, 0.006, 2.0, 3.0];
    let mut cases = Vec::new();
    for (i, &(lz, elevation)) in ls.iter().enumerate() {
        for (j, &mz) in mp.iter().enumerate() {
            cases.push(inputs.case(
                format!("LS{}-MP{}", i + 1, j + 1),
                &room,
                chamber_air(20.3, 40.3),
                (Vec3::new(2.487, AXIS_Y, lz), aimed(0.0, elevation)),
                Vec3::new(8.512, AXIS_Y, mz),
                &format!("RS5/RS5_RIR_LS{}_MP{}.wav", i + 1, j + 1),
            )?);
        }
    }
    Ok(vec![Group {
        name: "RS5 partition".into(),
        gated: true,
        note: "first-order diffraction over the crest and around the ends, with floor reflections; transmission ignored",
        valid_hz: (100.0, 4_000.0),
        cases,
    }])
}

/// RS6: a 0.72 × 0.72 × 4.14 m body on the floor. Only the loudspeaker and microphone that see each
/// other over it gate: the others hear the body's top through two edges.
fn rs6(inputs: &Inputs) -> Result<Vec<Group>> {
    let body = Prism {
        section: vec![(5.14, 0.0), (5.86, 0.0), (5.86, 0.72), (5.14, 0.72)],
        y: (AXIS_Y - 2.07, AXIS_Y + 2.07),
        material: inputs.material("mat_MDF12mm_plane_00deg")?,
    };
    let room = Room::box_with_prisms(
        Vec3::default(),
        HEMI,
        anechoic6(inputs.material("mat_Tiles")?),
        &[body],
    )?;
    let heights = [0.176, 0.4, 0.8];
    let mics = [0.006, 0.4, 0.8];
    let (mut lit, mut shadowed) = (Vec::new(), Vec::new());
    for (i, &lz) in heights.iter().enumerate() {
        for (j, &mz) in mics.iter().enumerate() {
            let case = inputs.case(
                format!("LS{}-MP{}", i + 1, j + 1),
                &room,
                chamber_air(19.9, 40.1),
                (Vec3::new(2.487, AXIS_Y, lz), aimed(0.0, 0.0)),
                Vec3::new(8.512, AXIS_Y, mz),
                &format!("RS6/RS6_RIR_LS{}_MP{}.wav", i + 1, j + 1),
            )?;
            if room.segment_blocked(case.scene.source, case.scene.receiver, None, None) {
                shadowed.push(case);
            } else {
                lit.push(case);
            }
        }
    }
    Ok(vec![
        Group {
            name: "RS6 finite body, in sight".into(),
            gated: false,
            note: "sound grazes 8 cm over the body's 0.72 m top: double-edge diffraction and specular reflection off a top narrower than the wavelength, which v1 does not represent (at RS2's plate the round robin found no algorithm that modelled grazing incidence)",
            valid_hz: (100.0, 4_000.0),
            cases: lit,
        },
        Group {
            name: "RS6 finite body, shadowed".into(),
            gated: false,
            note: "second-order diffraction over the body's top, which v1 does not represent",
            valid_hz: (100.0, 4_000.0),
            cases: shadowed,
        },
    ])
}

/// RS7: fifteen 0.12 m deep, 0.24 m high blocks at a 0.34 m pitch, 4.1 m wide.
fn rs7(inputs: &Inputs) -> Result<Vec<Group>> {
    let mdf = inputs.material("mat_MDF12mm_plane_00deg")?;
    let blocks: Vec<Prism> = (0..15)
        .map(|k| {
            let x = 3.958 + 0.34 * k as f64;
            Prism {
                section: vec![
                    (x - 0.06, 0.0),
                    (x + 0.06, 0.0),
                    (x + 0.06, 0.24),
                    (x - 0.06, 0.24),
                ],
                y: (AXIS_Y - 2.05, AXIS_Y + 2.05),
                material: mdf.clone(),
            }
        })
        .collect();
    let room = Room::box_with_prisms(
        Vec3::default(),
        HEMI,
        anechoic6(inputs.material("mat_Tiles")?),
        &blocks,
    )?;
    let ls = [(0.176, 0.0), (0.520, -10.0)];
    let mp = [
        (9.014, 0.337),
        (9.014, 0.400),
        (6.228, 0.337),
        (6.228, 0.400),
    ];
    let mut cases = Vec::new();
    for (i, &(lz, elevation)) in ls.iter().enumerate() {
        for (j, &(mx, mz)) in mp.iter().enumerate() {
            cases.push(inputs.case(
                format!("LS{}-MP{}", i + 1, j + 1),
                &room,
                chamber_air(19.2, 40.3),
                (Vec3::new(3.187, AXIS_Y, lz), aimed(0.0, elevation)),
                Vec3::new(mx, AXIS_Y, mz),
                &format!("RS7/RS7_RIR_LS{}_MP{}.wav", i + 1, j + 1),
            )?);
        }
    }
    Ok(vec![Group {
        name: "RS7 seat dip".into(),
        gated: false,
        note: "multiple diffraction over the rows, which v1 does not represent (V8)",
        valid_hz: (100.0, 4_000.0),
        cases,
    }])
}

/// A 46 ms cut starting `start_s` into `signal`: 3 ms Hann in, 10 ms Hann out.
fn window(signal: &[f64], start_s: f64) -> Vec<f64> {
    let fs = f64::from(FS);
    let (length, fade_in, fade_out) = (0.046, 0.003, 0.010);
    let first = (start_s * fs).round() as isize;
    (0..(length * fs).round() as usize)
        .map(|i| {
            let t = i as f64 / fs;
            let w = if t < fade_in {
                0.5 - 0.5 * (PI * t / fade_in).cos()
            } else if t > length - fade_out {
                0.5 + 0.5 * (PI * (t - (length - fade_out)) / fade_out).cos()
            } else {
                1.0
            };
            let j = first + i as isize;
            usize::try_from(j)
                .ok()
                .and_then(|j| signal.get(j))
                .map_or(0.0, |v| v * w)
        })
        .collect()
}

fn levels_db(energy: &[f64; NUM_BANDS]) -> [f64; NUM_BANDS] {
    std::array::from_fn(|k| 10.0 * energy[k].max(1e-30).log10())
}

fn gated_db(energy: &[f64; NUM_BANDS]) -> f64 {
    10.0 * GATED
        .iter()
        .map(|&k| energy[k])
        .sum::<f64>()
        .max(1e-30)
        .log10()
}

fn compare(case: &Case) -> Result<Outcome> {
    let options = SimulationOptions {
        image_sources: ImageSourceOptions {
            max_order: 2,
            ..Default::default()
        },
        rays: RayOptions {
            rays: 20_000,
            max_time_s: 0.3,
            ..Default::default()
        },
        // BRAS_DIFFRACTION=0 renders without diffraction, to bisect a mechanism.
        diffraction: (std::env::var("BRAS_DIFFRACTION").as_deref() != Ok("0")).then_some(
            DiffractionOptions {
                specular_order: 2,
                sample_rate: f64::from(FS),
            },
        ),
        wave: None,
    };
    let render = RenderOptions {
        // BRAS levels are absolute pascals: render physically.
        normalize_peak_dbfs: None,
        sample_rate: FS,
        ..RenderOptions::default()
    };
    let sim = simulate(&case.scene, &options)?;
    let rendered = render_set(&[&sim], &Array::mono().placed(&Frame::WORLD)?, &render)?.remove(0);
    let fs = f64::from(FS);
    let onset = rendered.origin_path_length_m / case.scene.air.speed_of_sound();
    // The render's sample `lead_samples` is the first arrival; the measurement's sample 0 is emission.
    let render_zero = onset - rendered.lead_samples as f64 / fs;
    let simulated: Vec<f64> = rendered
        .samples()
        .iter()
        .map(|&v| f64::from(v) * SOURCE_PA_M)
        .collect();
    let file =
        wav::read(&case.measured).map_err(|e| format!("{}: {e}", case.measured.display()))?;
    if file.sample_rate != FS {
        return Err(format!("{}: {} Hz", case.measured.display(), file.sample_rate).into());
    }
    let measured: Vec<f64> = file.channels[0].iter().map(|&v| f64::from(v)).collect();
    let start = onset - 0.003;
    let sim_energy = band_energies(&window(&simulated, start - render_zero), fs);
    let meas_energy = band_energies(&window(&measured, start), fs);
    let (ls, lm) = (levels_db(&sim_energy), levels_db(&meas_energy));
    let (ns, nm) = (gated_db(&sim_energy), gated_db(&meas_energy));
    Ok(Outcome {
        label: case.label.clone(),
        paths: sim.diffracted.len(),
        level: std::array::from_fn(|k| ls[k] - lm[k]),
        shape: std::array::from_fn(|k| (ls[k] - ns) - (lm[k] - nm)),
    })
}

fn median(mut v: Vec<f64>) -> f64 {
    v.sort_by(f64::total_cmp);
    let n = v.len();
    if n == 0 {
        f64::NAN
    } else if n % 2 == 1 {
        v[n / 2]
    } else {
        0.5 * (v[n / 2 - 1] + v[n / 2])
    }
}

fn row(label: &str, values: impl Iterator<Item = f64>) -> String {
    let mut s = format!("| {label} |");
    for v in values {
        let _ = write!(s, " {v:+.1} |");
    }
    s
}

fn main() -> Result<()> {
    let started = Instant::now();
    let inputs = Inputs::load()?;
    let mut groups = Vec::new();
    for build in [rs1, rs2, rs3, rs5, rs6, rs7] {
        groups.extend(build(&inputs)?);
    }
    let header = {
        let mut s = String::from("| |");
        for c in NOMINAL_CENTRES_HZ {
            let _ = write!(
                s,
                " {} |",
                if c >= 1000.0 {
                    format!("{}k", c / 1000.0)
                } else {
                    format!("{c}")
                }
            );
        }
        s.push_str("\n|---|");
        s.push_str(&"---:|".repeat(NUM_BANDS));
        s
    };
    let mut report = String::from(
        "# BRAS reference scenes against measurement (V8)\n\nGenerated by `examples/bras_benchmark.rs`. \
         Shape: octave level re the response's own 250 Hz–4 kHz energy, simulated minus measured, dB. \
         Level: absolute octave level, simulated minus measured, dB, with BRAS's source calibration. \
         A gated group passes when every band from 250 Hz to 4 kHz has a median |shape| within 1 JND (1 dB).\n",
    );
    let mut failed = Vec::new();
    let only = std::env::var("BRAS_ONLY").unwrap_or_default();
    for group in groups.iter().filter(|g| g.name.starts_with(&only)) {
        let t = Instant::now();
        let outcomes: Vec<Outcome> = group.cases.iter().map(compare).collect::<Result<_>>()?;
        let abs_median: [f64; NUM_BANDS] =
            std::array::from_fn(|k| median(outcomes.iter().map(|o| o.shape[k].abs()).collect()));
        let abs_max: [f64; NUM_BANDS] = std::array::from_fn(|k| {
            outcomes
                .iter()
                .map(|o| o.shape[k].abs())
                .fold(0.0, f64::max)
        });
        let level_median: [f64; NUM_BANDS] =
            std::array::from_fn(|k| median(outcomes.iter().map(|o| o.level[k]).collect()));
        let gated_bands: Vec<usize> = GATED
            .iter()
            .copied()
            .filter(|&k| (group.valid_hz.0..=group.valid_hz.1).contains(&NOMINAL_CENTRES_HZ[k]))
            .collect();
        let pass = gated_bands.iter().all(|&k| abs_median[k] <= JND_DB);
        let gated_list = if group.gated {
            gated_bands
                .iter()
                .map(|&k| format!("{} Hz", NOMINAL_CENTRES_HZ[k]))
                .collect::<Vec<_>>()
                .join(", ")
        } else {
            "none".to_string()
        };
        let verdict = match (group.gated, pass) {
            (true, true) => "gated: pass",
            (true, false) => "gated: FAIL",
            (false, _) => "reported, not gated",
        };
        if group.gated && !pass {
            failed.push(group.name.clone());
        }
        println!(
            "{} ({} responses, {} diffracted paths, {verdict}, {:.1?})\n  median |shape| 250 Hz–4 kHz: {}\n  median level 250 Hz–4 kHz: {}",
            group.name,
            outcomes.len(),
            outcomes.iter().map(|o| o.paths).sum::<usize>(),
            t.elapsed(),
            GATED
                .iter()
                .map(|&k| format!("{:.1}", abs_median[k]))
                .collect::<Vec<_>>()
                .join(" "),
            GATED
                .iter()
                .map(|&k| format!("{:+.1}", level_median[k]))
                .collect::<Vec<_>>()
                .join(" "),
        );
        let _ = write!(
            report,
            "\n## {} — {verdict}\n\n{} responses; {}. Gated bands: {}.\n\n{header}\n{}\n{}\n{}\n",
            group.name,
            outcomes.len(),
            group.note,
            gated_list,
            row("median \\|shape\\|", abs_median.iter().copied()),
            row("max \\|shape\\|", abs_max.iter().copied()),
            row("median level", level_median.iter().copied()),
        );
        let _ = writeln!(
            report,
            "\n<details><summary>Per response (shape, then level)</summary>\n\n{header}"
        );
        for o in &outcomes {
            let _ = writeln!(
                report,
                "{}",
                row(&format!("{} shape", o.label), o.shape.iter().copied())
            );
            let _ = writeln!(
                report,
                "{}",
                row(&format!("{} level", o.label), o.level.iter().copied())
            );
        }
        report.push_str("\n</details>\n");
    }
    let out = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../target/mxm-room-ir/bras-benchmark.md");
    std::fs::create_dir_all(out.parent().expect("has a parent"))?;
    std::fs::write(&out, &report)?;
    println!("wrote {} in {:.1?}", out.display(), started.elapsed());
    if failed.is_empty() {
        println!("V8: every gated group passes");
        Ok(())
    } else {
        println!("V8: FAIL in {}", failed.join("; "));
        std::process::exit(1);
    }
}
