//! Carrying a paused run on — F2, F3, F4, F5 and F6.
//!
//! Every fixture here is a **real store on disk**, opened through `Store::open`
//! on a temporary directory, and every run in one either really paused or holds
//! rows written by the same public writer the run loop writes them with. Two of
//! the five states can be reached by driving: a question pause is what a
//! scripted `ask_question` call plus io-harness's own declining responder
//! produces, and an interruption is what `Store::finish_run` records. The other
//! three cannot be driven to from outside the crate at all — a plan pause needs
//! a model that proposes one, an interrupted call needs a process to die
//! mid-call, and a run whose process went away needs the process to go away — so
//! their rows are written with `Store::put_plan`, `Store::open_attempt` and
//! `Store::record`. Those are the same three writers the dispatch and the step
//! loop call, so the rows are authentic in the only sense that matters here:
//! nothing in this file writes a row a run could not have written.
//!
//! What each block would catch, stated so that a green run means something:
//!
//! * **The classification block** catches a reader that answers from the run's
//!   status. Sabotage it by classifying on `Store::run_status` alone: a run
//!   paused on a question is recorded as `completed` — `finish_run` maps only
//!   `awaiting_approval` and `awaiting_plan` to `paused` — so the commonest
//!   pause of all reads as an ended run and nothing is ever offered.
//!
//! * **The step-arithmetic block** is F2 and F3. It reads `Store::last_step`
//!   before the resume and asserts the `resume` marker against that number plus
//!   one. Sabotage it by writing the literal `2`: the assertion still passes on
//!   this fixture and stops meaning anything the moment a run pauses at any other
//!   step, which is every real run. It also counts the `skipped` markers, which
//!   is what fails if a resume re-drives work that was already committed and
//!   already paid for.
//!
//! * **The turn block** is F4. Sabotage it by deleting the `finish_turn` call:
//!   nothing errors, the run is genuinely carried on, and the turn goes on
//!   reading `awaiting_answer` with an empty reply for the rest of the
//!   conversation's life — so an operator scrolling back sees the question they
//!   answered still open, and the transcript disagrees with the run.
//!
//! * **The race block** is the half of F4 a happy path cannot reach. Sabotage it
//!   by reaching for `set_session_head` instead of `set_session_head_if` and
//!   every other assertion in this file stays green while a second process's
//!   turn is silently taken off the head path — answered, billed, and invisible
//!   to the next turn. Two `Store` handles on one file is the shape io-harness's
//!   own lease suite uses; there is no second process and nothing waits.
//!
//! * **The interruption block** is F5. Sabotage it by letting the driver through
//!   to `resume_with_observed`: the call returns `Ok` carrying the original
//!   outcome, so a driver that trusted the `Result` would report a successful
//!   resume of a run it never touched. The assertion is on the absence of a
//!   `resume` marker rather than on the return value, for exactly that reason.
//!
//! * **The recovery block** is F6, and it asserts the step number rather than
//!   the text. Sabotage it by filing the operator's account at the run's current
//!   step: the text is still in the ledger and a `contains` assertion still
//!   passes, while the model reads a transcript in which a call it made three
//!   steps ago was answered after the fact — which is not what happened, and is
//!   the one thing the operator was asked to establish.
//!
//! * **The cancelled-plan block** catches a `Cancel` mapped onto a plain resume.
//!   Sabotage it that way and the outcome is no longer `plan_rejected`, the run
//!   re-enters the loop, and the rest of the budget goes on the approach that was
//!   just refused.
//!
//! * **The two wrong-owner blocks** catch a driver that trusts the ids it is
//!   handed. Sabotage either by dropping the ownership check: io-harness refuses
//!   it too, but as one `Error::Resume` whose `Display` is written for a library
//!   caller, and the sentence an operator gets stops naming which run their
//!   answer was about to reach.

mod support;

use io_cli::resume::{self, Failure, Pending};
use io_harness::provider::{CompletionRequest, CompletionResponse, ToolCall};
use io_harness::{
    ApproveAll, Ignore, Plan, PlanStep, PlanVerdict, Policy, Provider, RecoveryDecision, RunOutcome,
    Session, StepRecord, Store, TaskContract, ToolRecovery, ASK_QUESTION_TOOL,
};
use support::Scripted;

