//! Playback (the plan's §2.6): the listened sound and the rebuild's parts through the default output
//! device, with a playhead the window follows.
//!
//! **Everything that costs happens before the callback.** Each sound is resampled to the device's rate
//! with the listener's band-limited resampler on a thread of its own, and handed to the callback
//! whole, as an immutable shared buffer, through a lock-free queue. **The callback only copies**: it
//! takes waiting commands at the start of a block, writes the current buffer's samples to every device
//! channel in the device's sample format, and publishes where it is. It never allocates, frees, locks,
//! logs or waits. A buffer it lets go of goes back through a second queue for the window to drop; a
//! command waits in its queue until there is room to give the current buffer back, so the callback
//! never holds a buffer's last reference when it lets go.
//!
//! The device is opened on the first play, not before: a window that never plays never takes it. A
//! stream error stops playback and says why; the next play opens the device again.

use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::mpsc::{self, Receiver, Sender};
use std::sync::{Arc, Mutex};

use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use mxm_listening::repr::resample;

/// What can be played.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Which {
    /// The mono buffer the listener read.
    Listened,
    /// The rebuild's modes alone.
    Modes,
    /// The rebuild's noise alone.
    Noise,
    /// The whole rebuild.
    Rebuild,
}

impl Which {
    pub const ALL: [Which; 4] = [Which::Listened, Which::Modes, Which::Noise, Which::Rebuild];

    #[must_use]
    pub fn label(self) -> &'static str {
        match self {
            Which::Listened => "LISTENED",
            Which::Modes => "MODES",
            Which::Noise => "NOISE",
            Which::Rebuild => "REBUILD",
        }
    }

    /// What it is, for its button's hover.
    #[must_use]
    pub fn meaning(self) -> &'static str {
        match self {
            Which::Listened => {
                "The sound as the listener read it: mono, cut at its limit. Space plays or stops it."
            }
            Which::Modes => {
                "The rebuild's ringing alone: the modes or partials the readings hold, resynthesised."
            }
            Which::Noise => "The rebuild's noise alone: what the readings hold that is not a mode.",
            Which::Rebuild => {
                "The sound rebuilt from its readings. Where it differs from the listened sound, the \
                 readings miss something."
            }
        }
    }
}

/// A message to the callback.
#[derive(Debug)]
pub enum Command {
    Play { id: u64, buffer: Arc<[f32]> },
    Stop,
}

/// What the callback publishes: which buffer plays (0 for none) and where, in frames.
#[derive(Debug, Default)]
pub struct Shared {
    playing: AtomicU64,
    frame: AtomicU64,
}

impl Shared {
    /// The buffer playing and its frame, or `None`.
    #[must_use]
    pub fn now(&self) -> Option<(u64, u64)> {
        let id = self.playing.load(Ordering::Acquire);
        (id != 0).then(|| (id, self.frame.load(Ordering::Acquire)))
    }
}

/// The callback's side: the current buffer and where it is.
pub struct Transport {
    commands: rtrb::Consumer<Command>,
    returns: rtrb::Producer<Arc<[f32]>>,
    current: Option<(u64, Arc<[f32]>)>,
    pos: usize,
    shared: Arc<Shared>,
}

/// The window's side of the two queues.
pub struct Controls {
    commands: rtrb::Producer<Command>,
    returns: rtrb::Consumer<Arc<[f32]>>,
    shared: Arc<Shared>,
}

/// The two queues, `capacity` deep, and their ends.
#[must_use]
pub fn channel(capacity: usize) -> (Controls, Transport) {
    let (commands, take) = rtrb::RingBuffer::new(capacity);
    let (give, returns) = rtrb::RingBuffer::new(capacity);
    let shared = Arc::new(Shared::default());
    (
        Controls {
            commands,
            returns,
            shared: Arc::clone(&shared),
        },
        Transport {
            commands: take,
            returns: give,
            current: None,
            pos: 0,
            shared,
        },
    )
}

impl Controls {
    /// Sends a command; `false` when the queue is full.
    pub fn send(&mut self, command: Command) -> bool {
        self.commands.push(command).is_ok()
    }

    /// Drops, here, every buffer the callback gave back; returns how many.
    pub fn reclaim(&mut self) -> usize {
        let mut n = 0;
        while self.returns.pop().is_ok() {
            n += 1;
        }
        n
    }

    #[must_use]
    pub fn shared(&self) -> &Arc<Shared> {
        &self.shared
    }
}

