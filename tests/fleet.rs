//! F4 — the fleet counts tiers, and a queued child is a count and never a row.
//! F5 — the view opens over the composer, updates live, and costs no row it does
//! not have.
//! F9 — a child spawned from a roster entry that asks for its own worktree says
//! so, and no row names a directory.
//!
//! Everything here is driven by a synthetic `RunEvent` stream, which is the whole
//! point: the model has to be right for a tree that queues, drains, detaches and
//! resumes, and none of those shapes needs a provider to state.

mod support;

use std::time::Duration;

use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use io_cli::app::App;
use io_cli::fleet::{Fleet, State};
use io_cli::glyphs::{ASCII, UNICODE};
use io_cli::theme::DARK;
use io_harness::{AgentDef, Agents, EventKind, RunEvent};

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
    assert_eq!(
        fleet.tiers().len(),
        1,
        "one entry per tier: {:?}",
        fleet.tiers()
    );
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
    assert!(
        summary.contains("tier 1: 4 working, 0 queued, 1 done"),
        "{summary}"
    );
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
    assert_eq!(
        fleet.selection(),
        None,
        "nothing is selected, rather than row zero"
    );
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
    app.event(
        &spawned(1, 0, 7, "read every file under src/"),
        Duration::ZERO,
    );
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

    // **Four rows on purpose: this is the terminal that cannot give the view what
    // it asks for.** Since 0.32.0 the fleet states the rows it needs and a real
    // session grows to hold them — `o14_a_view_that_cannot_show_everything_says_
    // how_much_it_held_back` covers the demand — but what *this* test is about is
    // the layout under pressure: the view takes the composer's rows and leaves
    // the status line alone, whatever it has to drop to do it.
    let (mut screen, _) = support::screen_of(80, 24, 4);
    screen
        .draw(|frame| app.render(frame, frame.area()))
        .expect("frame");
    let viewport = screen.viewport_text();
    assert!(
        viewport.contains("5 queued"),
        "the tier line is first: {viewport:?}"
    );
    // **And what it could not draw is a count rather than a silence.** At four
    // rows there is no room for both children; through 0.31.0 the second simply
    // vanished, on the one surface whose whole job is saying what a fan-out is
    // doing.
    assert!(
        viewport.contains("more"),
        "rows were dropped without a word: {viewport:?}",
    );
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
    assert!(
        row.contains("run 7"),
        "the identity survives the cut: {row:?}"
    );
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
    assert_eq!(
        app.fleet.selection(),
        Some(2),
        "the marker stops at the last row"
    );
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
    app.event(
        &spawned(1, 0, 7, "read every file under src/"),
        Duration::ZERO,
    );
    app.toggle_fleet();

    let (mut screen, recorder) = support::screen_of(80, 24, 4);
    screen
        .draw(|frame| app.render(frame, frame.area()))
        .expect("frame");
    let bytes = recorder.text();
    let shown = bytes.rfind("\x1b[?25h");
    let hidden = bytes.rfind("\x1b[?25l");
    assert!(
        shown.is_some() && hidden.is_none_or(|at| at <= shown.expect("shown")),
        "the fleet view left the terminal cursor hidden",
    );
}

/// F5 — the view closes with the turn, and the tree it drew is still there.
///
/// A live run found this: with the view up when the turn ended, the composer
/// stayed hidden behind a tree that had stopped moving, and a session saying
/// `ready` with nowhere to type reads as one that has hung.
#[test]
fn f5_the_view_closes_when_the_turn_ends_and_the_model_survives() {
    let mut app = App::new(DARK, "a-model");
    app.started();
    app.event(
        &spawned(1, 0, 7, "read every file under src/"),
        Duration::ZERO,
    );
    app.toggle_fleet();
    assert!(app.fleet_open());

    app.finished();
    assert!(!app.fleet_open(), "the prompt comes back on its own");
    assert_eq!(
        app.fleet.children().len(),
        1,
        "and `/fleet` still has something to reopen",
    );
}

