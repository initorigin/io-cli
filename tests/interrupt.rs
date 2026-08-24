//! F6 — an interrupt stops the turn and keeps the session.
//!
//! Asserted at the seam rather than against a live provider: `Ctrl+C` during a
//! running turn must reach `Steer::interrupt`, the partial output must be
//! committed rather than lost with the turn, and the composer must take the next
//! prompt. A real interrupted turn against a real model is F1's job.

mod support;

use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use io_cli::app::{App, Command, Mode};
use io_cli::theme::DARK;
use io_harness::{EventKind, RunEvent, Steer};

fn key(code: KeyCode) -> KeyEvent {
    KeyEvent::new(code, KeyModifiers::NONE)
}

fn control(character: char) -> KeyEvent {
    KeyEvent::new(KeyCode::Char(character), KeyModifiers::CONTROL)
}

fn type_text(app: &mut App, text: &str) {
    for character in text.chars() {
        app.key(key(KeyCode::Char(character)));
    }
}

fn text_of(lines: &[ratatui::text::Line<'_>]) -> String {
    lines
        .iter()
        .flat_map(|line| line.spans.iter())
        .map(|span| span.content.as_ref())
        .collect::<Vec<_>>()
        .join("")
}

#[test]
fn f6_ctrl_c_during_a_turn_interrupts_it_and_keeps_the_partial_output() {
    let (steer, inbox) = Steer::channel();
    let mut app = App::new(DARK, "anthropic/claude-sonnet-4");

    // A prompt starts a turn.
    type_text(&mut app, "refactor the parser");
    assert_eq!(
        app.key(key(KeyCode::Enter)),
        Command::Submit("refactor the parser".into()),
    );
    app.started();
    assert_eq!(app.mode(), Mode::Running);

    // The model streams some of an answer.
    for word in ["Looking ", "at ", "the ", "parser"] {
        app.event(
            &RunEvent::new(1, 1, EventKind::Token { text: word.into() }),
            std::time::Duration::ZERO,
        );
    }
    assert_eq!(app.events.live(), "Looking at the parser");

    // Ctrl+C.
    assert_eq!(app.key(control('c')), Command::Interrupt);
    steer.interrupt().expect("the turn is still listening");

    let (messages, interrupted) = inbox.pending();
    assert!(interrupted, "Steer::interrupt never reached the inbox");
    assert!(messages.is_empty(), "an interrupt is not a message");

    // The turn ends. The partial output is committed rather than lost with it.
    app.finished();
    let committed = text_of(&app.take_pending());
    assert!(
        committed.contains("Looking at the parser"),
        "the partial answer was lost when the turn was interrupted: {committed:?}",
    );
    assert_eq!(app.mode(), Mode::Idle);
    assert_eq!(app.events.live(), "");

    // And the composer takes the next prompt.
    type_text(&mut app, "try again");
    assert_eq!(
        app.key(key(KeyCode::Enter)),
        Command::Submit("try again".into()),
        "the session did not survive the interrupt",
    );
}

#[test]
fn f6_ctrl_c_twice_at_an_idle_composer_exits() {
    let mut app = App::new(DARK, "m");
    assert_eq!(app.key(control('c')), Command::None, "the first one warns");
    // In the footer since 0.13.1: it answers the key just pressed and is gone at
    // the next one, rather than living in the scrollback for the session's life.
    let warning = app
        .status
        .notice
        .as_ref()
        .map(|(_, text)| text.clone())
        .unwrap_or_default();
    assert!(
        warning.contains("again"),
        "the first Ctrl+C should say what the second one does: {warning:?}",
    );
    assert_eq!(app.key(control('c')), Command::Exit);
}

#[test]
fn ctrl_c_with_something_typed_clears_it_rather_than_exiting() {
    let mut app = App::new(DARK, "m");
    type_text(&mut app, "half a thought");

    assert_eq!(app.key(control('c')), Command::None);
    assert!(
        app.composer.is_empty(),
        "the composer should have been cleared"
    );
    // ...and that did not count towards the exit, so a stray Ctrl+C cannot end
    // the session by surprise.
    assert_eq!(app.key(control('c')), Command::None);
    assert_eq!(app.key(control('c')), Command::Exit);
}

#[test]
fn ctrl_d_exits_only_on_an_empty_composer() {
    let mut app = App::new(DARK, "m");
    type_text(&mut app, "not finished yet");
    assert_eq!(
        app.key(control('d')),
        Command::None,
        "Ctrl+D must not discard what was typed",
    );

    app.composer.clear();
    assert_eq!(app.key(control('d')), Command::Exit);
}

#[test]
fn ctrl_l_clears_the_viewport_and_says_so() {
    let mut app = App::new(DARK, "m");
    assert_eq!(app.key(control('l')), Command::ClearViewport);
}

#[test]
fn a_prompt_beginning_with_a_slash_is_a_command() {
    let mut app = App::new(DARK, "m");
    type_text(&mut app, "/theme");
    assert_eq!(app.key(key(KeyCode::Enter)), Command::Slash("theme".into()));

    type_text(&mut app, "/model gpt-5");
    assert_eq!(
        app.key(key(KeyCode::Enter)),
        Command::Slash("model gpt-5".into()),
    );
}

#[test]
fn the_observer_hands_events_on_without_blocking_the_run() {
    use io_harness::Observer;

    let (bridge, mut receiver) = io_cli::bridge::channel();
    for step in 0..1000 {
        let flow = bridge.event(&RunEvent::new(1, step, EventKind::Stalled));
        assert_eq!(
            flow,
            io_harness::Flow::Continue,
            "the observer must not cancel the run it is watching",
        );
    }

    let mut seen = 0;
    while receiver.try_recv().is_ok() {
        seen += 1;
    }
    assert_eq!(
        seen, 1000,
        "an event was dropped between the run and the screen"
    );
}

#[test]
fn a_dropped_interface_does_not_cancel_the_run() {
    use io_harness::Observer;

    let (bridge, receiver) = io_cli::bridge::channel();
    drop(receiver);
    assert_eq!(
        bridge.event(&RunEvent::new(1, 1, EventKind::Stalled)),
        io_harness::Flow::Continue,
        "a send failure must not stop the agent loop",
    );
}

#[test]
fn f9_a_streaming_answer_of_any_length_leaves_the_viewport_the_same_size() {
    // The viewport is a fixed few rows and stays that way however much streams,
    // because each line commits to the terminal's own scrollback as it finishes.
    // Only the unfinished tail is ever live, so there is nothing for the viewport
    // to grow to hold.
    let mut app = App::new(DARK, "m");
    let quiet = app.viewport_height();
    assert!(quiet >= 3, "streaming tail, composer and status line");

    let mut committed = 0;
    for index in 0..200 {
        committed += app_lines(&mut app, &format!("line {index} of an answer\n"));
    }
    assert_eq!(
        committed, 200,
        "every finished line should have been committed as it arrived",
    );
    assert_eq!(app.viewport_height(), quiet, "the viewport did not move");

    // A partial line stays live until something finishes it.
    app_lines(&mut app, "the tail with no newline yet");
    assert_eq!(app.events.live(), "the tail with no newline yet");
    assert_eq!(app.viewport_height(), quiet);

    app.finished();
    assert_eq!(app.events.live(), "");
    let tail = text_of(&app.take_pending());
    assert!(tail.contains("the tail with no newline yet"), "{tail:?}");
}

/// Feed one token and report how many lines it committed.
fn app_lines(app: &mut App, text: &str) -> usize {
    let before = app.take_pending().len();
    assert_eq!(before, 0, "the caller should have drained first");
    app.event(
        &RunEvent::new(1, 1, EventKind::Token { text: text.into() }),
        std::time::Duration::ZERO,
    );
    app.take_pending().len()
}
