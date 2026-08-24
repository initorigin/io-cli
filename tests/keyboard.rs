//! F7 — what is pushed is popped, including through a panic.
//! F8 — `Shift+Enter` inserts a newline where the protocol is negotiated, and the
//! backslash keeps working everywhere.
//! N5 — the terminal is restored on every exit path, now including the keyboard
//! protocol.
//! F9 (0.13.0) — and io *names* the spelling that works here, the same way on
//! every surface that names one.
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
//!
//! F9 is the same split one layer up. `io_cli::keys::Newline::of` takes the same
//! `bool` and answers what the surfaces that *name* the key render, so the two
//! terminals are both reachable from a machine that is only one of them — and
//! what is asserted is the rendered rows and the rendered screen, never the
//! source, because a table nobody drew is a table nobody can be misled by.

mod support;

use std::sync::{Mutex, MutexGuard, PoisonError};

use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use io_cli::commands::{self, KEYS};
use io_cli::composer::{Composer, Reply};
use io_cli::keys::{Keys, Newline};
use io_cli::term;
use io_cli::theme::DARK;
use io_cli::wizard::Wizard;
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

/// The one row of a rendered key reference that is about the newline, found by
/// what it *does* rather than by the key it names — the key it names is the
/// answer under test, so a lookup on it would find whatever it was looking for.
///
/// It also asserts there is exactly one. Two rows about the newline is the shape
/// a half-done substitution takes: the shipped row left in place beside a
/// corrected one, which is a table that names both keys and recommends neither.
fn newline_row(rows: &[(String, String)]) -> (&str, &str) {
    let mut about: Vec<&(String, String)> = rows
        .iter()
        .filter(|(_, what)| what.starts_with("new line"))
        .collect();
    assert_eq!(
        about.len(),
        1,
        "the key reference has {} rows about the newline: {rows:?}",
        about.len(),
    );
    let row = about.remove(0);
    (row.0.as_str(), row.1.as_str())
}

/// Every line of a rendered screen as one string, so an assertion is about what
/// the operator can read rather than about which span it landed in.
fn text(lines: &[ratatui::text::Line<'_>]) -> String {
    lines
        .iter()
        .map(|line| {
            line.spans
                .iter()
                .map(|span| span.content.as_ref())
                .collect::<String>()
        })
        .collect::<Vec<_>>()
        .join("\n")
}

/// **F9.** The key reference names the chord this terminal can actually send.
///
/// The failure it exists to stop is quiet and looks like a product bug: on a
/// terminal without the protocol, `Enter` and `Shift+Enter` are the same byte, so
/// a row that says `Shift+Enter` — new line is naming the key that *sends* the
/// half-written prompt. Both arms are asserted on the rendered rows, because the
/// row is the artifact and `KEYS` is only its first draft.
#[test]
fn f9_the_key_reference_names_the_key_this_terminal_can_report() {
    // Nothing here touches the keyboard push, but `f7_a_panic_pops_it_too`
    // replaces the process's panic hook with a silent one while it runs — and a
    // failing assertion is a panic, so a test overlapping it loses the message
    // that says what broke.
    let _serial = serially();
    let keys = Keys::default();

    let advertised = commands::rows(&keys, Newline::of(true));
    let (key, what) = newline_row(&advertised);
    assert_eq!(
        key, "Shift+Enter",
        "a terminal that reports it should be told to use it: {advertised:?}",
    );
    for spelling in ["Alt+Enter", "Ctrl+J", "\\"] {
        assert!(
            what.contains(spelling),
            "the other spellings still work and the row still lists them: {what:?}",
        );
    }

    let unreportable = commands::rows(&keys, Newline::of(false));
    let (key, what) = newline_row(&unreportable);
    assert_eq!(
        key, "Alt+Enter",
        "on a terminal that cannot report Shift+Enter the reference has to lead \
         with a chord that arrives: {unreportable:?}",
    );
    assert!(
        what.contains('\\'),
        "the trailing backslash is the spelling every terminal has, and it is the \
         one this reader is left with: {what:?}",
    );
    assert!(
        what.contains("cannot report") && what.contains("Shift+Enter"),
        "Shift+Enter is *said* rather than dropped in silence: a reader who has \
         seen it in the README and cannot find it here goes and presses it to \
         find out why, which is the keystroke this row exists to prevent: {what:?}",
    );
    assert!(
        !unreportable.iter().any(|(key, _)| key == "Shift+Enter"),
        "no first column may offer a chord that submits the prompt here: \
         {unreportable:?}",
    );

    assert_eq!(
        advertised.len(),
        unreportable.len(),
        "the naming changes what a row says, never how many rows there are",
    );
}

/// **F9's join.** The shipped table is the advertised naming, word for word.
///
/// `KEYS` is what the README prints, and a README is read on a machine other than
/// the one it describes — so it names the key this product prefers and
/// `Newline::of` corrects it for the terminal in front of the operator. That
/// makes the pair a join on two strings written in two files, which is exactly
/// the shape that rots in silence: reword one and `/help` would substitute a row
/// that was never there, leaving the shipped row on screen beside it.
#[test]
fn f9_the_shipped_row_is_the_advertised_naming() {
    let _serial = serially();
    let shipped = Newline::of(true);
    assert!(
        KEYS.iter()
            .any(|(key, what)| *key == shipped.key && *what == shipped.what),
        "no row of KEYS is ({:?}, {:?}): {KEYS:?}",
        shipped.key,
        shipped.what,
    );
}

/// **F9, and the failure the named sabotage causes.** Two surfaces cannot name
/// two different keys in one session.
///
/// The sabotage is to let each surface work the answer out for itself at the
/// point it draws. Nothing about it looks wrong in a diff and every surface still
/// renders — what it costs is agreement, and agreement is the whole property: an
/// operator told `Shift+Enter` by the wizard and `Alt+Enter` by `/help` has been
/// given two keys, one of which sends the prompt, and no way to know which
/// terminal each screen was talking about.
///
/// So both surfaces are handed the *same* value and asserted to render it. Under
/// the sabotage the argument is ignored, both arms come back with whatever the
/// test machine's environment says, and the `true` arm fails on the first screen
/// it reaches.
#[test]
fn f9_two_surfaces_cannot_name_two_different_keys_in_one_session() {
    let _serial = serially();
    for advertised in [true, false] {
        let newline = Newline::of(advertised);

        let rows = commands::rows(&Keys::default(), newline);
        let (key, _) = newline_row(&rows);
        assert_eq!(
            key, newline.key,
            "the key reference rendered a key it was not given (advertised: \
             {advertised})",
        );

        // The wizard's closing screen, at a width that fits it. Nothing else on
        // the screen has been chosen — the naming does not depend on the provider
        // or the model, and this test is about the key and only the key.
        let closing = text(&Wizard::new(DARK).summary_at(80, newline));
        assert!(
            closing.contains(newline.key),
            "the wizard's closing screen names a different key from the one \
             /help just named (advertised: {advertised}): {closing:?}",
        );
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
