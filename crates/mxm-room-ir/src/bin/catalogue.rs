//! Renders and releases the catalogue (plan §4.7, R5).
//!
//! ```bash
//! cargo run -p mxm-room-ir --release --bin catalogue -- estimate [slug ...]                         # Eyring per octave
//! cargo run -p mxm-room-ir --release --bin catalogue -- render [--rays=N] [--true-stereo] [--near] [slug ...] # into target/
//! cargo run -p mxm-room-ir --release --bin catalogue -- v9 [--near]                                          # class ranges
//! cargo run -p mxm-room-ir --release --bin catalogue -- release [--true-stereo] [--near]                     # into impulses/
//! ```
//!
//! **The catalogue is the far position, in stereo.** Every command covers each room's far position;
//! `--near` adds the near one (owner, 2026-09-15: with the mix turned down the two differ little).
//! Per position `render` simulates the centre source and writes its
//! stereo file and its full-response render with their analysis. With `--true-stereo` it also
//! simulates the left and right sources and writes the true-stereo pair (owner, 2026-09-15: our
//! consumer loads one- or two-channel files, and the pair tripled the render). Files go under
//! `target/mxm-room-ir/catalogue/<slug>/`, normalized as every render is by default, and last a key naming
//! everything the render depended on. A position whose key matches is skipped, so an interrupted
//! run resumes and a changed scene renders again; `v9` and `release` refuse a position whose key does
//! not match. `release` checks every file against the consumer's WAV path (V12), confirms each
//! file peaks at −1 dBFS, and replaces `impulses/` with the result in one step: each room's WAVs in
//! `<family>/<slug>/`, their JSON sidecars in the room's `metadata/` subfolder, and a manifest; with
//! `--true-stereo` it releases the pair too. `v9` exits 1 when a gated class misses.

use std::path::{Path, PathBuf};
use std::process::ExitCode;
use std::time::Instant;

use mxm_room_ir::analysis::{Analysis, analyse};
use mxm_room_ir::catalogue::{
    self, ARCHETYPES, Archetype, CAP, Decay, PAIR_HALF_ANGLE_DEG, PAIR_SPACING_M, Position,
    SAMPLE_RATE, Space, Summary,
};
use mxm_room_ir::directivity::{Array, Frame, PlacedCapsule};
use mxm_room_ir::rays::RayOptions;
use mxm_room_ir::render::{RenderOptions, Rendered};
use mxm_room_ir::sidecar::{JsonObject, Sidecar};
use mxm_room_ir::simulation::{
    Simulation, SimulationOptions, WaveOptions, render_set, schroeder_seam, simulate,
};
use mxm_room_ir::{ImageSourceOptions, Scene, Vec3, wav};

type Result<T> = std::result::Result<T, Box<dyn std::error::Error>>;

/// The sources, in the order of [`KINDS`] and of every per-source array below. Only the centre is
/// rendered unless true stereo is asked for.
const SOURCES: [&str; 3] = ["centre", "left", "right"];
/// The released file of each source (plan §4.4): the stereo file, then the true-stereo pair.
const KINDS: [&str; 3] = ["stereo", "true-stereo-left", "true-stereo-right"];
/// The last line of a centre-only render's key. A key without it means the pair was rendered too,
/// which is how every render was keyed before true stereo became optional.
const CENTRE_ONLY: &str = "sources centre\n";
/// Rays per simulation in the catalogue. **Chosen by convergence:** at 40,000 a 7,500 m³ hall's
/// decay times moved 5–10 % at 125–500 Hz against 160,000, the JND's size; at 160,000 its near and
/// far positions agree within 5 %.
const RAYS: usize = 160_000;
const IMAGE_SOURCE_ORDER: usize = 3;
/// Bump when a change to the simulation or rendering code changes what a render produces. The
/// render key covers the scenes and the options; it cannot see the library's code.
const RECIPE_EPOCH: u32 = 4;

fn crate_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
}

fn cache_dir() -> PathBuf {
    crate_dir().join("../../target/mxm-room-ir/catalogue")
}

