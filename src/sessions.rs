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

use io_harness::Store;

use crate::glyphs::Glyphs;
use crate::picker::{fit, fit_left, Row};

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
    let mut ids: Vec<i64> = Vec::new();
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
        if ids.contains(&turn.session_id) {
            continue;
        }
        ids.push(turn.session_id);
    }

    let mut out = Vec::with_capacity(ids.len());
    for id in ids {
        // Three queries per row, and none at all for a run the walk never
        // reached — which is what splitting the walk from this loop buys, and why
        // the scan ceiling is charged above rather than here.
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
pub fn rows(recent: &[Recent], width: u16, glyphs: &Glyphs) -> Vec<Row> {
    // A third rather than a half. The label is the prompt, which is unbounded, and
    // every column it takes is one the workspace cannot have.
    let room = ((width as usize) / 3).max(12);
    let separator = glyphs.separator;
    recent
        .iter()
        .map(|session| {
            let label = fit(&session.prompt, room, glyphs);
            // The picker's own arithmetic, mirrored: two cells of marker, the
            // label, two cells of gap. Mirrored rather than guessed, because a
            // budget that is one cell out puts the ellipsis back. Four in either
            // glyph set — the marker is two cells in both, which is the property
            // that lets this stay a number.
            let mut left = (width as usize)
                .saturating_sub(4)
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

            Row::with_detail(label, detail)
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
