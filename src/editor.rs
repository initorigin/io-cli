//! The editing model the prompt and the wizard's fields are built on.
//!
//! A buffer of lines, a cursor, a selection anchor and a keymap — and nothing
//! else. It is not a text editor: it is exactly what [`crate::composer`] and
//! [`crate::wizard`] ask for, which is why there is no viewport in it, no
//! syntax, no search and no widget. **Both callers draw their own rows**, the
//! composer because it wraps and the widget it used to hold scrolled sideways
//! instead, the wizard because its field is one row of masked characters.
//!
//! Positions are `(row, column)` in **characters**, never bytes. A pasted path
//! carries a narrow no-break space, a pasted prompt carries whatever the
//! operator copied, and a column that counted bytes would put the caret inside a
//! character and panic on the next edit.
//!
//! The keymap is the readline-shaped one this product shipped with, kept binding
//! for binding so that nothing an operator has in their fingers changed when the
//! buffer underneath did. One group of those bindings is deliberately gone — the
//! four that scrolled a viewport this type does not have — and it says so where
//! they used to be.

use std::cmp::Ordering;

use crossterm::event::{KeyCode, KeyEvent, KeyEventKind, KeyModifiers};

/// What a character counts as when a word-wise key asks where the word ends.
///
/// Three kinds rather than two, so `fn foo(a)` is the five words `fn`, `foo`,
/// `(`, `a`, `)` — a boundary falls wherever the kind changes, which is what
/// makes `Option+Backspace` in a path stop at the separator.
#[derive(PartialEq, Eq, Clone, Copy)]
enum Kind {
    Space,
    Punct,
    Other,
}

impl Kind {
    fn of(c: char) -> Self {
        if c.is_whitespace() {
            Self::Space
        } else if c.is_ascii_punctuation() {
            Self::Punct
        } else {
            Self::Other
        }
    }
}

/// The column the next word starts at, at or after `from`.
fn word_start_forward(line: &str, from: usize) -> Option<usize> {
    let mut characters = line.chars().enumerate().skip(from);
    let mut previous = Kind::of(characters.next()?.1);
    for (column, character) in characters {
        let kind = Kind::of(character);
        if kind != Kind::Space && previous != kind {
            return Some(column);
        }
        previous = kind;
    }
    None
}

/// The column one past the end of the word `from` is in.
fn word_end_forward(line: &str, from: usize) -> Option<usize> {
    let mut characters = line.chars().enumerate().skip(from);
    let mut previous = Kind::of(characters.next()?.1);
    for (column, character) in characters {
        let kind = Kind::of(character);
        if previous != Kind::Space && previous != kind {
            return Some(column);
        }
        previous = kind;
    }
    None
}

/// The column the word before `from` starts at.
fn word_start_backward(line: &str, from: usize) -> Option<usize> {
    let end = byte_of(line, from);
    let mut characters = line[..end].chars().rev().enumerate();
    let mut kind = Kind::of(characters.next()?.1);
    for (back, character) in characters {
        let next = Kind::of(character);
        if kind != Kind::Space && next != kind {
            return Some(from - back);
        }
        kind = next;
    }
    (kind != Kind::Space).then_some(0)
}

/// Where character `column` starts in `line`, or the line's length when the
/// column is past its end — which is where the cursor rests after the last
/// character, and is a position rather than a character.
fn byte_of(line: &str, column: usize) -> usize {
    line.char_indices()
        .nth(column)
        .map(|(at, _)| at)
        .unwrap_or(line.len())
}

fn width_of(line: &str) -> usize {
    line.chars().count()
}

/// `column`, or the end of `line` when the line is too short to have one there.
///
/// What `Up` and `Down` do at the edge of a ragged block of text: the column is
/// not remembered between moves, which is what the buffer this replaced did and
/// what every terminal editor with no "goal column" does.
fn fit(column: usize, line: &str) -> usize {
    column.min(width_of(line))
}

