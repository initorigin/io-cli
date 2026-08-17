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

/// What a keystroke did.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Reply {
    /// Handled; nothing for the session to do.
    Idle,
    /// The user pressed `Enter` on a finished prompt.
    Submitted(String),
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
        }
    }

    /// What is currently typed.
    pub fn text(&self) -> String {
        self.area.lines().join("\n")
    }

    /// Whether there is nothing typed. `Ctrl+D` exits on an empty composer, so
    /// this decides whether a keystroke ends the session.
    pub fn is_empty(&self) -> bool {
        self.area.lines().iter().all(|line| line.trim().is_empty())
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
                if text.trim().is_empty() {
                    return Reply::Idle;
                }
                self.clear();
                self.remember(&text);
                Reply::Submitted(text)
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
    pub fn paste(&mut self, text: &str) {
        self.editing();
        self.area.insert_str(text);
    }

    /// Empty the composer, keeping the history.
    pub fn clear(&mut self) {
        self.area.select_all();
        self.area.cut();
        self.recalled = None;
        self.draft.clear();
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
            ratatui::widgets::Paragraph::new(Line::styled(PROMPT, theme.style(Tone::Accent))),
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
                self.draft = self.text();
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

    fn replace(&mut self, text: &str) {
        let recalled = self.recalled;
        let draft = std::mem::take(&mut self.draft);
        self.clear();
        self.recalled = recalled;
        self.draft = draft;
        self.area.insert_str(text);
        self.area.move_cursor(CursorMove::End);
    }
}
