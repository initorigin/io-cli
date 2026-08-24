//! The composer, and its share of F9: it stays usable at eighty columns.

mod support;

use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use io_cli::app::App;
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

/// F7: a paste reaches the composer when nothing modal is up.
///
/// The control for the two refusals below. Without it, a `paste` that refused
/// everything would satisfy them both.
#[test]
fn f7_a_paste_reaches_the_composer_when_nothing_is_in_the_way() {
    let mut app = App::new(DARK, "opus-5");

    assert!(
        app.paste("from the clipboard", false),
        "nothing was open, so the paste had nowhere else to go",
    );
    assert_eq!(app.composer.text(), "from the clipboard");
}

/// F7: a paste during a turn is not discarded.
///
/// The defect this fixes: the driver's mid-turn arm matched a key press and a
/// resize and let everything else fall into a catch-all, so a bracketed paste
/// while the agent worked was dropped on the floor. Typing already reached the
/// composer on that path, so a paste that did not was the same keystroke
/// treated two ways.
#[test]
fn f7_a_paste_while_a_turn_is_running_is_not_dropped() {
    let mut app = App::new(DARK, "opus-5");
    app.started();

    assert!(app.paste("typed while it worked", false));
    assert_eq!(app.composer.text(), "typed while it worked");
}

/// 0.11.0 — pasting the same block twice expands it in place.
///
/// The first paste collapses to a placeholder because a screenful of somebody
/// else's text is not a prompt anyone can read. Pressing paste again on the same
/// block is the operator saying they want it after all, so the placeholder is
/// replaced by what it stands for rather than a second placeholder being added
/// beside the first.
#[test]
fn a_second_paste_of_the_same_block_expands_it() {
    let block = "x".repeat(PASTE_THRESHOLD + 1);
    let mut composer = Composer::new();
    composer.paste(&block);
    assert!(
        composer.typed().contains("pasted text #1"),
        "{:?}",
        composer.typed(),
    );

    composer.paste(&block);
    assert_eq!(
        composer.typed(),
        block,
        "the second paste of one block shows the block",
    );
    assert_eq!(composer.text(), block);

    // A DIFFERENT block still collapses, and is numbered as the paste it is.
    let other = "y".repeat(PASTE_THRESHOLD + 1);
    composer.paste(&other);
    assert!(composer.typed().starts_with(&block));
    assert!(composer.typed().contains("pasted text #2"));
    assert_eq!(composer.text(), format!("{block}{other}"));
}

/// 0.11.0 — backspace over a placeholder removes the whole thing.
///
/// Thirty-five presses to remove one thing the operator thinks of as one thing is
/// bad enough. The first of them is worse: a placeholder is matched by its exact
/// text, so an edited one silently stops standing for the block it named and the
/// prompt sends the words `[pasted text #1, 157 characters]` to the model.
#[test]
fn backspace_removes_a_placeholder_whole() {
    let block = "x".repeat(PASTE_THRESHOLD + 1);
    let mut composer = Composer::new();
    type_text(&mut composer, "look at ");
    composer.paste(&block);

    composer.key(key(KeyCode::Backspace));
    assert_eq!(composer.typed(), "look at ");
    assert_eq!(
        composer.text(),
        "look at ",
        "the block went with the placeholder standing for it",
    );

    // And an ordinary character still deletes one character.
    composer.key(key(KeyCode::Backspace));
    assert_eq!(composer.typed(), "look at");
}

/// 0.11.0 — a pasted path is quoted and resolved.
///
/// Dragging a file into a terminal pastes its path, and a path with a space in it
/// is two words to everything downstream unless something quotes it. Prose is
/// never quoted at anybody: the check is that the path names a file that exists.
#[test]
fn a_pasted_path_is_quoted_and_prose_is_not() {
    let dir = tempfile::tempdir().expect("a directory");
    let file = dir.path().join("a picture.png");
    std::fs::write(&file, b"not really a picture").expect("the fixture file");

    let mut composer = Composer::new();
    composer.paste(&file.display().to_string());
    let typed = composer.typed();
    assert!(typed.starts_with('"') && typed.ends_with('"'), "{typed:?}");
    assert!(typed.contains("a picture.png"), "{typed:?}");

    // The shell-escaped form a drag produces resolves to the same thing.
    let mut escaped = Composer::new();
    escaped.paste(&file.display().to_string().replace(' ', "\\ "));
    assert_eq!(escaped.typed(), typed);

    // A sentence that is not a path is pasted exactly as it arrived.
    let mut prose = Composer::new();
    prose.paste("look at the picture in my documents");
    assert_eq!(prose.typed(), "look at the picture in my documents");
}

