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
    /// A short mark saying what KIND of row this is, drawn before the label.
    ///
    /// **Separate from the label because the matcher ranks the label**, and a
    /// mark folded into it would give every row of a kind the same first
    /// character — under which no query is ever a prefix of a row and both of
    /// `crate::fuzzy`'s top tiers become unreachable. That is the defect the
    /// palette's stripped slash exists to avoid, and it would have come straight
    /// back as the price of marking the rows.
    ///
    /// **And separate from the detail because the detail is dropped first on a
    /// narrow terminal**, which is exactly where a row is hardest to tell apart.
    /// Through 0.15.0 a template and a skill were marked only in the detail, so
    /// the distinction vanished at the width that needed it most.
    pub mark: Option<&'static str>,
    /// Whether this row is a heading rather than a choice.
    ///
    /// **A heading is drawn while the list is being browsed and disappears the
    /// moment a character is typed**, which is not decoration: a ranked list with
    /// headings interleaved puts a heading above a row that ranked there for
    /// reasons having nothing to do with it. So `Picker::refilter` admits
    /// headings only for an empty query, and nothing can be chosen while one is
    /// under the marker.
    pub heading: bool,
}

impl Row {
    pub fn new(label: impl Into<String>) -> Self {
        Self {
            label: label.into(),
            detail: None,
            mark: None,
            heading: false,
        }
    }

    pub fn with_detail(label: impl Into<String>, detail: impl Into<String>) -> Self {
        Self {
            label: label.into(),
            detail: Some(detail.into()),
            mark: None,
            heading: false,
        }
    }

    /// A row that says what kind it is.
    ///
    /// `mark` is drawn before the label and is not matched against.
    pub fn marked(mark: &'static str, label: impl Into<String>, detail: impl Into<String>) -> Self {
        Self {
            label: label.into(),
            detail: Some(detail.into()),
            mark: Some(mark),
            heading: false,
        }
    }

