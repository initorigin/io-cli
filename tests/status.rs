//! The status line, and its share of F9: at eighty columns it degrades to a
//! narrow form rather than wrapping.

mod support;

use std::time::Duration;

use io_cli::app::App;
use io_cli::status::{format_elapsed, Status};
use io_cli::theme::{DARK, PLAIN};
use io_harness::{EventKind, RunEvent};

/// A run event at step zero, which is where everything but a step sits.
fn event(kind: EventKind) -> RunEvent {
    RunEvent::new(1, 0, kind)
}

fn step(number: u32, tokens: u64) -> RunEvent {
    RunEvent::new(
        1,
        number,
        EventKind::Step {
            decision: "edited src/lib.rs".into(),
            tool_call: "apply_patch".into(),
            tokens,
            changed: true,
        },
    )
}

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

/// **F9.** A field with nothing behind it is absent. Not a zero, not a dash, not a
/// placeholder — and the field this matters most for is the one about spending.
#[test]
fn f9_a_field_with_nothing_behind_it_is_absent_rather_than_zero() {
    let status = Status::new("opus-5");
    let line = status.line(120, &DARK).to_string();

    assert!(
        !line.contains("tok"),
        "no step has reported a token count yet: {line:?}",
    );
    assert!(
        !line.contains("ctx"),
        "nothing has said how full the context is: {line:?}",
    );
    // Deliberately not "the line contains no zero": the elapsed field is `0s` and
    // is legitimately zero, because the session really has been open no time at
    // all. The criterion is about a field with no *fact* behind it.
    assert!(
        !line.contains("0 tok") && !line.contains("ctx 0"),
        "an unknown value must not be rendered as a zero: {line:?}",
    );
    // And nothing has said how this run's commands are contained, which is a
    // different statement from saying they are not.
    assert!(
        !line.contains("sandbox"),
        "containment is unknown until the run says so: {line:?}",
    );
}

/// Tokens accumulate across the steps of a session rather than showing the last
/// step's own count, which would swing rather than climb.
#[test]
fn the_token_field_is_the_session_and_not_the_last_step() {
    let mut app = App::new(DARK, "opus-5");
    app.event(&step(1, 1_200), std::time::Duration::ZERO);
    app.event(&step(2, 300), std::time::Duration::ZERO);

    let line = app.status.line(120, &DARK).to_string();
    assert!(
        line.contains("1.5k tok"),
        "the field is the running total: {line:?}",
    );
}

/// **F9, containment.** The mode is what was asked for and the backend is what
/// answered on this host, and io-harness's own documentation says a surface
/// showing the first without the second is reading an intention rather than a
/// fact: `workspace-write` on a portable floor means resource caps only.
#[test]
fn the_containment_field_carries_the_backend_and_not_only_the_mode() {
    let mut app = App::new(DARK, "opus-5");
    app.event(
        &event(EventKind::Contained {
            mode: "workspace-write".into(),
            backend: "portable-floor".into(),
            roots: 0,
        }),
        std::time::Duration::ZERO,
    );

    let line = app.status.line(120, &DARK).to_string();
    assert!(line.contains("workspace-write"), "{line:?}");
    assert!(
        line.contains("portable-floor"),
        "the mode without the backend is an intention, not a fact: {line:?}",
    );
}

/// Context pressure appears once something has said what it is, and says it as a
/// share of the budget io-harness itself declares rather than of a number copied
/// into this repository.
#[test]
fn the_context_field_appears_when_a_fold_reports_one() {
    let mut app = App::new(DARK, "opus-5");
    assert!(!app.status.line(120, &DARK).to_string().contains("ctx"));

    app.event(
        &event(EventKind::Compacted {
            through_step: 4,
            before_tokens: 11_000,
            after_tokens: 6_000,
        }),
        std::time::Duration::ZERO,
    );

    let line = app.status.line(120, &DARK).to_string();
    assert!(
        line.contains("ctx "),
        "a fold is the harness telling us how full it was: {line:?}",
    );
    assert!(line.contains('%'), "{line:?}");
}

/// N5's half of this task: the new fields drop from the right, and the line never
/// becomes two lines. A status line that wraps has taken a row from the transcript
/// and stopped being a status line.
#[test]
fn the_new_fields_drop_from_the_right_rather_than_wrapping() {
    let mut app = App::new(DARK, "opus-5");
    app.set_posture(Some(io_cli::settings::Posture::Workspace));
    app.event(&step(1, 12_400), std::time::Duration::ZERO);
    app.event(
        &event(EventKind::Contained {
            mode: "workspace-write".into(),
            backend: "seatbelt".into(),
            roots: 2,
        }),
        std::time::Duration::ZERO,
    );

    let wide = app.status.line(160, &DARK).to_string();
    assert!(wide.contains("seatbelt"), "{wide:?}");

    let narrow = app.status.line(40, &DARK).to_string();
    assert!(narrow.chars().count() <= 40, "{narrow:?}");
    assert!(!narrow.contains('\n'), "the line wrapped: {narrow:?}");
    assert!(
        narrow.contains("opus-5"),
        "the model is the last field to go: {narrow:?}",
    );
    assert!(
        !narrow.contains("seatbelt"),
        "the rightmost fields are the ones that drop: {narrow:?}",
    );
}
