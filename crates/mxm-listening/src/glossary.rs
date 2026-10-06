//! What each reading, table and curve means, in plain words (`data/glossary.tsv`). A reading's label names
//! it; the glossary says what it tells you about the sound. It is interpretation, so it lives here: a
//! window shows it on hover, and `explain` may use it.
//!
//! Every reading, table or curve id in this crate's source has a row, and every row an id the source holds;
//! `tests/glossary.rs` checks both against a scan of `src/`, since a synthetic report would miss the
//! ids only some sounds emit.

const GLOSSARY: &str = include_str!("../data/glossary.tsv");

/// Every row: a reading or table id and what it means.
pub fn entries() -> impl Iterator<Item = (&'static str, &'static str)> {
    GLOSSARY
        .lines()
        .filter(|line| !line.starts_with('#') && !line.trim().is_empty())
        .filter_map(|line| line.split_once('\t'))
}

/// What the reading or table `id` means, in plain words; `None` for an id the glossary lacks.
#[must_use]
pub fn meaning(id: &str) -> Option<&'static str> {
    entries().find(|(k, _)| *k == id).map(|(_, v)| v)
}
