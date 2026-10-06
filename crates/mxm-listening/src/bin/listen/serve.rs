//! `listen session`: a calibration session served to a local page (the plan's §5). Plain `std`
//! networking, bound to 127.0.0.1 only and one request at a time — one listener, one browser tab;
//! never published.

use std::io::{BufRead, BufReader, Read, Write};
use std::net::{TcpListener, TcpStream};
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use mxm_listening::audibility::Thresholds;
use mxm_listening::session::{self, Session, Trial};

const PAGE: &str = include_str!("../../../data/session.html");

/// The served session: the trials, the pending trial's audio, and where the files go.
pub struct Served {
    pub session: Session,
    /// When the session ends: set by the first trial, so the clock runs from the listener's start,
    /// not from the server's.
    pub deadline: Option<Instant>,
    pub minutes: f64,
    pub dir: PathBuf,
    pub started: String,
    /// The sound's file, recorded in the local log for `calibrate`.
    pub file: Option<String>,
    audio: Vec<Vec<u8>>,
    trial: Option<u64>,
    finished: Option<String>,
}

impl Served {
    #[must_use]
    pub fn new(session: Session, minutes: f64, dir: PathBuf) -> Self {
        Self {
            session,
            deadline: None,
            minutes,
            dir,
            started: session::now_utc(),
            file: None,
            audio: Vec::new(),
            trial: None,
            finished: None,
        }
    }

    /// Serves until the session is finished and its files written; returns their paths.
    pub fn run(&mut self, port: u16) -> Result<Vec<PathBuf>, String> {
        let listener = TcpListener::bind(("127.0.0.1", port))
            .map_err(|e| format!("cannot listen on 127.0.0.1:{port}: {e}"))?;
        println!(
            "Open http://127.0.0.1:{port}/ — {} staircases, up to {} minutes.",
            self.session.stairs.len(),
            self.minutes
        );
        for stream in listener.incoming() {
            let Ok(stream) = stream else {
                continue;
            };
            if let Err(e) = self.handle(stream) {
                eprintln!("request failed: {e}");
            }
            if self.finished.is_some() {
                break;
            }
        }
        Ok(self.written())
    }

    fn written(&self) -> Vec<PathBuf> {
        let stem = self.stem();
        vec![
            self.dir.join("sessions").join(format!("{stem}.tsv")),
            self.dir
                .join("sessions")
                .join(format!("{stem}-outcomes.tsv")),
        ]
    }

    fn stem(&self) -> String {
        let sound: String = self
            .session
            .subject
            .sound
            .name
            .chars()
            .map(|c| {
                if c.is_ascii_alphanumeric() || c == '-' {
                    c
                } else {
                    '_'
                }
            })
            .collect();
        format!("{}-{sound}", self.started)
    }

