//! F1 and F2 — the session repaints while a turn runs, and never while it is idle.
//!
//! Both are asserted against a clock the test advances by hand. Nothing here
//! sleeps and nothing here measures how long anything took: `App::tick` takes the
//! session's age as an argument precisely so that the two properties can be
//! checked without a timer being involved in the checking. N1 depends on that,
//! and `tests/timing.rs` enforces it over this file too.

mod support;

use std::time::Duration;

use io_cli::app::App;
use io_cli::theme::DARK;

/// What the driver does with a tick: repaint if, and only if, the tick says to.
///
/// This is the shape `main.rs` uses, reproduced here because the property being
/// asserted is about the pair — a `tick` that returns the right answer and a
/// driver that ignores it would still redraw forever.
fn pump(app: &mut App, screen: &mut io_cli::term::Screen<support::Fixed>, age: Duration) -> bool {
    if !app.tick(age) {
        return false;
    }
    screen
        .draw(|frame| app.render(frame, frame.area()))
        .expect("frame");
    true
}

/// The footer's identity row: the state word, the model, and the clock.
///
/// Second from the bottom since 0.11.0 — the last row is the counts, and the row
/// above the identity one is the rule. This is the row the clock is on, which is
/// what every test in this file is about.
fn status_row(screen: &io_cli::term::Screen<support::Fixed>) -> String {
    let viewport = screen.viewport_text().to_string();
    let rows: Vec<&str> = viewport.lines().collect();
    rows.get(rows.len().saturating_sub(2))
        .unwrap_or(&"")
        .to_string()
}

#[test]
fn f1_a_running_turn_repaints_with_no_event_arriving() {
    let (mut screen, _recorder) = support::screen(80, 24);
    let mut app = App::new(DARK, "anthropic/claude-sonnet-4");
    app.started();

    // No event is ever delivered to `app` in this test. The only thing that
    // happens is the clock moving.
    assert!(
        pump(&mut app, &mut screen, Duration::from_secs(1)),
        "a running turn did not repaint on the tick",
    );
    let first = status_row(&screen);

    assert!(pump(&mut app, &mut screen, Duration::from_secs(2)));
    let second = status_row(&screen);

    assert_ne!(
        first, second,
        "the status line did not change between two ticks of a running turn",
    );
    assert!(
        second.contains("2s"),
        "the clock did not advance on its own: {second:?}",
    );
}

#[test]
fn f2_an_idle_session_never_repaints() {
    let (mut screen, recorder) = support::screen(80, 24);
    let mut app = App::new(DARK, "anthropic/claude-sonnet-4");

    // One frame so the recorder holds a session that has already drawn; what is
    // asserted below is that nothing is added to it, not that nothing was ever
    // written.
    screen
        .draw(|frame| app.render(frame, frame.area()))
        .expect("frame");
    let quiet = recorder.bytes().len();

    for second in 1..=10 {
        assert!(
            !pump(&mut app, &mut screen, Duration::from_secs(second)),
            "an idle session repainted at second {second}",
        );
    }

    assert_eq!(
        recorder.bytes().len(),
        quiet,
        "an idle session wrote to the terminal",
    );
}

#[test]
fn f2_the_tick_stops_again_when_the_turn_ends() {
    let (mut screen, _recorder) = support::screen(80, 24);
    let mut app = App::new(DARK, "m");

    // Whether the running tick fires is F1's assertion and is deliberately not
    // repeated here, so that sabotaging the repaint fails F1's test and this one
    // keeps testing only what it is named for.
    app.started();
    pump(&mut app, &mut screen, Duration::from_secs(1));

    app.finished();
    assert!(
        !pump(&mut app, &mut screen, Duration::from_secs(2)),
        "the tick outlived the turn that justified it",
    );
}

/// The activity line: the third row the viewport draws.
///
/// The first is the streaming tail of what the agent is saying and the second is
/// deliberately blank — the air between the work and the line describing it.
/// **The order was the other way round until 0.13.1**, which is why this reads a
/// row index at all: the line moved, and a helper that had gone on reading row
/// one would have been asserting about the blank.
fn activity_row(screen: &io_cli::term::Screen<support::Fixed>) -> String {
    screen
        .viewport_text()
        .lines()
        .nth(2)
        .unwrap_or_default()
        .to_string()
}

/// Whether any word from the activity line's own list is on the row.
fn is_activity(row: &str) -> bool {
    io_cli::status::WORDS.iter().any(|word| row.contains(word))
}

/// 0.11.0 F5 — the activity line is up for exactly the turn.
///
/// Before the first turn there is no turn for it to be about; after the last one
/// ends there is none either, and a clock still moving over an idle session is
/// the criterion's own sabotage arm. `App::finished` is the single exit — a turn
/// that ended by interrupt, by refusal or by error leaves through it too — so
/// asserting on it is asserting on all four endings.
#[test]
fn f5_the_activity_line_is_present_for_exactly_the_turn() {
    let (mut screen, _recorder) = support::screen(80, 24);
    let mut app = App::new(DARK, "anthropic/claude-sonnet-4");

    screen
        .draw(|frame| app.render(frame, frame.area()))
        .expect("frame");
    assert!(
        !is_activity(&activity_row(&screen)),
        "an idle session drew an activity line: {:?}",
        activity_row(&screen),
    );

    app.started();
    pump(&mut app, &mut screen, Duration::from_secs(1));
    let running = activity_row(&screen);
    assert!(is_activity(&running), "{running:?}");
    assert!(
        running.contains("1s"),
        "the clock is on the activity line: {running:?}",
    );

    app.finished();
    screen
        .draw(|frame| app.render(frame, frame.area()))
        .expect("frame");
    assert!(
        !is_activity(&activity_row(&screen)),
        "the activity line outlived the turn: {:?}",
        activity_row(&screen),
    );
}

/// 0.11.0 F5 — the clock advances on the tick, not on an event arriving.
///
/// No event is delivered in this test at all. The only thing that happens is the
/// age the driver hands in, which is the same argument the status line's own
/// clock is drawn from — and the token count is read off the same `Status`, so
/// the two lines cannot disagree about a number they share.
#[test]
fn f5_the_activity_clock_advances_on_the_tick_and_shares_the_status_lines_numbers() {
    let (mut screen, _recorder) = support::screen(80, 24);
    let mut app = App::new(DARK, "m");
    app.started();
    app.status.tokens = Some(9_000);
    app.status.run_tokens = Some(1_500);

    pump(&mut app, &mut screen, Duration::from_secs(1));
    let first = activity_row(&screen);
    pump(&mut app, &mut screen, Duration::from_secs(62));
    let second = activity_row(&screen);

    assert_ne!(
        first, second,
        "the activity line's clock did not move between two ticks",
    );
    assert!(second.contains("1m02s"), "{second:?}");

    // The same token count, in the same spelling, on both rows. The footer's
    // counts are its last row — the identity row above them carries the state,
    // the model and the clock, and the numbers live under it.
    let counts = screen
        .viewport_text()
        .lines()
        .next_back()
        .unwrap_or_default()
        .to_string();
    // The activity line carries THIS turn's spend; the footer carries the
    // session's. Two counters because the two rows answer different questions.
    assert!(second.contains("1.5k tok"), "{second:?}");
    assert!(counts.contains("9.0k tok"), "{counts:?}");
}

#[test]
fn an_idle_tick_does_not_move_the_clock() {
    let mut app = App::new(DARK, "m");
    app.tick(Duration::from_secs(30));
    assert_eq!(
        app.status.elapsed,
        Duration::ZERO,
        "an idle session advanced its own clock, which is a repaint waiting to happen",
    );
}