const GOAL: &str = "tidy the configuration";
const QUESTION: &str = "Which configuration file did you mean?";
const CHOICE_A: &str = "io.toml";
const CHOICE_B: &str = "io.local.toml";
/// Deliberately unlike anything the loop writes on its own, so the assertion
/// about *where* it was filed cannot be satisfied by some other row.
const ACCOUNT: &str = "the charge landed, reference ch-9f21";

/// One `ask_question` call, with its arguments assembled as JSON and parsed.
///
/// `ToolCall::arguments` is a `serde_json::Value`, which implements `FromStr`, so
/// the type is inferred from the field rather than named — the same route
/// `support::write_call` takes and for the same reason.
fn ask_call() -> ToolCall {
    ToolCall {
        name: ASK_QUESTION_TOOL.to_string(),
        arguments: format!(
            "{{\"question\":\"{QUESTION}\",\"choices\":[\"{CHOICE_A}\",\"{CHOICE_B}\"]}}"
        )
        .parse()
        .expect("the arguments were assembled as JSON and must parse as JSON"),
    }
}

/// A provider whose every completion asks the operator the same thing.
///
/// It is called exactly once: the contract carries no responder, io-harness
/// stands in its own declining one, and the run pauses on the row the dispatch
/// has already written. A provider that could also finish would make the pause a
/// matter of timing rather than of the responder, which is the property these
/// fixtures rest on.
struct Asking;

impl Provider for Asking {
    async fn complete(
        &self,
        _request: CompletionRequest,
    ) -> io_harness::Result<CompletionResponse> {
        Ok(CompletionResponse {
            text: None,
            tool_calls: vec![ask_call()],
            ..Default::default()
        })
    }
}

/// A store on disk, a session over it, and one turn that really paused.
///
/// On disk rather than in memory because one of the blocks below opens a second
/// handle on the same file, and an in-memory store is private to the connection
/// that made it.
struct Paused {
    dir: tempfile::TempDir,
    store: Store,
    session: Session,
    run_id: i64,
    question_id: i64,
}

impl Paused {
    async fn new() -> Self {
        let dir = tempfile::tempdir().expect("a temp dir");
        let store = Store::open(dir.path().join("runs.db")).expect("a store on disk");
        let mut session = Session::open(&store, dir.path()).expect("a session");
        let result = session
            .turn_bounded(
                &TaskContract::workspace(GOAL, dir.path()),
                &Asking,
                &store,
                &Policy::permissive(),
                &ApproveAll,
            )
            .await
            .expect("a turn that pauses is not a turn that failed");
        let (run_id, question_id) = match result.outcome {
            RunOutcome::AwaitingAnswer { question_id, .. } => (result.run_id, question_id),
            other => panic!("the turn was supposed to pause for an answer, and got {other:?}"),
        };
        Self {
            dir,
            store,
            session,
            run_id,
            question_id,
        }
    }

    /// The contract a resume of this run is driven under, rebuilt the way
    /// `crate::resume` says a caller has to rebuild one.
    fn contract(&self) -> TaskContract {
        let goal = resume::goal_for(&self.store, self.run_id)
            .expect("the store answers")
            .expect("a session turn's goal is recoverable from its own prompt");
        TaskContract::workspace(goal, self.dir.path())
    }

    fn path(&self) -> std::path::PathBuf {
        self.dir.path().join("runs.db")
    }
}

/// A store on disk and a bare run in it with `steps` committed steps.
///
/// The steps are written with `Store::record`, which is the writer the step loop
/// itself calls, so `Store::last_step` answers about this run exactly as it would
/// about one whose process went away mid-loop — which, with no outcome recorded,
/// is what this run is.
fn bare_run(steps: u32) -> (tempfile::TempDir, Store, i64) {
    let dir = tempfile::tempdir().expect("a temp dir");
    let store = Store::open(dir.path().join("runs.db")).expect("a store on disk");
    let run_id = store
        .start_run("port the parser", &dir.path().display().to_string())
        .expect("a run row");
    for step in 1..=steps {
        store
            .record(run_id, &StepRecord::new(step, "wrote a file", "ok"))
            .expect("a trace row");
    }
    (dir, store, run_id)
}

