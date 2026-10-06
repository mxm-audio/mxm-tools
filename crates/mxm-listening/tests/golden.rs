//! The golden cases as data: the committed table parses, and a case matches exactly the readings it
//! names, in their band and on their side.

use mxm_listening::golden::{self, Kind};

#[test]
fn the_committed_cases_parse_and_name_their_sounds() {
    let cases = golden::cases();
    assert!(cases.len() >= 7, "{} cases", cases.len());
    let higher = cases
        .iter()
        .find(|c| c.name == "higher-pitched")
        .expect("the snare's pitch case");
    assert_eq!(higher.kind, Kind::Phrase);
    assert_eq!(higher.pairs.len(), 3);
    assert_eq!(
        higher.pairs[2],
        (
            "frankensnare-14s vl9 rr1".to_string(),
            "finished-14s hard".to_string()
        )
    );
    assert_eq!(higher.expect[0].id, "pitch.rest");
    assert_eq!(higher.expect[0].sign, Some(1.0));
    // No case names a file: sounds are logical names.
    for c in &cases {
        for (a, b) in &c.pairs {
            for name in [a, b] {
                assert!(
                    !name.contains('/')
                        && !name.contains('\\')
                        && !name.contains(".wav")
                        && !name.contains(".flac"),
                    "{}: {name}",
                    c.name
                );
            }
        }
    }
}

#[test]
fn a_reading_matches_by_id_band_and_side() {
    let cases = golden::parse(
        "c\t2026-09-26\ttest\tdeviation\ta>b\ttone.band_level@6400:+ | attack.peak_time:-\tbright top\n",
    )
    .unwrap();
    let e = &cases[0].expect;
    assert_eq!(e.len(), 2);
    assert_eq!(e[0].band_lo, Some(6400.0));
    assert_eq!(e[1].sign, Some(-1.0));
    assert!(golden::parse("c\td\ts\tsometimes\ta>b\tx\tp\n").is_err());
    assert!(golden::parse("c\td\ts\tphrase\tab\tx\tp\n").is_err());
    assert!(golden::parse("c\td\ts\tphrase\ta>b\tx:up\tp\n").is_err());
    assert!(golden::parse("too\tfew\n").is_err());
}
