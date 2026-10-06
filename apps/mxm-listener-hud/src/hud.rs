//! The window: its state, and the HUD (the plan's H2).
//!
//! On the left the drop area, the family ring, what the name claims, the notices and the log — the
//! listener's real steps as they land. In the middle the pitch (`scope`), the parts orbiting it
//! (`glyphs`) and, when the wheel zooms into one, its inspector; along the bottom the sound through
//! time (`timeline`). RING, RADAR and LIST choose the view. Motion that stands for work runs only while
//! the listener works; the rest of the motion is the look, and every number is the listener's.

use std::path::PathBuf;
use std::sync::mpsc::Receiver;
use std::sync::{Arc, OnceLock};
use std::time::{Duration, Instant};

use egui::{Align2, Color32, Rect, RichText, Sense, Stroke, Ui, UiBuilder, pos2, vec2};
use mxm_listening::curves::Curves;
use mxm_listening::family::Family;
use mxm_listening::name::{self, Agreement, Claims};
use mxm_listening::reading::Report;

use crate::analysis::{self, Loaded, Rebuilt, Request, Stage};
use crate::glyphs::{self, Focus, Knowledge};
use crate::list::{self, codec_name, container_name};
use crate::playback::{Playback, Which};
use crate::scene::{self, Lock, Scene, with_unit};
use crate::scope::{self, Geometry, View};
use crate::theme::{
    AMBER, BG, BRIGHT, CYAN, DIM, GRID, PANEL, TEXT, brackets, fade, glow_text, mono, radial,
};
use crate::timeline;
use crate::worker::{Outcome, Worker};

/// The family ring: the listener's own detection, or a family declared. Notes, held notes, voices and
/// impulse responses are declared, never detected (`crates/mxm-listening/AGENTS.md`); turning the
/// ring reads the sound again.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Ring {
    Auto,
    Hit,
    Note,
    Held,
    Synth,
    Ir,
}

impl Ring {
    pub const ALL: [Ring; 6] = [
        Ring::Auto,
        Ring::Hit,
        Ring::Note,
        Ring::Held,
        Ring::Synth,
        Ring::Ir,
    ];

    /// HIT, not DRUM: the listener establishes a percussive envelope, not a drum.
    #[must_use]
    pub fn label(self) -> &'static str {
        match self {
            Ring::Auto => "AUTO",
            Ring::Hit => "HIT",
            Ring::Note => "NOTE",
            Ring::Held => "HELD",
            Ring::Synth => "SYNTH",
            Ring::Ir => "IR",
        }
    }

    /// The ring position that declares `family`; `None` for one no caller declares.
    #[must_use]
    pub fn of(family: Family) -> Option<Ring> {
        Ring::ALL.into_iter().find(|r| r.family() == Some(family))
    }

    /// The family declared to the listener; `None` lets it detect one.
    #[must_use]
    pub fn family(self) -> Option<Family> {
        match self {
            Ring::Auto => None,
            Ring::Hit => Some(Family::Percussive),
            Ring::Note => Some(Family::Note),
            Ring::Held => Some(Family::Sustained),
            Ring::Synth => Some(Family::Voice),
            Ring::Ir => Some(Family::Impulse),
        }
    }
}

/// Which view fills the middle.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Mode {
    /// The note ring: pitch class around, octave outward.
    Ring,
    /// The radar: time around, frequency outward.
    Radar,
    /// Every reading as a list.
    List,
}

/// Everything known about the sound on screen.
#[derive(Debug)]
pub struct Current {
    pub seq: u64,
    pub name: String,
    /// What the name claims: shown beside what the listener hears, never in its place.
    pub claims: Claims,
    pub loaded: Option<Loaded>,
    pub refused: Option<String>,
    pub failed: Option<String>,
    /// The first readout, without the perceptual models, and its time from the drop.
    pub quick: Option<(Report, Duration)>,
    /// The full reading, and its time from the drop.
    pub full: Option<(Report, Duration)>,
    /// What the window draws.
    pub curves: Option<Arc<Curves>>,
    /// What the HUD shows of the newest reading.
    pub scene: Option<Scene>,
    /// The self-test's rebuild, to play; `None` until it arrives or where there is none.
    pub rebuilt: Option<Arc<Rebuilt>>,
    /// The listener's steps as they landed, each with its time from the drop.
    pub log: Vec<(f64, String)>,
    pub dropped: Instant,
}

impl Current {
    fn new(seq: u64, name: String, claims: Claims) -> Self {
        Self {
            seq,
            name,
            claims,
            loaded: None,
            refused: None,
            failed: None,
            quick: None,
            full: None,
            curves: None,
            scene: None,
            rebuilt: None,
            log: Vec::new(),
            dropped: Instant::now(),
        }
    }

