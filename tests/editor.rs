//! The editing model, on its own.
//!
//! `tests/composer.rs` and `tests/wizard.rs` own what an operator sees; this file
//! owns the buffer underneath them — the cursor arithmetic, the boundaries where
//! an edit either joins two rows or does nothing at all, and the masking the
//! credential field's whole promise rests on.
//!
//! **Columns are characters here, never bytes**, and that is most of what these
//! tests are for. io-cli's prompt takes pasted paths with a narrow no-break space
//! in them, prose in scripts that are three bytes to the character, and emoji; a
//! column that counted bytes would put the caret inside a character and panic on
//! the next keystroke.

use crossterm::event::{KeyCode, KeyEvent, KeyEventKind, KeyEventState, KeyModifiers};
use io_cli::editor::Editor;

fn key(code: KeyCode) -> KeyEvent {
    KeyEvent::new(code, KeyModifiers::NONE)
}

fn chord(code: KeyCode, modifiers: KeyModifiers) -> KeyEvent {
    KeyEvent::new(code, modifiers)
}

fn typed(editor: &mut Editor, text: &str) {
    for character in text.chars() {
        editor.key(key(KeyCode::Char(character)));
    }
}

/// Everything in the buffer, rows joined the way [`io_cli::composer`] joins them.
fn text(editor: &Editor) -> String {
    editor.lines().join("\n")
}

fn holding(text: &str) -> Editor {
    let mut editor = Editor::new();
    typed(&mut editor, text);
    editor
}

/// `Ctrl+U`, pressed the way an operator presses it.
///
/// Every undo test goes through the **key** rather than through `Editor::undo`,
/// because the key is what regressed: io-cli forwards raw crossterm events into
/// the buffer, the buffer this replaced bound `Ctrl+U` itself
/// (`tui-textarea-0.7.0/src/textarea.rs:576-587`), and a working `undo()` that
/// nothing was bound to would be the same silent loss.
fn undo(editor: &mut Editor) {
    editor.key(chord(KeyCode::Char('u'), KeyModifiers::CONTROL));
}

/// `Ctrl+R`, for the same reason.
fn redo(editor: &mut Editor) {
    editor.key(chord(KeyCode::Char('r'), KeyModifiers::CONTROL));
}

// --- typing, and where the cursor ends up ---

#[test]
fn typing_puts_the_characters_in_and_the_cursor_after_them() {
    let editor = holding("fix the failing test");
    assert_eq!(text(&editor), "fix the failing test");
    assert_eq!(editor.cursor(), (0, 20));
    assert_eq!(editor.lines().len(), 1, "nothing opened a second row");
}

#[test]
fn a_capital_arrives_as_itself_rather_than_as_a_modifier() {
    // crossterm reports a capital as the capital *with* `SHIFT` still set. An
    // editor that read the modifier before the character would refuse to type
    // one, which is the whole alphabet gone.
    let mut editor = Editor::new();
    editor.key(chord(KeyCode::Char('A'), KeyModifiers::SHIFT));
    assert_eq!(text(&editor), "A");
}

#[test]
fn a_key_release_is_not_a_second_keystroke() {
    // Windows, and any terminal with the Kitty keyboard protocol on, reports the
    // release of every key it reported the press of. Acting on both is every
    // character typed twice.
    let mut editor = Editor::new();
    editor.key(KeyEvent::new_with_kind(
        KeyCode::Char('x'),
        KeyModifiers::NONE,
        KeyEventKind::Press,
    ));
    editor.key(KeyEvent {
        code: KeyCode::Char('x'),
        modifiers: KeyModifiers::NONE,
        kind: KeyEventKind::Release,
        state: KeyEventState::NONE,
    });
    assert_eq!(text(&editor), "x");
}

#[test]
fn a_tab_is_spaces_to_the_next_stop() {
    let mut editor = holding("ab");
    editor.key(key(KeyCode::Tab));
    assert_eq!(text(&editor), "ab  ");
    editor.key(key(KeyCode::Tab));
    assert_eq!(text(&editor), "ab      ", "a full stop from a full column");
}