/// The steps a run's checkpoint markers of one kind name, in the order they were
/// written.
fn markers(store: &Store, run_id: i64, kind: &str) -> Vec<u32> {
    store
        .checkpoint_events(run_id)
        .expect("the store answers")
        .into_iter()
        .filter(|event| event.kind == kind)
        .map(|event| event.step)
        .collect()
}

/// Where a session's head points, read from the store rather than from a handle
/// that caches it.
fn head_of(store: &Store, session_id: i64) -> Option<i64> {
    Session::reopen(store, session_id)
        .expect("the session is still there")
        .head()
}

#[tokio::test]
async fn a_paused_question_is_reported_with_its_own_row_and_nothing_is_driven() {
    let fixture = Paused::new().await;
    let before = fixture.store.last_step(fixture.run_id).expect("a step count");

    assert_eq!(
        resume::pending_for(&fixture.store, fixture.run_id).expect("the store answers"),
        Pending::Question {
            question_id: fixture.question_id,
            question: QUESTION.to_string(),
            context: None,
            choices: vec![CHOICE_A.to_string(), CHOICE_B.to_string()],
            step: before,
        },
        "the classification carries the row's own id and the agent's own words",
    );
    assert!(
        markers(&fixture.store, fixture.run_id, "resume").is_empty(),
        "reading what a run stopped on must not drive it",
    );
}

#[tokio::test]
async fn an_answer_drives_the_run_from_the_step_after_the_last_one_committed() {
    let fixture = Paused::new().await;
    // Read before anything is driven, and every number below is arithmetic on it.
    // A literal here would pass on this fixture and mean nothing on a run that
    // paused anywhere else, which is every run an operator actually has.
    let before = fixture.store.last_step(fixture.run_id).expect("a step count");
    let head = fixture.session.head();

    let resumed = resume::answer_question(
        &fixture.contract(),
        &Scripted::writing(&[]),
        &fixture.store,
        fixture.run_id,
        fixture.question_id,
        CHOICE_B,
        &Policy::permissive(),
        &ApproveAll,
        None,
        &Ignore,
        head,
    )
    .await
    .expect("an answered question carries the run on");

    assert_eq!(
        resumed.resumed_after, before,
        "the report names the step the run had reached when the answer was given",
    );
    assert_eq!(
        markers(&fixture.store, fixture.run_id, "resume"),
        vec![before + 1],
        "one resume marker, at the step after the last committed one",
    );
    assert_eq!(
        markers(&fixture.store, fixture.run_id, "skipped").len(),
        before as usize,
        "one skipped marker per step that was already committed and already paid for",
    );
    assert_eq!(
        fixture.store.last_step(fixture.run_id).expect("a step count"),
        before + 1,
        "the run really moved on",
    );
}

#[tokio::test]
async fn a_resumed_turn_is_closed_with_the_outcome_and_the_reply_the_store_holds() {
    let fixture = Paused::new().await;
    let head = fixture.session.head();
    let turn_id = fixture
        .store
        .turn_for_run(fixture.run_id)
        .expect("the store answers")
        .expect("a session turn served this run");

    // Before: the turn is closed with the pause, and that is what an operator
    // scrolling back would still see if nothing here stitched it up.
    let parked = fixture
        .store
        .session_turn(turn_id)
        .expect("the store answers")
        .expect("the turn");
    assert_eq!(parked.outcome.as_deref(), Some("awaiting_answer"));
    assert_eq!(parked.reply, None);

    let resumed = resume::answer_question(
        &fixture.contract(),
        &Scripted::writing(&[]),
        &fixture.store,
        fixture.run_id,
        fixture.question_id,
        CHOICE_B,
        &Policy::permissive(),
        &ApproveAll,
        None,
        &Ignore,
        head,
    )
    .await
    .expect("an answered question carries the run on");

    assert_eq!(resumed.turn_id, Some(turn_id));
    assert!(
        matches!(resumed.outcome, RunOutcome::Finished { .. }),
        "the scripted provider says its piece and stops, and got {:?}",
        resumed.outcome,
    );

    let closed = fixture
        .store
        .session_turn(turn_id)
        .expect("the store answers")
        .expect("the turn");
    assert_eq!(
        closed.outcome.as_deref(),
        Some("finished"),
        "the turn carries the ending the store recorded, not the pause it was closed with",
    );
    assert!(
        closed.reply.as_deref().is_some_and(|said| !said.is_empty()),
        "a resumed turn holds what the agent said, and got {:?}",
        closed.reply,
    );
    assert_eq!(
        closed.reply, resumed.reply,
        "the report and the row are the same reply rather than two readings of it",
    );
    assert_eq!(
        head_of(&fixture.store, fixture.session.id()),
        Some(turn_id),
        "the conversation continues from the turn that was carried on",
    );
}

