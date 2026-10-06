//! R3's proof of solver agreement, V3: across the transition band the geometric early field and the
//! wave solver's agree in time and phase (with edge diffraction where an edge is exposed), the merged
//! response has no crossover notch or peak, and band decays agree across the seam.
//!
//! Rooms are sized to whole wave-solver cells, so the staircase lies exactly on the walls and the
//! residual is the solvers' own. The measured agreement is printed, and the thresholds sit below it
//! with a margin, not at it.

use std::f64::consts::PI;

use mxm_room_ir::analysis::analyse_f64;
use mxm_room_ir::catalogue::ARCHETYPES;
use mxm_room_ir::complex::C64;
use mxm_room_ir::diffraction::DiffractionOptions;
use mxm_room_ir::directivity::{Array, Frame};
use mxm_room_ir::fdtd::WaveGrid;
use mxm_room_ir::rays::RayOptions;
use mxm_room_ir::render::{RenderOptions, render_fields, render_parts};
use mxm_room_ir::simulation::{SimulationOptions, WaveOptions, simulate};
use mxm_room_ir::{Air, ImageSourceOptions, Material, NUM_BANDS, Room, Scene, Vec3};

const FS: f64 = 48_000.0;
const SEAM: f64 = 200.0;
const ACCURACY: f64 = 0.03;

fn spacing() -> f64 {
    let top = SEAM * std::f64::consts::SQRT_2;
    WaveGrid::spacing_for(top / ACCURACY, Air::standard().speed_of_sound())
}

/// A length of `n` whole cells, a hair short so the grid's ceiling lands on `n`.
fn cells(n: f64) -> f64 {
    (n - 1e-9) * spacing()
}

fn material(alpha: f64) -> Material {
    Material::new("wall", [alpha; NUM_BANDS], [0.02; NUM_BANDS]).unwrap()
}

fn options(max_order: usize, max_time_s: f64, diffraction: bool) -> SimulationOptions {
    SimulationOptions {
        image_sources: ImageSourceOptions {
            max_order,
            ..Default::default()
        },
        rays: RayOptions {
            rays: 20_000,
            max_time_s,
            ..Default::default()
        },
        diffraction: diffraction.then(DiffractionOptions::default),
        wave: Some(WaveOptions {
            seam_hz: Some(SEAM),
            accuracy: ACCURACY,
            ..Default::default()
        }),
    }
}

/// Physical level and no air absorption: these tests check closed forms and solver agreement.
fn dry() -> RenderOptions {
    RenderOptions {
        air_absorption: false,
        normalize_peak_dbfs: None,
        ..Default::default()
    }
}

/// Cross-spectral correlation and energy ratio of two signals over frequencies `lo..hi`.
fn band_agreement(a: &[f64], b: &[f64], lo: f64, hi: f64) -> (f64, f64) {
    let spectrum = |x: &[f64], f: f64| {
        x.iter().enumerate().fold(C64::ZERO, |acc, (n, &v)| {
            acc + C64::from_polar(v, -2.0 * PI * f * n as f64 / FS)
        })
    };
    let (mut cross, mut ea, mut eb) = (0.0, 0.0, 0.0);
    let steps = 120;
    for i in 0..=steps {
        let f = lo * (hi / lo).powf(i as f64 / steps as f64);
        let (x, y) = (spectrum(a, f), spectrum(b, f));
        cross += (x * y.conj()).re;
        ea += x.norm_sqr();
        eb += y.norm_sqr();
    }
    (cross / (ea * eb).sqrt(), ea / eb)
}

fn window(x: &[f64], from: usize, length: usize) -> Vec<f64> {
    x[from..from + length]
        .iter()
        .enumerate()
        .map(|(i, v)| {
            // Tukey: flat, with 10 % cosine tapers.
            let t = i as f64 / length as f64;
            let taper = 0.1;
            let w = if t < taper {
                0.5 * (1.0 - (PI * t / taper).cos())
            } else if t > 1.0 - taper {
                0.5 * (1.0 - (PI * (1.0 - t) / taper).cos())
            } else {
                1.0
            };
            v * w
        })
        .collect()
}

