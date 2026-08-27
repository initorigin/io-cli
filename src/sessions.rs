//! The session list behind `/resume`.
//!
//! **io-harness has no call that enumerates sessions.** Every session accessor
//! takes a `session_id` and nothing hands out the set of them, which is the one
//! part of this release's premise that did not survive being read. What does
//! exist is `Store::runs` — every run id, newest first — and two lookups that
//! walk a run to the turn it served and the session that turn belongs to. Walked
//! in that order and deduplicated, that *is* the session list in recency order,
//! without a sort and without an index.
//!
//! **io-cli writes nothing down to do it.** An index of session ids kept in
//! `[app.io-cli]` was considered and rejected: it is a second answer to a
//! question the store already answers, it is wrong the moment another copy of the
//! binary opens a session, and keeping session state is what this product's first
//! constraint forbids. Everything here is a read, and this module cannot reach
//! the filesystem at all — it takes a `&Store` and nothing else.
//!
//! **Every row also says what its session stopped on.** The walk already knows
//! each session's newest run — it is the run that put that session where it is in
//! the list — and [`crate::resume::pending_for`] turns that id into the state the
//! run is holding: a question, a plan, an interrupted call, a process that died,
//! a turn the operator ended, or nothing at all. That state is the fact `/resume`
//! is chosen *on*, so it is drawn as a [`Row::mark`] rather than folded into the
//! label the matcher ranks or the detail a narrow terminal drops. Reading it is
//! the whole of what this module does with it; deciding anything about it belongs
//! to [`crate::resume`], and nothing here drives.

use io_harness::Store;

use crate::glyphs::Glyphs;
use crate::picker::{fit, fit_left, Row};
use crate::resume::{pending_for, Pending};

/// How many run ids the walk will look at before giving up.
///
/// The only bound left, and it bounds *cost* rather than the length of the list:
/// a session's runs are contiguous only if nothing else ran in between, so a
/// hundred sessions can sit behind far more than a hundred runs. Five hundred
/// point queries on an indexed column is the ceiling this release accepts.
///
/// There was a second bound until 0.7.0 — twenty sessions, and its own comment
/// said it existed because a picker that could not be typed at was a list nobody
/// could reach the bottom of. The picker filters now, so the reason has gone and
/// the bound went with it: `/resume` offers what the walk found.
///
/// **This is not a row count and must never be read as one.** What it leaves is
/// however many distinct sessions those five hundred runs served — one, if a
/// single busy workspace ran them all — which is why [`cut_note`] takes its
/// number from what is on screen.
pub const MAX_RUNS_SCANNED: usize = 500;

/// How much of a workspace path a row keeps before the picker fits it again.
const ROOT_ROOM: usize = 30;

/// The mark on a session holding a question the operator has not answered.
///
/// **Words rather than symbols, and ASCII by rule.** A mark is text and not a
/// colour, so it survives `NO_COLOR`, `--plain` and the ASCII glyph set
/// unchanged — the same rule `crate::commands::PINNED_MARK` and its neighbours
/// are held to, and `tests/glyphs.rs` sweeps for. Those marks are one cell each
/// because they sit on *every* row of their page and a varying width would move
/// the label column; these do not. A finished session carries no mark at all, so
/// a `/resume` list is ragged by construction and there is no column to keep —
/// which buys a mark that says *which* state it is, to an operator who has no
/// legend on screen and is about to choose on it.
pub const QUESTION_MARK: &str = "asks";

/// The mark on a session holding a plan waiting for a verdict.
pub const PLAN_MARK: &str = "plan";

/// The mark on a session whose last run has a tool call nobody has ruled on:
/// io-harness could not establish whether it landed, and will not guess.
pub const RECOVERY_MARK: &str = "tool";

/// The mark on a session whose last run was never closed — the process died
/// mid-loop — and which is therefore resumable from its last committed step.
pub const DIED_MARK: &str = "died";

/// The mark on a session whose last turn the **operator** stopped.
///
/// Not a resume target and drawn as one only over this product's dead body: a
/// cancelled run is recorded `completed`, every io-harness resume entry point
/// short-circuits on it, and a row that offered to continue it would be offering
/// something that cannot happen. [`note`] says so in words and names `/fork`.
pub const ENDED_MARK: &str = "ended";

/// One session, as `/resume` shows it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Recent {
    /// What `Session::reopen` takes.
    pub id: i64,
    /// The workspace the conversation is about.
    pub root: String,
    /// Every turn in the session, including the ones a branch left behind.
    pub turns: usize,
    /// What it was first asked to do.
    pub prompt: String,
    /// When its newest turn was recorded, as the store wrote it.
    pub at: String,
    /// What the session's **newest run** stopped on.
    ///
    /// The newest run and not the newest turn: a turn is closed by the driver
    /// and says nothing about a run that paused, and it is the run that holds the
    /// question, the plan or the open call. Read through
    /// [`crate::resume::pending_for`], which drives nothing — this is a list, and
    /// building it must not answer anything.
    pub pending: Pending,
}

