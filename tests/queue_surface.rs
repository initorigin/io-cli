//! F2 — the queue is on screen while the turn runs, and it is not a modal.
//! N2 — the viewport does not grow, whatever is queued.
//!
//! `tests/queue.rs` owns the state: what is captured, in what order, and how the
//! driver drains it. This file owns the *surface*, and the two criteria it
//! carries are both about what a surface must refuse to do.
//!
//! F2 is a refusal of the keyboard. Three surfaces in this product are modal —
//! an approval, a question and a plan — and every one of them is modal because a
//! run is stopped inside a harness callback waiting for the answer. A queue
//! stops nothing, so `App::modal()` must not learn about it: the sabotage that
//! adds it there produces a list that swallows `Ctrl+C`, and the operator's own
//! stop key is answered by a surface they never asked to open.
//!
//! N2 is a refusal of a row. The viewport is subtracted from the terminal and
//! the transcript is the terminal's own scrollback, so a surface that claimed a
//! row per queued line would walk the conversation upward one row for every line
//! typed against it. The rows are borrowed instead, and the frame is the same
//! height with nine prompts waiting as with none: the composer's spare rows
//! where a taller terminal has any — the way `Fleet::render` takes them — and,
//! at the height a running turn is actually drawn at, the blank above the
//! activity line, which the layout's own argument describes as carrying
//! nothing. That is `US-IO-CLI-0.17.0-I02`, and the reason it exists is that
//! the composer's allowance at that height *is* the composer's floor.
//!
//! Asserted on the rendered viewport rather than on the rectangles wherever a
//! reader's answer differs from the renderer's arithmetic: a test that recomputed
//! the offsets would agree with `App::render` by construction.

mod support;

use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use io_cli::app::{App, Command, Mode};
use io_cli::glyphs::{ASCII, UNICODE};
use io_cli::queue;
use io_cli::theme::DARK;
use io_harness::{EventKind, RunEvent};

fn key(code: KeyCode) -> KeyEvent {
    KeyEvent::new(code, KeyModifiers::NONE)
}

/// Type a line and press `Enter`, the way an operator sends one.
fn send(app: &mut App, text: &str) -> Command {
    for character in text.chars() {
        app.key(key(KeyCode::Char(character)));
    }
    app.key(key(KeyCode::Enter))
}

/// A session with a turn genuinely in flight: the mode, the harness's `Started`,
/// and a streamed tail with no newline on it.
///
/// The tail is not decoration. `App::undoable` is true for a turn that has done
/// nothing yet, and the stop key *abandons* such a turn rather than interrupting
/// it — so a test that asserted `Ctrl+C` still interrupts would otherwise be
/// asserting it against a turn no interrupt was ever reachable for.
fn running(theme: io_cli::theme::Theme) -> App {
    let mut app = App::new(theme, "a-model");
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
    app
}

