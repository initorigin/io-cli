//! F3 — a queued line is recalled, edited, dropped and reordered, and prompt
//! history still works.
//!
//! `tests/queue.rs` owns the state and `tests/queue_surface.rs` owns the drawing;
//! this file owns the *verbs*. Everything here is about a surface that has stopped
//! being a list an operator reads and become one they drive, and about the one
//! thing that had to keep working while it did.
//!
//! That one thing is prompt history. `Up` at the first line of the composer has
//! recalled the previous prompt since it was documented, it is in the shipped key
//! table, and `tests/docs.rs` mirrors that table into the README. The queue's
//! arrows and the composer's arrows are the same two keys, so the binding is only
//! correct if it is scoped: **inside the open surface**, with an empty prompt.
//! Bound at the bare composer instead it would work perfectly for every operator
//! with something queued and silently cost prompt history to every operator
//! without — a feature nobody asked to trade away, broken by a release about a
//! different one. `f3_with_the_surface_closed_up_still_recalls_the_previous_prompt`
//! is the test that kills that.
//!
//! The verbs are asserted twice over, and on purpose. `queue::Cursor` is proved
//! directly, because the arithmetic of a mark that survives a queue mutating
//! underneath it is worth asserting without a keyboard in the way; the same verbs
//! are then proved through `App::key` and the rendered viewport, because a verb
//! nothing routes a key to is a verb no operator has.

mod support;

use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use io_cli::app::{App, Command, Mode};
use io_cli::glyphs::{ASCII, UNICODE};
use io_cli::queue::{self, Cursor, Put};
use io_cli::theme::DARK;
use io_harness::{EventKind, RunEvent};

fn key(code: KeyCode) -> KeyEvent {
    KeyEvent::new(code, KeyModifiers::NONE)
}

fn shifted(code: KeyCode) -> KeyEvent {
    KeyEvent::new(code, KeyModifiers::SHIFT)
}

/// Type a line and press `Enter`, the way an operator sends one.
fn send(app: &mut App, text: &str) -> Command {
    for character in text.chars() {
        app.key(key(KeyCode::Char(character)));
    }
    app.key(key(KeyCode::Enter))
}

/// Type a line without sending it.
fn typed(app: &mut App, text: &str) {
    for character in text.chars() {
        app.key(key(KeyCode::Char(character)));
    }
}

/// Empty the prompt the way an operator does: backspace until there is nothing
/// left. More presses than there are characters, because a press at an empty
/// prompt is a no-op and the count is not the assertion.
fn erase(app: &mut App) {
    for _ in 0..64 {
        app.key(key(KeyCode::Backspace));
    }
}

/// A session with a turn genuinely in flight. `tests/queue_surface.rs`'s fixture,
/// with the streamed tail that keeps `App::undoable` false.
fn running() -> App {
    let mut app = App::new(DARK, "a-model");
    started(&mut app);
    app
}

/// Put the session into a running turn.
fn started(app: &mut App) {
    app.started();
    app.event(
        &RunEvent::new(
            1,
            1,
            EventKind::Started {
                goal: "count the tests".into(),
                provider: "test".into(),
            },
        ),
        std::time::Duration::ZERO,
    );
    app.event(
        &RunEvent::new(
            1,
            1,
            EventKind::Token {
                text: "STREAMING".into(),
            },
        ),
        std::time::Duration::ZERO,
    );
}

/// A running session with `count` prompts typed into it, in order.
fn with_queue(count: usize) -> App {
    let mut app = running();
    for at in 0..count {
        assert_eq!(
            send(&mut app, &format!("queued prompt {at}")),
            Command::None,
            "a prompt typed mid-turn is kept rather than sent",
        );
    }
    app
}

/// The viewport's rows as text, blanks kept: a row's *index* is the assertion.
fn rows_at(app: &mut App, viewport: u16) -> Vec<String> {
    let (mut screen, _recorder) = support::screen_of(80, 40, viewport);
    screen
        .draw(|frame| app.render(frame, frame.area()))
        .expect("frame");
    screen
        .viewport_text()
        .split('\n')
        .map(str::to_string)
        .collect()
}

