//! The composer: the one part of the viewport the user types into.
//!
//! `tui-textarea` does the editing. What is added here is the small set of
//! decisions a prompt has that a text editor does not: when `Enter` submits and
//! when it inserts a newline, what the arrow keys mean at the edges of the text,
//! and how a paste of a whole file behaves.

use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use ratatui::layout::{Position, Rect};
use ratatui::text::Line;
use ratatui::Frame;
use tui_textarea::{CursorMove, TextArea};

use crate::theme::{Theme, Tone};

/// The prompt marker. Two cells, so wrapped continuation lines line up under the
/// text rather than under the marker.
pub const PROMPT: &str = "> ";

/// How many characters a paste may carry before it is collapsed to one line.
///
/// Two rows of an eighty-column terminal, less the two the prompt marker takes:
/// `2 × 78`. The number is the composer's own size rather than a round one —
/// `crate::app` gives the composer exactly two of the four viewport rows, so a
/// paste of a hundred and fifty-six characters is the largest one the operator
/// can still see all of. The first character past that is the first one that
/// falls outside a box two rows tall, and from there the input stops being
/// navigable in the only sense that matters to somebody about to send it: what
/// is on screen is no longer what is in the prompt.
///
/// It is a character count and not a line count on purpose. A pasted block of
/// three short lines is three rows and still perfectly readable, and collapsing
/// it would take away the one thing a bracketed paste is for.
pub const PASTE_THRESHOLD: usize = 156;

/// What a keystroke did.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Reply {
    /// Handled; nothing for the session to do.
    Idle,
    /// The user pressed `Enter` on a finished prompt.
    Submitted(String),
}

/// The path a paste names, if it names one that exists.
///
/// Dragging a file into a terminal pastes its path, and a file manager's copy
/// does the same. What arrives may be shell-escaped — `My\ Documents` — or
/// already quoted, and it is one line either way. Quoting it is what makes a
/// path with a space in it survive as one word; resolving it to an absolute path
/// is what makes it survive the agent's working directory being somewhere else.
///
/// **It has to exist.** The whole safety of this is that prose is never quoted
/// at somebody: a sentence is not a path, and a path this process cannot see is
/// not one either, so both are pasted exactly as they arrived.
fn pasted_path(text: &str) -> Option<String> {
    let trimmed = text.trim();
    if trimmed.is_empty() || trimmed.contains('\n') {
        return None;
    }
    let unescaped = trimmed.trim_matches(['"', '\'']).replace("\\ ", " ");
    let candidate = std::path::Path::new(&unescaped);
    if !candidate.exists() {
        return None;
    }
    Some(
        candidate
            .canonicalize()
            .unwrap_or_else(|_| candidate.to_path_buf())
            .display()
            .to_string(),
    )
}

pub struct Composer {
    area: TextArea<'static>,
    /// Prompts already submitted, oldest first.
    history: Vec<String>,
    /// Where the arrow keys currently are in `history`, or `None` when the user
    /// is editing their own text rather than walking back through old prompts.
    recalled: Option<usize>,
    /// What was in the composer when the walk back started, so `Down` past the
    /// newest entry returns it rather than clearing it.
    draft: String,
    /// Every block that was pasted in and shown as one line, as
    /// `(placeholder, what it stands for)`, oldest first.
    ///
    /// The composer owns them, and [`Composer::text`] — not the submit path —
    /// puts them back. That is the whole design decision: a placeholder that is
    /// expanded only on `Enter` is one caller away from sending a description of
    /// a file where the file should have been, and nothing on screen would say
    /// so. Here the collapsed form exists only inside this type.
    pastes: Vec<(String, String)>,
}

impl Default for Composer {
    fn default() -> Self {
        Self::new()
    }
}

impl Composer {
    pub fn new() -> Self {
        let mut area = TextArea::default();
        // The composer draws its own prompt marker, so the widget must not also
        // draw a border, a line number or a cursor-line highlight.
        area.set_cursor_line_style(ratatui::style::Style::default());
        Self {
            area,
            history: Vec::new(),
            recalled: None,
            draft: String::new(),
            pastes: Vec::new(),
        }
    }

