//! What io-cli says about the session, and what it puts in the record.
//!
//! Two different things, and through 0.13.0 they were the same thing. Stopping
//! one turn committed three rows into the terminal's permanent scrollback —
//! `stopping at the next step boundary`, `stopping now`, `stopped` — in warning
//! colour, sitting between two answers for as long as the terminal lived. None
//! of them is part of the conversation: each answered a key that had just been
//! pressed.
//!
//! So a notice lives in the footer, replaces the one before it, and is gone at
//! the next keystroke. What still reaches the transcript is what belongs to the
//! record: what the agent said, what was authorised, and why a turn failed.

mod support;

use std::time::Duration;

use std::sync::Arc;

use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use io_cli::app::{App, Command};
use io_cli::settings;
use io_cli::theme::{Tone, DARK};
use io_harness::{Config, EventKind, RunEvent};

fn key(code: KeyCode) -> KeyEvent {
    KeyEvent::new(code, KeyModifiers::NONE)
}

/// The text of everything committed into the scrollback, one row per line.
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

/// What a session start makes of a file, read the way `src/main.rs` reads one.
///
/// `Config::from_toml` and not `Config::discover`: nothing here depends on a
/// section discovery populates, and a fixture that touches no filesystem and no
/// environment variable is one that cannot race another test.
fn startup_notice(toml: &str) -> Option<String> {
    let config = Config::from_toml(toml).expect("the fixture file parses");
    let (_, complaint) = settings::stored(&config);
    assert_eq!(complaint, None, "the fixture section reads");
    // The RAW section, since 0.16.0 removed the typed field. A file still
    // carrying the key LOADS — `CliSettings` has no `deny_unknown_fields` — which
    // is exactly why the notice has to exist.
    settings::deprecated_max_steps(&config)
}

fn notice(app: &App) -> String {
    app.status
        .notice
        .as_ref()
        .map(|(_, text)| text.clone())
        .unwrap_or_default()
}

/// A turn with the operator's prompt echoed and nothing else yet.
fn just_started(app: &mut App, goal: &str) {
    app.started();
    app.event(
        &RunEvent::new(
            1,
            1,
            EventKind::Started {
                goal: goal.into(),
                provider: "openrouter".into(),
            },
        ),
        Duration::from_secs(0),
    );
    // The driver commits what is pending on the next paint, which is what counts
    // the rows the echo took.
    let committed = app.take_pending();
    assert!(!committed.is_empty(), "the goal line is committed");
}

#[test]
fn a_notice_goes_to_the_footer_and_never_to_the_transcript() {
    let mut app = App::new(DARK, "opus-5");
    app.say(Tone::Muted, "press Ctrl+C again to exit");

    assert_eq!(notice(&app), "press Ctrl+C again to exit");
    assert!(
        app.take_pending().is_empty(),
        "a notice is not part of the conversation and does not go in it",
    );
}

#[test]
fn a_record_goes_to_the_transcript_and_never_to_the_footer() {
    let mut app = App::new(DARK, "opus-5");
    app.record(Tone::Error, "the provider refused");

    assert_eq!(notice(&app), "");
    let committed = app.take_pending();
    assert_eq!(committed.len(), 1, "{committed:?}");
}

#[test]
fn the_next_keystroke_takes_the_notice_off() {
    let mut app = App::new(DARK, "opus-5");
    app.say(Tone::Muted, "press Ctrl+C again to exit");

    app.key(key(KeyCode::Char('a')));
    assert_eq!(
        notice(&app),
        "",
        "a notice answers one keystroke and is gone at the next",
    );
}

/// **The turn an operator stops a moment after sending it.** No step, nothing
/// streamed, nothing on screen but the echo — so it is taken back whole rather
/// than stopped: the first press abandons, the rows come off the screen, the
/// prompt goes back in the composer, and nothing is said at all.
#[test]
fn an_early_stop_undoes_the_turn_instead_of_reporting_it() {
    let mut app = App::new(DARK, "opus-5");
    just_started(&mut app, "count the tests");

    assert!(
        app.undoable(),
        "a turn with only its echo on screen is undoable"
    );
    assert_eq!(
        app.key(key(KeyCode::Esc)),
        Command::Abandon,
        "the first press stops it, with no boundary to wait for",
    );
    assert_eq!(notice(&app), "", "nothing to say about a turn nobody saw");

    let (rows, prompt) = app.undo_turn();
    assert!(rows > 0, "the echo took rows and they come back off");
    assert_eq!(prompt, "count the tests");
    assert_eq!(
        app.composer.text(),
        "count the tests",
        "the prompt is back in the composer, ready to edit or send again",
    );
    assert!(
        app.take_pending().is_empty(),
        "and nothing is left to commit"
    );
}