fn impulses_dir() -> PathBuf {
    crate_dir().join("impulses")
}

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let rest = args.get(1..).unwrap_or_default();
    let result = match args.first().map(String::as_str) {
        Some("estimate") => estimate(rest),
        Some("render") => render(rest),
        Some("geometry") => geometry(rest),
        Some("v9") => match flags(rest, &["--near"]) {
            Ok([near]) => v9(near),
            Err(e) => Err(format!("usage: catalogue v9 [--near] ({e})").into()),
        },
        Some("release") => match flags(rest, &["--true-stereo", "--near"]) {
            Ok([true_stereo, near]) => release(true_stereo, near),
            Err(e) => {
                Err(format!("usage: catalogue release [--true-stereo] [--near] ({e})").into())
            }
        },
        _ => {
            eprintln!(
                "usage: catalogue estimate [slug ...] | geometry [slug ...] | render [--rays=N] [--true-stereo] [--near] [slug ...] | v9 [--near] | release [--true-stereo] [--near]"
            );
            return ExitCode::from(2);
        }
    };
    match result {
        Ok(true) => ExitCode::SUCCESS,
        Ok(false) => ExitCode::FAILURE,
        Err(e) => {
            eprintln!("error: {e}");
            ExitCode::FAILURE
        }
    }
}

/// Which of `known` appear in `args`, refusing anything else.
fn flags<const N: usize>(
    args: &[String],
    known: &[&str; N],
) -> std::result::Result<[bool; N], String> {
    let mut set = [false; N];
    for arg in args {
        let i = known
            .iter()
            .position(|k| k == arg)
            .ok_or_else(|| format!("unknown argument `{arg}`"))?;
        set[i] = true;
    }
    Ok(set)
}

/// The positions a command covers: the far one, and the near one too when `near` is set.
fn covered(space: &Space, near: bool) -> impl Iterator<Item = &Position> {
    space
        .positions
        .iter()
        .filter(move |p| near || p.name == "far")
}

fn selected(slugs: &[String]) -> Result<Vec<Archetype>> {
    if slugs.is_empty() {
        return Ok(ARCHETYPES.to_vec());
    }
    slugs
        .iter()
        .map(|s| {
            ARCHETYPES
                .iter()
                .find(|a| a.slug == s)
                .copied()
                .ok_or_else(|| format!("no archetype `{s}`").into())
        })
        .collect()
}

fn seconds(values: impl IntoIterator<Item = Option<f64>>) -> String {
    values
        .into_iter()
        .map(|v| v.map_or("—".to_string(), |v| format!("{v:.2}")))
        .collect::<Vec<_>>()
        .join(" ")
}

fn decay_name(decay: Decay) -> &'static str {
    match decay {
        Decay::T20 => "T20",
        Decay::T30 => "T30",
    }
}

/// `<base><suffix>`: the stem's dots are part of the name, never an extension.
fn with_suffix(base: &Path, suffix: &str) -> PathBuf {
    let mut name = base.as_os_str().to_owned();
    name.push(suffix);
    PathBuf::from(name)
}

