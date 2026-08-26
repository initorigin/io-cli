//! F6 — a frame whose content is unchanged is not drawn.
//!
//! Asserted over the byte count, which is the only thing that separates a
//! skipped repaint from a cheap one. ratatui's own diff already suppresses the
//! *cells* of an unchanged frame, so a renderer that compares the frames and
//! then draws anyway looks identical in every other way: the same escape
//! sequences, the same viewport text, the same screen. What it does not have is
//! a flat byte count, because the synchronized-output pair, the colour resets
//! crossterm emits after every diff however empty, and the cursor ratatui
//! re-places on every frame are all written regardless of whether anything
//! moved. A session repaints on every keystroke and every streamed token; those
//! bytes are the ones this criterion is about.
//!
//! No clock appears anywhere, and none can: what is asserted is content, not
//! how long anything took (N1).

mod support;

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::thread;

use ratatui::layout::Position;
use ratatui::style::{Color, Style};
use ratatui::text::Line;
use ratatui::widgets::Paragraph;

/// One frame holding `text` and nothing else.
fn paint(screen: &mut io_cli::term::Screen<support::Fixed>, text: &str) {
    screen
        .draw(|frame| frame.render_widget(Paragraph::new(text), frame.area()))
        .expect("frame");
}

#[test]
fn f6_a_frame_whose_content_is_unchanged_is_not_drawn() {
    let (mut screen, recorder) = support::screen(80, 24);

    // The first frame always happens: there is nothing on the screen to compare
    // it against, and a renderer that skipped it would draw nothing ever.
    paint(&mut screen, "ready");
    let one_frame = recorder.bytes().len();
    assert!(
        one_frame > 0,
        "the first frame wrote nothing, so the comparison is skipping the frame \
         that has nothing to be compared with",
    );

    // The same frame again. One frame's worth of bytes, still.
    paint(&mut screen, "ready");
    assert_eq!(
        recorder.bytes().len(),
        one_frame,
        "a frame identical to the one already on the screen was written to the \
         terminal; the repaint was made cheap rather than skipped",
    );

    // One cell different, and it is drawn.
    paint(&mut screen, "readz");
    assert!(
        recorder.bytes().len() > one_frame,
        "a frame differing from the screen by one cell was not written",
    );
}

#[test]
fn f6_a_still_screen_costs_nothing_however_many_frames_are_asked_for() {
    let (mut screen, recorder) = support::screen(80, 24);

    paint(&mut screen, "idle");
    let one_frame = recorder.bytes().len();

    for _ in 0..50 {
        paint(&mut screen, "idle");
    }

    assert_eq!(
        recorder.bytes().len(),
        one_frame,
        "fifty repaints of a screen that did not move reached the terminal",
    );
    // A skipped frame is still a frame: what it would have drawn is what the
    // renderer reports, because a caller reading the viewport cannot be made to
    // care whether the bytes went out.
    assert!(
        screen.viewport_text().starts_with("idle"),
        "the viewport text was lost on a skipped frame: {:?}",
        screen.viewport_text(),
    );
}

#[test]
fn a_frame_that_only_changes_a_style_is_still_drawn() {
    // The comparison is over the buffer, not over the viewport's text. A picker
    // moving its highlight from one row to the next changes no character at all,
    // and skipping that frame would freeze the selection on the screen while the
    // application believed it had moved.
    let (mut screen, recorder) = support::screen(80, 24);

    screen
        .draw(|frame| frame.render_widget(Paragraph::new("same text"), frame.area()))
        .expect("frame");
    let one_frame = recorder.bytes().len();

    screen
        .draw(|frame| {
            frame.render_widget(
                Paragraph::new("same text").style(Style::default().fg(Color::Red)),
                frame.area(),
            );
        })
        .expect("frame");

    assert_eq!(
        screen.viewport_text().lines().next(),
        Some("same text"),
        "the two frames were supposed to differ only in style",
    );
    assert!(
        recorder.bytes().len() > one_frame,
        "a frame whose only change is a style was skipped, so the comparison is \
         over the text rather than over the buffer",
    );
}

