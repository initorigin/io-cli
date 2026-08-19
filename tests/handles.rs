//! F6 — background work is named, counted and accounted for.
//!
//! A `shell_start` is the one tool call whose effect outlives the step that made
//! it. io-harness emits five events describing a handle's whole life and, before
//! this release, nothing rendered any of them — so a run waiting on a dev server
//! looked exactly like a run that had hung.
//!
//! The count is asserted as a **property of the sequence**, not of a single
//! event: a handle opens once and closes once, so any interleaving of starts and
//! endings must return the field to nothing. That is what makes the missing-arm
//! sabotage visible.

mod support;

use io_cli::app::App;
use io_cli::status::Status;
use io_cli::theme::DARK;
use io_harness::{EventKind, RunEvent};
use std::time::Duration;

const WIDE: u16 = 120;

fn at() -> Duration {
    Duration::from_secs(1)
}

fn event(kind: EventKind) -> RunEvent {
    RunEvent::new(1, 0, kind)
}

fn started(handle: u64, line: &str) -> RunEvent {
    event(EventKind::HandleStarted {
        handle,
        line: line.to_string(),
    })
}

/// The status line as a reader would see it.
fn status_line(status: &Status) -> String {
    status
        .line(WIDE, &DARK)
        .spans
        .iter()
        .map(|span| span.content.as_ref())
        .collect()
}

/// Everything the app has queued for the scrollback, as text.
fn committed(app: &mut App) -> String {
    app.take_pending()
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

fn app() -> App {
    App::new(DARK, "a-model")
}

#[test]
fn a_started_handle_names_the_command_it_is_running() {
    let mut app = app();
    app.event(&started(7, "npm run dev"), at());

    let text = committed(&mut app);
    assert!(text.contains("job 7"), "{text}");
    assert!(text.contains("npm run dev"), "{text}");
    assert!(text.contains("background"), "{text}");
}

#[test]
fn nothing_running_shows_no_field_at_all_rather_than_zero() {
    // A session that has started no background work has not started zero jobs.
    // The same distinction `tokens` and `spend` are held to, and what keeps this
    // field off the overwhelming majority of lines.
    let app = app();
    assert!(
        !status_line(&app.status).contains("bg"),
        "{}",
        status_line(&app.status)
    );
}

#[test]
fn a_live_handle_is_counted_on_the_status_line() {
    let mut app = app();
    app.event(&started(1, "sleep 100"), at());
    assert!(
        status_line(&app.status).contains("bg 1"),
        "{}",
        status_line(&app.status)
    );

    app.event(&started(2, "tail -f log"), at());
    assert!(
        status_line(&app.status).contains("bg 2"),
        "{}",
        status_line(&app.status)
    );
}

#[test]
fn every_ending_returns_the_count_and_each_says_which_one_it_was() {
    // Three endings, one each, asserted together — because the arm that ships is
    // an ending with no arm behind it, and a test that only ever exercises
    // `HandleExited` cannot see the other two go missing.
    for (kind, expected) in [
        (
            EventKind::HandleExited {
                handle: 1,
                code: Some(0),
            },
            "exited cleanly",
        ),
        (
            EventKind::HandleExited {
                handle: 1,
                code: Some(2),
            },
            "exited with status 2",
        ),
        (EventKind::HandleKilled { handle: 1 }, "killed"),
        (
            EventKind::HandleOrphaned {
                handle: 1,
                reason: "the run finished".to_string(),
            },
            "was left running",
        ),
    ] {
        let mut app = app();
        app.event(&started(1, "sleep 100"), at());
        let _ = committed(&mut app);

        app.event(&event(kind), at());
        let text = committed(&mut app);

        assert!(text.contains(expected), "{text}");
        assert!(text.contains("job 1"), "{text}");
        assert!(
            !status_line(&app.status).contains("bg"),
            "the count returns to nothing after {expected}: {}",
            status_line(&app.status),
        );
    }
}

#[test]
fn an_exit_with_no_status_says_so_rather_than_inventing_a_number() {
    // A process killed by a signal ends with no code. Rendering that as `0` would
    // report a signal as a clean exit.
    let mut app = app();
    app.event(&started(3, "sleep 100"), at());
    let _ = committed(&mut app);
    app.event(
        &event(EventKind::HandleExited {
            handle: 3,
            code: None,
        }),
        at(),
    );

    let text = committed(&mut app);
    assert!(text.contains("no status"), "{text}");
    assert!(!text.contains("status 0"), "{text}");
}

#[test]
fn a_poll_is_not_an_ending_and_commits_nothing() {
    // `HandlePolled` carries a byte count and never the bytes. A line per poll
    // would bury the transcript under the progress of the very thing the operator
    // put in the background so they would not have to watch it — and treating it
    // as an ending would take the count down while the job is still alive.
    let mut app = app();
    app.event(&started(4, "cargo watch"), at());
    let _ = committed(&mut app);

    app.event(
        &event(EventKind::HandlePolled {
            handle: 4,
            bytes: 2_048,
        }),
        at(),
    );

    assert_eq!(committed(&mut app), "", "a poll writes no line");
    assert!(
        status_line(&app.status).contains("bg 1"),
        "the job is still running: {}",
        status_line(&app.status),
    );
}

#[test]
fn two_jobs_ending_one_at_a_time_count_down_rather_than_off() {
    let mut app = app();
    app.event(&started(1, "a"), at());
    app.event(&started(2, "b"), at());
    app.event(&event(EventKind::HandleKilled { handle: 1 }), at());

    assert!(
        status_line(&app.status).contains("bg 1"),
        "one of the two is still up: {}",
        status_line(&app.status),
    );
}

#[test]
fn an_ending_whose_start_was_never_seen_does_not_wrap_the_count() {
    // A resumed run replays a backlog, and a bare decrement on an unsigned count
    // would put eighteen quintillion background jobs on the status line.
    let mut app = app();
    app.event(&event(EventKind::HandleKilled { handle: 9 }), at());

    assert!(
        !status_line(&app.status).contains("bg"),
        "{}",
        status_line(&app.status),
    );
}

#[test]
fn the_count_belongs_to_the_run_that_started_it() {
    // `/resume`, `/fork` and a rewind all forget the run. A count left behind
    // would assert that another run's jobs are alive, and no event would ever
    // arrive to close them.
    let mut app = app();
    app.event(&started(1, "sleep 100"), at());
    assert!(status_line(&app.status).contains("bg 1"));

    app.status.forget_run();
    assert!(
        !status_line(&app.status).contains("bg"),
        "{}",
        status_line(&app.status),
    );
}
