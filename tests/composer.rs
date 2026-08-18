//! The composer, and its share of F9: it stays usable at eighty columns.

mod support;

use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use io_cli::composer::{Composer, Reply, PASTE_THRESHOLD, PROMPT};
use io_cli::glyphs::ASCII;
use io_cli::theme::DARK;
use ratatui::layout::Rect;

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

/// The two rows of the viewport `app` gives the composer, taken from the frame
/// rather than built from constants: an inline viewport does not start at the top
/// of the terminal, and a rectangle outside it is not somewhere a frame can draw.
fn two_rows(area: Rect) -> Rect {
    Rect { height: 2, ..area }
}

/// A paste far too large for the two rows the composer is drawn in: a file, with
/// a distinctive line at each end so a test can tell whole from truncated.
fn big_paste() -> String {
    let mut text = String::from("fn main() {\n");
    for index in 0..40 {
        text.push_str(&format!("    println!(\"line {index}\");\n"));
    }
    text.push_str("}\n// the last line of the paste\n");
    text
}

/// A paste large enough to collapse and made of nothing but whitespace: a blank
/// region of a file, a run of indentation, a column of empty lines. Every other
/// fixture here pastes something visible, which is why the collapsed form and
/// the expanded one were free to disagree about whether the prompt is empty.
fn blank_paste() -> String {
    let text = "    \n".repeat(40);
    assert!(
        text.chars().count() > PASTE_THRESHOLD,
        "the fixture has to be over the threshold or it never collapses",
    );
    text
}

#[test]
fn f6_a_paste_of_nothing_but_whitespace_leaves_an_empty_composer() {
    // The placeholder is on screen, so what is rendered is not blank. What it
    // stands for is. `Enter` reads the expanded text and will not send it, and
    // `is_empty` used to read the screen and say the composer was full — so the
    // operator held a prompt that looked full, could not be sent, could not be
    // exited with `Ctrl+D` and whose `Esc` cleared instead of arming the rewind,
    // with nothing on the frame saying why.
    let mut composer = Composer::new();
    composer.paste(&blank_paste());

    assert_eq!(composer.height(80), 1, "the paste was not collapsed");
    assert_eq!(
        composer.key(key(KeyCode::Enter)),
        Reply::Idle,
        "whitespace is not a prompt, however it arrived",
    );
    assert!(
        composer.is_empty(),
        "the composer claims to hold something it will not send",
    );
}

#[test]
fn f6_a_blank_paste_beside_typed_text_still_arrives_whole() {
    // The other half of the same question: emptiness is decided on the expanded
    // text, and deciding it there must not start trimming the text that is sent.
    let blank = blank_paste();
    let mut composer = Composer::new();
    type_text(&mut composer, "run this on: ");
    composer.paste(&blank);

    assert!(!composer.is_empty(), "there is a prompt here to send");
    assert_eq!(
        composer.key(key(KeyCode::Enter)),
        Reply::Submitted(format!("run this on: {blank}")),
        "the whitespace was trimmed out of a paste that arrives byte for byte",
    );
}

#[test]
fn f6_a_blank_paste_does_not_empty_a_prompt_that_holds_another_one() {
    // Two pastes, one blank and one a whole file: the prompt is as full as the
    // fullest thing in it, and both blocks come back in the order they arrived.
    let blank = blank_paste();
    let code = big_paste();
    let mut composer = Composer::new();
    composer.paste(&blank);
    composer.paste(&code);

    assert!(
        !composer.is_empty(),
        "one of the two pastes is a whole file"
    );
    assert_eq!(
        composer.key(key(KeyCode::Enter)),
        Reply::Submitted(format!("{blank}{code}")),
    );
}

#[test]
fn f6_a_large_paste_leaves_one_line_naming_what_it_is() {
    let paste = big_paste();
    let mut composer = Composer::new();
    composer.paste(&paste);

    assert_eq!(
        composer.height(80),
        1,
        "a collapsed paste is one line, whatever it was pasted from",
    );

    let (mut screen, _recorder) = support::screen(80, 24);
    screen
        .draw(|frame| composer.render(frame, two_rows(frame.area()), &DARK))
        .expect("frame");
    let viewport = screen.viewport_text();

    let content = viewport
        .find("pasted text")
        .unwrap_or_else(|| panic!("the placeholder does not say what it is: {viewport:?}"));
    let size = viewport
        .find(&format!("{} characters", paste.chars().count()))
        .unwrap_or_else(|| panic!("the placeholder does not say how large it is: {viewport:?}"));
    assert!(
        content < size,
        "metadata came before the content it describes: {viewport:?}",
    );
    assert!(
        !viewport.contains("println!"),
        "the paste flooded the composer's two rows: {viewport:?}",
    );
}

