//! The catalogue: 100 generic archetypes in nine families, each a room, its air, three sources and
//! a far receiver (a near one is defined too, and rendered only on request), with the published class reference its render is checked against
//! (V9). Plan §7 chose 21; the owner asked for 100 of the typical variety (2026-09-15), and the
//! rest follow the survey of shipped catalogues (`research:sources/room-acoustics-simulation/notes/
//! survey-shipped-ir-catalogue.md`): size and material variants of its common classes, and its
//! niche spaces that a closed room can hold.
//!
//! **Scenes are code, not files.** The plan's `rooms/` folder became this module, because a scene
//! format would need a parser and the crate has no dependencies (plan revision 13). Geometry is
//! invented and generic; no archetype copies a real building (owner decision 4). Surfaces are blends
//! of the research page's published rows ([`materials`]); a blend's fractions are the only thing
//! chosen per room, and they are recorded in every sidecar through the material names.
//!
//! **Sources.** `centre` feeds the stereo file; `left` and `right` feed the true-stereo pair, which
//! the `catalogue` binary renders only when asked, and lie on the listener's left and right at both
//! positions (a test holds it). All sources are omni
//! (plan §4.4); the one frequency-dependent source option is a library feature, not a catalogue one.
//!
//! **Class references** come from `research:sources/room-acoustics-simulation/notes/r0-class-ranges.md`.
//! Only classes with a measured range from more than one room gate (plan V9), per octave or as a
//! band mean or maximum; the rest
//! are reported against their estimate or single room. A value passes when it lies inside the range
//! widened by the ISO 3382-1 JND of 5 % (or a figure read-off's stated tolerance, when wider).

pub mod materials;
mod spaces;

use crate::air::Air;
use crate::analysis::Analysis;
use crate::bands::{NOMINAL_CENTRES_HZ, exact_centre_hz};
use crate::error::Error;
use crate::geometry::{Room, Vec3};
use crate::render::Cap;
use crate::wav::Decoded;

/// Committed files: 32-bit float at this rate (plan §4.5).
pub const SAMPLE_RATE: u32 = 48_000;
/// The consumer's soft limit, and the fade the generator applies at it (plan §4.5, §9). The fade
/// length is **chosen**.
pub const CAP: Cap = Cap {
    length_s: 10.0,
    fade_s: 0.5,
};
/// Every released file's own peak, dBFS: the renderer's default normalization (owner, 2026-09-15).
pub const FILE_PEAK_DBFS: f64 = crate::render::DEFAULT_PEAK_DBFS;
/// The catalogue's pair: near-coincident cardioids, ORTF's 17 cm and ±55°.
pub const PAIR_SPACING_M: f64 = 0.17;
pub const PAIR_HALF_ANGLE_DEG: f64 = 55.0;
/// The largest room the wave solver carries below the seam, m³. **Chosen:** the plan's small
/// spaces reach 600 m³ and its halls start at 2,500 m³; a room this size solves in minutes, a
/// larger one's modes are dense enough below any seam worth solving.
pub const WAVE_MAX_VOLUME_M3: f64 = 1_000.0;
/// ISO 3382-1 Table A.1: the just-noticeable difference of reverberation time, relative.
pub const JND_T: f64 = 0.05;
/// `mxm-fx-convolution`'s WAV path (factory branch, `plugins/mxm-fx-convolution/src/response.rs`):
/// one or two channels, 8–384 kHz, and ten seconds before it fades a file itself.
pub const CONSUMER_RATES: (u32, u32) = (8_000, 384_000);
pub const CONSUMER_SECONDS: f64 = 10.0;

