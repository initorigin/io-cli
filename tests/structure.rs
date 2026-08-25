//! F5 — no alternate screen and no mouse capture, ever.
//! N3 — no full-screen clear during a session.
//!
//! Both are assertions over the bytes io-cli writes to the terminal, and both are
//! written so that a later release cannot reintroduce fullscreen or a clear-based
//! redraw without turning a named test red.
//!
//! O1 — and one assertion that is not about bytes at all: the order of two calls
//! in `src/main.rs`. Nothing under `tests/` links the binary, so a decision made
//! in the driver is one no test can drive; this file reads the driver instead.

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

/// `src/main.rs`, with every comment taken off before anything is matched.
///
/// The stripping is the whole difference between a gate and a green light. 0.14.0
/// shipped a check that asserted the source contained `EventKind::Dialed` and was
/// satisfied by a *comment* naming the variant — a passing test over code that had
/// none of it. Prose about `adopt` is exactly as easy to write, so the prose is
/// removed and what is left is the code the compiler sees. `//` appears in no
/// string literal in this file and it has no block comments, so a line cut at the
/// first `//` is a line cut at its comment.
fn driver_without_comments() -> String {
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("src/main.rs");
    let text = std::fs::read_to_string(path).expect("the driver is readable");
    text.lines()
        .map(|line| match line.find("//") {
            Some(at) => &line[..at],
            None => line,
        })
        .collect::<Vec<_>>()
        .join("\n")
}

/// **O1.** The home is adopted before the configuration is discovered.
///
/// Presence is not the property; order is. `io_harness::config::user_path` reads
/// the environment at call time, so a `Config` discovered before `home::adopt` set
/// `IO_CONFIG_HOME` is a configuration read out of the directory the run store —
/// derived from that file's own directory — has already left. The symptom is not
/// an error: it is a session that starts fine, writes to one place, and answers
/// `/resume` from another that is empty.
///
/// It is asserted here because nothing under `tests/` links the binary, which is
/// how this repository already pins the decisions `src/main.rs` makes — see
/// `tests/contract.rs` and `tests/plan.rs`. The offsets come from the source with
/// its comments removed, so the paragraph you are reading could be pasted into the
/// driver and this test would still fail.
#[test]
fn o1_the_home_is_adopted_before_the_configuration_is_discovered() {
    let text = driver_without_comments();

    let adopt = text
        .find("io_cli::home::adopt()")
        .expect("`run` calls `io_cli::home::adopt`, in code and not in a sentence about it");
    // The first one, which is the only one either arm reaches — the wizard's
    // re-read below it happens after a file has been written.
    let discover = text
        .find("Config::discover(")
        .expect("`run` discovers the configuration");

    assert!(
        adopt < discover,
        "the home is adopted at byte {adopt} and the configuration discovered at {discover}: \
         a configuration discovered first is read from the directory the store has left",
    );
}

/// **F6, the session arm.** What the migration did is committed into the
/// scrollback rather than said on a row that repaints.
///
/// `App::say` answers a keystroke and is gone at the next one. A migration happens
/// once, on the run after an upgrade, and the operator it matters to has pressed
/// nothing yet — so said, it would be replaced by the first thing they typed and
/// never be seen again. `App::record` is the half that belongs to the conversation.
///
/// The call is matched with the loop it sits in, so the assertion is over the code
/// that runs and not over the word `record` appearing anywhere in the file.
#[test]
fn f6_the_migration_report_is_recorded_rather_than_said() {
    let text = driver_without_comments();

    assert!(
        text.contains("for line in report {\n        app.record(Tone::Muted, line);"),
        "the migration report reaches the scrollback through `App::record`; `App::say` would \
         put it on the footer's row, where the first keystroke replaces it",
    );
}