/// F6's other half — a pasted path is quoted, never debug-escaped.
///
/// `format!("{path:?}")` is what this wrote up to 0.13.0, and `Debug` for a
/// string escapes every character Rust considers unprintable — including the
/// U+202F narrow no-break space macOS puts in every screenshot's name. What
/// landed on the prompt was `\u{202f}` as six literal characters, so the path
/// named no file even once `/attach` took the quotes off.
#[test]
fn f6_a_pasted_path_keeps_the_characters_it_was_pasted_with() {
    let dir = tempfile::tempdir().expect("a temporary directory");
    let name = "Screenshot 2026-08-24 at 8.00.01\u{202f}AM.png";
    let path = dir.path().join(name);
    std::fs::write(&path, b"not really a png").expect("write");

    let mut composer = Composer::new();
    composer.paste(&path.display().to_string());
    let typed = composer.typed();

    assert!(
        typed.contains('\u{202f}'),
        "the narrow no-break space was escaped out of the path: {typed:?}",
    );
    assert!(
        !typed.contains("\\u{"),
        "a debug escape reached the prompt line: {typed:?}",
    );
    // Quoted, because that is what keeps a path with a space in it one word for
    // everything downstream.
    assert!(typed.starts_with('"') && typed.ends_with('"'), "{typed:?}");
    assert!(typed.contains(name), "{typed:?}");
}

/// What the composer writes, `/attach` reads. The two halves are tested apart —
/// `tests/attach.rs` owns the reading — so this asserts they meet.
#[test]
fn f6_what_the_composer_quotes_is_what_attach_unquotes() {
    let dir = tempfile::tempdir().expect("a temporary directory");
    let name = "one two.png";
    let path = dir.path().join(name);
    std::fs::write(&path, support::png_bytes(2, 2)).expect("write");

    let mut composer = Composer::new();
    composer.paste(&path.display().to_string());

    // Canonicalized, because the composer canonicalizes what it quotes and macOS
    // hands out `/var/…` for a `/private/var/…` temporary directory. A workspace
    // rooted at one and handed a path under the other is a path that escapes it.
    let root = dir.path().canonicalize().expect("a real path");
    let staged = io_cli::attach::prepare(
        &root,
        &io_harness::Policy::permissive(),
        true,
        &composer.text(),
    )
    .unwrap_or_else(|error| panic!("what the composer wrote was refused: {error}"));
    assert_eq!(staged.media_type, "image/png");
}

/// F9 — one cursor, and the rows the prompt is actually on.
///
/// `tui-textarea` scrolls sideways rather than wrapping, and it paints its own
/// inverted block at its own idea of the insertion point. io-cli measures
/// everything — the viewport's height, the rows the composer asks for, the caret
/// it places — as if the text wrapped. So a prompt long enough to wrap was drawn
/// clipped at the left with **two** cursor blocks on it, in different places,
/// which is how 0.13.1 was reported. The composer draws its own wrapped rows now.
mod one_cursor {
    use super::*;

    /// Every cell the frame drew, row by row, and where the terminal cursor was
    /// left. A block cursor is a *cell* to a terminal, so the only way to count
    /// cursors is to look at what was painted.
    fn drawn(composer: &Composer, width: u16, height: u16) -> (Vec<String>, (u16, u16)) {
        let (mut screen, _recorder) = support::screen_of(width, 24, height);
        let seen = std::cell::Cell::new((0, 0));
        screen
            .draw(|frame| {
                let area = frame.area();
                composer.render(frame, area, &DARK);
                let (x, y) = composer.cursor(Rect {
                    x: area.x + PROMPT.len() as u16,
                    width: area.width - PROMPT.len() as u16,
                    ..area
                });
                // Relative to the viewport: an inline viewport does not start at
                // the top of the terminal, and the row a test means is the row
                // inside the composer.
                seen.set((x - area.x, y - area.y));
            })
            .expect("frame");
        (
            screen
                .viewport_text()
                .lines()
                .map(str::to_string)
                .collect(),
            seen.get(),
        )
    }

    /// **The assertion that catches the second cursor**, and it has to read the
    /// bytes rather than the text: a cursor block is a *style*, not a character.
    /// `tui-textarea` paints its insertion point as a reverse-video cell — SGR 7
    /// — while the real terminal cursor is placed with a cursor-position escape
    /// and writes no style at all. So a frame of the composer that carries a `7m`
    /// is a frame with a second cursor drawn into it, which is what an operator
    /// saw as two blocks in two places.
    #[test]
    fn f9_the_composer_paints_no_second_cursor() {
        let mut composer = Composer::new();
        type_text(&mut composer, &"abcdefghij".repeat(6));

        let (mut screen, recorder) = support::screen_of(22, 24, 4);
        screen
            .draw(|frame| composer.render(frame, frame.area(), &DARK))
            .expect("frame");

        let written = recorder.text();
        assert!(
            !written.contains("\x1b[7m"),
            "the composer painted a reverse-video cell, which is a cursor block \
             drawn on top of the one the terminal already has: {written:?}",
        );
    }

