//! A VST3 instrument rendered offline: String Studio VS-3, the bowed and plucked strings.
//!
//! The module is loaded through the kit's `mxm-vst3-host` as a CLAP entry, the way MXM Player and
//! newDAWn load one, and driven with notes and parameter values at exact frames. Nothing here is
//! MIDI: a pitch is a parameter's value at a sample offset, at the parameter's own precision.
//! Every render is a fresh instance with its state loaded, so a case never hears the one before.

use clack_extensions::audio_ports::{AudioPortInfoBuffer, PluginAudioPorts};
use clack_extensions::params::{ParamInfoBuffer, ParamInfoFlags, PluginParams};
use clack_extensions::state::PluginState;
use clack_host::events::Match;
use clack_host::events::event_types::{NoteOffEvent, NoteOnEvent, ParamValueEvent};
use clack_host::prelude::*;
use clack_host::utils::Cookie;
use std::ffi::CString;
use std::path::{Path, PathBuf};

pub type Result<T> = std::result::Result<T, Box<dyn std::error::Error>>;

/// Frames per process call. Small, so a plugin that reads one parameter value per block still
/// follows a curve closely.
pub const BLOCK: usize = 32;

struct Offline;

impl HostHandlers for Offline {
    type Shared<'a> = ();
    type MainThread<'a> = ();
    type AudioProcessor<'a> = ();
}

/// One parameter as the host sees it.
#[derive(Clone, Debug)]
pub struct Param {
    pub id: u32,
    pub name: String,
    pub min: f64,
    pub max: f64,
    pub stepped: bool,
    pub hidden: bool,
}

/// What a render plays, at a frame of the render.
#[derive(Clone, Copy, Debug)]
pub enum Event {
    NoteOn {
        frame: u64,
        key: u16,
        velocity: f64,
    },
    NoteOff {
        frame: u64,
        key: u16,
    },
    /// A CLAP value: a continuous parameter's normalised value, a stepped one's step.
    Param {
        frame: u64,
        id: u32,
        value: f64,
    },
}

impl Event {
    pub fn frame(&self) -> u64 {
        match *self {
            Event::NoteOn { frame, .. } | Event::NoteOff { frame, .. } => frame,
            Event::Param { frame, .. } => frame,
        }
    }
}

/// A loaded module and the plugin it offers.
pub struct Instrument {
    entry: PluginEntry,
    id: CString,
    bundle: PathBuf,
}

impl Instrument {
    pub fn load(bundle: &Path) -> Result<Self> {
        let path = CString::new(bundle.to_string_lossy().as_bytes())?;
        let entry =
            PluginEntry::load_from_clack::<mxm_vst3_host::Vst3Entry>(&path).map_err(|_| {
                match mxm_vst3_host::load_error(bundle) {
                    Some(why) => format!("{}: {why}", bundle.display()),
                    None => format!("{}: not loadable", bundle.display()),
                }
            })?;
        let factory = entry.get_plugin_factory().ok_or("no plugin factory")?;
        let id = factory
            .plugin_descriptors()
            .find_map(|descriptor| descriptor.id().map(|id| id.to_owned()))
            .ok_or("the module offers no plugin")?;
        Ok(Self {
            entry,
            id,
            bundle: bundle.to_owned(),
        })
    }

    pub fn bundle(&self) -> &Path {
        &self.bundle
    }

    /// A new instance, its state loaded when one is given.
    pub fn instance(&self, state: Option<&[u8]>) -> Result<Instance> {
        let host_info = HostInfo::new("mxm-synthetic-audio", "mxm", "https://mxm.dk", "0")?;
        let mut instance =
            PluginInstance::<Offline>::new(|_| (), |_| (), &self.entry, &self.id, &host_info)?;
        let mut outputs = 2;
        if let Some(ports) = instance
            .plugin_shared_handle()
            .get_extension::<PluginAudioPorts>()
        {
            let mut buffer = AudioPortInfoBuffer::new();
            if let Some(port) = ports.get(&mut instance.plugin_handle(), 0, false, &mut buffer) {
                outputs = port.channel_count as usize;
            }
        }
        let mut instance = Instance {
            instance,
            outputs: outputs.max(1),
        };
        if let Some(state) = state {
            instance.load_state(state)?;
        }
        Ok(instance)
    }
}

/// One plugin instance.
pub struct Instance {
    instance: PluginInstance<Offline>,
    outputs: usize,
}

impl Instance {
    fn params_ext(&self) -> Result<PluginParams> {
        Ok(self
            .instance
            .plugin_shared_handle()
            .get_extension::<PluginParams>()
            .ok_or("the plugin has no parameters")?)
    }

    pub fn params(&mut self) -> Result<Vec<Param>> {
        let params = self.params_ext()?;
        let mut handle = self.instance.plugin_handle();
        let mut buffer = ParamInfoBuffer::new();
        let mut found = Vec::new();
        for index in 0..params.count(&mut handle) {
            if let Some(info) = params.get_info(&mut handle, index, &mut buffer) {
                found.push(Param {
                    id: info.id.get(),
                    name: String::from_utf8_lossy(info.name).into_owned(),
                    min: info.min_value,
                    max: info.max_value,
                    stepped: info.flags.contains(ParamInfoFlags::IS_STEPPED),
                    hidden: info.flags.contains(ParamInfoFlags::IS_HIDDEN),
                });
            }
        }
        Ok(found)
    }

