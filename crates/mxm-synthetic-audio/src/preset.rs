//! String Studio VS-3's factory presets, read from its own pack files and put into a saved state.
//!
//! A pack is a list of records, `{name="…",version="…",schema="…",values="…",metadata="…"}`, in
//! Lua's syntax; a record without a schema uses the last one before it. The plugin's saved state
//! (the component's half of `mxm-vst3-host`'s blob) holds the same three strings in its
//! `parameters` table, so a preset is loaded by writing its strings there, with its name, and
//! loading the state. Nothing here knows what a value means: the plugin reads them.

use crate::vst::Result;

/// One preset, its strings as the pack writes them (Lua string contents, escapes kept).
#[derive(Clone, Debug)]
pub struct Preset {
    pub name: String,
    pub version: String,
    pub schema: String,
    pub values: String,
}

impl Preset {
    /// The schema's names, flattened, each with its group: a value's index is its name's.
    fn names(&self) -> Vec<(String, String)> {
        let schema = self.schema.replace("\\\"", "\"");
        let mut names = Vec::new();
        let mut group = String::new();
        let mut rest = schema.as_str();
        while let Some(open) = rest.find('"') {
            // A group is a table of one name, `{"Group"}`; a pack spaces it, `{  "Group",  }`, and
            // breaks lines with `\`.
            let in_group = rest[..open]
                .trim_end_matches(|c: char| c.is_whitespace() || c == '\\')
                .ends_with('{');
            let Some(close) = rest[open + 1..].find('"') else {
                break;
            };
            let name = &rest[open + 1..open + 1 + close];
            if in_group {
                group = name.to_owned();
            } else {
                names.push((group.clone(), name.to_owned()));
            }
            rest = &rest[open + 1 + close + 1..];
        }
        names
    }

    /// Sets the value named `group` / `name` (a plain value, as the pack writes it).
    pub fn set(&mut self, group: &str, name: &str, value: f64) -> Result<()> {
        let index = self
            .names()
            .iter()
            .position(|(g, n)| g == group && n == name)
            .ok_or_else(|| format!("no {group}/{name} in the schema"))?;
        let mut values = table(&self.values)?;
        let slot = values
            .get_mut(index)
            .ok_or_else(|| format!("{group}/{name} has no value"))?;
        *slot = format!("{value}");
        self.values = format!("{{{}}}", values.join(","));
        Ok(())
    }

    /// The value named `group` / `name`.
    pub fn get(&self, group: &str, name: &str) -> Option<f64> {
        let index = self
            .names()
            .iter()
            .position(|(g, n)| g == group && n == name)?;
        table(&self.values).ok()?.get(index)?.parse().ok()
    }
}

/// The entries of a Lua table of numbers, as a pack or a state writes one: spaced, perhaps broken
/// into lines with `\`, perhaps with a trailing comma.
fn table(values: &str) -> Result<Vec<String>> {
    let blank = |c: char| c.is_whitespace() || c == '\\';
    let inner = values
        .trim_matches(blank)
        .strip_prefix('{')
        .and_then(|v| v.trim_end_matches(blank).strip_suffix('}'))
        .ok_or("values are not a table")?;
    Ok(inner
        .split(',')
        .map(|v| v.trim_matches(blank).to_owned())
        .filter(|v| !v.is_empty())
        .collect())
}

/// The contents of the Lua string literal starting at `text[start]` (just after its opening
/// quote), and the index just past its closing quote.
fn literal(text: &str, start: usize) -> Result<(&str, usize)> {
    let bytes = text.as_bytes();
    let mut i = start;
    while i < bytes.len() {
        match bytes[i] {
            b'\\' => i += 2,
            b'"' => return Ok((&text[start..i], i + 1)),
            _ => i += 1,
        }
    }
    Err("an unterminated string".into())
}

/// The string field `key` of the record `record`, if it has one.
fn field<'a>(record: &'a str, key: &str) -> Result<Option<&'a str>> {
    let marker = format!(",{key}=\"");
    let found = record.find(&marker).or_else(|| {
        record
            .strip_prefix('{')
            .and_then(|_| record.find(&format!("{{{key}=\"")))
    });
    match found {
        Some(at) => Ok(Some(literal(record, at + marker.len())?.0)),
        None => Ok(None),
    }
}

/// The preset `name` in the pack `pack` (its text).
pub fn find(pack: &str, name: &str) -> Result<Preset> {
    let starts: Vec<usize> = pack.match_indices("{name=\"").map(|(at, _)| at).collect();
    let mut schema = None;
    for (n, &start) in starts.iter().enumerate() {
        let end = starts.get(n + 1).copied().unwrap_or(pack.len());
        let record = &pack[start..end];
        if let Some(found) = field(record, "schema")? {
            schema = Some(found);
        }
        if field(record, "name")? == Some(name) {
            return Ok(Preset {
                name: name.to_owned(),
                version: field(record, "version")?
                    .ok_or("a preset without a version")?
                    .to_owned(),
                schema: schema.ok_or("a preset before any schema")?.to_owned(),
                values: field(record, "values")?
                    .ok_or("a preset without values")?
                    .to_owned(),
            });
        }
    }
    Err(format!("no preset {name:?} in the pack").into())
}