/// Where the row holding `needle` is, or a panic naming what was on screen.
fn row_of(rows: &[String], needle: &str) -> usize {
    rows.iter()
        .position(|row| row.contains(needle))
        .unwrap_or_else(|| panic!("no row holds {needle:?}: {rows:#?}"))
}

/// The queued line the marker is on, as the rendered row shows it.
fn marked(rows: &[String]) -> Option<&str> {
    rows.iter()
        .find(|row| row.starts_with(DARK.glyphs.marker))
        .map(|row| row.trim_end())
}

/// The text in the prompt, as the rendered row shows it.
fn prompt_row(rows: &[String]) -> String {
    let at = row_of(rows, io_cli::composer::PROMPT.trim_end());
    rows[at].trim_end().to_string()
}

/// Twelve rows leaves the composer four to lend — see `tests/queue_surface.rs`,
/// which is where that arithmetic is argued. Enough that a queue of three is
/// drawn a row at a time and a mark can be read off the frame.
const TALL: u16 = 12;

fn queue_of(count: usize) -> Vec<String> {
    (0..count).map(|at| format!("prompt {at}")).collect()
}

// ---------------------------------------------------------------------------
// The verbs, without a keyboard
// ---------------------------------------------------------------------------

/// The mark enters the list from the row nearest the prompt, moves, and stops at
/// both ends.
#[test]
fn the_mark_enters_from_the_bottom_and_clamps_at_both_ends() {
    let mut cursor = Cursor::default();
    assert_eq!(
        cursor.selection(3),
        None,
        "nothing is marked until an arrow"
    );

    assert!(
        !cursor.move_by(1, 3),
        "Down with nothing marked is not the queue's key: below the queue is the \
         composer, and the operator is already in it",
    );
    assert_eq!(cursor.selection(3), None);

    assert!(cursor.move_by(-1, 3));
    assert_eq!(
        cursor.selection(3),
        Some(2),
        "Up enters at the line nearest the prompt, which is the row above the caret",
    );
    cursor.move_by(-1, 3);
    cursor.move_by(-1, 3);
    assert_eq!(cursor.selection(3), Some(0));
    cursor.move_by(-1, 3);
    assert_eq!(
        cursor.selection(3),
        Some(0),
        "the top is the top, not a wrap"
    );
    cursor.move_by(1, 3);
    assert_eq!(cursor.selection(3), Some(1));

    // A queue that shrank under the mark. The read clamps rather than indexing
    // past the end — the drain takes the oldest between turns, and a mark held
    // across that must not be a panic in a renderer.
    cursor.move_by(-1, 3);
    cursor.move_by(1, 3);
    cursor.move_by(1, 3);
    assert_eq!(cursor.selection(3), Some(2));
    assert_eq!(cursor.selection(2), Some(1));
    assert_eq!(cursor.selection(0), None);
}

/// The mark travels with the line it is on, which is what makes three presses
/// move a line three places.
#[test]
fn a_reordered_line_carries_the_mark_with_it() {
    let mut waiting = queue_of(3);
    let mut cursor = Cursor::default();
    cursor.move_by(-1, waiting.len());
    assert_eq!(cursor.selection(waiting.len()), Some(2));

    assert!(cursor.reorder(-1, &mut waiting));
    assert_eq!(waiting, ["prompt 0", "prompt 2", "prompt 1"]);
    assert_eq!(
        cursor.selection(waiting.len()),
        Some(1),
        "the mark followed the text: a mark that stayed on the slot would move a \
         different line on the next press",
    );

    assert!(cursor.reorder(-1, &mut waiting));
    assert_eq!(waiting, ["prompt 2", "prompt 0", "prompt 1"]);
    assert!(
        !cursor.reorder(-1, &mut waiting),
        "the first line has nowhere further up to go",
    );
    assert_eq!(
        waiting,
        ["prompt 2", "prompt 0", "prompt 1"],
        "and nothing moved"
    );

    assert!(cursor.reorder(1, &mut waiting));
    assert_eq!(waiting, ["prompt 0", "prompt 2", "prompt 1"]);

    // Nothing marked is nothing to move.
    let mut fresh = Cursor::default();
    assert!(!fresh.reorder(-1, &mut waiting));
}

