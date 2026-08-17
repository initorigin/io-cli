//! Undoing the last turn — its files, what it remembered, and where the
//! conversation is pointing.
//!
//! This module is pure logic over `Store`, `Session` and `Workspace`. It draws
//! nothing and it reads no keyboard, so a test can drive a whole undo without a
//! terminal being involved in the asserting. The interface's job is to render
//! [`Undone`]; deciding what happened is this module's, and the two are kept
//! apart because every fact worth asserting about an undo is a fact about the
//! store rather than about a screen.
//!
//! Three decisions shape everything here.
//!
//! **The counts come from what the undo returned, never from a second look.**
//! `io_harness::rewind_run` hands back a [`Rewound`](io_harness::Rewound)
//! naming every path it acted on, and this module reports that value. It never
//! lists the workspace afterwards to count what is there. io-harness gives the
//! reason in its own source, on the observer event it emits from the same value:
//! *a number re-read from the store would be true whether or not the rewind
//! happened.* A workspace listed after the fact says "three files exist", which
//! is equally true when nothing was undone at all — so a report built that way
//! is green for the one failure it exists to catch.
//!
//! **A verdict that changed nothing is reported as a decline, not as a
//! success.** `Rewound::files` carries one verdict per path, and two of the four
//! mean the file is exactly as the run left it. Those go in
//! [`Undone::declined`] with the harness's own reason attached, never in
//! [`Undone::restored`]. An operator who is told "restored" about a file that
//! still holds the agent's version has been told the opposite of what happened,
//! and would have no reason to reach for their own backup.
//!
//! **The head moves back with the files, including for a session's only turn.**
//! An undo that puts the files back and leaves the conversation pointing at the
//! turn it just undid is a half-undo that looks complete: the next turn is
//! assembled from a history that still contains the prompt whose work is gone.
//! [`last_turn`] moves the head to the undone turn's parent, and the
//! single-turn case — where the parent is `None` — is the one that needs the
//! explanation in [`last_turn`]'s own documentation.

use crate::glyphs::Glyphs;
use crate::theme::Tone;
use io_harness::tools::Workspace;
use io_harness::{rewind_run, Error, Rewind, Session, Store};

/// What the armed prompt says before anything is undone.
///
/// Deliberately only names the turn. The obvious richer preview — "this will
/// put back four files" — is not available: the set of paths a run recorded a
/// restore point for lives behind `Store`'s crate-private snapshot queries, and
/// the only public way to learn it is to perform the rewind and read
/// `Rewound::files`. A preview that guessed the number by listing the workspace
/// would be the exact defect this module's first rule forbids, one step earlier.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Preview {
    /// The turn that would be undone.
    pub turn_id: i64,
    /// The run that served it, and therefore the run whose effects would go.
    pub run_id: i64,
    /// The prompt, in the operator's own words. The armed prompt quotes this so
    /// the operator confirms the thing they meant rather than a turn number.
    pub prompt: String,
}

/// What one undo did, in the words the interface will print.
///
/// Every field is derived from the single [`Rewound`](io_harness::Rewound) the
/// harness returned, so the counts on screen and the work that happened cannot
/// disagree — there is only one of them.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Undone {
    /// The prompt of the turn that was undone, so the report names what went
    /// rather than an id the operator never saw.
    pub prompt: String,
    /// The paths this actually changed.
    ///
    /// A file the run created is in here too, because "the way it was" for a
    /// file that did not exist is not existing — deleting it *is* putting it
    /// back, and it is a path the operator will find gone. The alternative,
    /// leaving created files out, would report a rewind that emptied a directory
    /// as having touched nothing.
    pub restored: Vec<String>,
    /// The paths this left exactly as the run left them, each with the harness's
    /// own reason.
    ///
    /// This is the half of the report an operator acts on. A decline means the
    /// agent's version is still on disk and this product cannot remove it, so
    /// the reason — "not valid UTF-8", "over the 1 MiB snapshot cap" — is
    /// carried through verbatim rather than summarised into a count.
    pub declined: Vec<(String, String)>,
    /// How many memory keys were put back to an earlier value.
    pub memory_restored: usize,
    /// How many memory keys the run had invented, and which are now gone.
    pub memory_removed: usize,
    /// How many queued children were dropped.
    pub queue_cleared: usize,
    /// Where the conversation head now is. `None` means the session is back to
    /// having said nothing.
    pub head: Option<i64>,
}

/// How much of a turn's prompt a sentence quotes back.
///
/// Fixed rather than scaled to the terminal, and deliberately: these lines are
/// committed into scrollback, which wraps, so a wider terminal buys fewer rows
/// rather than more quotation. Forty characters is enough to recognise a prompt
/// you typed minutes ago, which is all the quotation is for — the operator is
/// being asked to confirm a thing, not to re-read it.
const QUOTED_PROMPT: usize = 40;

