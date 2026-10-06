//! The worker's bounds (the plan's §2.2 and §5): one analysis running and one waiting, the newest
//! drop winning, a superseded analysis stopped at its next stage boundary, never a second worker, and
//! a panicking analysis survived. Each test holds an analysis still with a release channel.

use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::mpsc;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use mxm_listener_hud::worker::{Emit, Outcome, Worker};

/// Long enough for a thread to be scheduled on a loaded machine; a pass never waits this long.
const PATIENCE: Duration = Duration::from_secs(10);

#[test]
fn a_burst_of_drops_runs_the_first_and_then_only_the_newest() {
    let (started_tx, started) = mpsc::channel::<u32>();
    let (release, release_rx) = mpsc::channel::<()>();
    let release_rx = Arc::new(Mutex::new(release_rx));
    let running = Arc::new(AtomicUsize::new(0));
    let most = Arc::new(AtomicUsize::new(0));
    let stopped_early = Arc::new(AtomicUsize::new(0));

    let analyse = {
        let (running, most, stopped_early) = (
            Arc::clone(&running),
            Arc::clone(&most),
            Arc::clone(&stopped_early),
        );
        move |request: &u32, emit: &mut Emit<'_, (u32, u32)>| {
            let now = running.fetch_add(1, Ordering::SeqCst) + 1;
            most.fetch_max(now, Ordering::SeqCst);
            started_tx.send(*request).unwrap();
            for stage in 0..3 {
                release_rx.lock().unwrap().recv().unwrap();
                if !emit((*request, stage)) {
                    stopped_early.fetch_add(1, Ordering::SeqCst);
                    break;
                }
            }
            running.fetch_sub(1, Ordering::SeqCst);
        }
    };
    let (mut worker, updates) = Worker::spawn(analyse, || {});

    worker.submit(1);
    assert_eq!(started.recv_timeout(PATIENCE).unwrap(), 1);
    // Three drops while the first is held: only the newest may wait.
    worker.submit(2);
    worker.submit(3);
    let newest = worker.submit(4);

    // The first finishes its stage, is refused its update, and stops there.
    release.send(()).unwrap();
    assert_eq!(
        started.recv_timeout(PATIENCE).unwrap(),
        4,
        "2 and 3 never start"
    );
    for _ in 0..3 {
        release.send(()).unwrap();
    }
    let got: Vec<(u64, (u32, u32))> = (0..3)
        .map(|_| match updates.recv_timeout(PATIENCE).unwrap() {
            (seq, Outcome::Update(update)) => (seq, update),
            (_, Outcome::Failed(why)) => panic!("{why}"),
        })
        .collect();
    assert_eq!(
        got,
        vec![(newest, (4, 0)), (newest, (4, 1)), (newest, (4, 2))]
    );
    assert!(started.recv_timeout(Duration::from_millis(200)).is_err());
    assert!(updates.try_recv().is_err(), "nothing from the first drop");
    assert_eq!(stopped_early.load(Ordering::SeqCst), 1);
    assert_eq!(most.load(Ordering::SeqCst), 1, "never two analyses at once");
}

#[test]
fn a_panicking_analysis_is_reported_and_the_worker_goes_on() {
    let analyse = |request: &u32, emit: &mut Emit<'_, u32>| {
        assert!(*request != 1, "the listener failed");
        emit(*request);
    };
    let (mut worker, updates) = Worker::spawn(analyse, || {});
    let first = worker.submit(1);
    match updates.recv_timeout(PATIENCE).unwrap() {
        (seq, Outcome::Failed(why)) => {
            assert_eq!(seq, first);
            assert!(why.contains("the listener failed"), "{why}");
        }
        other => panic!("{other:?}"),
    }
    let second = worker.submit(2);
    match updates.recv_timeout(PATIENCE).unwrap() {
        (seq, Outcome::Update(2)) => assert_eq!(seq, second),
        other => panic!("{other:?}"),
    }
}