/// One archetype of the catalogue.
#[derive(Clone, Copy)]
pub struct Archetype {
    /// File-name stem: lower case, hyphenated.
    pub slug: &'static str,
    pub name: &'static str,
    /// One of [`FAMILIES`].
    pub family: &'static str,
    pub build: fn() -> Result<Space, Error>,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Sources {
    pub centre: Vec3,
    pub left: Vec3,
    pub right: Vec3,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Position {
    pub name: &'static str,
    pub receiver: Vec3,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Space {
    pub room: Room,
    pub air: Air,
    pub sources: Sources,
    /// Near, then far. Both face the centre source.
    pub positions: [Position; 2],
    pub non_diffuse: bool,
    /// The wave solver's seam when it is not the Schroeder default, Hz; the reason is in `notes`.
    pub seam_hz: Option<f64>,
    /// How long rays run, s: past the longest band's decay to the ray floor.
    pub max_time_s: f64,
    pub occupancy: &'static str,
    pub reference: Reference,
    pub notes: Vec<&'static str>,
}

impl Space {
    /// Whether the wave solver carries the space below its seam: every room up to
    /// [`WAVE_MAX_VOLUME_M3`], and no other.
    pub fn wave_solved(&self) -> bool {
        self.room.volume() <= WAVE_MAX_VOLUME_M3
    }
}

/// Which decay a reference was measured as.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Decay {
    T20,
    T30,
}

/// A published range. Octave indices run 0 = 125 Hz to 5 = 4 kHz.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Target {
    Octaves([Option<(f64, f64)>; 6]),
    Mean {
        first: usize,
        last: usize,
        range: (f64, f64),
    },
    Max {
        first: usize,
        last: usize,
        range: (f64, f64),
    },
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Reference {
    /// Whether it gates V9: a measured range from more than one room.
    pub gated: bool,
    pub decay: Decay,
    pub target: Target,
    /// Absolute tolerance, s, when the source is a figure read-off wider than the JND.
    pub slack_s: f64,
    pub basis: &'static str,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Comparison {
    pub label: String,
    pub value: Option<f64>,
    pub range: (f64, f64),
    pub within: bool,
}

impl Reference {
    /// Compares a render's decay times per octave (125 Hz–4 kHz) with the reference.
    pub fn compare(&self, octaves: &[Option<f64>; 6]) -> Vec<Comparison> {
        let check = |label: String, value: Option<f64>, range: (f64, f64)| {
            let (lo, hi) = range;
            let widened = (
                lo - (lo * JND_T).max(self.slack_s),
                hi + (hi * JND_T).max(self.slack_s),
            );
            Comparison {
                label,
                value,
                range,
                within: value.is_some_and(|v| v >= widened.0 && v <= widened.1),
            }
        };
        let span = |first: usize, last: usize| {
            format!(
                "{}–{} Hz",
                NOMINAL_CENTRES_HZ[first + 1],
                NOMINAL_CENTRES_HZ[last + 1]
            )
        };
        match self.target {
            Target::Octaves(ranges) => ranges
                .iter()
                .enumerate()
                .filter_map(|(k, r)| {
                    r.map(|range| {
                        check(
                            format!("{} Hz", NOMINAL_CENTRES_HZ[k + 1]),
                            octaves[k],
                            range,
                        )
                    })
                })
                .collect(),
            Target::Mean { first, last, range } => {
                let values: Option<Vec<f64>> = octaves[first..=last].iter().copied().collect();
                let mean = values.map(|v| v.iter().sum::<f64>() / v.len() as f64);
                vec![check(format!("mean {}", span(first, last)), mean, range)]
            }
            Target::Max { first, last, range } => {
                let values: Option<Vec<f64>> = octaves[first..=last].iter().copied().collect();
                let max = values.map(|v| v.iter().copied().fold(f64::NEG_INFINITY, f64::max));
                vec![check(format!("max {}", span(first, last)), max, range)]
            }
        }
    }
}

/// The families, in the catalogue's order: each is a folder of the release.
pub const FAMILIES: [&str; 9] = [
    "rooms",
    "studios",
    "chambers",
    "vehicles",
    "halls",
    "venues",
    "worship",
    "industrial",
    "transit",
];

const fn archetype(
    slug: &'static str,
    name: &'static str,
    family: &'static str,
    build: fn() -> Result<Space, Error>,
) -> Archetype {
    Archetype {
        slug,
        name,
        family,
        build,
    }
}

/// The catalogue, by family: the 21 archetypes of plan §7 first in each, then the owner's
/// expansion to 100 (2026-09-15).
pub const ARCHETYPES: [Archetype; 100] = [
    archetype(
        "living-room",
        "Furnished living room",
        "rooms",
        spaces::rooms::living_room,
    ),
    archetype(
        "tiled-bathroom",
        "Tiled bathroom",
        "rooms",
        spaces::rooms::tiled_bathroom,
    ),
    archetype(
        "hard-small-room",
        "Hard small room",
        "rooms",
        spaces::rooms::hard_small_room,
    ),
    archetype(
        "kitchen",
        "Domestic kitchen",
        "rooms",
        spaces::rooms::kitchen,
    ),
    archetype(
        "bedroom",
        "Furnished bedroom",
        "rooms",
        spaces::rooms::bedroom,
    ),
    archetype(
        "narrow-hallway",
        "Narrow hallway",
        "rooms",
        spaces::rooms::narrow_hallway,
    ),
    archetype(
        "walk-in-closet",
        "Walk-in closet",
        "rooms",
        spaces::rooms::walk_in_closet,
    ),
    archetype(
        "shower-stall",
        "Tiled shower stall",
        "rooms",
        spaces::rooms::shower_stall,
    ),
    archetype(
        "attic-room",
        "Attic room",
        "rooms",
        spaces::rooms::attic_room,
    ),
    archetype(
        "brick-cellar",
        "Brick-vaulted cellar",
        "rooms",
        spaces::rooms::brick_cellar,
    ),
    archetype("garage", "Domestic garage", "rooms", spaces::rooms::garage),
    archetype(
        "empty-flat",
        "Unfurnished flat",
        "rooms",
        spaces::rooms::empty_flat,
    ),
    archetype("sauna", "Timber sauna", "rooms", spaces::rooms::sauna),
    archetype(
        "meeting-room",
        "Meeting room",
        "rooms",
        spaces::rooms::meeting_room,
    ),
    archetype(
        "open-plan-office",
        "Open-plan office",
        "rooms",
        spaces::rooms::open_plan_office,
    ),
    archetype(
        "school-corridor",
        "School corridor",
        "rooms",
        spaces::rooms::school_corridor,
    ),
    archetype(
        "library-reading-room",
        "Library reading room",
        "rooms",
        spaces::rooms::library_reading_room,
    ),
    archetype(
        "museum-gallery",
        "Top-lit museum gallery",
        "rooms",
        spaces::rooms::museum_gallery,
    ),
    archetype(
        "vocal-booth",
        "Dead vocal booth",
        "studios",
        spaces::studios::vocal_booth,
    ),
    archetype(
        "tracking-room",
        "Wood-lined tracking room",
        "studios",
        spaces::studios::tracking_room,
    ),
    archetype(
        "drum-room",
        "Large bright drum room",
        "studios",
        spaces::studios::drum_room,
    ),
    archetype(
        "control-room",
        "Treated control room",
        "studios",
        spaces::studios::control_room,
    ),
    archetype(
        "amp-booth",
        "Amp isolation booth",
        "studios",
        spaces::studios::amp_booth,
    ),
    archetype(
        "piano-room",
        "Piano room",
        "studios",
        spaces::studios::piano_room,
    ),
    archetype(
        "string-room",
        "Wood-lined string room",
        "studios",
        spaces::studios::string_room,
    ),
    archetype(
        "stone-live-room",
        "Stone live room",
        "studios",
        spaces::studios::stone_live_room,
    ),
    archetype(
        "dead-studio",
        "Dead studio",
        "studios",
        spaces::studios::dead_studio,
    ),
    archetype(
        "large-live-room",
        "Large live room",
        "studios",
        spaces::studios::large_live_room,
    ),
    archetype(
        "tight-drum-room",
        "Tight drum room",
        "studios",
        spaces::studios::tight_drum_room,
    ),
    archetype(
        "rehearsal-room",
        "Rehearsal room",
        "studios",
        spaces::studios::rehearsal_room,
    ),
    archetype(
        "broadcast-studio",
        "Broadcast studio",
        "studios",
        spaces::studios::broadcast_studio,
    ),
    archetype(
        "echo-chamber",
        "Concrete echo chamber",
        "chambers",
        spaces::chambers::echo_chamber,
    ),
    archetype(
        "small-echo-chamber",
        "Small echo chamber",
        "chambers",
        spaces::chambers::small_echo_chamber,
    ),
    archetype(
        "large-echo-chamber",
        "Large echo chamber",
        "chambers",
        spaces::chambers::large_echo_chamber,
    ),
    archetype(
        "tiled-chamber",
        "Bright tiled chamber",
        "chambers",
        spaces::chambers::tiled_chamber,
    ),
    archetype(
        "water-tank",
        "Concrete water tank",
        "chambers",
        spaces::chambers::water_tank,
    ),
    archetype(
        "cistern",
        "Vaulted cistern",
        "chambers",
        spaces::chambers::cistern,
    ),
    archetype(
        "grain-silo",
        "Grain silo",
        "chambers",
        spaces::chambers::grain_silo,
    ),
    archetype(
        "rock-cave",
        "Rock cave",
        "chambers",
        spaces::chambers::rock_cave,
    ),
    archetype(
        "catacomb",
        "Catacomb galleries",
        "chambers",
        spaces::chambers::catacomb,
    ),
    archetype(
        "car-cabin",
        "Car cabin",
        "vehicles",
        spaces::vehicles::car_cabin,
    ),
    archetype(
        "cargo-van",
        "Cargo van",
        "vehicles",
        spaces::vehicles::cargo_van,
    ),
    archetype(
        "city-bus",
        "City bus",
        "vehicles",
        spaces::vehicles::city_bus,
    ),
    archetype(
        "train-carriage",
        "Train carriage",
        "vehicles",
        spaces::vehicles::train_carriage,
    ),
    archetype(
        "lift-car",
        "Lift car",
        "vehicles",
        spaces::vehicles::lift_car,
    ),
    archetype(
        "recital-hall",
        "Recital hall",
        "halls",
        spaces::halls::recital_hall,
    ),
    archetype(
        "shoebox-hall",
        "Shoebox concert hall",
        "halls",
        spaces::halls::shoebox_hall,
    ),
    archetype(
        "scoring-stage",
        "Orchestral scoring stage",
        "halls",
        spaces::halls::scoring_stage,
    ),
    archetype(
        "horseshoe-opera",
        "Horseshoe theatre",
        "halls",
        spaces::halls::horseshoe_opera,
    ),
    archetype(
        "chamber-music-hall",
        "Chamber music hall",
        "halls",
        spaces::halls::chamber_music_hall,
    ),
    archetype(
        "small-concert-hall",
        "Small concert hall",
        "halls",
        spaces::halls::small_concert_hall,
    ),
    archetype(
        "large-shoebox-hall",
        "Large shoebox hall",
        "halls",
        spaces::halls::large_shoebox_hall,
    ),
    archetype(
        "vineyard-hall",
        "Vineyard concert hall",
        "halls",
        spaces::halls::vineyard_hall,
    ),
    archetype(
        "fan-auditorium",
        "Fan-shaped auditorium",
        "halls",
        spaces::halls::fan_auditorium,
    ),
    archetype("cinema", "Cinema", "halls", spaces::halls::cinema),
    archetype(
        "drama-theatre",
        "Drama theatre",
        "halls",
        spaces::halls::drama_theatre,
    ),
    archetype(
        "lecture-theatre",
        "Raked lecture theatre",
        "halls",
        spaces::halls::lecture_theatre,
    ),
    archetype(
        "church-scoring-stage",
        "Church scoring stage",
        "halls",
        spaces::halls::church_scoring_stage,
    ),
    archetype(
        "small-scoring-stage",
        "Small scoring stage",
        "halls",
        spaces::halls::small_scoring_stage,
    ),
    archetype(
        "club-venue",
        "Club venue",
        "venues",
        spaces::venues::club_venue,
    ),
    archetype(
        "indoor-arena",
        "Indoor arena",
        "venues",
        spaces::venues::indoor_arena,
    ),
    archetype(
        "jazz-club",
        "Jazz club",
        "venues",
        spaces::venues::jazz_club,
    ),
    archetype(
        "brick-pub",
        "Brick pub",
        "venues",
        spaces::venues::brick_pub,
    ),
    archetype("ballroom", "Ballroom", "venues", spaces::venues::ballroom),
    archetype(
        "large-music-venue",
        "Large music venue",
        "venues",
        spaces::venues::large_music_venue,
    ),
    archetype(
        "sports-hall",
        "Sports hall",
        "venues",
        spaces::venues::sports_hall,
    ),
    archetype(
        "velodrome",
        "Velodrome",
        "venues",
        spaces::venues::velodrome,
    ),
    archetype(
        "village-hall",
        "Village hall",
        "venues",
        spaces::venues::village_hall,
    ),
    archetype(
        "basement-club",
        "Basement club",
        "venues",
        spaces::venues::basement_club,
    ),
    archetype(
        "swimming-hall",
        "Swimming hall",
        "venues",
        spaces::venues::swimming_hall,
    ),
    archetype("ice-rink", "Ice rink", "venues", spaces::venues::ice_rink),
    archetype(
        "small-chapel",
        "Small chapel",
        "worship",
        spaces::worship::small_chapel,
    ),
    archetype(
        "stone-church",
        "Stone parish church",
        "worship",
        spaces::worship::stone_church,
    ),
    archetype(
        "gothic-cathedral",
        "Gothic cathedral",
        "worship",
        spaces::worship::gothic_cathedral,
    ),
    archetype(
        "timber-church",
        "Timber church",
        "worship",
        spaces::worship::timber_church,
    ),
    archetype(
        "baroque-church",
        "Baroque church",
        "worship",
        spaces::worship::baroque_church,
    ),
    archetype(
        "romanesque-church",
        "Romanesque church",
        "worship",
        spaces::worship::romanesque_church,
    ),
    archetype("basilica", "Basilica", "worship", spaces::worship::basilica),
    archetype(
        "domed-church",
        "Domed church",
        "worship",
        spaces::worship::domed_church,
    ),
    archetype(
        "concrete-church",
        "Concrete church",
        "worship",
        spaces::worship::concrete_church,
    ),
    archetype("crypt", "Crypt", "worship", spaces::worship::crypt),
    archetype(
        "domed-mausoleum",
        "Domed mausoleum",
        "worship",
        spaces::worship::domed_mausoleum,
    ),
    archetype(
        "domed-prayer-hall",
        "Carpeted domed prayer hall",
        "worship",
        spaces::worship::domed_prayer_hall,
    ),
    archetype(
        "vaulted-refectory",
        "Vaulted refectory",
        "worship",
        spaces::worship::vaulted_refectory,
    ),
    archetype(
        "concrete-stairwell",
        "Concrete stairwell",
        "industrial",
        spaces::industrial::concrete_stairwell,
    ),
    archetype(
        "empty-warehouse",
        "Empty warehouse",
        "industrial",
        spaces::industrial::empty_warehouse,
    ),
    archetype(
        "sawtooth-factory",
        "Sawtooth factory hall",
        "industrial",
        spaces::industrial::sawtooth_factory,
    ),
    archetype(
        "aircraft-hangar",
        "Aircraft hangar",
        "industrial",
        spaces::industrial::aircraft_hangar,
    ),
    archetype(
        "plant-room",
        "Plant room",
        "industrial",
        spaces::industrial::plant_room,
    ),
    archetype(
        "mill-loft",
        "Brick mill loft",
        "industrial",
        spaces::industrial::mill_loft,
    ),
    archetype(
        "shipping-container",
        "Shipping container",
        "industrial",
        spaces::industrial::shipping_container,
    ),
    archetype(
        "car-park",
        "Underground car park",
        "transit",
        spaces::transit::car_park,
    ),
    archetype(
        "rail-tunnel",
        "Arched rail tunnel",
        "transit",
        spaces::transit::rail_tunnel,
    ),
    archetype(
        "station-concourse",
        "Warehouse or station concourse",
        "transit",
        spaces::transit::station_concourse,
    ),
    archetype(
        "pedestrian-underpass",
        "Pedestrian underpass",
        "transit",
        spaces::transit::pedestrian_underpass,
    ),
    archetype(
        "metro-platform",
        "Metro platform",
        "transit",
        spaces::transit::metro_platform,
    ),
    archetype(
        "road-tunnel",
        "Road tunnel",
        "transit",
        spaces::transit::road_tunnel,
    ),
    archetype(
        "airport-terminal",
        "Airport terminal",
        "transit",
        spaces::transit::airport_terminal,
    ),
    archetype(
        "glass-atrium",
        "Glass atrium",
        "transit",
        spaces::transit::glass_atrium,
    ),
    archetype(
        "parking-deck",
        "Open parking deck",
        "transit",
        spaces::transit::parking_deck,
    ),
];

/// The ISO 3382 parameters of one octave, as a render summary keeps them.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct BandSummary {
    pub edt_s: Option<f64>,
    pub t20_s: Option<f64>,
    pub t30_s: Option<f64>,
    pub c80_db: Option<f64>,
    pub d50: Option<f64>,
}

/// A full-response analysis reduced to what V9 and the manifest read, per band 63 Hz–16 kHz.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct Summary {
    pub bands: [BandSummary; 9],
}