    /// What is currently typed, with every collapsed paste put back whole.
    ///
    /// This is the prompt, not the picture of it: a paste over
    /// [`PASTE_THRESHOLD`] is one line on screen and its full text here, byte
    /// for byte. Callers get the text they would have got had the paste been
    /// inserted as it always was.
    pub fn text(&self) -> String {
        self.expand(&self.typed())
    }

    /// What is on screen, placeholders and all.
    ///
    /// **What is on screen, and never what will be sent.** Everything that acts
    /// on the prompt asks [`Composer::text`], which cannot forget a paste; this
    /// is for the surfaces and the tests that need to know what the operator is
    /// actually looking at — a placeholder standing for a block, rather than the
    /// block.
    pub fn typed(&self) -> String {
        self.area.lines().join("\n")
    }

    /// `typed` with each placeholder replaced by the block it stands for.
    ///
    /// One left-to-right pass, never re-scanning what it has already written, so
    /// a pasted block that happens to contain the literal text of a placeholder
    /// is copied out rather than expanded a second time. A chain of
    /// `str::replace` calls would have expanded it, and pasting a transcript of a
    /// session that already collapsed a paste is all it would take.
    ///
    /// A placeholder is matched by its exact text. Edit a character of one and it
    /// stops being a placeholder and is sent as the words it reads as: the
    /// composer has no way to tell a person rewriting the line from a person
    /// typing that sentence, and quietly attaching a file to a line somebody
    /// edited is the worse of the two mistakes.
    fn expand(&self, typed: &str) -> String {
        if self.pastes.is_empty() {
            return typed.to_string();
        }
        let mut out = String::with_capacity(typed.len());
        let mut rest = typed;
        'text: while !rest.is_empty() {
            for (placeholder, text) in &self.pastes {
                if let Some(tail) = rest.strip_prefix(placeholder.as_str()) {
                    out.push_str(text);
                    rest = tail;
                    continue 'text;
                }
            }
            let mut characters = rest.chars();
            out.push(characters.next().expect("the remainder is not empty"));
            rest = characters.as_str();
        }
        out
    }

    /// Whether there is nothing here worth sending or worth keeping. `Ctrl+D`
    /// exits, `Esc` arms the rewind and `/` opens the palette on an empty
    /// composer, so this decides what three keystrokes mean.
    ///
    /// Asked of [`Composer::text`] rather than of the widget's lines, because
    /// those two disagree the moment a paste is not what it looks like. Paste a
    /// blank region of a file — a run of indentation, a column of empty lines —
    /// and it is over [`PASTE_THRESHOLD`], so it collapses to a placeholder that
    /// is emphatically not whitespace, while the prompt it stands for is nothing
    /// at all. Read the screen and the composer looks full; `Enter` reads the
    /// expanded text and will not send it. The operator is then holding a prompt
    /// that cannot be submitted, cannot be exited and cannot be cleared, and
    /// nothing on the frame says why.
    ///
    /// So there is one question and `Enter` is where it is answered: what the
    /// callers gate on is exactly what would be sent. The cost is expanding the
    /// pastes for a keystroke that only wanted to know whether the prompt is
    /// blank, which is bounded by what a terminal hands over in one paste event.
    /// A cheaper second predicate that could drift from the first is precisely
    /// what this was.
    pub fn is_empty(&self) -> bool {
        self.text().trim().is_empty()
    }

    /// The prompts submitted so far, oldest first.
    pub fn history(&self) -> &[String] {
        &self.history
    }

    /// Rows the composer needs at this width, prompt marker included.
    pub fn height(&self, width: u16) -> u16 {
        let usable = width.saturating_sub(PROMPT.len() as u16).max(1) as usize;
        self.area
            .lines()
            .iter()
            .map(|line| line.chars().count().div_ceil(usable).max(1) as u16)
            .sum::<u16>()
            .max(1)
    }

    /// Feed a key.
    pub fn key(&mut self, key: KeyEvent) -> Reply {
        match (key.code, key.modifiers) {
            // `Shift+Enter` is the newline every terminal that can report it uses.
            // Reachable because `term::negotiate_keyboard` asks for
            // `DISAMBIGUATE_ESCAPE_CODES` on the terminals that advertise it:
            // without that flag no terminal reports a modifier on `Enter` at all,
            // and this arm is a binding nobody can reach.
            (KeyCode::Enter, m) if m.contains(KeyModifiers::SHIFT) => {
                self.editing();
                self.area.insert_newline();
                Reply::Idle
            }
            (KeyCode::Enter, _) => {
                // The fallback for terminals that cannot distinguish `Shift+Enter`
                // from `Enter` at all — which is most of them without the Kitty
                // keyboard protocol. A trailing backslash means "continue".
                //
                // It stays even though the arm above now works: the protocol is
                // negotiated only where the terminal advertises it, and the
                // terminal on the far end of an ssh session, a tmux without
                // `extended-keys`, or a plain xterm advertises nothing. Deleting a
                // fallback because its replacement works on the developer's
                // terminal is how somebody else loses the newline entirely.
                if self.current_line().ends_with('\\') {
                    self.editing();
                    self.area.delete_char();
                    self.area.insert_newline();
                    return Reply::Idle;
                }
                let text = self.text();
                // The same test [`Composer::is_empty`] makes, on the same
                // expanded text — it is written out here only because the text
                // is already in hand. Change one and change the other, or the
                // composer goes back to refusing to send a prompt it also
                // refuses to call empty.
                if text.trim().is_empty() {
                    return Reply::Idle;
                }
                self.clear();
                self.remember(&text);
                Reply::Submitted(text)
            }
            // **A placeholder deletes as one thing, because it is one thing.**
            // Thirty-five presses to remove `[pasted text #4, 366 characters]`
            // is bad enough; the first of them is worse, because a placeholder
            // is matched by its exact text and an edited one silently stops
            // standing for the block it named.
            (KeyCode::Backspace, m) if !m.contains(KeyModifiers::ALT) => {
                self.editing();
                match self.placeholder_before_cursor() {
                    Some(placeholder) => {
                        for _ in 0..placeholder.chars().count() {
                            self.area.delete_char();
                        }
                        // The block goes with it. A prompt that still carried it
                        // would send text nothing on screen stands for.
                        self.pastes.retain(|(held, _)| held != &placeholder);
                    }
                    None => {
                        self.area.delete_char();
                    }
                }
                Reply::Idle
            }
            // History, but only from the edge of the text: inside a multiline
            // prompt the arrows have to move the cursor, or a long prompt cannot
            // be edited at all.
            (KeyCode::Up, _) if self.at_first_line() => {
                self.recall_older();
                Reply::Idle
            }
            (KeyCode::Down, _) if self.at_last_line() => {
                self.recall_newer();
                Reply::Idle
            }
            _ => {
                self.editing();
                self.area.input(key);
                Reply::Idle
            }
        }
    }

    /// Take a bracketed paste.
    ///
    /// Inserted verbatim, newlines included, rather than replayed as keystrokes —
    /// which is what an unbracketed paste of a multi-line block does, and it
    /// submits the prompt on the first newline.
    ///
    /// A paste over [`PASTE_THRESHOLD`] leaves one line saying what it is and how
    /// large, and the block itself is kept beside the text and put back by
    /// [`Composer::text`]. Anything at or under the threshold is inserted exactly
    /// as it always was.
    pub fn paste(&mut self, text: &str) {
        self.editing();

        // **A path pasted is a path, and it is quoted.** Dragging a file into a
        // terminal, or copying one out of a file manager, pastes its path — and a
        // path with a space in it is two words to everything downstream unless
        // something quotes it. The check is that it names a file that exists, so
        // ordinary prose is never quoted at somebody.
        if let Some(path) = pasted_path(text) {
            self.area.insert_str(format!("{path:?}"));
            return;
        }

        // **The same block pasted twice is a request to see it.** The first
        // paste collapses to a placeholder because a screenful of someone
        // else's text is not a prompt you can read; pressing paste again on the
        // same block is the operator saying they want it after all, so the
        // placeholder standing for it is replaced by what it stands for.
        if let Some((placeholder, held)) = self
            .pastes
            .iter()
            .find(|(_, held)| held == text)
            .cloned()
            .filter(|(placeholder, _)| self.typed().contains(placeholder.as_str()))
        {
            let expanded = self.typed().replace(&placeholder, &held);
            self.replace(&expanded);
            return;
        }

        // `chars`, never `len`: a byte count is not the size the operator can
        // check against what they copied, and it is off by a factor of three for
        // a paste of prose in most of the world's scripts.
        let size = text.chars().count();
        if size <= PASTE_THRESHOLD {
            self.area.insert_str(text);
            return;
        }
        // Content before metadata: what this line is, then how large it is. No
        // mark from `theme.glyphs` appears in it, deliberately — the line is
        // composed here, where the composer has no theme in hand, and a glyph set
        // stashed on this type would be a second one derived downstream of the
        // one chosen at startup, which is the thing `crate::glyphs` says nothing
        // may do. Every character below is in both sets, so the line reads the
        // same under either.
        //
        // The ordinal is what keeps two placeholders apart: two pastes of the
        // same size would otherwise spell the same line, and the second would be
        // sent as the first. It counts every paste this prompt has taken, not the
        // ones still in the text, so deleting one cannot free its number for
        // reuse.
        let placeholder = format!(
            "[pasted text #{}, {size} characters]",
            self.pastes.len() + 1
        );
        self.area.insert_str(&placeholder);
        self.pastes.push((placeholder, text.to_string()));
    }

    /// Put `text` in the prompt, replacing whatever is there, cursor at the end.
    ///
    /// The slash palette's `Enter`, and its only caller. The command is *typed*
    /// rather than run: the submit path stays the one path — `Enter`, then
    /// `strip_prefix('/')`, then [`crate::commands::parse`] — so the palette adds
    /// no second way for a command to be dispatched, and the operator sees and
    /// can edit what they are about to send.
    ///
    /// It goes through the same replacement a walk back through history uses, so
    /// there is one answer to "what happens to the pasted blocks" and not two.
    /// The palette only opens on an empty prompt, so in practice there is nothing
    /// to carry across.
    pub fn set(&mut self, text: &str) {
        self.replace(text);
    }

    /// Empty the composer, keeping the history.
    ///
    /// The pasted blocks go with the text. They belong to the prompt being
    /// written — once it is submitted or abandoned, a placeholder that survived
    /// would be standing in for something nobody can see any more.
    pub fn clear(&mut self) {
        self.area.select_all();
        self.area.cut();
        self.recalled = None;
        self.draft.clear();
        self.pastes.clear();
    }

    /// Render into `area`, prompt marker included.
    pub fn render(&self, frame: &mut Frame, area: Rect, theme: &Theme) {
        if area.height == 0 {
            return;
        }
        if area.width <= PROMPT.len() as u16 {
            // Too narrow to draw the prompt and any text beside it — but a frame
            // that accepts input still has to say where the focus is, and a
            // terminal this narrow is the last place to take that away. The
            // caret goes to the origin, which is where the first character
            // would land if there were room for one. Returning here without it
            // was the composer's own version of the defect F2 exists for, and
            // it was the surface that already knew better.
            frame.set_cursor_position(Position {
                x: area.x,
                y: area.y,
            });
            return;
        }
        let marker = Rect {
            width: PROMPT.len() as u16,
            ..area
        };
        let text = Rect {
            x: area.x + PROMPT.len() as u16,
            width: area.width - PROMPT.len() as u16,
            ..area
        };
        frame.render_widget(
            // Bold, because this row is where the operator's attention starts
            // and the mark is the only thing on screen that says "type here".
            ratatui::widgets::Paragraph::new(Line::styled(
                PROMPT,
                theme
                    .style(Tone::Accent)
                    .add_modifier(ratatui::style::Modifier::BOLD),
            )),
            marker,
        );
        frame.render_widget(&self.area, text);

        // The real terminal cursor is put where the insertion point is, and that
        // is done here rather than left to a caller. ratatui hides the cursor on
        // any frame that does not set a position, and a hidden cursor removes the
        // only focus indicator a screen reader has — the criticism the whole
        // category is unusable for. Owning it in the widget that owns the
        // insertion point means no frame can forget.
        let (x, y) = self.cursor(text);
        frame.set_cursor_position(Position { x, y });
    }

    /// The cursor's position inside `area`, which is the text region rather than
    /// the whole composer.
    pub fn cursor(&self, text: Rect) -> (u16, u16) {
        let (row, column) = self.area.cursor();
        let width = text.width.max(1);
        // Wrapped rows count: a cursor past the end of a visual row is on the next
        // one, and reporting it off the right edge puts the terminal's cursor
        // somewhere the text is not.
        let column = column as u16;
        (
            text.x + column % width,
            (text.y + row as u16 + column / width).min(text.bottom().saturating_sub(1)),
        )
    }

    /// How many rows this prompt wants, at `width`.
    ///
    /// What the driver grows the viewport to. Counted rather than guessed: a
    /// line wider than the composer wraps, and a prompt of three wrapped lines
    /// needs the rows those wraps take or the operator is typing into a window
    /// they cannot see the top of.
    pub fn rows_wanted(&self, width: u16) -> u16 {
        let room = usize::from(width.saturating_sub(PROMPT.len() as u16)).max(1);
        let rows: usize = self
            .area
            .lines()
            .iter()
            .map(|line| line.chars().count().div_ceil(room).max(1))
            .sum();
        u16::try_from(rows).unwrap_or(u16::MAX).max(1)
    }

    fn current_line(&self) -> &str {
        let (row, _) = self.area.cursor();
        self.area.lines().get(row).map(String::as_str).unwrap_or("")
    }

    fn at_first_line(&self) -> bool {
        self.area.cursor().0 == 0
    }

    fn at_last_line(&self) -> bool {
        self.area.cursor().0 + 1 >= self.area.lines().len()
    }

    /// Mark that the user is editing their own text again, so `Down` no longer
    /// walks forward through history into somebody else's prompt.
    fn editing(&mut self) {
        self.recalled = None;
    }

    fn remember(&mut self, text: &str) {
        // A prompt repeated back to back is one entry. Walking past five copies of
        // the same line is the most annoying thing a history can do.
        if self.history.last().map(String::as_str) != Some(text) {
            self.history.push(text.to_string());
        }
    }

    fn recall_older(&mut self) {
        if self.history.is_empty() {
            return;
        }
        let next = match self.recalled {
            None => {
                // The collapsed form, not the expanded one. The draft is put
                // back into the composer verbatim when the walk returns, and
                // stashing the expanded text here would hand the operator the
                // whole file back in two rows — the flood the placeholder exists
                // to prevent, arriving by a different door.
                self.draft = self.typed();
                self.history.len() - 1
            }
            Some(0) => 0,
            Some(index) => index - 1,
        };
        self.recalled = Some(next);
        let text = self.history[next].clone();
        self.replace(&text);
    }

    fn recall_newer(&mut self) {
        let Some(index) = self.recalled else {
            return;
        };
        if index + 1 < self.history.len() {
            self.recalled = Some(index + 1);
            let text = self.history[index + 1].clone();
            self.replace(&text);
        } else {
            self.recalled = None;
            let draft = std::mem::take(&mut self.draft);
            self.replace(&draft);
        }
    }

    /// Swap what is in the composer for `text`, keeping everything the walk
    /// through history is holding.
    ///
    /// The pasted blocks are carried across the `clear` along with `recalled` and
    /// `draft`, and for the same reason: the draft this walk will come back to
    /// still has its placeholders in it, and a block dropped here would leave one
    /// of them standing for nothing. A recalled *history* entry needs none of
    /// them — history keeps prompts whole — so the stale entries simply never
    /// match, and `clear` drops them when the prompt is done.
    /// The placeholder immediately before the cursor, if the cursor is at the
    /// end of one.
    ///
    /// Backspacing through `[pasted text #4, 366 characters]` one character at a
    /// time is thirty-five presses to remove one thing the operator thinks of as
    /// one thing — and the first press already breaks it, because a placeholder
    /// is matched by its exact text and an edited one stops standing for
    /// anything.
    fn placeholder_before_cursor(&self) -> Option<String> {
        let (row, column) = self.area.cursor();
        let line = self.area.lines().get(row)?;
        let before: String = line.chars().take(column).collect();
        self.pastes
            .iter()
            .map(|(placeholder, _)| placeholder)
            .filter(|placeholder| before.ends_with(placeholder.as_str()))
            .max_by_key(|placeholder| placeholder.chars().count())
            .cloned()
    }

    fn replace(&mut self, text: &str) {
        let recalled = self.recalled;
        let draft = std::mem::take(&mut self.draft);
        let pastes = std::mem::take(&mut self.pastes);
        self.clear();
        self.recalled = recalled;
        self.draft = draft;
        self.pastes = pastes;
        self.area.insert_str(text);
        self.area.move_cursor(CursorMove::End);
    }
}