impl Transport {
    /// Renders `frames` frames, handing each frame's sample to `write`. Takes waiting commands first,
    /// each only when the buffer it replaces can be given back.
    pub fn render(&mut self, frames: usize, mut write: impl FnMut(usize, f32)) {
        while self.returns.slots() > 0 {
            match self.commands.pop() {
                Ok(Command::Play { id, buffer }) => {
                    self.release();
                    self.current = Some((id, buffer));
                    self.pos = 0;
                }
                Ok(Command::Stop) => self.release(),
                Err(_) => break,
            }
        }
        for frame in 0..frames {
            let sample = match &self.current {
                Some((_, buffer)) => buffer.get(self.pos).copied(),
                None => None,
            };
            match sample {
                Some(v) => {
                    self.pos += 1;
                    write(frame, v);
                }
                None => {
                    if self.current.is_some() && self.returns.slots() > 0 {
                        self.release();
                    }
                    write(frame, 0.0);
                }
            }
        }
        let id = self.current.as_ref().map_or(0, |(id, _)| *id);
        self.shared.frame.store(self.pos as u64, Ordering::Release);
        self.shared.playing.store(id, Ordering::Release);
    }

    /// Lets go of the current buffer, giving it back; only called with room in the return queue.
    fn release(&mut self) {
        if let Some((_, buffer)) = self.current.take() {
            // Room was checked: the push cannot fail, and the buffer is never dropped here.
            let _ = self.returns.push(buffer);
        }
        self.pos = 0;
    }
}

/// `source` at `from` Hz, ready for a device at `to` Hz: the same buffer when the rates agree, else
/// resampled by the listener's band-limited resampler. Held within ±1.
#[must_use]
pub fn prepare(source: &Arc<[f32]>, from: u32, to: u32) -> Arc<[f32]> {
    if from == to {
        return Arc::clone(source);
    }
    let x: Vec<f64> = source.iter().map(|&v| f64::from(v)).collect();
    resample::rate(&x, f64::from(from), f64::from(to))
        .into_iter()
        .map(|v| (v as f32).clamp(-1.0, 1.0))
        .collect()
}

/// A sound to prepare: whose it is (the drop's sequence number), which, the samples and their rate.
type Key = (u64, Which, u32);

/// A prepared sound, as its thread hands it back.
type Ready = (Key, Arc<[f32]>);

/// The device, the queues, and the sounds prepared for it.
pub struct Playback {
    stream: Option<Open>,
    /// A stream's error, set on cpal's error thread.
    error: Arc<Mutex<Option<String>>>,
    prepared: HashMap<Key, Arc<[f32]>>,
    ready: (Sender<Ready>, Receiver<Ready>),
    /// The sound asked for and waiting to be prepared.
    wanted: Option<Key>,
    /// What each id sent to the callback was.
    sent: HashMap<u64, Key>,
    next_id: u64,
    /// Why playback last stopped on its own, for the window to say.
    notice: Option<String>,
}

struct Open {
    // Kept alive: dropping it stops the sound.
    _stream: cpal::Stream,
    controls: Controls,
    rate: u32,
}

impl Default for Playback {
    fn default() -> Self {
        Self::new()
    }
}

impl Playback {
    #[must_use]
    pub fn new() -> Self {
        Self {
            stream: None,
            error: Arc::default(),
            prepared: HashMap::new(),
            ready: mpsc::channel(),
            wanted: None,
            sent: HashMap::new(),
            next_id: 0,
            notice: None,
        }
    }

    /// Plays `which` of the drop `seq`: `source` at `rate` Hz. Opens the device if it is not open,
    /// and prepares the sound on a thread of its own if it has not been.
    pub fn play(&mut self, seq: u64, which: Which, source: &Arc<[f32]>, rate: u32) {
        if self.stream.is_none() {
            match open(Arc::clone(&self.error)) {
                Ok(open) => self.stream = Some(open),
                Err(why) => {
                    self.notice = Some(format!("No playback: {why}"));
                    return;
                }
            }
        }
        let device = self.stream.as_ref().map_or(rate, |s| s.rate);
        let key = (seq, which, device);
        if let Some(buffer) = self.prepared.get(&key).cloned() {
            self.start(key, buffer);
            return;
        }
        self.wanted = Some(key);
        let source = Arc::clone(source);
        let done = self.ready.0.clone();
        std::thread::Builder::new()
            .name("mxm-listener-hud playback prep".into())
            .spawn(move || {
                let _ = done.send((key, prepare(&source, rate, device)));
            })
            .expect("the preparing thread starts");
    }