/// Every session in this store that ran a turn, newest first.
///
/// The `bool` says the walk stopped before it ran out of runs — so a caller can
/// say the list was cut rather than letting a truncated list read as a complete
/// one, which is the failure this pair exists to prevent.
///
/// A session that was opened and never used has no run and so does not appear.
/// That is correct rather than a gap: there is nothing in it to resume.
pub fn recent(store: &Store) -> Result<(Vec<Recent>, bool), io_harness::Error> {
    let runs = store.runs()?;
    // Each session paired with the newest run that served it, which costs
    // nothing: `Store::runs` is newest-first, so the run that first mentions a
    // session **is** that session's newest, and the dedupe below is already
    // throwing every older one away. Re-deriving it afterwards would mean a
    // second walk to find a number this one has already had in its hand.
    let mut ids: Vec<(i64, i64)> = Vec::new();
    let mut cut = false;

    for (scanned, run) in runs.iter().enumerate() {
        if scanned >= MAX_RUNS_SCANNED {
            cut = true;
            break;
        }
        let Some(turn_id) = store.turn_for_run(*run)? else {
            continue;
        };
        let Some(turn) = store.session_turn(turn_id)? else {
            continue;
        };
        if ids.iter().any(|(session, _)| *session == turn.session_id) {
            continue;
        }
        ids.push((turn.session_id, *run));
    }

    let mut out = Vec::with_capacity(ids.len());
    for (id, newest_run) in ids {
        // Three queries per row and the state read below, and none at all for a
        // run the walk never reached — which is what splitting the walk from this
        // loop buys, and why the scan ceiling is charged above rather than here.
        let Some(root) = store.session_root(id)? else {
            continue;
        };
        let turns = store.session_turns(id)?;
        let Some(first) = turns.first() else {
            continue;
        };
        out.push(Recent {
            id,
            root,
            turns: turns.len(),
            prompt: first.prompt.clone(),
            at: stamp(&turns[turns.len() - 1].created_at),
            // The read that makes the list worth having: without it every row
            // looks alike, and the session actually waiting on an answer is
            // indistinguishable from the twenty that finished. Charged per **row**
            // rather than per run scanned, so the ceiling above still bounds the
            // walk — and taken on the newest run, which the walk above kept for
            // exactly this.
            pending: pending_for(store, newest_run)?,
        });
    }
    Ok((out, cut))
}

/// The session the row at `index` stands for, or `None` for the cut note.
///
/// One line, in the library, for exactly the reason [`resume`] is one line in the
/// library: `src/main.rs` is `[[bin]] name = "io"`, no integration test links it,
/// and until 0.7.0 this lookup lived inline in a match arm there. `ids.first()`
/// written in its place resumes the wrong session on every choice the operator
/// makes and fails no test at all, because there was no test that could reach it.
///
/// The index is [`crate::picker::Outcome::Chosen`]'s, which addresses the rows the
/// picker was *given* whatever has since been typed into it. So this reads the id
/// list back positionally, and a choice made out of a filtered list resolves the
/// row the operator was looking at rather than the row that happens to sit at that
/// position in what is drawn.
///
/// **`None` is the cut note.** [`cut_note`]'s line is pushed after the session
/// rows, so it is the one index with no id behind it. It was unreachable in
/// practice until this release — it was last, and nothing could move it — but the
/// filter ranks it against the session rows like any other row and can put it
/// under the marker, so the caller has to answer it with a sentence rather than by
/// closing the picker and doing nothing.
pub fn pick(ids: &[i64], index: usize) -> Option<i64> {
    ids.get(index).copied()
}

/// Reopen a session the picker chose.
///
/// One line, and it exists so that line is *testable*. `src/main.rs` is the
/// binary: no integration test links it, so a decision made in a match arm there
/// is a decision nothing can sabotage. And this is the decision F3 is about —
/// `Session::reopen` continues the conversation the chosen row names, while
/// `Session::open` against the same root starts a second one that merely shares a
/// directory. The two compile identically and differ only in the parent of the
/// next turn, which is exactly the kind of difference that ships.
///
/// A wrapper with one caller is usually noise. This one earns its place by moving
/// a claim out of untestable code, and `tests/fork.rs` calls it rather than
/// calling the harness directly, so a swap here fails a test instead of a demo.
pub fn resume(store: &Store, id: i64) -> Result<io_harness::Session, io_harness::Error> {
    io_harness::Session::reopen(store, id)
}

