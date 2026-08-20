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
