//! Putting work back at three granularities, and the two events that proves.
//!
//! **The assertion that matters most here is that an event *arrives*.**
//! `EventKind::Rewound` is emitted only inside `rewind_run_observed`
//! (`run.rs:878`) and `EventKind::Reverted` only inside `rewind_step_observed`
//! (`run.rs:1085`). `src/rewind.rs` called the plain `rewind_run` from 0.4.0
//! until this release, so neither has ever fired in this product's history — and
//! `src/events.rs` has had an arm for one of them the whole time, unreachable.
//!
//! So the gates below collect what an observer was handed. Asserting the
//! rendered sentence would pass just as happily on the unobserved call, which is
//! the defect: the words come from io-cli either way, and only the event says
//! which function was called.
//!
//! **The workspace assertions are on bytes on disk, never on a message.** An
//! undo that reports success and leaves the file alone is the whole failure mode.

mod support;

use std::sync::Mutex;

use io_cli::undo::{
    confirm_file, confirm_step, one_file, one_step, said, step_advice, step_said, Grain,
};
use io_harness::tools::Workspace;
use io_harness::{
    ApproveAll, EventKind, Flow, Observer, Policy, Reverted, Rewind, RunEvent, Session, Store,
};
use support::Scripted;

/// An observer that keeps every event it is handed.
///
/// The only instrument that can tell `rewind_run` from `rewind_run_observed`.
#[derive(Default)]
struct Collected {
    events: Mutex<Vec<EventKind>>,
}

impl Observer for Collected {
    fn event(&self, event: &RunEvent) -> Flow {
        self.events
            .lock()
            .expect("the lock is not poisoned")
            .push(event.kind.clone());
        Flow::Continue
    }
}

impl Collected {
    fn kinds(&self) -> Vec<EventKind> {
        self.events
            .lock()
            .expect("the lock is not poisoned")
            .clone()
    }

    fn saw_rewound(&self) -> bool {
        self.kinds()
            .iter()
            .any(|kind| matches!(kind, EventKind::Rewound { .. }))
    }

    fn saw_reverted(&self) -> Option<u32> {
        self.kinds().iter().find_map(|kind| match kind {
            EventKind::Reverted { undid_step, .. } => Some(*undid_step),
            _ => None,
        })
    }
}

/// A temporary workspace, a store, and a session over the two.
///
/// **Every run here is a real turn.** A restore point is written by io-harness's
/// own run loop through `Store::record_snapshot`, which is `pub(crate)` — so a
/// test cannot seed one, and a fixture that inserted rows by hand would be
/// testing a shape production does not produce. This is the same fixture
/// `tests/rewind.rs` has used since 0.4.0, for the same reason.
struct Fixture {
    dir: tempfile::TempDir,
    store: Store,
    session: Session,
}

impl Fixture {
    fn new() -> Self {
        let dir = tempfile::tempdir().expect("a temp dir");
        let store = Store::memory().expect("an in-memory store");
        let session = Session::open(&store, dir.path()).expect("a session");
        Self {
            dir,
            store,
            session,
        }
    }

    /// One turn whose agent writes every one of `files`, in one step.
    async fn turn_writing(&mut self, prompt: &str, files: &[(&str, &str)]) -> i64 {
        self.drive(prompt, Scripted::writing(files)).await
    }

    /// One turn that takes a step per slice, so the steps are distinct.
    async fn turn_in_steps(&mut self, prompt: &str, steps: &[&[(&str, &str)]]) -> i64 {
        self.drive(prompt, Scripted::in_steps(steps)).await
    }

    async fn drive(&mut self, prompt: &str, provider: Scripted) -> i64 {
        self.session
            .turn(
                prompt,
                &provider,
                &self.store,
                &Policy::permissive(),
                &ApproveAll,
            )
            .await
            .expect("a scripted turn cannot fail");
        let head = self.session.head().expect("the turn is on the head");
        self.store
            .session_turn(head)
            .expect("the turn is readable")
            .expect("the turn is there")
            .run_id
    }

