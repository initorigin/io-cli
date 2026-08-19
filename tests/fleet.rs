//! F4 — the fleet counts tiers, and a queued child is a count and never a row.
//! F5 — the view opens over the composer, updates live, and costs no row it does
//! not have.
//!
//! Everything here is driven by a synthetic `RunEvent` stream, which is the whole
//! point: the model has to be right for a tree that queues, drains, detaches and
//! resumes, and none of those shapes needs a provider to state.

mod support;

use std::time::Duration;

use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use io_cli::app::App;
use io_cli::fleet::{Fleet, State};
use io_cli::theme::DARK;
use io_harness::{EventKind, RunEvent};

fn key(code: KeyCode) -> KeyEvent {
    KeyEvent::new(code, KeyModifiers::NONE)
}

fn spawned(parent: i64, depth: u32, child: i64, goal: &str) -> RunEvent {
    RunEvent::at_depth(
        parent,
        1,
        depth,
        EventKind::Spawned {
            child_run_id: child,
            goal: goal.to_string(),
        },
    )
}

fn tier(at: u32, working: u32, queued: u32, done: u32) -> RunEvent {
    RunEvent::at_depth(
        1,
        1,
        0,
        EventKind::Fleet {
            tier: at,
            working,
            queued,
            done,
        },
    )
}

/// F4 — a tier's shape is replaced, never accumulated.
///
/// Each `Fleet` carries that tier's whole shape as it now stands, including the
/// backlog a resumed tree reads back out of the store and reports before its
/// provider is called. Adding them up would make a restart look like a doubling.
#[test]
fn f4_a_tier_is_replaced_rather_than_added_up() {
    let mut fleet = Fleet::new();
    fleet.event(&tier(1, 1, 3, 0));
    fleet.event(&tier(1, 2, 2, 0));
    fleet.event(&tier(1, 4, 0, 2));
    assert_eq!(fleet.tiers().len(), 1, "one entry per tier: {:?}", fleet.tiers());
    let held = fleet.tiers()[0];
    assert_eq!((held.working, held.queued, held.done), (4, 0, 2));
}

/// F4 — tiers are counted apart, because a stuck level is invisible in a total.
#[test]
fn f4_each_tier_is_counted_on_its_own() {
    let mut fleet = Fleet::new();
    fleet.event(&tier(1, 4, 0, 1));
    fleet.event(&tier(2, 1, 9, 0));
    assert_eq!(fleet.tiers().len(), 2);
    let summary = fleet.summary();
    assert!(summary.contains("tier 1: 4 working, 0 queued, 1 done"), "{summary}");
    assert!(
        summary.contains("tier 2: 1 working, 9 queued, 0 done"),
        "a fan-out stuck at depth two is exactly what one tree-wide number \
         cannot say: {summary}",
    );
}

/// F4 — a queued child is a count and never a row.
///
/// io-harness emits `Fleet` for a child that has to wait *before* it is admitted,
/// and `Spawned` only after a slot frees and a run id exists — so a waiting child
/// has no id, no goal and nothing to draw a row from. The sabotage arm adds a
/// placeholder row per queued child, which puts an agent on screen that does not
/// exist yet.
#[test]
fn f4_a_queued_child_is_a_count_and_never_a_row() {
    let mut fleet = Fleet::new();
    // The order io-harness emits them in: the queue is reported first, and the
    // admission that follows is what produces a run id.
    fleet.event(&tier(1, 1, 2, 0));
    assert!(
        fleet.children().is_empty(),
        "two children are waiting and none of them has a row: {:?}",
        fleet.children(),
    );
    assert!(fleet.summary().contains("2 queued"));

    fleet.event(&spawned(1, 0, 7, "read every file under src/"));
    assert_eq!(fleet.children().len(), 1, "the admitted one has a row");
    assert_eq!(fleet.children()[0].run_id, 7);
}

/// F4 — a child announced twice is one child.
///
/// A resumed tree announces children it already had. A second row for one agent
/// is a fleet that looks twice its size.
#[test]
fn f4_a_child_announced_twice_is_one_child() {
    let mut fleet = Fleet::new();
    fleet.event(&spawned(1, 0, 7, "one"));
    fleet.event(&spawned(1, 0, 7, "one"));
    assert_eq!(fleet.children().len(), 1);
}

