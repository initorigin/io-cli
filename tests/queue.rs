//! F1 — a prompt typed during a turn is kept.
//! F4 — the queue fires in order when the turn ends, one prompt per turn.
//!
//! Asserted against `App` rather than against the driver, because `src/main.rs`
//! is linked by no integration test: a guard written in the turn loop could not
//! be sabotaged and would not be covered, which is the reason the queueing
//! decision lives in `App::compose` at all. What the driver still owns is the
//! *drain* — a whole turn between two `App::next_queued_prompt` calls — and the
//! shape of that loop is asserted here the only way a test can reach it: by
//! doing what the driver does, one prompt at a time, and checking that three
//! queued lines produce three exchanges rather than one.
//!
//! Nothing here draws the queue. The surface is its own criterion; this file is
//! state, capture and drain.

use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use io_cli::app::{App, Command, Mode};
use io_cli::theme::DARK;
use io_harness::{EventKind, RunEvent};

fn key(code: KeyCode) -> KeyEvent {
    KeyEvent::new(code, KeyModifiers::NONE)
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

/// Type a line and press `Enter`, the way an operator sends one.
fn send(app: &mut App, text: &str) -> Command {
    type_text(app, text);
    app.key(key(KeyCode::Enter))
}

/// Start a turn the way the driver does: the mode, then the harness's own
/// `Started`, which is what puts the prompt's echo in the scrollback.
fn start_turn(app: &mut App, goal: &str) {
    app.started();
    app.event(
        &RunEvent::new(
            1,
            1,
            EventKind::Started {
                goal: goal.into(),
                provider: "test".into(),
            },
        ),
        std::time::Duration::ZERO,
    );
}

/// Stream one answer and end the turn.
fn answer_turn(app: &mut App, text: &str) {
    app.event(
        &RunEvent::new(
            1,
            2,
            EventKind::Token {
                text: format!("{text}\n"),
            },
        ),
        std::time::Duration::ZERO,
    );
    app.finished();
}

#[test]
fn f1_a_prompt_typed_during_a_turn_is_kept_in_the_order_it_was_typed() {
    let mut app = App::new(DARK, "anthropic/claude-sonnet-4");

    // A turn is in flight. Everything below happens while it holds the session.
    start_turn(&mut app, "refactor the parser");
    assert_eq!(app.mode(), Mode::Running);

    // The first prompt. Nothing is returned to the driver — a `Command::Submit`
    // handed out here is an instruction nobody could carry out, and through
    // 0.16.0 it was handed out anyway and dropped in the turn loop's catch-all.
    assert_eq!(send(&mut app, "and add a test for it"), Command::None);
    assert!(
        app.composer.is_empty(),
        "the composer clears on Enter whether or not the line was sent, which is \
         exactly why losing it here was invisible",
    );
    assert_eq!(app.queued_prompts(), ["and add a test for it"]);

    // A second and a third queue behind it rather than replacing it.
    assert_eq!(send(&mut app, "then update the README"), Command::None);
    assert_eq!(send(&mut app, "finally run the whole suite"), Command::None);
    assert_eq!(
        app.queued_prompts(),
        [
            "and add a test for it",
            "then update the README",
            "finally run the whole suite",
        ],
        "the queue is send order, and send order is the order they were typed",
    );

    // Nothing was discarded and nothing was sent: the turn that was running is
    // still the turn that is running.
    assert_eq!(app.mode(), Mode::Running);
}

#[test]
fn f1_queueing_says_so_and_says_how_many_are_waiting() {
    let mut app = App::new(DARK, "m");
    start_turn(&mut app, "the turn in flight");

    send(&mut app, "one");
    let first = notice(&app);
    assert!(
        first.contains("queued"),
        "a keystroke that emptied the composer and did nothing visible is the \
         defect this criterion is about: {first:?}",
    );

    // The count is in the sentence, or the second prompt produces a notice
    // identical to the first — which reads as a key that did nothing.
    send(&mut app, "two");
    let second = notice(&app);
    assert!(second.contains('2'), "{second:?}");
}

#[test]
fn f4_three_queued_lines_become_three_turns_in_order_each_its_own_exchange() {
    let mut app = App::new(DARK, "m");

    start_turn(&mut app, "the first prompt");
    send(&mut app, "the second prompt");
    send(&mut app, "the third prompt");
    send(&mut app, "the fourth prompt");
    answer_turn(&mut app, "answer to the first prompt");
    // The turn that was running is committed and out of the way, so what the
    // drain below produces is the queue's and nothing else's.
    app.take_pending();

    // What the driver does: take one, run a whole turn, come back for the next.
    // The queue is asked again each pass rather than drained into a list, which
    // is what lets a prompt typed during a queued turn go behind the rest.
    let mut exchanges = Vec::new();
    while let Some(prompt) = app.next_queued_prompt() {
        start_turn(&mut app, &prompt);
        answer_turn(&mut app, &format!("answer to {prompt}"));
        exchanges.push((prompt, text_of(&app.take_pending())));
    }

    assert_eq!(exchanges.len(), 3, "three queued lines are three turns");
    let order: Vec<&str> = exchanges
        .iter()
        .map(|(prompt, _)| prompt.as_str())
        .collect();
    assert_eq!(
        order,
        ["the second prompt", "the third prompt", "the fourth prompt"],
        "the queue fired out of the order it was typed in",
    );

    // Each turn is its own exchange: its own echo, its own answer under it, and
    // no trace of the two it is not about. A queue joined into one prompt would
    // put all three questions in the first exchange and leave the other two
    // empty — a run that answers everything in one breath and cannot be stopped
    // between the parts.
    for (prompt, scrollback) in &exchanges {
        assert!(
            scrollback.contains(prompt.as_str()),
            "{prompt:?} was not echoed into its own exchange: {scrollback:?}",
        );
        assert!(
            scrollback.contains(&format!("answer to {prompt}")),
            "{prompt:?} did not get its own answer: {scrollback:?}",
        );
        for (other, _) in &exchanges {
            assert!(
                other == prompt || !scrollback.contains(other.as_str()),
                "{other:?} appeared in the exchange for {prompt:?}: {scrollback:?}",
            );
        }
    }
}

#[test]
fn f4_the_queue_hands_back_the_lines_as_they_were_typed_never_joined() {
    let mut app = App::new(DARK, "m");
    start_turn(&mut app, "the turn in flight");
    send(&mut app, "what does this module do");
    send(&mut app, "now write the test");
    app.finished();

    // Taken one at a time and byte for byte. Joining them — with a newline, a
    // space or anything else — would be one prompt where three were typed, and
    // this is the assertion that catches it whatever the separator was.
    assert_eq!(
        app.next_queued_prompt().as_deref(),
        Some("what does this module do"),
    );
    assert_eq!(
        app.next_queued_prompt().as_deref(),
        Some("now write the test")
    );
    assert_eq!(app.next_queued_prompt(), None);
    assert!(app.queued_prompts().is_empty());
}

#[test]
fn a_prompt_typed_during_a_queued_turn_goes_behind_what_is_still_waiting() {
    let mut app = App::new(DARK, "m");

    start_turn(&mut app, "the first prompt");
    send(&mut app, "the second prompt");
    send(&mut app, "the third prompt");
    app.finished();

    // The queued turn starts, and the operator types again while it runs.
    let second = app.next_queued_prompt().expect("the queue had two");
    start_turn(&mut app, &second);
    send(&mut app, "the fourth prompt");
    assert_eq!(
        app.queued_prompts(),
        ["the third prompt", "the fourth prompt"],
        "a prompt typed during a queued turn must not jump the queue",
    );
    app.finished();

    assert_eq!(
        app.next_queued_prompt().as_deref(),
        Some("the third prompt")
    );
    assert_eq!(
        app.next_queued_prompt().as_deref(),
        Some("the fourth prompt")
    );
}

#[test]
fn an_idle_prompt_is_sent_rather_than_queued() {
    let mut app = App::new(DARK, "m");

    // The guard is on the mode and nothing else. A queue that also swallowed the
    // ordinary prompt would be a session that never runs anything.
    assert_eq!(
        send(&mut app, "what does this module do"),
        Command::Submit("what does this module do".into()),
    );
    assert!(app.queued_prompts().is_empty());

    // And after the turn it started has ended, the next one is sent again.
    start_turn(&mut app, "what does this module do");
    app.finished();
    assert_eq!(send(&mut app, "again"), Command::Submit("again".into()));
    assert!(app.queued_prompts().is_empty());
}

#[test]
fn a_slash_command_typed_mid_turn_is_still_refused_rather_than_queued() {
    let mut app = App::new(DARK, "m");
    start_turn(&mut app, "the turn in flight");

    // `/model` or `/fork` held until the turn ended would take effect at a moment
    // nobody could predict. The driver refuses them with a sentence; only a
    // prompt is the kind of thing that keeps its meaning after the turn in front
    // of it, so only a prompt is queued.
    assert_eq!(
        send(&mut app, "/model gpt-5"),
        Command::Slash("model gpt-5".into())
    );
    assert!(app.queued_prompts().is_empty());
}

#[test]
fn a_shell_line_typed_mid_turn_is_still_refused_rather_than_queued() {
    let mut app = App::new(DARK, "m");
    start_turn(&mut app, "the turn in flight");

    // The operator's own line, wanted now or not at all — and the agent never
    // hears about it either way, so there is nothing for a queue to sequence it
    // against.
    assert_eq!(
        send(&mut app, "!git status"),
        Command::Shell("git status".into())
    );
    assert!(app.queued_prompts().is_empty());
}

#[test]
fn a_blank_line_mid_turn_queues_nothing() {
    let mut app = App::new(DARK, "m");
    start_turn(&mut app, "the turn in flight");

    // The composer refuses to submit whitespace, so this never reaches the queue
    // — asserted rather than assumed, because a queue that accepted empty lines
    // would spend a turn each on them.
    assert_eq!(send(&mut app, "   "), Command::None);
    assert!(app.queued_prompts().is_empty());
    assert!(
        app.status.notice.is_none(),
        "nothing happened, so nothing is said"
    );
}

#[test]
fn the_queue_is_not_the_prompt_the_running_turn_is_about() {
    let mut app = App::new(DARK, "m");

    start_turn(&mut app, "the prompt this turn is about");
    send(&mut app, "the prompt waiting behind it");

    // `submitted` is single-valued and stays that way: an undone turn puts back
    // the line it was about, not the line typed after it and not the two joined.
    let (_, restored) = app.undo_turn();
    assert_eq!(restored, "the prompt this turn is about");
    assert_eq!(app.composer.text(), "the prompt this turn is about");

    // The queue is untouched by the undo. Dropping it is the driver's decision,
    // taken because the operator pressed a stop key — see the test below — and
    // not a side effect of the turn's rows coming off the screen.
    assert_eq!(app.queued_prompts(), ["the prompt waiting behind it"]);
}

#[test]
fn a_stopped_turn_drops_what_was_waiting_behind_it() {
    let mut app = App::new(DARK, "m");
    start_turn(&mut app, "the turn in flight");
    send(&mut app, "one");
    send(&mut app, "two");
    app.finished();

    // The stop key stops the session, not just the step in front of the
    // operator. A queue that fired anyway would make one press start two more
    // turns against a conversation they had just decided to steer elsewhere.
    assert_eq!(
        app.forget_queued_prompts(),
        2,
        "the count is what the driver says"
    );
    assert!(app.queued_prompts().is_empty());
    assert_eq!(app.next_queued_prompt(), None);
    assert_eq!(app.forget_queued_prompts(), 0, "nothing left to drop");
}

#[test]
fn the_queue_starts_empty_and_is_never_read_from_anywhere_else() {
    // A session that has run nothing has nothing waiting, which is the state the
    // surface has to render on the very first frame.
    let app = App::new(DARK, "m");
    assert!(app.queued_prompts().is_empty());
}

/// **F4, the half that lives in the driver.** The drain is one turn per prompt.
///
/// `src/main.rs` is linked by no integration test, so the loop that turns a queued
/// prompt back into a turn is a decision nothing can drive — the same problem
/// `tests/structure.rs` solves for the order of two calls in the same file, and
/// the same answer: read the driver. The offsets come from the source with its
/// comments stripped, so the paragraph you are reading could be pasted into the
/// driver and this test would still fail.
///
/// What it pins is the shape a joined queue cannot have: the arm asks for one
/// prompt, runs a whole turn, and comes back — rather than collecting the queue
/// and sending it as a single prompt, which is a run that answers three questions
/// in one breath with no boundary an operator can stop it at.
#[test]
fn f4_the_driver_runs_one_turn_per_queued_prompt() {
    let driver = driver_without_comments();
    let arm = driver
        .split_once("Command::Submit(text) => {")
        .expect("the driver dispatches a submitted prompt")
        .1
        .split_once("\n            }")
        .expect("the arm closes")
        .0;

    assert!(
        arm.contains("while let Some(text) = next.take()"),
        "a submitted prompt is followed by whatever queued behind it, in a loop; \
         without one the queue is captured and never fires:\n{arm}",
    );
    assert!(
        arm.contains("next = app.next_queued_prompt();"),
        "the queue is asked again at the bottom of every pass, which is what puts a \
         prompt typed during a queued turn behind the rest:\n{arm}",
    );
    assert!(
        !arm.contains("join("),
        "the queue is never joined into one prompt — three lines are three turns, \
         each with its own echo, its own answer and its own Ctrl+C:\n{arm}",
    );
    assert!(
        arm.contains("app.forget_queued_prompts()"),
        "a stopped turn drops what was waiting behind it, or one press of the stop \
         key starts the next three turns:\n{arm}",
    );
}

/// `src/main.rs` with every trailing comment removed, so the assertions above are
/// over the code that runs rather than over prose about it. The same helper
/// `tests/structure.rs` reads the driver with; duplicated rather than shared
/// because `tests/support/mod.rs` does not carry it yet.
fn driver_without_comments() -> String {
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("src/main.rs");
    let text = std::fs::read_to_string(path).expect("the driver is readable");
    text.lines()
        .map(|line| match line.find("//") {
            Some(at) => &line[..at],
            None => line,
        })
        .collect::<Vec<_>>()
        .join("\n")
}

fn notice(app: &App) -> String {
    app.status
        .notice
        .as_ref()
        .map(|(_, text)| text.clone())
        .unwrap_or_default()
}
