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
//!
//! **It filters as it is typed**, through [`crate::fuzzy`], and it is the widget
//! the filter landed in first because the slash palette is a filtering picker: the
//! other order would have meant writing the filter twice. A printable character
//! narrows the rows, backspace widens them, and the query is drawn where the title
//! was rather than on a line of its own — see [`Picker::render`] for why that
//! choice is load-bearing rather than cosmetic. What leaves this widget is
//! unaffected: [`Outcome::Chosen`] and [`Picker::selected`] are indices into the
//! caller's own row list at every point, filtered or not.

use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use ratatui::layout::{Position, Rect};
use ratatui::text::{Line, Span};
use ratatui::widgets::Paragraph;
use ratatui::Frame;

use crate::fuzzy;
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
    /// What has been typed. Empty until the first printable character, which is
    /// the state every picker in the product opens in.
    query: String,
    /// Which rows the query admits, best first, **as indices into `rows`**.
    ///
    /// The whole of the outward-facing contract rests on this being indices into
    /// the caller's own list rather than a filtered copy of it. Five of the nine
    /// call sites read the chosen index back positionally and three of them index
    /// a slice raw — `Kind::ALL[index]`, `Posture::ALL[index]`, `open.rows()[index]`
    /// — so a filtered index is a panic; `/resume` and `/fork` do `ids.get(index)`
    /// and would resume a different session without saying so.
    matches: Vec<usize>,
    /// Where the marker is **within `matches`**, which is the only place it can
    /// live: the row under the marker has to stay under the marker as the list
    /// reorders, and a position in `rows` says nothing about where a row was drawn.
    cursor: usize,
    /// The first row currently drawn, so a long list scrolls instead of being cut.
    offset: usize,
}

impl Picker {
    pub fn new(title: impl Into<String>, rows: Vec<Row>) -> Self {
        Self {
            title: title.into(),
            matches: (0..rows.len()).collect(),
            rows,
            query: String::new(),
            cursor: 0,
            offset: 0,
        }
    }

    /// Open with a row already selected — what `/theme` does, so the picker opens
    /// on the theme in use rather than on the first one in the list.
    ///
    /// The argument is a row index, like every other index this widget takes and
    /// returns. Nothing has been typed yet when a caller uses this, so the two
    /// numbering schemes agree; going through `matches` anyway is what keeps that
    /// an observation rather than an assumption.
    pub fn selecting(mut self, index: usize) -> Self {
        self.cursor = self
            .matches
            .iter()
            .position(|row| *row == index)
            // Out of range is clamped rather than panicking: the caller is passing
            // an index derived from configuration, which can name a row that no
            // longer exists.
            .unwrap_or_else(|| self.matches.len().saturating_sub(1));
        self
    }

    /// Every row the caller handed in, in the order they handed them in.
    ///
    /// Unfiltered on purpose. A caller reading a label back out of this is holding
    /// an index this widget gave it, and that index addresses this list.
    pub fn rows(&self) -> &[Row] {
        &self.rows
    }

    /// Which row is highlighted, as an index into [`Picker::rows`]. Read every
    /// frame by the theme step, which re-renders its sample transcript behind the
    /// picker as this moves — and whose value is also what gets written to
    /// `io.toml`, so a filtered index here would preview one theme and persist
    /// another.
    ///
    /// Zero when nothing matches, which is the same answer an empty picker has
    /// always given and is safe for the same reason: nothing can be chosen from a
    /// picker with no rows under the marker, so no caller ever indexes with it.
    pub fn selected(&self) -> usize {
        self.matches.get(self.cursor).copied().unwrap_or(0)
    }

    /// What has been typed so far, which is what the top line shows.
    pub fn query(&self) -> &str {
        &self.query
    }

    /// How many rows the query currently admits.
    pub fn matching(&self) -> usize {
        self.matches.len()
    }

    /// Rows the picker wants, given the rows it has and the space available.
    ///
    /// At least one row's worth even when nothing matches, because the empty
    /// result has a line of its own to say so and a picker sized to zero rows
    /// would draw a query with nothing under it.
    pub fn height(&self, available: u16) -> u16 {
        let wanted = self.matches.len().max(1) as u16 + 1; // the title, or the query
        wanted.min(available.max(1))
    }