/// Where a movement key wants the cursor.
#[derive(Clone, Copy)]
enum Move {
    Forward,
    Back,
    Up,
    Down,
    Head,
    End,
    Top,
    Bottom,
    WordForward,
    WordBack,
    ParagraphForward,
    ParagraphBack,
}

/// How many steps `Ctrl+U` can walk back through.
///
/// The number `TextArea` shipped with (`tui-textarea-0.7.0/src/textarea.rs:219`),
/// and it means the same thing here that it meant there: one step per mutating
/// operation, one character at a time while typing, so fifty presses of `Ctrl+U`
/// reach back over fifty typed characters.
// ponytail: a step is a copy of the whole buffer, so the ceiling is fifty times
// the prompt rather than fifty small edits — a megabyte of pasted text edited
// fifty times is fifty megabytes held. That is the right trade for a prompt of a
// few lines and a one-row credential field, and it is the *only* thing that
// makes this small enough to be obviously correct. The upgrade path, if a prompt
// ever gets big enough for it to matter, is the operation-based history
// tui-textarea had: `tui-textarea-0.7.0/src/history.rs:5-90`, an enum of
// invertible edits, roughly ten times this code.
//
// ponytail: coalescing a run of typed characters into one step is the obvious
// other upgrade — it would make `Ctrl+U` take back a word rather than a letter,
// and it would cut what the fifty hold. It is declined on purpose. `TextArea`
// pushed one `Edit` per character with no merging at all
// (`tui-textarea-0.7.0/src/history.rs:141`), so per-character undo is the
// behaviour an operator already has, and this release is not the place to change
// it under them. When somebody actually asks: a `bool` on the editor, one
// condition on the `push_undo` in `edit`, and a line clearing it everywhere a run
// ends — `step`, `copy`, `select_all`, `restore`, and every edit that is not a
// typed character.
const HISTORY: usize = 50;

/// One state the buffer was in, and everything `Ctrl+U` puts back.
///
/// The cursor rides with the text because text alone is not the state an
/// operator left: a buffer that comes back with the caret somewhere else is its
/// own defect, and the next keystroke would then land in the wrong place.
struct Step {
    lines: Vec<String>,
    cursor: (usize, usize),
}

/// A buffer of lines with one cursor in it.
///
/// There is always at least one line, and the cursor is always a valid position
/// in it — every method below restores both before it returns, which is what
/// lets the callers index with `cursor()` without checking.
pub struct Editor {
    lines: Vec<String>,
    /// `(row, column)`, in characters. The column may be one past the last
    /// character of its row: that is where the next one goes.
    cursor: (usize, usize),
    /// Where a selection started, or `None` when nothing is selected. The other
    /// end is always the cursor.
    anchor: Option<(usize, usize)>,
    /// What the last cut or word-wise delete took, as the lines it spanned. One
    /// element is a piece of a line; more than one is a run that crossed a
    /// newline, and it goes back the same shape it came out.
    yank: Vec<String>,
    /// The character every character is drawn as, for a field that must not echo
    /// what is typed into it. Masking is a property of the *drawing* — the
    /// buffer still holds what was typed, and [`Editor::lines`] still hands it
    /// back, because the wizard has to read the credential it is not allowed to
    /// show.
    ///
    /// **It is not part of a [`Step`]**, which is why undo cannot lose it: a
    /// masked field that came back unmasked one press of `Ctrl+U` after an API
    /// key was typed into it would put the credential on the operator's screen.
    mask: Option<char>,
    /// States to go back to, oldest first, newest last. Capped at [`HISTORY`].
    undone: Vec<Step>,
    /// States [`Editor::undo`] stepped out of, newest last. Cleared by any new
    /// edit: once the text has moved somewhere else, there is no forward left to
    /// go to, and offering one would put back text the operator has since
    /// replaced.
    redone: Vec<Step>,
    /// Whether an edit is already being recorded.
    ///
    /// The mutating methods compose — `delete_char` reaches `delete_newline`,
    /// `delete_line_by_end` reaches `delete_next_char` reaches `delete_char` —
    /// and each of them is wrapped. Without this, one keystroke would leave
    /// three steps behind and cost three presses of `Ctrl+U` to take back.
    recording: bool,
}

