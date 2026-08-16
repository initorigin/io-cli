//! The `Picker`: one overlay, every choice.
//!
//! The wizard's provider, model, theme and posture steps are this widget with
//! different rows, and so are `/model` and `/theme`. Later releases add `/resume`,
//! `/fork` and the approval overlay, which are also this widget.
//!
//! Building it once is the reason the product has one look. The alternative —
//! adopting a Rust prompt library for the wizard — would have put a second owner
//! of raw mode and a second aesthetic in the one flow where first impressions are
//! formed, and would have had to be replaced anyway the first time an approval
//! needed to render inside the viewport.
//!
//! It lives in the viewport, never in scrollback. A choice that has scrolled away
//! cannot be answered.

use crossterm::event::{KeyCode, KeyEvent};
use ratatui::layout::Rect;
use ratatui::text::{Line, Span};
use ratatui::widgets::Paragraph;
use ratatui::Frame;

use crate::theme::{Theme, Tone};

/// The marker in front of the selected row. Two cells, and a word — `>` is also
/// what the composer uses, so the two never appear at once.
const MARKER: &str = "› ";
const UNMARKED: &str = "  ";

/// One choice.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Row {
    /// What the row is. Shown first, because content precedes metadata.
    pub label: String,
    /// A dimmer explanation, dropped first when the terminal is narrow.
    pub detail: Option<String>,
}

impl Row {
    pub fn new(label: impl Into<String>) -> Self {
        Self {
            label: label.into(),
            detail: None,
        }
    }

    pub fn with_detail(label: impl Into<String>, detail: impl Into<String>) -> Self {
        Self {
            label: label.into(),
            detail: Some(detail.into()),
        }
    }
}

/// What a keystroke did to the picker.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Outcome {
    /// Handled, or ignored; the picker is still open.
    Idle,
    /// The user chose the row at this index.
    Chosen(usize),
    /// The user backed out.
    Cancelled,
}

pub struct Picker {
    title: String,
    rows: Vec<Row>,
    selected: usize,
    /// The first row currently drawn, so a long list scrolls instead of being cut.
    offset: usize,
}

impl Picker {
    pub fn new(title: impl Into<String>, rows: Vec<Row>) -> Self {
        Self {
            title: title.into(),
            rows,
            selected: 0,
            offset: 0,
        }
    }

    /// Open with a row already selected — what `/theme` does, so the picker opens
    /// on the theme in use rather than on the first one in the list.
    pub fn selecting(mut self, index: usize) -> Self {
        self.selected = index.min(self.rows.len().saturating_sub(1));
        self
    }

    pub fn rows(&self) -> &[Row] {
        &self.rows
    }

    /// Which row is highlighted. Read every frame by the theme step, which
    /// re-renders its sample transcript behind the picker as this moves.
    pub fn selected(&self) -> usize {
        self.selected
    }

    /// Rows the picker wants, given the rows it has and the space available.
    pub fn height(&self, available: u16) -> u16 {
        let wanted = self.rows.len() as u16 + 1; // the title
        wanted.min(available.max(1))
    }

    pub fn key(&mut self, key: KeyEvent) -> Outcome {
        match key.code {
            // Clamped at both ends rather than wrapping, for the same reason the
            // composer's history is: a list that jumps from the last row to the
            // first on one keypress loses the reader's place.
            KeyCode::Up | KeyCode::Char('k') => {
                self.selected = self.selected.saturating_sub(1);
                Outcome::Idle
            }
            KeyCode::Down | KeyCode::Char('j') => {
                if self.selected + 1 < self.rows.len() {
                    self.selected += 1;
                }
                Outcome::Idle
            }
            KeyCode::Home => {
                self.selected = 0;
                Outcome::Idle
            }
            KeyCode::End => {
                self.selected = self.rows.len().saturating_sub(1);
                Outcome::Idle
            }
            KeyCode::Enter => {
                if self.rows.is_empty() {
                    Outcome::Idle
                } else {
                    Outcome::Chosen(self.selected)
                }
            }
            KeyCode::Esc => Outcome::Cancelled,
            _ => Outcome::Idle,
        }
    }

    /// Draw into `area`, scrolling so the selected row is always visible.
    pub fn render(&mut self, frame: &mut Frame, area: Rect, theme: &Theme) {
        if area.height == 0 || area.width == 0 {
            return;
        }

        let mut lines = vec![Line::from(Span::styled(
            self.title.clone(),
            theme.style(Tone::Muted),
        ))];

        let visible = area.height.saturating_sub(1) as usize;
        self.scroll_to_selection(visible);

        for (index, row) in self
            .rows
            .iter()
            .enumerate()
            .skip(self.offset)
            .take(visible.max(1))
        {
            let chosen = index == self.selected;
            let marker = if chosen { MARKER } else { UNMARKED };
            let mut spans = vec![
                Span::styled(marker, theme.style(Tone::Accent)),
                Span::styled(
                    row.label.clone(),
                    theme.style(if chosen { Tone::Accent } else { Tone::Normal }),
                ),
            ];
            if let Some(detail) = &row.detail {
                // Fitted rather than wrapped. A row that wraps makes the list
                // stop being a list, and the label is the part that has to
                // survive — which is why the detail is what gets cut.
                let used = marker.chars().count() + row.label.chars().count() + 2;
                if let Some(room) = (area.width as usize).checked_sub(used) {
                    if room > 1 {
                        spans.push(Span::styled("  ", theme.style(Tone::Muted)));
                        spans.push(Span::styled(fit(detail, room), theme.style(Tone::Muted)));
                    }
                }
            }
            lines.push(Line::from(spans));
        }

        frame.render_widget(Paragraph::new(lines), area);
    }

    fn scroll_to_selection(&mut self, visible: usize) {
        let visible = visible.max(1);
        if self.selected < self.offset {
            self.offset = self.selected;
        } else if self.selected >= self.offset + visible {
            self.offset = self.selected + 1 - visible;
        }
        let last_offset = self.rows.len().saturating_sub(visible);
        self.offset = self.offset.min(last_offset);
    }
}

/// Shorten `text` to at most `room` cells, marking that it was shortened.
fn fit(text: &str, room: usize) -> String {
    if text.chars().count() <= room {
        return text.to_string();
    }
    // Counted in characters, and the ellipsis is one character rather than three
    // dots, so a shortened row and a full one are the same width.
    let keep = room.saturating_sub(1);
    let mut out: String = text.chars().take(keep).collect();
    out.push('…');
    out
}