impl Summary {
    pub fn from_analysis(a: &Analysis) -> Self {
        Self {
            bands: std::array::from_fn(|k| BandSummary {
                edt_s: a.bands[k].edt_s,
                t20_s: a.bands[k].t20_s,
                t30_s: a.bands[k].t30_s,
                c80_db: a.bands[k].c80_db,
                d50: a.bands[k].d50,
            }),
        }
    }

    /// Decay times 125 Hz–4 kHz.
    pub fn octaves(&self, decay: Decay) -> [Option<f64>; 6] {
        std::array::from_fn(|k| {
            let b = &self.bands[k + 1];
            match decay {
                Decay::T20 => b.t20_s,
                Decay::T30 => b.t30_s,
            }
        })
    }

    /// Tab-separated, one band per line, an empty field for a missing value.
    pub fn to_tsv(&self) -> String {
        let field = |v: Option<f64>| v.map_or(String::new(), |v| format!("{v}"));
        let mut s = String::from("band_hz\tedt_s\tt20_s\tt30_s\tc80_db\td50\n");
        for (k, b) in self.bands.iter().enumerate() {
            s.push_str(&format!(
                "{}\t{}\t{}\t{}\t{}\t{}\n",
                NOMINAL_CENTRES_HZ[k],
                field(b.edt_s),
                field(b.t20_s),
                field(b.t30_s),
                field(b.c80_db),
                field(b.d50)
            ));
        }
        s
    }