impl Default for Editor {
    fn default() -> Self {
        Self::new()
    }
}

impl Editor {
    pub fn new() -> Self {
        Self {
            lines: vec![String::new()],
            cursor: (0, 0),
            anchor: None,
            yank: Vec::new(),
            mask: None,
            undone: Vec::new(),
            redone: Vec::new(),
            recording: false,
        }
    }

    /// An editor whose characters are drawn as `mask` rather than as themselves.
    pub fn masked(mask: char) -> Self {
        Self {
            mask: Some(mask),
            ..Self::new()
        }
    }

    /// The text, line by line, exactly as it was typed. Never empty.
    pub fn lines(&self) -> &[String] {
        &self.lines
    }

    /// The insertion point as `(row, column)`, in characters.
    pub fn cursor(&self) -> (usize, usize) {
        self.cursor
    }

    /// The cursor's row as it should be **drawn** — masked, when a mask is set.
    ///
    /// One mask character per character of text, so a caller placing a caret at
    /// `cursor().1` lands on the same cell either way.
    pub fn shown(&self) -> String {
        let line = self
            .lines
            .get(self.cursor.0)
            .map(String::as_str)
            .unwrap_or("");
        match self.mask {
            Some(mask) => std::iter::repeat_n(mask, width_of(line)).collect(),
            None => line.to_string(),
        }
    }

    /// Whether a selection is in progress.
    pub fn selecting(&self) -> bool {
        self.selection().is_some()
    }

    /// The selection as `(start, end)`, ordered, or `None` when nothing is
    /// selected or the two ends are the same position.
    fn selection(&self) -> Option<((usize, usize), (usize, usize))> {
        let anchor = self.anchor?;
        match anchor.cmp(&self.cursor) {
            Ordering::Less => Some((anchor, self.cursor)),
            Ordering::Equal => None,
            Ordering::Greater => Some((self.cursor, anchor)),
        }
    }

    /// Select everything, cursor at the end of the text.
    pub fn select_all(&mut self) {
        let row = self.lines.len() - 1;
        self.cursor = (row, width_of(&self.lines[row]));
        self.anchor = Some((0, 0));
    }

    /// Delete the selection and keep it, so it can be put back.
    pub fn cut(&mut self) -> bool {
        self.edit(|editor| editor.delete_selection(true))
    }

    // --- undo and redo ---

    /// Go back one step. `false` when there is nothing to go back to.
    ///
    /// `Ctrl+U` in this product's keymap. It is bound here rather than in
    /// [`crate::composer`] because that is where it has always been: io-cli
    /// forwards raw crossterm events into the buffer, and the buffer this one
    /// replaced bound the key itself at
    /// `tui-textarea-0.7.0/src/textarea.rs:576-581`.
    /// An operator has had this key working since the first release, and the
    /// composer intercepts neither `u` nor `r` on the way here.
    pub fn undo(&mut self) -> bool {
        match self.undone.pop() {
            Some(step) => {
                let left = self.restore(step);
                self.redone.push(left);
                true
            }
            // Nothing to undo is a no-op, not a panic and not a cursor that
            // jumps: an operator who presses `Ctrl+U` on an empty prompt keeps
            // the caret they had.
            None => false,
        }
    }

    /// Go forward one step, undoing an undo. `false` when there is no forward.
    ///
    /// `Ctrl+R` in this product's keymap, for the same reason [`Editor::undo`]
    /// is `Ctrl+U` — `tui-textarea-0.7.0/src/textarea.rs:582-587`.
    pub fn redo(&mut self) -> bool {
        match self.redone.pop() {
            Some(step) => {
                let left = self.restore(step);
                self.push_undo(left);
                true
            }
            None => false,
        }
    }

    /// Put `step` back, handing out the state it displaced so the caller can
    /// stack it on the other side.
    fn restore(&mut self, step: Step) -> Step {
        let left = Step {
            lines: std::mem::replace(&mut self.lines, step.lines),
            cursor: self.cursor,
        };
        self.cursor = step.cursor;
        // The selection is cancelled rather than restored, which is what
        // `TextArea::undo` did (`tui-textarea-0.7.0/src/textarea.rs:1558`). A
        // selection that came back over text that has just changed underneath it
        // would delete the wrong span on the next keystroke.
        self.anchor = None;
        left
    }

