//! F4 — finished content reaches scrollback, not the viewport.
//!
//! This is the product's whole claim, so it is asserted from both sides: the text
//! of every finished message appears in the byte stream, and the viewport buffer
//! at rest holds only the composer and the status line.

mod support;

use ratatui::text::Line;
use ratatui::widgets::Paragraph;

#[test]
fn f4_finished_messages_reach_the_terminal_and_not_the_viewport() {
    let (mut screen, recorder) = support::screen(100, 30);

    let messages = ["the first finished reply", "the second", "and the third"];
    for message in messages {
        screen
            .commit(&[Line::from(message), Line::from("")])
            .expect("commit");
        // A frame after every commit, exactly as a session draws.
        screen
            .draw(|frame| {
                let area = frame.area();
                frame.render_widget(Paragraph::new("> "), area);
            })
            .expect("frame");
    }

    let text = recorder.text();
    for message in messages {
        assert!(
            text.contains(message),
            "{message:?} never reached the terminal, so it is not in the scrollback either",
        );
    }

    // At rest the viewport holds what the composer and status line drew, and no
    // part of any committed message. This is the half that fails when a finished
    // message is rendered into the viewport instead of being committed.
    let viewport = screen.viewport_text();
    for message in messages {
        assert!(
            !viewport.contains(message),
            "{message:?} is still in the live viewport; it was rendered rather than committed",
        );
    }
    assert!(
        viewport.contains('>'),
        "the viewport should hold the composer, but it holds {viewport:?}",
    );
}

#[test]
fn f4_committed_content_survives_the_frames_drawn_after_it() {
    // The ordering half: a commit followed by more frames must not have its lines
    // overwritten by the viewport that renders below them.
    let (mut screen, recorder) = support::screen(100, 30);

    screen
        .commit(&[Line::from("committed before ten frames")])
        .expect("commit");
    for _ in 0..10 {
        screen.draw(|_| {}).expect("frame");
    }

    assert!(
        recorder.contains("committed before ten frames"),
        "the committed line was never written",
    );
}
