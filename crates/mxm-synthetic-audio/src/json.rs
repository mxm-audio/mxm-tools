//! A small JSON writer for the sidecars: values in, text out, keys in the order given.

use std::fmt::Write;

#[derive(Clone, Debug)]
pub enum Json {
    Null,
    Bool(bool),
    Num(f64),
    Str(String),
    Arr(Vec<Json>),
    Obj(Vec<(String, Json)>),
}

impl From<f64> for Json {
    fn from(value: f64) -> Self {
        Json::Num(value)
    }
}

impl From<usize> for Json {
    fn from(value: usize) -> Self {
        Json::Num(value as f64)
    }
}

impl From<i32> for Json {
    fn from(value: i32) -> Self {
        Json::Num(f64::from(value))
    }
}

impl From<bool> for Json {
    fn from(value: bool) -> Self {
        Json::Bool(value)
    }
}

impl From<&str> for Json {
    fn from(value: &str) -> Self {
        Json::Str(value.to_owned())
    }
}

impl From<String> for Json {
    fn from(value: String) -> Self {
        Json::Str(value)
    }
}

impl<T: Into<Json>> From<Vec<T>> for Json {
    fn from(values: Vec<T>) -> Self {
        Json::Arr(values.into_iter().map(Into::into).collect())
    }
}

impl<T: Into<Json>> From<Option<T>> for Json {
    fn from(value: Option<T>) -> Self {
        value.map_or(Json::Null, Into::into)
    }
}

/// An object from `(key, value)` pairs.
pub fn obj<K: Into<String>>(pairs: Vec<(K, Json)>) -> Json {
    Json::Obj(pairs.into_iter().map(|(k, v)| (k.into(), v)).collect())
}

impl Json {
    /// The object's pairs, for adding to; a non-object becomes an empty object first.
    pub fn push(&mut self, key: &str, value: impl Into<Json>) {
        if !matches!(self, Json::Obj(_)) {
            *self = Json::Obj(Vec::new());
        }
        if let Json::Obj(pairs) = self {
            pairs.push((key.to_owned(), value.into()));
        }
    }

    pub fn pretty(&self) -> String {
        let mut out = String::new();
        self.write(&mut out, 0);
        out.push('\n');
        out
    }

    /// Short arrays of numbers stay on one line, so a sidecar stays readable.
    fn inline(&self) -> bool {
        match self {
            Json::Arr(values) => {
                values.len() <= 12
                    && values
                        .iter()
                        .all(|v| matches!(v, Json::Num(_) | Json::Str(_) | Json::Bool(_)))
            }
            Json::Obj(pairs) => pairs.is_empty(),
            _ => true,
        }
    }

    fn write(&self, out: &mut String, depth: usize) {
        let pad = |out: &mut String, depth: usize| {
            for _ in 0..depth {
                out.push_str("  ");
            }
        };
        match self {
            Json::Null => out.push_str("null"),
            Json::Bool(b) => out.push_str(if *b { "true" } else { "false" }),
            Json::Num(x) => number(out, *x),
            Json::Str(s) => string(out, s),
            Json::Arr(values) if self.inline() => {
                out.push('[');
                for (i, v) in values.iter().enumerate() {
                    if i > 0 {
                        out.push_str(", ");
                    }
                    v.write(out, depth);
                }
                out.push(']');
            }
            Json::Arr(values) => {
                out.push_str("[\n");
                for (i, v) in values.iter().enumerate() {
                    pad(out, depth + 1);
                    v.write(out, depth + 1);
                    if i + 1 < values.len() {
                        out.push(',');
                    }
                    out.push('\n');
                }
                pad(out, depth);
                out.push(']');
            }
            Json::Obj(pairs) if pairs.is_empty() => out.push_str("{}"),
            Json::Obj(pairs) => {
                out.push_str("{\n");
                for (i, (k, v)) in pairs.iter().enumerate() {
                    pad(out, depth + 1);
                    string(out, k);
                    out.push_str(": ");
                    v.write(out, depth + 1);
                    if i + 1 < pairs.len() {
                        out.push(',');
                    }
                    out.push('\n');
                }
                pad(out, depth);
                out.push('}');
            }
        }
    }
}

/// A number as JSON: non-finite ones are `null`, whole ones without a fraction, the rest rounded
/// to 6 significant decimals past the point (enough for seconds to the microsecond).
fn number(out: &mut String, x: f64) {
    if !x.is_finite() {
        out.push_str("null");
    } else if x == x.trunc() && x.abs() < 1e15 {
        let _ = write!(out, "{}", x as i64);
    } else {
        let text = format!("{x:.6}");
        let text = text.trim_end_matches('0').trim_end_matches('.');
        out.push_str(text);
    }
}

fn string(out: &mut String, s: &str) {
    out.push('"');
    for c in s.chars() {
        match c {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\t' => out.push_str("\\t"),
            c if (c as u32) < 0x20 => {
                let _ = write!(out, "\\u{:04x}", c as u32);
            }
            c => out.push(c),
        }
    }
    out.push('"');
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn writes_nested_values() {
        let value = obj(vec![
            ("a", Json::from(1.5)),
            ("b", Json::from(vec![1.0, 2.0])),
            ("c", obj(vec![("d", Json::from("x\"y"))])),
            ("e", Json::Num(f64::NAN)),
        ]);
        assert_eq!(
            value.pretty(),
            "{\n  \"a\": 1.5,\n  \"b\": [1, 2],\n  \"c\": {\n    \"d\": \"x\\\"y\"\n  },\n  \"e\": null\n}\n"
        );
    }
}