    fn push_undo(&mut self, step: Step) {
        if self.undone.len() == HISTORY {
            self.undone.remove(0);
        }
        self.undone.push(step);
    }

    /// Run one mutating operation, recording the state it started from.
    ///
    /// One step per operation, and a typed character is an operation — which is
    /// what `TextArea` did (`tui-textarea-0.7.0/src/history.rs:141`), so
    /// `Ctrl+U` steps back one character at a time exactly as it always has.
    ///
    /// The snapshot is taken **before** `action`, because that is the state
    /// `Ctrl+U` has to put back — after is too late, the text is already gone.
    /// It is kept only if the text actually changed: `Backspace` at the head of
    /// the buffer and `Delete` at its end do nothing, and a step for one of them
    /// is a press of `Ctrl+U` that appears to have no effect at all.
    ///
    /// A nested call belongs to the outer one's step. See [`Editor::recording`].
    fn edit<T>(&mut self, action: impl FnOnce(&mut Self) -> T) -> T {
        if self.recording {
            return action(self);
        }
        let before = Step {
            lines: self.lines.clone(),
            cursor: self.cursor,
        };
        self.recording = true;
        let out = action(self);
        self.recording = false;

        if self.lines == before.lines {
            return out;
        }
        self.push_undo(before);
        self.redone.clear();
        out
    }

    /// Keep the selection without deleting it.
    fn copy(&mut self) {
        if let Some((start, end)) = self.selection() {
            let taken = self.spanned(start, end);
            self.yank = taken;
        }
        self.anchor = None;
    }

    /// Put back what the last cut or word-wise delete took.
    fn put(&mut self) {
        self.edit(|editor| {
            editor.delete_selection(false);
            match editor.yank.len() {
                0 => {}
                1 => {
                    let piece = editor.yank[0].clone();
                    editor.insert_piece(&piece);
                }
                _ => {
                    let chunk = editor.yank.clone();
                    editor.insert_chunk(&chunk);
                }
            }
        });
    }

    /// Insert `text` at the cursor, newlines and all.
    ///
    /// Both `\n` and `\r\n` open a line; a bare `\r` does not, because a
    /// terminal that sends one means a carriage return rather than a paragraph.
    pub fn insert_str(&mut self, text: impl AsRef<str>) {
        self.edit(move |editor| {
            editor.delete_selection(false);
            let pieces: Vec<&str> = text
                .as_ref()
                .split('\n')
                .map(|piece| piece.strip_suffix('\r').unwrap_or(piece))
                .collect();
            match pieces.len() {
                0 => {}
                1 => editor.insert_piece(pieces[0]),
                _ => editor.insert_chunk(&pieces),
            }
        });
    }

    /// Open a line at the cursor, carrying the rest of the row down with it.
    pub fn insert_newline(&mut self) {
        self.edit(|editor| {
            editor.delete_selection(false);
            let (row, column) = editor.cursor;
            let at = byte_of(&editor.lines[row], column);
            let carried = editor.lines[row][at..].to_string();
            editor.lines[row].truncate(at);
            editor.lines.insert(row + 1, carried);
            editor.cursor = (row + 1, 0);
        });
    }

    /// Move the cursor to the end of the row it is on.
    pub fn move_to_end(&mut self) {
        self.step(Move::End, false);
    }

    /// Delete the character before the cursor, or the newline when the cursor is
    /// at the head of a row. A selection is deleted instead.
    pub fn delete_char(&mut self) -> bool {
        self.edit(|editor| {
            if editor.delete_selection(false) {
                return true;
            }
            let (row, column) = editor.cursor;
            if column == 0 {
                return editor.delete_newline();
            }
            let at = byte_of(&editor.lines[row], column - 1);
            let to = byte_of(&editor.lines[row], column);
            editor.lines[row].drain(at..to);
            editor.cursor.1 -= 1;
            true
        })
    }