    /// The newest reading: the full one once it has arrived.
    #[must_use]
    pub fn report(&self) -> Option<&Report> {
        self.full
            .as_ref()
            .or(self.quick.as_ref())
            .map(|(report, _)| report)
    }

    /// Still listening: nothing final has arrived.
    #[must_use]
    pub fn busy(&self) -> bool {
        self.refused.is_none() && self.failed.is_none() && self.full.is_none()
    }

    fn log(&mut self, at: f64, line: String) {
        self.log.push((at, line));
    }
}

/// The window's state: the worker, the ring, the view, the zoom and the sound on screen.
pub struct Hud {
    worker: Worker<Request>,
    updates: Receiver<(u64, Outcome<Stage>)>,
    /// The window's context, known from its first frame, so the worker can wake it.
    ctx: Arc<OnceLock<egui::Context>>,
    ring: Ring,
    /// Whether the name's note is handed to the listener to find a pitched note's fundamental by.
    trust_name: bool,
    /// The sound on screen's path, kept to read it again when the ring turns. Never shown.
    path: Option<PathBuf>,
    current: Option<Current>,
    mode: Mode,
    focus: Option<Focus>,
    know: Knowledge,
    styled: bool,
    playback: Playback,
    /// The waveform as last drawn, for the sound `seq` at a width: pooled once, not every frame.
    wave: Option<(u64, usize, Arc<timeline::Wave>)>,
}

impl Default for Hud {
    fn default() -> Self {
        Self::new()
    }
}

impl Hud {
    #[must_use]
    pub fn new() -> Self {
        let ctx: Arc<OnceLock<egui::Context>> = Arc::default();
        let wake = Arc::clone(&ctx);
        let (worker, updates) = Worker::spawn(analysis::analyse, move || {
            if let Some(ctx) = wake.get() {
                ctx.request_repaint();
            }
        });
        Self {
            worker,
            updates,
            ctx,
            ring: Ring::Auto,
            trust_name: false,
            path: None,
            current: None,
            mode: Mode::Ring,
            focus: None,
            know: Knowledge::load(),
            styled: false,
            playback: Playback::new(),
            wave: None,
        }
    }

    /// Reads a sound, superseding whatever was being read.
    pub fn open(&mut self, path: PathBuf) {
        let name = analysis::display_name(&path);
        let claims = name::claims(&name);
        let expect_hz = self
            .trust_name
            .then(|| claims.note.as_ref().map(|n| n.hz))
            .flatten();
        let seq = self.worker.submit(Request {
            path: path.clone(),
            name: name.clone(),
            family: self.ring.family(),
            expect_hz,
        });
        self.path = Some(path);
        self.playback.stop();
        self.playback.keep_only(seq);
        let mut current = Current::new(seq, name, claims);
        current.log(
            0.0,
            format!(
                "DROP {} {}",
                current.name,
                self.ring
                    .family()
                    .map_or("AUTO".to_string(), |f| f.name().to_uppercase())
            ),
        );
        self.current = Some(current);
    }

    /// Hands the name's note to the listener, or stops, reading the sound on screen again.
    pub fn set_trust_name(&mut self, trust: bool) {
        if trust == self.trust_name {
            return;
        }
        self.trust_name = trust;
        if let Some(path) = self.path.clone() {
            self.open(path);
        }
    }

    /// Turns the family ring, reading the sound on screen again under it.
    pub fn set_ring(&mut self, ring: Ring) {
        if ring == self.ring {
            return;
        }
        self.ring = ring;
        if let Some(path) = self.path.clone() {
            self.open(path);
        }
    }

    #[must_use]
    pub fn ring(&self) -> Ring {
        self.ring
    }

    #[must_use]
    pub fn mode(&self) -> Mode {
        self.mode
    }

    pub fn set_mode(&mut self, mode: Mode) {
        self.mode = mode;
    }

    #[must_use]
    pub fn focus(&self) -> Option<&Focus> {
        self.focus.as_ref()
    }

    pub fn set_focus(&mut self, focus: Option<Focus>) {
        self.focus = focus;
    }

    #[must_use]
    pub fn current(&self) -> Option<&Current> {
        self.current.as_ref()
    }

    /// Takes every update the worker has sent.
    pub fn poll(&mut self) {
        while let Ok((seq, outcome)) = self.updates.try_recv() {
            self.apply(seq, outcome);
        }
    }