/// F4 — a child sits one level below the event that announced it.
#[test]
fn f4_a_child_is_one_level_below_the_event_that_announced_it() {
    let mut fleet = Fleet::new();
    fleet.event(&spawned(1, 0, 7, "one"));
    fleet.event(&spawned(7, 1, 9, "two"));
    assert_eq!(fleet.children()[0].depth, 1);
    assert_eq!(fleet.children()[1].depth, 2);
}

/// F4 — a child's own events find its row, and the root's do not.
#[test]
fn f4_a_childs_own_events_find_its_row() {
    let mut fleet = Fleet::new();
    fleet.event(&spawned(1, 0, 7, "one"));
    fleet.event(&spawned(1, 0, 8, "two"));

    // A draw on the child's own run, which is where io-harness emits it.
    fleet.event(&RunEvent::at_depth(
        7,
        2,
        1,
        EventKind::SpendDraw {
            tokens: 1_200,
            remaining: Some(8_000),
        },
    ));
    // And one on the root's, which belongs to no row.
    fleet.event(&RunEvent::new(
        1,
        2,
        EventKind::SpendDraw {
            tokens: 400,
            remaining: Some(7_600),
        },
    ));
    assert_eq!(fleet.children()[0].drawn, 1_200);
    assert_eq!(fleet.children()[1].drawn, 0);

    fleet.event(&RunEvent::at_depth(
        7,
        3,
        1,
        EventKind::Finished {
            outcome: "success".into(),
            steps: 3,
            tokens: 1_200,
        },
    ));
    assert_eq!(fleet.children()[0].state, State::Done);
    assert_eq!(
        fleet.children()[1].state,
        State::Working,
        "one child ending does not end its sibling",
    );

    // The ROOT finishing is not a child finishing.
    fleet.event(&RunEvent::new(
        1,
        9,
        EventKind::Finished {
            outcome: "success".into(),
            steps: 9,
            tokens: 5_000,
        },
    ));
    assert_eq!(fleet.children()[1].state, State::Working);
}

/// F4 — detaching is not ending.
#[test]
fn f4_a_detached_child_is_not_a_finished_one() {
    let mut fleet = Fleet::new();
    fleet.event(&spawned(1, 0, 7, "one"));
    fleet.event(&RunEvent::new(
        1,
        2,
        EventKind::ChildDetached {
            child_run_id: 7,
            after: Some(Duration::from_secs(30)),
        },
    ));
    assert_eq!(fleet.children()[0].state, State::Detached);
}

/// F4 — the fleet belongs to the run that reported it.
#[test]
fn f4_forgetting_a_run_forgets_its_fleet() {
    let mut fleet = Fleet::new();
    fleet.event(&spawned(1, 0, 7, "one"));
    fleet.event(&tier(1, 1, 0, 0));
    fleet.forget();
    assert!(fleet.is_empty());
    assert_eq!(fleet.selection(), None, "nothing is selected, rather than row zero");
}

/// F5 — the view opens over the composer and closes back to it, text intact.
#[test]
fn f5_the_view_opens_over_the_composer_and_gives_it_back() {
    let mut app = App::new(DARK, "a-model");
    for character in "note to self".chars() {
        app.key(key(KeyCode::Char(character)));
    }
    assert!(!app.fleet_open());

    // The key rather than the command, because the moment this is worth opening
    // is mid-turn and a slash command cannot be typed then.
    app.key(KeyEvent::new(KeyCode::Char('f'), KeyModifiers::CONTROL));
    assert!(app.fleet_open());

    app.key(key(KeyCode::Esc));
    assert!(!app.fleet_open());
    assert_eq!(
        app.composer.text(),
        "note to self",
        "the prompt is where it was left",
    );
}