    pub fn value(&mut self, id: u32) -> Result<f64> {
        let params = self.params_ext()?;
        let id = ClapId::from_raw(id).ok_or("invalid parameter id")?;
        Ok(params
            .get_value(&mut self.instance.plugin_handle(), id)
            .ok_or("no value")?)
    }

    pub fn text(&mut self, id: u32, value: f64) -> Result<String> {
        let params = self.params_ext()?;
        let id = ClapId::from_raw(id).ok_or("invalid parameter id")?;
        let mut buffer = [0u8; 256];
        let text = params
            .value_to_text(&mut self.instance.plugin_handle(), id, value, &mut buffer)
            .map_err(|_| "no text")?;
        Ok(String::from_utf8_lossy(text).into_owned())
    }

    pub fn parse(&mut self, id: u32, text: &str) -> Result<Option<f64>> {
        let params = self.params_ext()?;
        let id = ClapId::from_raw(id).ok_or("invalid parameter id")?;
        let text = CString::new(text)?;
        Ok(params.text_to_value(&mut self.instance.plugin_handle(), id, &text))
    }

    pub fn save_state(&mut self) -> Result<Vec<u8>> {
        let state = self
            .instance
            .plugin_shared_handle()
            .get_extension::<PluginState>()
            .ok_or("the plugin keeps no state")?;
        let mut saved = Vec::new();
        state.save(&mut self.instance.plugin_handle(), &mut saved)?;
        Ok(saved)
    }

    pub fn load_state(&mut self, bytes: &[u8]) -> Result<()> {
        let state = self
            .instance
            .plugin_shared_handle()
            .get_extension::<PluginState>()
            .ok_or("the plugin keeps no state")?;
        state.load(
            &mut self.instance.plugin_handle(),
            &mut std::io::Cursor::new(bytes),
        )?;
        Ok(())
    }

    /// Renders `frames` frames at `rate`, the channels summed to mono. `events` need not be in
    /// order; an event before the render's start is played at its first frame.
    pub fn render(&mut self, rate: f64, frames: usize, events: &[Event]) -> Result<Vec<f32>> {
        let mut events = events.to_vec();
        events.sort_by_key(Event::frame);
        let processor = self.instance.activate(
            |_, _| (),
            PluginAudioConfiguration {
                sample_rate: rate,
                min_frames_count: 1,
                max_frames_count: BLOCK as u32,
            },
        )?;
        let mut processor = processor.start_processing()?;
        let mut input_block = vec![vec![0.0f32; BLOCK]; 1];
        let mut output_block = vec![vec![0.0f32; BLOCK]; self.outputs];
        let mut input_ports = AudioPorts::with_capacity(1, 1);
        let mut output_ports = AudioPorts::with_capacity(self.outputs, 1);
        let mut input_events = EventBuffer::new();
        let mut output_events = EventBuffer::new();
        let mut mono = Vec::with_capacity(frames);
        let gain = 1.0 / self.outputs as f32;
        let (mut frame, mut next) = (0usize, 0usize);
        while frame < frames {
            let n = BLOCK.min(frames - frame);
            input_events.clear();
            output_events.clear();
            while next < events.len() && events[next].frame() < (frame + n) as u64 {
                let event = events[next];
                let time = event.frame().saturating_sub(frame as u64) as u32;
                match event {
                    Event::NoteOn { key, velocity, .. } => {
                        let pckn = Pckn::new(0u16, 0u16, key, Match::All);
                        input_events.push(&NoteOnEvent::new(time, pckn, velocity));
                    }
                    Event::NoteOff { key, .. } => {
                        let pckn = Pckn::new(0u16, 0u16, key, Match::All);
                        input_events.push(&NoteOffEvent::new(time, pckn, 0.0));
                    }
                    Event::Param { id, value, .. } => {
                        if let Some(id) = ClapId::from_raw(id) {
                            input_events.push(&ParamValueEvent::new(
                                time,
                                id,
                                Pckn::match_all(),
                                value,
                                Cookie::empty(),
                            ));
                        }
                    }
                }
                next += 1;
            }
            let inputs = input_ports.with_input_buffers([AudioPortBuffer {
                latency: 0,
                channels: AudioPortBufferType::f32_input_only(
                    input_block
                        .iter_mut()
                        .map(|b| InputChannel::from_buffer(&mut b[..n], true)),
                ),
            }]);
            let mut outputs = output_ports.with_output_buffers([AudioPortBuffer {
                latency: 0,
                channels: AudioPortBufferType::f32_output_only(
                    output_block.iter_mut().map(|b| &mut b[..n]),
                ),
            }]);
            processor.process(
                &inputs,
                &mut outputs,
                &InputEvents::from_buffer(&input_events),
                &mut OutputEvents::from_buffer(&mut output_events),
                Some(frame as u64),
                None,
            )?;
            for i in 0..n {
                mono.push(output_block.iter().map(|channel| channel[i]).sum::<f32>() * gain);
            }
            frame += n;
        }
        let processor = processor.stop_processing();
        self.instance.deactivate(processor);
        Ok(mono)
    }
}
