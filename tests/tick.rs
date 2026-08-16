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

/// The status line is the last row the viewport drew.
fn status_row(screen: &io_cli::term::Screen<support::Fixed>) -> String {
    screen
        .viewport_text()
        .lines()
        .next_back()
        .unwrap_or_default()
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