    #[test]
    fn f9_a_wrapped_prompt_is_drawn_wrapped_rather_than_scrolled_sideways() {
        let mut composer = Composer::new();
        // Three rows' worth at a width of twenty-two: twenty usable columns.
        type_text(&mut composer, &"abcdefghij".repeat(6));

        let (rows, _) = drawn(&composer, 22, 4);
        assert!(
            rows[0].starts_with("> abcdefghij"),
            "the first row starts at the start of the prompt: {rows:?}",
        );
        assert_eq!(
            rows.iter().filter(|row| row.contains("abcdefghij")).count(),
            3,
            "sixty characters at twenty columns are three drawn rows: {rows:?}",
        );
    }

    #[test]
    fn f9_the_caret_is_on_the_row_the_text_is_on() {
        let mut composer = Composer::new();
        type_text(&mut composer, &"x".repeat(25));

        let (_, (x, y)) = drawn(&composer, 22, 4);
        // Twenty-five characters at twenty usable columns: the twenty-sixth cell
        // is the sixth column of the second row.
        assert_eq!((x, y), (2 + 5, 1), "the caret is where the next character goes");
    }

    #[test]
    fn f9_the_window_follows_the_insertion_point() {
        let mut composer = Composer::new();
        // Ten rows of text in a composer two rows tall.
        type_text(&mut composer, &"y".repeat(200));

        let (rows, (_, y)) = drawn(&composer, 22, 2);
        assert_eq!(rows.len(), 2, "{rows:?}");
        assert!(
            rows.iter().all(|row| row.contains('y')),
            "a prompt taller than its box shows the end of itself: {rows:?}",
        );
        assert_eq!(y, 1, "the caret is on the last drawn row");
    }

    #[test]
    fn f9_a_caret_at_the_end_of_a_full_row_stays_in_the_row() {
        let mut composer = Composer::new();
        type_text(&mut composer, &"z".repeat(20));

        assert_eq!(
            composer.height(22),
            1,
            "a prompt that ends flush with the row does not grow one",
        );
        let (_, (x, y)) = drawn(&composer, 22, 2);
        assert_eq!(
            (x, y),
            (21, 0),
            "the caret rests in the last cell, the way a terminal's own does",
        );
    }

    #[test]
    fn f9_a_multi_line_prompt_draws_every_line() {
        let mut composer = Composer::new();
        type_text(&mut composer, "one");
        composer.key(KeyEvent::new(KeyCode::Enter, KeyModifiers::ALT));
        type_text(&mut composer, "two");
        composer.key(KeyEvent::new(KeyCode::Enter, KeyModifiers::ALT));
        type_text(&mut composer, "three");

        let (rows, (_, y)) = drawn(&composer, 40, 4);
        assert!(rows[0].contains("one"), "{rows:?}");
        assert!(rows[1].contains("two"), "{rows:?}");
        assert!(rows[2].contains("three"), "{rows:?}");
        assert_eq!(y, 2, "the caret is on the line being typed");
    }

    /// Backspacing back through a newline, which is the sequence the two-cursor
    /// glitch was reported from: the caret follows the text back up.
    #[test]
    fn f9_backspacing_through_a_newline_brings_the_caret_back_with_it() {
        let mut composer = Composer::new();
        type_text(&mut composer, "abc");
        composer.key(KeyEvent::new(KeyCode::Enter, KeyModifiers::ALT));
        type_text(&mut composer, "d");

        let (_, (_, before)) = drawn(&composer, 40, 4);
        assert_eq!(before, 1);

        composer.key(key(KeyCode::Backspace));
        composer.key(key(KeyCode::Backspace));
        let (rows, (x, y)) = drawn(&composer, 40, 4);
        assert_eq!((x, y), (2 + 3, 0), "the caret is back on the first row: {rows:?}");
        assert_eq!(composer.height(40), 1, "and the composer wants one row again");
    }
}