#[test]
fn a_frame_that_only_moves_the_cursor_is_still_drawn() {
    // The other half of "content": moving the caret through text that does not
    // change is a real change with no cell behind it.
    let (mut screen, recorder) = support::screen(80, 24);

    screen
        .draw(|frame| {
            let area = frame.area();
            frame.render_widget(Paragraph::new("> typing"), area);
            frame.set_cursor_position(Position { x: 2, y: area.y });
        })
        .expect("frame");
    let one_frame = recorder.bytes().len();

    screen
        .draw(|frame| {
            let area = frame.area();
            frame.render_widget(Paragraph::new("> typing"), area);
            frame.set_cursor_position(Position { x: 6, y: area.y });
        })
        .expect("frame");

    assert!(
        recorder.bytes().len() > one_frame,
        "a frame that moved only the cursor was skipped, so the caret is still \
         where the previous frame left it",
    );
}

#[test]
fn a_commit_makes_the_next_identical_frame_a_real_repaint() {
    // `insert_before` ends by clearing the viewport off the screen. The frame
    // after it repaints an erased region, so it cannot be compared against the
    // frame that drew that region — the terminal is no longer showing it.
    let (mut screen, recorder) = support::screen(80, 24);

    paint(&mut screen, "> ");
    screen
        .commit(&[Line::from("a finished reply")])
        .expect("commit");
    let after_commit = recorder.bytes().len();

    paint(&mut screen, "> ");
    assert!(
        recorder.bytes().len() > after_commit,
        "the frame after a commit was skipped, which leaves the viewport erased",
    );
}

/// Enough spinning that a commit which did not wait for the terminal has
/// certainly finished.
///
/// A count rather than a duration, because N1 forbids the clock in every test
/// here. A commit that does not wait is microseconds of buffer work; this is
/// three or four orders of magnitude more than that, and it is only ever a
/// margin — the assertion below can miss a regression on a badly descheduled
/// machine but cannot fail on a correct one.
const LONG_ENOUGH: u32 = 10_000_000;

#[test]
fn a_commit_takes_the_terminal_before_it_asks_the_terminal_anything() {
    // **This is what 0.18.0's first build died of.** ratatui 0.30 ends
    // `insert_before` by clearing the viewport, and `Terminal::clear` now
    // snapshots the backend cursor first so it can put it back — `ESC[6n`, whose
    // answer arrives on stdin. In 0.29 that same clear wrote only the escape. So
    // committing became a question put to the terminal, and the keyboard reader
    // was still holding stdin when it was asked: the splash was drawn, two
    // seconds passed, and the session gave up saying the cursor position could
    // not be read.
    //
    // Asserted as a decision rather than a duration: a placement is held on
    // another thread, and the commit must not have finished before that
    // placement let go.
    let holding = Arc::new(AtomicBool::new(false));
    let committing = Arc::new(AtomicBool::new(false));
    let released = Arc::new(AtomicBool::new(false));

    let holder = {
        let (held, started, freed) = (
            Arc::clone(&holding),
            Arc::clone(&committing),
            Arc::clone(&released),
        );
        thread::spawn(move || {
            let placement = io_cli::stdin::placing();
            held.store(true, Ordering::SeqCst);
            while !started.load(Ordering::SeqCst) {
                std::hint::spin_loop();
            }
            for _ in 0..LONG_ENOUGH {
                std::hint::spin_loop();
            }
            freed.store(true, Ordering::SeqCst);
            drop(placement);
        })
    };
    while !holding.load(Ordering::SeqCst) {
        std::hint::spin_loop();
    }

    // Built after the placement is held, and it must be: `from_terminal` wraps a
    // terminal that already exists, so nothing here asks the terminal anything
    // until the commit does.
    let (mut screen, _recorder) = support::screen(80, 24);
    committing.store(true, Ordering::SeqCst);
    screen
        .commit(&[Line::from("a finished reply")])
        .expect("commit");
    let waited = released.load(Ordering::SeqCst);

    holder.join().expect("the placement thread does not panic");
    assert!(
        waited,
        "a commit ran while another thread held the terminal — `insert_before` \
         asks the terminal where its cursor is, so the keyboard reader is free \
         to take the answer and the session dies two seconds later",
    );
}

