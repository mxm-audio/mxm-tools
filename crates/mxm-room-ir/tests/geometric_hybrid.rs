//! R2's proofs for the geometric hybrid: V4 diffuse decay, V5 coupled volumes, V6 echo density,
//! V7 coherence, and V10's ray-count convergence of ray-assisted image sources.
//!
//! Every expectation is a closed form or a statistical model named beside it; none is a value
//! read off this library's own output.

use mxm_room_ir::analysis::{analyse, band_correlation, diffuse_correlation, echo_density};
use mxm_room_ir::directivity::{Array, Frame};
use mxm_room_ir::rays::RayOptions;
use mxm_room_ir::render::RenderOptions;
use mxm_room_ir::simulation::{SimulationOptions, render_set, simulate};
use mxm_room_ir::{Air, ImageSourceOptions, Material, NUM_BANDS, Room, Scene, Vec3};

/// `ln(10^6)`: a 60 dB energy decay.
const LN_1E6: f64 = 13.815_510_557_964_274;

fn uniform(name: &str, absorption: f64, scattering: f64) -> Material {
    Material::new(name, [absorption; NUM_BANDS], [scattering; NUM_BANDS]).unwrap()
}

fn options(rays: usize, max_time_s: f64) -> SimulationOptions {
    SimulationOptions {
        image_sources: ImageSourceOptions {
            max_order: 3,
            ..Default::default()
        },
        rays: RayOptions {
            rays,
            max_time_s,
            ..Default::default()
        },
        ..Default::default()
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

fn as_f64(x: &[f32]) -> Vec<f64> {
    x.iter().map(|&v| f64::from(v)).collect()
}

/// V4: a uniformly absorbing, highly scattering room decays per Eyring,
/// `T = 24·ln 10·V / (c·S·(−ln(1 − α)))` (research page §3).
#[test]
fn v4_diffuse_room_decays_per_eyring() {
    let size = Vec3::new(8.3, 6.1, 4.2);
    let alpha = 0.2;
    let room = Room::shoebox(size, std::array::from_fn(|_| uniform("m", alpha, 0.8))).unwrap();
    let (v, s) = (room.volume(), room.surface_area());
    let scene = Scene::new(
        "eyring",
        room,
        Air::standard(),
        Vec3::new(2.1, 1.7, 1.4),
        Vec3::new(5.9, 4.3, 1.8),
    )
    .unwrap();
    let c = scene.air.speed_of_sound();
    let eyring = LN_1E6 * 4.0 * v / (c * s * -(1.0 - alpha).ln());
    let sim = simulate(&scene, &options(30_000, 3.0)).unwrap();
    let out = render_set(
        &[&sim],
        &Array::mono().placed(&Frame::WORLD).unwrap(),
        &dry(),
    )
    .unwrap()
    .remove(0);
    let a = analyse(out.samples(), 48_000.0).unwrap();
    for band in 2..=7 {
        let t30 = a.bands[band].t30_s.unwrap();
        assert!(
            (t30 / eyring - 1.0).abs() < 0.08,
            "band {band}: T30 {t30:.3} s vs Eyring {eyring:.3} s"
        );
    }
    let t30 = a.broadband.t30_s.unwrap();
    assert!(
        (t30 / eyring - 1.0).abs() < 0.08,
        "broadband T30 {t30} vs {eyring}"
    );
}

/// V4 with scattering that rises across the bands, as the depth rule gives a real surface: every
/// band still decays per Eyring. A single scatter-or-specular probability shared by all bands
/// failed this: its band weights multiplied without bound, and the low bands' decays fell in steps.
#[test]
fn v4_holds_when_scattering_varies_with_frequency() {
    let size = Vec3::new(8.3, 6.1, 4.2);
    let alpha = 0.2;
    let scattering = [0.3, 0.3, 0.3, 0.4, 0.6, 0.8, 0.95, 0.99, 0.99];
    let wall = Material::new("rising", [alpha; NUM_BANDS], scattering).unwrap();
    let room = Room::shoebox(size, std::array::from_fn(|_| wall.clone())).unwrap();
    let (v, s) = (room.volume(), room.surface_area());
    let scene = Scene::new(
        "eyring-rising",
        room,
        Air::standard(),
        Vec3::new(2.1, 1.7, 1.4),
        Vec3::new(5.9, 4.3, 1.8),
    )
    .unwrap();
    let c = scene.air.speed_of_sound();
    let eyring = LN_1E6 * 4.0 * v / (c * s * -(1.0 - alpha).ln());
    let sim = simulate(&scene, &options(30_000, 3.0)).unwrap();
    let out = render_set(
        &[&sim],
        &Array::mono().placed(&Frame::WORLD).unwrap(),
        &dry(),
    )
    .unwrap()
    .remove(0);
    let a = analyse(out.samples(), 48_000.0).unwrap();
    // From 250 Hz, as V4: a 0.7 s decay at 125 Hz is estimated from one synthetic noise tail and
    // varies by about the tolerance between seeds. The fault this guards against failed at 250 Hz.
    for band in 2..=7 {
        let t30 = a.bands[band].t30_s.unwrap();
        assert!(
            (t30 / eyring - 1.0).abs() < 0.08,
            "band {band}: T30 {t30:.3} s vs Eyring {eyring:.3} s"
        );
    }
}

/// V5: weak coupling between a live and a dead room gives the double slope the statistical
/// two-room model predicts [D]: with absorption areas `A₁, A₂`, coupling aperture `S` and volumes
/// `V₁, V₂`, the energy densities obey `4V·dE/dt = −c(A + S)E + cS·E_other`, whose slower
/// eigenvalue sets the late decay heard in the dead room.
#[test]
fn v5_weakly_coupled_rooms_give_the_predicted_double_slope() {
    let live = || uniform("live", 0.08, 0.7);
    let dead = || uniform("dead", 0.45, 0.7);
    let height = 4.0;
    let (neck_w, neck_l) = (0.6, 1.0);
    let y0 = 3.7;
    let live_plan = vec![
        (0.0, 0.0),
        (10.0, 0.0),
        (10.0, y0),
        (10.0, y0 + neck_w),
        (10.0, 8.0),
        (0.0, 8.0),
    ];
    let neck = vec![
        (10.0, y0),
        (10.0 + neck_l, y0),
        (10.0 + neck_l, y0 + neck_w),
        (10.0, y0 + neck_w),
    ];
    let x1 = 10.0 + neck_l;
    let dead_plan = vec![
        (x1, 1.0),
        (x1 + 6.0, 1.0),
        (x1 + 6.0, 7.0),
        (x1, 7.0),
        (x1, y0 + neck_w),
        (x1, y0),
    ];
    let room = Room::extruded_regions(
        &[
            (live_plan, [live(), live(), live()]),
            (neck, [live(), live(), live()]),
            (dead_plan, [dead(), dead(), dead()]),
        ],
        height,
    )
    .unwrap();
    let scene = Scene::new(
        "coupled",
        room,
        Air::standard(),
        Vec3::new(x1 + 4.3, 5.1, 1.6),
        Vec3::new(x1 + 2.2, 2.4, 1.3),
    )
    .unwrap();

    // The two-room model.
    let c = scene.air.speed_of_sound();
    let aperture = neck_w * height;
    let (v1, v2) = (10.0 * 8.0 * height, 6.0 * 6.0 * height);
    let s1 = 2.0 * (10.0 * 8.0 + 10.0 * height + 8.0 * height) - aperture;
    let s2 = 2.0 * (36.0 + 12.0 * height) - aperture;
    let (a1, a2) = (0.08 * s1, 0.45 * s2);
    let m11 = -c * (a1 + aperture) / (4.0 * v1);
    let m22 = -c * (a2 + aperture) / (4.0 * v2);
    let m12 = c * aperture / (4.0 * v1);
    let m21 = c * aperture / (4.0 * v2);
    let tr = m11 + m22;
    let det = m11 * m22 - m12 * m21;
    let slow = (tr + (tr * tr - 4.0 * det).sqrt()) / 2.0;
    let t_slow = LN_1E6 / -slow;
    let t_dead = LN_1E6 / -m22;

    let sim = simulate(&scene, &options(60_000, 4.0)).unwrap();
    let out = render_set(
        &[&sim],
        &Array::mono().placed(&Frame::WORLD).unwrap(),
        &dry(),
    )
    .unwrap()
    .remove(0);
    let ir = as_f64(out.samples());
    // Schroeder curve of the 1 kHz band, and its slopes early and late.
    let band = mxm_room_ir::analysis::band_split(&ir, 48_000.0)[4].clone();
    let mut edc = vec![0.0; band.len()];
    let mut tail = 0.0;
    for i in (0..band.len()).rev() {
        tail += band[i] * band[i];
        edc[i] = tail;
    }
    let db: Vec<f64> = edc.iter().map(|e| 10.0 * (e / edc[0]).log10()).collect();
    let slope_time = |from: f64, to: f64| {
        let i0 = db.iter().position(|&d| d <= from).unwrap();
        let i1 = db.iter().position(|&d| d <= to).unwrap();
        60.0 * ((i1 - i0) as f64 / 48_000.0) / (from - to).abs()
    };
    let late = slope_time(-30.0, -50.0);
    let early = slope_time(0.0, -10.0);
    assert!(
        (late / t_slow - 1.0).abs() < 0.15,
        "late slope T {late:.2} s vs two-room model {t_slow:.2} s"
    );
    assert!(
        early < 0.6 * late,
        "no double slope: early T {early:.2} s, late T {late:.2} s (dead room alone {t_dead:.2} s)"
    );
}

/// V6: in a diffuse scene the normalised echo density reaches the diffuse value and has no step
/// where image sources give way to rays.
#[test]
fn v6_echo_density_reaches_diffuse_without_a_step() {
    let room = Room::shoebox(
        Vec3::new(9.1, 7.3, 4.4),
        std::array::from_fn(|_| uniform("m", 0.15, 0.7)),
    )
    .unwrap();
    let scene = Scene::new(
        "density",
        room,
        Air::standard(),
        Vec3::new(2.3, 2.9, 1.5),
        Vec3::new(6.4, 4.8, 1.7),
    )
    .unwrap();
    let sim = simulate(&scene, &options(20_000, 1.5)).unwrap();
    let out = render_set(
        &[&sim],
        &Array::mono().placed(&Frame::WORLD).unwrap(),
        &dry(),
    )
    .unwrap()
    .remove(0);
    let ir = as_f64(out.samples());
    let density = echo_density(&ir, 48_000.0, 0.02, 0.005);
    let late: Vec<f64> = density
        .iter()
        .filter(|(t, _)| (0.15..0.8).contains(t))
        .map(|(_, d)| *d)
        .collect();
    let mean = late.iter().sum::<f64>() / late.len() as f64;
    assert!((0.85..1.1).contains(&mean), "late echo density {mean}");
    let first_reflection = sim
        .arrivals
        .iter()
        .find(|a| a.order() > 0)
        .unwrap()
        .path_length_m
        / scene.air.speed_of_sound()
        - out.trimmed_delay_s;
    for w in density.windows(2) {
        if w[0].0 > first_reflection + 0.02 && w[1].0 < 0.8 {
            assert!(
                (w[1].1 - w[0].1).abs() < 0.3,
                "echo density steps from {:.2} to {:.2} at {:.3} s",
                w[0].1,
                w[1].1,
                w[1].0
            );
        }
    }
}

/// V7: in an isotropic diffuse field a spaced omni pair's tail correlation follows sin(kd)/kd,
/// averaged over each octave.
#[test]
fn v7_spaced_omni_pair_follows_diffuse_field_correlation() {
    let room = Room::shoebox(
        Vec3::new(9.4, 7.7, 5.2),
        std::array::from_fn(|_| uniform("m", 0.06, 0.9)),
    )
    .unwrap();
    let scene = Scene::new(
        "coherence",
        room,
        Air::standard(),
        Vec3::new(2.4, 2.1, 1.9),
        Vec3::new(6.1, 5.2, 2.3),
    )
    .unwrap();
    let spacing = 0.3;
    let sim = simulate(&scene, &options(20_000, 2.5)).unwrap();
    let capsules = Array::spaced_omni(spacing)
        .placed(&Frame::facing(scene.receiver, scene.source))
        .unwrap();
    let out = render_set(&[&sim], &capsules, &dry()).unwrap().remove(0);
    let (l, r) = (as_f64(&out.channels[0]), as_f64(&out.channels[1]));
    let from = (0.2 * 48_000.0) as usize;
    let to = (1.8 * 48_000.0) as usize;
    let measured = band_correlation(&l, &r, 48_000.0, from, to);
    let c = scene.air.speed_of_sound();
    for (band, &value) in measured.iter().enumerate().take(8).skip(1) {
        let expected = diffuse_correlation(band, spacing, c, 48_000.0);
        assert!(
            (value - expected).abs() < 0.1,
            "band {band}: correlation {:.3} vs diffuse {expected:.3}",
            value
        );
    }
}

/// V10 for non-diffuse scenes: the energy of ray-assisted image sources converges as rays grow,
/// toward the exact image sum. Rays are seeded by index, so a larger run's found paths include a
/// smaller run's. The corridor is a box, so the complete set of specular paths is Allen &
/// Berkley's, and its energy past the image-source order and within the rays' reach is the target.
#[test]
fn v10_ray_assisted_specular_energy_converges() {
    let size = Vec3::new(40.0, 2.6, 3.1);
    let (alpha, scattering) = (0.04, 0.05);
    let room = Room::shoebox(
        size,
        std::array::from_fn(|_| uniform("tile", alpha, scattering)),
    )
    .unwrap();
    let scene = Scene::new(
        "corridor",
        room,
        Air::standard(),
        Vec3::new(3.1, 1.2, 1.5),
        Vec3::new(21.7, 1.4, 1.6),
    )
    .unwrap()
    .non_diffuse();
    let max_time = 0.4;
    let reach = max_time * scene.air.speed_of_sound();
    // Exact: images at 2nL +/- s along each axis, with |2n| or |2n - 1| reflections.
    let per_reflection = (1.0 - alpha) * (1.0 - scattering);
    let axis = |l: f64, s: f64| {
        let n_max = (reach / (2.0 * l)).ceil() as i64 + 1;
        let mut out = Vec::new();
        for n in -n_max..=n_max {
            out.push((2.0 * n as f64 * l + s, (2 * n).unsigned_abs()));
            out.push((2.0 * n as f64 * l - s, (2 * n - 1).unsigned_abs()));
        }
        out
    };
    let (src, rcv) = (scene.source, scene.receiver);
    let mut exact = 0.0;
    for &(x, ox) in &axis(size.x, src.x) {
        for &(y, oy) in &axis(size.y, src.y) {
            for &(z, oz) in &axis(size.z, src.z) {
                let order = ox + oy + oz;
                let length = (Vec3::new(x, y, z) - rcv).length();
                if order > 3 && length <= reach {
                    exact += per_reflection.powi(order as i32) / (length * length);
                }
            }
        }
    }
    let energy = |rays: usize| {
        let sim = simulate(&scene, &options(rays, max_time)).unwrap();
        (sim.ray_assisted_arrivals, sim.ray_assisted_energy(4))
    };
    let (n1, e1) = energy(12_500);
    let (n2, e2) = energy(50_000);
    let (n3, e3) = energy(200_000);
    eprintln!("ray-assisted: {n1} {e1:.4e} | {n2} {e2:.4e} | {n3} {e3:.4e} | exact {exact:.4e}");
    assert!(n1 <= n2 && n2 <= n3 && n1 > 0, "{n1} {n2} {n3}");
    assert!(
        e1 <= e2 && e2 <= e3 && e3 <= exact * (1.0 + 1e-9),
        "{e1} {e2} {e3} {exact}"
    );
    assert!(
        e3 - e2 < e2 - e1 && e3 / exact > 0.95,
        "not converging: {e1:.3e}, {e2:.3e}, {e3:.3e} toward {exact:.3e}"
    );
}

/// V10 for the hybrid: late synthesis never precedes the sound it follows. With the direct path on a
/// sample and air off, a mono render has no leading silence, the direct sound is sample zero, and
/// nothing sounds before the first reflection's kernel. Written after the first church render put
/// late energy 3 ms ahead of the direct sound.
#[test]
fn hybrid_render_keeps_time_zero_on_the_direct_sound() {
    let room = Room::shoebox(
        Vec3::new(9.1, 7.3, 4.4),
        std::array::from_fn(|_| uniform("m", 0.15, 0.7)),
    )
    .unwrap();
    let scene = Scene::new(
        "causal",
        room,
        Air::standard(),
        Vec3::new(2.3, 2.9, 1.5),
        Vec3::new(6.4, 4.8, 1.7),
    )
    .unwrap();
    let sim = simulate(&scene, &options(10_000, 1.0)).unwrap();
    let out = render_set(
        &[&sim],
        &Array::mono().placed(&Frame::WORLD).unwrap(),
        &dry(),
    )
    .unwrap()
    .remove(0);
    assert!(out.late_events > 0);
    assert_eq!(out.lead_samples, 0, "late energy rang before time zero");
    let samples = out.samples();
    let direct = (scene.receiver - scene.source).length();
    assert!((f64::from(samples[0]) - 1.0 / direct).abs() < 1e-6);
    let c = scene.air.speed_of_sound();
    let first_reflection = sim
        .arrivals
        .iter()
        .filter(|a| a.order() > 0)
        .map(|a| a.path_length_m)
        .fold(f64::INFINITY, f64::min);
    let quiet_until = ((first_reflection - direct) / c * 48_000.0).floor() as usize - 32;
    assert!(
        samples[1..quiet_until].iter().all(|&v| v == 0.0),
        "sound before the first reflection"
    );
}