/// V3, early field: in a box, image sources with the boundary models' `R(θ)` and the wave
/// solver agree in phase and level across the transition band.
#[test]
fn v3_early_fields_agree_in_the_transition_band() {
    let size = Vec3::new(cells(84.0), cells(64.0), cells(43.0));
    let room = Room::shoebox(size, std::array::from_fn(|_| material(0.12))).unwrap();
    let scene = Scene::new(
        "agreement",
        room,
        Air::standard(),
        Vec3::new(1.37, 1.21, 1.13),
        Vec3::new(3.62, 2.48, 1.52),
    )
    .unwrap();
    let sim = simulate(&scene, &options(10, 0.3, false)).unwrap();
    let wave = sim.wave.as_ref().unwrap();
    let (lo, hi) = wave.transition_hz;
    let mono = Array::mono().placed(&Frame::WORLD).unwrap();
    let parts = render_parts(&[sim.field()], &scene.air, &mono, &dry()).unwrap();
    let field = &parts.fields[0];
    let length = (0.025 * FS) as usize;
    let geometric = window(&field.early[0], parts.lead_samples, length);
    let solved = window(&field.wave.as_ref().unwrap()[0], parts.lead_samples, length);
    let (correlation, ratio) = band_agreement(&geometric, &solved, lo, hi);
    eprintln!(
        "early field {lo:.0}–{hi:.0} Hz: correlation {correlation:.3}, level {:+.2} dB (grid {} cells, h {:.3} m)",
        10.0 * ratio.log10(),
        wave.cells,
        wave.spacing_m
    );
    assert!(correlation > 0.9, "correlation {correlation}");
    assert!((10.0 * ratio.log10()).abs() < 1.5, "level ratio {ratio}");
}

/// V3, exposed edge: behind the re-entrant corner of an L-shaped room, first-order diffraction
/// brings the geometric early field toward the wave solver's. The walls absorb 90 %, so what
/// reaches the shadowed receiver early is mostly sound bent round the corner; in a live room the
/// reflections fill the window and hide whether diffraction is right (measured: diffraction was
/// 12 dB below the wave field there, and its presence moved the correlation by less than 0.02).
///
/// Measured here: correlation 0.59 without diffraction and 0.91 with it. The geometric field is
/// 3.7 dB louder than the wave solver's, and the level is reported, not gated. The line-integral
/// model assumes rigid wedge faces, and these faces absorb; no exact solution for impedance faces
/// is known (research page §7.1).
#[test]
fn v3_diffraction_closes_the_gap_behind_a_corner() {
    let a = cells(96.0);
    let b = cells(40.0);
    let plan = [(0.0, 0.0), (a, 0.0), (a, b), (b, b), (b, a), (0.0, a)];
    let height = cells(40.0);
    let m = || material(0.9);
    let room = Room::extruded(&plan, height, [m(), m(), m()]).unwrap();
    let scene = Scene::new(
        "corner",
        room,
        Air::standard(),
        Vec3::new(a - 1.1, 0.9, 1.1),
        Vec3::new(0.8, a - 1.3, 1.3),
    )
    .unwrap();
    let mono = Array::mono().placed(&Frame::WORLD).unwrap();
    let sims: Vec<_> = [false, true]
        .into_iter()
        .map(|d| simulate(&scene, &options(4, 0.3, d)).unwrap())
        .collect();
    let parts: Vec<_> = sims
        .iter()
        .map(|s| render_parts(&[s.field()], &scene.air, &mono, &dry()).unwrap())
        .collect();
    let (lo, hi) = sims[1].wave.as_ref().unwrap().transition_hz;
    // Diffraction moves time zero earlier (the diffracted arrival precedes every reflection), so
    // both renders are windowed on absolute time from the earlier origin, not from their own.
    let t0 = parts
        .iter()
        .map(|p| p.origin_path_length_m)
        .fold(f64::INFINITY, f64::min);
    let c = scene.air.speed_of_sound();
    let length = (0.03 * FS) as usize;
    let aligned = |p: &mxm_room_ir::render::Parts, x: &[f64]| -> Vec<f64> {
        let shift = ((t0 - p.origin_path_length_m) / c * FS).round() as i64;
        let raw: Vec<f64> = (0..length as i64)
            .map(|i| {
                let k = p.lead_samples as i64 + i + shift;
                if k >= 0 && (k as usize) < x.len() {
                    x[k as usize]
                } else {
                    0.0
                }
            })
            .collect();
        window(&raw, 0, length)
    };
    let agreement = |p: &mxm_room_ir::render::Parts| {
        let f = &p.fields[0];
        band_agreement(
            &aligned(p, &f.early[0]),
            &aligned(p, &f.wave.as_ref().unwrap()[0]),
            lo,
            hi,
        )
    };
    let (without, _) = agreement(&parts[0]);
    let (with, ratio) = agreement(&parts[1]);
    let paths = sims[1].diffracted.len();
    eprintln!(
        "behind the corner: correlation {without:.3} without diffraction, {with:.3} with ({paths} paths), level {:+.2} dB",
        10.0 * ratio.log10()
    );
    assert!(paths > 0);
    assert!(
        with > without + 0.05,
        "diffraction did not help: {without} → {with}"
    );
    assert!(with > 0.6, "correlation with diffraction {with}");
}

