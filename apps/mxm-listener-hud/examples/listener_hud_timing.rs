//! How long the window's stages take on real files (the plan's §2.3 measurement): the load, the
//! first readout without the perceptual models, and the full reading. A local, manual run; it reads
//! whatever files it is given and prints each by its stem only.
//!
//! ```text
//! cargo run -p mxm-listener-hud --release --example listener_hud_timing -- <file>… [--family note]
//! ```

use std::path::PathBuf;
use std::time::Instant;

use mxm_listener_hud::analysis::{self, Request, Stage};
use mxm_listening::Family;

fn main() {
    let mut family = None;
    let mut files = Vec::new();
    let mut args = std::env::args().skip(1);
    while let Some(arg) = args.next() {
        if arg == "--family" {
            family = args.next().and_then(|f| Family::parse(&f));
        } else {
            files.push(PathBuf::from(arg));
        }
    }
    println!("sound\tseconds\tloaded s\tfirst readout s\tfull s\trebuilt s\treadings");
    for path in files {
        let request = Request {
            name: analysis::display_name(&path),
            path,
            family,
            expect_hz: None,
        };
        let started = Instant::now();
        let (mut duration, mut loaded, mut quick, mut full, mut rebuilt, mut count) =
            (0.0, 0.0, 0.0, 0.0, 0.0, 0);
        analysis::analyse(&request, &mut |stage| {
            match stage {
                Stage::Curves(_) => {}
                Stage::Rebuilt(_) => rebuilt = started.elapsed().as_secs_f64(),
                Stage::Loaded(l) => {
                    duration = l.duration_s;
                    loaded = started.elapsed().as_secs_f64();
                }
                Stage::Refused(why) => println!("{}\trefused: {why}", request.name),
                Stage::Read {
                    report,
                    perception,
                    took,
                } => {
                    count = report.sections.iter().map(|s| s.readings.len()).sum();
                    if perception {
                        full = took.as_secs_f64();
                    } else {
                        quick = took.as_secs_f64();
                    }
                }
            }
            true
        });
        println!(
            "{}\t{duration:.2}\t{loaded:.3}\t{quick:.2}\t{full:.2}\t{rebuilt:.2}\t{count}",
            request.name
        );
    }
}
