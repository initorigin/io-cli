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

use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use ratatui::layout::{Position, Rect};
use ratatui::text::{Line, Span};
use ratatui::widgets::Paragraph;
use ratatui::Frame;

use crate::glyphs::Glyphs;
use crate::theme::{Theme, Tone};

/// What an unselected row is drawn with: exactly the width of
/// [`crate::glyphs::Glyphs::marker`], in either set, so the labels line up in a
/// column whatever the terminal can draw.
///
/// The marker itself comes off the chosen set. Under the ASCII set it is `> `,
/// which is also [`crate::composer::PROMPT`] — the same collision the Unicode
/// marker's note has always described, and safe for the same reason: this widget
/// is drawn *instead of* the composer, so the two never appear at once.
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
        // `Ctrl+C` leaves, exactly as `Esc` does. The picker owns the keyboard
        // while it is open, and the shipped keybinding table promises `Ctrl+C`
        // interrupts the turn and exits from an empty prompt — a picker that
        // swallowed it would make the documentation describe a trap, with the
        // only way out a key the table never names.
        //
        // Backing out rather than a second, picker-only meaning: the press
        // closes the overlay and the one after it reaches the app, where the
        // table's meaning is the one that applies. This is the approval
        // overlay's answer (`App::key`, which exempts `Ctrl+C` from the open
        // question) reached from the other side — the approval interrupts and
        // the question is denied as a consequence; here there is nothing to
        // interrupt, so backing out *is* the whole consequence.
        if key.modifiers.contains(KeyModifiers::CONTROL) && key.code == KeyCode::Char('c') {
            return Outcome::Cancelled;
        }
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

        // Everything this widget draws is fitted to the area, including the two
        // things that used to be handed to ratatui raw: the title and the label.
        // A `Paragraph` in the viewport does not wrap and does not complain — it
        // clips the row and draws nothing to say so — so an unfitted title was a
        // question with its second half missing, and an unfitted label was a
        // choice whose identifying tail was gone. `/resume` and `/fork` avoided
        // both only because `sessions::rows` happened to shorten them first,
        // which is a property of one caller and not of the widget.
        let width = area.width as usize;
        let mut lines = vec![Line::from(Span::styled(
            fit(&self.title, width, &theme.glyphs),
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
            let marker = if chosen {
                theme.glyphs.marker
            } else {
                UNMARKED
            };
            // Fitted to what the marker leaves, never to the whole row. The
            // marker is the only thing on screen that says which choice Enter
            // would take, and it is drawn first — so a label fitted to the full
            // width pushes exactly its own marker's worth of characters off the
            // right-hand edge, on the selected row, where it matters most. The
            // reservation is `marker` rather than a literal two because that is
            // the string actually about to be drawn; the two sets agree on its
            // width today and this does not depend on them continuing to.
            let marker_width = marker.chars().count();
            let label = fit(
                &row.label,
                width.saturating_sub(marker_width),
                &theme.glyphs,
            );
            let label_width = label.chars().count();
            let mut spans = vec![
                Span::styled(marker, theme.style(Tone::Accent)),
                Span::styled(
                    label,
                    theme.style(if chosen { Tone::Accent } else { Tone::Normal }),
                ),
            ];
            if let Some(detail) = &row.detail {
                // Fitted rather than wrapped. A row that wraps makes the list
                // stop being a list, and the label is the part that has to
                // survive — which is why the detail is what gets cut.
                //
                // Measured off the **fitted** label, which is the string actually
                // about to be drawn. A budget that counts anything other than what
                // reaches the buffer is a budget that disagrees with the row by
                // however much they differ — and this budget being one cell out is
                // precisely how an ellipsis ends up on the floor.
                let used = marker_width + label_width + 2;
                if let Some(room) = width.checked_sub(used) {
                    if room > 1 {
                        spans.push(Span::styled("  ", theme.style(Tone::Muted)));
                        spans.push(Span::styled(
                            fit(detail, room, &theme.glyphs),
                            theme.style(Tone::Muted),
                        ));
                    }
                }
            }
            lines.push(Line::from(spans));
        }

        frame.render_widget(Paragraph::new(lines), area);

        // The real terminal cursor goes on the selected row, and it is put there
        // here rather than by any of the callers. ratatui hides the cursor on any
        // frame that does not set a position, and this widget is drawn *instead
        // of* the composer — `paint_picker` renders the open picker in place of
        // the app — so an open picker used to be a frame with no caret anywhere
        // on it. A hidden cursor removes the only focus indicator a screen reader
        // has, at the one moment the operator is being asked to choose. Owning it
        // in the widget that owns the selection is what makes it unforgettable,
        // exactly as `Composer::render` owns its insertion point.
        //
        // At the **start of the label** rather than on the marker: the marker is
        // decoration and the label is what identifies the choice, so a reader
        // following the caret lands on the word that says what pressing Enter
        // would do.
        //
        // The row is measured from `offset`, which `scroll_to_selection` has
        // already moved, so a selection in a scrolled list is placed where it was
        // actually drawn and not where an unscrolled list would have put it.
        let row = (self.selected.saturating_sub(self.offset) + 1)
            .min(area.height.saturating_sub(1) as usize) as u16;
        frame.set_cursor_position(Position {
            x: (area.x + theme.glyphs.marker.chars().count() as u16)
                .min(area.right().saturating_sub(1)),
            y: area.y + row,
        });
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
///
/// Public because the approval overlay fits a path and a line of file content
/// with it. One fitting rule in the product, so a shortened row and a shortened
/// target are shortened the same way and by the same mark.
///
/// **The mark's own width is measured, never assumed, and this is the whole of
/// why.** Until 0.6.0 the ellipsis was one character and this function reserved
/// exactly one for it. The ASCII set spells it `...`, which is three — so a
/// fitter that kept the old reservation would have returned a string two cells
/// wider than the room it was given, on every shortened row of every surface at
/// once. ratatui clips a viewport row silently, so the symptom would have been
/// load-bearing text disappearing off the right-hand edge with nothing on screen
/// to say so, which is the same failure this product has now shipped three times.
/// The result of this function is `room` characters or fewer, always.
///
/// At a room too small for the mark itself the mark is what gets cut, and the
/// text does not appear at all. A fragment of a word in two cells is not
/// information; two dots at least say that something was there.
pub fn fit(text: &str, room: usize, glyphs: &Glyphs) -> String {
    if text.chars().count() <= room {
        return text.to_string();
    }
    let mark = glyphs.ellipsis;
    let width = mark.chars().count();
    if room <= width {
        return mark.chars().take(room).collect();
    }
    let mut out: String = text.chars().take(room - width).collect();
    out.push_str(mark);
    out
}

/// [`fit`], keeping the **end** of the text instead of the beginning.
///
/// For a filesystem path, and only for a path. Every workspace on one machine
/// shares its first several segments, so shortening a path from the right keeps
/// the part that is the same on every row and drops the part that identifies it.
/// `/Users/someone/code/io-cli` matters; `/Users/someone/co…` does not.
///
/// Bounded by `room` on the same terms as [`fit`], and for the same reason.
pub fn fit_left(text: &str, room: usize, glyphs: &Glyphs) -> String {
    let count = text.chars().count();
    if count <= room {
        return text.to_string();
    }
    let mark = glyphs.ellipsis;
    let width = mark.chars().count();
    if room <= width {
        return mark.chars().take(room).collect();
    }
    let keep = room - width;
    let mut out = String::from(mark);
    out.extend(text.chars().skip(count - keep));
    out
}
