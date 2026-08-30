//! The queue surface: what is waiting behind the turn that is running.
//!
//! **A renderer over [`crate::app::App`]'s waiting prompts, plus the one piece
//! of state a renderer cannot borrow: where the operator is inside them.**
//! `App` still owns the queue itself — the driver drains it, and "what is
//! waiting" stays a single fact with a single owner. What lives here is
//! [`Cursor`]: which line is marked and which line has been taken into the
//! composer to be edited. That split is deliberate and it is the same one
//! [`crate::picker::Picker`] makes. A selection is not a fact about the
//! session, it is a fact about the *surface* — it is meaningless with the
//! surface shut, it must not survive into a session that has no queue, and
//! nothing outside the drawing of these rows has any business reading it. Put
//! on `App` it would be a fifth field about a list that already has three, and
//! every mutation of the queue would have to remember to keep it honest;
//! held here, the verbs that move the queue and the arithmetic that draws it
//! are the same few lines and cannot disagree.
//!
//! The verbs take `&mut Vec<String>` rather than owning it, which is the whole
//! of how both halves stay true at once: the queue is still `App`'s, and the
//! only code that reorders it is the code that draws it.
//!
//! Three decisions shape everything here, and each of them is a *refusal*.
//!
//! **It is not modal.** An approval, a question and a plan take the whole
//! viewport because a run is stopped inside a harness callback waiting for the
//! answer — see [`crate::app::App::modal`], which is the one predicate naming
//! all three. Nothing is blocked by a queue: the turn goes on streaming, the
//! operator goes on typing, and the only thing this surface knows is a list. It
//! sets no cursor, and the session it is drawn over behaves exactly as it did
//! before it appeared. Adding it to `modal()` would make `Ctrl+C` reach a guard
//! rather than the turn, which is a surface that swallows the interrupt while
//! claiming to be a list.
//!
//! It does take four keys, and only while it is up — the arrows, the shifted
//! arrows, `Enter` on an empty prompt and `Esc` — which is what F3 asks for and
//! is not the same thing as taking the keyboard. Every other key still falls
//! through to the composer, because typing is how the *next* line joins the
//! queue this is drawing. The cost is named where it is paid: while the surface
//! is open `Up` marks a row instead of walking back through prompt history, the
//! same trade the fleet view already makes with the same two keys. `Esc` shuts
//! the surface and hands both back. Binding those keys at the *composer* rather
//! than inside the open surface is the one mistake this design has to refuse —
//! it would cost an operator with nothing queued the history recall that has
//! been documented since 0.1.0, in exchange for moving a selection through a
//! list that is not on screen.
//!
//! **It never grows the viewport.** The rows it draws come out of the
//! composer's own allowance, the way [`crate::fleet::Fleet::render`] takes them,
//! and the composer keeps [`crate::app::COMPOSER_ROWS`] whatever is queued.
//! A surface that claimed a row per queued line would walk the transcript
//! upward by its own queue — every line typed mid-turn pushing the conversation
//! it was typed against off the screen — and the queue is at its longest exactly
//! when the turn under it is at its most worth reading.
//!
//! **Where the spare row comes from, and why there is one.** The composer's
//! allowance at [`crate::term::VIEWPORT_HEIGHT`] — the eight rows a running turn
//! actually holds — is exactly `COMPOSER_ROWS`, so a surface taking only what is
//! left over that floor would draw nothing on every real session. So while the
//! queue is open the layout releases the **blank row above the activity line**
//! into the composer's allowance, and takes it back the moment the queue closes:
//! see `air_rows` in [`crate::app::App::render`] for the argument, and
//! `n2_the_surface_is_visible_at_the_running_viewport_and_costs_it_no_height` in
//! `tests/queue_surface.rs` for the assertion. The blank carries nothing and the
//! queue carries something; the frame is the same height either way, which is
//! what "never grows the viewport" was always about.
//!
//! **It never hides the prompt.** This is the one place it parts company with
//! the fleet view, which takes the whole composer rect. That view is opened by a
//! key, so an operator who cannot see the prompt knows why and closes it; this
//! one opens on its own the moment something is queued, and a surface that
//! appears unasked and takes the prompt with it is a session that looks hung to
//! the person who caused it. So it draws above the composer or it does not draw
//! at all.
//!
//! What falls out of the last two together is the honest shape of a terminal
//! too short even for the blank: below eight rows there is no row to release,
//! nothing is spare over `COMPOSER_ROWS`, and this draws nothing. The notice
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