/// Band energy in `lo..hi`, by the same spectral sum.
fn band_energy(x: &[f64], lo: f64, hi: f64) -> f64 {
    let steps = 60;
    (0..=steps)
        .map(|i| {
            let f = lo * (hi / lo).powf(i as f64 / steps as f64);
            x.iter()
                .enumerate()
                .fold(C64::ZERO, |acc, (n, &v)| {
                    acc + C64::from_polar(v, -2.0 * PI * f * n as f64 / FS)
                })
                .norm_sqr()
        })
        .sum()
}

/// V3, merge: across the transition band, the merged response's energy lies between the two
/// solvers' own, so the crossover neither notches nor peaks.
#[test]
fn v3_merged_response_has_no_crossover_notch_or_peak() {
    let size = Vec3::new(cells(84.0), cells(64.0), cells(43.0));
    let room = Room::shoebox(size, std::array::from_fn(|_| material(0.15))).unwrap();
    let scene = Scene::new(
        "merge",
        room,
        Air::standard(),
        Vec3::new(1.37, 1.21, 1.13),
        Vec3::new(3.62, 2.48, 1.52),
    )
    .unwrap();
    let sim = simulate(&scene, &options(3, 0.8, false)).unwrap();
    let (lo, hi) = sim.wave.as_ref().unwrap().transition_hz;
    let mono = Array::mono().placed(&Frame::WORLD).unwrap();
    let parts = render_parts(&[sim.field()], &scene.air, &mono, &dry()).unwrap();
    let merged = render_fields(&[sim.field()], &scene.air, &mono, &dry())
        .unwrap()
        .remove(0);
    let f = &parts.fields[0];
    let span = (0.4 * FS) as usize;
    let from = parts.lead_samples;
    let geometric: Vec<f64> = (from..from + span)
        .map(|i| f.early[0][i] + f.late[0][i])
        .collect();
    let solved: Vec<f64> = f.wave.as_ref().unwrap()[0][from..from + span].to_vec();
    let merged: Vec<f64> = merged.samples()[from..from + span]
        .iter()
        .map(|&v| f64::from(v))
        .collect();
    let bands = 6;
    for i in 0..bands {
        let f0 = lo * (hi / lo).powf(i as f64 / bands as f64);
        let f1 = lo * (hi / lo).powf((i + 1) as f64 / bands as f64);
        let db = |x: &[f64]| 10.0 * band_energy(x, f0, f1).log10();
        let (g, w, m) = (db(&geometric), db(&solved), db(&merged));
        eprintln!("{f0:.0}–{f1:.0} Hz: geometric {g:.1}, wave {w:.1}, merged {m:.1} dB");
        assert!(
            m > g.min(w) - 1.5 && m < g.max(w) + 1.5,
            "{f0:.0}–{f1:.0} Hz: merged {m:.1} dB outside geometric {g:.1} and wave {w:.1}"
        );
    }
}

