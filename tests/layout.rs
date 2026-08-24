//! F7 — the work is above the line that says it is working.
//!
//! The viewport's rows, top to bottom, while a turn is in flight: the streaming
//! tail of what the agent is saying, a row of air, the activity line, the
//! composer, the status footer. Up to 0.13.0 the first two were the other way
//! round, so the newest words the agent had written sat *under* a spinner — a
//! footnote to the state of the turn rather than the continuation of the
//! transcript directly above them.
//!
//! Asserted on the rendered viewport rather than on the rectangles, because a
//! test that recomputed the offsets would agree with the renderer by
//! construction and say nothing about what an operator sees.

mod support;

use std::time::Duration;

use io_cli::app::App;
use io_cli::theme::DARK;
use io_harness::{EventKind, RunEvent};

/// A session with a turn in flight and one line of streamed prose in the live
/// row.
fn working() -> App {
    let mut app = App::new(DARK, "opus-5");
    app.started();
    app.event(
        &RunEvent::new(
            1,
            1,
            EventKind::Started {
                goal: "count the tests".into(),
                provider: "openrouter".into(),
            },
        ),
        Duration::from_secs(1),
    );
    app.event(
        &RunEvent::new(
            1,
            1,
            EventKind::Token {
                text: "STREAMING".into(),
            },
        ),
        Duration::from_secs(1),
    );
    app
}

/// The viewport's rows as text, with the trailing blanks kept: a row's *index*
/// is the whole assertion here.
fn rows(app: &mut App, height: u16) -> Vec<String> {
    let (mut screen, _recorder) = support::screen_of(60, 24, height);
    screen
        .draw(|frame| app.render(frame, frame.area()))
        .expect("frame");
    screen.viewport_text().lines().map(str::to_string).collect()
}

/// Where the row holding `needle` is, or a panic naming what was on screen.
fn row_of(rows: &[String], needle: &str) -> usize {
    rows.iter()
        .position(|row| row.contains(needle))
        .unwrap_or_else(|| panic!("no row holds {needle:?}: {rows:#?}"))
}

#[test]
fn f7_the_streaming_row_is_above_the_activity_line() {
    let mut app = working();
    let rows = rows(&mut app, io_cli::term::VIEWPORT_HEIGHT);

    let live = row_of(&rows, "STREAMING");
    // The activity line is the one carrying the elapsed clock beside the word
    // that names the state; the word itself is chosen by the step count, so the
    // clock is what identifies the row without pinning the vocabulary.
    let activity = rows
        .iter()
        .position(|row| row.contains('s') && row.contains('·') && !row.contains("STREAMING"))
        .unwrap_or_else(|| panic!("no activity line on screen: {rows:#?}"));
    let composer = row_of(&rows, io_cli::composer::PROMPT.trim_end());

    assert!(
        live < activity,
        "the streaming row is at {live} and the activity line at {activity}: the \
         work is still under the line that says it is working. {rows:#?}"
    );
    // **A rule between them since 0.13.1.** The footer has opened with one since
    // 0.1.0 and the prompt had a boundary on one side only, so the composer read
    // as the tail of whatever the turn had last written rather than as a field.
    assert!(
        rows[activity + 1].chars().all(|c| c == '─' || c == '-'),
        "a rule belongs between the activity line and the composer. {rows:#?}"
    );
    assert_eq!(
        activity + 2,
        composer,
        "the composer sits directly under its own rule. {rows:#?}"
    );
    assert!(
        rows[live + 1].trim().is_empty(),
        "there is no row of air between the work and the activity line: {rows:#?}"
    );
}

#[test]
fn f7_a_viewport_too_short_for_the_air_keeps_the_order() {
    // Seven rows is the height at which `App::render` stops claiming the blank.
    // What must not happen is the two rows swapping back to buy it.
    let mut app = working();
    let rows = rows(&mut app, 7);

    let live = row_of(&rows, "STREAMING");
    let composer = row_of(&rows, io_cli::composer::PROMPT.trim_end());
    assert!(
        live < composer,
        "the work fell below the composer on a short viewport: {rows:#?}"
    );
}