#[tokio::test]
async fn a_head_another_handle_moved_first_makes_the_head_write_refuse() {
    let fixture = Paused::new().await;
    let head = fixture.session.head();
    let session_id = fixture.session.id();
    let turn_id = fixture
        .store
        .turn_for_run(fixture.run_id)
        .expect("the store answers")
        .expect("a session turn served this run");

    // A second handle on the same file — the shape io-harness's own lease suite
    // uses. No second process is started and nothing waits on a clock: the race
    // is decided by which connection writes first, and this one does.
    let other = Store::open(fixture.path()).expect("a second handle on the same store");
    other
        .set_session_head_if(session_id, head, None)
        .expect("the other handle holds the head it expected, so its write lands");

    let failure = resume::answer_question(
        &fixture.contract(),
        &Scripted::writing(&[]),
        &fixture.store,
        fixture.run_id,
        fixture.question_id,
        CHOICE_B,
        &Policy::permissive(),
        &ApproveAll,
        None,
        &Ignore,
        head,
    )
    .await
    .expect_err("a head that moved under the resume must be reported, not overwritten");

    assert!(
        matches!(
            failure,
            Failure::HeadMoved { session_id: s, turn_id: t } if s == session_id && t == turn_id
        ),
        "the loser of the race is told which turn holds its answer, and got {failure:?}",
    );

    // The half that must survive being told the head did not move: the answer was
    // given, the run was driven, and the turn holds what came back. A resume that
    // rolled any of that back would destroy the one copy of what the model said.
    let closed = fixture
        .store
        .session_turn(turn_id)
        .expect("the store answers")
        .expect("the turn");
    assert_eq!(closed.outcome.as_deref(), Some("finished"));
    assert!(closed.reply.is_some());
    assert_eq!(
        head_of(&fixture.store, session_id),
        None,
        "the head the other handle wrote stands; nothing here overwrote it",
    );
}

#[tokio::test]
async fn a_question_somebody_else_answered_is_refused_before_the_run_is_driven() {
    let fixture = Paused::new().await;
    // What a second process attached to this run would have done. It is the
    // dangerous case rather than a contrived one: the run has already acted on
    // whatever this answer was, and driving it again from a second one is the
    // silent double-answer the store's compare-and-swap exists to prevent.
    assert!(
        fixture
            .store
            .answer_question(fixture.question_id, CHOICE_A, "human")
            .expect("the store answers"),
        "the swap finds an unanswered question and takes it",
    );

    let failure = resume::answer_question(
        &fixture.contract(),
        &Scripted::writing(&[]),
        &fixture.store,
        fixture.run_id,
        fixture.question_id,
        CHOICE_B,
        &Policy::permissive(),
        &ApproveAll,
        None,
        &Ignore,
        fixture.session.head(),
    )
    .await
    .expect_err("a question with an answer already on it cannot take a second");

    let asked = fixture.question_id;
    assert!(
        matches!(failure, Failure::QuestionAnswered { question_id } if question_id == asked),
        "the sentence names the question rather than the run, and got {failure:?}",
    );
    assert!(
        markers(&fixture.store, fixture.run_id, "resume").is_empty(),
        "the refusal happens before anything drives",
    );
}