    pub fn from_tsv(text: &str) -> Option<Self> {
        let mut out = Self::default();
        let rows: Vec<&str> = text.lines().skip(1).filter(|l| !l.is_empty()).collect();
        if rows.len() != 9 {
            return None;
        }
        for (k, row) in rows.iter().enumerate() {
            let fields: Vec<&str> = row.split('\t').collect();
            if fields.len() != 6 {
                return None;
            }
            let value = |i: usize| -> Option<Option<f64>> {
                if fields[i].is_empty() {
                    Some(None)
                } else {
                    fields[i].parse().ok().map(Some)
                }
            };
            out.bands[k] = BandSummary {
                edt_s: value(1)?,
                t20_s: value(2)?,
                t30_s: value(3)?,
                c80_db: value(4)?,
                d50: value(5)?,
            };
        }
        Some(out)
    }
}

/// Eyring's reverberation time per octave, 125 Hz–4 kHz, with air absorption: a quick estimate for
/// choosing a blend before a render, never a result.
pub fn eyring_octaves(room: &Room, air: &Air) -> [f64; 6] {
    let (volume, surface) = (room.volume(), room.surface_area());
    let c = air.speed_of_sound();
    std::array::from_fn(|k| {
        let band = k + 1;
        let absorption: f64 = room
            .polygons
            .iter()
            .map(|p| p.area() * room.materials[p.material].absorption[band])
            .sum();
        let mean = (absorption / surface).clamp(1e-6, 0.999);
        let m =
            air.attenuation_db_per_m(exact_centre_hz(band)) / (10.0 * std::f64::consts::LOG10_E);
        24.0 * std::f64::consts::LN_10 * volume
            / (c * (-surface * (1.0 - mean).ln() + 4.0 * m * volume))
    })
}