    fn handle(&mut self, mut stream: TcpStream) -> Result<(), String> {
        let (method, path, body) = read_request(&stream)?;
        let (status, kind, bytes): (&str, &str, Vec<u8>) = match (method.as_str(), path.as_str()) {
            ("GET", "/") => (
                "200 OK",
                "text/html; charset=utf-8",
                PAGE.as_bytes().to_vec(),
            ),
            ("POST", "/device") => {
                self.session.device = field(&body, "device").unwrap_or_default();
                ("200 OK", "application/json", b"{}".to_vec())
            }
            ("GET", "/next") => ("200 OK", "application/json", self.next().into_bytes()),
            ("POST", "/answer") => {
                let trial = field(&body, "trial").and_then(|v| v.parse::<u64>().ok());
                let choice = field(&body, "choice").and_then(|v| v.parse::<usize>().ok());
                match (trial, choice) {
                    (Some(t), Some(c)) if c < 3 => match self.session.answer(t, c) {
                        Some(correct) => (
                            "200 OK",
                            "application/json",
                            format!("{{\"correct\": {correct}}}").into_bytes(),
                        ),
                        None => (
                            "409 Conflict",
                            "application/json",
                            b"{\"error\": \"not the pending trial\"}".to_vec(),
                        ),
                    },
                    _ => (
                        "400 Bad Request",
                        "application/json",
                        b"{\"error\": \"trial and choice\"}".to_vec(),
                    ),
                }
            }
            ("POST", "/stop") => ("200 OK", "application/json", self.finish().into_bytes()),
            ("GET", p) if p.starts_with("/audio/") => {
                let i = p
                    .trim_start_matches("/audio/")
                    .split(['?', '.'])
                    .next()
                    .and_then(|v| v.parse::<usize>().ok());
                match i.and_then(|i| self.audio.get(i)) {
                    Some(wav) => ("200 OK", "audio/wav", wav.clone()),
                    None => ("404 Not Found", "text/plain", b"no such interval".to_vec()),
                }
            }
            _ => ("404 Not Found", "text/plain", b"not found".to_vec()),
        };
        let head = format!(
            "HTTP/1.1 {status}\r\nContent-Type: {kind}\r\nContent-Length: {}\r\nCache-Control: no-store\r\nConnection: close\r\n\r\n",
            bytes.len()
        );
        stream
            .write_all(head.as_bytes())
            .and_then(|()| stream.write_all(&bytes))
            .map_err(|e| e.to_string())
    }

    /// The next trial as JSON, or the session's end.
    fn next(&mut self) -> String {
        let deadline = *self
            .deadline
            .get_or_insert_with(|| Instant::now() + Duration::from_secs_f64(self.minutes * 60.0));
        if Instant::now() >= deadline || self.session.done() {
            return self.finish();
        }
        let Some(trial) = self.session.next_trial() else {
            return self.finish();
        };
        self.audio = encode(&trial);
        self.trial = Some(trial.id);
        let (answered, left) = self.session.progress();
        let minutes_left = deadline
            .saturating_duration_since(Instant::now())
            .as_secs_f64()
            / 60.0;
        format!(
            "{{\"trial\": {}, \"answered\": {answered}, \"left\": {left}, \"minutes_left\": {minutes_left:.1}}}",
            trial.id
        )
    }

    /// Ends the session: writes the log and the outcomes, and answers with a summary.
    fn finish(&mut self) -> String {
        if let Some(done) = &self.finished {
            return done.clone();
        }
        let outcomes = self.session.outcomes(&Thresholds::literature());
        let [log, out] = [self.written()[0].clone(), self.written()[1].clone()];
        let date = self.started.clone();
        let wrote = std::fs::create_dir_all(self.dir.join("sessions"))
            .and_then(|()| {
                std::fs::write(
                    &log,
                    session::log_tsv(&self.session, &date, self.file.as_deref()),
                )
            })
            .and_then(|()| {
                std::fs::write(&out, session::outcomes_tsv(&self.session, &outcomes, &date))
            });
        let (checks, right) = self.session.checks();
        let mut lines: Vec<String> = outcomes
            .iter()
            .map(|o| {
                format!(
                    "{}: {} ({} {}) against the literature's {}",
                    o.operator.name(),
                    short(o.value),
                    o.reading,
                    match o.kind {
                        mxm_listening::audibility::Kind::Absolute => "abs",
                        mxm_listening::audibility::Kind::Relative => "rel",
                    },
                    short(o.literature)
                )
            })
            .collect();
        let unfinished = self
            .session
            .stairs
            .iter()
            .filter(|(_, s)| s.estimate().is_none())
            .count();
        if unfinished > 0 {
            lines.push(format!(
                "{unfinished} staircase(s) did not reach enough reversals: run another session"
            ));
        }
        lines.push(format!("checks: {right} of {checks} right"));
        match wrote {
            Ok(()) => lines.push(format!("written: {}", log.display())),
            Err(e) => lines.push(format!("could not write the session: {e}")),
        }
        let json = format!(
            "{{\"done\": true, \"summary\": [{}]}}",
            lines
                .iter()
                .map(|l| format!("\"{}\"", escape(l)))
                .collect::<Vec<_>>()
                .join(", ")
        );
        self.finished = Some(json.clone());
        json
    }
}

