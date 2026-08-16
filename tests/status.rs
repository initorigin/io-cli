//! The status line, and its share of F9: at eighty columns it degrades to a
//! narrow form rather than wrapping.

mod support;

use std::time::Duration;

use io_cli::status::{format_elapsed, Status};
use io_cli::theme::{DARK, PLAIN};

fn rendered(status: &Status, width: u16) -> String {
    status
        .line(width, &DARK)
        .spans
        .iter()
        .map(|span| span.content.as_ref())
        .collect()
}

#[test]
fn it_says_the_model_the_state_and_the_elapsed_time() {
    let mut status = Status::new("anthropic/claude-sonnet-4");
    status.elapsed = Duration::from_secs(72);
    status.working = true;

    let line = rendered(&status, 80);
    assert!(line.contains("anthropic/claude-sonnet-4"), "got {line:?}");
    assert!(line.contains("working"), "got {line:?}");
    assert!(line.contains("1m12s"), "got {line:?}");
}

#[test]
fn the_running_state_is_a_word_and_not_only_a_colour() {
    let mut status = Status::new("m");
    assert!(rendered(&status, 80).contains("ready"));
    status.working = true;
    assert!(rendered(&status, 80).contains("working"));

    // The same under NO_COLOR, where the tone carries nothing at all.
    let plain: String = status
        .line(80, &PLAIN)
        .spans
        .iter()
        .map(|span| span.content.as_ref())
        .collect();
    assert!(plain.contains("working"), "got {plain:?}");
}

#[test]
fn f4_a_running_turn_carries_a_moving_indicator_beside_the_word() {
    let mut status = Status::new("m");
    status.working = true;

    let first = rendered(&status, 80);
    assert!(first.contains("working"), "got {first:?}");
    let spinning = first
        .chars()
        .find(|character| io_cli::status::SPINNER.contains(character))
        .expect("a running turn shows an indicator");

    // The tick is what moves it, and it moves on the tick alone — nothing here
    // waits for anything.
    status.advance();
    let second = rendered(&status, 80);
    let moved = second
        .chars()
        .find(|character| io_cli::status::SPINNER.contains(character))
        .expect("the indicator is still there");
    assert_ne!(
        spinning, moved,
        "the indicator did not move between two ticks: {second:?}",
    );

    // An idle session has nothing to be alive about.
    status.working = false;
    let idle = rendered(&status, 80);
    assert!(
        !idle.chars().any(|c| io_cli::status::SPINNER.contains(&c)),
        "an idle session was animating: {idle:?}",
    );
}

#[test]
fn f4_no_color_keeps_the_word_and_drops_the_animation() {
    let mut status = Status::new("m");
    status.working = true;

    for tick in 0..SPINNER_LEN {
        let plain: String = status
            .line(80, &PLAIN)
            .spans
            .iter()
            .map(|span| span.content.as_ref())
            .collect();
        assert!(
            plain.contains("working"),
            "the word went with the animation at tick {tick}: {plain:?}",
        );
        assert!(
            !plain.chars().any(|c| io_cli::status::SPINNER.contains(&c)),
            "NO_COLOR animated at tick {tick}: {plain:?}",
        );
        status.advance();
    }
}

/// How many frames the indicator cycles through.
const SPINNER_LEN: usize = io_cli::status::SPINNER.len();

#[test]
fn f9_a_narrow_terminal_drops_whole_fields_from_the_right() {
    let mut status = Status::new("anthropic/claude-sonnet-4");
    status.elapsed = Duration::from_secs(72);
    status.working = true;

    let wide = rendered(&status, 80);
    assert_eq!(
        wide, "anthropic/claude-sonnet-4 · ⠋ working · 1m12s",
        "the full line at eighty columns",
    );

    // Room for the model and the state, but not the clock.
    let narrow = rendered(&status, 38);
    assert_eq!(narrow, "anthropic/claude-sonnet-4 · ⠋ working");

    // Room for the model only.
    let narrower = rendered(&status, 30);
    assert_eq!(narrower, "anthropic/claude-sonnet-4");

    for width in [1u16, 8, 20, 25, 26, 40, 43, 44, 200] {
        let line = rendered(&status, width);
        assert!(
            line.chars().count() <= width as usize,
            "the line overflowed {width} columns: {line:?}",
        );
        assert!(
            !line.contains('\n'),
            "the status line wrapped at {width} columns: {line:?}",
        );
        assert!(!line.is_empty(), "the line vanished at {width} columns");
    }
}

#[test]
fn f9_it_renders_on_one_row_at_eighty_columns() {
    let mut status = Status::new("anthropic/claude-sonnet-4");
    status.elapsed = Duration::from_secs(3725);
    let (mut screen, _recorder) = support::screen(80, 24);

    screen
        .draw(|frame| status.render(frame, frame.area(), &DARK))
        .expect("frame");

    let viewport = screen.viewport_text();
    assert_eq!(
        viewport.lines().filter(|line| !line.is_empty()).count(),
        1,
        "the status line took more than one row: {viewport:?}",
    );
    assert!(viewport.contains("1h02m"), "got {viewport:?}");
}

#[test]
fn elapsed_time_is_readable_at_every_scale() {
    assert_eq!(format_elapsed(Duration::ZERO), "0s");
    assert_eq!(format_elapsed(Duration::from_secs(9)), "9s");
    assert_eq!(format_elapsed(Duration::from_secs(59)), "59s");
    assert_eq!(format_elapsed(Duration::from_secs(60)), "1m00s");
    assert_eq!(format_elapsed(Duration::from_secs(72)), "1m12s");
    assert_eq!(format_elapsed(Duration::from_secs(3599)), "59m59s");
    assert_eq!(format_elapsed(Duration::from_secs(3600)), "1h00m");
    assert_eq!(format_elapsed(Duration::from_secs(3725)), "1h02m");
}