/// `geometry [slug ...]`: each room as a Wavefront OBJ under `target/mxm-room-ir/geometry/`, one
/// group per surface, then the sources and receivers as points. For looking at a room; nothing
/// reads these back, and no render depends on them. OBJ is y-up, so a vertex is written (x, z, y).
fn geometry(slugs: &[String]) -> Result<bool> {
    let dir = cache_dir().with_file_name("geometry");
    std::fs::create_dir_all(&dir)?;
    for a in selected(slugs)? {
        let space = (a.build)()?;
        let room = &space.room;
        let mut obj = format!(
            "# {} ({}, {}): {:.0} m3, {:.0} m2, {} faces\n",
            a.name,
            a.slug,
            a.family,
            room.volume(),
            room.surface_area(),
            room.polygons.len()
        );
        let point = |obj: &mut String, p: Vec3| {
            obj.push_str(&format!("v {:.4} {:.4} {:.4}\n", p.x, p.z, p.y));
        };
        for polygon in &room.polygons {
            for v in &polygon.vertices {
                point(&mut obj, *v);
            }
        }
        let marks = [
            space.sources.centre,
            space.sources.left,
            space.sources.right,
        ];
        for m in marks {
            point(&mut obj, m);
        }
        for p in &space.positions {
            point(&mut obj, p.receiver);
        }
        let mut next = 1;
        let mut group = String::new();
        for polygon in &room.polygons {
            let name = room.materials[polygon.material]
                .name
                .replace([' ', ',', '(', ')', '%'], "_");
            if name != group {
                obj.push_str(&format!("g {name}\n"));
                group = name;
            }
            let face: Vec<String> = (0..polygon.vertices.len())
                .map(|i| (next + i).to_string())
                .collect();
            obj.push_str(&format!("f {}\n", face.join(" ")));
            next += polygon.vertices.len();
        }
        obj.push_str("g sources_and_receivers\n");
        for i in 0..marks.len() + space.positions.len() {
            obj.push_str(&format!("p {}\n", next + i));
        }
        let path = dir.join(format!("{}.obj", a.slug));
        std::fs::write(&path, obj)?;
        println!(
            "{:22} {:5} faces  {}",
            a.slug,
            room.polygons.len(),
            path.display()
        );
    }
    Ok(true)
}

fn estimate(slugs: &[String]) -> Result<bool> {
    for a in selected(slugs)? {
        let space = (a.build)()?;
        let t = catalogue::eyring_octaves(&space.room, &space.air);
        let wave = if space.wave_solved() {
            let scene = Scene::new(
                "estimate",
                space.room.clone(),
                space.air,
                space.sources.centre,
                space.positions[0].receiver,
            )?;
            format!(
                "wave, seam {:.0} Hz",
                space.seam_hz.unwrap_or_else(|| schroeder_seam(&scene))
            )
        } else {
            "geometric".to_string()
        };
        println!(
            "{:<22} {:<10} {:>9.0} m³  Eyring 125–4k: {}  max {} s, {wave}  ({})",
            a.slug,
            a.family,
            space.room.volume(),
            seconds(t.map(Some)),
            space.max_time_s,
            space.reference.basis
        );
    }
    Ok(true)
}

/// A position's three scenes, in [`SOURCES`] order. All three are hashed into the key, rendered or
/// not, so a moved side source makes even a centre-only render stale; that is conservative.
fn scenes(space: &Space, position: &Position, stem: &str) -> Result<[Scene; 3]> {
    let s = space.sources;
    let make = |name: &str, source: Vec3| -> Result<Scene> {
        let scene = Scene::new(
            format!("{stem}.{name}"),
            space.room.clone(),
            space.air,
            source,
            position.receiver,
        )?;
        Ok(if space.non_diffuse {
            scene.non_diffuse()
        } else {
            scene
        })
    };
    Ok([
        make(SOURCES[0], s.centre)?,
        make(SOURCES[1], s.left)?,
        make(SOURCES[2], s.right)?,
    ])
}

/// Everything a position's render depends on that the code can name, and which sources it holds.
fn render_key(space: &Space, scenes: &[Scene; 3], rays: usize, true_stereo: bool) -> String {
    let mut key = format!(
        "generator {}\nrecipe {RECIPE_EPOCH}\nrays {rays}\nmax_time_s {}\nwave {}\nseam_hz {:?}\n\
         image_source_order {IMAGE_SOURCE_ORDER}\nsample_rate {SAMPLE_RATE}\ncap {} {}\n\
         pair {PAIR_SPACING_M} {PAIR_HALF_ANGLE_DEG}\nscenes {} {} {}\n",
        env!("CARGO_PKG_VERSION"),
        space.max_time_s,
        space.wave_solved(),
        // The seam the solver will use, so a change to the automatic seam invalidates the cache.
        space
            .wave_solved()
            .then(|| space.seam_hz.unwrap_or_else(|| schroeder_seam(&scenes[0]))),
        CAP.length_s,
        CAP.fade_s,
        scenes[0].hash_hex(),
        scenes[1].hash_hex(),
        scenes[2].hash_hex()
    );
    if !true_stereo {
        key.push_str(CENTRE_ONLY);
    }
    key
}

