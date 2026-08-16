//! The composer, and its share of F9: it stays usable at eighty columns.

mod support;

use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use io_cli::composer::{Composer, Reply, PROMPT};
use io_cli::theme::DARK;

fn key(code: KeyCode) -> KeyEvent {
    KeyEvent::new(code, KeyModifiers::NONE)
}

fn type_text(composer: &mut Composer, text: &str) {
    for character in text.chars() {
        composer.key(key(KeyCode::Char(character)));
    }
}

#[test]
fn enter_submits_and_clears() {
    let mut composer = Composer::new();
    type_text(&mut composer, "fix the failing test");

    assert_eq!(
        composer.key(key(KeyCode::Enter)),
        Reply::Submitted("fix the failing test".into()),
    );
    assert!(
        composer.is_empty(),
        "the composer should be ready for the next prompt"
    );
}

#[test]
fn an_empty_enter_does_nothing() {
    let mut composer = Composer::new();
    assert_eq!(composer.key(key(KeyCode::Enter)), Reply::Idle);
    type_text(&mut composer, "   ");
    assert_eq!(
        composer.key(key(KeyCode::Enter)),
        Reply::Idle,
        "whitespace is not a prompt",
    );
}

#[test]
fn shift_enter_inserts_a_newline() {
    let mut composer = Composer::new();
    type_text(&mut composer, "first");
    composer.key(KeyEvent::new(KeyCode::Enter, KeyModifiers::SHIFT));
    type_text(&mut composer, "second");

    assert_eq!(composer.text(), "first\nsecond");
    assert_eq!(
        composer.key(key(KeyCode::Enter)),
        Reply::Submitted("first\nsecond".into()),
    );
}

#[test]
fn a_trailing_backslash_continues_the_line_on_terminals_without_shift_enter() {
    // Most terminals cannot report `Shift+Enter` at all without the Kitty
    // keyboard protocol, so this fallback is the one that actually gets used.
    let mut composer = Composer::new();
    type_text(&mut composer, "first\\");
    assert_eq!(composer.key(key(KeyCode::Enter)), Reply::Idle);
    type_text(&mut composer, "second");

    assert_eq!(
        composer.text(),
        "first\nsecond",
        "the backslash should be consumed, not left in the prompt",
    );
}

#[test]
fn the_arrows_walk_prompt_history_and_come_back() {
    let mut composer = Composer::new();
    for prompt in ["first", "second", "third"] {
        type_text(&mut composer, prompt);
        composer.key(key(KeyCode::Enter));
    }
    type_text(&mut composer, "a draft");

    composer.key(key(KeyCode::Up));
    assert_eq!(composer.text(), "third");
    composer.key(key(KeyCode::Up));
    assert_eq!(composer.text(), "second");
    composer.key(key(KeyCode::Up));
    assert_eq!(composer.text(), "first");
    composer.key(key(KeyCode::Up));
    assert_eq!(
        composer.text(),
        "first",
        "the oldest entry is the end of the walk"
    );

    composer.key(key(KeyCode::Down));
    assert_eq!(composer.text(), "second");
    composer.key(key(KeyCode::Down));
    assert_eq!(composer.text(), "third");
    composer.key(key(KeyCode::Down));
    assert_eq!(
        composer.text(),
        "a draft",
        "walking past the newest entry returns what was being typed",
    );
}

#[test]
fn a_repeated_prompt_is_one_history_entry() {
    let mut composer = Composer::new();
    for _ in 0..3 {
        type_text(&mut composer, "cargo test");
        composer.key(key(KeyCode::Enter));
    }
    assert_eq!(composer.history(), ["cargo test"]);
}

#[test]
fn the_arrows_move_the_cursor_inside_a_multiline_prompt() {
    let mut composer = Composer::new();
    type_text(&mut composer, "one");
    composer.key(key(KeyCode::Enter)); // submitted, so there is history to recall
    type_text(&mut composer, "alpha");
    composer.key(KeyEvent::new(KeyCode::Enter, KeyModifiers::SHIFT));
    type_text(&mut composer, "beta");

    // The cursor is on the second line, so `Up` must move within the text rather
    // than replacing it with the previous prompt.
    composer.key(key(KeyCode::Up));
    assert_eq!(
        composer.text(),
        "alpha\nbeta",
        "history stole a cursor move"
    );
}

#[test]
fn a_paste_arrives_whole_rather_than_as_keystrokes() {
    let mut composer = Composer::new();
    composer.paste("line one\nline two\nline three");

    assert_eq!(composer.text(), "line one\nline two\nline three");
    assert_eq!(
        composer.key(key(KeyCode::Enter)),
        Reply::Submitted("line one\nline two\nline three".into()),
        "a pasted block is one prompt, not three",
    );
}

#[test]
fn f9_the_composer_is_usable_at_eighty_columns() {
    let (mut screen, recorder) = support::screen(80, 24);
    let mut composer = Composer::new();
    type_text(
        &mut composer,
        "a prompt long enough to need most of an eighty column terminal",
    );

    screen
        .draw(|frame| composer.render(frame, frame.area(), &DARK))
        .expect("frame");

    let viewport = screen.viewport_text();
    assert!(
        viewport.contains(PROMPT.trim()),
        "the prompt marker is missing"
    );
    assert!(
        viewport.contains("eighty column terminal"),
        "the end of the prompt was cut off at eighty columns: {viewport:?}",
    );

    // Asserted on the viewport rather than on the byte stream, deliberately.
    // ratatui writes only the cells that differ from the previous frame, and a
    // space that was already a space is not one of them — so viewport text
    // arrives in the stream split at its spaces, with a cursor move in between.
    // Committed content has no such problem, because `insert_before` draws every
    // cell of a fresh buffer, which is why the scrollback tests do read the bytes.
    assert!(
        !recorder.text().is_empty(),
        "the frame never reached the terminal at all"
    );
}

#[test]
fn the_real_cursor_is_never_hidden_while_input_is_possible() {
    // A hidden cursor removes the only focus indicator a screen reader has, and
    // ratatui hides it on any frame that does not set a position. The composer
    // sets one, so this holds without any caller having to remember.
    let (mut screen, recorder) = support::screen(80, 24);
    let mut composer = Composer::new();
    type_text(&mut composer, "typing");

    for _ in 0..3 {
        screen
            .draw(|frame| composer.render(frame, frame.area(), &DARK))
            .expect("frame");
    }

    assert!(
        !recorder.contains("\x1b[?25l"),
        "the byte stream hides the cursor",
    );
}

#[test]
fn f9_a_prompt_wider_than_the_terminal_asks_for_more_rows() {
    let mut composer = Composer::new();
    type_text(&mut composer, &"x".repeat(200));

    assert_eq!(
        composer.height(80),
        3,
        "two hundred characters need three rows of seventy-eight",
    );
    // The prompt marker takes two of the columns, so two hundred characters need
    // two hundred and two columns to sit on one row.
    assert_eq!(composer.height(202), 1);
    assert_eq!(composer.height(200), 2);
}