/// What undoing the last turn would be about, or `None` when there is nothing
/// to undo.
///
/// `None` is returned rather than an error for a store that will not answer, and
/// that is the right trade for this one caller: this exists to decide whether to
/// arm a confirmation prompt, and a store that cannot name the turn cannot
/// promise anything about undoing it either. The failure is not swallowed
/// silently in any way that matters — [`last_turn`] runs against the same store a
/// keystroke later and returns its error in full.
pub fn preview(session: &Session, store: &Store) -> Option<Preview> {
    let turn = store.session_turn(session.head()?).ok()??;
    Some(Preview {
        turn_id: turn.id,
        run_id: turn.run_id,
        prompt: turn.prompt,
    })
}

/// Undo the last turn: its files, its memory, its queued children, and the
/// conversation head.
///
/// `Ok(None)` for a session that has taken no turns. That is not an error: an
/// operator who presses undo on a fresh session has asked for nothing to happen,
/// and nothing happening is the correct answer rather than a failure to report.
///
/// **Nothing in the trace is deleted.** The turn, its steps, its events and its
/// ledger rows all stay, and the harness writes the undo down as a row of its own
/// — readable through `Store::rewinds`. This function makes no attempt to remove
/// turns. The spend happened; a history that erased it would make "this agent has
/// tried this three times" unanswerable, and would make the ledger disagree with
/// the invoice.
///
/// # Why the head is moved through the store and the session re-opened
///
/// The obvious way to move the head back is `Session::branch_from`, which is what
/// the interface already uses to leave a turn behind. It cannot express this
/// case. `branch_from` takes the turn to branch *from*, so the one thing it
/// cannot say is "there is no turn to branch from" — and that is precisely the
/// state a session is left in when its only turn is undone. Handing it the
/// undone turn's id would leave the head on the turn whose work has just been
/// deleted, which is the half-undo this module exists to avoid.
///
/// So the head is written directly, `store.set_session_head(id, parent)?`, where
/// `parent` is the undone turn's `parent_turn_id` and is `None` for a first
/// turn. The session is then re-read with `Session::reopen(store, id)?` rather
/// than being patched in place, because `Session` caches its head in a private
/// field that no public setter reaches: a session left holding the old value
/// would keep answering `head()` with a turn the store no longer points at, and
/// would parent the *next* turn onto the undone one. Re-opening makes the store
/// the single source of that answer.
///
/// This matters most for the one-turn session, which is also the case nobody
/// tries by hand: a two-turn undo still leaves a plausible-looking head, so a
/// build that got this wrong would pass every test written against a longer
/// conversation.
pub fn last_turn(session: &mut Session, store: &Store) -> Result<Option<Undone>, Error> {
    let Some(head) = session.head() else {
        return Ok(None);
    };
    // A head pointing at a turn the store does not hold is the same answer as no
    // head at all: there is nothing nameable to undo. Reported as `None` rather
    // than as an error for the same reason the empty session is.
    let Some(turn) = store.session_turn(head)? else {
        return Ok(None);
    };

    // Rooted at the session's own workspace, which the store owns — the same root
    // the run wrote through, so a restore lands where the edit did. The default
    // policy is permissive, and deliberately so: every path in this run's
    // snapshots is a path whose write already passed the operator's gate, so
    // putting it back cannot reach anywhere the run could not.
    let workspace = Workspace::new(session.root());
    let rewound = rewind_run(&workspace, store, turn.run_id)?;

    let mut restored = Vec::new();
    let mut declined = Vec::new();
    for (path, verdict) in rewound.files {
        match verdict {
            // Put back, byte for byte. The `Wrote` inside says whether the file
            // was already in that state; it is dropped here because the operator's
            // question is "is my version back", and it is either way.
            Rewind::Restored(_) => restored.push(path),
            // The run created it, so putting it back meant deleting it.
            Rewind::Removed => restored.push(path),
            // The run rewrote it and the previous contents were deliberately not
            // kept. **Nothing was changed** — the file is exactly as the run left
            // it, and the reason is the harness's own words.
            Rewind::NotKept(why) => declined.push((path, why)),
            // Unreachable through `rewind_run`, which only visits paths that have
            // a snapshot row, but reported honestly rather than folded into
            // `restored` if it ever arrives. Collapsing it would tell an operator
            // a file came back when nothing touched it.
            Rewind::NotRecorded => {
                declined.push((path, "this run recorded no restore point".to_string()))
            }
        }
    }

    let id = session.id();
    store.set_session_head(id, turn.parent_turn_id)?;
    *session = Session::reopen(store, id)?;

    Ok(Some(Undone {
        prompt: turn.prompt,
        restored,
        declined,
        // Lengths of the vectors the rewind returned. Never `store.memory_get`
        // in a loop afterwards: a key that reads back at its old value is equally
        // consistent with the restore having happened and with it never having
        // been written in the first place.
        memory_restored: rewound.memory_restored.len(),
        memory_removed: rewound.memory_removed.len(),
        queue_cleared: rewound.queue_cleared.len(),
        head: session.head(),
    }))
}