/// A multi-line prompt is more rows of echo, and all of them come back off.
#[test]
fn an_undone_turn_counts_every_row_its_echo_took() {
    let mut app = App::new(DARK, "opus-5");
    just_started(&mut app, "one\ntwo\nthree");

    let (rows, prompt) = app.undo_turn();
    assert!(
        rows >= 3,
        "three lines of prompt are at least three rows on screen: {rows}",
    );
    assert_eq!(prompt, "one\ntwo\nthree");
}

/// Past the first step there is work worth keeping, so the ordinary stop
/// applies: one sentence, in the footer, and the second press ends it.
#[test]
fn a_turn_with_work_in_it_stops_rather_than_disappearing() {
    let mut app = App::new(DARK, "opus-5");
    just_started(&mut app, "count the tests");
    app.status.steps = Some(2);

    assert!(!app.undoable());
    assert_eq!(app.key(key(KeyCode::Esc)), Command::Interrupt);
    let said = notice(&app);
    assert!(said.contains("stopping"), "{said:?}");
    assert!(said.contains("esc again"), "{said:?}");
    assert!(
        app.take_pending().is_empty(),
        "and it says so in the footer rather than in the transcript",
    );

    assert_eq!(
        app.key(key(KeyCode::Esc)),
        Command::Abandon,
        "the second press does not wait for a boundary",
    );
}

/// **One sentence for one decision.** Three rows for one stop is what this
/// replaced: `stopping at the next step boundary`, then `stopping now`, then
/// `stopped`.
#[test]
fn stopping_a_turn_says_one_thing_and_says_it_once() {
    let mut app = App::new(DARK, "opus-5");
    just_started(&mut app, "count the tests");
    app.status.steps = Some(2);

    app.key(key(KeyCode::Esc));
    let first = notice(&app);
    app.key(key(KeyCode::Esc));

    assert_ne!(first, "", "the first press says where it will stop");
    assert!(
        app.take_pending().is_empty(),
        "and neither press writes a row into the conversation",
    );
}

/// **F12 — a file that still writes `[app.io-cli] max_steps` is told all three
/// things.** The key it used, the number that is NOT in force — the key was
/// removed in 0.16.0, so the number it quotes is the one the turn is no longer
/// running on — and `[run] max_steps` as where the cap lives now. A removal that
/// names only the key leaves the operator to find the replacement themselves,
/// and one that names only the replacement leaves them unsure whether their
/// number was ever being read.
///
/// Sabotage: emit the notice whenever the key is *readable* rather than when it
/// was written — this arm still passes, and the two below it fail.
#[test]
fn a_file_that_still_writes_the_removed_step_cap_is_told_where_the_cap_lives() {
    let said = startup_notice("[app.io-cli]\ntheme = \"dark\"\nmax_steps = 40\n")
        .expect("a file carrying the removed key earns a line");

    assert!(said.contains("[app.io-cli] max_steps"), "{said:?}");
    assert!(said.contains("40"), "the value it asked for: {said:?}");
    assert!(said.contains("[run] max_steps"), "where the cap lives: {said:?}");
    assert!(said.contains("no longer read"), "that it is dead: {said:?}");
}

/// **F12 — the new spelling alone says nothing.** `[run] max_steps` is where the
/// key is going, so a file already there has nothing to migrate and nothing to
/// be told.
///
/// Sabotage: key the notice off the step cap this session ended up running under
/// rather than off the field — every session has a cap, so this arm fails.
#[test]
fn a_file_with_only_run_max_steps_is_told_nothing() {
    assert_eq!(
        startup_notice("[run]\nmax_steps = 20\n"),
        None,
        "the file uses the spelling this release is moving to",
    );
}

/// **F12 — and neither does a file that names no step cap at all**, which
/// includes the one `io setup` writes: `settings::render` leaves `max_steps` out
/// deliberately. A deprecation notice on a session that is not using the
/// deprecated key is noise, and an operator who is told at every start about a
/// key they have never used is one who stops reading the start-up lines — after
/// which the line they skip is the one that mattered.
///
/// Sabotage: emit the notice whenever `[app.io-cli]` is *present* rather than
/// when the key inside it was written — under which the first arm here fails, on
/// the default file.
#[test]
fn a_file_with_neither_spelling_is_told_nothing() {
    assert_eq!(
        startup_notice("[app.io-cli]\ntheme = \"dark\"\n"),
        None,
        "the section is there; the deprecated key in it is not",
    );
    assert_eq!(startup_notice(""), None, "and an empty file says nothing");
}