/// The gain that brings a file whose loudest sample is `peak` to [`FILE_PEAK_DBFS`]. Renders arrive
/// normalized, so at release this is 1 up to rounding; it re-normalizes a file rendered otherwise.
pub fn normalizing_gain(peak: f64) -> Option<f64> {
    (peak.is_finite() && peak > 0.0).then(|| 10f64.powf(FILE_PEAK_DBFS / 20.0) / peak)
}

/// The top-level number `key` of a sidecar as [`crate::sidecar::JsonObject::pretty`] writes it.
pub fn top_level_number(json: &str, key: &str) -> Option<f64> {
    let needle = format!("\n  \"{key}\": ");
    if json.matches(&needle).count() != 1 {
        return None;
    }
    let start = json.find(&needle)? + needle.len();
    let end = start + json[start..].find([',', '\n'])?;
    json[start..end].parse().ok()
}

/// `json`, a sidecar as [`crate::sidecar::JsonObject::pretty`] writes it, with the top-level
/// number `key` replaced. `None` unless the key occurs exactly once.
pub fn patch_top_level_number(json: &str, key: &str, value: f64) -> Option<String> {
    let needle = format!("\n  \"{key}\": ");
    if json.matches(&needle).count() != 1 || !value.is_finite() {
        return None;
    }
    let start = json.find(&needle)? + needle.len();
    let end = start + json[start..].find([',', '\n'])?;
    Some(format!("{}{value}{}", &json[..start], &json[end..]))
}