/// F6 — pasting the same block again toggles it, and never piles up.
///
/// Expanding used to drop the placeholder from the prompt while leaving the
/// block in `pastes`, so the *next* paste of the same clipboard matched nothing
/// and appended a fresh placeholder after text that was already there:
/// `[pasted text #2, 462 characters]`, then `#3`, then `#4`. An operator hit it
/// in the first minute of 0.13.1.
#[test]
fn f6_pasting_the_same_block_again_toggles_it_rather_than_piling_up() {
    let paste = big_paste();
    let mut composer = Composer::new();

    composer.paste(&paste);
    let collapsed = composer.typed();
    assert!(collapsed.contains("[pasted text #1"), "{collapsed:?}");

    composer.paste(&paste);
    let expanded = composer.typed();
    assert!(
        expanded.contains("the last line of the paste"),
        "the second paste shows the block: {expanded:?}",
    );

    composer.paste(&paste);
    let again = composer.typed();
    assert_eq!(
        again, collapsed,
        "the third paste puts it back the way the first one had it",
    );
    assert!(
        !again.contains("#2"),
        "a repeat paste must not add a second placeholder: {again:?}",
    );

    // Four more presses, because the defect was cumulative and one round trip
    // would not have caught it.
    for _ in 0..4 {
        composer.paste(&paste);
    }
    let typed = composer.typed();
    assert!(!typed.contains("#2"), "{typed:?}");
    // Whichever way it ended, the prompt is still exactly one copy of the block.
    assert_eq!(composer.text(), paste);
}

/// F6 — a placeholder deletes as one thing, whichever backwards deletion key
/// the reader has in their fingers.
///
/// The arm used to exclude `Alt`, so `Option+Backspace` — the delete-word every
/// macOS reader uses — fell through to the widget and ate the placeholder one
/// word at a time, leaving `[pasted text #8, 464 chara` on the prompt. A
/// placeholder is matched by its exact text, so the first press had already
/// stopped it standing for the block it named.
#[test]
fn f6_every_backwards_deletion_takes_a_placeholder_whole() {
    let paste = big_paste();
    for key in [
        KeyEvent::new(KeyCode::Backspace, KeyModifiers::NONE),
        KeyEvent::new(KeyCode::Backspace, KeyModifiers::ALT),
        KeyEvent::new(KeyCode::Backspace, KeyModifiers::CONTROL),
        KeyEvent::new(KeyCode::Char('w'), KeyModifiers::CONTROL),
    ] {
        let mut composer = Composer::new();
        type_text(&mut composer, "look at ");
        composer.paste(&paste);
        assert!(composer.typed().contains("[pasted text #1"), "{key:?}");

        composer.key(key);

        assert_eq!(
            composer.typed(),
            "look at ",
            "{key:?} left part of a placeholder behind",
        );
        assert_eq!(
            composer.text(),
            "look at ",
            "{key:?} left the block on the prompt with nothing standing for it",
        );
    }
}

/// And a deletion that is not at a placeholder is still the widget's own.
#[test]
fn f6_a_word_delete_away_from_a_placeholder_still_deletes_a_word() {
    let mut composer = Composer::new();
    type_text(&mut composer, "one two three");

    composer.key(KeyEvent::new(KeyCode::Backspace, KeyModifiers::ALT));

    let typed = composer.typed();
    assert!(
        typed.starts_with("one two") && !typed.contains("three"),
        "a word-wise delete away from a placeholder still deletes a word: {typed:?}",
    );
}

/// F10 — a block keeps its number for the life of the prompt.
///
/// Expand a paste, edit the text, and neither form is in the prompt any more —
/// the placeholder is gone and the block is no longer there verbatim. That used
/// to mint `#2`, then `#3`, then `#4` for the same clipboard, and none of them
/// could be toggled either: each stood for a block whose expanded form was
/// already on the prompt under somebody's edits.
#[test]
fn f10_editing_an_expanded_paste_does_not_mint_a_new_number() {
    let paste = big_paste();
    let mut composer = Composer::new();

    composer.paste(&paste);
    composer.paste(&paste);
    assert!(composer.typed().contains("the last line of the paste"));

    // Edit it: five characters off the end, which is what breaks the verbatim
    // match the toggle looked for.
    for _ in 0..5 {
        composer.key(key(KeyCode::Backspace));
    }

    // Three more presses of the same clipboard.
    composer.paste(&paste);
    let once = composer.typed();
    assert!(once.contains("[pasted text #1"), "{once:?}");
    assert!(!once.contains("#2"), "a second number was minted: {once:?}");

    // And the toggle is working again immediately: the placeholder is on screen,
    // so the next press expands it rather than adding anything.
    composer.paste(&paste);
    let twice = composer.typed();
    assert!(!twice.contains("[pasted text #1"), "{twice:?}");
    assert!(!twice.contains("#2"), "{twice:?}");

    composer.paste(&paste);
    let thrice = composer.typed();
    assert!(thrice.contains("[pasted text #1"), "{thrice:?}");
    assert!(!thrice.contains("#2"), "{thrice:?}");
}