/// V3, decay: in the octave holding the seam, the wave solver's and the geometric solvers' decay
/// times agree, and the merged response decays between them.
#[test]
fn v3_band_decays_agree_across_the_seam() {
    let size = Vec3::new(cells(84.0), cells(64.0), cells(43.0));
    let room = Room::shoebox(
        size,
        std::array::from_fn(|_| {
            Material::new("wall", [0.15; NUM_BANDS], [0.3; NUM_BANDS]).unwrap()
        }),
    )
    .unwrap();
    let scene = Scene::new(
        "decay",
        room,
        Air::standard(),
        Vec3::new(1.37, 1.21, 1.13),
        Vec3::new(3.62, 2.48, 1.52),
    )
    .unwrap();
    let sim = simulate(&scene, &options(3, 2.0, false)).unwrap();
    let mono = Array::mono().placed(&Frame::WORLD).unwrap();
    // The 250 Hz octave holds the 200 Hz seam.
    let t30 = |x: &[f64]| analyse_f64(x, FS).unwrap().bands[2].t30_s.unwrap();
    // One late-synthesis draw's decay time here lands anywhere from 5 % to 19 % below the wave
    // solver's, so the geometric and merged decays are means over six seeds. The wave solver's
    // does not depend on the seed.
    const SEEDS: u64 = 6;
    let (mut g, mut m, mut w) = (0.0, 0.0, 0.0);
    for seed in 1..=SEEDS {
        let mut o = dry();
        o.late.seed = seed;
        let parts = render_parts(&[sim.field()], &scene.air, &mono, &o).unwrap();
        let f = &parts.fields[0];
        let geometric: Vec<f64> = f.early[0]
            .iter()
            .zip(&f.late[0])
            .map(|(a, b)| a + b)
            .collect();
        w = t30(&f.wave.as_ref().unwrap()[0]);
        let merged = render_fields(&[sim.field()], &scene.air, &mono, &o)
            .unwrap()
            .remove(0);
        let merged: Vec<f64> = merged.samples().iter().map(|&v| f64::from(v)).collect();
        g += t30(&geometric) / SEEDS as f64;
        m += t30(&merged) / SEEDS as f64;
    }
    eprintln!("250 Hz T30, means over seeds: geometric {g:.3} s, wave {w:.3} s, merged {m:.3} s");
    assert!((g / w - 1.0).abs() < 0.2, "geometric {g} vs wave {w}");
    assert!(m > g.min(w) * 0.9 && m < g.max(w) * 1.1, "merged {m}");
}