/// Where the operator is inside the queue, and which line they are editing.
///
/// **Two `Option`s and no third field, because every other question is already
/// answered by the queue itself.** How long the list is, what is on it and
/// whether it is on screen at all are `App`'s — see
/// [`crate::app::App::queue_open`], which is three facts and no fourth field for
/// the same reason. This holds only what the queue cannot tell you: a mark, and
/// a line that is out of the queue and in the prompt.
///
/// **The mark is stored as an index and read through
/// [`Cursor::selection`], which clamps.** The queue mutates underneath it —
/// the driver takes the oldest off the front between turns, and the operator can
/// drop a line from the middle — so an index held here can outlive the line it
/// pointed at. 0.7.0 paid for the other answer in the picker: an index stored
/// into a list that is *recomputed* moves the marked row every time the list
/// changes shape. Here the list is not derived from anything, so the index is
/// the honest handle; what it needs is a reader that never indexes past the end,
/// and every read in this module goes through that one.
#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct Cursor {
    /// The marked line, as an index into the queue. `None` until an arrow says
    /// otherwise, and that is what keeps a surface nobody has touched drawing
    /// exactly the rows it drew before this type existed.
    at: Option<usize>,
    /// The line taken into the composer, as the position it came from and the
    /// text it had when it left.
    ///
    /// **Both halves, because the second is the undo.** The composer holds what
    /// the operator is typing *now*; this holds what they would get back by
    /// pressing `Esc`, which is 0.13.1's rule — an erase is undoable where the
    /// undo is cheap, and one `String` for the length of one edit is as cheap as
    /// undo gets.
    editing: Option<(usize, String)>,
}

/// What putting an edited line back did.
///
/// Returned rather than said here: the sentence belongs to the footer, which is
/// `App`'s, and this module draws rows.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Put {
    /// It went back, at this position.
    Kept(usize),
    /// There was nothing left of it to put back, so the line is gone. Carries
    /// the text it had, so the caller can name what it dropped.
    Dropped(String),
}

impl Cursor {
    /// The marked line, given how long the queue is now.
    ///
    /// The only way to read the mark. See the field's own note for why.
    pub fn selection(&self, waiting: usize) -> Option<usize> {
        let at = self.at?;
        (waiting > 0).then(|| at.min(waiting - 1))
    }

    /// Move the mark by `delta`, and say whether the key was ours.
    ///
    /// **`false` is not a failure, it is the key falling through to the
    /// composer**, and it is what keeps `Down` from meaning two things. With
    /// nothing marked, `Up` enters the list at the line nearest the prompt — the
    /// row physically above the composer the operator's caret is in, which is
    /// where an upward key lands on any other list in any other program. `Down`
    /// with nothing marked enters nothing: below the queue is the composer, the
    /// operator is already there, and the key goes on doing what it does at a
    /// prompt with a queue behind it.
    pub fn move_by(&mut self, delta: isize, waiting: usize) -> bool {
        if waiting == 0 {
            return false;
        }
        let last = waiting - 1;
        let at = match self.selection(waiting) {
            Some(at) => at as isize + delta,
            None if delta < 0 => last as isize,
            None => return false,
        };
        // Clamped rather than wrapped. A list of things that are about to run
        // has a first and a last, and an arrow that jumped from one end to the
        // other would move a line the operator was not looking at.
        self.at = Some(at.clamp(0, last as isize) as usize);
        true
    }