// --- multi-byte characters ---

#[test]
fn the_cursor_counts_characters_and_not_bytes() {
    // Five characters, six bytes. A column that counted bytes would report six
    // here, and the composer's wrap would put the caret a cell to the right of
    // where the text is.
    let editor = holding("héllo");
    assert_eq!(editor.cursor(), (0, 5));
    assert_eq!(text(&editor).len(), 6, "the fixture really is multi-byte");
}

#[test]
fn the_arrows_walk_a_multi_byte_line_one_character_at_a_time() {
    let mut editor = holding("héllo");
    for expected in (0..5).rev() {
        editor.key(key(KeyCode::Left));
        assert_eq!(editor.cursor(), (0, expected));
    }
    editor.key(key(KeyCode::Left));
    assert_eq!(
        editor.cursor(),
        (0, 0),
        "there is nowhere further back to go"
    );
    for expected in 1..=5 {
        editor.key(key(KeyCode::Right));
        assert_eq!(editor.cursor(), (0, expected));
    }
}

#[test]
fn deleting_takes_one_whole_character_however_many_bytes_it_is() {
    // Four bytes to the character, and it is the pasted-screenshot case: an
    // editor that removed a byte would leave the buffer holding something that
    // is not text at all.
    let mut editor = holding("🙂🙂🙂");
    assert_eq!(editor.cursor(), (0, 3));
    editor.key(key(KeyCode::Backspace));
    assert_eq!(text(&editor), "🙂🙂");
    assert_eq!(editor.cursor(), (0, 2));
}

#[test]
fn inserting_in_the_middle_of_a_multi_byte_line_lands_on_the_boundary() {
    let mut editor = holding("héllo");
    editor.key(key(KeyCode::Left));
    editor.key(key(KeyCode::Left));
    typed(&mut editor, "X");
    assert_eq!(text(&editor), "hélXlo");
}

#[test]
fn the_narrow_no_break_space_a_screenshot_carries_survives_a_round_trip() {
    // The exact character macOS puts between the time and the `AM` in every
    // screenshot's name — whitespace, three bytes, and the one that has already
    // cost this product a release.
    let name = "Screenshot 2026-08-24 at 8.00.01\u{202f}AM.png";
    let mut editor = Editor::new();
    editor.insert_str(name);
    assert_eq!(text(&editor), name);
    assert_eq!(editor.cursor(), (0, name.chars().count()));
}

// --- boundaries: where an edit joins two rows, or does nothing ---

#[test]
fn insert_str_opens_a_row_for_every_newline_and_keeps_the_tail() {
    let mut editor = holding("start end");
    for _ in 0..3 {
        editor.key(key(KeyCode::Left));
    }
    editor.insert_str("one\ntwo\n");

    assert_eq!(text(&editor), "start one\ntwo\nend");
    assert_eq!(
        editor.cursor(),
        (2, 0),
        "the caret is where the text ran out"
    );
}

#[test]
fn a_carriage_return_before_a_newline_is_not_a_row_of_its_own() {
    let mut editor = Editor::new();
    editor.insert_str("one\r\ntwo");
    assert_eq!(editor.lines(), ["one", "two"]);
}

#[test]
fn backspace_at_the_head_of_a_row_joins_it_to_the_one_above() {
    let mut editor = Editor::new();
    editor.insert_str("abc\nd");
    editor.key(key(KeyCode::Backspace));
    editor.key(key(KeyCode::Backspace));

    assert_eq!(text(&editor), "abc");
    assert_eq!(editor.lines().len(), 1);
    assert_eq!(
        editor.cursor(),
        (0, 3),
        "the caret comes back up with the text",
    );
}

#[test]
fn backspace_at_the_very_start_does_nothing_at_all() {
    let mut editor = Editor::new();
    editor.key(key(KeyCode::Backspace));
    assert_eq!(text(&editor), "");
    assert_eq!(editor.cursor(), (0, 0));
    assert_eq!(editor.lines().len(), 1, "there is always one row");
}