    /// Takes one update. One for a sound no longer on screen is discarded: a slow reading never
    /// paints over a newer drop.
    pub fn apply(&mut self, seq: u64, outcome: Outcome<Stage>) {
        let Some(current) = self.current.as_mut().filter(|c| c.seq == seq) else {
            return;
        };
        let now = current.dropped.elapsed().as_secs_f64();
        match outcome {
            Outcome::Failed(why) => {
                current.log(now, format!("FAILED {why}"));
                current.failed = Some(why);
            }
            Outcome::Update(Stage::Refused(why)) => {
                current.log(now, format!("REFUSED {why}"));
                current.refused = Some(why);
            }
            Outcome::Update(Stage::Loaded(loaded)) => {
                current.log(
                    now,
                    format!(
                        "LOAD {} Hz {}CH {} {:.2}s",
                        loaded.rate,
                        loaded.kept.source_channels,
                        codec_name(loaded.kept.codec),
                        loaded.duration_s
                    ),
                );
                current.loaded = Some(loaded);
            }
            Outcome::Update(Stage::Rebuilt(rebuilt)) => {
                current.log(
                    now,
                    match &rebuilt {
                        Some(_) => "REBUILD MODES+NOISE".into(),
                        None => "NO REBUILD".into(),
                    },
                );
                current.rebuilt = rebuilt.map(|r| Arc::new(*r));
            }
            Outcome::Update(Stage::Curves(curves)) => {
                let frames = curves.spectrogram.as_ref().map_or(0, |s| s.times_ms.len());
                let points = curves.envelope.as_ref().map_or(0, |e| e.points.len());
                current.log(now, format!("CURVES ENV {points}pt SPEC {frames}fr"));
                current.curves = Some(Arc::new(*curves));
            }
            Outcome::Update(Stage::Read {
                report,
                perception,
                took,
            }) => {
                let scene = scene::build(&report);
                let at = took.as_secs_f64();
                if perception {
                    current.log(at, format!("PERCEPTION ECMA-418-2 {}rd", scene.readings));
                } else {
                    if let Some(onset) = report.onset_s {
                        current.log(at, format!("ONSET {onset:.3}s"));
                    }
                    current.log(
                        at,
                        format!(
                            "STAGE 1 {} {}rd {}pt",
                            report.family.family.name().to_uppercase(),
                            scene.readings,
                            scene.parts.len()
                        ),
                    );
                    current.log(
                        at,
                        match &scene.lock {
                            Lock::Locked {
                                note,
                                cents,
                                hz,
                                what,
                                ..
                            } => format!(
                                "LOCK {note} {cents:+.0}c {}Hz {}",
                                scene::number(*hz),
                                what.to_uppercase()
                            ),
                            Lock::NoLock { why } => format!("NO LOCK {why}"),
                        },
                    );
                }
                current.scene = Some(scene);
                let read = Some((*report, took));
                if perception {
                    current.full = read;
                } else {
                    current.quick = read;
                }
            }
        }
    }

    /// What the listener did not read, or read differently from the file.
    #[must_use]
    pub fn notices(&self) -> Vec<String> {
        self.current
            .as_ref()
            .and_then(|c| c.loaded.as_ref().map(|l| analysis::notices(l, c.report())))
            .unwrap_or_default()
    }

    /// One frame.
    pub fn ui(&mut self, ui: &mut Ui) {
        let _ = self.ctx.get_or_init(|| ui.ctx().clone());
        if !self.styled {
            style(ui.ctx());
            self.styled = true;
        }
        self.poll();
        self.playback.poll();
        if ui.input(|i| i.key_pressed(egui::Key::Space)) {
            self.toggle(Which::Listened);
        }
        let dropped = ui.ctx().input(|i| {
            i.raw
                .dropped_files
                .last()
                .map(|file| file.path().to_path_buf())
        });
        if let Some(path) = dropped {
            self.focus = None;
            self.open(path);
        }
        let hovering = ui.ctx().input(|i| !i.raw.hovered_files.is_empty());
        let time = ui.input(|i| i.time);

        let full = ui.max_rect();
        ui.painter().rect_filled(full, 0.0, BG);
        let top = Rect::from_min_size(full.min, vec2(full.width(), 46.0));
        let left = Rect::from_min_max(
            pos2(full.left(), top.bottom()),
            pos2(full.left() + 320.0, full.bottom()),
        );
        let main = Rect::from_min_max(pos2(left.right(), top.bottom()), full.max);

        self.top_bar(ui, top, time);
        let action = ui
            .scope_builder(UiBuilder::new().max_rect(left.shrink(12.0)), |ui| {
                self.left_column(ui, hovering, time)
            })
            .inner;
        match action {
            Some(Action::Ring(ring)) => self.set_ring(ring),
            Some(Action::TrustName(trust)) => self.set_trust_name(trust),
            Some(Action::Play(which)) => self.toggle(which),
            Some(Action::Stop) => self.playback.stop(),
            None => {}
        }
        ui.painter().line_segment(
            [
                pos2(left.right(), left.top() + 8.0),
                pos2(left.right(), left.bottom() - 8.0),
            ],
            Stroke::new(1.0, GRID),
        );

        match self.mode {
            Mode::List => {
                ui.scope_builder(UiBuilder::new().max_rect(main.shrink(12.0)), |ui| {
                    egui::ScrollArea::vertical()
                        .auto_shrink(false)
                        .show(ui, |ui| {
                            match self.current.as_ref().and_then(Current::report) {
                                Some(report) => list::report_list(ui, report),
                                None => {
                                    ui.label("Nothing read yet.");
                                }
                            }
                        });
                });
            }
            Mode::Ring | Mode::Radar => self.instrument(ui, main, time),
        }

        // The look moves on its own; kittest's `run` waits only on immediate repaints.
        ui.ctx().request_repaint_after(Duration::from_millis(16));
    }