    /// Move the marked **line** by `delta`, carrying the mark with it.
    ///
    /// The mark follows the text rather than staying on the slot, which is the
    /// whole difference between reordering a queue and scrolling through one:
    /// three presses move a line three places, and the operator watches the same
    /// line travel. A mark that stayed put would move a different line on every
    /// press.
    ///
    /// Refused at either end rather than wrapped, for [`Cursor::move_by`]'s
    /// reason, and refused with nothing marked because there is no line to move.
    pub fn reorder(&mut self, delta: isize, waiting: &mut [String]) -> bool {
        let Some(at) = self.selection(waiting.len()) else {
            return false;
        };
        let to = at as isize + delta;
        if to < 0 || to >= waiting.len() as isize {
            return false;
        }
        let to = to as usize;
        waiting.swap(at, to);
        self.at = Some(to);
        true
    }

    /// The position of the line currently out of the queue and in the composer.
    pub fn editing(&self) -> Option<usize> {
        self.editing.as_ref().map(|(at, _)| *at)
    }

    /// Take the marked line **out** of the queue, to be edited in the composer.
    ///
    /// **Out, not copied, and that is the decision the whole edit hangs on.** A
    /// line drawn in the queue while a second copy of it sits in the prompt is a
    /// session that would run it twice if the turn ended in between — and the
    /// turn *can* end in between, because nothing about an edit stops one. Out of
    /// the queue there is exactly one copy of that prompt in the session and it
    /// is in the most visible place a session has, so the answer to "what happens
    /// to a line being edited when the turn ends underneath it" is: nothing
    /// happens to it. The surface closes with the turn, the drain never sees the
    /// line, and the operator is left holding it at an idle prompt where `Enter`
    /// sends it as its own turn — which is the turn it was queued to become.
    ///
    /// `None` when nothing is marked or an edit is already in flight, so the
    /// keystroke falls through to the composer rather than silently taking a
    /// second line.
    pub fn take(&mut self, waiting: &mut Vec<String>) -> Option<String> {
        if self.editing.is_some() {
            return None;
        }
        let at = self.selection(waiting.len())?;
        let text = waiting.remove(at);
        self.editing = Some((at, text.clone()));
        Some(text)
    }

    /// Put an edited line back where it came from.
    ///
    /// **At its own position, and the position is the point.** A queue is an
    /// order; an edit that returned a line to the end of it would have quietly
    /// reordered the session's next few turns as the price of fixing a typo, and
    /// the operator has a key for reordering when they want one.
    ///
    /// Clamped to the length, because the queue can be shorter than it was: the
    /// driver may have drained a line while the edit was open. Past the end is
    /// the end, which is still the truthful answer to "after everything that was
    /// in front of it".
    ///
    /// **Empty is a drop.** There is no other honest reading of a line an
    /// operator has erased and pressed `Enter` on: an empty prompt is not a
    /// prompt — [`crate::composer::Composer`] has refused to submit one since
    /// 0.1.0 — and putting an empty string back into the queue would schedule a
    /// turn with nothing in it. The text comes back with the answer so the caller
    /// can say *what* it dropped rather than that something was dropped.
    ///
    /// `None` when no edit was in flight, which is the caller's signal that the
    /// key was never ours.
    pub fn put_back(&mut self, waiting: &mut Vec<String>, text: &str) -> Option<Put> {
        let (at, was) = self.editing.take()?;
        if text.trim().is_empty() {
            // The mark stays where the line was, so the next arrow starts from
            // the hole rather than from the top of a list that just got shorter.
            self.at = Some(at);
            return Some(Put::Dropped(was));
        }
        let at = at.min(waiting.len());
        waiting.insert(at, text.to_string());
        self.at = Some(at);
        Some(Put::Kept(at))
    }

    /// Abandon the edit and put the line back exactly as it was taken.
    ///
    /// The undo half of [`Cursor::take`], and the reason the original text is
    /// held: `Esc` on a half-edited line is "I did not mean to touch this", and
    /// answering it with the half-edited text would be a cancel that still
    /// changed the queue.
    pub fn cancel(&mut self, waiting: &mut Vec<String>) -> Option<usize> {
        let (at, was) = self.editing.take()?;
        let at = at.min(waiting.len());
        waiting.insert(at, was);
        self.at = Some(at);
        Some(at)
    }