#[test]
fn delete_at_the_very_end_does_nothing_at_all() {
    let mut editor = holding("abc");
    editor.key(key(KeyCode::Delete));
    assert_eq!(text(&editor), "abc");
    assert_eq!(editor.cursor(), (0, 3));
}

#[test]
fn delete_at_the_end_of_a_row_pulls_the_next_one_up() {
    let mut editor = Editor::new();
    editor.insert_str("abc\ndef");
    editor.key(key(KeyCode::Up));
    editor.key(chord(KeyCode::Char('e'), KeyModifiers::CONTROL));
    assert_eq!(editor.cursor(), (0, 3));

    editor.key(key(KeyCode::Delete));
    assert_eq!(text(&editor), "abcdef");
    assert_eq!(
        editor.cursor(),
        (0, 3),
        "the caret stayed where the join is"
    );
}

#[test]
fn an_up_arrow_on_the_first_row_leaves_the_cursor_where_it_is() {
    // The composer reads `cursor().0 == 0` to decide whether `Up` recalls a
    // prompt or moves the caret, so an `Up` that wrapped around would take the
    // history key away.
    let mut editor = holding("only one row");
    editor.key(key(KeyCode::Up));
    assert_eq!(editor.cursor(), (0, 12));
}

#[test]
fn the_arrows_keep_the_column_they_can_and_clamp_the_column_they_cannot() {
    let mut editor = Editor::new();
    editor.insert_str("a longer row\nshort\nanother longer row");
    editor.key(chord(KeyCode::Char('e'), KeyModifiers::CONTROL));
    assert_eq!(editor.cursor(), (2, 18));

    editor.key(key(KeyCode::Up));
    assert_eq!(editor.cursor(), (1, 5), "clamped to the short row");
    editor.key(key(KeyCode::Up));
    assert_eq!(
        editor.cursor(),
        (0, 5),
        "and it does not remember the column it wanted",
    );
}

// --- what the composer reads off it ---

#[test]
fn the_rows_are_what_the_composer_wraps_and_the_last_one_is_the_last() {
    let mut editor = Editor::new();
    editor.insert_str("one\ntwo\nthree");
    assert_eq!(editor.lines(), ["one", "two", "three"]);
    assert_eq!(editor.cursor(), (2, 5));
    assert_eq!(
        editor.cursor().0 + 1,
        editor.lines().len(),
        "the composer asks this to decide whether `Down` walks history",
    );
}

#[test]
fn move_to_end_goes_to_the_end_of_the_row_the_cursor_is_on() {
    let mut editor = Editor::new();
    editor.insert_str("alpha\nbeta");
    editor.key(key(KeyCode::Up));
    editor.key(chord(KeyCode::Char('a'), KeyModifiers::CONTROL));
    assert_eq!(editor.cursor(), (0, 0));

    editor.move_to_end();
    assert_eq!(
        editor.cursor(),
        (0, 5),
        "the end of this row, not of the text"
    );
}

#[test]
fn insert_newline_carries_the_rest_of_the_row_down() {
    let mut editor = holding("abcd");
    editor.key(key(KeyCode::Left));
    editor.key(key(KeyCode::Left));
    editor.insert_newline();
    assert_eq!(editor.lines(), ["ab", "cd"]);
    assert_eq!(editor.cursor(), (1, 0));
}

// --- selection, cut and put back ---

#[test]
fn select_all_then_cut_empties_the_buffer_and_keeps_what_it_took() {
    let mut editor = holding("hello");
    editor.select_all();
    assert!(editor.selecting());
    editor.cut();

    assert_eq!(text(&editor), "");
    assert_eq!(editor.lines().len(), 1);
    assert_eq!(editor.cursor(), (0, 0));
    assert!(!editor.selecting(), "the selection went with the text");

    editor.key(chord(KeyCode::Char('y'), KeyModifiers::CONTROL));
    assert_eq!(text(&editor), "hello", "the cut text came back");
}