/// 0.11.0 F5 — the activity line brings no repaint of its own.
///
/// The row is drawn by the tick that already advances the spinner and the clock,
/// and the frame is still diffed against the last one — so a running turn whose
/// age has not moved and whose events have not arrived writes nothing, exactly
/// as an idle session does. A row that carried a clock of its own, or a spinner
/// on its own schedule, would fail here by writing a second frame for a screen
/// that did not change.
#[test]
fn f5_an_activity_line_over_an_unchanged_turn_is_not_drawn_twice() {
    use std::time::Duration;

    let (mut screen, recorder) = support::screen(80, 24);
    let mut app = io_cli::app::App::new(io_cli::theme::DARK, "m");
    app.started();

    let mut draw = |app: &mut io_cli::app::App| {
        screen
            .draw(|frame| app.render(frame, frame.area()))
            .expect("frame");
        recorder.bytes().len()
    };

    app.tick(Duration::from_secs(1));
    let one_frame = draw(&mut app);
    assert!(
        one_frame > 0,
        "the first frame of a running turn wrote nothing"
    );

    // Drawn again with no tick in between. The row is a function of `Status`,
    // and `Status` has not moved — so an activity line with a clock or a spinner
    // of its own would show up here as a second frame for an unchanged screen.
    assert_eq!(
        draw(&mut app),
        one_frame,
        "the activity line changed without the tick that draws it",
    );

    // The tick moves, and now there is something to say. This is the repaint
    // that already existed — the spinner and the clock — and not a new one.
    app.tick(Duration::from_secs(2));
    assert!(
        draw(&mut app) > one_frame,
        "the tick advanced and the viewport did not",
    );
}

/// **0.14.0 F6 — a budget in force reaches a real frame, in both of the forms
/// the status line has.**
///
/// Asserted on the rendered viewport rather than on a `Line`, which is the half
/// a unit test cannot give: `Status::render` picks between two renderers on the
/// height of the area it is handed, and a claim made against whichever one the
/// test happened to call is a claim about a function rather than about a screen.
/// A viewport eight rows tall gives the status area three rows and draws
/// `Status::footer`, which is what the binary shows at every ordinary prompt; a
/// viewport of four gives it one and draws `Status::line`, which has exactly one
/// production caller and it is this fallback. 0.12.0 filled `line` alone, went
/// green, and put nothing on the operator's screen.
///
/// Sabotage: report the ceiling through the error path — under which only F6
/// fails, by reproducing the `error: step_cap_reached` under an unfinished
/// answer that `src/contract.rs` documents as the reason `MAX_STEPS` exists.
#[test]
fn f6_a_budget_in_force_reaches_a_rendered_frame_in_both_status_line_forms() {
    use std::time::Duration;

    /// The viewport a session with all three budgets set draws at `viewport`
    /// rows.
    fn drawn(viewport: u16) -> String {
        let (mut screen, _recorder) = support::screen_of(160, 40, viewport);
        let mut app = io_cli::app::App::new(io_cli::theme::DARK, "m");
        app.status.budgets = io_cli::status::Budgets::in_force(
            &io_harness::TaskContract::workspace("goal", std::path::PathBuf::from("/tmp"))
                .with_max_steps(20)
                .with_token_budget(10_000)
                .with_time_budget(Duration::from_secs(600)),
        );
        app.status.steps = Some(3);
        app.status.run_tokens = Some(2_500);
        app.status.elapsed = Duration::from_secs(90);
        screen
            .draw(|frame| app.render(frame, frame.area()))
            .expect("frame");
        screen.viewport_text().to_string()
    }

    // Eight rows: the status area gets three and the footer is what is drawn.
    let footer = drawn(8);
    // Four: one row for the status area, and the one-row form is the fallback.
    let line = drawn(4);

    for (form, text) in [("the footer", &footer), ("the one-row form", &line)] {
        assert!(
            text.contains("left 17/20 steps"),
            "{form} drew no step budget: {text:?}",
        );
        assert!(
            text.contains("left 7.5k/10.0k tok"),
            "{form} drew no token budget: {text:?}",
        );
        assert!(
            text.contains("left 8m30s/10m00s"),
            "{form} drew no time budget: {text:?}",
        );
    }
}

