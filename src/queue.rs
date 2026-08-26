//! The queue surface: what is waiting behind the turn that is running.
//!
//! **A renderer over [`crate::app::App`]'s waiting prompts, and nothing else.**
//! It owns no state, decides nothing about when a queued line runs, and cannot
//! reorder one. `App` holds the queue because the driver drains it; this draws
//! it, and the separation is what keeps "what is waiting" a single fact with a
//! single owner.
//!
//! Three decisions shape everything here, and each of them is a *refusal*.
//!
//! **It is not modal.** An approval, a question and a plan take the whole
//! viewport because a run is stopped inside a harness callback waiting for the
//! answer — see [`crate::app::App::modal`], which is the one predicate naming
//! all three. Nothing is blocked by a queue: the turn goes on streaming, the
//! operator goes on typing, and the only thing this surface knows is a list. So
//! it takes no keyboard, sets no cursor, and the session it is drawn over
//! behaves exactly as it did before it appeared. Adding it to `modal()` would
//! make `Ctrl+C` reach a guard rather than the turn, which is a surface that
//! swallows the interrupt while claiming to be a list.
//!
//! **It never grows the viewport.** The rows it draws come out of the
//! composer's own allowance, the way [`crate::fleet::Fleet::render`] takes them,
//! and the composer keeps [`crate::app::COMPOSER_ROWS`] whatever is queued.
//! A surface that claimed a row per queued line would walk the transcript
//! upward by its own queue — every line typed mid-turn pushing the conversation
//! it was typed against off the screen — and the queue is at its longest exactly
//! when the turn under it is at its most worth reading.
//!
//! **It never hides the prompt.** This is the one place it parts company with
//! the fleet view, which takes the whole composer rect. That view is opened by a
//! key, so an operator who cannot see the prompt knows why and closes it; this
//! one opens on its own the moment something is queued, and a surface that
//! appears unasked and takes the prompt with it is a session that looks hung to
//! the person who caused it. So it draws above the composer or it does not draw
//! at all.
//!
//! What falls out of the last two together is the honest shape of a short
//! viewport: when the composer's allowance is a single row there is nothing to
//! spare, and this draws nothing. The notice
//! [`crate::app::App::queue_prompt`] already wrote is what says the line was
//! kept — a surface is what keeps it visible *afterwards*, and afterwards is
//! worth a row only when there is one to give.

use ratatui::layout::Rect;
use ratatui::text::{Line, Span};
use ratatui::widgets::Paragraph;
use ratatui::Frame;

use crate::glyphs::Glyphs;
use crate::picker::fit;
use crate::theme::{Theme, Tone};

/// The rows this surface would draw for `waiting`, oldest first, at most `room`
/// of them and none wider than `width`.
///
/// **The oldest are the ones kept.** They are the ones that run next, so a
/// window that followed the newest would show the operator the part of the queue
/// they are furthest from seeing happen. What did not fit is a count on the last
/// row rather than silence: a list that stops at the bottom of its rows and says
/// nothing is a queue an operator reads as three long when it is nine long, and
/// [`Glyphs::elision`] exists for exactly this — *lines are missing here*,
/// always followed by how many.
///
/// Separate from [`render`] so the arithmetic can be asserted without a frame.
pub fn rows(waiting: &[String], width: u16, room: u16, glyphs: &Glyphs) -> Vec<String> {
    let room = usize::from(room);
    if room == 0 || waiting.is_empty() {
        return Vec::new();
    }
    // **One row is the case that matters, because one row is what the running
    // viewport actually has.** Eight rows, of which the streaming tail, the
    // activity line, the rule and the three-row footer take six and the composer
    // keeps its floor — the blank this surface borrows is the seventh, and it
    // leaves exactly one. Spending that row on the count alone, which is what
    // reserving a row for the elision would do here, would draw a surface that
    // says three lines are waiting and never says what any of them is. The row
    // goes to the line that runs NEXT, with the rest counted on the end of it.
    let width = usize::from(width);
    if room == 1 && waiting.len() > 1 {
        let rest = waiting.len() - 1;
        return vec![fit(
            &format!(
                "1. {} {} {rest} more",
                one_line(&waiting[0]),
                glyphs.elision
            ),
            width,
            glyphs,
        )];
    }
    // The whole queue when it fits; otherwise one row fewer, because the last
    // row is spent saying what is under it.
    let shown = if waiting.len() <= room {
        waiting.len()
    } else {
        room - 1
    };
    let mut out: Vec<String> = waiting
        .iter()
        .take(shown)
        .enumerate()
        // Numbered rather than headed. A header row would spend the surface's
        // only row on a terminal that can spare one, and would repeat a sentence
        // the footer notice has already said; the position says both what the
        // rows are and which order they run in, on every row, for no rows at
        // all.
        .map(|(at, prompt)| fit(&format!("{}. {}", at + 1, one_line(prompt)), width, glyphs))
        .collect();
    let hidden = waiting.len() - shown;
    if hidden > 0 {
        out.push(fit(
            &format!("{} {hidden} more", glyphs.elision),
            width,
            glyphs,
        ));
    }
    out
}

/// A prompt as one row's worth of text.
///
/// **The newline is the danger, not the length.** A prompt finished with
/// `Shift+Enter` in it is several lines, and a `Line` handed text with a `\n`
/// inside it draws rows the layout never budgeted for — which is the one way a
/// surface that takes a fixed number of rows can push the composer off the
/// screen anyway. Collapsing the whitespace also takes tabs and carriage
/// returns, which a paste can carry and neither of which is a row of anything.
fn one_line(prompt: &str) -> String {
    prompt.split_whitespace().collect::<Vec<_>>().join(" ")
}

/// Draw what is waiting into the rows it has been given, and no others.
///
/// Muted, because none of it has run: the composer under it and the turn above
/// it are what is live, and a queue drawn in the colour of either would compete
/// with them for an attention it has no claim on.
///
/// **No cursor.** [`crate::fleet::Fleet::render`] parks one on its marked row
/// because the arrows move that marker and a reader has to be able to find it;
/// nothing here is selectable, and the composer below owns the cursor. A second
/// widget setting it would move the terminal's own cursor off the prompt an
/// operator is typing into.
pub fn render(waiting: &[String], frame: &mut Frame, area: Rect, theme: &Theme) {
    let lines: Vec<Line<'static>> = rows(waiting, area.width, area.height, &theme.glyphs)
        .into_iter()
        .map(|row| Line::from(Span::styled(row, theme.style(Tone::Muted))))
        .collect();
    if lines.is_empty() {
        return;
    }
    // Unwrapped on purpose: `rows` has already cut every line to the width, and
    // a wrap would turn one queued prompt into two rows inside a rectangle
    // measured in prompts.
    frame.render_widget(Paragraph::new(lines), area);
}
