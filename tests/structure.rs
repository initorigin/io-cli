//! F5 — no alternate screen and no mouse capture, ever.
//! N3 — no full-screen clear during a session.
//!
//! Both are assertions over the bytes io-cli writes to the terminal, and both are
//! written so that a later release cannot reintroduce fullscreen or a clear-based
//! redraw without turning a named test red.

mod support;

use ratatui::text::Line;
use support::FORBIDDEN;

/// A session long enough to have written everything a session writes: a splash
/// into scrollback, a viewport frame, several committed messages, a resize, and
/// more frames after it.
fn scripted_session(width: u16, height: u16) -> support::Recorder {
    let (mut screen, recorder) = support::screen(width, height);

    screen
        .commit(&[Line::from("io-cli"), Line::from("")])
        .expect("splash");
    screen.draw(|_| {}).expect("first frame");

    for turn in 0..5 {
        screen
            .commit(&[
                Line::from(format!("> prompt {turn}")),
                Line::from(format!("assistant reply {turn}")),
                Line::from(""),
            ])
            .expect("commit");
        screen.draw(|_| {}).expect("frame");
    }

    support::resize(&mut screen, width, height + 4);
    screen.draw(|_| {}).expect("frame after resize");
    screen
        .commit(&[Line::from("after the resize")])
        .expect("commit after resize");

    drop(screen);
    recorder
}

#[test]
fn f5_never_enters_the_alternate_screen_or_captures_the_mouse() {
    let recorder = scripted_session(100, 30);
    let text = recorder.text();

    for (name, sequence) in FORBIDDEN {
        assert!(
            !text.contains(sequence),
            "the byte stream contains {name} ({}), which this product has no code path for",
            sequence.escape_debug(),
        );
    }
}

#[test]
fn f5_holds_at_eighty_columns_too() {
    // The narrow path takes a different branch through `insert_before`, which
    // loops when the content is taller than the screen. F5 has to hold there too.
    let recorder = scripted_session(80, 24);
    let text = recorder.text();

    for (name, sequence) in FORBIDDEN {
        assert!(
            !text.contains(sequence),
            "the byte stream at 80x24 contains {name} ({})",
            sequence.escape_debug(),
        );
    }
}

#[test]
fn n3_never_clears_the_whole_screen_during_a_session() {
    let recorder = scripted_session(100, 30);
    let text = recorder.text();

    // `ESC [ 2 J` erases the display, and `ESC [ 3 J` erases the scrollback —
    // which on this renderer is where the transcript lives, so it is worse than
    // the one the criterion names.
    assert!(
        !text.contains("\x1b[2J"),
        "the byte stream contains a full-screen clear",
    );
    assert!(
        !text.contains("\x1b[3J"),
        "the byte stream contains a scrollback erase, which would destroy the transcript",
    );
}

#[test]
fn every_frame_is_wrapped_in_synchronized_output() {
    let (mut screen, recorder) = support::screen(100, 30);
    // The two frames must DIFFER, and that is 0.6.0's doing rather than an
    // arbitrary choice. Since F6 a frame whose content matches the one on screen
    // is not drawn at all, so two empty frames would produce one begin and this
    // test would read the skip as a missing wrapper — a green-to-red for the
    // opposite of the reason it exists. What it is actually about is that every
    // frame io-cli *does* draw is wrapped, so the frames are given something to
    // differ by.
    for word in ["ready", "working"] {
        screen
            .draw(|frame| {
                // `frame.area()` and not a rectangle of the test's own: an inline
                // viewport sits at the bottom of the terminal, so its area has a
                // non-zero origin and anything drawn at row zero is outside the
                // buffer.
                frame.render_widget(ratatui::widgets::Paragraph::new(word), frame.area());
            })
            .expect("frame");
    }
    drop(screen);

    let text = recorder.text();
    let begins = text.matches("\x1b[?2026h").count();
    let ends = text.matches("\x1b[?2026l").count();

    assert_eq!(begins, 2, "one begin-synchronized-update per frame");
    assert_eq!(ends, begins, "every begin is closed by an end");
}
