//! N2 — the terminal is always restored.
//!
//! A panic inside a turn must put the terminal back into cooked mode with the
//! cursor visible *before* anything is printed, or the panic message itself lands
//! in raw mode and the user is left with a terminal that no longer echoes.
//!
//! The signal case is a manual check; it cannot be asserted here because the test
//! process is the one that would receive the signal.
//!
//! N5 grew a second half in 0.6.0: the restore now also pops the Kitty keyboard
//! protocol where io-cli pushed it. That half is asserted over the byte stream in
//! `tests/keyboard.rs`, because a keyboard mode has no other observable. What is
//! asserted here is the property the pop rides on — that the restore happens at
//! all, before the panic is reported, and exactly once.

mod support;

use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;

use io_cli::term;

#[test]
fn n2_a_panic_runs_the_restore_before_the_previous_hook() {
    // Two counters rather than one flag, so the test can assert the *order*: the
    // restore has to have happened by the time the reporting hook runs.
    let order = Arc::new(AtomicUsize::new(0));
    let restored_at = Arc::new(AtomicUsize::new(0));
    let reported_at = Arc::new(AtomicUsize::new(0));

    {
        let order = Arc::clone(&order);
        let reported_at = Arc::clone(&reported_at);
        std::panic::set_hook(Box::new(move |_| {
            reported_at.store(order.fetch_add(1, Ordering::SeqCst) + 1, Ordering::SeqCst);
        }));
    }

    {
        let order = Arc::clone(&order);
        let restored_at = Arc::clone(&restored_at);
        term::install_panic_hook(move || {
            restored_at.store(order.fetch_add(1, Ordering::SeqCst) + 1, Ordering::SeqCst);
        });
    }

    let panicked = std::panic::catch_unwind(|| panic!("a turn went wrong"));
    assert!(panicked.is_err(), "the test needs the panic to happen");

    let restored = restored_at.load(Ordering::SeqCst);
    let reported = reported_at.load(Ordering::SeqCst);
    assert!(restored > 0, "the restore never ran on a panic");
    assert!(
        reported > 0,
        "the previous hook was swallowed rather than chained"
    );
    assert!(
        restored < reported,
        "the terminal was restored at step {restored} but the panic was reported at step \
         {reported}; the message would have been printed into a raw-mode terminal",
    );

    let _ = std::panic::take_hook();
}

#[test]
fn dropping_the_screen_restores_it_once() {
    let calls = Arc::new(AtomicUsize::new(0));
    let (mut screen, _recorder) = support::screen(100, 30);
    {
        let calls = Arc::clone(&calls);
        screen.on_restore(move || {
            calls.fetch_add(1, Ordering::SeqCst);
        });
    }

    screen.restore();
    // A `Drop` that restores a second time would re-enable the cursor over a
    // terminal some other process may already own.
    drop(screen);

    assert_eq!(
        calls.load(Ordering::SeqCst),
        1,
        "restore ran more than once"
    );
}
