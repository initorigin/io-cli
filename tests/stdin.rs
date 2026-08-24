//! The stdin lock, and the one property that makes it usable: a reader running
//! flat out must not starve the caller placing a viewport.
//!
//! This is here rather than in `src/main.rs` because no integration test links a
//! binary — 0.4.0 paid for that once and 0.13.1 pays for it again. The reader
//! below takes the lock through the same function the binary's keyboard thread
//! takes it through, so removing the stand-aside from that function is a sabotage
//! this file feels.
//!
//! **No clock and no sleep appear here, and none may:** `tests/timing.rs` (N1)
//! forbids both in every test in this repository, and it is right to. The first
//! draft of this file asserted a hand-over latency, and the assertion could not
//! fail — with the stand-aside removed, a placement was still served in under a
//! microsecond. Two threads of one process contending on a macOS mutex hand over
//! cleanly; what starved the placement in the field was a reader parked inside
//! `crossterm::event::poll`'s kqueue wait, which nothing in this process can
//! stand in for. So what is asserted here is the *decision* — who takes the lock
//! and who declines — sequenced with flags rather than with time, and the latency
//! is measured where it is real: the pty capture of the running binary in
//! `evidence/0.13.1/`, where putting the stand-aside back brings the freeze
//! straight back with it.
//!
//! The reader also does not poll a terminal here. `crossterm::event::poll` on a
//! `cargo test` process's stdin — a pipe, or `/dev/null` — never comes back, so a
//! test that called the real one would hang rather than assert.

use std::sync::atomic::{AtomicBool, AtomicU8, AtomicUsize, Ordering};
use std::sync::Arc;
use std::thread;

/// One terminal, one lock, one test at a time.
///
/// These tests are about who holds a process-wide lock, and `cargo test` runs
/// them on threads of one process — so two at once are two readers and two
/// placements contending for the thing under test.
static SERIAL: std::sync::Mutex<()> = std::sync::Mutex::new(());

fn serially() -> std::sync::MutexGuard<'static, ()> {
    SERIAL
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
}

/// Spin until `ready` says so.
///
/// A busy wait rather than a sleep, because N1 forbids the sleep and because
/// what is being waited for is a flag another thread sets in a handful of
/// instructions.
fn until(ready: impl Fn() -> bool) {
    while !ready() {
        std::hint::spin_loop();
    }
}

/// What a reader did when it asked for a terminal a placement was using.
const RUNNING: u8 = 0;
const DECLINED: u8 = 1;
const TOOK_IT: u8 = 2;

/// Enough spinning that a thread with something to do has been scheduled.
///
/// A count rather than a duration, because N1 forbids the clock — and because
/// what is being waited for is one thread being run at all, not an interval. It
/// is generous by two orders of magnitude on any machine this suite runs on.
const LONG_ENOUGH: u32 = 50_000_000;

#[test]
fn the_reader_stands_aside_while_a_placement_has_the_terminal() {
    let _serial = serially();

    // The whole mechanism, and it is a decision rather than a duration: while a
    // placement wants the terminal — from the moment it asks until the moment it
    // is finished — a reader declines instead of queueing on the lock. A reader
    // that queued would hold the terminal again the instant the placement let go,
    // and the next placement would wait behind it. That is the starvation, and on
    // the running binary it was 5.7 seconds of a session that answered nothing.
    //
    // **The placement is released before the reader is joined**, and that
    // ordering is the whole reason this test can fail rather than hang. A reader
    // with the stand-aside removed is blocked on the mutex this thread is
    // holding, so a join here would deadlock and the suite would time out with
    // nothing to say. Instead the answer is read off a flag while the terminal is
    // still held, and the assertion is made after everything has been let go.
    let answered = Arc::new(AtomicU8::new(RUNNING));
    let reader = {
        let told = Arc::clone(&answered);
        let held = io_cli::stdin::placing();
        assert!(
            io_cli::stdin::placement_waiting(),
            "a placement holding the terminal does not say so, so no reader can \
             stand aside for it",
        );

        let reader = thread::spawn(move || {
            let took = io_cli::stdin::reading(|| ());
            told.store(
                if took.is_some() { TOOK_IT } else { DECLINED },
                Ordering::SeqCst,
            );
        });
        for _ in 0..LONG_ENOUGH {
            if answered.load(Ordering::SeqCst) != RUNNING {
                break;
            }
            std::hint::spin_loop();
        }
        drop(held);
        reader
    };
    reader.join().expect("the reader thread does not panic");

    assert_eq!(
        answered.load(Ordering::SeqCst),
        DECLINED,
        "a reader took, or queued for, a terminal a placement was using",
    );
}

#[test]
fn a_placement_waits_out_the_reader_turn_already_in_flight() {
    let _serial = serially();

    // The other half, and it is what keeps the cursor reply safe: a placement
    // never lands inside a turn that has already started. The reader's poll and
    // its read are inside one such turn, so nothing can take the answer to the
    // placement's own query out from between them.
    //
    // Asserted without a clock: the reader marks the end of its own turn from
    // inside the critical section, so a placement that acquired while the turn
    // was still running would see the mark unset. Mutual exclusion is the whole
    // claim, and a duration would only have been a proxy for it.
    let started = Arc::new(AtomicBool::new(false));
    let finished = Arc::new(AtomicBool::new(false));
    let (opened, done) = (Arc::clone(&started), Arc::clone(&finished));
    let reader = thread::spawn(move || {
        io_cli::stdin::reading(|| {
            opened.store(true, Ordering::SeqCst);
            // Work, standing in for a poll: enough instructions that a placement
            // racing for the lock has somewhere to lose.
            for _ in 0..100_000 {
                std::hint::spin_loop();
            }
            done.store(true, Ordering::SeqCst);
        })
    });
    until(|| started.load(Ordering::SeqCst));

    let held = io_cli::stdin::placing();
    let turn_was_over = finished.load(Ordering::SeqCst);
    drop(held);
    reader
        .join()
        .expect("the reader thread does not panic")
        .expect("the reader took the terminal, since nothing was placing yet");

    assert!(
        turn_was_over,
        "a placement took the terminal while a reader's turn was still running — \
         the two are no longer mutually excluded, and a cursor reply can be \
         swallowed between the reader's poll and its read",
    );
}

#[test]
fn a_reader_that_stood_aside_gets_the_terminal_back_afterwards() {
    let _serial = serially();

    // The positive the two assertions above are paired with. "The reader
    // declines" passes just as happily against a reader that declines forever,
    // and a keyboard that stopped reading would be a worse defect than the one
    // this release fixes.
    let turns = Arc::new(AtomicUsize::new(0));
    let counted = Arc::clone(&turns);
    let stop = Arc::new(AtomicBool::new(false));
    let flag = Arc::clone(&stop);
    let reader = thread::spawn(move || {
        while !flag.load(Ordering::SeqCst) {
            if io_cli::stdin::reading(|| ()).is_some() {
                counted.fetch_add(1, Ordering::SeqCst);
            }
        }
    });

    for _ in 0..10 {
        let held = io_cli::stdin::placing();
        drop(held);
    }
    until(|| turns.load(Ordering::SeqCst) > 0);
    stop.store(true, Ordering::SeqCst);
    reader.join().expect("the reader thread does not panic");

    assert!(
        turns.load(Ordering::SeqCst) > 0,
        "the reader never took the terminal at all",
    );
}