#[test]
fn f6_the_pasted_text_reaches_the_agent_whole() {
    // The half of F6 the criterion actually turns on, and the half a placeholder
    // that is never expanded passes on the screen while losing the file. Asserted
    // on what leaves `Reply::Submitted` — which is what `app` hands the session —
    // rather than on anything rendered.
    let paste = big_paste();
    let mut composer = Composer::new();
    type_text(&mut composer, "review this: ");
    composer.paste(&paste);

    assert_eq!(
        composer.key(key(KeyCode::Enter)),
        Reply::Submitted(format!("review this: {paste}")),
        "the placeholder was submitted instead of the text it stands for",
    );
    assert_eq!(
        composer.history(),
        [format!("review this: {paste}")],
        "history kept the placeholder, so recalling the prompt would lose the paste",
    );
}

#[test]
fn f6_the_threshold_collapses_only_what_floods_the_two_rows() {
    let under = "x".repeat(PASTE_THRESHOLD);
    let mut composer = Composer::new();
    composer.paste(&under);
    assert_eq!(
        composer.height(80),
        2,
        "a paste that fits the two rows is inserted as it always was",
    );
    assert_eq!(composer.text(), under);

    // One character more is the first one the operator cannot see.
    let over = "x".repeat(PASTE_THRESHOLD + 1);
    let mut composer = Composer::new();
    composer.paste(&over);
    assert_eq!(composer.height(80), 1, "the paste was not collapsed");
    assert_eq!(
        composer.key(key(KeyCode::Enter)),
        Reply::Submitted(over),
        "collapsing lost a character somewhere",
    );
}

#[test]
fn f6_a_deleted_placeholder_does_not_resurrect_its_text() {
    let mut composer = Composer::new();
    composer.paste(&big_paste());
    for _ in 0..200 {
        if composer.is_empty() {
            break;
        }
        composer.key(key(KeyCode::Backspace));
    }
    type_text(&mut composer, "never mind");

    assert_eq!(
        composer.key(key(KeyCode::Enter)),
        Reply::Submitted("never mind".into()),
        "a paste the operator deleted came back on submit",
    );
}

#[test]
fn f6_two_pastes_each_expand_to_their_own_text() {
    // Both blocks are the same size, so a placeholder that named only the size
    // would collide and send the first block twice.
    let first = "a".repeat(PASTE_THRESHOLD + 1);
    let second = "b".repeat(PASTE_THRESHOLD + 1);
    let mut composer = Composer::new();
    composer.paste(&first);
    type_text(&mut composer, " and ");
    composer.paste(&second);

    assert_eq!(
        composer.key(key(KeyCode::Enter)),
        Reply::Submitted(format!("{first} and {second}")),
    );
}

#[test]
fn f6_a_paste_survives_a_walk_through_history() {
    // Walking back and forward rebuilds the composer's text, and a rebuild that
    // dropped the pasted blocks would leave a placeholder standing for nothing.
    let paste = big_paste();
    let mut composer = Composer::new();
    type_text(&mut composer, "first");
    composer.key(key(KeyCode::Enter));
    composer.paste(&paste);

    composer.key(key(KeyCode::Up));
    assert_eq!(composer.text(), "first");
    composer.key(key(KeyCode::Down));

    assert_eq!(
        composer.height(80),
        1,
        "the draft came back expanded rather than collapsed",
    );
    assert_eq!(
        composer.key(key(KeyCode::Enter)),
        Reply::Submitted(paste),
        "the walk through history lost the pasted text",
    );
}

#[test]
fn f6_a_recalled_prompt_carries_its_pasted_text_back() {
    // A recalled prompt is the whole text again, not a placeholder: the blocks
    // belong to the prompt being written, and submitting cleared them.
    let paste = big_paste();
    let mut composer = Composer::new();
    composer.paste(&paste);
    composer.key(key(KeyCode::Enter));

    composer.key(key(KeyCode::Up));
    assert_eq!(composer.text(), paste);
    assert_eq!(composer.key(key(KeyCode::Enter)), Reply::Submitted(paste));
}

#[test]
fn f6_the_placeholder_line_draws_under_the_ascii_glyph_set() {
    let (mut screen, _recorder) = support::screen(80, 24);
    let mut composer = Composer::new();
    composer.paste(&big_paste());

    screen
        .draw(|frame| composer.render(frame, two_rows(frame.area()), &DARK.with_glyphs(ASCII)))
        .expect("frame");

    let viewport = screen.viewport_text();
    assert!(
        viewport.contains("pasted text"),
        "the placeholder is missing: {viewport:?}",
    );
    assert!(
        viewport.is_ascii(),
        "the placeholder drew something the ASCII set cannot: {viewport:?}",
    );
}