    // --- everything below is reached through `key` ---

    fn insert_char(&mut self, character: char) {
        if character == '\n' || character == '\r' {
            self.insert_newline();
            return;
        }
        self.edit(move |editor| {
            editor.delete_selection(false);
            let (row, column) = editor.cursor;
            let at = byte_of(&editor.lines[row], column);
            editor.lines[row].insert(at, character);
            editor.cursor.1 += 1;
        });
    }

    fn insert_piece(&mut self, piece: &str) {
        if piece.is_empty() {
            return;
        }
        let (row, column) = self.cursor;
        let at = byte_of(&self.lines[row], column);
        self.lines[row].insert_str(at, piece);
        self.cursor.1 += width_of(piece);
    }

    fn insert_chunk(&mut self, chunk: &[impl AsRef<str>]) {
        debug_assert!(chunk.len() > 1, "a chunk is more than one line");
        let (row, column) = self.cursor;
        let at = byte_of(&self.lines[row], column);
        let carried = self.lines[row][at..].to_string();
        self.lines[row].truncate(at);
        self.lines[row].push_str(chunk[0].as_ref());

        let last = chunk.len() - 1;
        let mut opened: Vec<String> = chunk[1..last]
            .iter()
            .map(|line| line.as_ref().to_string())
            .collect();
        opened.push(format!("{}{carried}", chunk[last].as_ref()));
        self.lines.splice(row + 1..row + 1, opened);
        self.cursor = (row + last, width_of(chunk[last].as_ref()));
    }

    /// A tab is spaces to the next stop, never a tab character: the composer
    /// measures its wrapped rows in characters, and one character that draws as
    /// four columns is a row whose width nothing downstream agrees on.
    fn insert_tab(&mut self) {
        self.edit(|editor| {
            editor.delete_selection(false);
            const STOP: usize = 4;
            let (row, column) = editor.cursor;
            // ponytail: the stop is counted in characters, not display columns,
            // so a line of double-width characters indents to the wrong stop.
            // Count `unicode-width` here if the prompt ever grows a real indent.
            let taken = column.min(width_of(&editor.lines[row]));
            editor.insert_piece(&" ".repeat(STOP - taken % STOP));
        });
    }

    /// Join this row onto the one above it.
    fn delete_newline(&mut self) -> bool {
        self.edit(|editor| {
            if editor.delete_selection(false) {
                return true;
            }
            let (row, _) = editor.cursor;
            if row == 0 {
                return false;
            }
            let joined = editor.lines.remove(row);
            editor.cursor = (row - 1, width_of(&editor.lines[row - 1]));
            editor.lines[row - 1].push_str(&joined);
            true
        })
    }

    fn delete_next_char(&mut self) -> bool {
        self.edit(|editor| {
            if editor.delete_selection(false) {
                return true;
            }
            let before = editor.cursor;
            editor.step(Move::Forward, false);
            if before == editor.cursor {
                return false;
            }
            editor.delete_char()
        })
    }

    fn delete_line_by_end(&mut self) {
        self.edit(|editor| {
            // A selection is deleted rather than yanked, which is what the
            // buffer this replaced did: the keep-and-put-back is for the word or
            // the run of line the key names, never for whatever happened to be
            // selected.
            if editor.delete_selection(false) {
                return;
            }
            let (row, column) = editor.cursor;
            let end = width_of(&editor.lines[row]);
            if column < end {
                editor.delete_range((row, column), (row, end), true);
                return;
            }
            editor.delete_next_char();
        });
    }

    fn delete_line_by_head(&mut self) {
        self.edit(|editor| {
            // A selection is deleted rather than yanked, which is what the
            // buffer this replaced did: the keep-and-put-back is for the word or
            // the run of line the key names, never for whatever happened to be
            // selected.
            if editor.delete_selection(false) {
                return;
            }
            let (row, column) = editor.cursor;
            if column > 0 {
                editor.delete_range((row, 0), (row, column), true);
                return;
            }
            editor.delete_newline();
        });
    }