/// The three intervals as WAV bytes, scaled together so the loudest peak sits at −1 dBFS or below:
/// their levels relative to each other are the trial.
fn encode(trial: &Trial) -> Vec<Vec<u8>> {
    let peak = trial
        .sounds
        .iter()
        .flat_map(|s| s.samples.iter())
        .fold(0.0f32, |m, v| m.max(v.abs()));
    let gain = if peak > 0.891 { 0.891 / peak } else { 1.0 };
    trial
        .sounds
        .iter()
        .map(|s| {
            let x: Vec<f32> = s.samples.iter().map(|v| v * gain).collect();
            mxm_audio_file::encode(
                &x,
                1,
                s.rate,
                mxm_audio_file::Target::Wav(mxm_audio_file::Bits::TwentyFour),
            )
            .map(|(bytes, _)| bytes)
            .unwrap_or_default()
        })
        .collect()
}

/// Reads one request: method, path, and body.
fn read_request(stream: &TcpStream) -> Result<(String, String, String), String> {
    let mut reader = BufReader::new(stream);
    let mut line = String::new();
    reader.read_line(&mut line).map_err(|e| e.to_string())?;
    let mut parts = line.split_whitespace();
    let method = parts.next().unwrap_or_default().to_string();
    let path = parts.next().unwrap_or("/").to_string();
    let mut length = 0usize;
    loop {
        let mut header = String::new();
        if reader.read_line(&mut header).map_err(|e| e.to_string())? == 0
            || header == "\r\n"
            || header == "\n"
        {
            break;
        }
        if let Some((k, v)) = header.split_once(':') {
            if k.trim().eq_ignore_ascii_case("content-length") {
                length = v.trim().parse().unwrap_or(0);
            }
        }
    }
    let mut body = vec![0u8; length.min(1 << 16)];
    reader.read_exact(&mut body).map_err(|e| e.to_string())?;
    Ok((method, path, String::from_utf8_lossy(&body).into_owned()))
}

/// A form field from an `application/x-www-form-urlencoded` body.
fn field(body: &str, name: &str) -> Option<String> {
    body.split('&').find_map(|kv| {
        let (k, v) = kv.split_once('=')?;
        (k == name).then(|| decode(v))
    })
}

/// Percent-decoding, `+` as a space.
fn decode(s: &str) -> String {
    let bytes = s.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        match bytes[i] {
            b'+' => out.push(b' '),
            b'%' if i + 2 < bytes.len() => match u8::from_str_radix(&s[i + 1..i + 3], 16) {
                Ok(b) => {
                    out.push(b);
                    i += 2;
                }
                Err(_) => out.push(b'%'),
            },
            b => out.push(b),
        }
        i += 1;
    }
    String::from_utf8_lossy(&out).into_owned()
}

fn escape(s: &str) -> String {
    s.replace('\\', "\\\\").replace('"', "\\\"")
}

fn short(x: f64) -> String {
    if x.abs() >= 100.0 {
        format!("{x:.0}")
    } else if x.abs() >= 1.0 {
        format!("{x:.2}")
    } else {
        format!("{x:.4}")
    }
}

/// The folder for local listening state: `--dir`, else `.listening` in the current directory.
#[must_use]
pub fn state_dir(given: Option<&Path>) -> PathBuf {
    given.map_or_else(|| PathBuf::from(".listening"), Path::to_path_buf)
}

#[cfg(test)]
mod tests {
    use super::{decode, field};

    #[test]
    fn form_fields_decode() {
        assert_eq!(field("trial=12&choice=2", "choice").as_deref(), Some("2"));
        assert_eq!(
            field("device=open+back%2C+AKG", "device").as_deref(),
            Some("open back, AKG")
        );
        assert_eq!(field("a=1", "b"), None);
        assert_eq!(decode("100%"), "100%");
        assert_eq!(decode("%zz"), "%zz");
    }
}
