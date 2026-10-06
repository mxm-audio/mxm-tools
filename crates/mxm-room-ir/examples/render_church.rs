//! Renders one large archetype through the geometric hybrid, the R2 end-to-end path: a generic
//! stone parish church (nave and chancel), near and far, as stereo, a true-stereo pair and
//! first-order ambisonics, each with a sidecar, plus the full-response render its metrics come
//! from. Output goes under ignored `target/mxm-room-ir/`.
//!
//! The materials are the research page's rows; the geometry is invented, not any real church.
//!
//! ```bash
//! cargo run -p mxm-room-ir --release --example render_church
//! ```

use std::path::{Path, PathBuf};
use std::time::Instant;

use mxm_room_ir::analysis::analyse;
use mxm_room_ir::directivity::{Array, Frame, PlacedCapsule};
use mxm_room_ir::rays::RayOptions;
use mxm_room_ir::render::{Cap, RenderOptions, Rendered};
use mxm_room_ir::sidecar::Sidecar;
use mxm_room_ir::simulation::{Simulation, SimulationOptions, render_set, simulate};
use mxm_room_ir::{Air, ImageSourceOptions, Material, Room, Scene, Vec3, wav};

type Result<T> = std::result::Result<T, Box<dyn std::error::Error>>;

fn main() -> Result<()> {
    // Absorption: `research:effects/room-acoustics-simulation.md` §8.1 (PTB compilation rows:
    // exposed brick, Meyer No. 12; rough sandstone floor, Meyer No. 15; pews, Meyer No. 463).
    // Scattering from the page's depth rule, checked against Zeng et al.'s mid-band ranges:
    // rough masonry 0.3, pews (a rough structure) 0.5.
    let brick = Material::from_125_to_4k(
        "exposed brick",
        [0.15, 0.13, 0.15, 0.15, 0.13, 0.14],
        [0.3; 6],
    )?;
    let stone = Material::from_125_to_4k(
        "rough sandstone",
        [0.02, 0.02, 0.03, 0.04, 0.05, 0.05],
        [0.3; 6],
    )?;
    let pews = Material::from_125_to_4k(
        "pews, no cushions",
        [0.10, 0.15, 0.18, 0.20, 0.20, 0.20],
        [0.5; 6],
    )?;
    let nave = vec![
        (0.0, 0.0),
        (30.0, 0.0),
        (30.0, 2.0),
        (30.0, 10.0),
        (30.0, 12.0),
        (0.0, 12.0),
    ];
    let chancel = vec![(30.0, 2.0), (38.0, 2.0), (38.0, 10.0), (30.0, 10.0)];
    let room = Room::extruded_regions(
        &[
            (nave, [pews, stone.clone(), brick]),
            (chancel, [stone.clone(), stone.clone(), stone]),
        ],
        14.0,
    )?;
    let options = SimulationOptions {
        image_sources: ImageSourceOptions {
            max_order: 3,
            ..Default::default()
        },
        rays: RayOptions {
            rays: 40_000,
            max_time_s: 12.0,
            ..Default::default()
        },
        ..Default::default()
    };
    let out_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../target/mxm-room-ir");
    let render = RenderOptions {
        include_direct: false,
        cap: Some(Cap {
            length_s: 10.0,
            fade_s: 0.5,
        }),
        ..Default::default()
    };
    let full = RenderOptions {
        include_direct: true,
        cap: None,
        ..render
    };
    let centre = Vec3::new(31.5, 6.0, 1.6);
    let left = Vec3::new(31.5, 8.0, 1.6);
    let right = Vec3::new(31.5, 4.0, 1.6);
    for (position, receiver) in [
        ("near", Vec3::new(22.0, 6.4, 1.5)),
        ("far", Vec3::new(6.0, 5.6, 1.5)),
    ] {
        let started = Instant::now();
        let scene = |name: &str, source: Vec3| {
            Scene::new(
                format!("r2-stone-church.{position}.{name}"),
                room.clone(),
                Air::standard(),
                source,
                receiver,
            )
        };
        let sim_c = simulate(&scene("centre", centre)?, &options)?;
        let sim_l = simulate(&scene("left", left)?, &options)?;
        let sim_r = simulate(&scene("right", right)?, &options)?;
        let frame = Frame::facing(receiver, centre);
        let traced = started.elapsed();

        let mono = Array::mono().placed(&frame)?;
        let reference = render_set(&[&sim_c], &mono, &full)?.remove(0);
        let analysis = analyse(reference.samples(), f64::from(reference.sample_rate));
        let stem = format!("r2-stone-church.{position}");
        let notes = [
            "R2: geometric hybrid; no wave solver (a large space, plan section 7)",
            "direct sound excluded; metrics are from the full-response render",
        ];
        let write = |name: &str,
                     sim: &Simulation,
                     array: &str,
                     capsules: &[PlacedCapsule],
                     options: &RenderOptions,
                     rendered: &Rendered|
         -> Result<()> {
            write_file(
                &out_dir.join(format!("{stem}.{name}")),
                Sidecar {
                    simulation: sim,
                    array_name: array,
                    capsules,
                    render: options,
                    rendered,
                    full_response_analysis: analysis.as_ref(),
                    extra: vec![("position".into(), format!("\"{position}\""))],
                    notes: &notes,
                },
            )
        };
        write("full", &sim_c, "mono omni", &mono, &full, &reference)?;

        let ortf = Array::near_coincident_cardioids(0.17, 55.0);
        let ortf_capsules = ortf.placed(&frame)?;
        let stereo = render_set(&[&sim_c], &ortf_capsules, &render)?.remove(0);
        write(
            "stereo",
            &sim_c,
            &ortf.name,
            &ortf_capsules,
            &render,
            &stereo,
        )?;
        let pair = render_set(&[&sim_l, &sim_r], &ortf_capsules, &render)?;
        write(
            "true-stereo-left",
            &sim_l,
            &ortf.name,
            &ortf_capsules,
            &render,
            &pair[0],
        )?;
        write(
            "true-stereo-right",
            &sim_r,
            &ortf.name,
            &ortf_capsules,
            &render,
            &pair[1],
        )?;
        let foa = Array::first_order_ambisonics();
        let foa_capsules = foa.placed(&frame)?;
        let ambi = render_set(&[&sim_c], &foa_capsules, &render)?.remove(0);
        write("foa", &sim_c, &foa.name, &foa_capsules, &render, &ambi)?;

        println!(
            "{position}: {:.0} m³, {} image sources, {} rays traced ×3 in {traced:.2?}; all renders in {:.2?}",
            room.volume(),
            sim_c.image_source_arrivals,
            sim_c.trace.rays,
            started.elapsed()
        );
        if let Some(a) = &analysis {
            let t = |k: usize| a.bands[k].t30_s.unwrap_or(f64::NAN);
            println!(
                "  T30 125/250/500/1k/2k/4k: {:.2} {:.2} {:.2} {:.2} {:.2} {:.2} s; C80 1k {:.1} dB; EDT 1k {:.2} s",
                t(1),
                t(2),
                t(3),
                t(4),
                t(5),
                t(6),
                a.bands[4].c80_db.unwrap_or(f64::NAN),
                a.bands[4].edt_s.unwrap_or(f64::NAN)
            );
        }
        println!(
            "  stereo {:.2} s (lead {}), peak {:.4}; full response {:.2} s",
            stereo.duration_s(),
            stereo.lead_samples,
            stereo
                .channels
                .iter()
                .flatten()
                .fold(0.0f32, |m, v| m.max(v.abs())),
            reference.duration_s()
        );
    }
    println!("wrote {}", out_dir.display());
    Ok(())
}

/// Writes `<stem>.wav` and `<stem>.json`. The stem's dots are part of the name, never an extension.
fn write_file(stem: &Path, sidecar: Sidecar) -> Result<()> {
    let channels: Vec<&[f32]> = sidecar
        .rendered
        .channels
        .iter()
        .map(Vec::as_slice)
        .collect();
    let with = |suffix: &str| {
        let mut name = stem.as_os_str().to_owned();
        name.push(suffix);
        PathBuf::from(name)
    };
    wav::write_f32(&with(".wav"), &channels, sidecar.rendered.sample_rate)?;
    std::fs::write(with(".json"), sidecar.to_json())?;
    Ok(())
}