/// F5 — the conversation changing under the view forgets it.
///
/// `/resume`, `/fork` and a rewind each put a different run on screen. The same
/// rule `Status::forget_run` holds, for the same reason.
#[test]
fn f5_forgetting_a_run_forgets_the_fleet_and_shuts_the_view() {
    let mut app = App::new(DARK, "a-model");
    app.event(&spawned(1, 0, 7, "work"), Duration::ZERO);
    app.toggle_fleet();
    app.forget_fleet();
    assert!(!app.fleet_open());
    assert!(app.fleet.is_empty());
}

/// A fleet belongs to one turn, and the turn that clears it is the turn that
/// starts.
///
/// **Not the turn that ends, and the difference is the whole of it.** Clearing on
/// `finished` would throw away the tree at the exact moment the operator is most
/// likely to open `/fleet` and read what their fan-out did — which is why
/// `f5_the_view_closes_when_the_turn_ends_and_the_model_survives` above asserts the
/// opposite for `App::finished`, and why the two tests have to sit beside each
/// other. Clearing at the start keeps the record readable for as long as it is the
/// record of the last thing that happened, and no longer.
///
/// Sabotage: drop `self.fleet.forget()` from `App::started`. **Nothing else in
/// this suite fails.** Every test above builds its fleet inside one turn, so a
/// model that never clears agrees with all of them. What ships instead is a
/// session where one fan-out anywhere poisons the rest of the conversation: turn
/// two's `/fleet` draws turn one's agents and their mail as though they were
/// current — rows for children that finished minutes ago, under a pane whose whole
/// claim is that it shows what is running now — and `note_fleet`'s `is_empty`
/// early return is defeated for good, so every ordinary step of every later turn
/// pays a `run_root` and a `tree_addresses` query for a tree that is not there.
/// One of those is a lie on screen and the other is a cost with nothing to show
/// for it, and neither leaves a mark anyone would trace back here.
#[test]
fn f5_a_turn_starting_clears_the_previous_turns_fleet() {
    let mut app = App::new(DARK, "a-model");

    // Turn one fans out.
    app.started();
    app.event(
        &spawned(1, 0, 7, "read every file under src/"),
        Duration::ZERO,
    );
    app.event(&tier(1, 1, 0, 0), Duration::ZERO);
    app.finished();
    assert_eq!(
        app.fleet.children().len(),
        1,
        "the tree survives the turn it belonged to, which is the other half of \
         this rule",
    );

    // Turn two starts, and it starts with nothing.
    app.started();
    assert!(
        app.fleet.is_empty(),
        "turn one's agents are still in the model, so `/fleet` will draw them as \
         this turn's: {:?}",
        app.fleet.children(),
    );
    assert!(
        app.fleet.children().is_empty(),
        "the rows in particular, which are what reaches the screen",
    );
    assert!(
        app.fleet.tiers().is_empty(),
        "and the counts, which are what the summary line is built from",
    );
    assert_eq!(
        app.fleet.selection(),
        None,
        "nothing is marked, rather than a row zero that no longer exists",
    );
}

/// F5 — `Enter` with something typed is still `queue this`, with the pane open.
///
/// The pane is drawn *over the prompt*, not in front of the keyboard: `Ctrl+C`
/// still interrupts through it and the composer still takes typing, because the
/// moment this view is worth opening is mid-turn. `Enter` on a fleet row means
/// something only at an empty prompt — there is one row it can act on and nothing
/// half-written to lose — so with text in the composer it has to fall through.
///
/// Sabotage: drop the `if self.composer.is_empty()` guard from the `KeyCode::Enter`
/// arm of the fleet block in `App::key`, which is what this arm actually shipped
/// with. **The return value does not change** — a prompt queued mid-turn is
/// `Command::None` and a swallowed key is `Command::None` — so nothing a caller can
/// see distinguishes the two, and the assertion that bites is the queue's contents.
/// What the operator gets is a line they typed against a running turn, pressed
/// `Enter` on, and that is simply gone: no message, no change on screen, and the
/// pane is open at precisely the moment queueing a line is most likely.
#[test]
fn f5_enter_with_a_written_prompt_reaches_the_queue_through_the_open_view() {
    let mut app = App::new(DARK, "a-model");
    app.started();
    app.event(
        &spawned(1, 0, 7, "read every file under src/"),
        Duration::ZERO,
    );
    app.toggle_fleet();
    assert!(app.fleet_open());

    for character in "and then run the tests".chars() {
        app.key(key(KeyCode::Char(character)));
    }
    assert_eq!(
        app.composer.text(),
        "and then run the tests",
        "the pane is over the prompt, not in front of the keyboard",
    );

    app.key(key(KeyCode::Enter));

    assert_eq!(
        app.queued_prompts(),
        ["and then run the tests"],
        "the line was swallowed by a pane that had nothing to do with it",
    );
    assert!(
        app.composer.is_empty(),
        "and the prompt is clear for the next one, the way every other submit \
         leaves it",
    );
    assert!(
        app.fleet_open(),
        "queueing a line is not a reason to close the view that was being read",
    );
}