    fn top_bar(&mut self, ui: &mut Ui, rect: Rect, time: f64) {
        let painter = ui.painter().clone();
        painter.rect_filled(rect, 0.0, PANEL);
        painter.line_segment(
            [rect.left_bottom(), rect.right_bottom()],
            Stroke::new(1.0, GRID),
        );
        glow_text(
            &painter,
            pos2(rect.left() + 16.0, rect.center().y),
            Align2::LEFT_CENTER,
            "MXM // LISTENER HUD",
            mono(15.0),
            CYAN,
        );
        // The headline: readings only.
        let mut x = rect.left() + 250.0;
        if let Some(scene) = self.current.as_ref().and_then(|c| c.scene.as_ref()) {
            let mut items: Vec<(String, String)> =
                vec![("FAMILY".into(), scene.family.name().to_uppercase())];
            if let Lock::Locked { note, cents, .. } = &scene.lock {
                items.push(("PITCH".into(), format!("{note} {cents:+.0}c")));
            }
            for h in &scene.headline {
                let key = h.id.rsplit('.').next().unwrap_or(h.id).replace('_', " ");
                items.push((key.to_uppercase(), with_unit(h)));
            }
            for (key, value) in items {
                let k = painter.text(
                    pos2(x, rect.center().y - 7.0),
                    Align2::LEFT_CENTER,
                    &key,
                    mono(9.0),
                    DIM,
                );
                let v = painter.text(
                    pos2(x, rect.center().y + 8.0),
                    Align2::LEFT_CENTER,
                    &value,
                    mono(12.0),
                    TEXT,
                );
                x = k.right().max(v.right()) + 22.0;
            }
        }
        // The status, and the views.
        let busy = self.current.as_ref().is_some_and(Current::busy);
        let status = match &self.current {
            None => ("AWAITING SIGNAL", DIM),
            Some(c) if c.refused.is_some() => ("REFUSED", AMBER),
            Some(c) if c.failed.is_some() => ("FAILED", AMBER),
            Some(_) if busy => ("SCANNING", CYAN),
            Some(c) => match c.scene.as_ref().map(|s| &s.lock) {
                Some(Lock::Locked { .. }) => ("LOCKED", CYAN),
                _ => ("NO LOCK", TEXT),
            },
        };
        let blink = busy && (time * 2.0).fract() < 0.5;
        let views = Rect::from_min_max(pos2(rect.right() - 330.0, rect.top()), rect.max);
        painter.text(
            pos2(views.left() - 12.0, rect.center().y),
            Align2::RIGHT_CENTER,
            if blink {
                format!("● {}", status.0)
            } else {
                format!("  {}", status.0)
            },
            mono(12.0),
            status.1,
        );
        ui.scope_builder(
            UiBuilder::new().max_rect(views.shrink2(vec2(8.0, 10.0))),
            |ui| {
                ui.horizontal_centered(|ui| {
                    for (mode, label) in [
                        (Mode::Ring, "RING"),
                        (Mode::Radar, "RADAR"),
                        (Mode::List, "LIST"),
                    ] {
                        if ui.selectable_label(self.mode == mode, label).clicked() {
                            self.mode = mode;
                        }
                    }
                });
            },
        );
    }