/// The store's own timestamp, cut to the minute.
///
/// A stored string sliced, never a clock read and never a relative age. *Two
/// minutes ago* would need the current time, and `src/main.rs` is the only module
/// in this crate allowed to ask for it — a rule `tests/timing.rs` enforces and
/// which a resume picker is the most tempting thing yet shipped to break.
fn stamp(created_at: &str) -> String {
    let cut: String = created_at.chars().take(16).collect();
    cut.replace('T', " ")
}

/// The mark a session's row carries, or `None` for one that finished.
///
/// **`None` is the whole of how a finished session is reported, and that is
/// deliberate.** A mark saying *fine* would be on almost every row of almost
/// every list, and a mark that is nearly always there is one nobody reads — the
/// same argument [`cut_note`] is built on. The three states an operator has to
/// act on, and the two they have to know about, are the ones that get a mark;
/// everything else is the absence of one.
pub fn mark(pending: &Pending) -> Option<&'static str> {
    match pending {
        Pending::Question { .. } => Some(QUESTION_MARK),
        Pending::Plan { .. } => Some(PLAN_MARK),
        Pending::Recovery { .. } => Some(RECOVERY_MARK),
        Pending::Died { .. } => Some(DIED_MARK),
        Pending::Interrupted => Some(ENDED_MARK),
        Pending::Finished => None,
    }
}

/// The same state as a sentence, for a surface with room for prose.
///
/// A second rendering rather than a longer mark: a picker row has four columns
/// to spare and a status line or a `/resume` confirmation has a line, and the
/// state is worth saying properly wherever there is room to say it. `None`
/// wherever [`mark`] is `None`, so the two agree about what is worth reporting
/// and a finished session is silent on both.
///
/// **The interrupted sentence is the one that has to be read carefully.** A turn
/// the operator stopped cannot be resumed — io-harness records it `completed`
/// and every resume entry point returns the original outcome without driving —
/// so this says who ended it and points at `/fork` from the turn before, which
/// is the neighbouring thing that *does* work. It must never read as an offer to
/// continue.
pub fn note(pending: &Pending) -> Option<String> {
    Some(match pending {
        Pending::Question { question, .. } => {
            format!("waiting on your answer: {question}")
        }
        Pending::Plan { steps, .. } => format!(
            "waiting on your verdict on a plan of {} step{}",
            steps.len(),
            if steps.len() == 1 { "" } else { "s" }
        ),
        Pending::Recovery { tool, .. } => format!(
            "waiting on your decision about an interrupted {tool} call — io-harness \
             cannot tell whether it landed"
        ),
        Pending::Died { last_step } => {
            format!("the process died after step {last_step}; resumable from there")
        }
        Pending::Interrupted => "you ended this turn, and an ended turn cannot be \
             continued — /fork from the turn before it to carry on"
            .to_string(),
        Pending::Finished => return None,
    })
}