/// What a rendered position holds: the centre source's summary, and the side sources' when the
/// true-stereo pair was rendered.
struct Rendered3 {
    centre: Summary,
    sides: Option<[Summary; 2]>,
}

fn read_summary(dir: &Path, stem: &str, source: &str) -> Option<Summary> {
    std::fs::read_to_string(dir.join(format!("{stem}.{source}.summary.tsv")))
        .ok()
        .and_then(|t| Summary::from_tsv(&t))
}

/// A position rendered from exactly the current scenes and options, holding the pair if
/// `true_stereo` asks for it; `None` if it is missing, incomplete, stale or lacks the pair.
fn load_position(
    a: &Archetype,
    space: &Space,
    position: &Position,
    dir: &Path,
    rays: usize,
    true_stereo: bool,
) -> Result<Option<Rendered3>> {
    let stem = format!("{}.{}", a.slug, position.name);
    let scenes = scenes(space, position, &stem)?;
    let Ok(found) = std::fs::read_to_string(dir.join(format!("{stem}.key"))) else {
        return Ok(None);
    };
    let with_pair = found == render_key(space, &scenes, rays, true);
    let centre_only = found == render_key(space, &scenes, rays, false);
    if !(with_pair || centre_only && !true_stereo) {
        return Ok(None);
    }
    let Some(centre) = read_summary(dir, &stem, SOURCES[0]) else {
        return Ok(None);
    };
    let sides = if with_pair {
        match (
            read_summary(dir, &stem, SOURCES[1]),
            read_summary(dir, &stem, SOURCES[2]),
        ) {
            (Some(l), Some(r)) => Some([l, r]),
            _ => return Ok(None),
        }
    } else {
        None
    };
    Ok(Some(Rendered3 { centre, sides }))
}

/// `render [--rays=N] [--true-stereo] [--near] [slug ...]`. Another ray count renders beside the catalogue,
/// under `catalogue-rays-N/`, for a convergence check (V10); `v9` and `release` never read it.
fn render(args: &[String]) -> Result<bool> {
    let mut rays = RAYS;
    let mut true_stereo = false;
    let mut near = false;
    let mut slugs = Vec::new();
    for arg in args {
        if arg == "--true-stereo" {
            true_stereo = true;
        } else if arg == "--near" {
            near = true;
        } else if let Some(n) = arg.strip_prefix("--rays=") {
            rays = n.parse().map_err(|_| format!("bad ray count `{n}`"))?;
        } else {
            slugs.push(arg.clone());
        }
    }
    let root = if rays == RAYS {
        cache_dir()
    } else {
        cache_dir().with_file_name(format!("catalogue-rays-{rays}"))
    };
    for a in selected(&slugs)? {
        let space = (a.build)()?;
        let dir = root.join(a.slug);
        std::fs::create_dir_all(&dir)?;
        for position in covered(&space, near) {
            let stem = format!("{}.{}", a.slug, position.name);
            if load_position(&a, &space, position, &dir, rays, true_stereo)?.is_some() {
                println!("{stem}: cached");
                continue;
            }
            render_position(&a, &space, position, rays, true_stereo, &dir, &stem)?;
        }
    }
    Ok(true)
}