    /// A group heading: shown while browsing, gone the moment anything is typed.
    pub fn heading(label: impl Into<String>) -> Self {
        Self {
            label: label.into(),
            detail: None,
            mark: None,
            heading: true,
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
    /// The row the marker is *meant* to be on, **as an index into `rows`**, and
    /// held here rather than derived from `cursor` because `cursor` cannot express
    /// it: a query that admits no row leaves nothing under the marker at all, and
    /// a query that hides one row leaves the marker on a different one.
    ///
    /// This is what a widening query restores. Without it the intent lived only in
    /// the current match set, so one keystroke that hid the opening row destroyed
    /// it and the backspace that followed had a *different* row to remember —
    /// `/fork` opened on the newest turn and branched from turn 0, and the wizard
    /// opened on the theme in use and wrote `dark`.
    ///
    /// `None` when there is no row to intend: an empty picker, and nothing else.
    intent: Option<usize>,
    /// The first row currently drawn, so a long list scrolls instead of being cut.
    offset: usize,
    /// A row that opens a block beneath itself while it holds the marker, and the
    /// rows that block wants.
    ///
    /// **The index is into `rows`, like [`Self::intent`] and never like
    /// [`Self::cursor`]**, so a query that reorders the list keeps the unfold on
    /// the row it belongs to rather than on whatever ranked into that position.
    ///
    /// Only one row can carry it, which is the whole mechanic: an accordion with
    /// two panels open is two things competing for the rows beneath one marker.
    /// 0.32.0 ships it with exactly one caller — the question overlay's free-text
    /// row, which unfolds a composer — because a mechanic built without one is a
    /// mode nobody can see, and this product shipped a whole pipeline reachable
    /// only from its own tests one release ago.
    unfold: Option<(usize, u16)>,
    /// Where the unfolded block was drawn by the last [`Self::render`], for the
    /// caller to draw into.
    ///
    /// Set by rendering rather than computed by the caller, because only this
    /// widget knows where the row landed after scrolling — and a caller doing that
    /// arithmetic itself is a second implementation of the scroll.
    opened: Option<Rect>,
}

impl Picker {
    pub fn new(title: impl Into<String>, rows: Vec<Row>) -> Self {
        let mut picker = Self {
            title: title.into(),
            matches: (0..rows.len()).collect(),
            intent: (!rows.is_empty()).then_some(0),
            rows,
            query: String::new(),
            cursor: 0,
            offset: 0,
            unfold: None,
            opened: None,
        };
        // A grouped list opens with a heading in the first slot, and the marker
        // may not rest on one. Stepping here rather than only in `refilter`
        // because nothing has been typed yet, so `refilter` has not run — and a
        // picker that opened with its marker on a heading would answer `Enter`
        // with nothing.
        picker.step_off_heading(1);
        picker.intent = picker.matches.get(picker.cursor).copied();
        picker
    }

    /// Open with a row already selected — what `/theme` does, so the picker opens
    /// on the theme in use rather than on the first one in the list.
    ///
    /// The argument is a row index, like every other index this widget takes and
    /// returns. Nothing has been typed yet when a caller uses this, so the two
    /// numbering schemes agree; going through `matches` anyway is what keeps that
    /// an observation rather than an assumption.
    pub fn selecting(mut self, index: usize) -> Self {
        self.aim(index);
        self
    }

    /// Replace every row, keeping what has been typed and aiming at `selecting`.
    ///
    /// For a caller whose rows arrive after the picker is already on the screen —
    /// the wizard's model step opens on the provider's default while the catalogue
    /// request is in flight. Building a fresh `Picker` instead would throw away
    /// whatever was typed during the wait, which on a four-hundred-model list is
    /// the only thing anybody does.
    pub fn set_rows(&mut self, rows: Vec<Row>, selecting: usize) {
        self.rows = rows;
        self.aim(selecting);
    }

    /// Intend the row at `index`, then put the marker where the query allows.
    fn aim(&mut self, index: usize) {
        // Out of range is clamped rather than panicking: the caller is passing an
        // index derived from configuration, or from a catalogue that has just
        // changed under it, either of which can name a row that no longer exists.
        self.intent = self.rows.len().checked_sub(1).map(|last| index.min(last));
        self.refilter();
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
    /// always given — and a row 0 that nothing is standing on. A caller that
    /// indexes with this on every keystroke, rather than on a choice, wants
    /// [`Picker::selection`] instead: the theme step did index with it, and a
    /// letter no theme name carries used to preview *and persist* `dark`.
    pub fn selected(&self) -> usize {
        self.selection().unwrap_or(0)
    }

    /// Which row is highlighted, or `None` when no row is: an empty picker, or a
    /// query that admits nothing.
    ///
    /// The honest half of [`Picker::selected`], kept as a second accessor rather
    /// than as a changed signature because the two questions are genuinely
    /// different. `Enter` is already guarded on there being a match, so every
    /// caller reading a *choice* has a row by construction and wants the plain
    /// index; a caller reading the marker every frame — the theme step's live
    /// preview — has to be able to see that there is nothing there.
    pub fn selection(&self) -> Option<usize> {
        self.matches.get(self.cursor).copied()
    }

    /// What has been typed so far, which is what the top line shows.
    pub fn query(&self) -> &str {
        &self.query
    }

    /// How many rows the query currently admits.
    pub fn matching(&self) -> usize {
        self.matches.len()
    }

    /// Re-rank against the query, keeping the marker on the row it was on.
    ///
    /// The row rather than the position: on a backspace the list widens and the
    /// row the operator was looking at moves down it, and a marker that stayed at
    /// position 0 would have silently changed what Enter takes.
    ///
    /// **The row is read from `intent`, never from the current match set**, which
    /// is the whole of the difference. A row read from `matches` is only there for
    /// as long as the query admits it — so the keystroke that hid it also forgot
    /// it, and the next keystroke remembered whatever had fallen under the marker
    /// in its place. Held separately, the intended row survives a query that hides
    /// it and comes back when the query widens again, and the marker in the
    /// meantime sits on the best match, which is where it belongs when the row it
    /// wants is not on the screen.
    ///
    /// **The label is matched and the detail is not.** The detail is the first
    /// thing [`Picker::render`] drops when the terminal is narrow, so a row kept by
    /// a hit inside it would be a row matching text that is not on screen — a
    /// filter whose result the operator cannot account for is worse than a filter
    /// that misses.
    fn refilter(&mut self) {
        self.matches = fuzzy::rank(self.rows.iter().map(|row| row.label.as_str()), &self.query);
        // **Headings survive only an empty query.** With anything typed the order
        // is the matcher's alone, and a heading left in it would sit above
        // whatever happened to rank there.
        if !self.query.is_empty() {
            self.matches.retain(|index| !self.rows[*index].heading);
        }
        self.cursor = self
            .intent
            .and_then(|row| self.matches.iter().position(|index| *index == row))
            .unwrap_or(0);
        self.offset = 0;
        // The marker opens on a choice rather than on a heading, which is the
        // first row of an unfiltered grouped list.
        self.step_off_heading(1);
    }

    /// Move the marker off a heading in `direction`, wrapping at the ends.
    ///
    /// A heading is not selectable, so every movement that could land on one
    /// continues past it. Bounded by the number of rows so a list that is nothing
    /// but headings terminates rather than spinning.
    fn step_off_heading(&mut self, direction: isize) {
        if self.matches.is_empty() {
            return;
        }
        for _ in 0..self.matches.len() {
            let Some(index) = self.matches.get(self.cursor) else {
                self.cursor = 0;
                continue;
            };
            if !self.rows[*index].heading {
                return;
            }
            let len = self.matches.len() as isize;
            self.cursor = (((self.cursor as isize + direction) % len + len) % len) as usize;
        }
    }

    /// Record what the marker was just moved onto, and answer `Idle`.
    ///
    /// A deliberate move is the operator saying which row they want, so it
    /// replaces the row the picker was opened on. It is only ever a move onto a
    /// matched row: with nothing under the marker there is nothing to intend, and
    /// the previous intent — which a widening query will restore — is what an
    /// arrow pressed at an empty list must not throw away.
    fn moved(&mut self) -> Outcome {
        self.intent = self.matches.get(self.cursor).copied().or(self.intent);
        Outcome::Idle
    }

    /// Let the row at `row` open a block of `height` rows beneath itself whenever
    /// it holds the marker.
    ///
    /// `row` indexes the caller's own `rows`, the same as [`Outcome::Chosen`].
    pub fn set_unfold(&mut self, row: usize, height: u16) {
        self.unfold = Some((row, height));
    }

    /// Put the marker on `row`, an index into the caller's own `rows`.
    ///
    /// Answers whether it could: a query that hides the row leaves the marker
    /// where it was rather than moving it somewhere arbitrary.
    pub fn focus(&mut self, row: usize) -> bool {
        match self.matches.iter().position(|index| *index == row) {
            Some(at) => {
                self.cursor = at;
                self.intent = Some(row);
                true
            }
            None => false,
        }
    }

    /// Whether the unfolding row is the one under the marker right now.
    ///
    /// The caller asks this to decide where a keystroke goes: with the block open
    /// it belongs to whatever is drawn in it, and with it shut the picker filters
    /// as it always has.
    pub fn unfolded_now(&self) -> bool {
        match (self.unfold, self.matches.get(self.cursor)) {
            (Some((row, height)), Some(under)) => height > 0 && row == *under,
            _ => false,
        }
    }

    /// Where the unfolded block was drawn by the last [`Self::render`].
    ///
    /// `None` when nothing is unfolded, or when the terminal had no room for it —
    /// growth is a request rather than a guarantee, and a caller that drew into a
    /// rectangle this widget never reserved would paint over the list.
    pub fn opened(&self) -> Option<Rect> {
        self.opened
    }

    /// Rows this picker would like: its head, every row it holds, and whatever an
    /// unfolded block is asking for.
    ///
    /// A request, not a demand — [`crate::app::App::viewport_wanted`] clamps it to
    /// what the terminal can spare, and [`Self::render`] elides against whatever it
    /// is actually given.
    pub fn rows_wanted(&self) -> u16 {
        let list = u16::try_from(self.matches.len()).unwrap_or(u16::MAX);
        let block = if self.unfolded_now() {
            self.unfold.map_or(0, |(_, height)| height)
        } else {
            0
        };
        list.saturating_add(block).saturating_add(1)
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
            // Every movement continues past a heading, which is not selectable.
            // Done here rather than by filtering headings out of `matches`,
            // because they have to be DRAWN in their places — the whole point of
            // them is where they sit.
            KeyCode::Up => {
                self.cursor = self.cursor.saturating_sub(1);
                self.step_off_heading(-1);
                self.moved()
            }
            KeyCode::Down => {
                if self.cursor + 1 < self.matches.len() {
                    self.cursor += 1;
                }
                self.step_off_heading(1);
                self.moved()
            }
            KeyCode::Home => {
                self.cursor = 0;
                self.step_off_heading(1);
                self.moved()
            }
            KeyCode::End => {
                self.cursor = self.matches.len().saturating_sub(1);
                self.step_off_heading(-1);
                self.moved()
            }
            // **`Shift+Tab` steps the marker back, and it is spelled twice on
            // purpose.** A terminal may send `BackTab`, or `Tab` with the shift
            // modifier set; `crate::keys` normalises the first into the second for
            // a *binding*, but a picker reads raw key events and never goes
            // through that table, so both spellings arrive here as themselves.
            // Matching one of them is a key that works on some terminals.
            KeyCode::BackTab => {
                self.cursor = self.cursor.saturating_sub(1);
                self.step_off_heading(-1);
                self.moved()
            }
            KeyCode::Tab if key.modifiers.contains(KeyModifiers::SHIFT) => {
                self.cursor = self.cursor.saturating_sub(1);
                self.step_off_heading(-1);
                self.moved()
            }
            // **`Tab` accepts what is under the marker**, in every picker in the
            // product rather than in the palette alone, because the product ships
            // one `Picker`. It is the completion key an operator arrives with, and
            // until 0.32.0 it fell into the catch-all below and did nothing at all
            // — a key that looks like it should work and silently does not is
            // worse than one that is not bound.
            KeyCode::Tab | KeyCode::Enter => {
                // A heading under the marker cannot happen — every path that
                // moves it steps off one — but `Enter` is the key that would turn
                // a mistake here into the wrong action, so it declines rather
                // than trusting that.
                let on_heading = self
                    .matches
                    .get(self.cursor)
                    .is_some_and(|index| self.rows[*index].heading);
                if self.matches.is_empty() || on_heading {
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
        // in-session viewport is `term::VIEWPORT_HEIGHT` and is fixed at attach —
        // four rows when this was written and eight since 0.13.0, of which the
        // status footer, the rule and the activity line take most — so a query line
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

        self.opened = None;
        // The rows the head and an open block take before any row is drawn. The
        // block is reserved rather than borrowed from the list, because a composer
        // squeezed into rows the list is also using is the overlap 0.32.0 exists to
        // end.
        let block = if self.unfolded_now() {
            self.unfold.map_or(0, |(_, height)| height)
        } else {
            0
        };
        let room = (area.height as usize).saturating_sub(1 + block as usize);
        // **A row is spent saying what is not shown, and only when something is
        // not.** Until 0.32.0 a picker over four hundred rows drew the twenty it
        // had room for and said nothing at all, so a list that had been filtered
        // to nothing looked the same as one scrolled to its end.
        let visible = if self.matches.len() > room {
            room.saturating_sub(1)
        } else {
            room
        };
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

        let taken = visible.max(1);
        let mut opened_at: Option<Rect> = None;
        for (position, row) in self
            .matches
            .iter()
            .map(|index| &self.rows[*index])
            .enumerate()
            .skip(self.offset)
            .take(taken)
        {
            // A heading is a label and nothing else: no marker, no mark, no
            // detail, and never under the cursor. It exists only while the list
            // is being browsed — `refilter` drops headings the moment anything is
            // typed — so it can be drawn plainly here without a query to worry
            // about.
            if row.heading {
                lines.push(Line::from(Span::styled(
                    fit(&row.label, width, &theme.glyphs),
                    theme.style(Tone::Muted),
                )));
                continue;
            }

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
            // The kind mark rides here, between the marker and the label, and
            // never inside either. Not in the label, because the matcher ranks
            // the label and a shared first character makes both of its top tiers
            // unreachable; not in the detail, because the detail is the first
            // thing dropped on a narrow terminal and the kind is what a reader
            // most needs there.
            let mark = row.mark.map(|mark| format!("{mark} ")).unwrap_or_default();
            let mark_width = mark.chars().count();
            let label = fit(
                &row.label,
                width.saturating_sub(marker_width + mark_width),
                &theme.glyphs,
            );
            let label_width = label.chars().count();
            let mut spans = vec![Span::styled(marker, theme.style(Tone::Accent))];
            if !mark.is_empty() {
                spans.push(Span::styled(mark, theme.style(Tone::Muted)));
            }
            spans.push(Span::styled(
                label,
                theme.style(if chosen { Tone::Accent } else { Tone::Normal }),
            ));
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
                let used = marker_width + mark_width + label_width + 2;
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

            // **The block opens directly beneath the row that owns it**, which is
            // the whole of the accordion: the operator reads top to bottom, and
            // what they are typing into sits under the row that says what it is
            // for. Reserved as blank rows here and drawn by the caller into the
            // rectangle recorded after the loop, because only this widget knows
            // where the row landed once the list has scrolled.
            if block > 0
                && self
                    .unfold
                    .is_some_and(|(row, _)| row == self.matches[position])
            {
                let top = area
                    .y
                    .saturating_add(u16::try_from(lines.len()).unwrap_or(u16::MAX));
                if top.saturating_add(block) <= area.y.saturating_add(area.height) {
                    opened_at = Some(Rect {
                        x: area.x,
                        y: top,
                        width: area.width,
                        height: block,
                    });
                    for _ in 0..block {
                        lines.push(Line::from(""));
                    }
                }
            }
        }
        self.opened = opened_at;

        // What the list could not show. Counted against everything the query
        // admits rather than against what is below the window, because a row
        // scrolled off the top is just as absent as one below the bottom.
        let drawn = taken.min(
            self.matches
                .len()
                .saturating_sub(self.offset.min(self.matches.len())),
        );
        let hidden = self.matches.len().saturating_sub(drawn);
        if hidden > 0 {
            let note = format!("{} {hidden} more", theme.glyphs.elision);
            lines.push(Line::from(Span::styled(
                fit(&note, width, &theme.glyphs),
                theme.style(Tone::Muted),
            )));
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
        // Past the marker on a row that has one. The line saying nothing matched
        // does not, so the caret goes to its first character instead of two cells
        // inside its own sentence — a reader following the caret should land on
        // the start of what it is being told.
        // Past the KIND MARK as well, on a row that has one. The caret says
        // where the row's own name begins, and a mark is a fact about the row
        // rather than part of what it is called — a reader following the caret
        // onto `: clear` has been pointed at punctuation.
        let indent = if self.matches.is_empty() {
            0
        } else {
            let mark = self
                .matches
                .get(self.cursor)
                .and_then(|index| self.rows[*index].mark)
                .map(|mark| mark.chars().count() + 1)
                .unwrap_or(0);
            (theme.glyphs.marker.chars().count() + mark) as u16
        };
        frame.set_cursor_position(Position {
            x: (area.x + indent).min(area.right().saturating_sub(1)),
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