    fn workspace(&self) -> Workspace {
        Workspace::new(self.dir.path())
    }

    fn read(&self, path: &str) -> Option<String> {
        std::fs::read_to_string(self.dir.path().join(path)).ok()
    }

    fn exists(&self, path: &str) -> bool {
        self.dir.path().join(path).exists()
    }
}

// ---------------------------------------------------------------------------
// F6 — one file, and four answers that stay four answers
// ---------------------------------------------------------------------------

/// **F6 — a modified file comes back with its pre-run bytes.**
///
/// Asserted on the workspace, never on the returned sentence.
#[tokio::test]
async fn f6_a_modified_file_comes_back_as_it_was() {
    let mut fixture = Fixture::new();
    std::fs::write(fixture.dir.path().join("thing.rs"), "the original\n")
        .expect("the file starts out as the operator left it");
    let run = fixture
        .turn_writing("edit it", &[("thing.rs", "what the agent wrote\n")])
        .await;
    assert_eq!(
        fixture.read("thing.rs").as_deref(),
        Some("what the agent wrote\n")
    );

    let answer =
        one_file(&fixture.workspace(), &fixture.store, run, "thing.rs").expect("the undo runs");

    assert!(matches!(answer, Rewind::Restored(_)), "{answer:?}");
    assert_eq!(
        fixture.read("thing.rs").as_deref(),
        Some("the original\n"),
        "the bytes on disk are the pre-run bytes",
    );
}

/// **F6 — a file the run created is removed, not emptied.**
#[tokio::test]
async fn f6_a_file_the_run_created_is_removed() {
    let mut fixture = Fixture::new();
    let run = fixture
        .turn_writing("make it", &[("new.rs", "brand new\n")])
        .await;
    assert!(fixture.exists("new.rs"));

    let answer =
        one_file(&fixture.workspace(), &fixture.store, run, "new.rs").expect("the undo runs");

    assert_eq!(answer, Rewind::Removed);
    assert!(
        !fixture.exists("new.rs"),
        "a created file is gone, not left empty",
    );
}

/// **F6 — a path the run never touched answers `NotRecorded` and changes
/// nothing.**
#[tokio::test]
async fn f6_a_path_the_run_never_touched_is_not_recorded() {
    let mut fixture = Fixture::new();
    let run = fixture
        .turn_writing("touch one", &[("written.rs", "by the agent\n")])
        .await;
    std::fs::write(fixture.dir.path().join("untouched.rs"), "mine\n")
        .expect("the operator's own file");

    let answer =
        one_file(&fixture.workspace(), &fixture.store, run, "untouched.rs").expect("the undo runs");

    assert_eq!(answer, Rewind::NotRecorded);
    assert_eq!(
        fixture.read("untouched.rs").as_deref(),
        Some("mine\n"),
        "an unrecorded path is left exactly alone",
    );
}

/// **F6 — the four answers are four sentences.**
///
/// `NotKept` and `NotRecorded` in particular: collapsing them tells an operator
/// "nothing to undo" about a file the run very much wrote, whose previous
/// contents were merely over the snapshot cap.
#[test]
fn f6_the_four_answers_say_four_different_things() {
    let restored = said("a.rs", &Rewind::Restored(io_harness::tools::Wrote::Changed));
    let removed = said("a.rs", &Rewind::Removed);
    let not_kept = said("a.rs", &Rewind::NotKept("over the cap".to_string()));
    let not_recorded = said("a.rs", &Rewind::NotRecorded);

    let all = [&restored, &removed, &not_kept, &not_recorded];
    for (i, one) in all.iter().enumerate() {
        for (j, other) in all.iter().enumerate() {
            if i != j {
                assert_ne!(one, other, "every answer is its own sentence");
            }
        }
    }
    assert!(not_kept.contains("not kept"), "{not_kept}");
    assert!(
        not_kept.contains("over the cap"),
        "the reason survives: {not_kept}"
    );
    assert!(not_recorded.contains("no restore point"), "{not_recorded}");
    assert!(
        removed.contains("created"),
        "a removal says why it was a removal: {removed}",
    );
}