    /// Forget the mark and any edit, because the turn they were about has ended.
    ///
    /// **It does not put a lapsed edit back.** The line is in the composer, where
    /// the operator can see it and send it; re-queueing it here would put a
    /// second copy behind the drain that is about to start, and both of them
    /// would run. What this clears is the *position*, which is the part that goes
    /// stale — a slot remembered across a drain and a new turn's queue points at
    /// somebody else's line.
    pub fn lapsed(&mut self) {
        *self = Self::default();
    }
}

/// The rows this surface would draw for `waiting` with nothing marked.
///
/// The surface as it is until an operator touches it, which is nearly always:
/// the queue opens itself, and most of the time it is read rather than driven.
/// [`rows_for`] is the same rows with a mark on one of them.
pub fn rows(waiting: &[String], width: u16, room: u16, glyphs: &Glyphs) -> Vec<String> {
    rows_for(waiting, None, width, room, glyphs)
}

/// The rows this surface would draw for `waiting`, oldest first, at most `room`
/// of them and none wider than `width`, with `selected` marked.
///
/// **The oldest are the ones kept.** They are the ones that run next, so a
/// window that followed the newest would show the operator the part of the queue
/// they are furthest from seeing happen. What did not fit is a count on the last
/// row rather than silence: a list that stops at the bottom of its rows and says
/// nothing is a queue an operator reads as three long when it is nine long, and
/// [`Glyphs::elision`] exists for exactly this — *lines are missing here*,
/// always followed by how many.
///
/// **A mark moves the window, and it has to.** A selection the operator cannot
/// see is worse than no selection at all — it is a list where the arrows appear
/// to do nothing and then something moves. So the rows scroll to keep the marked
/// line on screen, exactly as [`crate::fleet::Fleet::render`] does, and the
/// numbers stay absolute while they scroll: the number is the run order, and a
/// window that renumbered itself from one would be telling the operator that the
/// line they marked runs first.
///
/// The marker column costs two cells on *every* row, or none on any of them.
/// [`Glyphs::marker`] is two cells wide in both sets precisely so it can be
/// swapped for two spaces without moving the text beside it; a column drawn only
/// on the marked row would shift that one row out of line with its neighbours,
/// which reads as the row having changed rather than as it having been chosen.
/// With nothing marked there is no column at all, so a surface nobody has
/// touched is drawn to the same width it always was.
///
/// Separate from [`render`] so the arithmetic can be asserted without a frame.
pub fn rows_for(
    waiting: &[String],
    selected: Option<usize>,
    width: u16,
    room: u16,
    glyphs: &Glyphs,
) -> Vec<String> {
    let room = usize::from(room);
    if room == 0 || waiting.is_empty() {
        return Vec::new();
    }
    // Clamped here as well as in [`Cursor::selection`], because this is a public
    // function over a slice and the index is used to *subscript* it below. One
    // caller reading a mark that outlived its line would be a panic in a
    // renderer, which is a crash in the middle of somebody's turn.
    let selected = selected.map(|at| at.min(waiting.len() - 1));
    let mark = |at: usize| match selected {
        Some(on) if on == at => glyphs.marker,
        Some(_) => "  ",
        None => "",
    };
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
        // **The one row goes to the line the operator is working on, and with
        // nothing marked that is the line that runs next.** This is where a
        // selection at one row either reads or does not exist: an arrow that
        // moved a mark onto a line the single row never shows is an arrow that
        // did nothing an operator could see. So the row follows the mark, the
        // number on it says which line of the queue this is, and the count on
        // the end goes on saying how many others are waiting — which is the same
        // sentence it always said, because it was never a count of what is
        // *below* the row, only of what is not on it.
        let at = selected.unwrap_or(0);
        let rest = waiting.len() - 1;
        return vec![fit(
            &format!(
                "{}{}. {} {} {rest} more",
                mark(at),
                at + 1,
                one_line(&waiting[at]),
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
    // The window ends on the marked line when the queue is longer than the rows,
    // and never runs past the end of the list. With nothing marked it starts at
    // the top, which is where it started before there was anything to mark.
    let first = match selected {
        Some(at) if waiting.len() > shown => {
            at.saturating_sub(shown - 1).min(waiting.len() - shown)
        }
        _ => 0,
    };
    let mut out: Vec<String> = waiting
        .iter()
        .enumerate()
        .skip(first)
        .take(shown)
        // Numbered rather than headed. A header row would spend the surface's
        // only row on a terminal that can spare one, and would repeat a sentence
        // the footer notice has already said; the position says both what the
        // rows are and which order they run in, on every row, for no rows at
        // all.
        .map(|(at, prompt)| {
            fit(
                &format!("{}{}. {}", mark(at), at + 1, one_line(prompt)),
                width,
                glyphs,
            )
        })
        .collect();
    // **What is BELOW the window, not what is outside it.** The row sits under
    // the last one drawn, so a count of everything unshown reads as a count of
    // what follows — and once the mark has scrolled the window down, most of what
    // is unshown is above. Nine queued, four rows, marked on the eighth: the rows
    // read `6.` `7.` `8.` and the old count said six more, of which five were
    // behind the operator. It was right on the first draw, when nothing is marked
    // and the window starts at the top, and wrong from the first arrow — which is
    // the shape this release has been hunting.
    //
    // What is above needs no row of its own: the numbers are absolute, so a
    // window opening at `6.` says so by saying `6.`.
    let below = waiting.len() - (first + shown);
    if below > 0 {
        out.push(fit(
            &format!(
                "{}{} {below} more",
                if selected.is_some() { "  " } else { "" },
                glyphs.elision
            ),
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
/// with them for an attention it has no claim on. The marked row is the one
/// exception, in [`Tone::Accent`] — the tone the theme names for a selection —
/// because a mark that is only a glyph is a mark an operator has to hunt for on
/// a row that is the same colour as every other row.
///
/// **No cursor, and this is where it parts company with the fleet view.**
/// [`crate::fleet::Fleet::render`] parks the terminal's cursor on its marked row,
/// and can, because it is drawn *over* the composer and there is no prompt
/// underneath to take the caret away from. This surface draws above a composer
/// that is still visible and still taking typing — that is the refusal at the top
/// of this module — so the caret stays in the prompt where the next line is being
/// written, and the mark is carried by the glyph and the tone instead.
pub fn render(
    waiting: &[String],
    selected: Option<usize>,
    frame: &mut Frame,
    area: Rect,
    theme: &Theme,
) {
    let lines: Vec<Line<'static>> =
        rows_for(waiting, selected, area.width, area.height, &theme.glyphs)
            .into_iter()
            // Read back off the row rather than recomputed from `selected`: the
            // window that decides *which* rows these are lives in `rows_for`, and a
            // second copy of that arithmetic here is the copy that would drift and
            // paint the wrong row.
            .map(|row| {
                let tone = if row.starts_with(theme.glyphs.marker) {
                    Tone::Accent
                } else {
                    Tone::Muted
                };
                Line::from(Span::styled(row, theme.style(tone)))
            })
            .collect();
    if lines.is_empty() {
        return;
    }
    // Unwrapped on purpose: `rows` has already cut every line to the width, and
    // a wrap would turn one queued prompt into two rows inside a rectangle
    // measured in prompts.
    frame.render_widget(Paragraph::new(lines), area);
}

/// What happened when a queue was handed to a running turn.
///
/// Returned by [`crate::app::App::deliver_queued`], which is where the loop and
/// the transcript records live. This type exists so the driver has something to
/// report from rather than a count it computed itself — `/steer` and the
/// automatic drain say different sentences about the same outcome, and both have
/// to agree about what the outcome was.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Delivered {
    /// How many messages reached the turn, in the order they were typed.
    pub sent: usize,
    /// Why the rest did not, if delivery stopped.
    ///
    /// **A refusal is not a count.** When this is set the message that failed has
    /// been put back at the front of the queue, so it is still the operator's, and
    /// a surface that reported `sent` alongside it would be claiming success on
    /// the one path where there was none.
    pub refused: Option<String>,
}