/// What the first `Esc` says, before anything has been undone.
///
/// It quotes the turn's own prompt, because that is what makes a confirmation a
/// confirmation of something rather than of a keystroke. It deliberately does
/// **not** name a file count: the set of paths a run recorded a restore point for
/// lives behind the store's crate-private snapshot queries, so the only way to
/// produce a number here would be to list the workspace — which is precisely the
/// recount this release forbids for the report, one keystroke earlier. A number
/// that might be wrong is worse than no number in the sentence that asks
/// permission.
///
/// **It does warn about the one thing an operator can lose.** `rewind_run`
/// restores each path from the snapshot taken before the run's first write to it,
/// and it does not compare that against what is on disk now — so an edit the
/// operator made by hand *after* the turn is overwritten without a word. io-cli
/// cannot detect that: the snapshot is not readable from here. What it can do is
/// say so before the second keystroke, which is why this sentence names the
/// consequence rather than only the action. Anything else would be a confirmation
/// prompt that concealed the risk it existed to disclose.
///
/// **The disclosure sits at the end of the sentence, and that is safe here for a
/// reason worth writing down rather than assuming.** Twice this product has
/// shipped a row whose load-bearing half was the half that got cut, so the
/// instinct is to move the warning to the front. It is not needed: this line is
/// *committed* — it goes through `Screen::commit` into the terminal's own
/// scrollback, which wraps, and `tests/narrow.rs` proves that a committed line
/// wider than the terminal wraps rather than truncating. The rows that were cut
/// before were rows *drawn in the viewport*, where there is no second line to
/// wrap onto. A test pins this so the distinction is checked and not merely
/// argued.
pub fn armed_line(about: &Preview, glyphs: &Glyphs) -> String {
    format!(
        "undo {open}{}{close}? Esc again puts its files back as they were BEFORE \
         that turn {dash} anything you have edited by hand since is lost. Any \
         other key cancels.",
        crate::picker::fit(about.prompt.trim(), QUOTED_PROMPT, glyphs),
        open = glyphs.quote_open,
        close = glyphs.quote_close,
        dash = glyphs.dash,
    )
}

/// The report after a rewind, as tone-carrying lines.
///
/// Declined paths come **first** and carry a tone of their own, because they are
/// the half an operator has to act on: a decline means the agent's version is
/// still on disk. A report that led with two restorations and mentioned the
/// decline afterwards would read as a success with a footnote.
///
/// Every number here is read off the `Undone` the harness's own return value
/// produced. Nothing lists the workspace to check.
pub fn undone_lines(undone: &Undone, glyphs: &Glyphs) -> Vec<(Tone, String)> {
    let (open, close, dash) = (glyphs.quote_open, glyphs.quote_close, glyphs.dash);
    let mut lines = Vec::new();
    for (path, why) in &undone.declined {
        lines.push((
            Tone::Warning,
            format!("left as the turn left it: {path} {dash} {why}"),
        ));
    }
    lines.push((
        Tone::Success,
        match undone.restored.len() {
            0 => "no file was put back".to_string(),
            1 => format!("put back {}", undone.restored[0]),
            n => format!("put back {n} files: {}", undone.restored.join(", ")),
        },
    ));
    if undone.memory_restored > 0 || undone.memory_removed > 0 {
        lines.push((
            Tone::Muted,
            format!(
                "{} note{} put back, {} removed",
                undone.memory_restored,
                if undone.memory_restored == 1 { "" } else { "s" },
                undone.memory_removed
            ),
        ));
    }
    if undone.queue_cleared > 0 {
        lines.push((
            Tone::Muted,
            format!(
                "{} queued child{} cleared",
                undone.queue_cleared,
                if undone.queue_cleared == 1 { "" } else { "ren" }
            ),
        ));
    }
    lines.push((
        Tone::Muted,
        // The report names the turn it undid, which the armed line also did —
        // but the armed line is a question that scrolls past, and this is the
        // record that stays in the scrollback. A reader coming back to it an hour
        // later should not have to scroll up one line to learn what went.
        match undone.head {
            Some(_) => format!(
                "{open}{}{close} is undone; the conversation continues from the turn before it",
                crate::picker::fit(undone.prompt.trim(), QUOTED_PROMPT, glyphs)
            ),
            // Said in words rather than left to be inferred from silence: this is
            // the case `Session::branch_from` cannot express, and the one nobody
            // tries by hand.
            None => format!(
                "{open}{}{close} is undone; this conversation is back to having said nothing",
                crate::picker::fit(undone.prompt.trim(), QUOTED_PROMPT, glyphs)
            ),
        },
    ));
    lines
}
