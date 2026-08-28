//! Putting work back at the granularity the operator actually meant.
//!
//! [`crate::rewind`] has undone a whole run since 0.4.0, and for three years of
//! releases that was the only instrument. It is the wrong size for most
//! mistakes: *this one file went wrong* and *that step should not have happened*
//! are the two things an operator reaches for, and an undo whose cost is larger
//! than the mistake it fixes is an undo nobody uses.
//!
//! Both granularities are public in io-harness and neither had ever been called
//! from this crate — [`io_harness::rewind`] for one path, and
//! [`io_harness::rewind_step_observed`] for one step's recorded diff.
//!
//! # The whole-run case changed too, and that is what makes two events reachable
//!
//! `src/rewind.rs` called the plain `rewind_run`, and **`EventKind::Rewound` is
//! emitted only inside `rewind_run_observed`** (`run.rs:804`). So in this
//! product's entire history that event has never fired once. The same is true of
//! `EventKind::Reverted`, which only `rewind_step_observed` emits (`run.rs:1011`).
//!
//! **What that buys is not a line on screen, and an earlier draft of this file
//! claimed it was.** Both kinds are `Disposition::Silent` in `crate::triage` and
//! route to io-cli's own rewind summary — written from the value the call
//! returned, which `crate::rewind` explains at length — so `src/events.rs` has
//! no arm for either and correctly draws nothing. What the observed forms buy is
//! that the events now **exist**: they reach `[[hook]]` observers, the
//! `io exec --json` stream and the durable trace, none of which they had ever
//! reached. Recorded because the adversarial review found the overclaim.
//!
//! # Three traps, and the middle one will bite an operator
//!
//! **1. Reverting a step is order-sensitive, and io-harness will not guess.**
//! Reverse-applying a hunk while a *later* step's change still sits on top of it
//! finds context that has moved, and the honest answer is
//! [`io_harness::Reverted::Stale`] with the file untouched —
//! never a fuzzy match that quietly corrupts it. Newest step first is the order
//! that works, and [`step_advice`] is what says so on screen when a stale answer
//! comes back, because "nothing happened" without that sentence reads as a bug.
//!
//! **2. A restore does not know about an edit made afterwards.** io-harness puts
//! the file back from the snapshot taken before the run first wrote it, and does
//! not compare that against what is on disk now. An edit the operator made by
//! hand after the turn is overwritten without a word, and io-cli cannot detect it
//! — the snapshot is not readable from here. `src/rewind.rs` already discloses
//! this before its second keystroke; [`confirm_file`] does the same.
//!
//! **3. The four answers are four answers.** [`io_harness::Rewind`] distinguishes
//! `Restored`, `Removed`, `NotKept(why)` and `NotRecorded`, and they mean
//! different things: the first two changed the workspace, the last two changed
//! nothing at all. Collapsing `NotKept` and `NotRecorded` into one sentence would
//! tell an operator "nothing to undo" about a file the run very much wrote, whose
//! previous contents were simply over the snapshot cap.
//!
//! # What the policy is, and whose it is
//!
//! io-harness's. [`io_harness::rewind`] restores through `Workspace::write_file`,
//! so the same path policy a write obeys applies; a **removal** asks
//! `Workspace::check_path` and refuses anything that is not an outright
//! `Effect::Allow`, because a deletion is not content a human can inspect
//! afterwards. Nothing here re-implements either check, and nothing here softens
//! a refusal.

use io_harness::tools::Workspace;
use io_harness::{Error, Observer, Reverted, Rewind, Store};

/// Which granularity an undo was asked for.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Grain {
    /// One named path, from this run's snapshot.
    File(String),
    /// One step's recorded diff, reverse-applied.
    Step(u32),
    /// Everything the run did.
    Run,
}

// `Grain` carries no `label()`. The confirmations build their own sentences —
// `confirm_file` and `confirm_step` — and the run arm never renders a label at
// all, so a method that formatted one would be reachable only from a test.

/// Put one file back, from the snapshot taken before this run first wrote it.
///
/// The answer is io-harness's own [`Rewind`], returned whole rather than reduced
/// to a boolean: see trap 3. `Restored` and `Removed` changed the workspace;
/// `NotKept` and `NotRecorded` changed nothing and say why.
pub fn one_file(
    workspace: &Workspace,
    store: &Store,
    run_id: i64,
    path: &str,
) -> Result<Rewind, Error> {
    io_harness::rewind(workspace, store, run_id, path)
}