fn render_position(
    a: &Archetype,
    space: &Space,
    position: &Position,
    rays: usize,
    true_stereo: bool,
    dir: &Path,
    stem: &str,
) -> Result<()> {
    let started = Instant::now();
    let frame = Frame::facing(position.receiver, space.sources.centre);
    let mono = Array::mono().placed(&frame)?;
    let pair = Array::near_coincident_cardioids(PAIR_SPACING_M, PAIR_HALF_ANGLE_DEG);
    let pair_capsules = pair.placed(&frame)?;
    let options = SimulationOptions {
        image_sources: ImageSourceOptions {
            max_order: IMAGE_SOURCE_ORDER,
            ..Default::default()
        },
        rays: RayOptions {
            rays,
            max_time_s: space.max_time_s,
            ..Default::default()
        },
        diffraction: None,
        wave: space.wave_solved().then(|| WaveOptions {
            seam_hz: space.seam_hz,
            ..WaveOptions::for_arrays(&[&mono, &pair_capsules])
        }),
    };
    let scenes = scenes(space, position, stem)?;
    let key = render_key(space, &scenes, rays, true_stereo);
    let rendered_sources = if true_stereo { 3 } else { 1 };
    let sims = scenes[..rendered_sources]
        .iter()
        .map(|scene| simulate(scene, &options))
        .collect::<std::result::Result<Vec<Simulation>, _>>()?;
    let simulated = started.elapsed();

    let full = RenderOptions {
        sample_rate: SAMPLE_RATE,
        include_direct: true,
        cap: None,
        ..Default::default()
    };
    let released = RenderOptions {
        include_direct: false,
        cap: Some(CAP),
        ..full
    };
    let mut fulls: Vec<Rendered> = Vec::new();
    let mut analyses: Vec<Analysis> = Vec::new();
    for sim in &sims {
        let rendered = render_set(&[sim], &mono, &full)?.remove(0);
        analyses.push(
            analyse(rendered.samples(), f64::from(rendered.sample_rate))
                .ok_or("a full response is silent")?,
        );
        fulls.push(rendered);
    }
    let stereo = render_set(&[&sims[0]], &pair_capsules, &released)?.remove(0);
    let sides = if true_stereo {
        render_set(&[&sims[1], &sims[2]], &pair_capsules, &released)?
    } else {
        Vec::new()
    };

    let surfaces: Vec<&str> = space
        .room
        .materials
        .iter()
        .map(|m| m.name.as_str())
        .collect();
    let decay = decay_name(space.reference.decay);
    let catalogue_json = JsonObject::new()
        .string("archetype", a.slug)
        .string("name", a.name)
        .string("position", position.name)
        .string("occupancy", space.occupancy)
        .raw(
            "class_reference",
            JsonObject::new()
                .boolean("gated", space.reference.gated)
                .string("decay", decay)
                .string("basis", space.reference.basis)
                .finish(),
        )
        .strings("surfaces", &surfaces)
        .finish();
    let mut extra = vec![("catalogue".to_string(), catalogue_json)];
    if let Some(w) = &sims[0].wave {
        extra.push((
            "wave_solver".into(),
            JsonObject::new()
                .number("seam_hz", w.seam_hz)
                .number("solver_rate_hz", w.sample_rate)
                .number("grid_spacing_m", w.spacing_m)
                .integer("cells", w.cells as u64)
                .finish(),
        ));
    }
    let mut notes = vec![
        "R5 catalogue: generic geometry, published material rows (plan section 7)",
        if space.wave_solved() {
            "wave solver below the seam, geometric hybrid above"
        } else {
            "geometric hybrid; no wave solver (a large space, plan section 7)"
        },
        "direct sound excluded from released files; metrics are from this source's full-response render",
        "no edge diffraction: every receiver sees its centre source (plan revision 13)",
    ];
    notes.extend(space.notes.iter().copied());
    let pair_name = pair.name.as_str();
    /// A file of the position: kind, source index, array name, capsules, options, render.
    type FileSpec<'a> = (
        &'a str,
        usize,
        &'a str,
        &'a [PlacedCapsule],
        &'a RenderOptions,
        &'a Rendered,
    );
    let mut files: Vec<FileSpec<'_>> = vec![
        ("full", 0, "mono omni", &mono, &full, &fulls[0]),
        (KINDS[0], 0, pair_name, &pair_capsules, &released, &stereo),
    ];
    if true_stereo {
        files.extend([
            ("full-left", 1, "mono omni", &mono[..], &full, &fulls[1]),
            ("full-right", 2, "mono omni", &mono[..], &full, &fulls[2]),
            (
                KINDS[1],
                1,
                pair_name,
                &pair_capsules[..],
                &released,
                &sides[0],
            ),
            (
                KINDS[2],
                2,
                pair_name,
                &pair_capsules[..],
                &released,
                &sides[1],
            ),
        ]);
    }
    for (kind, source, array_name, capsules, render, rendered) in files {
        let sidecar = Sidecar {
            simulation: &sims[source],
            array_name,
            capsules,
            render,
            rendered,
            full_response_analysis: Some(&analyses[source]),
            extra: extra.clone(),
            notes: &notes,
        };
        let channels: Vec<&[f32]> = rendered.channels.iter().map(Vec::as_slice).collect();
        let base = dir.join(format!("{stem}.{kind}"));
        wav::write_f32(&with_suffix(&base, ".wav"), &channels, rendered.sample_rate)?;
        std::fs::write(with_suffix(&base, ".json"), sidecar.to_json())?;
    }
    let summaries: Vec<Summary> = analyses.iter().map(Summary::from_analysis).collect();
    for (summary, source) in summaries.iter().zip(SOURCES) {
        std::fs::write(
            dir.join(format!("{stem}.{source}.summary.tsv")),
            summary.to_tsv(),
        )?;
    }
    // Written last: its presence and match mean the position is complete.
    std::fs::write(dir.join(format!("{stem}.key")), key)?;
    let peak = std::iter::once(&stereo)
        .chain(sides.iter())
        .flat_map(|r| r.channels.iter().flatten())
        .fold(0f32, |m, v| m.max(v.abs()));
    println!(
        "{stem}: simulated in {simulated:.1?}, rendered in {:.1?}{}; {decay} 125–4k {}; stereo {:.2} s{}, peak {peak:.4}",
        started.elapsed() - simulated,
        sims[0].wave.as_ref().map_or(String::new(), |w| format!(
            " (wave: seam {:.0} Hz, {} cells)",
            w.seam_hz, w.cells
        )),
        seconds(summaries[0].octaves(space.reference.decay)),
        stereo.duration_s(),
        if true_stereo {
            " with the true-stereo pair"
        } else {
            ""
        }
    );
    Ok(())
}