/// F5 — the view is folded from events whether or not it is open.
///
/// A model built only while somebody is watching would start empty at the moment
/// it is wanted.
#[test]
fn f5_the_model_is_folded_while_the_view_is_shut() {
    let mut app = App::new(DARK, "a-model");
    app.event(&spawned(1, 0, 7, "read every file under src/"), Duration::ZERO);
    app.event(&tier(1, 1, 0, 0), Duration::ZERO);
    assert!(!app.fleet_open());
    assert_eq!(app.fleet.children().len(), 1);
}

/// F5 — the view draws inside the rows it is given, and the status line survives.
#[test]
fn f5_the_view_takes_the_composers_rows_and_not_the_status_line() {
    let mut app = App::new(DARK, "a-model");
    app.status.spend = Some((1_200, Some(8_000)));
    for (child, goal) in [(7, "read every file under src/"), (8, "and the tests")] {
        app.event(&spawned(1, 0, child, goal), Duration::ZERO);
    }
    app.event(&tier(1, 2, 5, 0), Duration::ZERO);
    app.toggle_fleet();

    let (mut screen, _) = support::screen_of(80, 24, 4);
    screen
        .draw(|frame| app.render(frame, frame.area()))
        .expect("frame");
    let viewport = screen.viewport_text();
    assert!(viewport.contains("5 queued"), "the tier line is first: {viewport:?}");
    assert!(viewport.contains("run 7"), "{viewport:?}");
    assert!(
        viewport.contains("spend 1.2k/9.2k"),
        "the status line stays under the view, so what the fan-out costs is \
         still on screen: {viewport:?}",
    );
    for row in viewport.lines() {
        assert!(
            row.chars().count() <= 80,
            "no row is wider than the terminal it was drawn for: {row:?}",
        );
    }
}

/// F5 — at eighty columns the goal is what gets cut, not the identity.
#[test]
fn f5_a_long_goal_is_cut_and_the_row_still_identifies_itself() {
    let mut fleet = Fleet::new();
    fleet.event(&spawned(
        1,
        0,
        7,
        "port the tokenizer, the error paths, and everything that reads either of \
         them, one at a time, without changing behaviour",
    ));
    let rows = fleet.rows(80, &DARK.glyphs);
    let row = &rows[0];
    assert!(row.chars().count() <= 80, "{row:?}");
    assert!(row.contains("run 7"), "the identity survives the cut: {row:?}");
    assert!(row.contains("working"), "{row:?}");
}

/// F5 — the marker moves under the arrows and stays inside the list.
#[test]
fn f5_the_marker_moves_and_cannot_leave_the_list() {
    let mut app = App::new(DARK, "a-model");
    for child in [7, 8, 9] {
        app.event(&spawned(1, 0, child, "work"), Duration::ZERO);
    }
    app.toggle_fleet();
    assert_eq!(app.fleet.selection(), Some(0));
    app.key(key(KeyCode::Down));
    app.key(key(KeyCode::Down));
    app.key(key(KeyCode::Down));
    assert_eq!(app.fleet.selection(), Some(2), "the marker stops at the last row");
    for _ in 0..5 {
        app.key(key(KeyCode::Up));
    }
    assert_eq!(app.fleet.selection(), Some(0));
}

/// F5 — the view sets a cursor, which is 0.6.0's gate and the one that regresses
/// quietly.
///
/// A frame that sets none makes ratatui hide the terminal cursor, which removes
/// the only focus indicator a screen reader has — at the moment the operator is
/// being asked to walk a list.
#[test]
fn f5_the_view_sets_a_cursor_on_the_marked_row() {
    let mut app = App::new(DARK, "a-model");
    app.event(&spawned(1, 0, 7, "read every file under src/"), Duration::ZERO);
    app.toggle_fleet();

    let (mut screen, recorder) = support::screen_of(80, 24, 4);
    screen
        .draw(|frame| app.render(frame, frame.area()))
        .expect("frame");
    let bytes = recorder.text();
    let shown = bytes.rfind("\x1b[?25h");
    let hidden = bytes.rfind("\x1b[?25l");
    assert!(
        shown.is_some() && !hidden.is_some_and(|at| at > shown.expect("shown")),
        "the fleet view left the terminal cursor hidden",
    );
}