/// A running session with `queued` prompts typed into it, in order.
fn with_queue(count: usize) -> App {
    let mut app = running(DARK);
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

/// A viewport tall enough that the composer's allowance has rows to spare.
///
/// Twelve rows leaves the composer five — the streaming row, the blank, the
/// activity line, the rule and three rows of footer take the rest — of which one
/// is the composer's own floor and four can be lent. It is not
/// `term::VIEWPORT_HEIGHT`, and the difference is a finding rather than a
/// convenience: at the running viewport there are no spare rows at all and the
/// surface is drawn in the borrowed blank instead, with one row rather than
/// four. See `n2_the_surface_is_visible_at_the_running_viewport_and_costs_it_no_height`.
const TALL: u16 = 12;

// ---------------------------------------------------------------------------
// F2 — on screen, in order, and not in front of the keyboard
// ---------------------------------------------------------------------------

/// F2 — the queued lines are drawn above the composer, in the order they were
/// sent.
#[test]
fn f2_the_queued_lines_are_drawn_above_the_composer_in_send_order() {
    let mut app = with_queue(3);
    assert!(app.queue_open(), "queueing a line opens the surface");

    let rows = rows_at(&mut app,TALL);
    let first = row_of(&rows, "queued prompt 0");
    let second = row_of(&rows, "queued prompt 1");
    let third = row_of(&rows, "queued prompt 2");
    let composer = row_of(&rows, io_cli::composer::PROMPT.trim_end());

    assert!(
        first < second && second < third,
        "the queue is drawn in the order it was typed, which is the order it \
         runs in: {rows:#?}"
    );
    assert!(
        third < composer,
        "the whole queue sits above the prompt: a line still being typed has not \
         been sent, so it belongs under the ones that have. {rows:#?}"
    );
    // The status line survives underneath, exactly as it does under the fleet
    // view: the surface takes the composer's rows and nobody else's.
    assert!(
        rows.len() == usize::from(TALL),
        "the surface drew outside the frame it was given: {rows:#?}"
    );
}

/// F2's sabotage arm: adding the queue to `App::modal()`.
///
/// **This is the test that kills it.** A modal surface takes the whole viewport
/// and takes the keyboard with it, and the one key it is allowed to let through
/// is `Ctrl+C`. Nothing is blocked by a queue — the turn is still streaming and
/// the composer is still taking typing — so the predicate that names the three
/// genuinely blocked surfaces must not learn a fourth.
#[test]
fn f2_the_queue_surface_is_not_modal_and_ctrl_c_still_interrupts() {
    let mut app = with_queue(2);
    assert!(app.queue_open());
    assert!(
        !app.modal(),
        "a queue blocks no run, so no caller may treat it as a surface that does",
    );

    assert_eq!(
        app.key(KeyEvent::new(KeyCode::Char('c'), KeyModifiers::CONTROL)),
        Command::Interrupt,
        "the stop key reaches the turn with the surface open",
    );
    assert!(
        !app.modal(),
        "and it is still not modal on the way out of the turn",
    );
}

/// F2 — `Esc` closes the surface rather than the turn, and the turn goes on.
#[test]
fn f2_esc_closes_the_surface_and_leaves_the_turn_running() {
    let mut app = with_queue(2);
    assert!(app.queue_open());

    assert_eq!(app.key(key(KeyCode::Esc)), Command::None);
    assert!(!app.queue_open(), "the first Esc answers the surface");
    assert_eq!(
        app.mode(),
        Mode::Running,
        "and it does not touch the turn underneath it",
    );
    assert_eq!(
        app.queued_prompts().len(),
        2,
        "closing the view is not dropping the queue — the lines still run",
    );

    let rows = rows_at(&mut app,TALL);
    assert!(
        !rows.iter().any(|row| row.contains("queued prompt 0")),
        "a closed surface draws nothing: {rows:#?}"
    );

    // And the second press reaches the turn, which is the key doing what it does
    // when no surface is up.
    assert_eq!(app.key(key(KeyCode::Esc)), Command::Interrupt);
}

/// F2 — something newly queued reopens a surface that was dismissed.
///
/// A dismissal is "I have seen this list", not "never show me one again". The
/// line typed after it is a new thing to have seen, and the notice announcing it
/// scrolls away.
#[test]
fn a_line_queued_after_a_dismissal_opens_the_surface_again() {
    let mut app = with_queue(1);
    app.key(key(KeyCode::Esc));
    assert!(!app.queue_open());

    send(&mut app, "one more while it is still running");
    assert!(app.queue_open());
}

/// F2 — the surface is up only while a turn is, and comes back for the next
/// queued turn.
///
/// The first half is the fleet view's rationale: a surface left standing over an
/// idle session describes a state that is no longer true. The second half is
/// what a flag cleared in `App::finished` would have cost — the drain is one
/// turn per queued prompt, and the two lines still waiting during the second
/// turn are exactly the ones worth a row.
#[test]
fn the_surface_closes_with_the_turn_and_comes_back_for_the_next_queued_turn() {
    let mut app = with_queue(3);
    assert!(app.queue_open());

    app.finished();
    assert_eq!(app.mode(), Mode::Idle);
    assert!(!app.queue_open(), "nothing is queued behind an idle prompt");
    assert_eq!(app.queued_prompts().len(), 3, "the queue itself survives");

    // The driver's drain: take the oldest and run it.
    let next = app.next_queued_prompt().expect("a queued prompt");
    assert_eq!(next, "queued prompt 0");
    app.started();
    assert!(
        app.queue_open(),
        "the two still waiting are on screen again while their turn runs",
    );

    // And the last one out closes it without anything having to say so.
    app.finished();
    app.next_queued_prompt();
    app.next_queued_prompt();
    app.started();
    assert!(app.queued_prompts().is_empty());
    assert!(!app.queue_open());
}

/// F2 — the rows carry no box-drawing character under the ASCII glyph set.
///
/// The sweep is over every non-ASCII character rather than over a list of box
/// corners, which is the same standard `tests/glyphs.rs` holds every other
/// surface to: a terminal that cannot draw `│` cannot draw `·` either, and a
/// queue whose rows arrive as replacement characters is a queue nobody can read.
#[test]
fn f2_the_surface_draws_in_the_ascii_glyph_set_with_no_box_drawing_character() {
    let mut app = running(DARK.with_glyphs(ASCII));
    for at in 0..9 {
        send(&mut app, &format!("queued prompt {at}"));
    }

    let rows = rows_at(&mut app,TALL);
    let first = row_of(&rows, "queued prompt 0");
    let composer = row_of(&rows, io_cli::composer::PROMPT.trim_end());
    let drawn = rows[first..composer].join("\n");

    if let Some(bad) = drawn.chars().find(|character| !character.is_ascii()) {
        panic!(
            "the queue surface drew {bad:?} (U+{:04X}) under the ASCII set.\n{drawn}",
            bad as u32,
        );
    }
    // Named explicitly as well, because the sweep above would also pass on a
    // surface that drew nothing at all — and these are the characters a list
    // reaches for first.
    for box_drawing in ['│', '─', '┌', '└', '├', '╭', '╰', '┃'] {
        assert!(
            !drawn.contains(box_drawing),
            "the queue is a list of lines, not a drawn box: {drawn}",
        );
    }
    assert!(
        drawn.contains("queued prompt 0"),
        "the sweep must have had something to sweep: {drawn}",
    );
}

// ---------------------------------------------------------------------------
// O7 — the queue asks for the rows it needs, and gives them back
// ---------------------------------------------------------------------------

/// **This test used to assert the opposite, and 0.32.0 reversed it on purpose.**
///
/// 0.17.0 shipped the queue under a fixed viewport, so its whole surface had to
/// fit in rows the layout could already spare — which came to exactly one, by
/// releasing the blank above the activity line. Four queued messages therefore
/// collapsed to `1. … 3 more`, and the old `n2_a_queue_of_any_depth_leaves_the_
/// frame_the_same_height_as_an_empty_one` guarded that: it asserted the session
/// never asked the terminal for another row, because doing so was "the scrollback
/// being walked upward by its own queue".
///
/// The scrollback is still not walked — that is what `tests/viewport.rs` asserts
/// over a growth, a shrink and a resize, against the byte stream — so the reason
/// the constraint existed is now discharged somewhere it can actually be seen.
/// What is left is the cost it was buying, which was the operator not being able
/// to see what they had queued.
#[test]
fn o7_the_queue_asks_for_one_row_per_message_and_returns_them_when_it_empties() {
    let mut empty = with_queue(0);
    let floor = empty.viewport_wanted(80, 40);

    // **The first queued message is free and the rest are not**, which is not an
    // off-by-one: the layout releases the blank row above the activity line while
    // the queue is open, and that released row is exactly where 0.17.0's single
    // visible line came from. So one message still fits in the floor, and every
    // message after it is a row the viewport has to grow by.
    for depth in [1, 2, 3, 9] {
        let app = with_queue(depth);
        assert_eq!(
            app.queued_prompts().len(),
            depth,
            "the fixture queued what it meant to",
        );
        assert_eq!(
            app.viewport_wanted(80, 40),
            floor + u16::try_from(depth).expect("a small depth") - 1,
            "{depth} queued lines asked for the wrong number of rows",
        );
    }

    // **And the growth is bounded by the terminal, not by the queue.** Forty
    // queued messages on a forty-row terminal cannot have forty rows; what they
    // get is the ceiling, and `queue::rows_for`'s elision reports the rest.
    let flooded = with_queue(40).viewport_wanted(80, 40);
    assert_eq!(
        flooded,
        40 - io_cli::term::SCROLLBACK_RESERVE,
        "the queue took rows the conversation above it needs",
    );

    // Emptied, the session returns to where it started.
    empty.forget_queued_prompts();
    assert_eq!(
        empty.viewport_wanted(80, 40),
        floor,
        "the rows were not given back when the queue emptied",
    );
}

/// N2 — the rows come out of the composer's allowance, so a queue deeper than
/// the rows available takes no more of them.
#[test]
fn n2_the_queue_takes_the_composers_spare_rows_and_never_the_composers_own() {
    let shallow = rows_at(&mut with_queue(1), TALL);
    let deep = rows_at(&mut with_queue(40), TALL);
    assert_eq!(shallow.len(), deep.len(), "the frame is the frame");

    let prompt = io_cli::composer::PROMPT.trim_end();
    assert!(
        row_of(&shallow, prompt) < shallow.len(),
        "the prompt is still on screen with one line queued: {shallow:#?}"
    );
    assert!(
        row_of(&deep, prompt) < deep.len(),
        "the prompt is still on screen with forty: a surface that took the \
         composer's own row would leave a session with nowhere to type in the \
         middle of a turn. {deep:#?}"
    );
    // Four rows to lend at this height, so a queue of forty is cut to three and a
    // count — never to forty rows that push the prompt off the frame.
    assert!(
        deep.iter().any(|row| row.contains("more")),
        "a queue deeper than the rows says how much did not fit: {deep:#?}"
    );
}

/// N2 — the surface is visible at the viewport a running turn actually holds,
/// and it costs that frame no height.
///
/// **This is the finding the release turned on, and it is asserted rather than
/// left as prose.** `term::VIEWPORT_HEIGHT` is eight rows, of which the
/// streaming tail, the activity line, the rule and the three-row footer take six
/// and the composer keeps its floor of one. That leaves the blank — the row the
/// layout itself describes as carrying nothing — and while the queue is open the
/// queue has it. So the surface draws at the height every real session runs at,
/// the frame is the same height as it was with nothing queued, and the blank
/// comes back the moment the queue empties.
///
/// The single row goes to the line that runs next with the rest counted on the
/// end, never to the count alone: a surface that says three are waiting and
/// names none of them has spent the only row it will ever get on the half of the
/// answer the status line already carries.
#[test]
fn n2_the_surface_is_visible_at_the_running_viewport_and_costs_it_no_height() {
    let mut app = with_queue(3);
    assert!(app.queue_open(), "queueing a line opens the surface");

    let height = io_cli::term::VIEWPORT_HEIGHT;
    let rows = rows_at(&mut app,height);
    let queued = row_of(&rows, "queued prompt 0");
    let prompt = row_of(&rows, io_cli::composer::PROMPT.trim_end());
    assert!(
        queued < prompt,
        "the next line to run is drawn, above the composer: {rows:#?}",
    );
    assert!(
        rows[queued].contains("2 more"),
        "and the two behind it are counted on the same row: {:?}",
        rows[queued],
    );

    // The blank is what it took, so the frame is the height it always was.
    let quiet = rows_at(&mut with_queue(0), height);
    assert_eq!(
        rows.len(),
        quiet.len(),
        "a queue drawn is not a frame grown",
    );
    assert_eq!(
        prompt,
        row_of(&quiet, io_cli::composer::PROMPT.trim_end()),
        "and the composer keeps the row it has when nothing is queued at all",
    );
}

/// N2 — a session drawing a queue is the same session it was: no alternate
/// screen, no clear, eighty columns, every frame inside synchronized output.
#[test]
fn n2_the_queue_holds_at_eighty_columns_inside_the_synchronized_pair() {
    let (mut screen, recorder) = support::screen_of(80, 24, TALL);
    // Two frames that differ, because a frame identical to the one on screen is
    // not drawn at all — see `tests/frames.rs` — and a skipped frame would read
    // here as a missing wrapper.
    for depth in [1, 7] {
        let mut app = with_queue(depth);
        screen
            .draw(|frame| app.render(frame, frame.area()))
            .expect("frame");
    }
    let viewport = screen.viewport_text().to_string();
    drop(screen);

    let text = recorder.text();
    for (name, sequence) in support::FORBIDDEN {
        assert!(
            !text.contains(sequence),
            "drawing the queue emitted {name} ({})",
            sequence.escape_debug(),
        );
    }
    assert!(!text.contains("\x1b[2J"), "the queue cleared the display");
    assert!(
        !text.contains("\x1b[3J"),
        "the queue erased the scrollback, which is where the transcript lives",
    );
    let begins = text.matches("\x1b[?2026h").count();
    assert_eq!(begins, 2, "one begin-synchronized-update per frame");
    assert_eq!(
        text.matches("\x1b[?2026l").count(),
        begins,
        "every begin is closed by an end",
    );
    for row in viewport.split('\n') {
        assert!(
            row.chars().count() <= 80,
            "no row is wider than the terminal it was drawn for: {row:?}",
        );
    }
}

// ---------------------------------------------------------------------------
// The arithmetic, without a frame
// ---------------------------------------------------------------------------

/// The oldest are the rows that are kept, and what did not fit is counted.
#[test]
fn what_does_not_fit_is_a_count_and_never_a_silence() {
    let waiting: Vec<String> = (0..9).map(|at| format!("prompt {at}")).collect();

    let rows = queue::rows(&waiting, 40, 4, &UNICODE);
    assert_eq!(
        rows.len(),
        4,
        "it draws into the rows it was given, and no more"
    );
    assert!(rows[0].contains("prompt 0"), "{rows:#?}");
    assert!(rows[2].contains("prompt 2"), "{rows:#?}");
    assert!(
        rows[3].contains('6'),
        "the last row says how many did not fit: {rows:#?}",
    );

    // Room for everything is everything, with no count appended.
    let all = queue::rows(&waiting, 40, 9, &UNICODE);
    assert_eq!(all.len(), 9);
    assert!(all[8].contains("prompt 8"), "{all:#?}");

    // And the ASCII set spells the elision its own way, without changing what it
    // means.
    let ascii = queue::rows(&waiting, 40, 2, &ASCII);
    assert_eq!(ascii.len(), 2);
    assert!(ascii[1].contains("8 more"), "{ascii:#?}");
    assert!(ascii.iter().all(|row| row.is_ascii()), "{ascii:#?}");
}

/// No row is ever wider than the width it was fitted to, and nothing is drawn
/// into no rows at all.
#[test]
fn a_row_is_never_wider_than_the_width_it_was_given() {
    let long = "a prompt long enough to need cutting at any width a terminal has, \
                twice over, with no natural break in it";
    let waiting = vec![long.to_string(), "short".to_string()];

    for width in [10_u16, 24, 80] {
        for row in queue::rows(&waiting, width, 4, &UNICODE) {
            assert!(
                row.chars().count() <= usize::from(width),
                "a row of {} characters at width {width}: {row:?}",
                row.chars().count(),
            );
        }
    }

    assert!(queue::rows(&waiting, 80, 0, &UNICODE).is_empty());
    assert!(queue::rows(&[], 80, 4, &UNICODE).is_empty());
}

/// A queued prompt with newlines in it is still one row.
///
/// The one way a surface measured in rows can push the composer off the frame
/// anyway: a `Line` handed text with a `\n` in it draws rows the layout never
/// budgeted for.
#[test]
fn a_multi_line_prompt_is_drawn_as_one_row() {
    let waiting = vec!["first line\nsecond line\n\tthird".to_string()];
    let rows = queue::rows(&waiting, 80, 4, &UNICODE);
    assert_eq!(rows.len(), 1);
    assert!(!rows[0].contains('\n'), "{rows:#?}");
    assert!(!rows[0].contains('\t'), "{rows:#?}");
    assert!(
        rows[0].contains("first line second line third"),
        "{rows:#?}"
    );
}

/// The `… N more` row counts what is BELOW the window, not what is outside it.
///
/// **Right on the first draw and wrong from the first arrow**, which is why the
/// review found it and the suite did not. The row sits under the last line drawn,
/// so a count of everything unshown reads as a count of what follows — and once
/// the mark has scrolled the window down, most of what is unshown is above it.
/// What is above needs no row: the numbers are absolute, so a window opening at
/// `6.` says so by saying `6.`.
#[test]
fn the_count_under_the_window_is_what_is_under_the_window() {
    let waiting: Vec<String> = (1..=9).map(|n| format!("line {n}")).collect();

    // Nothing marked: the window is at the top and everything unshown IS below.
    let top = queue::rows_for(&waiting, None, 80, 4, &UNICODE);
    assert!(
        top.last().expect("a row").contains("6 more"),
        "three drawn of nine leaves six below: {top:#?}",
    );

    // Marked on the eighth: the window has scrolled to 6, 7, 8 — and only line 9
    // is below it. The old arithmetic said six, five of which were behind.
    let scrolled = queue::rows_for(&waiting, Some(7), 80, 4, &UNICODE);
    assert!(
        scrolled[0].contains("6. line 6"),
        "the window follows the mark: {scrolled:#?}",
    );
    assert!(
        scrolled.last().expect("a row").contains("1 more"),
        "one line is below the window, not six: {scrolled:#?}",
    );

    // And at the very bottom there is nothing below, so there is no row at all.
    let bottom = queue::rows_for(&waiting, Some(8), 80, 4, &UNICODE);
    assert!(
        !bottom.iter().any(|row| row.contains("more")),
        "a count of zero is a row that says nothing: {bottom:#?}",
    );
}

/// The queue does not take the blank row when the fleet view is drawn in its place.
///
/// The fleet is rendered INSTEAD of the queue, in the composer's own rect. Without
/// this the blank was released for a surface that then did not draw: the row
/// bought nothing and the fleet quietly grew by one.
#[test]
fn a_queue_behind_an_open_fleet_view_takes_no_row_from_the_layout() {
    let mut app = with_queue(2);
    let queued = rows_at(&mut app,TALL);

    app.toggle_fleet();
    let fleeted = rows_at(&mut app,TALL);
    assert_eq!(
        queued.len(),
        fleeted.len(),
        "the frame is the frame either way",
    );
    assert!(
        !fleeted.iter().any(|row| row.contains("queued prompt 0")),
        "the fleet is drawn in the queue's place, so the queue is not drawn: {fleeted:#?}",
    );
}

/// And it does not answer the keyboard from behind the fleet either.
///
/// `Enter` at an empty prompt used to reach `Cursor::take` with the fleet open —
/// a line leaving the queue into a composer the fleet was covering, under a footer
/// saying it was being edited. A surface acting while invisible.
#[test]
fn a_queue_behind_an_open_fleet_view_answers_no_key() {
    let mut app = with_queue(2);
    app.toggle_fleet();

    app.key(key(KeyCode::Enter));
    assert_eq!(
        app.queued_prompts().len(),
        2,
        "no line was taken out of the queue by a surface nobody can see",
    );
    assert!(
        app.composer.is_empty(),
        "and nothing was put in the composer the fleet is covering",
    );
}

// ---------------------------------------------------------------------------
// O7/O8/O9 — the list is drawn in full, and it reaches the turn it was typed into
// ---------------------------------------------------------------------------

/// **O7 — four queued messages are four rows, with their positions, and nothing
/// is elided.**
///
/// Through 0.31.0 the surface had exactly one row — released from the blank above
/// the activity line — so four messages collapsed to `1. … 3 more` and the
/// operator could see the first and a number. With the viewport sized to what the
/// queue asks for, the collapse survives only for a terminal that genuinely
/// cannot do better.
#[test]
fn o7_every_queued_message_is_listed_with_its_position() {
    let mut app = with_queue(4);
    let wanted = app.viewport_wanted(80, 40);
    let rows = rows_at(&mut app, wanted);
    let drawn = rows.join("\n");

    for at in 0..4 {
        let text = format!("queued prompt {at}");
        assert!(
            drawn.contains(&text),
            "{text} is not on screen, so the queue is still eliding: {drawn}",
        );
        assert!(
            drawn.contains(&format!("{}.", at + 1)),
            "the position of {text} is missing: {drawn}",
        );
    }
    assert!(
        !drawn.contains("more"),
        "nothing should be elided when the viewport grew to hold it: {drawn}",
    );
}

/// **O7's other half — a terminal that cannot grow still says what it is
/// holding.** The `room == 1` collapse is not deleted, it stops being what every
/// real session sees, and the count it reports has to be right.
#[test]
fn o7_a_terminal_too_small_to_grow_reports_what_it_did_not_draw() {
    let mut app = with_queue(4);
    // The floor, not what the queue asked for.
    let rows = rows_at(&mut app, io_cli::term::VIEWPORT_HEIGHT);
    let drawn = rows.join("\n");
    assert!(
        drawn.contains("more"),
        "a queue that cannot be drawn in full must say so: {drawn}",
    );
    assert!(
        drawn.contains('3'),
        "four queued with one drawn leaves three, and the count must be right: {drawn}",
    );
}

/// **O8 — the queue is delivered in the order it was typed, and each delivery is
/// recorded in the transcript.**
///
/// The loop lives in `App::deliver_queued` rather than in the driver because
/// nothing under `tests/` links `src/main.rs`: a loop written there could be
/// neither asserted nor sabotaged, and this one decides whether an operator's
/// words are kept.
#[test]
fn o8_delivery_is_in_order_and_reaches_the_transcript() {
    let mut app = with_queue(3);
    let mut seen: Vec<String> = Vec::new();
    let delivered = app.deliver_queued(|text| {
        seen.push(text.to_string());
        Ok(())
    });

    assert_eq!(delivered.sent, 3);
    assert_eq!(delivered.refused, None);
    assert_eq!(
        seen,
        vec![
            "queued prompt 0".to_string(),
            "queued prompt 1".to_string(),
            "queued prompt 2".to_string(),
        ],
        "io-harness pushes one observation per message and the model reads them \
         in order, so joining or reordering them is a paragraph the operator \
         never wrote",
    );
    assert!(
        app.queued_prompts().is_empty(),
        "everything delivered leaves the queue",
    );

    let said = app
        .take_pending()
        .iter()
        .flat_map(|line| line.spans.iter())
        .map(|span| span.content.to_string())
        .collect::<Vec<_>>()
        .join(" ");
    for at in 0..3 {
        assert!(
            said.contains(&format!("queued prompt {at}")),
            "a steered line is part of the conversation, not a footer note: {said}",
        );
    }
}

/// **O9 — a message that cannot be delivered is kept, not lost.**
///
/// The one thing worse than a correction that arrives late is one that reports
/// success and goes nowhere. Delivery stops at the first refusal, the message goes
/// back to the front of the queue, and the caller is told why — so `sent` is never
/// reported alongside a refusal as though both were true.
#[test]
fn o9_a_message_that_cannot_be_delivered_goes_back_on_the_queue() {
    let mut app = with_queue(3);
    let mut calls = 0;
    let delivered = app.deliver_queued(|_| {
        calls += 1;
        if calls == 2 {
            Err("nothing is listening".to_string())
        } else {
            Ok(())
        }
    });

    assert_eq!(delivered.sent, 1, "only the first one landed");
    assert_eq!(delivered.refused.as_deref(), Some("nothing is listening"));
    assert_eq!(
        app.queued_prompts(),
        ["queued prompt 1", "queued prompt 2"],
        "the message that failed is back at the front, with everything behind it",
    );
}