    fn left_column(&self, ui: &mut Ui, hovering: bool, time: f64) -> Option<Action> {
        let mut action = None;
        drop_area(ui, hovering, time);
        ui.add_space(8.0);
        ui.horizontal_wrapped(|ui| {
            ui.label(RichText::new("FAMILY").font(mono(10.0)).color(DIM));
            for ring in Ring::ALL {
                if ui
                    .selectable_label(self.ring == ring, ring.label())
                    .clicked()
                    && ring != self.ring
                {
                    action = Some(Action::Ring(ring));
                }
            }
        });
        ui.add_space(6.0);
        egui::ScrollArea::vertical().auto_shrink(false).show(ui, |ui| {
            let Some(current) = &self.current else {
                ui.label(
                    "Drop a sound file on the area above: WAV, AIFF, FLAC, ALAC, MP3, AAC or Vorbis.",
                );
                return;
            };
            ui.label(RichText::new(&current.name).font(mono(16.0)).color(BRIGHT));
            if let Some(loaded) = &current.loaded {
                ui.label(
                    RichText::new(format!(
                        "{} Hz · {:.2} s · {} in {}",
                        loaded.rate,
                        loaded.duration_s,
                        codec_name(loaded.kept.codec),
                        container_name(loaded.kept.container),
                    ))
                    .font(mono(10.0))
                    .color(DIM),
                );
            }
            if let Some(why) = &current.refused {
                ui.label(RichText::new(format!("Refused: {why}")).color(AMBER));
            }
            if let Some(why) = &current.failed {
                ui.label(
                    RichText::new(format!("The listener failed on this sound: {why}")).color(AMBER),
                );
            }
            match (&current.quick, &current.full) {
                (Some((_, quick)), Some((report, full))) => {
                    ui.label(
                        RichText::new(format!(
                            "First readout {:.2} s after the drop · the full reading {:.2} s",
                            quick.as_secs_f64(),
                            full.as_secs_f64()
                        ))
                        .font(mono(10.0)),
                    );
                    parts_line(ui, report);
                }
                (None, Some((report, full))) => {
                    ui.label(format!("Read in {:.2} s", full.as_secs_f64()));
                    parts_line(ui, report);
                }
                (Some((report, quick)), None) => {
                    ui.label(
                        RichText::new(format!(
                            "First readout {:.2} s after the drop; the perceptual models are still listening…",
                            quick.as_secs_f64()
                        ))
                        .font(mono(10.0)),
                    );
                    parts_line(ui, report);
                }
                (None, None) => {}
            }
            if let Some(a) = self.play_row(ui, current) {
                action = Some(a);
            }
            ui.add_space(6.0);
            if let Some(a) = self.claims(ui, current) {
                action = Some(a);
            }
            for notice in self.notices() {
                ui.label(RichText::new(format!("▸ {notice}")).font(mono(10.0)).color(TEXT));
            }
            ui.add_space(8.0);
            log(ui, current, time);
        });
        action
    }

    /// The waveform for the strip at `strip`, pooled once per sound and width.
    fn wave_for(
        &mut self,
        strip: Rect,
        curves: Option<&mxm_listening::curves::Curves>,
    ) -> Option<Arc<timeline::Wave>> {
        let c = self.current.as_ref()?;
        let loaded = c.loaded.as_ref()?;
        let span = timeline::span_ms(curves?);
        let columns = timeline::inner(strip).width().max(1.0) as usize;
        if let Some((seq, width, wave)) = &self.wave
            && *seq == c.seq
            && *width == columns
        {
            return Some(Arc::clone(wave));
        }
        let report = c.report()?;
        let wave = Arc::new(timeline::Wave {
            columns: timeline::wave_columns(
                &loaded.samples,
                loaded.rate,
                report.onset_s.unwrap_or(0.0),
                span,
                columns,
            ),
            peak_dbfs: report.find("level.peak", None).and_then(|r| r.value),
        });
        self.wave = Some((c.seq, columns, Arc::clone(&wave)));
        Some(wave)
    }

    /// The sound on screen's source for `which`, and its rate: the listened sound once loaded, the
    /// rebuild's parts once rebuilt.
    fn source(&self, which: Which) -> Option<(u64, Arc<[f32]>, u32)> {
        let c = self.current.as_ref()?;
        let (samples, rate) = match which {
            Which::Listened => {
                let l = c.loaded.as_ref()?;
                (Arc::clone(&l.samples), l.rate)
            }
            Which::Modes | Which::Noise | Which::Rebuild => {
                let r = c.rebuilt.as_ref()?;
                let part = match which {
                    Which::Modes => &r.modes,
                    Which::Noise => &r.noise,
                    _ => &r.whole,
                };
                (Arc::clone(part), r.rate)
            }
        };
        Some((c.seq, samples, rate))
    }

    /// Plays `which`, or stops it when it is what plays.
    fn toggle(&mut self, which: Which) {
        let playing = self.playhead().map(|(w, _)| w);
        if playing == Some(which) {
            self.playback.stop();
        } else if let Some((seq, samples, rate)) = self.source(which) {
            self.playback.play(seq, which, &samples, rate);
        }
    }

    /// What plays of the sound on screen, and where: ms from the report's onset, the timeline's axis.
    #[must_use]
    pub fn playhead(&self) -> Option<(Which, f64)> {
        let c = self.current.as_ref()?;
        let (seq, which, seconds) = self.playback.now()?;
        (seq == c.seq).then(|| {
            let onset = c.report().and_then(|r| r.onset_s).unwrap_or(0.0);
            (which, (seconds - onset) * 1000.0)
        })
    }