/// **F12 — the removed key no longer wins anything, and the file is told.**
///
/// Through 0.15.0 `[app.io-cli] max_steps` was applied after `Config::apply_to`
/// and therefore beat a `[run] max_steps` in the same file. 0.16.0 removes it,
/// which the 0.14.0 deprecation promised in the operator's own terminal, in the
/// README and in the CHANGELOG.
///
/// **The removal is silent by construction and that is what this test is really
/// about.** `CliSettings` carries no `#[serde(deny_unknown_fields)]`, so a file
/// still holding the key parses fine and the key is simply ignored — no error,
/// no warning, and a step cap that quietly changed. So both halves are asserted
/// together: the `[run]` value is what the turn runs on now, AND the file is
/// told its key is dead.
///
/// Sabotage: delete the notice along with the field — under which the first half
/// still passes, the contract is still right, and an operator's turns quietly
/// start ending at a different number with nothing on screen to say why.
#[test]
fn the_removed_step_cap_no_longer_wins_and_the_file_is_told() {
    let toml = "[run]\nmax_steps = 20\n\n[app.io-cli]\nmax_steps = 7\n";
    let config = Config::from_toml(toml).expect("a file with the dead key still parses");
    let (stored, complaint) = settings::stored(&config);
    assert_eq!(complaint, None, "an unknown key in [app.io-cli] is not a complaint");

    let (answerer, _questions) = io_cli::intent::channel();
    let responder: Arc<dyn io_harness::Responder> = Arc::new(answerer);
    let contract = io_cli::contract::session(
        "a goal",
        std::path::PathBuf::from("."),
        &config,
        &io_cli::contract::Capabilities::stored(stored.as_ref()),
        responder,
        None,
    );

    assert_eq!(
        contract.max_steps, 20,
        "`[run] max_steps` is the whole answer now; the removed key won nothing",
    );

    let said = startup_notice(toml).expect("a file still carrying the key is told");
    assert!(
        said.contains("no longer read"),
        "the notice must say the key is dead rather than going away: {said:?}",
    );
    assert!(
        said.contains("[run] max_steps"),
        "the notice must say where the cap lives now: {said:?}",
    );
    assert!(
        said.contains('7'),
        "the notice quotes the number that is NOT in force, which is the fact \
         the operator needs to act on: {said:?}",
    );
}

/// **Every startup notice reaches the scrollback, not just the last one.**
///
/// `App::say` writes `status.notice`, which holds one line: a session with an
/// unreadable `[app.io-cli]`, a keybinding naming no action and a deprecated step
/// cap showed the third and silently dropped the first two. These lines are not
/// answers to a keystroke — nobody has pressed anything yet, and the footer's
/// line is gone at the first key that is — so `src/main.rs` commits them.
///
/// The binary cannot be linked from here, which is why this asserts the property
/// of the two calls rather than of the loop: `record` accumulates and `say`
/// replaces, and the loop uses the one that accumulates.
///
/// Sabotage: put `say` back in that loop — which is the shipped 0.13.1 behaviour,
/// and under which only this fails.
#[test]
fn every_startup_notice_reaches_the_scrollback() {
    let startup = [
        "`[app.io-cli]` could not be read; this session is running on the defaults",
        "`ctrl+q` is not an action this session knows",
        "`[app.io-cli] max_steps` was removed in 0.16.0 and is no longer read",
    ];

    let mut app = App::new(DARK, "opus-5");
    for line in startup {
        app.record(Tone::Warning, line);
    }
    let committed = app.take_pending();
    assert_eq!(committed.len(), 3, "one row each: {committed:?}");
    let scrollback = text(&committed);
    for line in startup {
        assert!(scrollback.contains(line), "{line:?} survived");
    }

    let mut replaced = App::new(DARK, "opus-5");
    for line in startup {
        replaced.say(Tone::Warning, line);
    }
    assert!(
        replaced.take_pending().is_empty(),
        "which is what saying them instead costs",
    );
    assert_eq!(
        notice(&replaced),
        startup[2],
        "the footer holds one line, so the last sender wins and the rest are lost",
    );
}