#[test]
fn a_cut_that_crossed_a_newline_comes_back_as_the_rows_it_was() {
    let mut editor = Editor::new();
    editor.insert_str("one\ntwo\nthree");
    editor.select_all();
    editor.cut();
    assert_eq!(editor.lines(), [""]);

    editor.key(chord(KeyCode::Char('y'), KeyModifiers::CONTROL));
    assert_eq!(
        editor.lines(),
        ["one", "two", "three"],
        "a multi-row cut put back as one row would be a different prompt",
    );
    assert_eq!(editor.cursor(), (2, 5));
}

#[test]
fn select_all_on_an_empty_buffer_selects_nothing_and_cuts_nothing() {
    let mut editor = Editor::new();
    editor.select_all();
    assert!(
        !editor.selecting(),
        "the two ends are the same position, so there is no selection",
    );
    assert!(!editor.cut());
    assert_eq!(editor.lines(), [""]);
}

#[test]
fn a_shifted_arrow_selects_and_the_next_character_replaces_the_selection() {
    let mut editor = holding("hello");
    for _ in 0..3 {
        editor.key(chord(KeyCode::Left, KeyModifiers::SHIFT));
    }
    assert!(editor.selecting());

    typed(&mut editor, "x");
    assert_eq!(text(&editor), "hex");
    assert!(!editor.selecting());
}

#[test]
fn a_bare_arrow_ends_a_selection_without_deleting_it() {
    let mut editor = holding("hello");
    editor.key(chord(KeyCode::Left, KeyModifiers::SHIFT));
    editor.key(chord(KeyCode::Left, KeyModifiers::SHIFT));
    editor.key(key(KeyCode::Left));

    assert!(!editor.selecting());
    assert_eq!(text(&editor), "hello", "moving away is not deleting");
}

#[test]
fn copy_keeps_the_selection_text_and_leaves_the_buffer_alone() {
    let mut editor = holding("hello");
    for _ in 0..5 {
        editor.key(chord(KeyCode::Left, KeyModifiers::SHIFT));
    }
    editor.key(chord(KeyCode::Char('c'), KeyModifiers::CONTROL));
    assert_eq!(text(&editor), "hello");

    editor.move_to_end();
    editor.key(chord(KeyCode::Char('y'), KeyModifiers::CONTROL));
    assert_eq!(text(&editor), "hellohello");
}

#[test]
fn a_selection_that_crosses_rows_deletes_the_newline_with_it() {
    let mut editor = Editor::new();
    editor.insert_str("one\ntwo");
    editor.key(chord(KeyCode::Left, KeyModifiers::SHIFT));
    editor.key(chord(KeyCode::Left, KeyModifiers::SHIFT));
    editor.key(chord(KeyCode::Left, KeyModifiers::SHIFT));
    editor.key(chord(KeyCode::Left, KeyModifiers::SHIFT));
    editor.key(key(KeyCode::Backspace));

    assert_eq!(editor.lines(), ["one"], "the two rows became one");
    assert_eq!(editor.cursor(), (0, 3));
}

// --- the word-wise and line-wise keys ---

#[test]
fn a_word_delete_takes_one_word_and_puts_it_back() {
    let mut editor = holding("one two three");
    editor.key(chord(KeyCode::Char('w'), KeyModifiers::CONTROL));
    assert_eq!(text(&editor), "one two ");

    editor.key(chord(KeyCode::Backspace, KeyModifiers::ALT));
    assert_eq!(text(&editor), "one ");

    editor.key(chord(KeyCode::Char('y'), KeyModifiers::CONTROL));
    assert_eq!(text(&editor), "one two ");
}

#[test]
fn a_word_delete_stops_at_a_separator_rather_than_eating_a_whole_path() {
    // Punctuation is its own kind, which is what makes `Option+Backspace` inside
    // a pasted path stop at the slash instead of taking the path.
    let mut editor = holding("/home/someone/notes.md");
    editor.key(chord(KeyCode::Backspace, KeyModifiers::ALT));
    assert_eq!(text(&editor), "/home/someone/notes.");
}