/// A line goes out of the queue to be edited and comes back at its own position.
#[test]
fn an_edited_line_goes_back_where_it_came_from() {
    let mut waiting = queue_of(3);
    let mut cursor = Cursor::default();
    cursor.move_by(-1, waiting.len());
    cursor.move_by(-1, waiting.len());
    assert_eq!(cursor.selection(waiting.len()), Some(1));

    assert_eq!(cursor.take(&mut waiting).as_deref(), Some("prompt 1"));
    assert_eq!(
        waiting,
        ["prompt 0", "prompt 2"],
        "it is OUT of the queue while it is being edited — one copy of a prompt \
         in the session, so a turn that ends mid-edit cannot run it twice",
    );
    assert_eq!(cursor.editing(), Some(1));
    assert!(
        cursor.take(&mut waiting).is_none(),
        "a second take would be a second line out of the queue for one prompt",
    );

    assert_eq!(
        cursor.put_back(&mut waiting, "prompt 1, edited"),
        Some(Put::Kept(1)),
    );
    assert_eq!(waiting, ["prompt 0", "prompt 1, edited", "prompt 2"]);
    assert_eq!(cursor.editing(), None);
    assert_eq!(
        cursor.put_back(&mut waiting, "again"),
        None,
        "with no edit in flight the key was never the surface's",
    );
}

/// The position survives the queue draining underneath the edit.
#[test]
fn an_edit_that_outlived_a_drain_goes_back_at_the_end() {
    let mut waiting = queue_of(3);
    let mut cursor = Cursor::default();
    cursor.move_by(-1, waiting.len());
    assert_eq!(cursor.take(&mut waiting).as_deref(), Some("prompt 2"));

    // The driver took the two in front of it while the edit was open.
    waiting.clear();
    assert_eq!(
        cursor.put_back(&mut waiting, "prompt 2"),
        Some(Put::Kept(0))
    );
    assert_eq!(
        waiting,
        ["prompt 2"],
        "past the end is the end, which is still 'after everything in front of it'",
    );
}

/// Erased and sent back is dropped, and the text comes back so the operator can
/// be told what went.
#[test]
fn an_emptied_line_is_dropped_rather_than_queued_as_an_empty_turn() {
    let mut waiting = queue_of(3);
    let mut cursor = Cursor::default();
    cursor.move_by(-1, waiting.len());
    cursor.move_by(-1, waiting.len());
    cursor.take(&mut waiting);

    for erased in ["", "   ", "\n\t "] {
        let mut queue = waiting.clone();
        let mut aside = cursor.clone();
        assert_eq!(
            aside.put_back(&mut queue, erased),
            Some(Put::Dropped("prompt 1".to_string())),
            "an empty prompt is not a prompt, so there is no turn to schedule",
        );
        assert_eq!(queue, ["prompt 0", "prompt 2"]);
        assert_eq!(aside.editing(), None);
    }
}

/// `Esc` puts the line back as it was taken, not as it was left.
#[test]
fn cancelling_an_edit_restores_the_text_it_started_with() {
    let mut waiting = queue_of(3);
    let mut cursor = Cursor::default();
    cursor.move_by(-1, waiting.len());
    cursor.move_by(-1, waiting.len());
    cursor.take(&mut waiting);

    assert_eq!(cursor.cancel(&mut waiting), Some(1));
    assert_eq!(
        waiting,
        ["prompt 0", "prompt 1", "prompt 2"],
        "a cancel that kept the half-edited text would be a cancel that still \
         changed the queue",
    );
    assert_eq!(cursor.cancel(&mut waiting), None);
}