const MAGIC: &[u8; 4] = b"MXV3";

/// `mxm-vst3-host`'s blob split into its parts: `MXV3`, a version, then each part as a
/// little-endian `u64` length and its bytes.
fn split(blob: &[u8]) -> Result<(u32, Vec<Vec<u8>>)> {
    let rest = blob
        .strip_prefix(MAGIC.as_slice())
        .ok_or("not a VST3 state")?;
    let version = u32::from_le_bytes(rest.get(..4).ok_or("short")?.try_into()?);
    let mut rest = &rest[4..];
    let mut parts = Vec::new();
    while !rest.is_empty() {
        let length = u64::from_le_bytes(rest.get(..8).ok_or("short")?.try_into()?) as usize;
        parts.push(rest.get(8..8 + length).ok_or("short")?.to_vec());
        rest = &rest[8 + length..];
    }
    Ok((version, parts))
}

fn join(version: u32, parts: &[Vec<u8>]) -> Vec<u8> {
    let mut blob = MAGIC.to_vec();
    blob.extend_from_slice(&version.to_le_bytes());
    for part in parts {
        blob.extend_from_slice(&(part.len() as u64).to_le_bytes());
        blob.extend_from_slice(part);
    }
    blob
}

/// Replaces the text between `from` and the next `to` after it.
fn replace_between(text: &str, from: &str, to: &str, with: &str) -> Result<String> {
    let start = text.find(from).ok_or_else(|| format!("no {from:?}"))? + from.len();
    let end = start + text[start..].find(to).ok_or_else(|| format!("no {to:?}"))?;
    Ok(format!("{}{with}{}", &text[..start], &text[end..]))
}

/// `state` (a saved state of the plugin) with `preset` in place of its parameters.
pub fn state_with(state: &[u8], preset: &Preset) -> Result<Vec<u8>> {
    let (version, mut parts) = split(state)?;
    let component = parts.first_mut().ok_or("a state without its component")?;
    let text = std::str::from_utf8(component)?;
    let parameters = format!(
        "\n\t\tversion = \"{}\",\n\t\tschema = \"{}\",\n\t\tvalues = \"{}\",",
        preset.version, preset.schema, preset.values
    );
    let text = replace_between(text, "\tparameters = {", "\n\t},\n", &parameters)?;
    let text = replace_between(&text, "\tprogramName = \"", "\"", &preset.name)?;
    *component = text.into_bytes();
    Ok(join(version, &parts))
}

#[cfg(test)]
mod tests {
    use super::*;

    const PACK: &str = "{\n{name=\"One\",version=\"{5}\",schema=\"{\\\"A\\\"}\",values=\"{1,\\\n2}\",metadata=\"x\"},\n{name=\"Two\",version=\"{5}\",values=\"{3}\",metadata=\"y\"},\n}";

    #[test]
    fn a_preset_without_a_schema_takes_the_one_before_it() {
        let two = find(PACK, "Two").unwrap();
        assert_eq!(two.schema, "{\\\"A\\\"}");
        assert_eq!(two.values, "{3}");
        assert_eq!(find(PACK, "One").unwrap().values, "{1,\\\n2}");
        assert!(find(PACK, "Three").is_err());
    }

    #[test]
    fn a_value_is_set_by_its_name_in_the_schema() {
        let pack = "{name=\"P\",version=\"{5}\",schema=\"{  {  \\\"G\\\",  },  \\\"a\\\",\\\n  {  \\\"Setup\\\",  },  \\\"Bend\\\",  \\\"c\\\",}\",values=\"{  1,  2,\\\n  3,}\",metadata=\"\"}";
        let mut preset = find(pack, "P").unwrap();
        assert_eq!(preset.get("Setup", "Bend"), Some(2.0));
        preset.set("Setup", "Bend", 13.0).unwrap();
        assert_eq!(preset.values, "{1,13,3}");
        assert!(preset.set("Setup", "Nope", 1.0).is_err());
    }

    #[test]
    fn a_state_keeps_its_other_parts() {
        let component =
            "{\n\tparameters = {\n\t\tvalues = \"{9}\",\n\t},\n\tprogramName = \"Old\",\n}";
        let state = join(1, &[component.as_bytes().to_vec(), b"ctl".to_vec()]);
        let made = state_with(&state, &find(PACK, "Two").unwrap()).unwrap();
        let (version, parts) = split(&made).unwrap();
        assert_eq!(version, 1);
        assert_eq!(parts[1], b"ctl");
        let text = String::from_utf8(parts[0].clone()).unwrap();
        assert!(text.contains("values = \"{3}\""));
        assert!(text.contains("programName = \"Two\""));
        assert!(!text.contains("{9}"));
    }
}