/// Whether a decoded file loads through the consumer's WAV path unchanged: one or two channels, a
/// rate it accepts, at least one frame, finite samples, and no more than its ten seconds, so it
/// never fades a committed file a second time (plan V12).
pub fn consumer_check(decoded: &Decoded) -> Result<(), String> {
    let channels = decoded.channels.len();
    if !(1..=2).contains(&channels) {
        return Err(format!(
            "{channels} channels; the consumer takes one or two"
        ));
    }
    let rate = decoded.sample_rate;
    if !(CONSUMER_RATES.0..=CONSUMER_RATES.1).contains(&rate) {
        return Err(format!("{rate} Hz is outside the consumer's rates"));
    }
    let frames = decoded.channels[0].len();
    if frames == 0 || decoded.channels.iter().any(|c| c.len() != frames) {
        return Err("no frames, or channels of unequal length".into());
    }
    if decoded.channels.iter().flatten().any(|v| !v.is_finite()) {
        return Err("a non-finite sample".into());
    }
    let limit = (f64::from(rate) * CONSUMER_SECONDS).round() as usize;
    if frames > limit {
        return Err(format!(
            "{frames} frames exceed the consumer's {CONSUMER_SECONDS} s"
        ));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::directivity::{Array, Frame};
    use crate::rays::RayOptions;
    use crate::render::RenderOptions;
    use crate::scene::Scene;
    use crate::simulation::{SimulationOptions, render_set, simulate};
    use crate::{ImageSourceOptions, Material};

    #[test]
    fn every_archetype_builds_with_its_sources_on_the_listeners_side() {
        let mut slugs: Vec<&str> = ARCHETYPES.iter().map(|a| a.slug).collect();
        slugs.sort_unstable();
        slugs.dedup();
        assert_eq!(slugs.len(), 100);
        assert!(ARCHETYPES.iter().all(|a| FAMILIES.contains(&a.family)));
        let gated = ARCHETYPES
            .iter()
            .filter(|a| (a.build)().unwrap().reference.gated)
            .count();
        assert_eq!(
            gated, 8,
            "the classes with a range measured in more than one room"
        );
        for a in &ARCHETYPES {
            let space = (a.build)().unwrap_or_else(|e| panic!("{}: {e}", a.slug));
            let s = space.sources;
            for p in &space.positions {
                for (label, source) in [("centre", s.centre), ("left", s.left), ("right", s.right)]
                {
                    Scene::new("check", space.room.clone(), space.air, source, p.receiver)
                        .unwrap_or_else(|e| panic!("{} {} {label}: {e}", a.slug, p.name));
                }
                let frame = Frame::facing(p.receiver, s.centre);
                assert!(
                    (s.left - s.centre).dot(frame.left) > 0.0
                        && (s.right - s.centre).dot(frame.left) < 0.0,
                    "{} {}: sources are not on the listener's sides",
                    a.slug,
                    p.name
                );
                assert!(
                    space.room.clearance(p.receiver) >= 0.25,
                    "{} {}: receiver too close to a surface",
                    a.slug,
                    p.name
                );
            }
            let t = eyring_octaves(&space.room, &space.air);
            assert!(t.iter().all(|v| v.is_finite() && *v > 0.0), "{}", a.slug);
            assert!(space.max_time_s > 0.0);
            assert!(
                space.room.contains(s.centre) && space.room.clearance(s.centre) >= 0.1,
                "{}: source too close to a surface",
                a.slug
            );
        }
    }

    #[test]
    fn references_compare_within_the_jnd() {
        let octaves = [Some(1.0), Some(1.2), Some(0.8), None, Some(0.5), Some(0.4)];
        let r = Reference {
            gated: true,
            decay: Decay::T30,
            target: Target::Octaves([Some((0.9, 0.96)), Some((1.3, 1.4)), None, None, None, None]),
            slack_s: 0.0,
            basis: "test",
        };
        let c = r.compare(&octaves);
        // 1.0 lies within 0.96·1.05; 1.2 lies below 1.3·0.95.
        assert_eq!(c.len(), 2);
        assert!(c[0].within && !c[1].within);
        let slack = Reference { slack_s: 0.1, ..r };
        assert!(slack.compare(&octaves)[1].within);
        let mean = Reference {
            target: Target::Mean {
                first: 0,
                last: 2,
                range: (1.0, 1.0),
            },
            ..r
        };
        assert!(mean.compare(&octaves)[0].within);
        let max = Reference {
            target: Target::Max {
                first: 0,
                last: 2,
                range: (1.0, 1.1),
            },
            ..r
        };
        assert!(!max.compare(&octaves)[0].within);
        let missing = Reference {
            target: Target::Mean {
                first: 2,
                last: 3,
                range: (0.0, 9.0),
            },
            ..r
        };
        assert!(!missing.compare(&octaves)[0].within);
    }

    #[test]
    fn release_helpers_scale_patch_and_check() {
        // A file peaking at 0.5 reaches −1 dBFS with a gain of 1.78.
        let g = normalizing_gain(0.5).unwrap();
        assert!(
            (20.0 * (0.5 * g).log10() - FILE_PEAK_DBFS).abs() < 1e-12,
            "{g}"
        );
        assert_eq!(normalizing_gain(0.0), None);
        assert_eq!(normalizing_gain(f64::NAN), None);

        let json = "{\n  \"reference_distance_m\": 1,\n  \"peak_abs\": 0.25,\n  \"x\": [1, 2]\n}\n";
        let patched = patch_top_level_number(json, "peak_abs", 0.125).unwrap();
        assert_eq!(
            patched,
            "{\n  \"reference_distance_m\": 1,\n  \"peak_abs\": 0.125,\n  \"x\": [1, 2]\n}\n"
        );
        assert!(patch_top_level_number(json, "missing", 1.0).is_none());
        assert_eq!(top_level_number(json, "peak_abs"), Some(0.25));
        assert_eq!(top_level_number(json, "reference_distance_m"), Some(1.0));
        assert_eq!(top_level_number(json, "missing"), None);

        let ok = Decoded {
            channels: vec![vec![0.0, 0.5]; 2],
            sample_rate: 48_000,
        };
        assert!(consumer_check(&ok).is_ok());
        let long = Decoded {
            channels: vec![vec![0.0; 480_001]],
            sample_rate: 48_000,
        };
        assert!(consumer_check(&long).is_err());
        let nan = Decoded {
            channels: vec![vec![f32::NAN]],
            sample_rate: 48_000,
        };
        assert!(consumer_check(&nan).is_err());

        let mut summary = Summary::default();
        summary.bands[4].t30_s = Some(1.25);
        summary.bands[2].c80_db = Some(-3.5);
        assert_eq!(Summary::from_tsv(&summary.to_tsv()), Some(summary));
    }

    #[test]
    fn rendering_scales_linearly_with_the_reference_distance() {
        let wall = Material::from_125_to_4k("wall", [0.1; 6], [0.3; 6]).unwrap();
        let room = Room::shoebox(
            Vec3::new(4.0, 3.0, 2.5),
            std::array::from_fn(|_| wall.clone()),
        )
        .unwrap();
        let scene = Scene::new(
            "linear",
            room,
            Air::standard(),
            Vec3::new(1.0, 1.2, 1.3),
            Vec3::new(3.1, 1.9, 1.2),
        )
        .unwrap();
        let options = SimulationOptions {
            image_sources: ImageSourceOptions {
                max_order: 2,
                ..Default::default()
            },
            rays: RayOptions {
                rays: 2_000,
                max_time_s: 0.6,
                ..Default::default()
            },
            ..Default::default()
        };
        let sim = simulate(&scene, &options).unwrap();
        let pair = Array::near_coincident_cardioids(PAIR_SPACING_M, PAIR_HALF_ANGLE_DEG)
            .placed(&Frame::facing(scene.receiver, scene.source))
            .unwrap();
        let at = |reference_distance_m: f64| {
            let o = RenderOptions {
                include_direct: false,
                reference_distance_m,
                normalize_peak_dbfs: None,
                ..Default::default()
            };
            render_set(&[&sim], &pair, &o).unwrap().remove(0)
        };
        let (one, quarter) = (at(1.0), at(0.25));
        assert_eq!(one.channels[0].len(), quarter.channels[0].len());
        let peak = one
            .channels
            .iter()
            .flatten()
            .fold(0f32, |m, v| m.max(v.abs()));
        for (a, b) in one
            .channels
            .iter()
            .flatten()
            .zip(quarter.channels.iter().flatten())
        {
            assert!((a * 0.25 - b).abs() <= 1e-6 * peak, "{a} {b}");
        }
    }
}