// ---------------------------------------------------------------------------
// F7 — one step, the rest of the run untouched, and the events that prove it
// ---------------------------------------------------------------------------

/// **F7 — undoing one step leaves the other step's file exactly as the run left
/// it, and `EventKind::Reverted` arrives.**
///
/// The event is the assertion that proves the `_observed` form is what was
/// called. A rendered sentence would be identical either way.
#[tokio::test]
async fn f7_one_step_is_undone_and_the_rest_of_the_run_stands() {
    let mut fixture = Fixture::new();
    std::fs::write(fixture.dir.path().join("two.rs"), "two before\n").expect("a starting file");
    std::fs::write(fixture.dir.path().join("three.rs"), "three before\n").expect("a starting file");
    let run = fixture
        .turn_in_steps(
            "two steps",
            &[
                &[("two.rs", "two after\n")],
                &[("three.rs", "three after\n")],
            ],
        )
        .await;

    // Step 2 is the second batch, because the run loop numbers from one.
    let watcher = Collected::default();
    let answers =
        one_step(&fixture.workspace(), &fixture.store, run, 1, &watcher).expect("the undo runs");

    assert!(
        !answers.is_empty(),
        "step 1 wrote a file, so it has an answer"
    );
    assert_eq!(
        fixture.read("two.rs").as_deref(),
        Some("two before\n"),
        "the undone step's file is back",
    );
    assert_eq!(
        fixture.read("three.rs").as_deref(),
        Some("three after\n"),
        "and the other step's is exactly as the run left it",
    );
    assert_eq!(
        watcher.saw_reverted(),
        Some(1),
        "EventKind::Reverted arrived, naming the step — which only the observed \
         form emits, and which nothing in this product had ever emitted",
    );
}

/// **F7 — undoing the whole run emits `EventKind::Rewound`.**
///
/// The event this product has never fired. `src/rewind.rs:200` called the plain
/// `rewind_run` from 0.4.0 until this release, so the event had never fired.
///
/// **What that buys is not a line on screen.** `rewound` is `Disposition::Silent`
/// and routes to io-cli's own rewind summary; `src/events.rs` has no arm for it
/// and correctly draws nothing. What the observed form buys is that the event
/// exists at all — it reaches `[[hook]]` observers, `io exec --json` and the
/// durable trace, none of which it had ever reached. An earlier draft of this
/// file claimed there was an unreachable renderer arm; there is not, and the
/// adversarial review caught the overclaim.
///
/// It goes through `rewind::last_turn`, which is the production path: that is
/// what moves the conversation head back, and a thin wrapper beside it would be
/// a second entry point to one act.
#[tokio::test]
async fn f7_the_whole_run_emits_rewound() {
    let mut fixture = Fixture::new();
    std::fs::write(fixture.dir.path().join("one.rs"), "before\n").expect("a starting file");
    fixture
        .turn_writing("a whole run", &[("one.rs", "after\n")])
        .await;

    let watcher = Collected::default();
    let undone = io_cli::rewind::last_turn(&mut fixture.session, &fixture.store, &watcher)
        .expect("the undo runs")
        .expect("there was a turn to undo");

    assert!(!undone.restored.is_empty(), "there was a file to put back");
    assert_eq!(
        fixture.read("one.rs").as_deref(),
        Some("before\n"),
        "and it is back",
    );
    assert!(
        watcher.saw_rewound(),
        "EventKind::Rewound arrived — the assertion that the observed form was \
         called, which no rendered sentence could make",
    );
}

/// A step that wrote nothing is not an error, and says so.
#[tokio::test]
async fn a_step_that_wrote_nothing_changes_nothing_and_is_not_an_error() {
    let mut fixture = Fixture::new();
    let run = fixture.turn_writing("read only", &[]).await;

    let watcher = Collected::default();
    let answers =
        one_step(&fixture.workspace(), &fixture.store, run, 1, &watcher).expect("not an error");

    assert!(
        answers.is_empty(),
        "nothing was written, so nothing came back"
    );
}

