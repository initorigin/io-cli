//! The mark, printed once into scrollback at the start of a session.
//!
//! Into scrollback, not the viewport, so it scrolls away like any other output
//! instead of occupying rows for the life of the session.

use ratatui::text::{Line, Span};

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

/// The lines to commit, mark included.
pub fn lines(theme: &Theme, tty: bool, width: u16) -> Vec<Line<'static>> {
    let mut lines = Vec::new();
    if visible(theme.coloured, tty, width) {
        for row in MARK {
            lines.push(Line::from(Span::styled(
                (*row).to_string(),
                theme.style(Tone::Accent),
            )));
        }
        lines.push(Line::from(""));
    }
    lines.push(Line::from(Span::styled(
        format!("io {}", env!("CARGO_PKG_VERSION")),
        theme.style(Tone::Muted),
    )));
    lines.push(Line::from(""));
    lines
}
