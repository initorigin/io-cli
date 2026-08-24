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

use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use io_cli::app::{App, Command};
use io_cli::theme::{Tone, DARK};
use io_harness::{EventKind, RunEvent};

fn key(code: KeyCode) -> KeyEvent {
    KeyEvent::new(code, KeyModifiers::NONE)
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