    fn start(&mut self, key: Key, buffer: Arc<[f32]>) {
        self.next_id += 1;
        let id = self.next_id;
        if let Some(open) = &mut self.stream
            && open.controls.send(Command::Play { id, buffer })
        {
            self.sent.insert(id, key);
            self.notice = None;
        }
    }

    pub fn stop(&mut self) {
        self.wanted = None;
        if let Some(open) = &mut self.stream {
            let _ = open.controls.send(Command::Stop);
        }
    }

    /// Forgets every prepared sound but those of the drop `seq`, dropping them here.
    pub fn keep_only(&mut self, seq: u64) {
        self.prepared.retain(|k, _| k.0 == seq);
    }

    /// Once a frame: drops the buffers the callback gave back, starts a sound whose preparation has
    /// finished, and stops on a stream error, saying why.
    pub fn poll(&mut self) {
        while let Ok((key, buffer)) = self.ready.1.try_recv() {
            self.prepared.insert(key, Arc::clone(&buffer));
            if self.wanted == Some(key) {
                self.wanted = None;
                self.start(key, buffer);
            }
        }
        if let Some(open) = &mut self.stream {
            open.controls.reclaim();
        }
        let error = self.error.lock().ok().and_then(|mut e| e.take());
        if let Some(why) = error {
            self.stream = None;
            self.wanted = None;
            self.notice = Some(format!("Playback stopped: {why}"));
        }
    }

    /// What plays now — its drop, which part, and seconds from its start — or `None`.
    #[must_use]
    pub fn now(&self) -> Option<(u64, Which, f64)> {
        let open = self.stream.as_ref()?;
        let (id, frame) = open.controls.shared().now()?;
        let (seq, which, rate) = *self.sent.get(&id)?;
        Some((seq, which, frame as f64 / f64::from(rate.max(1))))
    }

    /// Whether a sound is being prepared to play.
    #[must_use]
    pub fn preparing(&self) -> bool {
        self.wanted.is_some()
    }

    /// Why playback last stopped on its own, or could not start.
    #[must_use]
    pub fn notice(&self) -> Option<&str> {
        self.notice.as_deref()
    }
}

/// The default output device, at its own rate and format, with a callback over a new transport.
fn open(error: Arc<Mutex<Option<String>>>) -> Result<Open, String> {
    let host = cpal::default_host();
    let device = host
        .default_output_device()
        .ok_or_else(|| "there is no output device".to_string())?;
    let default = device.default_output_config().map_err(|e| e.to_string())?;
    let config = cpal::StreamConfig {
        channels: default.channels(),
        sample_rate: default.sample_rate(),
        buffer_size: cpal::BufferSize::Default,
    };
    let rate = config.sample_rate;
    let (controls, transport) = channel(64);
    let stream = match default.sample_format() {
        cpal::SampleFormat::F32 => build::<f32>(&device, config, transport, error),
        cpal::SampleFormat::I16 => build::<i16>(&device, config, transport, error),
        cpal::SampleFormat::U16 => build::<u16>(&device, config, transport, error),
        cpal::SampleFormat::I32 => build::<i32>(&device, config, transport, error),
        other => Err(format!(
            "the device takes {other} samples, which are not played"
        )),
    }?;
    stream.play().map_err(|e| e.to_string())?;
    Ok(Open {
        _stream: stream,
        controls,
        rate,
    })
}

fn build<T>(
    device: &cpal::Device,
    config: cpal::StreamConfig,
    mut transport: Transport,
    error: Arc<Mutex<Option<String>>>,
) -> Result<cpal::Stream, String>
where
    T: cpal::SizedSample + cpal::FromSample<f32>,
{
    let channels = usize::from(config.channels).max(1);
    device
        .build_output_stream(
            config,
            move |data: &mut [T], _: &cpal::OutputCallbackInfo| {
                let frames = data.len() / channels;
                transport.render(frames, |frame, v| {
                    let s = T::from_sample(v);
                    for c in &mut data[frame * channels..(frame + 1) * channels] {
                        *c = s;
                    }
                });
            },
            // On cpal's error path, not in the render: records the cause for the window.
            move |e| {
                if let Ok(mut slot) = error.lock() {
                    *slot = Some(e.to_string());
                }
            },
            None,
        )
        .map_err(|e| e.to_string())
}