    fn delete_word(&mut self) {
        self.edit(|editor| {
            // A selection is deleted rather than yanked, which is what the
            // buffer this replaced did: the keep-and-put-back is for the word or
            // the run of line the key names, never for whatever happened to be
            // selected.
            if editor.delete_selection(false) {
                return;
            }
            let (row, column) = editor.cursor;
            match word_start_backward(&editor.lines[row], column) {
                Some(start) if start < column => {
                    editor.delete_range((row, start), (row, column), true)
                }
                Some(_) => {}
                None if column > 0 => editor.delete_range((row, 0), (row, column), true),
                None => {
                    editor.delete_newline();
                }
            }
        });
    }

    fn delete_next_word(&mut self) {
        self.edit(|editor| {
            // A selection is deleted rather than yanked, which is what the
            // buffer this replaced did: the keep-and-put-back is for the word or
            // the run of line the key names, never for whatever happened to be
            // selected.
            if editor.delete_selection(false) {
                return;
            }
            let (row, column) = editor.cursor;
            let end = width_of(&editor.lines[row]);
            match word_end_forward(&editor.lines[row], column) {
                Some(stop) if stop > column => {
                    editor.delete_range((row, column), (row, stop), true)
                }
                Some(_) => {}
                None if column < end => editor.delete_range((row, column), (row, end), true),
                None if row + 1 < editor.lines.len() => {
                    editor.cursor = (row + 1, 0);
                    editor.delete_newline();
                }
                None => {}
            }
        });
    }

    /// The text between two positions, as the lines it spans.
    fn spanned(&self, start: (usize, usize), end: (usize, usize)) -> Vec<String> {
        let ((from_row, from_column), (to_row, to_column)) = (start, end);
        let from = byte_of(&self.lines[from_row], from_column);
        let to = byte_of(&self.lines[to_row], to_column);
        if from_row == to_row {
            return vec![self.lines[from_row][from..to].to_string()];
        }
        let mut taken = vec![self.lines[from_row][from..].to_string()];
        taken.extend(self.lines[from_row + 1..to_row].iter().cloned());
        taken.push(self.lines[to_row][..to].to_string());
        taken
    }

    /// Remove everything between two positions, cursor left at the start.
    fn delete_range(&mut self, start: (usize, usize), end: (usize, usize), should_yank: bool) {
        if should_yank {
            let taken = self.spanned(start, end);
            self.yank = taken;
        }
        let ((from_row, from_column), (to_row, to_column)) = (start, end);
        let from = byte_of(&self.lines[from_row], from_column);
        let to = byte_of(&self.lines[to_row], to_column);
        self.cursor = start;
        self.anchor = None;
        if from_row == to_row {
            self.lines[from_row].drain(from..to);
            return;
        }
        let carried = self.lines[to_row][to..].to_string();
        self.lines[from_row].truncate(from);
        self.lines[from_row].push_str(&carried);
        self.lines.drain(from_row + 1..=to_row);
    }

    /// Delete the selection, if there is one. Either way the selection ends.
    fn delete_selection(&mut self, should_yank: bool) -> bool {
        let range = self.selection();
        self.anchor = None;
        match range {
            Some((start, end)) => {
                self.delete_range(start, end, should_yank);
                true
            }
            None => false,
        }
    }