    /// The row of sounds to hear: the listened sound, and the rebuild's parts once they exist.
    fn play_row(&self, ui: &mut Ui, current: &Current) -> Option<Action> {
        current.loaded.as_ref()?;
        let mut action = None;
        let playing = self.playhead().map(|(w, _)| w);
        ui.add_space(4.0);
        ui.horizontal_wrapped(|ui| {
            ui.label(RichText::new("PLAY").font(mono(10.0)).color(DIM));
            for which in Which::ALL {
                let available = which == Which::Listened || current.rebuilt.is_some();
                let button = ui
                    .add_enabled(
                        available,
                        egui::Button::selectable(playing == Some(which), which.label()),
                    )
                    .on_hover_text(which.meaning());
                if button.clicked() {
                    action = Some(Action::Play(which));
                }
            }
            if ui.button("STOP").clicked() {
                action = Some(Action::Stop);
            }
        });
        if self.playback.preparing() {
            ui.label(
                RichText::new("preparing for the device…")
                    .font(mono(10.0))
                    .color(DIM),
            );
        }
        if let Some(why) = self.playback.notice() {
            ui.label(RichText::new(why).font(mono(10.0)).color(AMBER));
        }
        action
    }

    /// What the name claims, and — once there is a report — whether the listener hears the same.
    /// Words, not colour, say which (root *User Preferences*).
    fn claims(&self, ui: &mut Ui, current: &Current) -> Option<Action> {
        let claims = &current.claims;
        if claims.is_empty() {
            return None;
        }
        let mut action = None;
        ui.label(
            RichText::new(claims_line(claims))
                .font(mono(10.0))
                .color(TEXT),
        );
        if let Some(note) = &claims.note {
            let mut trust = self.trust_name;
            let toggle = ui
                .checkbox(
                    &mut trust,
                    format!(
                        "Use the name's note ({}) to find a pitched note's fundamental",
                        note.name
                    ),
                )
                .on_hover_text(
                    "The listener takes the partial within a semitone of it: for NOTE, HELD and \
                     SYNTH. The name can be wrong; off, the listener finds the fundamental itself.",
                );
            if toggle.changed() {
                action = Some(Action::TrustName(trust));
            }
        }
        let Some(report) = current.report() else {
            return action;
        };
        for check in name::check(claims, report) {
            ui.horizontal_wrapped(|ui| {
                ui.label(
                    RichText::new(match check.agreement {
                        Agreement::Agrees => "AGREES",
                        Agreement::Differs => "DIFFERS",
                        Agreement::Suggests => "SUGGESTS",
                        Agreement::Unmeasured => "NOT MEASURED",
                    })
                    .font(mono(10.0))
                    .strong()
                    .color(match check.agreement {
                        Agreement::Agrees => CYAN,
                        Agreement::Differs | Agreement::Suggests => AMBER,
                        Agreement::Unmeasured => DIM,
                    }),
                );
                ui.label(
                    RichText::new(format!("{} — {}", check.what, check.detail)).font(mono(10.0)),
                );
                let ring = claims
                    .family
                    .as_ref()
                    .and_then(|(_, family)| Ring::of(*family))
                    .filter(|ring| *ring != self.ring);
                if check.what == "family"
                    && check.agreement != Agreement::Agrees
                    && let Some(ring) = ring
                    && ui.button(format!("Read as {}", ring.label())).clicked()
                {
                    action = Some(Action::Ring(ring));
                }
            });
        }
        action
    }