/// The picker rows, fitted for a terminal this wide.
///
/// Content precedes metadata, so the label is what the session was asked to do.
/// The workspace comes **first** in the detail because the detail is what the
/// picker shortens from the right: put the load-bearing fact where it cannot be
/// the part that goes. The path itself is shortened from the *left*, since the
/// end of a path is what identifies it and the beginning is the same for every
/// row on the machine.
///
/// **The detail is composed to a budget rather than assembled and then trimmed.**
/// Four facts do not fit in eighty columns, and the version of this function that
/// wrote all four and let the picker cut the overflow produced
/// `…/that/go/on/io-cli · 6 …` — a turn count amputated to a digit and an
/// ellipsis. That is the third time this product has cut a row's load-bearing
/// half, so the rule here is stronger than "order them well": each field is added
/// only if the whole of it fits, in order of how much the operator needs it. A
/// narrow terminal therefore loses the timestamp entirely, which is legible,
/// instead of keeping a fragment of it, which is not.
///
/// **The state goes on [`Row::mark`], and neither in the label nor in the
/// detail.** Not the label, because [`crate::picker::Picker`] ranks the label
/// and nothing else: a state folded into it would give every waiting session the
/// same first characters, under which no query is a prefix of a row and both of
/// `crate::fuzzy`'s top tiers stop being reachable — and it would also let a
/// query for `plan` return the sessions that are *stopped on* a plan rather than
/// the ones that were asked about one. Not the detail, because the detail is the
/// first thing the picker drops when the terminal is narrow, and the state is
/// precisely what an operator on a narrow terminal still has to see: 0.16.0
/// marked a template and a skill in the detail alone and the distinction vanished
/// at the width that needed it most. The mark column is the one that survives
/// both.
pub fn rows(recent: &[Recent], width: u16, glyphs: &Glyphs) -> Vec<Row> {
    // A third rather than a half. The label is the prompt, which is unbounded, and
    // every column it takes is one the workspace cannot have.
    let room = ((width as usize) / 3).max(12);
    let separator = glyphs.separator;
    recent
        .iter()
        .map(|session| {
            let label = fit(&session.prompt, room, glyphs);
            let kind = mark(&session.pending);
            // The picker's own arithmetic, mirrored: two cells of marker, the
            // mark and the space after it on a row that has one, the label, two
            // cells of gap. Mirrored rather than guessed, because a budget that is
            // one cell out puts the ellipsis back. Four in either glyph set — the
            // marker is two cells in both, which is the property that lets that
            // half stay a number; the mark's own width is counted rather than
            // assumed, for the reason `picker::fit` measures the ellipsis.
            let mut left = (width as usize)
                .saturating_sub(4)
                .saturating_sub(kind.map_or(0, |mark| mark.chars().count() + 1))
                .saturating_sub(label.chars().count());

            let path = fit_left(&session.root, ROOT_ROOM.min(left), glyphs);
            left = left.saturating_sub(path.chars().count());
            let mut detail = path;

            let turns = format!(
                "{separator}{} turn{}",
                session.turns,
                if session.turns == 1 { "" } else { "s" }
            );
            if turns.chars().count() <= left {
                left -= turns.chars().count();
                detail.push_str(&turns);
            }

            let at = format!("{separator}{}", session.at);
            if at.chars().count() <= left {
                detail.push_str(&at);
            }

            match kind {
                Some(mark) => Row::marked(mark, label, detail),
                None => Row::with_detail(label, detail),
            }
        })
        .collect()
}

/// The rows `/fork` offers, from the conversation's own path.
///
/// The turns come from [`io_harness::Session::history`], which is the path from
/// the head back to the root rather than the whole tree — so a fork of a fork
/// lists the line the operator is actually on, which is what they mean by *this
/// conversation*. Numbered from one, because a turn id is a database key and
/// nobody counts in database keys.
pub fn turn_rows(turns: &[io_harness::Turn], width: u16, glyphs: &Glyphs) -> Vec<Row> {
    let room = ((width as usize) * 2 / 3).max(12);
    let separator = glyphs.separator;
    turns
        .iter()
        .enumerate()
        .map(|(index, turn)| {
            Row::with_detail(
                fit(turn.prompt.trim(), room, glyphs),
                format!("turn {}{separator}{}", index + 1, stamp(&turn.created_at)),
            )
        })
        .collect()
}

/// The turn the row at `index` stands for: its id, and the number drawn beside it.
///
/// Both from one call, because both are derived from the one index and they used
/// to be derived in two different places — the id in the driver's match arm, the
/// number in the sentence that arm printed. Two readings of one index is two
/// chances to be off by one, and the sentence names the turn the operator is being
/// told they are now continuing from, so a number that disagrees with the branch
/// is a lie about what just happened.
///
/// Numbered from one, matching [`turn_rows`] — which is the only place the two
/// have to agree, and they agree here by being the same arithmetic rather than by
/// two copies of it staying in step.
///
/// `None` for an index past the end. `/fork` carries no cut note, so that is
/// unreachable rather than impossible; the caller says something anyway, because
/// the alternative is a chosen row that closes the picker and branches from
/// nothing.
pub fn pick_turn(ids: &[i64], index: usize) -> Option<(i64, usize)> {
    ids.get(index).map(|id| (*id, index + 1))
}

/// The line that says the list was cut, or `None` when it was not.
///
/// A silently truncated list reads as a complete one. This is the sentence that
/// stops it, and it names the size of what is on screen rather than apologising
/// for it.
///
/// **The number comes from `shown`, never from a constant.** There is one bound
/// left and it counts runs rather than rows: five hundred runs served by a single
/// busy workspace leave one session on screen, so a note quoting
/// [`MAX_RUNS_SCANNED`] would tell that operator they were looking at five hundred
/// sessions while one was there. Losing the session bound has not made a constant
/// safe to quote — it has widened the gap between the ceiling and the row count
/// from *sometimes* to *always*. 0.4.0 paid for this once, when a run-bound cut
/// showing one row claimed twenty. The note is the whole of this behaviour's
/// honesty, so it may not be the part that is wrong.
pub fn cut_note(cut: bool, shown: usize) -> Option<String> {
    cut.then(|| format!("the {shown} most recent; older sessions are not listed"))
}