/// A turn ending under an edit forgets the position and leaves the line alone.
#[test]
fn a_lapsed_edit_is_forgotten_and_never_re_queued() {
    let mut waiting = queue_of(2);
    let mut cursor = Cursor::default();
    cursor.move_by(-1, waiting.len());
    cursor.take(&mut waiting);

    cursor.lapsed();
    assert_eq!(cursor.editing(), None);
    assert_eq!(cursor.selection(waiting.len()), None);
    assert_eq!(
        waiting,
        ["prompt 0"],
        "the line stays in the composer where the operator can see it — putting \
         it back would queue a second copy behind the drain that is starting",
    );
}

// ---------------------------------------------------------------------------
// The mark, drawn
// ---------------------------------------------------------------------------

/// The marker column is on every row or on none of them, and the rows nobody has
/// touched are the rows this surface always drew.
#[test]
fn the_marker_is_a_column_on_every_row_or_on_no_row() {
    let waiting = queue_of(3);

    let plain = queue::rows_for(&waiting, None, 40, 4, &UNICODE);
    assert_eq!(
        plain,
        queue::rows(&waiting, 40, 4, &UNICODE),
        "with nothing marked this is the surface it has always been",
    );
    assert!(plain[0].starts_with("1. "), "{plain:#?}");

    let marked = queue::rows_for(&waiting, Some(1), 40, 4, &UNICODE);
    assert!(marked[1].starts_with(UNICODE.marker), "{marked:#?}");
    assert!(marked[0].starts_with("  1. "), "{marked:#?}");
    assert!(marked[2].starts_with("  3. "), "{marked:#?}");
    // Two cells in both sets is what keeps every label in one column: a column
    // drawn only on the marked row would shift that row out of line with its
    // neighbours, which reads as the row having changed rather than been chosen.
    for row in &marked {
        assert!(
            row.chars().nth(2).is_some_and(|at| at.is_ascii_digit()),
            "the number starts in the same column on every row: {row:?}",
        );
    }

    // And an index that outlived its line is clamped rather than a panic.
    let stale = queue::rows_for(&waiting, Some(99), 40, 4, &UNICODE);
    assert!(stale[2].starts_with(UNICODE.marker), "{stale:#?}");
}

/// The window follows the mark when the queue is longer than the rows, and the
/// numbers stay absolute while it does.
#[test]
fn the_rows_scroll_to_keep_the_marked_line_on_screen() {
    let waiting = queue_of(9);

    let top = queue::rows_for(&waiting, None, 40, 4, &UNICODE);
    assert!(top[0].contains("1. prompt 0"), "{top:#?}");

    let deep = queue::rows_for(&waiting, Some(7), 40, 4, &UNICODE);
    assert_eq!(deep.len(), 4, "it draws into the rows it was given");
    assert!(
        deep.iter().any(|row| row.starts_with(UNICODE.marker)),
        "a mark the rows never show is an arrow that appears to do nothing: {deep:#?}",
    );
    assert!(deep[2].contains("8. prompt 7"), "{deep:#?}");
    assert!(
        deep[0].contains("6. prompt 5"),
        "the numbers are the run order, not the row: a window that renumbered \
         itself from one would say the marked line runs first. {deep:#?}",
    );
    // **One, not six, and this assertion is the defect it was written over.** The
    // row sits UNDER the window, so it counts what is under it; with the window
    // scrolled to 6, 7, 8 only line 9 is below, and the five that are above are
    // announced by the numbering starting at `6.` rather than by a count below
    // them. The first draft said six — everything unshown — which was right only
    // while nothing was marked and the window began at the top.
    assert!(deep[3].contains("1 more"), "{deep:#?}");

    // The window never runs past the end.
    let last = queue::rows_for(&waiting, Some(8), 40, 4, &UNICODE);
    assert!(last[2].contains("9. prompt 8"), "{last:#?}");
}