fn v9(near: bool) -> Result<bool> {
    let mut report = String::from(
        "# V9: catalogue decay times against class references\n\n\
         Generated by `cargo run -p mxm-room-ir --release --bin catalogue -- v9` from the centre \
         source's full-response render. A value passes inside the published range widened by the \
         5 % JND (or a figure's stated tolerance). Only gated classes decide the verdict.\n\n",
    );
    let mut pass = true;
    for a in ARCHETYPES {
        let space = (a.build)()?;
        let r = space.reference;
        let decay = decay_name(r.decay);
        report.push_str(&format!(
            "## {} (`{}`, {})\n\n{}: {}. Occupancy: {}.\n\n| Position | {decay} 125 / 250 / 500 / 1k / 2k / 4k Hz, s | Compared | Verdict |\n|---|---|---|---|\n",
            a.name,
            a.slug,
            a.family,
            if r.gated { "Gated" } else { "Reported" },
            r.basis,
            space.occupancy
        ));
        let dir = cache_dir().join(a.slug);
        for p in covered(&space, near) {
            let Some(rendered) = load_position(&a, &space, p, &dir, RAYS, false)? else {
                report.push_str(&format!(
                    "| {} | not rendered from the current scenes | | |\n",
                    p.name
                ));
                pass &= !r.gated;
                continue;
            };
            let octaves = rendered.centre.octaves(r.decay);
            let comparisons = r.compare(&octaves);
            let within = comparisons.iter().all(|c| c.within);
            let compared: Vec<String> = comparisons
                .iter()
                .map(|c| {
                    format!(
                        "{} {} in {:.2}–{:.2}{}",
                        c.label,
                        c.value.map_or("—".into(), |v| format!("{v:.2}")),
                        c.range.0,
                        c.range.1,
                        if c.within { "" } else { " ✗" }
                    )
                })
                .collect();
            let verdict = match (r.gated, within) {
                (true, true) => "pass",
                (true, false) => "**FAIL**",
                (false, true) => "within",
                (false, false) => "outside",
            };
            pass &= !r.gated || within;
            report.push_str(&format!(
                "| {} | {} | {} | {verdict} |\n",
                p.name,
                seconds(octaves),
                compared.join("; ")
            ));
        }
        report.push('\n');
    }
    let path = cache_dir().join("v9-class-ranges.md");
    std::fs::create_dir_all(cache_dir())?;
    std::fs::write(&path, &report)?;
    print!("{report}");
    println!("wrote {}", path.display());
    Ok(pass)
}

