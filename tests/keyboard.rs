//! F7 — what is pushed is popped, including through a panic.
//! F8 — `Shift+Enter` inserts a newline where the protocol is negotiated, and the
//! backslash keeps working everywhere.
//! N5 — the terminal is restored on every exit path, now including the keyboard
//! protocol.
//!
//! io-cli negotiates the Kitty keyboard protocol *up* on terminals that advertise
//! it, which is a mode change to something this process does not own. Whatever it
//! pushes it has to pop, on every way out — and the way out that a manual test
//! never takes is the panic, which is why the panic is asserted here rather than
//! left to the eye.
//!
//! None of this can be driven through a real terminal:
//! `supports_keyboard_enhancement` writes a query to the tty and waits for a
//! reply, and under `cargo test` there is nothing on the other end. So the
//! decision — *given* that the terminal advertises it, push or do not — is a pure
//! function taking a `bool`, and these tests drive it both ways. What is asserted
//! is the byte stream that leaves the process, because a keyboard mode has no
//! other observable.

mod support;

use std::sync::{Mutex, MutexGuard, PoisonError};

use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use io_cli::composer::{Composer, Reply};
use io_cli::term;
use support::Recorder;

/// Whether a push is outstanding is process-wide state, and it has to be: the
/// thing that pops it is a panic hook, which runs with no `Screen` in reach.
/// Tests in one binary run in parallel threads, so every test here takes this
/// first. Two overlapping tests would otherwise pop into each other's recorder
/// and both be right about the wrong stream — and the panic test replaces the
/// process's panic hook while it runs, which is how a *different* test's failure
/// would lose the message that says what it was.
static SERIAL: Mutex<()> = Mutex::new(());

fn serially() -> MutexGuard<'static, ()> {
    // A failing assertion in one of these poisons the lock, and a poisoned lock
    // would turn one real failure into three confusing ones.
    SERIAL.lock().unwrap_or_else(PoisonError::into_inner)
}

fn type_text(composer: &mut Composer, text: &str) {
    for character in text.chars() {
        composer.key(KeyEvent::new(KeyCode::Char(character), KeyModifiers::NONE));
    }
}

/// **F7, and N5's keyboard half.** The ordinary exit: what attaching pushed, the
/// restore pops — and a second restore pops nothing.
#[test]
fn f7_a_clean_exit_pops_what_attaching_pushed() {
    let _serial = serially();
    let (mut screen, recorder) = support::screen(80, 24);

    term::negotiate_keyboard(screen.terminal_mut().backend_mut(), true).expect("the push");
    {
        // `Screen::attach` restores through `restore_terminal`, which writes to
        // the real stdout. This is the same function it delegates to, aimed at
        // somewhere a test can read it.
        let out = recorder.clone();
        screen.on_restore(move || {
            // Cloned again *inside*: the restore closure is an `Fn`, so it may
            // only borrow what it captured immutably, and writing needs `&mut`.
            // A `Recorder` is a handle to an `Arc<Mutex<Vec<u8>>>` rather than a
            // buffer of its own, so the clone writes into the same bytes this
            // test reads back — and being a handle is also what makes the
            // capture `Send + Sync + 'static`, which the hook demands.
            let _ = term::restore_into(&mut out.clone());
        });
    }
    screen.draw(|_| {}).expect("a frame");

    // `Drop` is the exit `main` takes when it returns.
    drop(screen);

    assert_eq!(
        support::keyboard_balance(&recorder),
        (1, 1),
        "the byte stream must carry one pop for the one push io-cli made",
    );

    // Restoring twice is the ordinary case, not the exotic one: `Screen::restore`
    // and then `Drop`. A pop written for a push that is no longer outstanding
    // would pop a level of somebody else's, in a shell this process has already
    // given back.
    let mut again = recorder.clone();
    let _ = term::restore_into(&mut again);
    assert_eq!(
        support::keyboard_balance(&recorder),
        (1, 1),
        "a second restore popped again",
    );
}

/// **F7, on the path a manual test never takes.** A panic pops it too, and it
/// does so before the panic is reported — the terminal the message lands in is
/// already the user's.
#[test]
fn f7_a_panic_pops_it_too() {
    let _serial = serially();
    let recorder = Recorder::new();
    let mut out = recorder.clone();

    term::negotiate_keyboard(&mut out, true).expect("the push");

    // Set first so the chained hook has something quiet to chain onto: the
    // default hook prints a panic message this test causes on purpose.
    std::panic::set_hook(Box::new(|_| {}));
    {
        let restored = recorder.clone();
        term::install_panic_hook(move || {
            // Same handle, same reason as above: `Restore` is a
            // `Box<dyn Fn() + Send + Sync + 'static>`, so nothing captured here
            // can be borrowed mutably, and the pop still lands in the one buffer
            // the assertion reads.
            let _ = term::restore_into(&mut restored.clone());
        });
    }

    let panicked = std::panic::catch_unwind(|| panic!("a turn went wrong"));
    assert!(panicked.is_err(), "the test needs the panic to happen");

    assert_eq!(
        support::keyboard_balance(&recorder),
        (1, 1),
        "the protocol was still pushed when the process unwound; it would have \
         outlived io-cli and been inherited by the shell",
    );

    let _ = std::panic::take_hook();
}

/// **F7's other half.** A terminal that does not advertise the protocol is never
/// spoken to in it — not the push, and not a pop that would arrive as text on a
/// terminal with nothing to pop.
#[test]
fn f7_a_terminal_that_does_not_advertise_it_sees_neither_sequence() {
    let _serial = serially();
    let mut recorder = Recorder::new();

    term::negotiate_keyboard(&mut recorder, false).expect("nothing to push");
    let _ = term::restore_into(&mut recorder);

    assert_eq!(
        support::keyboard_balance(&recorder),
        (0, 0),
        "neither sequence belongs on a terminal that never advertised the protocol",
    );
}

/// **F8.** Both spellings of a newline put one in, and neither breaks submitting.
///
/// `Shift+Enter` is the spelling a terminal can only send once the protocol is
/// negotiated. The trailing backslash is the spelling every other terminal has,
/// and it is the one an ssh session, a tmux without `extended-keys` and a plain
/// xterm are left with — which is why it is asserted beside its replacement
/// rather than retired behind it. Its own test is `tests/composer.rs`, unchanged.
#[test]
fn f8_both_spellings_of_a_newline_insert_one() {
    let _serial = serially();
    let mut composer = Composer::new();

    type_text(&mut composer, "first");
    composer.key(KeyEvent::new(KeyCode::Enter, KeyModifiers::SHIFT));
    type_text(&mut composer, "second\\");
    composer.key(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE));
    type_text(&mut composer, "third");

    assert_eq!(
        composer.text(),
        "first\nsecond\nthird",
        "both spellings insert a newline, and the backslash is consumed rather \
         than left in the prompt",
    );
    assert_eq!(
        composer.key(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE)),
        Reply::Submitted("first\nsecond\nthird".into()),
        "a plain Enter on a line ending in neither still submits the whole prompt",
    );
}