/// At the one row a running turn can spare, the row is the marked line.
///
/// The surface's whole visible budget at `term::VIEWPORT_HEIGHT` is a single row.
/// A mark that the row did not follow would be a selection an operator cannot
/// see, moved by arrows that appear to do nothing — which is worse than no
/// selection at all.
#[test]
fn at_one_row_the_row_is_the_marked_line() {
    let waiting = queue_of(3);

    let quiet = queue::rows_for(&waiting, None, 40, 1, &UNICODE);
    assert_eq!(quiet.len(), 1);
    assert!(quiet[0].contains("1. prompt 0"), "{quiet:#?}");
    assert!(quiet[0].contains("2 more"), "{quiet:#?}");

    let marked = queue::rows_for(&waiting, Some(2), 40, 1, &UNICODE);
    assert_eq!(marked.len(), 1);
    assert!(marked[0].starts_with(UNICODE.marker), "{marked:#?}");
    assert!(marked[0].contains("3. prompt 2"), "{marked:#?}");
    assert!(
        marked[0].contains("2 more"),
        "the count was never about what is below the row, only about what is not \
         on it: {marked:#?}",
    );
}

/// The mark draws in the ASCII set too, and no row outgrows its width.
#[test]
fn a_marked_row_stays_ascii_and_inside_its_width() {
    let waiting = queue_of(9);
    let rows = queue::rows_for(&waiting, Some(4), 24, 4, &ASCII);
    assert!(rows.iter().all(|row| row.is_ascii()), "{rows:#?}");
    assert!(
        rows.iter().any(|row| row.starts_with(ASCII.marker)),
        "{rows:#?}"
    );

    let long = vec!["a prompt long enough to need cutting at any width".to_string(); 4];
    for width in [10_u16, 24, 80] {
        for row in queue::rows_for(&long, Some(2), width, 4, &UNICODE) {
            assert!(
                row.chars().count() <= usize::from(width),
                "a row of {} characters at width {width}: {row:?}",
                row.chars().count(),
            );
        }
    }
}

// ---------------------------------------------------------------------------
// F3 — through the keyboard, on the frame
// ---------------------------------------------------------------------------

/// F3 — `Up` and `Down` move the selection inside the surface.
#[test]
fn f3_up_and_down_move_the_selection_inside_the_surface() {
    let mut app = with_queue(3);
    assert!(app.queue_open());
    assert!(
        marked(&rows_at(&mut app, TALL)).is_none(),
        "a surface nobody has touched draws no mark",
    );

    assert_eq!(app.key(key(KeyCode::Up)), Command::None);
    let rows = rows_at(&mut app, TALL);
    assert_eq!(
        marked(&rows).map(|row| row.contains("queued prompt 2")),
        Some(true),
        "Up enters at the line nearest the prompt: {rows:#?}",
    );
    assert_eq!(
        prompt_row(&rows),
        io_cli::composer::PROMPT.trim_end(),
        "and it is the queue that moved, not the composer: the arrow inside the \
         surface must not put a recalled prompt in the prompt. {rows:#?}",
    );

    app.key(key(KeyCode::Up));
    assert_eq!(
        marked(&rows_at(&mut app, TALL)).map(|row| row.contains("queued prompt 1")),
        Some(true),
    );
    app.key(key(KeyCode::Down));
    assert_eq!(
        marked(&rows_at(&mut app, TALL)).map(|row| row.contains("queued prompt 2")),
        Some(true),
    );

    assert_eq!(
        app.queued_prompts().len(),
        3,
        "moving a mark moves nothing else",
    );
}

