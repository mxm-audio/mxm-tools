//! Playback without a device (the plan's §2.6 and §5): the transport the callback runs — start,
//! switch, stop, the end of a sound — never lets a buffer go but through the return queue, holds a
//! command back while that queue is full, and publishes where it is; a sound is prepared for the
//! device's rate, the same buffer when the rates agree. Playing through a real device is checked by
//! hand.

use std::sync::Arc;

use mxm_listener_hud::playback::{Command, channel, prepare};

/// Renders `frames` mono frames, returning them.
fn render(t: &mut mxm_listener_hud::playback::Transport, frames: usize) -> Vec<f32> {
    let mut out = vec![f32::NAN; frames];
    t.render(frames, |i, v| out[i] = v);
    out
}

fn ramp(n: usize, from: f32) -> Arc<[f32]> {
    (0..n).map(|i| from + i as f32).collect()
}

#[test]
fn a_sound_plays_to_its_end_and_the_playhead_follows() {
    let (mut controls, mut transport) = channel(8);
    let a = ramp(5, 1.0);
    assert!(controls.send(Command::Play {
        id: 7,
        buffer: Arc::clone(&a)
    }));
    assert_eq!(render(&mut transport, 3), [1.0, 2.0, 3.0]);
    assert_eq!(controls.shared().now(), Some((7, 3)));
    // The end: the last samples, then silence, and nothing playing.
    assert_eq!(render(&mut transport, 4), [4.0, 5.0, 0.0, 0.0]);
    assert_eq!(controls.shared().now(), None);
    // The buffer came back to be dropped here, never in the callback.
    assert_eq!(controls.reclaim(), 1);
    assert_eq!(Arc::strong_count(&a), 1);
    assert_eq!(render(&mut transport, 2), [0.0, 0.0]);
}

#[test]
fn a_switch_and_a_stop_give_the_buffer_back() {
    let (mut controls, mut transport) = channel(8);
    let (a, b) = (ramp(100, 0.0), ramp(100, 1000.0));
    controls.send(Command::Play {
        id: 1,
        buffer: Arc::clone(&a),
    });
    render(&mut transport, 10);
    // Two references to `a`: this test's and the callback's.
    assert_eq!(Arc::strong_count(&a), 2);
    controls.send(Command::Play {
        id: 2,
        buffer: Arc::clone(&b),
    });
    // A switch starts the new sound from its start at the next block.
    assert_eq!(render(&mut transport, 2), [1000.0, 1001.0]);
    assert_eq!(controls.shared().now(), Some((2, 2)));
    // `a` is still alive — given back, not dropped — until the window reclaims it.
    assert_eq!(Arc::strong_count(&a), 2);
    assert_eq!(controls.reclaim(), 1);
    assert_eq!(Arc::strong_count(&a), 1);

    controls.send(Command::Stop);
    assert_eq!(render(&mut transport, 3), [0.0, 0.0, 0.0]);
    assert_eq!(controls.shared().now(), None);
    assert_eq!(controls.reclaim(), 1);
    assert_eq!(Arc::strong_count(&b), 1);
}

#[test]
fn a_full_return_queue_holds_the_next_command_back() {
    // Room for one returned buffer: the second switch must wait until the window has reclaimed it.
    let (mut controls, mut transport) = channel(1);
    let (a, b, c) = (ramp(50, 0.0), ramp(50, 100.0), ramp(50, 200.0));
    controls.send(Command::Play { id: 1, buffer: a });
    render(&mut transport, 1);
    controls.send(Command::Play { id: 2, buffer: b });
    render(&mut transport, 1); // `a` given back: the return queue is full
    controls.send(Command::Play { id: 3, buffer: c });
    assert_eq!(
        render(&mut transport, 1),
        [101.0],
        "still `b`: no room to give it back"
    );
    assert_eq!(controls.reclaim(), 1);
    assert_eq!(render(&mut transport, 1), [200.0], "now `c`");
    assert_eq!(controls.shared().now(), Some((3, 1)));
}

/// The default output device opens, and its callback runs and publishes: a manual check on a machine
/// with a device (`cargo test -p mxm-listener-hud --release --test playback -- --ignored`). It plays
/// half a second of silence, so nothing is heard.
#[test]
#[ignore = "needs an output device"]
fn the_default_device_plays() {
    use mxm_listener_hud::playback::{Playback, Which};
    let mut playback = Playback::new();
    let silence: Arc<[f32]> = vec![0.0; 24_000].into();
    playback.play(1, Which::Listened, &silence, 48_000);
    let started = std::time::Instant::now();
    let mut heard = None;
    while started.elapsed() < std::time::Duration::from_secs(5) {
        playback.poll();
        assert_eq!(playback.notice(), None, "{:?}", playback.notice());
        if let Some(now) = playback.now() {
            heard = Some(now);
            break;
        }
        std::thread::sleep(std::time::Duration::from_millis(10));
    }
    let (seq, which, seconds) = heard.expect("the callback ran");
    assert_eq!((seq, which), (1, Which::Listened));
    assert!(seconds >= 0.0);
}

#[test]
fn a_sound_is_prepared_for_the_devices_rate() {
    // The same rate: the very same buffer, no copy.
    let source: Arc<[f32]> = (0..480).map(|i| (i as f32 * 0.01).sin() * 0.5).collect();
    assert!(Arc::ptr_eq(&prepare(&source, 48_000, 48_000), &source));

    // 44.1 kHz to 48 kHz: the length scales, and a 1 kHz sine stays at 1 kHz.
    let from = 44_100.0;
    let sine: Arc<[f32]> = (0..44_100)
        .map(|i| (0.5 * (std::f64::consts::TAU * 1000.0 * f64::from(i) / from).sin()) as f32)
        .collect();
    let out = prepare(&sine, 44_100, 48_000);
    assert!((out.len() as i64 - 48_000).abs() <= 1, "{}", out.len());
    // Rising zero crossings over the middle second-half, away from the edges' ringing.
    let middle = &out[4_800..43_200];
    let rises = middle
        .windows(2)
        .filter(|w| w[0] < 0.0 && w[1] >= 0.0)
        .count();
    let hz = rises as f64 / (middle.len() as f64 / 48_000.0);
    assert!((hz - 1000.0).abs() < 5.0, "{hz} Hz");
    assert!(out.iter().all(|v| v.abs() <= 1.0));
}