/// F5 — `Shift+Enter` writes a newline through the open view.
///
/// The same guard, on the key that has no other road at all. `Enter` at least had
/// a reading while the pane was up; a shifted `Enter` never means "act on a fleet
/// row" under any circumstances, so an arm that matched `KeyCode::Enter` without
/// looking at the modifiers took a key it could not possibly want.
///
/// Sabotage: the same missing `composer.is_empty()`. Under it a multi-line prompt
/// cannot be *written* while the pane is open — the second line never starts, the
/// cursor does not move, and the operator concludes the key is broken rather than
/// that a view they opened is eating it.
#[test]
fn f5_shift_enter_still_writes_a_newline_through_the_open_view() {
    let mut app = App::new(DARK, "a-model");
    app.toggle_fleet();
    for character in "first line".chars() {
        app.key(key(KeyCode::Char(character)));
    }
    app.key(KeyEvent::new(KeyCode::Enter, KeyModifiers::SHIFT));
    for character in "second line".chars() {
        app.key(key(KeyCode::Char(character)));
    }

    assert_eq!(
        app.composer.text(),
        "first line\nsecond line",
        "the shifted Enter never reached the composer, so a multi-line prompt \
         cannot be written while the view is open",
    );
}

/// The roster io-cli already hands to `Fleet::name` for role labelling: one entry
/// that asks for its own worktree, one that does not.
fn roster() -> Agents {
    Agents::new()
        .with(AgentDef::new("builder").with_worktree())
        .with(AgentDef::new("scout"))
}

/// F9 — the mark is derived from `contract.agents`, and only from there.
///
/// Two children, same tree, same shape of address; the only thing that separates
/// them is the roster entry behind each. A view that marked both, or neither,
/// would be reporting something other than what the contract says.
#[test]
fn f9_a_child_of_a_worktree_entry_is_marked_and_an_ordinary_one_is_not() {
    let mut fleet = Fleet::new();
    fleet.event(&spawned(1, 0, 7, "port the tokenizer"));
    fleet.event(&spawned(1, 0, 8, "look around"));
    fleet.name(
        &[("builder#7".to_string(), 7), ("scout#8".to_string(), 8)],
        &roster(),
    );
    assert!(fleet.children()[0].worktree, "{:?}", fleet.children()[0]);
    assert!(!fleet.children()[1].worktree, "{:?}", fleet.children()[1]);

    let rows = fleet.rows(80, &UNICODE);
    assert!(rows[0].contains("worktree"), "{:?}", rows[0]);
    assert!(
        !rows[1].contains("worktree"),
        "an entry that never asked for one is not marked: {:?}",
        rows[1],
    );
    // And the mark sits in front of the goal, so the goal is still the column
    // that gives way when the row is narrow.
    let head = rows[0].find("worktree").expect("the mark");
    let goal = rows[0].find("port the tokenizer").expect("the goal");
    assert!(head < goal, "{:?}", rows[0]);
}