/// F3 — the sabotage arm: the queue's `Up` bound at the bare composer.
///
/// **This is the test that kills it.** With the surface shut, `Up` at the first
/// line of the composer is prompt history and has been since it was documented —
/// it is in `commands::KEYS`, which `tests/docs.rs` mirrors into the README. A
/// binding that moved a queue selection here would leave the operator's own
/// previous prompt unreachable, in a release that was about a different feature
/// entirely, and would do it *silently*: the surface is not on screen, so there is
/// nothing to watch the arrow move.
///
/// The queue is deliberately non-empty. A sabotage that only checked "is anything
/// queued" would pass against an empty one.
#[test]
fn f3_with_the_surface_closed_up_still_recalls_the_previous_prompt() {
    let mut app = App::new(DARK, "a-model");
    assert_eq!(
        send(&mut app, "the earlier prompt"),
        Command::Submit("the earlier prompt".to_string()),
    );
    started(&mut app);
    send(&mut app, "queued prompt 0");
    send(&mut app, "queued prompt 1");

    assert_eq!(app.key(key(KeyCode::Esc)), Command::None);
    assert!(!app.queue_open(), "the surface is shut and drawing nothing");
    assert_eq!(
        app.queued_prompts().len(),
        2,
        "and the queue is still there"
    );

    assert_eq!(app.key(key(KeyCode::Up)), Command::None);
    let rows = rows_at(&mut app, TALL);
    assert!(
        prompt_row(&rows).contains("queued prompt 1"),
        "Up at the first line of a shut surface is prompt history, which is what \
         it has done since it was documented — and the newest entry is the line \
         submitted last, queued or not: {rows:#?}",
    );

    // And the walk keeps going, which a mark being moved instead could not fake.
    app.key(key(KeyCode::Up));
    app.key(key(KeyCode::Up));
    let rows = rows_at(&mut app, TALL);
    assert!(
        prompt_row(&rows).contains("the earlier prompt"),
        "three presses reach the prompt sent before the turn began: {rows:#?}",
    );
    assert_eq!(
        app.queued_prompts().len(),
        2,
        "and it moved nothing in the queue",
    );
    assert!(
        marked(&rows).is_none(),
        "nor marked a row of a surface that is not on screen: {rows:#?}",
    );
}

/// F3 — a line is recalled into the composer, edited, and put back at its own
/// position, without becoming a turn of its own.
#[test]
fn f3_a_line_is_edited_in_the_composer_and_put_back_at_its_own_position() {
    let mut app = with_queue(3);
    app.key(key(KeyCode::Up));
    app.key(key(KeyCode::Up));

    assert_eq!(
        app.key(key(KeyCode::Enter)),
        Command::None,
        "Enter at an empty prompt takes the marked line into the composer",
    );
    assert_eq!(
        app.queued_prompts(),
        ["queued prompt 0", "queued prompt 2"],
        "the line is out of the queue while it is being edited",
    );
    let rows = rows_at(&mut app, TALL);
    assert!(
        prompt_row(&rows).contains("queued prompt 1"),
        "and it is in the prompt, where it can be seen: {rows:#?}",
    );

    typed(&mut app, ", edited");
    assert_eq!(
        app.key(key(KeyCode::Enter)),
        Command::None,
        "finishing an edit is not sending a turn: a Submit here would run the \
         line the operator was still writing",
    );
    assert_eq!(
        app.queued_prompts(),
        [
            "queued prompt 0",
            "queued prompt 1, edited",
            "queued prompt 2"
        ],
        "back at its own position — an edit that appended would have reordered \
         the next few turns as the price of fixing a typo",
    );
    assert_eq!(
        app.mode(),
        Mode::Running,
        "and the turn underneath is untouched"
    );
    let rows = rows_at(&mut app, TALL);
    assert_eq!(
        prompt_row(&rows),
        io_cli::composer::PROMPT.trim_end(),
        "the composer is empty again: {rows:#?}",
    );
}