/// Undo one step's recorded diff, announcing it.
///
/// **The observed form, so `EventKind::Reverted` fires.** One entry per path the
/// step wrote, in the order it wrote them; a step that wrote nothing comes back
/// empty and is not an error — asking to undo a step that only read files is a
/// reasonable question with a boring answer.
pub fn one_step(
    workspace: &Workspace,
    store: &Store,
    run_id: i64,
    step: u32,
    observer: &dyn Observer,
) -> Result<Vec<(String, Reverted)>, Error> {
    io_harness::rewind_step_observed(workspace, store, run_id, step, observer)
}

// **There is deliberately no `whole_run` here.** The whole-turn undo is
// `crate::rewind::last_turn`, which does what this module cannot: it moves the
// conversation head back, by compare-and-swap, and re-opens the session. A thin
// wrapper over `rewind_run_observed` beside it would be a second entry point to
// one act, and the first draft of this module shipped exactly that — public,
// tested, and called by nothing. This codebase has shipped tested-but-unreachable
// surfaces in 0.20.0, 0.21.0 and 0.26.0; the adversarial review caught this one
// before it became the fourth.

/// What one file's answer says, in the operator's words.
///
/// Four sentences for four answers. The two that changed nothing say so first,
/// because that is the fact an operator has to notice.
pub fn said(path: &str, answer: &Rewind) -> String {
    match answer {
        Rewind::Restored(_) => format!("{path} is back as it was before the run"),
        Rewind::Removed => format!("{path} is gone — the run had created it"),
        Rewind::NotKept(why) => {
            format!("{path} is unchanged: its previous contents were not kept ({why})")
        }
        Rewind::NotRecorded => {
            format!("{path} is unchanged: this run recorded no restore point for it")
        }
    }
}

/// What one step's answer says, per path.
pub fn step_said(path: &str, answer: &Reverted) -> String {
    match answer {
        Reverted::Applied(_) => format!("{path} is back to what it was before that step"),
        Reverted::Stale(why) => format!("{path} is unchanged: {why}"),
        // **`NoHunk` is the third variant and it is not exotic.** io-harness names
        // it for a row written before hunks were kept, and for a file whose
        // previous contents were not kept at all — over the snapshot cap, or not
        // text. That is the same class `Rewind::NotKept` gets a written sentence
        // for one function up, and it deserves one here rather than a Rust debug
        // string with quotes in it. The first version of this function let it
        // fall to a `{other:?}` wildcard, and the adversarial review found it.
        Reverted::NoHunk(why) => format!(
            "{path} is unchanged: no diff was stored for that step ({why}) — \
             `/undo {path}` puts the whole file back instead"
        ),
        // `Reverted` may gain a fourth variant in a later io-harness. Reporting
        // the conservative thing — nothing changed — is the arm that cannot
        // mislead, because the alternative is claiming a restore this build did
        // not understand.
        _ => format!("{path} is unchanged: this build does not know that answer"),
    }
}

/// The sentence to add when a step's revert came back stale.
///
/// Trap 1, said out loud. A stale answer with no explanation reads as a bug —
/// the operator asked for an undo, was told nothing happened, and has no way to
/// know that undoing the *newer* step first is what makes this one apply.
pub fn step_advice(answers: &[(String, Reverted)]) -> Option<String> {
    answers
        .iter()
        .any(|(_, answer)| matches!(answer, Reverted::Stale(_)))
        .then(|| {
            "a later step is still standing on top of this one — reverting is \
             order-sensitive, so undo the newest step first and this one applies"
                .to_string()
        })
}

/// The confirmation for putting one file back.
///
/// Row 0 declines, like every confirmation this release adds. The disclosure is
/// trap 2 and it is in the row that acts, because it is what the operator is
/// agreeing to rather than a caveat about what they are declining.
pub fn confirm_file(path: &str) -> (String, Vec<crate::picker::Row>) {
    (
        format!("Put {path} back as it was before this run?"),
        vec![
            crate::picker::Row::with_detail(crate::store::LEAVE_IT, "the file is left as it is"),
            crate::picker::Row::with_detail(
                format!("put {path} back"),
                "any edit you made by hand after the turn is overwritten",
            ),
        ],
    )
}

/// The confirmation for undoing one step.
pub fn confirm_step(step: u32) -> (String, Vec<crate::picker::Row>) {
    (
        format!("Undo what step {step} wrote?"),
        vec![
            crate::picker::Row::with_detail(crate::store::LEAVE_IT, "nothing is changed"),
            crate::picker::Row::with_detail(
                format!("undo step {step}"),
                "reverse-applies that step's diff; a newer step on the same lines \
                 makes it stale and nothing changes",
            ),
        ],
    )
}
