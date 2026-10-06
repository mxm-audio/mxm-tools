//! Renders the HUD with a sound dropped on it to a PNG, so its look can be reviewed without opening
//! the window. A local, manual run: the sound and the picture stay on this machine.
//!
//! ```text
//! cargo run -p mxm-listener-hud --release --example listener_hud_shot -- <file> <out.png>
//!     [--mode ring|radar|list] [--zoom PART:LEVEL] [--family note|held|synth|ir|hit] [--size WxH]
//! ```
//! Without a file (`-` in its place) it draws the waiting HUD.

use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::{Duration, Instant};

use egui_kittest::Harness;
use mxm_listener_hud::Hud;
use mxm_listener_hud::glyphs::Focus;
use mxm_listener_hud::hud::{Mode, Ring};

#[derive(Debug)]
struct Dropped(PathBuf);

impl egui::DroppedFile for Dropped {
    fn path(&self) -> &Path {
        &self.0
    }

    fn bytes(&self) -> Result<Vec<u8>, String> {
        std::fs::read(&self.0).map_err(|e| e.to_string())
    }
}

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let (Some(file), Some(out)) = (args.first(), args.get(1)) else {
        eprintln!(
            "usage: listener_hud_shot <file|-> <out.png> [--mode ring|radar|list] [--zoom PART:LEVEL] [--family …] [--size WxH]"
        );
        std::process::exit(2);
    };
    let value = |flag: &str| {
        args.iter()
            .position(|a| a == flag)
            .and_then(|i| args.get(i + 1))
            .cloned()
    };
    let size = value("--size")
        .and_then(|s| {
            let (w, h) = s.split_once('x')?;
            Some(egui::vec2(w.parse().ok()?, h.parse().ok()?))
        })
        .unwrap_or(egui::vec2(1600.0, 900.0));

    let mut hud = Hud::new();
    if let Some(ring) = value("--family").and_then(|f| {
        Ring::ALL
            .into_iter()
            .find(|r| r.label().eq_ignore_ascii_case(&f))
    }) {
        hud.set_ring(ring);
    }
    hud.set_mode(match value("--mode").as_deref() {
        Some("radar") => Mode::Radar,
        Some("list") => Mode::List,
        _ => Mode::Ring,
    });
    let mut harness = Harness::builder()
        .with_size(size)
        .build_ui_state(|ui, hud: &mut Hud| hud.ui(ui), hud);

    if file != "-" {
        harness
            .input_mut()
            .dropped_files
            .push(Arc::new(Dropped(PathBuf::from(file))));
        // The first frame takes the drop; only then is there a reading to wait for.
        harness.step();
        let started = Instant::now();
        while harness.state().current().is_some_and(|c| c.busy()) {
            assert!(
                started.elapsed() < Duration::from_secs(600),
                "never finished"
            );
            harness.step();
            std::thread::sleep(Duration::from_millis(20));
        }
    }
    // One picture without a zoom, then one per `--zoom PART:LEVEL[,PART:LEVEL…]`, named
    // `<out stem>-<part>-<level>.png` beside it: the sound is read once for them all.
    let mut shots = vec![(None, PathBuf::from(out))];
    for z in value("--zoom").iter().flat_map(|z| z.split(',')) {
        let Some((part, level)) = z
            .split_once(':')
            .and_then(|(p, l)| Some((p.to_string(), l.parse::<u8>().ok()?)))
        else {
            continue;
        };
        let path = Path::new(out).with_file_name(format!(
            "{}-{part}-{level}.png",
            Path::new(out)
                .file_stem()
                .map_or("shot".into(), |s| s.to_string_lossy())
        ));
        shots.push((Some((part, level)), path));
    }
    for (zoom, path) in shots {
        let focus = zoom.and_then(|(part, level)| {
            harness
                .state()
                .current()
                .and_then(|c| c.scene.as_ref())
                .and_then(|s| s.parts.iter().find(|p| p.part == part))
                .map(|p| Focus {
                    part: p.part,
                    level,
                })
        });
        harness.state_mut().set_focus(focus);
        harness.run_steps(8);
        let image = harness.render().expect("the HUD renders");
        image.save(&path).expect("the PNG is written");
        println!("wrote {}", path.display());
    }
}