/// `release [--true-stereo] [--near]`: the stereo file of every far position, the near positions'
/// too with `--near`, and the true-stereo pair too with `--true-stereo`; whatever is asked for must
/// have been rendered.
fn release(true_stereo: bool, near: bool) -> Result<bool> {
    struct Item {
        archetype: Archetype,
        position: &'static str,
        kind: &'static str,
        decoded: wav::Decoded,
        sidecar: String,
        summary: Summary,
    }
    let mut items = Vec::new();
    let mut missing = Vec::new();
    for a in ARCHETYPES {
        let space = (a.build)()?;
        let dir = cache_dir().join(a.slug);
        for p in covered(&space, near) {
            let stem = format!("{}.{}", a.slug, p.name);
            let Some(rendered) = load_position(&a, &space, p, &dir, RAYS, true_stereo)? else {
                missing.push(stem);
                continue;
            };
            let mut kinds = vec![(KINDS[0], rendered.centre)];
            if let (true, Some([left, right])) = (true_stereo, rendered.sides) {
                kinds.push((KINDS[1], left));
                kinds.push((KINDS[2], right));
            }
            for (kind, summary) in kinds {
                let base = dir.join(format!("{stem}.{kind}"));
                items.push(Item {
                    archetype: a,
                    position: p.name,
                    kind,
                    decoded: wav::read(&with_suffix(&base, ".wav"))?,
                    sidecar: std::fs::read_to_string(with_suffix(&base, ".json"))?,
                    summary,
                });
            }
        }
    }
    if !missing.is_empty() {
        return Err(format!(
            "not rendered from the current scenes{}: {}",
            if true_stereo {
                " with the true-stereo pair (render with --true-stereo)"
            } else {
                ""
            },
            missing.join(", ")
        )
        .into());
    }
    // Everything is encoded and checked before anything is written, so a failure leaves the
    // release folder as it was.
    struct Output {
        relative: String,
        metadata: String,
        bytes: Vec<u8>,
        sidecar: String,
    }
    let mut outputs = Vec::new();
    let mut entries = Vec::new();
    let mut per_archetype: Vec<(&str, u64)> = Vec::new();
    for item in items {
        let (slug, family) = (item.archetype.slug, item.archetype.family);
        // Renders arrive normalized. A gain of 1 up to rounding re-normalizes defensively, and the
        // sidecar's reference distance, which already carries the render's gain, takes it too.
        let cached_peak = item
            .decoded
            .channels
            .iter()
            .flatten()
            .fold(0f32, |m, v| m.max(v.abs()));
        let gain = catalogue::normalizing_gain(f64::from(cached_peak))
            .ok_or_else(|| format!("{slug}.{}.{} is silent", item.position, item.kind))?;
        let reference = catalogue::top_level_number(&item.sidecar, "reference_distance_m")
            .ok_or("a sidecar lacks its reference distance")?
            * gain;
        let scaled: Vec<Vec<f32>> = item
            .decoded
            .channels
            .iter()
            .map(|c| c.iter().map(|v| v * gain as f32).collect())
            .collect();
        let channels: Vec<&[f32]> = scaled.iter().map(Vec::as_slice).collect();
        let bytes = wav::encode_f32(&channels, item.decoded.sample_rate)?;
        let decoded = wav::decode(&bytes)?;
        catalogue::consumer_check(&decoded)
            .map_err(|why| format!("{slug}.{}.{}: {why}", item.position, item.kind))?;
        let peak = scaled.iter().flatten().fold(0f32, |m, v| m.max(v.abs()));
        let sidecar =
            catalogue::patch_top_level_number(&item.sidecar, "reference_distance_m", reference)
                .and_then(|j| catalogue::patch_top_level_number(&j, "peak_abs", f64::from(peak)))
                .ok_or("a sidecar lacks its level fields")?;
        let relative = format!("{family}/{slug}/{slug}.{}.{}", item.position, item.kind);
        // The sidecars sit apart from the audio, so a room's folder shows only its WAVs.
        let metadata = format!(
            "{family}/{slug}/metadata/{slug}.{}.{}.json",
            item.position, item.kind
        );
        let size = (bytes.len() + sidecar.len()) as u64;
        match per_archetype.last_mut() {
            Some((s, b)) if *s == slug => *b += size,
            _ => per_archetype.push((slug, size)),
        }
        let frames = decoded.channels[0].len();
        let t30: Vec<String> = item
            .summary
            .octaves(Decay::T30)
            .iter()
            .map(|v| v.map_or("null".into(), |v| format!("{v:.3}")))
            .collect();
        entries.push(
            JsonObject::new()
                .string("file", &format!("{relative}.wav"))
                .string("metadata", &metadata)
                .string("archetype", slug)
                .string("family", family)
                .string("name", item.archetype.name)
                .string("position", item.position)
                .string("kind", item.kind)
                .number("reference_distance_m", reference)
                .number(
                    "gain_db",
                    (20.0 * reference.log10() * 100.0).round() / 100.0,
                )
                .integer("channels", decoded.channels.len() as u64)
                .integer("sample_rate_hz", u64::from(decoded.sample_rate))
                .integer("frames", frames as u64)
                .number("duration_s", frames as f64 / f64::from(decoded.sample_rate))
                .number(
                    "peak_dbfs",
                    (20.0 * f64::from(peak).log10() * 100.0).round() / 100.0,
                )
                .raw("t30_125_to_4k_s", format!("[{}]", t30.join(", ")))
                .finish(),
        );
        outputs.push(Output {
            relative,
            metadata,
            bytes,
            sidecar,
        });
    }
    let manifest = JsonObject::new()
        .string("generator", concat!("mxm-room-ir ", env!("CARGO_PKG_VERSION")))
        .string("format", "32-bit float WAV, 48 kHz, direct sound excluded")
        .boolean("true_stereo", true_stereo)
        .boolean("near_positions", near)
        .string(
            "level",
            "each file normalized to its own peak; an entry's reference_distance_m is the distance at which the direct sound has pressure 1, and gain_db its gain against a render at 1 m",
        )
        .number("file_peak_dbfs", catalogue::FILE_PEAK_DBFS)
        .integer("files", entries.len() as u64)
        .raw("entries", format!("[\n    {}\n  ]", entries.join(",\n    ")))
        .pretty();

    // Written beside the folder, then swapped in: no file of an earlier release survives.
    let staging = crate_dir().join("impulses.staging");
    if staging.exists() {
        std::fs::remove_dir_all(&staging)?;
    }
    let mut total_bytes = manifest.len() as u64;
    for o in &outputs {
        let base = staging.join(&o.relative);
        std::fs::create_dir_all(base.parent().ok_or("no parent")?)?;
        std::fs::write(with_suffix(&base, ".wav"), &o.bytes)?;
        let metadata = staging.join(&o.metadata);
        std::fs::create_dir_all(metadata.parent().ok_or("no parent")?)?;
        std::fs::write(metadata, &o.sidecar)?;
        total_bytes += (o.bytes.len() + o.sidecar.len()) as u64;
    }
    std::fs::write(staging.join("manifest.json"), &manifest)?;
    let out = impulses_dir();
    if out.exists() {
        std::fs::remove_dir_all(&out)?;
    }
    std::fs::rename(&staging, &out)?;
    for (slug, bytes) in &per_archetype {
        println!("{slug:<22} {:>7.1} MB", *bytes as f64 / 1e6);
    }
    println!(
        "{} files{}, {:.1} MB, each normalized to {} dBFS; wrote {}",
        entries.len(),
        if true_stereo {
            " with true stereo"
        } else {
            ", stereo only"
        },
        total_bytes as f64 / 1e6,
        catalogue::FILE_PEAK_DBFS,
        out.display()
    );
    Ok(true)
}
