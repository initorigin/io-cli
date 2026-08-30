//! How many rows a surface needs, and how it says what it could not show.
//!
//! **Every surface in this product measured itself wrong until 0.32.0, and they
//! all measured it wrong the same way.** `Intent::render` and `Review::render`
//! both pushed logical [`Line`]s, took `lines.len()` as the height they occupy,
//! and handed them to a `Paragraph` **with wrapping switched on**. A question, a
//! context line or a plan step longer than the terminal is wide occupies more
//! rows than it is lines, so the paragraph consumed the whole area while the count
//! still said it had room — which lost the choices and the footer, and then drew
//! the composer on top of rows the paragraph had already painted.
//!
//! The correction is not a more careful count of lines. It is to stop counting
//! lines: [`wrapped`] asks ratatui itself how many rows a wrapped paragraph will
//! occupy, through the same `Paragraph::line_count` that [`crate::term::Screen::commit`]
//! has always used to tell `insert_before` how tall its insertion is. That method
//! is the wrapper, not a model of it, so the measurement and the render cannot
//! disagree.
//!
//! [`elide`] is the other half. A surface that cannot show everything says so with
//! a count, because silence is indistinguishable from there being nothing more —
//! and a plan whose last two steps are missing without a word is a plan somebody
//! approves without knowing they have not read it.
//!
//! **This is deliberately not a fourth spelling of an idiom the product already
//! has three of.** `approval.rs` carries two (`preview` rides the suffix on the
//! last shown line and fits with `picker::fit`; `as_diff` builds spans and fits
//! with its own `fit_line`) and `queue.rs` a third, on `String`s. Those keep their
//! own — their tests assert their exact shapes and none of them wraps. What is new
//! here is the case none of them handles: a surface whose content wraps, where the
//! number that has to be reported is a count of **rows**, not of items.

use ratatui::text::{Line, Text};
use ratatui::widgets::{Paragraph, Wrap};

use crate::theme::{Theme, Tone};

/// Rows a wrapped paragraph of `lines` occupies at `width`.
///
/// The one measurement, so a surface's idea of its own height and what ratatui
/// actually paints are the same number by construction. `width` of zero is
/// treated as one: a zero-width area draws nothing, and answering zero would tell
/// a caller it had infinite room.
pub fn wrapped(lines: &[Line<'_>], width: u16) -> u16 {
    let text = Text::from(lines.to_vec());
    let paragraph = Paragraph::new(text).wrap(Wrap { trim: false });
    u16::try_from(paragraph.line_count(width.max(1))).unwrap_or(u16::MAX)
}

/// The line a surface draws instead of the rows it had no room for.
///
/// `⋯ N more rows` in Unicode, `... N more rows` in ASCII — [`crate::glyphs`]
/// holds the rule that both spellings occupy the column the other does. Drawn
/// [`Tone::Muted`], which carries no word, so it reads the same under `NO_COLOR`
/// as it does in colour: the count is the message and it is in the text.
///
/// **Rows rather than items**, because that is what the reader lost. A plan step
/// that wraps to three rows and is not shown is three rows missing from the
/// screen, and saying "1 more step" understates a surface the operator is being
/// asked to approve.
pub fn more(rows: u16, theme: &Theme) -> Line<'static> {
    theme.notice(
        Tone::Muted,
        format!("{} {rows} more rows", theme.glyphs.elision),
    )
}

