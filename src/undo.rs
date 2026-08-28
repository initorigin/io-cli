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
//! Pointing both at the observed forms is not a stylistic change: it is the
//! difference between an undo that announces itself like every other thing this
//! interface draws and one that happens in silence.
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
use io_harness::{Error, Observer, Reverted, Rewind, Rewound, Store};

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

impl Grain {
    /// What to call it in a sentence.
    pub fn label(&self) -> String {
        match self {
            Grain::File(path) => format!("the file {path}"),
            Grain::Step(step) => format!("step {step}"),
            Grain::Run => "the whole run".to_string(),
        }
    }
}

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

/// Undo everything the run did, announcing it.
///
/// **The observed form, so `EventKind::Rewound` fires — for the first time in
/// this product's history.** `src/rewind.rs` has called the plain `rewind_run`
/// since 0.4.0, which is why the event has an arm in `src/events.rs` that has
/// never been reached.
pub fn whole_run(
    workspace: &Workspace,
    store: &Store,
    run_id: i64,
    observer: &dyn Observer,
) -> Result<Rewound, Error> {
    io_harness::rewind_run_observed(workspace, store, run_id, observer)
}

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
        // `Reverted` is `#[non_exhaustive]`-shaped in spirit — a third and fourth
        // variant already exist — so the wildcard reports the honest conservative
        // thing rather than guessing that something was applied.
        other => format!("{path} is unchanged: {other:?}"),
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