#[test]
fn ctrl_k_and_ctrl_j_cut_to_the_two_ends_of_the_row() {
    let mut editor = holding("abcde");
    editor.key(key(KeyCode::Left));
    editor.key(key(KeyCode::Left));
    editor.key(key(KeyCode::Left));
    editor.key(chord(KeyCode::Char('k'), KeyModifiers::CONTROL));
    assert_eq!(text(&editor), "ab");

    let mut editor = holding("abcde");
    editor.key(key(KeyCode::Left));
    editor.key(key(KeyCode::Left));
    editor.key(key(KeyCode::Left));
    editor.key(chord(KeyCode::Char('j'), KeyModifiers::CONTROL));
    assert_eq!(text(&editor), "cde");
    assert_eq!(editor.cursor(), (0, 0));
}

#[test]
fn the_word_moves_walk_the_row_without_changing_it() {
    let mut editor = holding("one two three");
    editor.key(chord(KeyCode::Char('b'), KeyModifiers::ALT));
    assert_eq!(editor.cursor(), (0, 8));
    editor.key(chord(KeyCode::Char('b'), KeyModifiers::ALT));
    assert_eq!(editor.cursor(), (0, 4));
    editor.key(chord(KeyCode::Char('f'), KeyModifiers::ALT));
    assert_eq!(editor.cursor(), (0, 8));
    assert_eq!(text(&editor), "one two three");
}

#[test]
fn ctrl_a_and_ctrl_e_go_to_the_ends_of_the_row() {
    let mut editor = holding("a line of text");
    editor.key(chord(KeyCode::Char('a'), KeyModifiers::CONTROL));
    assert_eq!(editor.cursor(), (0, 0));
    editor.key(chord(KeyCode::Char('e'), KeyModifiers::CONTROL));
    assert_eq!(editor.cursor(), (0, 14));
}

// --- masking: F9's sabotage arm drops it, so it has to genuinely mask ---

#[test]
fn a_masked_field_shows_the_mask_and_nothing_of_what_was_typed() {
    let mut editor = Editor::masked('•');
    typed(&mut editor, "aa-bb-v1-secret");

    assert_eq!(editor.shown(), "•".repeat(15));
    assert!(
        !editor.shown().contains("sk-"),
        "a fragment of the credential reached what is drawn: {:?}",
        editor.shown(),
    );
    assert_eq!(
        editor.lines(),
        ["aa-bb-v1-secret"],
        "the buffer still holds it, because the wizard has to read it",
    );
}

#[test]
fn the_mask_is_one_character_per_character_so_the_caret_lands_right() {
    // The caret is placed at `cursor().1` in the *drawn* row, so a mask that was
    // not one for one would put it somewhere the text is not.
    let mut editor = Editor::masked('•');
    typed(&mut editor, "hé🙂o");

    assert_eq!(editor.cursor(), (0, 4));
    assert_eq!(editor.shown().chars().count(), 4);
    assert_eq!(editor.shown(), "••••");
}

#[test]
fn an_unmasked_field_shows_what_was_typed() {
    let mut editor = Editor::new();
    typed(&mut editor, "http://localhost:11434/v1");
    assert_eq!(editor.shown(), "http://localhost:11434/v1");
}

#[test]
fn shown_follows_the_cursor_to_the_row_it_is_on() {
    let mut editor = Editor::new();
    editor.insert_str("first\nsecond");
    assert_eq!(editor.shown(), "second");
    editor.key(key(KeyCode::Up));
    assert_eq!(editor.shown(), "first");
}

// --- undo and redo ---
//
// `Ctrl+U` and `Ctrl+R` are not new keys. The buffer this editor replaced bound
// them itself (`tui-textarea-0.7.0/src/textarea.rs:576-587`) and io-cli forwards
// raw crossterm events straight into it, so an operator has had undo in the
// prompt and in the wizard's credential field since the first release. These
// tests are here so that the next rewrite of this file cannot take it away
// quietly the way the last one did.

