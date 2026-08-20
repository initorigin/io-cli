//! The mark, printed once into scrollback at the start of a session.
//!
//! Into scrollback, not the viewport, so it scrolls away like any other output
//! instead of occupying rows for the life of the session.

use ratatui::style::Modifier;
use ratatui::text::{Line, Span};

use crate::picker::fit_left;
use crate::theme::{Theme, Tone};

/// The `IO CLI` mark in box-drawing characters.
const MARK: &[&str] = &[
    "██████╗  ██████╗      ██████╗ ██╗     ██╗",
    "╚═██╔═╝ ██╔═══██╗    ██╔════╝ ██║     ██║",
    "  ██║   ██║   ██║    ██║      ██║     ██║",
    "  ██║   ██║   ██║    ██║      ██║     ██║",
    "██████╗ ╚██████╔╝    ╚██████╗ ███████╗██║",
    "╚═════╝  ╚═════╝      ╚═════╝ ╚══════╝╚═╝",
];

/// The mark is forty-one columns wide, so it does not fit an eighty-column
/// terminal with anything beside it and does not fit a narrower one at all.
const MARK_WIDTH: u16 = 41;

/// Whether to show the mark.
///
/// Suppressed without colour, without a tty, and on a terminal too narrow to hold
/// it. The first two are the contract's; the third is the reason a splash is the
/// first thing that looks broken on a small window.
pub fn visible(coloured: bool, tty: bool, width: u16) -> bool {
    coloured && tty && width >= MARK_WIDTH
}

/// What the card says about the session it is opening.
///
/// Borrowed rather than owned strings would tie the splash to the lifetime of a
/// configuration this function is called before and after. Every one of these is
/// already in hand at the call site, and there are three of them.
#[derive(Debug, Default, Clone)]
pub struct About {
    /// The model the first turn will be sent to.
    pub model: Option<String>,
    /// The permission posture, in the status line's own spelling.
    pub policy: Option<String>,
    /// The workspace this session is held over.
    pub workspace: Option<String>,
}

/// Blank columns between the frame and anything inside it.
///
/// Two, which is the convention every well-drawn terminal card uses — lipgloss
/// writes it as `Padding(1, 2)` and it is what keeps text from appearing glued
/// to the border. The row padding is the blank line drawn under the top edge and
/// above the bottom one.
const PAD: usize = 2;

/// The width the card's inside is drawn to.
///
/// The mark plus the padding on each side. Fixed rather than the terminal's,
/// because a card that grew to a hundred and twenty columns would be a banner
/// with a field lost in the middle of it, and because a fixed width is the only
/// one both glyph sets can agree on.
const CARD: usize = MARK_WIDTH as usize + PAD * 2;

/// The lines to commit, mark included.
///
/// **A card, not a logo.** The mark alone said the product's name and nothing
/// else, so a session opened with a picture and then a prompt with no answer to
/// "what am I about to send this to, and what is it allowed to do". Those are the
/// two questions an operator has at the first prompt, and the status line answers
/// them in an abbreviation they have not learned yet.
///
/// It degrades in one step and not several: without colour, without a tty, or on
/// a terminal too narrow to hold the mark, the whole card goes and the version
/// line is what is left. A frame drawn round a mark that could not be drawn is a
/// box with a hole in it.
pub fn lines(theme: &Theme, tty: bool, width: u16, about: &About) -> Vec<Line<'static>> {
    let mut lines = Vec::new();
    if !visible(theme.coloured, tty, width) {
        lines.push(Line::from(Span::styled(
            format!("io {}", env!("CARGO_PKG_VERSION")),
            theme.style(Tone::Muted),
        )));
        lines.push(Line::from(""));
        return lines;
    }

    let [top_left, top_right, bottom_left, bottom_right, across, down] = theme.glyphs.frame;
    let edge = across.repeat(CARD);
    lines.push(rule(format!("{top_left}{edge}{top_right}"), theme));
    lines.push(row_of(theme, down, Vec::new(), 0));

    for row in MARK {
        lines.push(row_of(
            theme,
            down,
            vec![Span::styled(
                (*row).to_string(),
                theme.style(Tone::Accent).add_modifier(Modifier::BOLD),
            )],
            MARK_WIDTH as usize,
        ));
    }
    lines.push(row_of(theme, down, Vec::new(), 0));

    // The version and what this thing is, on one row. The sentence is the
    // README's own first line, cut to what fits beside a version.
    // Cut to what the card holds beside a version, rather than to what reads
    // best in isolation: a tagline that overran the frame drew a row through the
    // right-hand edge of the box it was inside.
    let version = format!("v{}", env!("CARGO_PKG_VERSION"));
    let tagline = "an agent that shows its work";
    lines.push(row_of(
        theme,
        down,
        vec![
            Span::styled(
                version.clone(),
                theme.style(Tone::Normal).add_modifier(Modifier::BOLD),
            ),
            Span::styled(
                format!("{}{tagline}", theme.glyphs.separator),
                theme.style(Tone::Muted),
            ),
        ],
        version.chars().count() + theme.glyphs.separator.chars().count() + tagline.chars().count(),
    ));

    let facts = [
        ("model", about.model.as_deref()),
        ("policy", about.policy.as_deref()),
        ("workspace", about.workspace.as_deref()),
    ];
    if facts.iter().any(|(_, value)| value.is_some()) {
        lines.push(row_of(theme, down, Vec::new(), 0));
    }
    for (label, value) in facts {
        let Some(value) = value else { continue };
        // The label column is fixed so the values line up as a column of their
        // own, which is the whole reason to draw them as a table rather than as
        // a sentence.
        let label = format!("{label:<10}");
        let room = CARD.saturating_sub(PAD * 2 + label.chars().count());
        let value = fit_left(value, room, &theme.glyphs);
        let width = label.chars().count() + value.chars().count();
        lines.push(row_of(
            theme,
            down,
            vec![
                Span::styled(label, theme.style(Tone::Muted)),
                Span::styled(value, theme.style(Tone::Normal)),
            ],
            width,
        ));
    }

    lines.push(row_of(theme, down, Vec::new(), 0));
    lines.push(rule(format!("{bottom_left}{edge}{bottom_right}"), theme));
    lines.push(Line::from(""));
    lines
}

/// One horizontal edge of the card.
fn rule(text: String, theme: &Theme) -> Line<'static> {
    Line::from(Span::styled(text, theme.style(Tone::Muted)))
}

/// One row inside the card: the left bar, a cell of padding, the content, then
/// whatever padding is needed to bring the right bar back into its column.
///
/// `width` is the content's own width in cells, handed in rather than measured,
/// because the content is a list of spans and the caller is the only thing that
/// knows how wide the text inside them is once a glyph set has been chosen.
fn row_of(theme: &Theme, down: &str, content: Vec<Span<'static>>, width: usize) -> Line<'static> {
    let muted = theme.style(Tone::Muted);
    let mut spans = vec![Span::styled(format!("{down}{:PAD$}", "",), muted)];
    spans.extend(content);
    // The inside is `CARD` cells: the padding, the content, then whatever is left
    // as padding. Counting the left bar into the total put the right one a column
    // early on every row of the card.
    spans.push(Span::styled(
        format!("{:pad$}{down}", "", pad = CARD.saturating_sub(width + PAD)),
        muted,
    ));
    Line::from(spans)
}
