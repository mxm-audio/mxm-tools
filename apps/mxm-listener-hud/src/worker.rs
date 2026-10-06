//! One analysis thread, bounded, latest wins (the plan's §2.2).
//!
//! At most one analysis runs and one waits. A new request replaces the waiting one — which then never
//! starts — and tells the running one to stop at its next stage boundary: the `emit` it reports
//! through refuses anything once a newer request exists, and returns `false` so it stops. Every
//! update carries its request's sequence number, so the window can discard one that arrives late.
//!
//! The worker knows nothing about sounds or egui: it is given the analysis and a way to wake the
//! window, so the tests can hold an analysis still while drops pile up behind it.

use std::panic::{AssertUnwindSafe, catch_unwind};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::mpsc::{self, Receiver, Sender};
use std::sync::{Arc, Condvar, Mutex, MutexGuard, PoisonError};

/// What an analysis reports through: an update, and whether to go on.
pub type Emit<'a, U> = dyn FnMut(U) -> bool + 'a;

/// What the worker sends back: the request's sequence number and one update, or the analysis's
/// failure when it panicked (the worker survives it and takes the next request).
#[derive(Debug)]
pub enum Outcome<U> {
    Update(U),
    Failed(String),
}

struct Waiting<R> {
    seq: u64,
    request: R,
}

struct Slot<R> {
    waiting: Option<Waiting<R>>,
    closed: bool,
}

struct Shared<R> {
    slot: Mutex<Slot<R>>,
    wake: Condvar,
    /// The newest request's sequence number: an analysis whose own is older stops.
    latest: AtomicU64,
}

/// The analysis thread and the one slot that waits for it.
pub struct Worker<R> {
    shared: Arc<Shared<R>>,
    next: u64,
}

impl<R: Send + 'static> Worker<R> {
    /// Starts the thread. `analyse` runs one request, reporting each stage through `emit` and
    /// stopping when `emit` returns `false`; `notify` is called after every update (the window
    /// repaints).
    pub fn spawn<U, A, N>(analyse: A, notify: N) -> (Self, Receiver<(u64, Outcome<U>)>)
    where
        U: Send + 'static,
        A: FnMut(&R, &mut Emit<'_, U>) + Send + 'static,
        N: Fn() + Send + 'static,
    {
        let shared = Arc::new(Shared {
            slot: Mutex::new(Slot {
                waiting: None,
                closed: false,
            }),
            wake: Condvar::new(),
            latest: AtomicU64::new(0),
        });
        let (sender, receiver) = mpsc::channel();
        let thread_shared = Arc::clone(&shared);
        std::thread::Builder::new()
            .name("mxm-listener-hud analysis".into())
            .spawn(move || run(&thread_shared, analyse, &sender, &notify))
            .expect("the analysis thread starts");
        (Self { shared, next: 0 }, receiver)
    }

    /// Asks for `request` to be analysed, superseding every request before it. Returns its sequence
    /// number, which every update for it carries.
    pub fn submit(&mut self, request: R) -> u64 {
        self.next += 1;
        let seq = self.next;
        let mut slot = lock(&self.shared.slot);
        self.shared.latest.store(seq, Ordering::SeqCst);
        slot.waiting = Some(Waiting { seq, request });
        drop(slot);
        self.shared.wake.notify_one();
        seq
    }
}

impl<R> Drop for Worker<R> {
    /// Closes the slot and stops the running analysis at its next stage boundary. The thread is not
    /// joined: closing the window must not wait for a stage to finish.
    fn drop(&mut self) {
        let mut slot = lock(&self.shared.slot);
        slot.closed = true;
        slot.waiting = None;
        self.shared.latest.store(u64::MAX, Ordering::SeqCst);
        drop(slot);
        self.shared.wake.notify_one();
    }
}

fn run<R, U, A>(
    shared: &Shared<R>,
    mut analyse: A,
    sender: &Sender<(u64, Outcome<U>)>,
    notify: &dyn Fn(),
) where
    A: FnMut(&R, &mut Emit<'_, U>),
{
    loop {
        let Waiting { seq, request } = {
            let mut slot = lock(&shared.slot);
            loop {
                if slot.closed {
                    return;
                }
                if let Some(waiting) = slot.waiting.take() {
                    break waiting;
                }
                slot = shared
                    .wake
                    .wait(slot)
                    .unwrap_or_else(PoisonError::into_inner);
            }
        };
        let current = || shared.latest.load(Ordering::SeqCst) == seq;
        let mut emit = |update: U| -> bool {
            if !current() {
                return false;
            }
            let sent = sender.send((seq, Outcome::Update(update))).is_ok();
            notify();
            sent && current()
        };
        let ran = catch_unwind(AssertUnwindSafe(|| analyse(&request, &mut emit)));
        if let Err(panic) = ran {
            let why = panic
                .downcast_ref::<&str>()
                .map(|s| (*s).to_string())
                .or_else(|| panic.downcast_ref::<String>().cloned())
                .unwrap_or_else(|| "no message".into());
            if current() {
                let _ = sender.send((seq, Outcome::Failed(why)));
                notify();
            }
        }
    }
}

fn lock<T>(mutex: &Mutex<T>) -> MutexGuard<'_, T> {
    mutex.lock().unwrap_or_else(PoisonError::into_inner)
}