#[tokio::test]
async fn an_interrupted_run_is_never_driven_and_is_pointed_at_fork_instead() {
    let (dir, store, run_id) = bare_run(1);
    // What `Ctrl+C` leaves behind: the observer stops the run and the loop writes
    // this outcome, which `finish_run` then records as a *completed* status.
    store
        .finish_run(run_id, resume::CANCELLED)
        .expect("the outcome is recorded");

    assert_eq!(
        resume::pending_for(&store, run_id).expect("the store answers"),
        Pending::Interrupted,
        "a cancelled run is the operator's own ending, not a pause",
    );

    let failure = resume::carry_on(
        &TaskContract::workspace("port the parser", dir.path()),
        &Scripted::writing(&[]),
        &store,
        run_id,
        Some(&Policy::permissive()),
        &ApproveAll,
        None,
        &Ignore,
        None,
    )
    .await
    .expect_err("every resume entry point would return the original outcome having driven nothing");

    assert!(
        matches!(failure, Failure::Interrupted { run_id: r } if r == run_id),
        "{failure:?}",
    );
    assert!(
        failure.to_string().contains("/fork"),
        "the refusal names the neighbouring answer rather than only saying no: {failure}",
    );
    // The assertion that matters. A driver that let this through would get `Ok`
    // back carrying `Cancelled`, so a return value proves nothing; the absence of
    // a resume marker proves the loop was never entered.
    assert!(
        markers(&store, run_id, "resume").is_empty(),
        "nothing drove",
    );
    assert_eq!(store.last_step(run_id).expect("a step count"), 1);
}

#[tokio::test]
async fn a_run_whose_process_went_away_carries_on_from_its_last_committed_step() {
    let (dir, store, run_id) = bare_run(2);
    let before = store.last_step(run_id).expect("a step count");

    assert_eq!(
        resume::pending_for(&store, run_id).expect("the store answers"),
        Pending::Died { last_step: before },
    );

    let resumed = resume::carry_on(
        &TaskContract::workspace("port the parser", dir.path()),
        &Scripted::writing(&[]),
        &store,
        run_id,
        Some(&Policy::permissive()),
        &ApproveAll,
        None,
        &Ignore,
        None,
    )
    .await
    .expect("a run with committed work and no ending is exactly what a resume is for");

    assert_eq!(resumed.resumed_after, before);
    assert_eq!(
        markers(&store, run_id, "resume"),
        vec![before + 1],
        "the loop picks up after the last step that committed",
    );
    assert_eq!(
        resumed.turn_id, None,
        "this run served no session turn, so there is no turn to close and that is not a failure",
    );
    assert_eq!(resumed.reply, None);
}

#[tokio::test]
async fn a_run_whose_boundary_nothing_recorded_is_refused_rather_than_given_one() {
    let (dir, store, run_id) = bare_run(1);

    let failure = resume::carry_on(
        &TaskContract::workspace("port the parser", dir.path()),
        &Scripted::writing(&[]),
        &store,
        run_id,
        None,
        &ApproveAll,
        None,
        &Ignore,
        None,
    )
    .await
    .expect_err("no recorded policy and none supplied means nobody knows what it could do");

    assert!(
        matches!(failure, Failure::NoRecordedPolicy { run_id: r } if r == run_id),
        "{failure:?}",
    );
    assert!(
        markers(&store, run_id, "resume").is_empty(),
        "the refusal happens before anything drives",
    );
}

#[tokio::test]
async fn an_operators_account_of_an_interrupted_call_lands_on_the_step_it_was_made_on() {
    let (dir, store, run_id) = bare_run(3);
    // Written with `Store::open_attempt`, which is the writer the dispatch calls
    // before every call it cannot replay — so this row is what a process dying
    // mid-charge leaves behind, not an imitation of one. `Indeterminate` is the
    // recovery class that journals at all; a `Replayable` call is not written.
    let attempt_id = store
        .open_attempt(run_id, 2, "charge_card", ToolRecovery::Indeterminate)
        .expect("the store answers")
        .expect("an indeterminate call is journalled");

    assert_eq!(
        resume::pending_for(&store, run_id).expect("the store answers"),
        Pending::Recovery {
            attempt_id,
            tool: "charge_card".to_string(),
            step: 2,
        },
    );

    let resumed = resume::recover(
        &TaskContract::workspace("port the parser", dir.path()),
        &Scripted::writing(&[]),
        &store,
        run_id,
        attempt_id,
        RecoveryDecision::Completed {
            observation: ACCOUNT.to_string(),
        },
        &Policy::permissive(),
        &ApproveAll,
        None,
        &Ignore,
        None,
    )
    .await
    .expect("a decided call lets the run carry on");

    assert_eq!(
        resumed.resumed_after, 3,
        "the run had reached step 3, which is not the step the call was made on",
    );

    // The assertion is the step number. The text alone would be satisfied by an
    // account filed wherever the run happened to be, which is a transcript in
    // which a call made three steps ago was answered after the fact.
    let filed: Vec<u32> = store
        .observations(run_id)
        .expect("the store answers")
        .into_iter()
        .filter(|observation| observation.text.contains(ACCOUNT))
        .map(|observation| observation.step)
        .collect();
    assert_eq!(
        filed,
        vec![2],
        "the operator's account belongs to the step the call was made on",
    );
}

