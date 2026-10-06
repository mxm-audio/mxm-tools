//! Renders one small archetype through the full hybrid, the R3 end-to-end path: a generic furnished
//! living room, wave-solved below its Schroeder frequency and geometric above, as the mono
//! full response (with the direct sound, for metrics) and a direct-free ORTF stereo file, each with
//! a sidecar. Output goes under ignored `target/mxm-room-ir/`.
//!
//! The materials are the research page's rows; the geometry is invented.
//!
//! ```bash
//! cargo run -p mxm-room-ir --release --example render_small_room
//! ```

use std::path::PathBuf;
use std::time::Instant;

use mxm_room_ir::analysis::analyse;
use mxm_room_ir::directivity::{Array, Frame};
use mxm_room_ir::rays::RayOptions;
use mxm_room_ir::render::RenderOptions;
use mxm_room_ir::sidecar::{JsonObject, Sidecar};
use mxm_room_ir::simulation::{SimulationOptions, WaveOptions, render_set, simulate};
use mxm_room_ir::{Air, ImageSourceOptions, Material, Room, Scene, Vec3, wav};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    // `research:effects/room-acoustics-simulation.md` §8.1: heavy carpet on concrete (Harris 1991
    // No. 116), smooth plaster on masonry (Meyer No. 328), plasterboard on studs (DNA 1968 No. 304)
    // for the ceiling. Scattering: a furnished room's walls 0.3 (bookshelves, Zeng et al.), the
    // carpet 0.1, the ceiling 0.05.
    let carpet = Material::from_125_to_4k(
        "carpet, heavy, on concrete",
        [0.02, 0.06, 0.14, 0.37, 0.60, 0.65],
        [0.1; 6],
    )?;
    let walls = Material::from_125_to_4k(
        "plaster on masonry, furnished",
        [0.02, 0.02, 0.03, 0.04, 0.05, 0.05],
        [0.3; 6],
    )?;
    let ceiling = Material::from_125_to_4k(
        "plasterboard on studs",
        [0.08, 0.11, 0.04, 0.03, 0.03, 0.02],
        [0.05; 6],
    )?;
    let room = Room::shoebox(
        Vec3::new(5.6, 4.3, 2.6),
        [
            walls.clone(),
            walls.clone(),
            walls.clone(),
            walls,
            carpet,
            ceiling,
        ],
    )?;
    let source = Vec3::new(1.3, 2.9, 1.2);
    let receiver = Vec3::new(4.1, 1.6, 1.1);
    let scene = Scene::new("r3-living-room", room, Air::standard(), source, receiver)?;
    let frame = Frame::facing(receiver, source);
    let mono = Array::mono().placed(&frame)?;
    let ortf = Array::near_coincident_cardioids(0.17, 55.0);
    let ortf_capsules = ortf.placed(&frame)?;
    let options = SimulationOptions {
        image_sources: ImageSourceOptions {
            max_order: 3,
            ..Default::default()
        },
        rays: RayOptions {
            rays: 40_000,
            max_time_s: 3.0,
            ..Default::default()
        },
        diffraction: None,
        wave: Some(WaveOptions::for_arrays(&[&mono, &ortf_capsules])),
    };
    let started = Instant::now();
    let sim = simulate(&scene, &options)?;
    let simulated = started.elapsed();
    let wave = sim.wave.as_ref().expect("wave-solved");

    let full = RenderOptions::default();
    let stereo_options = RenderOptions {
        include_direct: false,
        ..full
    };
    let reference = render_set(&[&sim], &mono, &full)?.remove(0);
    let stereo = render_set(&[&sim], &ortf_capsules, &stereo_options)?.remove(0);
    let analysis = analyse(reference.samples(), 48_000.0);
    let out_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../target/mxm-room-ir");
    std::fs::create_dir_all(&out_dir)?;
    let wave_json = JsonObject::new()
        .number("seam_hz", wave.seam_hz)
        .number("solver_rate_hz", wave.sample_rate)
        .number("grid_spacing_m", wave.spacing_m)
        .integer("cells", wave.cells as u64)
        .finish();
    let notes = [
        "R3: wave solver below the transition band, geometric hybrid above, causal crossover",
        "boundary models fitted to band absorption (labelled fitted)",
    ];
    for (name, rendered, array, capsules, render) in [
        ("full", &reference, "mono omni", &mono, &full),
        (
            "stereo",
            &stereo,
            ortf.name.as_str(),
            &ortf_capsules,
            &stereo_options,
        ),
    ] {
        let stem = out_dir.join(format!("{}.{name}", scene.name));
        let channels: Vec<&[f32]> = rendered.channels.iter().map(Vec::as_slice).collect();
        let mut wav_path = stem.clone().into_os_string();
        wav_path.push(".wav");
        wav::write_f32(&PathBuf::from(wav_path), &channels, rendered.sample_rate)?;
        let sidecar = Sidecar {
            simulation: &sim,
            array_name: array,
            capsules,
            render,
            rendered,
            full_response_analysis: analysis.as_ref(),
            extra: vec![("wave_solver".into(), wave_json.clone())],
            notes: &notes,
        };
        let mut json_path = stem.into_os_string();
        json_path.push(".json");
        std::fs::write(PathBuf::from(json_path), sidecar.to_json())?;
    }
    println!(
        "{}: {:.0} m³; seam {:.0} Hz ({:.0}–{:.0} Hz), wave solver at {:.0} Hz, h {:.3} m, {} cells; simulated in {simulated:.2?}, rendered in {:.2?}",
        scene.name,
        scene.room.volume(),
        wave.seam_hz,
        wave.transition_hz.0,
        wave.transition_hz.1,
        wave.sample_rate,
        wave.spacing_m,
        wave.cells,
        started.elapsed() - simulated
    );
    if let Some(a) = &analysis {
        let t = |k: usize| a.bands[k].t30_s.unwrap_or(f64::NAN);
        println!(
            "  T30 63/125/250/500/1k/2k/4k: {:.2} {:.2} {:.2} {:.2} {:.2} {:.2} {:.2} s; C80 1k {:.1} dB",
            t(0),
            t(1),
            t(2),
            t(3),
            t(4),
            t(5),
            t(6),
            a.bands[4].c80_db.unwrap_or(f64::NAN)
        );
    }
    println!(
        "  stereo {:.2} s (lead {}), full {:.2} s; wrote {}",
        stereo.duration_s(),
        stereo.lead_samples,
        reference.duration_s(),
        out_dir.display()
    );
    Ok(())
}