    /// The pitch, the orbit, the inspector and the timeline.
    fn instrument(&mut self, ui: &mut Ui, main: Rect, time: f64) {
        let strip = Rect::from_min_max(
            pos2(main.left() + 12.0, main.bottom() - 212.0),
            main.right_bottom() - vec2(12.0, 12.0),
        );
        let middle = Rect::from_min_max(
            main.min + vec2(12.0, 8.0),
            pos2(main.right() - 12.0, strip.top() - 8.0),
        );
        // The zoom floats over the scope as a callout: the scope keeps the whole middle.
        let stage = middle;
        // The scope as large as the height allows once the orbit's top and bottom glyphs and their
        // labels fit; the orbit then widens into the room the sides have.
        let radius = (stage.height() / 2.0 - 100.0)
            .min(stage.width() / 2.0 - 190.0)
            .max(60.0);
        let orbit = vec2(
            (stage.width() / 2.0 - 120.0).clamp(radius + 60.0, radius * 1.75),
            radius + 60.0,
        );
        let geometry = Geometry {
            centre: stage.center(),
            radius,
        };
        let hover = ui.input(|i| i.pointer.hover_pos());
        let busy = self.current.as_ref().is_some_and(Current::busy);
        let status = match &self.current {
            None => "AWAITING SIGNAL".to_string(),
            Some(c) if c.refused.is_some() => "REFUSED".into(),
            Some(c) if c.failed.is_some() => "FAILED".into(),
            Some(c) => format!("LISTENING {:.2} s", c.dropped.elapsed().as_secs_f64()),
        };
        let painter = ui.painter().clone();
        let current = self.current.as_ref();
        let scene = current.and_then(|c| c.scene.as_ref());
        let curves = current.and_then(|c| c.curves.as_deref());
        let playhead = self.playhead().map(|(_, ms)| ms);
        let span = curves
            .and_then(|c| c.spectrogram.as_ref())
            .and_then(|s| s.times_ms.last().copied());
        let frame = scope::Frame {
            scene,
            curves,
            busy,
            status: &status,
            view: if self.mode == Mode::Radar {
                View::Radar
            } else {
                View::Ring
            },
            time,
            hover,
            playhead: playhead
                .zip(span)
                .filter(|(_, span)| *span > 0.0)
                .map(|(ms, span)| (ms / span).clamp(0.0, 1.0) as f32),
        };
        let under = scope::draw(&painter, geometry, &frame);
        if let Some(text) = under {
            painter.text(
                pos2(stage.left() + 4.0, stage.bottom() - 4.0),
                Align2::LEFT_BOTTOM,
                format!("⌖ {text}"),
                mono(11.0),
                TEXT,
            );
        }
        glyphs::legend(&painter, pos2(stage.left() + 4.0, stage.top() + 8.0));

        if let Some(scene) = scene.cloned() {
            let places = glyphs::orbit(
                ui,
                geometry.centre,
                radius,
                orbit,
                &scene,
                &mut self.focus,
                time,
                playhead,
            );
            if ui.input(|i| i.key_pressed(egui::Key::Escape)) {
                self.focus = None;
            }
            let focused = self.focus.as_ref().and_then(|f| {
                let at = places.iter().find(|(p, _)| *p == f.part)?.1;
                let role = scene.parts.iter().find(|p| p.part == f.part)?.role;
                Some((at, role))
            });
            if let (Some((at, role)), Some(report)) =
                (focused, self.current.as_ref().and_then(Current::report))
            {
                // A callout: a panel beside the glyph, over the scope, a leader line to it. It is as
                // tall as its figures, up to most of the middle; its height is last frame's.
                let id = ui.id().with("callout");
                let width = 420.0_f32.min(middle.width() * 0.46);
                let max_height = (middle.height() - 24.0).min(640.0);
                let height = ui
                    .ctx()
                    .memory(|m| m.area_rect(id))
                    .map_or(max_height, |r| r.height())
                    .min(max_height);
                let (panel, edge) =
                    glyphs::callout_place(at, geometry.centre, vec2(width, height), middle);
                glyphs::leader(&painter, at, edge, crate::theme::role_colour(role), time);
                let report = report.clone();
                let focus = &mut self.focus;
                let know = &self.know;
                egui::Area::new(id)
                    .order(egui::Order::Foreground)
                    .fixed_pos(panel.min)
                    .movable(false)
                    .show(ui.ctx(), |ui| {
                        glyphs::inspector(
                            ui, width, max_height, &report, &scene, focus, know, curves, playhead,
                        );
                    });
            }
        }
        let curves = self.current.as_ref().and_then(|c| c.curves.clone());
        let scene = self.current.as_ref().and_then(|c| c.scene.clone());
        let wave = self.wave_for(strip, curves.as_deref());
        timeline::draw(
            &painter,
            strip,
            scene.as_ref(),
            curves.as_deref(),
            wave.as_deref(),
            hover,
            playhead,
        );
        let _ = ui.interact(strip, ui.id().with("timeline"), Sense::hover());
    }
}

/// A change the left column asks for, applied after the frame's drawing.
enum Action {
    Ring(Ring),
    TrustName(bool),
    Play(Which),
    Stop,
}

fn parts_line(ui: &mut Ui, report: &Report) {
    let readings: usize = report.sections.iter().map(|s| s.readings.len()).sum();
    ui.label(
        RichText::new(format!(
            "{readings} readings in {} parts",
            report.sections.len()
        ))
        .font(mono(10.0))
        .color(DIM),
    );
}

/// The name's claims in one line.
fn claims_line(claims: &Claims) -> String {
    let mut parts = Vec::new();
    if let Some(note) = &claims.note {
        parts.push(format!("{} ({:.1} Hz)", note.name, note.hz));
    }
    if let Some((first, last)) = &claims.run {
        parts.push(format!("a run, {}–{}", first.name, last.name));
    }
    if let Some((word, family)) = &claims.family {
        parts.push(format!("\"{word}\", a {}", family.name()));
    }
    if let Some(bpm) = claims.tempo_bpm {
        parts.push(format!("{bpm:.0} BPM"));
    }
    if let Some(dynamic) = &claims.dynamic {
        parts.push(dynamic.clone());
    }
    if claims.looped {
        parts.push("a loop, which the listener reads as one sound".into());
    }
    format!("The name says: {}", parts.join(" · "))
}