/// **The order-sensitivity sentence is said only when something came back
/// stale.**
///
/// Advice about a problem the operator does not have is noise, and noise in a
/// destructive surface is what teaches people to stop reading it.
#[test]
fn the_stale_advice_is_given_only_when_something_is_stale() {
    let clean = vec![(
        "a.rs".to_string(),
        Reverted::Applied(io_harness::tools::Wrote::Changed),
    )];
    assert_eq!(step_advice(&clean), None, "nothing stale, nothing said");

    let stale = vec![("a.rs".to_string(), Reverted::Stale("moved".to_string()))];
    let advice = step_advice(&stale).expect("a stale answer earns the sentence");
    assert!(advice.contains("newest step first"), "{advice}");
    assert!(
        advice.contains("order-sensitive"),
        "and says why, or it reads as a bug: {advice}",
    );

    let said = step_said("a.rs", &Reverted::Stale("context moved".to_string()));
    assert!(
        said.contains("unchanged"),
        "a stale step changed nothing: {said}"
    );
    assert!(
        said.contains("context moved"),
        "the reason survives: {said}"
    );
}

// ---------------------------------------------------------------------------
// F10 — the group re-file
// ---------------------------------------------------------------------------

/// **F10 — `/contain` is in `Session`, `/undo` is in `Turn`, `/store` and
/// `/export` are in `Inspect`, and no group is longer than ten.**
#[test]
fn f10_the_groups_are_where_this_release_put_them() {
    use io_cli::commands::{group_of, Group, COMMANDS, GROUPS};

    assert_eq!(group_of("/contain"), Some(Group::Session));
    assert_eq!(group_of("/undo"), Some(Group::Turn));
    assert_eq!(group_of("/store"), Some(Group::Inspect));
    assert_eq!(group_of("/export"), Some(Group::Inspect));

    for (group, names) in GROUPS {
        assert!(
            names.len() <= 10,
            "{group:?} holds {} commands; ten is the bound",
            names.len(),
        );
    }
    assert_eq!(
        COMMANDS.len(),
        36,
        "0.27.0 adds exactly three commands to the thirty-three 0.26.0 shipped",
    );
}

/// The confirmations decline at row 0, like every other one this release adds.
#[test]
fn the_undo_confirmations_decline_at_row_zero() {
    for (what, rows) in [
        ("file", confirm_file("src/a.rs").1),
        ("step", confirm_step(4).1),
    ] {
        assert_eq!(rows[0].label, io_cli::store::LEAVE_IT, "{what}");
        assert!(!io_cli::store::acts(0), "{what}");
        assert!(io_cli::store::acts(1), "{what}");
    }
}

/// The file confirmation discloses the one thing an operator can lose.
///
/// io-harness restores from the snapshot taken before the run first wrote the
/// path and does not compare it against what is on disk now, so an edit made by
/// hand after the turn is overwritten without a word. io-cli cannot detect that;
/// it can say so before the keystroke, which is what this asserts.
#[test]
fn the_file_confirmation_discloses_what_a_restore_overwrites() {
    let (_title, rows) = confirm_file("src/a.rs");
    let detail = rows[1].detail.clone().unwrap_or_default();

    assert!(detail.contains("by hand"), "{detail}");
    assert!(detail.contains("overwritten"), "{detail}");
}

/// `Grain` is a description of what was asked for and nothing more.
///
/// It carries no `label()`: the confirmations build their own sentences and the
/// run arm never renders one, so a formatter here would be reachable only from
/// this file — the tested-but-unreachable shape this codebase has shipped three
/// times. The adversarial review caught it before it became a fourth.
#[test]
fn the_grains_carry_only_what_was_asked_for() {
    assert_eq!(
        Grain::File("src/a.rs".into()),
        Grain::File("src/a.rs".into())
    );
    assert_ne!(Grain::Step(1), Grain::Step(2));
    assert_ne!(Grain::Run, Grain::Step(1));
}