    /// Re-rank against the query, keeping the marker on the row it was on.
    ///
    /// The row rather than the position: on a backspace the list widens and the
    /// row the operator was looking at moves down it, and a marker that stayed at
    /// position 0 would have silently changed what Enter takes. When the selected
    /// row no longer matches there is nothing to keep, and the best match is where
    /// the marker belongs.
    ///
    /// **The label is matched and the detail is not.** The detail is the first
    /// thing [`Picker::render`] drops when the terminal is narrow, so a row kept by
    /// a hit inside it would be a row matching text that is not on screen — a
    /// filter whose result the operator cannot account for is worse than a filter
    /// that misses.
    fn refilter(&mut self) {
        let was = self.matches.get(self.cursor).copied();
        self.matches = fuzzy::rank(self.rows.iter().map(|row| row.label.as_str()), &self.query);
        self.cursor = was
            .and_then(|row| self.matches.iter().position(|index| *index == row))
            .unwrap_or(0);
        self.offset = 0;
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
            //
            // The arrows only. `j` and `k` moved the marker until 0.7.0 and are
            // ordinary query characters now, which is a behaviour change made on
            // purpose: they are printable, and a picker that filters cannot hold
            // back two letters of the alphabet without the operator discovering it
            // by typing a model name and watching the wrong thing happen. The
            // shipped keybinding table names the arrows and has never named these
            // two, so the documented way to move is the way that still works.
            KeyCode::Up => {
                self.cursor = self.cursor.saturating_sub(1);
                Outcome::Idle
            }
            KeyCode::Down => {
                if self.cursor + 1 < self.matches.len() {
                    self.cursor += 1;
                }
                Outcome::Idle
            }
            KeyCode::Home => {
                self.cursor = 0;
                Outcome::Idle
            }
            KeyCode::End => {
                self.cursor = self.matches.len().saturating_sub(1);
                Outcome::Idle
            }
            KeyCode::Enter => {
                if self.matches.is_empty() {
                    Outcome::Idle
                } else {
                    Outcome::Chosen(self.selected())
                }
            }
            // `Esc` cancels the picker, and never merely clears the query. An
            // operator who has learned one escape should not find that the same
            // key means "leave" on some screens and "undo what I typed" on others;
            // `tests/wizard.rs` asserts `Esc` leaves at every depth, and a query
            // is cleared by holding backspace, which is where it came from.
            KeyCode::Esc => Outcome::Cancelled,
            KeyCode::Backspace => {
                if self.query.pop().is_some() {
                    self.refilter();
                }
                Outcome::Idle
            }
            // Every printable character narrows. Modified characters are left
            // alone — `Ctrl+C` has already returned above, and a `Ctrl` or `Alt`
            // chord is a command somebody meant, not a letter they typed.
            KeyCode::Char(character)
                if !key.modifiers.contains(KeyModifiers::CONTROL)
                    && !key.modifiers.contains(KeyModifiers::ALT) =>
            {
                self.query.push(character);
                self.refilter();
                Outcome::Idle
            }
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
        // **The query is drawn in place of the title, never above it.** The
        // in-session viewport is four rows and is fixed at attach, so a query line
        // of its own would leave `/resume` two visible rows — and the note on
        // `term::WIZARD_VIEWPORT_HEIGHT` already records that three was the count
        // a live first run found unusable, which is why that constant exists. The
        // title is what the picker is *for*, which the operator has just read; the
        // query is what they are doing, which changes under their hands. Swapping
        // one for the other also leaves every piece of arithmetic downstream of
        // this alone: the row slots are still `height - 1` and the cursor is still
        // one past the top line.
        let (head, tone) = if self.query.is_empty() {
            (self.title.as_str(), Tone::Muted)
        } else {
            (self.query.as_str(), Tone::Accent)
        };
        let mut lines = vec![Line::from(Span::styled(
            fit(head, width, &theme.glyphs),
            theme.style(tone),
        ))];

        let visible = area.height.saturating_sub(1) as usize;
        self.scroll_to_selection(visible);

        // A query that admits nothing says so. Without this the screen is a query
        // over blank rows, which looks exactly like a picker that has broken —
        // and the difference between "no row is spelled that way" and "this thing
        // has stopped working" is the whole of what the operator needs to know.
        if self.matches.is_empty() && !self.query.is_empty() {
            let nothing = format!(
                "No row matches {}{}{}",
                theme.glyphs.quote_open, self.query, theme.glyphs.quote_close
            );
            lines.push(Line::from(Span::styled(
                fit(&nothing, width, &theme.glyphs),
                theme.style(Tone::Muted),
            )));
        }

        for (position, row) in self
            .matches
            .iter()
            .map(|index| &self.rows[*index])
            .enumerate()
            .skip(self.offset)
            .take(visible.max(1))
        {
            let chosen = position == self.cursor;
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
        // Measured in drawn positions, which is what `cursor` already is. When
        // nothing matches this lands on the line saying so, which is the only line
        // there is and is still the thing a reader following the caret should be
        // told.
        let row = (self.cursor.saturating_sub(self.offset) + 1)
            .min(area.height.saturating_sub(1) as usize) as u16;
        frame.set_cursor_position(Position {
            x: (area.x + theme.glyphs.marker.chars().count() as u16)
                .min(area.right().saturating_sub(1)),
            y: area.y + row,
        });
    }

    /// Counted over the rows the query admits, not over every row the caller
    /// handed in: the list that scrolls is the list that is drawn.
    fn scroll_to_selection(&mut self, visible: usize) {
        let visible = visible.max(1);
        if self.cursor < self.offset {
            self.offset = self.cursor;
        } else if self.cursor >= self.offset + visible {
            self.offset = self.cursor + 1 - visible;
        }
        let last_offset = self.matches.len().saturating_sub(visible);
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
