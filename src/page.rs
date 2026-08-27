//! The shape of a page committed into the scrollback.
//!
//! **Four surfaces draw one, and until 0.22.0 two of them each carried their own
//! copy of the folding.** `crate::status::committed` and
//! `crate::context::committed` had the same twenty lines under two names,
//! `folded` and `wrapped`, differing only in that one took its indents as
//! arguments and the other hard-coded the same two numbers. `/cost` and `/stats`
//! would have been the third and fourth copies, so the folding lives here and
//! all four call it.
//!
//! # What a committed page is
//!
//! Three rule glyphs and a title, one fact per row, three rule glyphs and the
//! title again with `ends`. It lands in the terminal's own scrollback beside every
//! earlier turn, which is why it has edges at all: a passage with no edges is one
//! a reader cannot tell the extent of.
//!
//! **It is not a table, and that is a decision about eighty columns rather than
//! about taste** — the argument is written out in full at
//! `crate::status::committed` and it holds identically here. A table has a column
//! width, a column width is decided by the widest cell, and on these two pages the
//! widest cell is a model id. One fact per row, `label: value`, nothing padded
//! into a column and nothing aligned across rows, so there is no width for
//! anything to be squeezed out of. A row too long for the terminal is **folded**,
//! never cut.
//!
//! # Sections, which the older two pages did not need
//!
//! `/status` and `/context` are each one list. `/cost` and `/stats` are seven and
//! eight lists respectively — this run, this session, by model, by day — and a
//! reader has to be able to tell which figure belongs to which question. So a
//! page here is a sequence of [`Row`]s rather than a flat vector of pairs, and a
//! [`Row::Heading`] is what separates them. It is drawn in the accent tone and
//! indented to zero, so it reads as a break rather than as another fact.

use ratatui::text::{Line, Span};

use crate::theme::{Theme, Tone};

/// One row of a committed page.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Row {
    /// `label: value`, folded at two columns and continued at four.
    Fact(String, String),
    /// A section break within the page.
    Heading(String),
    /// A whole sentence, in a tone of its own.
    ///
    /// What a page says when it has no figure to give: that the price table is
    /// empty, that a total is a floor because some calls were unpriced, that a
    /// provider reported no usage for a call. **These are the rows that make the
    /// figures trustworthy**, so they are rows rather than a footnote somewhere
    /// else, and they carry a tone because "this total is incomplete" is not the
    /// same kind of statement as "this total is 3,412 tokens".
    Note(String, Tone),
    /// A blank row, for separating a heading from what came before it.
    Blank,
}

impl Row {
    /// `label: value`, taking anything that can spell itself.
    pub fn fact(label: impl Into<String>, value: impl Into<String>) -> Self {
        Self::Fact(label.into(), value.into())
    }

    /// A section break.
    pub fn heading(text: impl Into<String>) -> Self {
        Self::Heading(text.into())
    }

    /// A sentence in the ordinary tone.
    pub fn note(text: impl Into<String>) -> Self {
        Self::Note(text.into(), Tone::Muted)
    }

    /// A sentence that qualifies a figure above it.
    ///
    /// `Warning` rather than `Muted`, because a total that is a floor and a total
    /// that is a total must not read the same at a glance. This is the tone the
    /// "lying by omission" rule in io-harness's own pricing documentation exists
    /// to prevent a renderer from skipping.
    pub fn caveat(text: impl Into<String>) -> Self {
        Self::Note(text.into(), Tone::Warning)
    }
}

/// A whole page, edged and folded, ready for `Screen::commit`.
///
/// `title` appears in both rules, the closing one suffixed with `ends`, which is
/// the edge `crate::status::committed` and `crate::transcript` already give a
/// committed passage.
pub fn commit(title: &str, rows: &[Row], theme: &Theme, width: u16) -> Vec<Line<'static>> {
    let rule = theme.glyphs.rule;
    let room = width as usize;
    let mut lines = vec![Line::from(Span::styled(
        format!("{rule}{rule}{rule} {title}"),
        theme.style(Tone::Accent),
    ))];
    for row in rows {
        match row {
            Row::Fact(label, value) => {
                for text in folded(&format!("{label}: {value}"), room, 2, 4) {
                    lines.push(Line::from(Span::styled(text, theme.style(Tone::Normal))));
                }
            }
            // Indented to zero rather than to two, so a heading is distinguishable
            // from a fact by position as well as by colour — which is the whole of
            // what a reader in `--plain` or under `NO_COLOR` has to go on.
            Row::Heading(text) => {
                for text in folded(text, room, 0, 2) {
                    lines.push(Line::from(Span::styled(text, theme.style(Tone::Accent))));
                }
            }
            Row::Note(text, tone) => {
                for text in folded(text, room, 2, 4) {
                    lines.push(Line::from(Span::styled(text, theme.style(*tone))));
                }
            }
            Row::Blank => lines.push(Line::from(String::new())),
        }
    }
    lines.push(Line::from(Span::styled(
        format!("{rule}{rule}{rule} {title} ends"),
        theme.style(Tone::Accent),
    )));
    lines
}

/// `text` as rows no wider than `width`, indented `first` and then `rest`.
///
/// **Folded and never fitted**, which is the one thing this differs in from every
/// other width-aware helper in this crate. [`crate::picker::fit`] and
/// [`crate::status::Status::line`] shorten, because a picker row and a status line
/// each own exactly one row of a viewport that cannot grow. A committed surface
/// owns as many rows as it needs, so there is no reason left to lose a character —
/// and the characters most likely to be lost are the tail of a workspace path, the
/// tail of a policy layer's act list and the tail of a model id, which is to say
/// the answer.
///
/// Broken at spaces, with a hanging indent that says a row is a continuation
/// without aligning anything into a column. A word longer than the room it has —
/// a deep path, an absurd model name — is **split** rather than allowed to
/// overflow: eighty columns is a supported size, and a row that runs past it gets
/// wrapped by the terminal at a place nothing here chose.
pub fn folded(text: &str, width: usize, first: usize, rest: usize) -> Vec<String> {
    let mut rows: Vec<String> = Vec::new();
    let mut indent = first;
    let mut row = " ".repeat(indent);
    // Content characters on this row, which is its width less the indent. Counted
    // rather than measured off `row` so the arithmetic is in one unit — the same
    // reason `Status::fits` counts characters and not bytes.
    let mut used = 0usize;
    for word in text.split_whitespace() {
        let mut word = word;
        loop {
            // At least one cell, so a terminal narrower than the indent still
            // makes progress instead of looping forever.
            let room = width.saturating_sub(indent).max(1);
            let space = usize::from(used > 0);
            let length = word.chars().count();
            if used + space + length <= room {
                if space == 1 {
                    row.push(' ');
                }
                row.push_str(word);
                used += space + length;
                break;
            }
            if used > 0 {
                rows.push(std::mem::take(&mut row));
                indent = rest;
                row = " ".repeat(indent);
                used = 0;
                // Retried whole on the fresh row: a word is only ever split when
                // a row of its own cannot hold it.
                continue;
            }
            let head: String = word.chars().take(room).collect();
            word = &word[head.len()..];
            row.push_str(&head);
            rows.push(std::mem::take(&mut row));
            indent = rest;
            row = " ".repeat(indent);
        }
    }
    if used > 0 || rows.is_empty() {
        rows.push(row);
    }
    rows
}