/// F9 — no row names a directory.
///
/// The sabotage arm draws the derived `.worktrees/<slug>` path instead. It cannot
/// be right: io-harness writes a contained child's real path into its `runs` row,
/// no query selects it back out, and the deriver is private — so anything drawn
/// here is reconstructed from two copied private functions, which the note on
/// `DERIVED_MARK` in `src/fleet.rs` already forbids for exactly this reason. A
/// wrong role costs a label; a wrong path is somewhere an operator goes.
#[test]
fn f9_no_row_names_a_directory() {
    let mut fleet = Fleet::new();
    fleet.event(&spawned(1, 0, 7, "port the tokenizer"));
    fleet.event(&spawned(1, 0, 8, "look around"));
    fleet.name(
        &[("builder#7".to_string(), 7), ("scout#8".to_string(), 8)],
        &roster(),
    );
    for glyphs in [UNICODE, ASCII] {
        for row in fleet.rows(80, &glyphs) {
            assert!(!row.contains(".worktrees"), "{} {row:?}", glyphs.name);
            // No separator either: the goals above carry none, so any that turns
            // up was put there by the row.
            assert!(!row.contains('/'), "{} {row:?}", glyphs.name);
            assert!(!row.contains('\\'), "{} {row:?}", glyphs.name);
        }
    }
}

/// F9 — the mark is a word, so it is the same in both sets and fits eighty
/// columns with a long address and a long goal in front of it.
#[test]
fn f9_the_mark_is_ascii_and_the_row_still_fits_eighty_columns() {
    let mut fleet = Fleet::new();
    fleet.event(&spawned(
        1,
        0,
        7,
        "port the tokenizer, the error paths, and everything that reads either of \
         them, one at a time, without changing behaviour",
    ));
    fleet.name(&[("builder#7".to_string(), 7)], &roster());
    for glyphs in [UNICODE, ASCII] {
        let row = fleet.rows(80, &glyphs).remove(0);
        assert!(row.chars().count() <= 80, "{} {row:?}", glyphs.name);
        assert!(row.contains("worktree"), "{} {row:?}", glyphs.name);
        assert!(
            row.contains("builder#7"),
            "the identity survives the cut: {} {row:?}",
            glyphs.name,
        );
    }
    assert!(
        fleet.rows(80, &ASCII).remove(0).is_ascii(),
        "the mark degrades because it never needed a glyph",
    );
}

/// F9 — an address the roster cannot name is not marked, and a reload that drops
/// the entry unmarks the child rather than remembering it.
#[test]
fn f9_an_unknown_address_is_not_marked_and_the_mark_is_not_kept_stale() {
    let mut fleet = Fleet::new();
    fleet.event(&spawned(1, 0, 7, "one"));
    fleet.name(&[("left-hand".to_string(), 7)], &roster());
    assert!(!fleet.children()[0].worktree, "nothing was guessed");

    fleet.name(&[("builder#7".to_string(), 7)], &roster());
    assert!(fleet.children()[0].worktree);
    // `/agents` reloaded, and the entry no longer asks for one.
    fleet.name(
        &[("builder#7".to_string(), 7)],
        &Agents::new().with(AgentDef::new("builder")),
    );
    assert!(
        !fleet.children()[0].worktree,
        "the row was still saying what a definition used to say",
    );
}

/// **O14 — a fan-out too big for the terminal says how much it is not showing.**
///
/// Messages sort after every child, so they are the first thing off the bottom of
/// this view — and until 0.32.0 they went with no count at all, on the one surface
/// whose entire job is saying what a fan-out is doing. A view that quietly drew
/// two of nine children looked exactly like a fan-out with two children in it.
#[test]
fn o14_a_view_that_cannot_show_everything_says_how_much_it_held_back() {
    let mut app = App::new(DARK, "a-model");
    for child in 0..8 {
        app.event(
            &spawned(1, 0, child, "read every file under src/"),
            Duration::ZERO,
        );
    }
    app.event(&tier(1, 8, 0, 0), Duration::ZERO);
    app.toggle_fleet();

    // Deliberately smaller than the view asks for: this is the terminal that
    // cannot give it what it wants, which is the case the count exists for.
    let (mut screen, _) = support::screen_of(80, 24, 6);
    screen
        .draw(|frame| app.render(frame, frame.area()))
        .expect("frame");
    let viewport = screen.viewport_text();

    assert!(
        viewport.contains("more"),
        "eight children in a six-row viewport were dropped without a word: {viewport:?}",
    );
    // And the view still asked for enough to have shown them all.
    assert!(
        app.viewport_wanted(80, 24) > 6,
        "the view did not ask the viewport for the rows it needed",
    );
}