#[test]
fn typing_then_undo_then_redo_walks_the_text_back_and_forward() {
    let mut editor = holding("hi");
    assert_eq!(editor.cursor(), (0, 2));

    undo(&mut editor);
    assert_eq!(text(&editor), "h");
    assert_eq!(editor.cursor(), (0, 1));
    undo(&mut editor);
    assert_eq!(text(&editor), "");
    assert_eq!(editor.cursor(), (0, 0));

    redo(&mut editor);
    redo(&mut editor);
    assert_eq!(text(&editor), "hi");
    assert_eq!(
        editor.cursor(),
        (0, 2),
        "and the caret came forward with it"
    );
}

#[test]
fn a_press_takes_back_one_character_and_not_the_whole_word() {
    // **Parity, on purpose.** `TextArea` pushed one edit per typed character and
    // merged none of them (`tui-textarea-0.7.0/src/history.rs:141`), so stepping
    // back one letter at a time is the undo an operator already has. Coalescing
    // a typed run into a single step is a better key — and taking away their
    // ability to drop one character would be the same silent change as dropping
    // the key was.
    let mut editor = holding("cat");

    undo(&mut editor);
    assert_eq!(text(&editor), "ca", "exactly one character, not the word");
    assert_eq!(editor.cursor(), (0, 2));

    undo(&mut editor);
    assert_eq!(text(&editor), "c");
    undo(&mut editor);
    assert_eq!(text(&editor), "");
}

#[test]
fn undo_of_a_character_typed_mid_line_puts_the_caret_back_before_it() {
    let mut editor = holding("hello");
    editor.key(key(KeyCode::Left));
    editor.key(key(KeyCode::Left));
    typed(&mut editor, "X");
    assert_eq!(text(&editor), "helXlo");
    assert_eq!(editor.cursor(), (0, 4));

    undo(&mut editor);
    assert_eq!(text(&editor), "hello");
    assert_eq!(
        editor.cursor(),
        (0, 3),
        "where the caret was before the insert"
    );
}

#[test]
fn undo_puts_back_the_newline_enter_opened() {
    let mut editor = holding("one");
    editor.key(key(KeyCode::Enter));
    typed(&mut editor, "t");
    assert_eq!(editor.lines(), ["one", "t"]);

    undo(&mut editor);
    assert_eq!(editor.lines(), ["one", ""], "the character, not the row");
    undo(&mut editor);
    assert_eq!(
        editor.lines(),
        ["one"],
        "and now the row the newline opened"
    );
    assert_eq!(editor.cursor(), (0, 3));

    redo(&mut editor);
    redo(&mut editor);
    assert_eq!(editor.lines(), ["one", "t"]);
    assert_eq!(editor.cursor(), (1, 1));
}

#[test]
fn undo_puts_back_the_word_a_word_delete_took() {
    let mut editor = holding("one two three");
    editor.key(chord(KeyCode::Char('w'), KeyModifiers::CONTROL));
    assert_eq!(text(&editor), "one two ");

    undo(&mut editor);
    assert_eq!(text(&editor), "one two three");
    assert_eq!(editor.cursor(), (0, 13), "back where the delete started");
}

#[test]
fn undo_puts_back_what_a_cut_took_across_rows() {
    let mut editor = Editor::new();
    editor.insert_str("one\ntwo\nthree");
    editor.select_all();
    editor.cut();
    assert_eq!(editor.lines(), [""]);

    undo(&mut editor);
    assert_eq!(editor.lines(), ["one", "two", "three"]);
    assert_eq!(editor.cursor(), (2, 5), "where `select_all` left the caret");
    assert!(
        !editor.selecting(),
        "undo cancels the selection rather than restoring it, so the next \
         keystroke cannot delete a span over text that has just changed",
    );
}

#[test]
fn undo_puts_the_caret_back_where_the_edit_started() {
    // A buffer that comes back with the caret somewhere else is its own defect:
    // the text is right and the next keystroke still lands in the wrong place.
    let mut editor = Editor::new();
    editor.insert_str("alpha\nbeta\ngamma");
    editor.key(key(KeyCode::Up));
    editor.key(chord(KeyCode::Char('a'), KeyModifiers::CONTROL));
    assert_eq!(editor.cursor(), (1, 0));

    editor.key(chord(KeyCode::Char('k'), KeyModifiers::CONTROL));
    assert_eq!(editor.lines(), ["alpha", "", "gamma"]);

    undo(&mut editor);
    assert_eq!(editor.lines(), ["alpha", "beta", "gamma"]);
    assert_eq!(editor.cursor(), (1, 0));
}