/// A cabin of a few cubic metres: its fitted boundaries against the air's stiffness make a
/// whole-room mode near 7 Hz, which the source's net volume excites. The 10 Hz drift block barely
/// touched it, and the wave part stopped mid-swing when the solver's run ended, so the 125 and
/// 250 Hz decays measured 7–13 s. A direct-free render, as the catalogue commits, must carry no
/// sub-audio mode and decay like the room, and its wave part must fade out rather than stop.
#[test]
fn a_small_cabin_carries_no_sub_audio_mode() {
    let cabin = ARCHETYPES.iter().find(|a| a.slug == "car-cabin").unwrap();
    let space = (cabin.build)().unwrap();
    let position = space.positions[0];
    let scene = Scene::new(
        "cabin",
        space.room.clone(),
        space.air,
        space.sources.centre,
        position.receiver,
    )
    .unwrap();
    let mono = Array::mono()
        .placed(&Frame::facing(position.receiver, space.sources.centre))
        .unwrap();
    let sim = simulate(
        &scene,
        &SimulationOptions {
            image_sources: ImageSourceOptions {
                max_order: 3,
                ..Default::default()
            },
            rays: RayOptions {
                rays: 5_000,
                max_time_s: space.max_time_s,
                ..Default::default()
            },
            diffraction: None,
            wave: Some(WaveOptions {
                seam_hz: space.seam_hz,
                ..WaveOptions::for_arrays(&[&mono])
            }),
        },
    )
    .unwrap();
    let direct_free = RenderOptions {
        include_direct: false,
        ..Default::default()
    };
    let rendered = render_fields(&[sim.field()], &scene.air, &mono, &direct_free)
        .unwrap()
        .remove(0);
    let x: Vec<f64> = rendered.samples().iter().map(|&v| f64::from(v)).collect();
    // Sub-audio content: the energy below 12 Hz, exactly, from the discrete Fourier transform's
    // bins there (Parseval), as a fraction of the whole render's energy.
    let n = x.len();
    let bin_hz = FS / n as f64;
    let bin_energy = |k: usize| {
        let f = k as f64 * bin_hz;
        let sum = x.iter().enumerate().fold(C64::ZERO, |acc, (i, &v)| {
            acc + C64::from_polar(v, -2.0 * PI * f * i as f64 / FS)
        });
        let e = sum.norm_sqr() / n as f64;
        if k == 0 { e } else { 2.0 * e }
    };
    let total: f64 = x.iter().map(|v| v * v).sum();
    let share_db = |e: f64| 10.0 * (e / total).log10();
    let last_sub = (12.0 / bin_hz) as usize;
    let dc = bin_energy(0);
    let below: f64 = dc + (1..=last_sub).map(bin_energy).sum::<f64>();
    let up_to_30: f64 = (last_sub + 1..=(30.0 / bin_hz) as usize)
        .map(bin_energy)
        .sum();
    let sub_audio_db = share_db(below);
    let a = analyse_f64(&x, FS).unwrap();
    let (t125, t250) = (a.bands[1].t30_s.unwrap(), a.bands[2].t30_s.unwrap());
    eprintln!(
        "cabin: below 12 Hz {sub_audio_db:.1} dB of total (DC {:.1} dB), 12-30 Hz {:.1} dB; T30 125 Hz {t125:.2} s, 250 Hz {t250:.2} s",
        share_db(dc),
        share_db(up_to_30)
    );
    assert!(
        sub_audio_db < -30.0,
        "sub-audio content {sub_audio_db:.1} dB"
    );
    assert!(
        t125 < 1.0 && t250 < 1.0,
        "T30 125 Hz {t125} s, 250 Hz {t250} s"
    );
    // The wave part fades out at the end of the solver's run instead of stopping mid-swing: its last
    // 5 ms stay far under the 50 ms before its fade began.
    let parts = render_parts(&[sim.field()], &scene.air, &mono, &direct_free).unwrap();
    let wave = &parts.fields[0].wave.as_ref().unwrap()[0];
    let peak = wave.iter().fold(0.0f64, |m, v| m.max(v.abs()));
    let end = wave.iter().rposition(|v| v.abs() > peak * 1e-9).unwrap() + 1;
    let ms = |t: f64| (t * FS / 1000.0) as usize;
    let max_abs = |r: std::ops::Range<usize>| wave[r].iter().fold(0.0f64, |m, v| m.max(v.abs()));
    let before = max_abs(end - ms(100.0)..end - ms(50.0));
    let tail = max_abs(end - ms(5.0)..end);
    eprintln!(
        "cabin wave part: its last 5 ms {:.1} dB under the 50 ms before its fade",
        20.0 * (before / tail).log10()
    );
    assert!(tail < 0.2 * before, "the wave part stops mid-swing");
}