#[tokio::test]
async fn a_cancelled_plan_ends_the_run_without_re_entering_the_loop() {
    let (dir, store, run_id) = bare_run(1);
    let steps = vec![
        PlanStep::new("rewrite the parser"),
        PlanStep::new("port the tests"),
    ];
    // `Store::put_plan` is what the gate writes before it is consulted, which is
    // the whole of io-harness's durability claim for a plan — so a row written
    // this way is the row a process dying between the proposal and the verdict
    // leaves behind.
    let plan_id = store
        .put_plan(run_id, 1, &Plan::new(steps.clone()))
        .expect("a plan row");

    assert_eq!(
        resume::pending_for(&store, run_id).expect("the store answers"),
        Pending::Plan {
            plan_id,
            steps,
            step: 1,
        },
    );

    let resumed = resume::decide_plan(
        &TaskContract::workspace("port the parser", dir.path()),
        &Scripted::writing(&[]),
        &store,
        run_id,
        plan_id,
        PlanVerdict::Cancel,
        &Policy::permissive(),
        &ApproveAll,
        None,
        &Ignore,
        None,
    )
    .await
    .expect("cancelling a plan is a decision, not a failure");

    assert!(
        matches!(resumed.outcome, RunOutcome::PlanRejected { .. }),
        "a cancelled plan ends the run as rejected, and got {:?}",
        resumed.outcome,
    );
    assert_eq!(
        store.outcome(run_id).expect("the store answers").as_deref(),
        Some("plan_rejected"),
    );
    assert!(
        markers(&store, run_id, "resume").is_empty(),
        "the loop is never re-entered, so the rest of the budget is not spent on \
         the approach that was just refused",
    );
    assert_eq!(
        store.last_step(run_id).expect("a step count"),
        1,
        "no step was driven",
    );
}

#[tokio::test]
async fn a_plan_belonging_to_another_run_is_refused_and_names_both_runs() {
    let (dir, store, proposer) = bare_run(1);
    let other = store
        .start_run("write the release notes", &dir.path().display().to_string())
        .expect("a second run row");
    store
        .record(other, &StepRecord::new(1, "read a file", "ok"))
        .expect("a trace row");
    let plan_id = store
        .put_plan(proposer, 1, &Plan::new([PlanStep::new("rewrite the parser")]))
        .expect("a plan row");

    let failure = resume::decide_plan(
        &TaskContract::workspace("write the release notes", dir.path()),
        &Scripted::writing(&[]),
        &store,
        other,
        plan_id,
        PlanVerdict::Approve,
        &Policy::permissive(),
        &ApproveAll,
        None,
        &Ignore,
        None,
    )
    .await
    .expect_err("a verdict on somebody else's plan authorises somebody else's work");

    assert!(
        matches!(
            failure,
            Failure::PlanElsewhere { plan_id: p, owner, run_id }
                if p == plan_id && owner == proposer && run_id == other
        ),
        "{failure:?}",
    );
    assert!(
        markers(&store, other, "resume").is_empty(),
        "the run that was named was never driven",
    );
    let untouched = store
        .plan(plan_id)
        .expect("the store answers")
        .expect("the plan");
    assert!(
        !untouched.resolved,
        "and the plan that was named still has no verdict on it",
    );
}