/// F3 — an edit never starts on top of a half-typed prompt.
///
/// The composer is shared with the line being written, and there is exactly one
/// of it. The surface's answer is to not claim `Enter` while there is text in the
/// prompt: the keystroke goes on meaning what it has meant all release — queue
/// this line — and the operator's half-typed prompt is kept rather than
/// overwritten by a queued one.
#[test]
fn f3_an_edit_never_starts_on_top_of_a_half_typed_prompt() {
    let mut app = with_queue(2);
    app.key(key(KeyCode::Up));
    typed(&mut app, "half typed");

    let before = rows_at(&mut app, TALL);
    let marked_before = marked(&before).map(str::to_string);
    assert_eq!(
        app.key(key(KeyCode::Up)),
        Command::None,
        "the arrow is the composer's while there is something in it",
    );
    // **The surface did not take it — the composer did, and it did what it has
    // always done.** `Up` at the first line recalls the previous prompt and stashes
    // the draft, which is shipped behaviour the key table documents; `Down` brings
    // the draft back. So what F3 is owed here is not "the text is untouched" — the
    // first draft of this test asserted that and failed against a feature older
    // than the queue — but that the QUEUE'S mark did not move. The key went to one
    // owner, and it was not this surface.
    let rows = rows_at(&mut app, TALL);
    assert_eq!(
        marked(&rows).map(str::to_string),
        marked_before,
        "the surface took an arrow that belonged to the composer: {rows:#?}",
    );
    app.key(key(KeyCode::Down));
    let rows = rows_at(&mut app, TALL);
    assert!(
        prompt_row(&rows).contains("half typed"),
        "and the composer's own recall gave the draft back: {rows:#?}",
    );

    assert_eq!(app.key(key(KeyCode::Enter)), Command::None);
    assert_eq!(
        app.queued_prompts(),
        ["queued prompt 0", "queued prompt 1", "half typed"],
        "Enter with text in the prompt queues that text, exactly as it did before \
         the surface had any keys at all",
    );

    // And now that the prompt is empty, the same key takes the marked line.
    app.key(key(KeyCode::Enter));
    assert_eq!(
        app.queued_prompts(),
        ["queued prompt 0", "half typed"],
        "the mark stayed on the line it was on while the queue grew under it",
    );
}

/// F3 — a line is dropped: erased in the composer and sent back to nothing.
#[test]
fn f3_a_queued_line_is_dropped() {
    let mut app = with_queue(3);
    app.key(key(KeyCode::Up));
    app.key(key(KeyCode::Up));
    app.key(key(KeyCode::Enter));
    assert_eq!(app.queued_prompts(), ["queued prompt 0", "queued prompt 2"]);

    erase(&mut app);
    assert_eq!(
        app.key(key(KeyCode::Enter)),
        Command::None,
        "an emptied line is not submitted either",
    );
    assert_eq!(
        app.queued_prompts(),
        ["queued prompt 0", "queued prompt 2"],
        "it is gone: an empty prompt is not a prompt, so there is no turn left to \
         schedule for it",
    );
    assert_eq!(app.mode(), Mode::Running);
    let rows = rows_at(&mut app, TALL);
    // **The queue no longer lists it, and the notice names it.** Those are two
    // different jobs and the first draft of this test conflated them: it swept the
    // whole frame for the text and failed on io-cli's own `dropped "…"` sentence.
    // A drop that went unnamed would be the worse product — the operator erased a
    // line and pressed a key, and what they get back is which line went and how
    // many are left.
    let listed: Vec<&String> = rows
        .iter()
        .take_while(|row| !row.contains(io_cli::composer::PROMPT.trim_end()))
        .collect();
    assert!(
        !listed.iter().any(|row| row.contains("queued prompt 1")),
        "the queue above the composer no longer lists it: {rows:#?}",
    );
    let notice = app
        .status
        .notice
        .as_ref()
        .map(|(_, text)| text.clone())
        .unwrap_or_default();
    assert!(
        notice.contains("dropped") && notice.contains("queued prompt 1"),
        "and the operator is told which line went: {notice:?}",
    );
}

