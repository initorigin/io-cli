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

use crate::picker::{fit, fit_left, Row};

/// How many sessions `/resume` offers.
///
/// A bound before it is a saving. Pickers do not filter as you type until 0.7.0,
/// so a list longer than a screen is a list nobody can reach the bottom of;
/// twenty rows that arrive at once beat four hundred an arrow key has to walk.
pub const MAX_SESSIONS: usize = 20;

/// How many run ids the walk will look at before giving up.
///
/// The other half of the bound, and the one that matters for a long-lived store:
/// a session's runs are contiguous only if nothing else ran in between, so twenty
/// sessions can sit behind far more than twenty runs. Five hundred point queries
/// on an indexed column is the ceiling this release accepts.
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
        // The bound is charged **only when a session that is not already listed
        // turns up**, and never merely on reaching the count. Checking it at the
        // top of the loop instead is wrong in a way that is easy to miss and
        // impossible to spot from the screen: twenty sessions in which any one
        // has run twice produce twenty-one runs, so the walk takes one more step
        // after the list is full, finds a session it already has, and reports
        // that older sessions are hidden when none are. A note that cries wolf
        // is worse than no note, because the note is the whole of F2.
        if ids.len() >= MAX_SESSIONS {
            cut = true;
            break;
        }
        ids.push(turn.session_id);
    }

    let mut out = Vec::with_capacity(ids.len());
    for id in ids {
        // No query at all for a row that is not shown — which is the whole of
        // what the bound above buys, and why it is applied before this loop.
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
pub fn rows(recent: &[Recent], width: u16) -> Vec<Row> {
    // A third rather than a half. The label is the prompt, which is unbounded, and
    // every column it takes is one the workspace cannot have.
    let room = ((width as usize) / 3).max(12);
    recent
        .iter()
        .map(|session| {
            let label = fit(&session.prompt, room);
            // The picker's own arithmetic, mirrored: two cells of marker, the
            // label, two cells of gap. Mirrored rather than guessed, because a
            // budget that is one cell out puts the ellipsis back.
            let mut left = (width as usize)
                .saturating_sub(4)
                .saturating_sub(label.chars().count());

            let path = fit_left(&session.root, ROOT_ROOM.min(left));
            left = left.saturating_sub(path.chars().count());
            let mut detail = path;

            let turns = format!(
                " · {} turn{}",
                session.turns,
                if session.turns == 1 { "" } else { "s" }
            );
            if turns.chars().count() <= left {
                left -= turns.chars().count();
                detail.push_str(&turns);
            }

            let at = format!(" · {}", session.at);
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
pub fn turn_rows(turns: &[io_harness::Turn], width: u16) -> Vec<Row> {
    let room = ((width as usize) * 2 / 3).max(12);
    turns
        .iter()
        .enumerate()
        .map(|(index, turn)| {
            Row::with_detail(
                fit(turn.prompt.trim(), room),
                format!("turn {} · {}", index + 1, stamp(&turn.created_at)),
            )
        })
        .collect()
}

/// The line that says the list was cut, or `None` when it was not.
///
/// A silently truncated list reads as a complete one. This is the sentence that
/// stops it, and it names the size of what is on screen rather than apologising
/// for it.
///
/// **The number comes from `shown`, never from [`MAX_SESSIONS`].** There are two
/// bounds and they cut to different sizes: the session bound leaves exactly
/// twenty rows, while the run bound can leave far fewer — a store in which one
/// workspace has run five hundred turns yields a single row, and a note quoting
/// the constant would tell that operator they were looking at twenty sessions
/// while one was on screen. The note is the whole of this behaviour's honesty, so
/// it may not be the part that is wrong.
pub fn cut_note(cut: bool, shown: usize) -> Option<String> {
    cut.then(|| format!("the {shown} most recent; older sessions are not listed"))
}