/// Keep what fits in `room` rows, and end with a count of the rows dropped.
///
/// Returns `lines` untouched when they already fit — the everyday case now that
/// the viewport grows to what a surface asks for, and the reason this is a
/// fallback rather than the normal path.
///
/// The suffix takes a row of its own rather than riding the last line kept, which
/// is what `approval.rs` does. With wrapping on, riding it means re-fitting a line
/// whose own width is not known until it has been wrapped, and getting that wrong
/// silently drops a row while claiming to report one. A row spent saying what is
/// missing is the cheapest honest answer.
///
/// The search starts at `room` lines rather than at the end: every line occupies
/// at least one row, so a prefix longer than `room` can never fit in `room` rows.
/// That bounds the loop by the height of a viewport rather than by the length of
/// the content, which matters for a picker over four hundred rows.
pub fn elide(
    lines: Vec<Line<'static>>,
    room: u16,
    width: u16,
    theme: &Theme,
) -> Vec<Line<'static>> {
    if room == 0 {
        return Vec::new();
    }
    let total = wrapped(&lines, width);
    if total <= room {
        return lines;
    }
    let mut keep = lines.len().min(usize::from(room));
    while keep > 0 && wrapped(&lines[..keep], width).saturating_add(1) > room {
        keep -= 1;
    }
    let mut out = lines[..keep].to_vec();
    let hidden = total.saturating_sub(wrapped(&out, width));
    out.push(more(hidden, theme));
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::theme::Theme;

    fn theme() -> Theme {
        // The ASCII glyph set deliberately, so the count row is asserted in the
        // spelling a terminal with no Unicode gets. Both sets occupy the same
        // column, so an assertion that holds here holds for the other.
        Theme::resolve(
            true,
            crate::theme::Background::Dark,
            None,
            crate::glyphs::ASCII,
        )
    }

    #[test]
    fn a_short_line_is_one_row_and_a_long_one_is_more() {
        let short = vec![Line::from("hello")];
        assert_eq!(wrapped(&short, 20), 1);

        // The defect this module exists for: one line, several rows.
        let long = vec![Line::from("hello world this wraps")];
        assert_eq!(
            wrapped(&long, 20),
            2,
            "a wrapped line still counted as one row"
        );
        assert!(
            wrapped(&long, 20) > u16::try_from(long.len()).unwrap(),
            "the wrapped height must exceed the line count, or the measurement is \
             the `lines.len()` one it replaces",
        );
    }

    #[test]
    fn a_zero_width_area_does_not_report_infinite_room() {
        // `Paragraph::line_count` answers 0 at width 0, which would tell a caller
        // it had room for everything. Treating 0 as 1 answers what a one-column
        // terminal would actually need — eight rows for eight characters — which
        // is large rather than free, and that is the safe direction.
        assert!(
            wrapped(&[Line::from("anything")], 0) >= 1,
            "a zero-width area reported no rows, which reads as unlimited room",
        );
        assert_eq!(
            wrapped(&[Line::from("anything")], 0),
            wrapped(&[Line::from("anything")], 1)
        );
    }

    #[test]
    fn content_that_fits_is_returned_untouched() {
        let lines = vec![Line::from("one"), Line::from("two")];
        let out = elide(lines.clone(), 8, 40, &theme());
        assert_eq!(out.len(), lines.len());
        assert_eq!(wrapped(&out, 40), 2);
    }

    #[test]
    fn content_that_does_not_fit_is_cut_and_counted() {
        let lines: Vec<Line<'static>> = (0..20).map(|n| Line::from(format!("row {n}"))).collect();
        let out = elide(lines, 5, 40, &theme());

        assert!(
            wrapped(&out, 40) <= 5,
            "the elided form must fit the room it was given",
        );
        let last = out.last().expect("a count row").to_string();
        assert!(
            last.contains("more rows"),
            "the surface dropped rows without saying so: {last:?}",
        );
        // 20 rows asked for, 5 available, one of which is the count itself.
        assert!(
            last.contains("16"),
            "the count must be the rows actually dropped, but it read {last:?}",
        );
    }

    #[test]
    fn the_count_is_rows_and_not_items_when_content_wraps() {
        // Three lines, each wrapping to two rows at this width.
        let lines: Vec<Line<'static>> = (0..3)
            .map(|n| Line::from(format!("item {n} is long enough to wrap here")))
            .collect();
        let width = 20;
        let total = wrapped(&lines, width);
        assert!(total > 3, "the fixture does not wrap, so it proves nothing");

        let out = elide(lines, 4, width, &theme());
        let last = out.last().expect("a count row").to_string();
        let dropped = total - wrapped(&out[..out.len() - 1], width);
        assert!(
            last.contains(&dropped.to_string()),
            "counted items rather than rows: {last:?} with {dropped} rows dropped",
        );
    }

    #[test]
    fn no_room_at_all_draws_nothing_rather_than_a_count_alone() {
        let lines = vec![Line::from("one"), Line::from("two")];
        assert!(elide(lines, 0, 40, &theme()).is_empty());
    }
}