/// F3 — `Esc` during an edit puts the line back exactly as it was taken.
///
/// 0.13.1's rule, in the one place this release can honour it: an erase is
/// undoable where the undo is cheap, and one string for the length of one edit is
/// as cheap as undo gets. It is also why the drop above is a deliberate act
/// rather than a slip — everything up to the `Enter` is `Esc`-able.
#[test]
fn f3_esc_during_an_edit_puts_the_line_back_unchanged() {
    let mut app = with_queue(3);
    app.key(key(KeyCode::Up));
    app.key(key(KeyCode::Up));
    app.key(key(KeyCode::Enter));
    erase(&mut app);
    typed(&mut app, "something else entirely");

    assert_eq!(app.key(key(KeyCode::Esc)), Command::None);
    assert_eq!(
        app.queued_prompts(),
        ["queued prompt 0", "queued prompt 1", "queued prompt 2"],
        "cancelled means the queue is as it was, not as the half-edit left it",
    );
    assert_eq!(
        app.mode(),
        Mode::Running,
        "the Esc that answered the edit did not reach the turn",
    );
    let rows = rows_at(&mut app, TALL);
    assert_eq!(
        prompt_row(&rows),
        io_cli::composer::PROMPT.trim_end(),
        "and the composer is clear for the next line: {rows:#?}",
    );
}

/// F3 — two lines swap order, and the mark travels with the line.
#[test]
fn f3_two_queued_lines_swap_order() {
    let mut app = with_queue(3);
    app.key(key(KeyCode::Up));
    app.key(key(KeyCode::Up));

    assert_eq!(app.key(shifted(KeyCode::Up)), Command::None);
    assert_eq!(
        app.queued_prompts(),
        ["queued prompt 1", "queued prompt 0", "queued prompt 2"],
        "the marked line moved one place up the run order",
    );
    let rows = rows_at(&mut app, TALL);
    assert_eq!(
        marked(&rows).map(|row| row.contains("queued prompt 1")),
        Some(true),
        "and the mark went with it, so a second press moves the same line: {rows:#?}",
    );
    assert!(
        row_of(&rows, "queued prompt 1") < row_of(&rows, "queued prompt 0"),
        "the rows are drawn in the new order: {rows:#?}",
    );

    assert_eq!(app.key(shifted(KeyCode::Down)), Command::None);
    assert_eq!(
        app.queued_prompts(),
        ["queued prompt 0", "queued prompt 1", "queued prompt 2"],
        "and back",
    );
    assert_eq!(app.mode(), Mode::Running, "reordering runs nothing");
}

/// F3 — the turn ending under an edit leaves the line in the prompt rather than
/// running it or losing it.
///
/// The one question a shared composer has to answer out loud. The line is out of
/// the queue, so the drain cannot run it; it is in the prompt, so the operator can
/// see it and send it; and the position it came from is forgotten, because a slot
/// remembered across a drain points at somebody else's line.
#[test]
fn f3_a_turn_ending_under_an_edit_leaves_the_line_in_the_prompt() {
    let mut app = with_queue(2);
    app.key(key(KeyCode::Up));
    app.key(key(KeyCode::Enter));
    assert_eq!(app.queued_prompts(), ["queued prompt 0"]);

    app.finished();
    assert_eq!(app.mode(), Mode::Idle);
    assert!(!app.queue_open(), "no turn to be queued behind");
    let rows = rows_at(&mut app, TALL);
    assert!(
        prompt_row(&rows).contains("queued prompt 1"),
        "the line being edited is still in the prompt: {rows:#?}",
    );
    assert_eq!(
        app.queued_prompts(),
        ["queued prompt 0"],
        "and it was not put back behind the drain, which would run it twice",
    );

    // At an idle prompt it is an ordinary line again, and Enter sends it.
    assert_eq!(
        app.key(key(KeyCode::Enter)),
        Command::Submit("queued prompt 1".to_string()),
    );
}
