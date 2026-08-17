//! The whole conversation, rendered for the terminal's own scrollback.
//!
//! This is what `Ctrl+T` commits. It is not a screen and cannot become one: the
//! viewport is four fixed rows that ratatui will not let anything grow, and there
//! is no alternate screen in this product to page a long conversation in. So the
//! output here is a `Vec<Line>` handed to `Screen::commit`, which pushes it above
//! the viewport with `insert_before` and leaves it in the terminal's real
//! scrollback — where the terminal's own scroll, search and copy already work.
//! Anything that paginated this would be re-implementing the scrollback the
//! product deliberately kept.
//!
//! **Branched-away turns are rendered, and said out loud.**
//! [`io_harness::Session::history`] returns only the turns the model can still
//! see, which is right for driving a next turn and wrong for reading back what
//! happened: a branch leaves earlier turns off the path, and
//! [`TranscriptTurn::on_path`] is the only place in this whole product where they
//! are visible at all. A renderer that skipped them would silently disagree with
//! the database about what the operator did, in the one surface built to answer
//! that — so every turn appears, and the ones the model has lost carry the words
//! saying so rather than a colour saying so.
//!
//! The compaction summaries each turn carries are not drawn. Nothing else in this
//! release renders a fold, and one is a paragraph standing in for steps whose own
//! narration is not on screen either; drawing it alone would raise a question the
//! interface has no other answer to yet.

use io_harness::{Transcript, TranscriptTurn};
use ratatui::text::{Line, Span};

use crate::events::outcome_tone;
use crate::status::SEPARATOR;
use crate::theme::{Theme, Tone};

/// The words a turn the model can no longer see is labelled with.
///
/// Words rather than a tone, because a tone is not readable under `NO_COLOR`, on
/// a monochrome terminal, or to a colour-blind reader — and this is the single
/// fact about a conversation that a reader cannot reconstruct from anywhere else
/// in the product.
pub const BRANCHED_AWAY: &str = "left behind by a branch";

/// The conversation as lines, oldest turn first.
///
/// Opens and closes with a marker so the transcript has visible edges in a
/// scrollback that also holds every earlier turn, every command's output and
/// whatever the shell printed before `io` started. The closing marker carries the
/// turn count, which is what tells a reader that the passage they just scrolled
/// past was the whole conversation rather than as much of it as fitted.
pub fn lines(transcript: &Transcript, theme: &Theme) -> Vec<Line<'static>> {
    let mut out = vec![Line::from(Span::styled(
        format!(
            "─── transcript begins{SEPARATOR}session {}{SEPARATOR}{}",
            transcript.session_id,
            transcript.root.display()
        ),
        theme.style(Tone::Accent),
    ))];

    if transcript.turns.is_empty() {
        // A sentence, not an empty gap. Committing nothing at all is
        // indistinguishable from the key having done nothing at all.
        out.push(Line::from(Span::styled(
            "This session has no turns yet, so there is nothing to show.".to_string(),
            theme.style(Tone::Muted),
        )));
    }

    for turn in &transcript.turns {
        out.extend(turn_lines(turn, theme));
    }

    let count = transcript.turns.len();
    let branched = transcript.turns.iter().filter(|turn| !turn.on_path).count();
    let mut closing = format!(
        "─── transcript ends{SEPARATOR}{count} turn{}",
        if count == 1 { "" } else { "s" }
    );
    if branched > 0 {
        closing.push_str(&format!("{SEPARATOR}{branched} {BRANCHED_AWAY}"));
    }
    out.push(Line::from(Span::styled(closing, theme.style(Tone::Accent))));
    out
}

/// One turn: what was asked, what was answered, and how it ended.
///
/// The prompt comes before the turn id on its line, never after it — the
/// codebase-wide rule that content precedes metadata. A reader skims the left
/// edge for what was said, an id at the head of every line makes that column
/// unreadable, and the id is only ever wanted once the line has already been
/// found.
fn turn_lines(turn: &TranscriptTurn, theme: &Theme) -> Vec<Line<'static>> {
    // An empty prompt is still one row: `str::lines` yields nothing for an empty
    // string, and a turn with no row at all would lose its id and its branch
    // label along with it.
    let mut rows: Vec<&str> = turn.prompt.lines().collect();
    if rows.is_empty() {
        rows.push("");
    }

    let mut lines: Vec<Line<'static>> = Vec::new();
    for (index, row) in rows.iter().enumerate() {
        let marker = if index == 0 { "› " } else { "  " };
        lines.push(Line::from(vec![
            Span::styled(marker, theme.style(Tone::Accent)),
            Span::styled((*row).to_string(), theme.style(Tone::Normal)),
        ]));
    }

    // The tail rides the LAST prompt row rather than the first, so a multi-line
    // prompt is not interrupted by its own metadata halfway down.
    let tail = lines.last_mut().expect("a prompt is at least one row");
    if !turn.on_path {
        tail.spans
            .push(Span::styled(SEPARATOR, theme.style(Tone::Muted)));
        tail.spans
            .push(Span::styled(BRANCHED_AWAY, theme.style(Tone::Warning)));
    }
    tail.spans.push(Span::styled(
        format!("{SEPARATOR}turn {}", turn.turn_id),
        theme.style(Tone::Muted),
    ));

    match turn
        .reply
        .as_deref()
        .filter(|reply| !reply.trim().is_empty())
    {
        Some(reply) => {
            for row in reply.lines() {
                let span = Span::styled(row.to_string(), theme.style(Tone::Normal));
                lines.push(Line::from(span));
            }
        }
        // Said, not omitted. A turn that produced no text is a different fact from
        // a turn missing from the transcript, and only one of those is worth
        // going to look into.
        None => lines.push(Line::from(Span::styled(
            "  no reply".to_string(),
            theme.style(Tone::Muted),
        ))),
    }

    if let Some(outcome) = &turn.outcome {
        // `notice` is how every toned line in this product is built, so the tone's
        // word is in front of the sentence by construction rather than by anyone
        // remembering to put it there.
        lines.push(theme.notice(
            outcome_tone(outcome),
            format!("{outcome}{SEPARATOR}run {}", turn.run_id),
        ));
    }

    lines.push(Line::from(""));
    lines
}