fn drop_area(ui: &mut Ui, hovering: bool, time: f64) {
    let (rect, _) = ui.allocate_exact_size(vec2(ui.available_width(), 104.0), Sense::hover());
    let painter = ui.painter();
    let t = time as f32;
    let colour = if hovering { CYAN } else { fade(CYAN, 0.55) };
    painter.rect_filled(rect, 2.0, PANEL);
    if hovering {
        radial(
            painter,
            rect.center(),
            rect.width() * 0.5,
            CYAN,
            0.35 + 0.15 * (t * 6.0).sin(),
        );
    }
    brackets(painter, rect, 14.0, Stroke::new(1.5, colour));
    crate::theme::dashed(
        painter,
        rect.left_center() + vec2(18.0, 20.0),
        rect.right_center() + vec2(-18.0, 20.0),
        4.0,
        4.0,
        Stroke::new(1.0, fade(CYAN, 0.3)),
    );
    glow_text(
        painter,
        rect.center() - vec2(0.0, 8.0),
        Align2::CENTER_CENTER,
        if hovering {
            "RELEASE TO LISTEN"
        } else {
            "DROP A SOUND"
        },
        mono(15.0),
        colour,
    );
    painter.text(
        rect.center() + vec2(0.0, 30.0),
        Align2::CENTER_CENTER,
        "WAV AIFF FLAC ALAC MP3 AAC VORBIS",
        mono(9.0),
        DIM,
    );
}

/// The listener's real steps, as a code block; a cursor blinks while it works.
fn log(ui: &mut Ui, current: &Current, time: f64) {
    egui::Frame::new()
        .fill(Color32::from_rgb(5, 10, 15))
        .stroke(Stroke::new(1.0, GRID))
        .inner_margin(8.0)
        .show(ui, |ui| {
            ui.set_width(ui.available_width());
            ui.label(RichText::new("// LISTENER LOG").font(mono(9.0)).color(DIM));
            for (at, line) in &current.log {
                ui.label(
                    RichText::new(format!("{at:>6.3} {line}"))
                        .font(mono(10.0))
                        .color(fade(CYAN, 0.85)),
                );
            }
            if current.busy() {
                let blink = if (time * 2.0).fract() < 0.5 {
                    "▮"
                } else {
                    " "
                };
                ui.label(
                    RichText::new(format!(
                        "{:>6.3} {blink}",
                        current.dropped.elapsed().as_secs_f64()
                    ))
                    .font(mono(10.0))
                    .color(CYAN),
                );
            }
        });
}

/// The HUD's style: dark, monospace, the palette's accents.
fn style(ctx: &egui::Context) {
    ctx.all_styles_mut(|s| {
        s.override_font_id = Some(mono(12.0));
        let v = &mut s.visuals;
        v.dark_mode = true;
        v.override_text_color = Some(TEXT);
        v.panel_fill = BG;
        v.window_fill = PANEL;
        v.extreme_bg_color = Color32::from_rgb(5, 10, 15);
        v.faint_bg_color = Color32::from_rgb(10, 18, 26);
        v.selection.bg_fill = fade(CYAN, 0.28);
        v.selection.stroke = Stroke::new(1.0, CYAN);
        v.widgets.noninteractive.bg_stroke = Stroke::new(1.0, GRID);
        v.widgets.noninteractive.fg_stroke = Stroke::new(1.0, TEXT);
        for w in [
            &mut v.widgets.inactive,
            &mut v.widgets.hovered,
            &mut v.widgets.active,
        ] {
            w.corner_radius = 1.into();
        }
        v.widgets.inactive.bg_fill = Color32::from_rgb(10, 20, 30);
        v.widgets.inactive.weak_bg_fill = Color32::from_rgb(10, 20, 30);
        v.widgets.inactive.bg_stroke = Stroke::new(1.0, GRID);
        v.widgets.hovered.bg_fill = Color32::from_rgb(14, 34, 46);
        v.widgets.hovered.weak_bg_fill = Color32::from_rgb(14, 34, 46);
        v.widgets.hovered.bg_stroke = Stroke::new(1.0, fade(CYAN, 0.7));
        v.widgets.active.bg_fill = fade(CYAN, 0.35);
        v.widgets.active.bg_stroke = Stroke::new(1.0, CYAN);
        v.window_stroke = Stroke::new(1.0, GRID);
    });
}