/// **0.14.0 F6 — a turn ended by a budget names the budget, and is not an
/// error.**
///
/// **Budget exhaustion is `Ok` in io-harness and always has been.** A run that
/// spends its steps, its seconds or its tokens returns a `RunOutcome` saying
/// which — `StepCapReached`, `TimeBudgetExceeded`, `CostBudgetExceeded` — so
/// nothing on the `Result` distinguishes a ceiling from a clean finish, and the
/// interactive driver's own `match` on the turn's return has nothing to say
/// about any of them. The only thing that reaches a reader is the outcome word
/// on `EventKind::Finished`, and until 0.14.0 three of the four ceilings fell
/// through `outcome_tone` to `Tone::Error`.
///
/// So this asserts the tone through the bytes rather than through the enum: what
/// the operator met was the literal string `error: step_cap_reached` over a
/// half-finished answer, and that string is what must not come back.
///
/// Sabotage: report the ceiling through the error path — under which only F6
/// fails, by reproducing the `error: step_cap_reached` under an unfinished
/// answer that `src/contract.rs` documents as the reason `MAX_STEPS` exists.
#[test]
fn f6_a_turn_ended_by_each_budget_commits_a_line_naming_it_rather_than_an_error() {
    use std::time::Duration;

    for outcome in [
        "step_cap_reached",
        "time_budget_exceeded",
        "cost_budget_exceeded",
    ] {
        let (mut screen, recorder) = support::screen_of(160, 40, 8);
        let mut app = io_cli::app::App::new(io_cli::theme::DARK, "m");
        app.started();
        app.event(
            &io_harness::RunEvent::new(
                1,
                4,
                io_harness::EventKind::Finished {
                    outcome: outcome.to_string(),
                    steps: 4,
                    tokens: 9_000,
                },
            ),
            Duration::from_secs(30),
        );
        app.finished();

        let committed = app.take_pending();
        assert!(
            !committed.is_empty(),
            "a turn that ended at the {outcome} ceiling committed nothing at all",
        );
        // **The tone's word and the outcome are two spans, so the pairing is
        // asserted on the line and the frame is asserted for the word.** A tone
        // carrier is drawn in the tone's own colour and the text beside it in the
        // ordinary foreground, which puts a colour escape between the two in the
        // byte stream — `contains("error: step_cap_reached")` over the bytes
        // would be a claim that passes because of the escape rather than because
        // of the tone.
        let said: String = committed
            .iter()
            .flat_map(|line| line.spans.iter())
            .map(|span| span.content.as_ref())
            .collect();
        assert!(
            said.contains(&format!("warning: {outcome}")),
            "the ceiling was not drawn in the tone `budget_ceiling_reached` has \
             always had: {said:?}",
        );
        assert!(
            !said.contains("error:"),
            "a ceiling was reported through the error path: {said:?}",
        );

        screen.commit(&committed).expect("commit");
        screen
            .draw(|frame| app.render(frame, frame.area()))
            .expect("frame");

        let text = recorder.text();
        assert!(
            text.contains(outcome),
            "the frame does not name the budget that ended the turn: {outcome}",
        );
        // The word itself is one span and survives the byte stream whole, so this
        // is the frame-level half of the same claim: the tone carrier `error`
        // never leaves the process for a turn that reached a ceiling.
        assert!(
            !text.contains("error"),
            "the word `error` reached the terminal for a turn that ended at the \
             {outcome} ceiling",
        );
    }
}

#[test]
fn a_resize_makes_the_next_identical_frame_a_real_repaint() {
    // Same reason, different cause: recomputing an inline viewport clears it.
    let (mut screen, recorder) = support::screen(80, 24);

    paint(&mut screen, "> ");
    support::resize(&mut screen, 80, 30);
    let after_resize = recorder.bytes().len();

    paint(&mut screen, "> ");
    assert!(
        recorder.bytes().len() > after_resize,
        "the frame after a resize was skipped, which leaves the viewport erased",
    );
}