#[test]
fn one_keystroke_is_one_press_however_many_methods_it_went_through() {
    // `Ctrl+K` at the end of a row reaches the join through `delete_next_char`,
    // then `delete_char`, then `delete_newline` — three recorded methods deep.
    // A step from each would cost three presses of `Ctrl+U` to take back one
    // keystroke, and the second and third would look like presses that did
    // nothing.
    let mut editor = Editor::new();
    editor.insert_str("abc\ndef");
    editor.key(key(KeyCode::Up));
    editor.key(chord(KeyCode::Char('e'), KeyModifiers::CONTROL));
    assert_eq!(editor.cursor(), (0, 3));

    editor.key(chord(KeyCode::Char('k'), KeyModifiers::CONTROL));
    assert_eq!(editor.lines(), ["abcdef"]);

    undo(&mut editor);
    assert_eq!(editor.lines(), ["abc", "def"], "one press put the row back");
    assert_eq!(editor.cursor(), (0, 3));
}

#[test]
fn an_edit_after_an_undo_throws_away_the_redo() {
    let mut editor = holding("abc");
    undo(&mut editor);
    assert_eq!(text(&editor), "ab");

    typed(&mut editor, "z");
    redo(&mut editor);
    assert_eq!(
        text(&editor),
        "abz",
        "redoing onto text the operator has since replaced would put back a \
         prompt they never asked for",
    );
}

#[test]
fn undo_and_redo_with_nothing_to_do_leave_the_buffer_and_the_caret_alone() {
    let mut editor = Editor::new();
    undo(&mut editor);
    redo(&mut editor);
    assert_eq!(text(&editor), "");
    assert_eq!(editor.cursor(), (0, 0));
    assert_eq!(editor.lines().len(), 1, "there is always one row");

    let mut editor = holding("abc");
    editor.key(key(KeyCode::Left));
    redo(&mut editor);
    assert_eq!(text(&editor), "abc");
    assert_eq!(
        editor.cursor(),
        (0, 2),
        "nothing to redo did not move the caret"
    );
}

#[test]
fn the_history_drops_its_oldest_step_rather_than_growing_without_end() {
    // A step is a copy of the whole buffer, so the depth is capped. Sixty steps
    // into a fifty-deep history: the ten oldest are gone, the fifty newest come
    // back, and the presses that run out of history are no-ops rather than a
    // panic on an empty stack.
    let mut editor = Editor::new();
    for _ in 0..60 {
        editor.insert_str("x");
    }
    assert_eq!(text(&editor), "x".repeat(60));

    for _ in 0..60 {
        undo(&mut editor);
    }
    assert_eq!(
        text(&editor),
        "x".repeat(10),
        "fifty steps came back and the ten before them were forgotten",
    );
}

#[test]
fn undo_in_a_masked_field_comes_back_masked() {
    // The wizard's field holds an API key. A mask that came off one press of
    // `Ctrl+U` after the key was typed would put the credential on the screen,
    // which is the same defect F9's sabotage arm exists to catch.
    let mut editor = Editor::masked('•');
    typed(&mut editor, "aa-bb-v1-secret");
    editor.key(chord(KeyCode::Char('w'), KeyModifiers::CONTROL));
    assert_eq!(editor.lines(), ["aa-bb-v1-"]);

    undo(&mut editor);
    assert_eq!(editor.lines(), ["aa-bb-v1-secret"], "the key came back");
    assert_eq!(editor.shown(), "•".repeat(15), "and came back masked");
    assert!(
        !editor.shown().contains("sk-"),
        "a fragment of the credential reached what is drawn: {:?}",
        editor.shown(),
    );
}