    /// Where `movement` wants the cursor, or `None` when there is nowhere to go.
    ///
    /// `None` is not the same as "stayed put": a movement that cannot happen
    /// leaves a selection alone, which is what keeps `Shift+Left` at the head of
    /// the text from silently dropping what is already selected.
    fn next_cursor(&self, movement: Move) -> Option<(usize, usize)> {
        let (row, column) = self.cursor;
        match movement {
            Move::Forward if column >= width_of(&self.lines[row]) => {
                (row + 1 < self.lines.len()).then_some((row + 1, 0))
            }
            Move::Forward => Some((row, column + 1)),
            Move::Back if column == 0 => {
                let row = row.checked_sub(1)?;
                Some((row, width_of(&self.lines[row])))
            }
            Move::Back => Some((row, column - 1)),
            Move::Up => {
                let row = row.checked_sub(1)?;
                Some((row, fit(column, &self.lines[row])))
            }
            Move::Down => Some((row + 1, fit(column, self.lines.get(row + 1)?))),
            Move::Head => Some((row, 0)),
            Move::End => Some((row, width_of(&self.lines[row]))),
            Move::Top => Some((0, fit(column, &self.lines[0]))),
            Move::Bottom => {
                let row = self.lines.len() - 1;
                Some((row, fit(column, &self.lines[row])))
            }
            Move::WordForward => match word_start_forward(&self.lines[row], column) {
                Some(start) => Some((row, start)),
                None if row + 1 < self.lines.len() => Some((row + 1, 0)),
                None => Some((row, width_of(&self.lines[row]))),
            },
            Move::WordBack => match word_start_backward(&self.lines[row], column) {
                Some(start) => Some((row, start)),
                None if row > 0 => Some((row - 1, width_of(&self.lines[row - 1]))),
                None => Some((row, 0)),
            },
            Move::ParagraphForward => {
                let mut blank = self.lines[row].is_empty();
                for next in row + 1..self.lines.len() {
                    let now = self.lines[next].is_empty();
                    if !now && blank {
                        return Some((next, fit(column, &self.lines[next])));
                    }
                    blank = now;
                }
                let last = self.lines.len() - 1;
                Some((last, fit(column, &self.lines[last])))
            }
            Move::ParagraphBack => {
                let from = row.checked_sub(1)?;
                let mut blank = self.lines[from].is_empty();
                for next in (0..from).rev() {
                    let now = self.lines[next].is_empty();
                    if now && !blank {
                        return Some((next + 1, fit(column, &self.lines[next + 1])));
                    }
                    blank = now;
                }
                Some((0, fit(column, &self.lines[0])))
            }
        }
    }

    /// Take a movement, extending the selection when `shift` is held and ending
    /// it when it is not.
    fn step(&mut self, movement: Move, shift: bool) {
        let Some(next) = self.next_cursor(movement) else {
            return;
        };
        if shift {
            if self.anchor.is_none() {
                self.anchor = Some(self.cursor);
            }
        } else {
            self.anchor = None;
        }
        self.cursor = next;
    }

