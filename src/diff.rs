//! Rendering an edit the agent made.
//!
//! **io-cli computes no diff.** io-harness renders a unified diff for every edit
//! its tools make and keeps it in the run's durable trace, so what this module
//! draws is text that already exists — `io_harness::Edit::hunk`, a hunk body
//! whose `@@` line numbers are the *file's* rather than the fragment's. That is
//! the difference between a diff this product can show and one it could only
//! approximate: io-cli never sees the file, so it could not number a hunk
//! correctly even if it wanted to compute one.
//!
//! Two consequences shape everything here.
//!
//! **The hunk is passed through, never reconstructed.** Its markers, its
//! spacing and its header are the harness's; this module decides colour and
//! nothing else. A reader who copies a rendered line into `patch` gets what
//! `patch` expects.
//!
//! **An absent hunk is a fact, not an empty one.** `Edit::hunk` is `None` when
//! the row predates the harness release that added hunks, when the file's
//! previous contents were not kept (over the store's snapshot cap, or not
//! UTF-8), or when the rendered diff would itself have exceeded that cap. Not
//! one of those is "nothing changed" — and `lines_added` and `lines_removed`
//! are still there to prove it — so the cell says the counts and says the diff
//! is missing, rather than drawing an empty body that reads as an untouched
//! file.

use io_harness::Edit;
use ratatui::text::{Line, Span};

use crate::status::SEPARATOR;
use crate::theme::{Theme, Tone};

/// What every line of the cell is indented by, so a hunk sits under its header
/// the way a tool call sits under a step.
const INDENT: &str = "  ";

/// The edits a given step made, out of everything the run recorded.
///
/// `Store::edits` answers for the whole run, so a caller that draws what it
/// returns re-renders every earlier edit at every later step — the transcript
/// grows quadratically and the same diff appears four times. Kept here rather
/// than in the driver so that it is reachable from a test; the driver is a
/// binary and an integration test cannot link one.
pub fn for_step(edits: Vec<Edit>, step: u32) -> Vec<Edit> {
    edits.into_iter().filter(|edit| edit.step == step).collect()
}

/// One edit, as lines for the terminal's scrollback.
///
/// The header first — the path, then the counts, then the tool — because the
/// path is the content and the rest is metadata, which is the rule the whole
/// interface follows.
pub fn cell(edit: &Edit, theme: &Theme) -> Vec<Line<'static>> {
    let mut lines = vec![header(edit, theme)];

    let Some(hunk) = edit.hunk.as_deref() else {
        // The header already carried the absence. Returning one line here is not
        // a shortcut: a body of zero lines under a header is exactly the empty
        // diff this must not draw.
        return lines;
    };

    lines.extend(hunk.lines().map(|line| body(line, theme)));
    lines.push(Line::from(""));
    lines
}

/// `  src/theme.rs · +1 -1 · edit_file`, and `· no diff stored` when there is
/// no hunk to draw under it.
fn header(edit: &Edit, theme: &Theme) -> Line<'static> {
    let mut spans = vec![
        Span::styled(INDENT.to_string(), theme.style(Tone::Muted)),
        Span::styled(edit.path.clone(), theme.style(Tone::Accent)),
        Span::styled(SEPARATOR.to_string(), theme.style(Tone::Muted)),
        Span::styled(format!("+{}", edit.lines_added), theme.style(Tone::Added)),
        Span::styled(" ".to_string(), theme.style(Tone::Muted)),
        Span::styled(
            format!("-{}", edit.lines_removed),
            theme.style(Tone::Removed),
        ),
        Span::styled(
            format!("{SEPARATOR}{}", edit.tool),
            theme.style(Tone::Muted),
        ),
    ];
    if edit.hunk.is_none() {
        spans.push(Span::styled(
            format!("{SEPARATOR}no diff stored"),
            theme.style(Tone::Muted),
        ));
    }
    Line::from(spans)
}

/// One line of a hunk body, coloured by the marker the harness put on it.
///
/// The marker stays on the line. Colour is never the sole carrier of meaning
/// here — under `NO_COLOR` every style collapses to nothing and the `+` and the
/// `-` are what is left saying which side a line is on.
fn body(line: &str, theme: &Theme) -> Line<'static> {
    let tone = match line.as_bytes().first() {
        Some(b'+') => Tone::Added,
        Some(b'-') => Tone::Removed,
        // `@@ … @@` — where in the file this is. The one part of a hunk a reader
        // navigates by, so it gets the product's own colour rather than the
        // muted one context lines take.
        Some(b'@') => Tone::Accent,
        // `\ No newline at end of file`. Real information about the bytes, and
        // not a change, so it reads as neither.
        Some(b'\\') => Tone::Muted,
        _ => Tone::Muted,
    };
    Line::from(Span::styled(format!("{INDENT}{line}"), theme.style(tone)))
}
