//! Resize, which is where a hybrid inline renderer loses its history.
//!
//! A full-screen renderer redraws everything it owns when the terminal changes
//! size, and since it owns the transcript the transcript is drawn again — which
//! is the duplication users report as a bug. Here the transcript belongs to the
//! terminal, so a resize must recompute the viewport and touch nothing above it.

mod support;

use ratatui::text::Line;

const MARK: &str = "committed-before-the-resize";

#[test]
fn a_resize_does_not_write_committed_content_a_second_time() {
    let (mut screen, recorder) = support::screen(100, 30);

    screen.commit(&[Line::from(MARK)]).expect("commit");
    screen.draw(|_| {}).expect("frame");
    let before = recorder.text().matches(MARK).count();
    assert_eq!(
        before, 1,
        "the committed line should be written exactly once"
    );

    support::resize(&mut screen, 80, 24);
    screen.draw(|_| {}).expect("frame after resize");
    support::resize(&mut screen, 120, 40);
    screen.draw(|_| {}).expect("frame after resizing back");

    assert_eq!(
        recorder.text().matches(MARK).count(),
        before,
        "the resize redrew content that belongs to the terminal's scrollback",
    );
}

#[test]
fn content_committed_after_a_resize_uses_the_new_width() {
    // A line that fits on one row at 100 columns needs two at 40, and the height
    // handed to `insert_before` is what decides whether the second row exists.
    //
    // There is deliberately NO frame between the resize and the commit. ratatui's
    // `autoresize` re-reads the terminal size at the top of every `draw`, so a
    // resize followed by a frame is handled whether or not this renderer does
    // anything at all — which is what a sabotage of `Screen::resize` proved. The
    // case that needs the explicit call is the one below: an event arrives, output
    // is committed, and the next frame has not happened yet.
    let wide = "x".repeat(70);
    let (mut screen, recorder) = support::screen(100, 30);
    screen.draw(|_| {}).expect("frame");

    support::resize(&mut screen, 40, 30);
    assert_eq!(screen.width(), 40, "the resize did not reach the renderer");
    screen
        .commit(&[Line::from(wide.clone())])
        .expect("commit after resize");

    let text = recorder.text();
    // All seventy characters reached the terminal, wrapped rather than truncated.
    let written: usize = text.matches('x').count();
    assert!(
        written >= wide.len(),
        "only {written} of {} characters were written after the resize; the commit \
         height was computed against the old width",
        wide.len(),
    );
}