    /// Feed a key.
    ///
    /// **The keymap is the whole of what an operator can press here**, so it is
    /// written out rather than derived: every arm below was a binding this
    /// product already had, and the order between them is load-bearing —
    /// `Ctrl+Left` is a word-wise move only because the arm that reads `Left`
    /// bare has already declined it.
    pub fn key(&mut self, key: KeyEvent) {
        // A terminal with the Kitty keyboard protocol on — and Windows, always —
        // reports the release of every key it reported the press of. Acting on
        // both is every character typed twice.
        if key.kind == KeyEventKind::Release {
            return;
        }
        let ctrl = key.modifiers.contains(KeyModifiers::CONTROL);
        let alt = key.modifiers.contains(KeyModifiers::ALT);
        let shift = key.modifiers.contains(KeyModifiers::SHIFT);
        match key.code {
            KeyCode::Enter => self.insert_newline(),
            KeyCode::Char('m') if ctrl && !alt => self.insert_newline(),
            // Shift is not consulted: a capital letter arrives as the capital
            // with the modifier still set, and reading it here would refuse to
            // type one.
            KeyCode::Char(character) if !ctrl && !alt => self.insert_char(character),
            KeyCode::Tab if !ctrl && !alt => self.insert_tab(),
            KeyCode::Backspace if !ctrl && !alt => {
                self.delete_char();
            }
            KeyCode::Char('h') if ctrl && !alt => {
                self.delete_char();
            }
            KeyCode::Delete if !ctrl && !alt => {
                self.delete_next_char();
            }
            KeyCode::Char('d') if ctrl && !alt => {
                self.delete_next_char();
            }
            KeyCode::Char('k') if ctrl && !alt => self.delete_line_by_end(),
            KeyCode::Char('j') if ctrl && !alt => self.delete_line_by_head(),
            KeyCode::Char('w') if ctrl && !alt => self.delete_word(),
            KeyCode::Char('h') if !ctrl && alt => self.delete_word(),
            KeyCode::Backspace if !ctrl && alt => self.delete_word(),
            KeyCode::Delete if !ctrl && alt => self.delete_next_word(),
            KeyCode::Char('d') if !ctrl && alt => self.delete_next_word(),
            KeyCode::Down if !ctrl && !alt => self.step(Move::Down, shift),
            KeyCode::Char('n') if ctrl && !alt => self.step(Move::Down, shift),
            KeyCode::Up if !ctrl && !alt => self.step(Move::Up, shift),
            KeyCode::Char('p') if ctrl && !alt => self.step(Move::Up, shift),
            KeyCode::Right if !ctrl && !alt => self.step(Move::Forward, shift),
            KeyCode::Char('f') if ctrl && !alt => self.step(Move::Forward, shift),
            KeyCode::Left if !ctrl && !alt => self.step(Move::Back, shift),
            KeyCode::Char('b') if ctrl && !alt => self.step(Move::Back, shift),
            // `Home` and `End` mean what they say whatever is held with them.
            KeyCode::Char('a') if ctrl && !alt => self.step(Move::Head, shift),
            KeyCode::Home => self.step(Move::Head, shift),
            KeyCode::Left | KeyCode::Char('b') if ctrl && alt => self.step(Move::Head, shift),
            KeyCode::Char('e') if ctrl && !alt => self.step(Move::End, shift),
            KeyCode::End => self.step(Move::End, shift),
            KeyCode::Right | KeyCode::Char('f') if ctrl && alt => self.step(Move::End, shift),
            KeyCode::Char('<') if !ctrl && alt => self.step(Move::Top, shift),
            KeyCode::Up | KeyCode::Char('p') if ctrl && alt => self.step(Move::Top, shift),
            KeyCode::Char('>') if !ctrl && alt => self.step(Move::Bottom, shift),
            KeyCode::Down | KeyCode::Char('n') if ctrl && alt => self.step(Move::Bottom, shift),
            KeyCode::Char('f') if !ctrl && alt => self.step(Move::WordForward, shift),
            KeyCode::Right if ctrl && !alt => self.step(Move::WordForward, shift),
            KeyCode::Char('b') if !ctrl && alt => self.step(Move::WordBack, shift),
            KeyCode::Left if ctrl && !alt => self.step(Move::WordBack, shift),
            KeyCode::Char(']' | 'n') if !ctrl && alt => self.step(Move::ParagraphForward, shift),
            KeyCode::Down if ctrl && !alt => self.step(Move::ParagraphForward, shift),
            KeyCode::Char('[' | 'p') if !ctrl && alt => self.step(Move::ParagraphBack, shift),
            KeyCode::Up if ctrl && !alt => self.step(Move::ParagraphBack, shift),
            KeyCode::Char('y') if ctrl && !alt => self.put(),
            KeyCode::Char('x') if ctrl && !alt => {
                self.cut();
            }
            KeyCode::Char('c') if ctrl && !alt => self.copy(),
            // `Ctrl+U` and `Ctrl+R` are not this crate's invention and are not
            // optional: the buffer this replaced bound them itself
            // (`tui-textarea-0.7.0/src/textarea.rs:576-587`), io-cli forwards raw
            // crossterm events straight into it, and neither `crate::composer`
            // nor `crate::wizard` takes `u` or `r` on the way. An operator has
            // had undo in the prompt and in the credential field since the first
            // release; dropping the keys here would take it away silently.
            KeyCode::Char('u') if ctrl && !alt => {
                self.undo();
            }
            KeyCode::Char('r') if ctrl && !alt => {
                self.redo();
            }
            // ponytail: `Ctrl+V`, `Alt+V`, `PageUp` and `PageDown` scrolled a
            // viewport this type does not have. Both callers draw their own rows
            // and scroll them themselves, so these were already inert.
            _ => {}
        }
    }
}
