//! Answering a run that paused, instead of abandoning it.
//!
//! Since 0.10.0 this interface has been able to ask the operator a question and
//! to propose a plan, and since 0.20.0 a tool call can be interrupted in a way
//! io-harness records but cannot judge. In every one of those cases the run
//! stops, the pending row is written to the store, and through 0.22.0 io-cli
//! walked away from it: [`crate::exec`] printed that carrying on was "not in
//! this release" and exited 4, and `/resume` — one line, `Session::reopen` —
//! swapped the session handle without ever asking what the last run was waiting
//! on.
//!
//! Everything needed to do better has been public in io-harness the whole time.
//! This module reads the pending rows back, drives the matching resume entry
//! point, and then does the session bookkeeping that entry point does not.
//!
//! # The four kinds, which look alike and are not
//!
//! A question, a plan, an interrupted tool call and a run whose process merely
//! died each have their own reader, their own resume function and their own
//! flat-versus-tree twin. They are kept apart on purpose: collapsing them into
//! one generic switch over an integer is how an operator's answer is delivered
//! into somebody else's run, so each pending kind is its own type carrying its
//! own id and no bare integer crosses a surface.
//!
//! # What a free resume does not do
//!
//! `Session::drive` is the only thing in io-harness that stitches a run to a
//! turn, and it is private. The free resume functions know nothing about turns,
//! so after one of them the `session_turns` row still reads `awaiting_answer`
//! with an empty reply and the session head has not moved. `Store::turn_for_run`,
//! `Store::finish_turn` and `Store::set_session_head_if` are all public, so this
//! module closes the turn itself — with the **compare-and-swap** head write and
//! never the unconditional one, which is the defect [`crate::rewind`] carried
//! until this release.
//!
//! The reply is taken off the last assistant turn in the store rather than
//! reconstructed. io-harness's own extraction is a private function splitting on
//! a `pub(crate)` sentinel, so reproducing it would mean hardcoding a literal
//! this crate cannot see change.
//!
//! # The one pause that cannot be answered
//!
//! A turn the operator interrupted is finished, not paused. `Ctrl+C` sets the
//! flag that returns `Flow::Cancel`, io-harness records the outcome `cancelled`,
//! `finish_run` maps that to a *completed* status, and every resume entry point
//! short-circuits on a completed run and returns the original outcome without
//! driving. So the most common way an io-cli turn stops is the one way it cannot
//! be continued. This module reports such a run as ended by the operator and
//! points at `/fork` from the turn before it, which is the honest neighbouring
//! answer.
//!
//! The published io-harness documentation disagrees — `Steer::interrupt` says
//! such a turn "stays resumable" — and it is wrong, contradicted by the run loop
//! in the same crate. Reported upstream; not worked around here.
//!
//! # What else this module works around rather than fixes
//!
//! * **The goal is not readable.** `runs.goal` has no public reader, so a
//!   contract cannot be rebuilt from the run alone. For a run that served a
//!   session turn the operator's own words are recoverable — that is
//!   [`goal_for`] — and for a bare run they are not, so the caller supplies
//!   them. Every driver here therefore takes a whole [`TaskContract`] rather
//!   than a run id and a promise to reconstruct one, which also settles what
//!   `Session::drive` being private would otherwise cost.
//! * **A stitched turn always reads back as `TurnKind::Run`.**
//!   `Store::turn_kind` and `Store::set_turn_kind` are both `pub(crate)`, so
//!   this module cannot say what kind of turn it closed and cannot read what
//!   kind the original was. A conversational turn that paused and was resumed
//!   here is therefore reported as a run. Nothing in this crate can do better
//!   until the pair is published.
//! * **An interrupted call in a *contained* run has no entry point.**
//!   io-harness 0.69 publishes `resume_tree_with_answer` and
//!   `resume_tree_with_plan_decision` but no recovery twin, and the flat
//!   `resume_with_recovery` ends in the flat loop — so driving a tree root
//!   through it would silently drop the containment the tree was running under.
//!   [`recover`] refuses instead, which is why it is the one driver here that
//!   takes no [`Containment`].
//! * **`Attach::answer_question` must not be used on this path.** On a run that
//!   has already ended it resolves the row and the compare-and-swap inside
//!   `resume_with_answer` then finds nothing left to answer, so the resume fails
//!   with `Error::Resume` and the operator's answer is stranded in a row no run
//!   will ever read. The answer travels as an argument to [`answer_question`]
//!   and by no other road.
//!
//! # Where the sentences come from
//!
//! `Error::Resume` is one variant carrying a `reason` string, and four quite
//! different mistakes arrive in it: a question that belongs to another run, a
//! question somebody already answered, a plan that belongs to another run, and a
//! run whose recorded policy cannot be rebuilt. An interface that printed the
//! `Display` would hand an operator a sentence written for a library caller. So
//! each of the four is checked here, before the harness is asked, and each has
//! its own [`Failure`] variant with its own sentence. What is left — a storage
//! failure, a checkpoint format from the future — is carried through as
//! [`Failure::Harness`], which is the honest place for a cause this module has
//! nothing better to say about.

use io_harness::{
    AssistantTurn, Containment, Error, Observer, PendingQuestion, PlanStep, PlanVerdict, Policy,
    Provider, RecoveryDecision, RunOutcome, RunStatus, Store, TaskContract,
};

/// What a run stopped on, as data. Marks, wording and ordering belong to the
/// surface that shows it; nothing here is a sentence an operator reads.
///
/// The four answerable kinds each carry their own id under its own name, so a
/// question id cannot be handed to the plan driver by a caller that got its
/// variables the wrong way round — the mistake that would deliver one operator's
/// answer into somebody else's run.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Pending {
    /// The agent asked the operator something and nobody answered.
    Question {
        /// The row [`answer_question`] takes.
        question_id: i64,
        /// What the agent asked, in its own words.
        question: String,
        /// What it already knew, when it said.
        context: Option<String>,
        /// The options it offered. An answer is not obliged to be one of them.
        choices: Vec<String>,
        /// The step it asked on, which is committed — so the resume starts after
        /// it and the question is not asked twice.
        step: u32,
    },
    /// The agent proposed an approach and nobody decided.
    Plan {
        /// The row [`decide_plan`] takes.
        plan_id: i64,
        /// The approach, step by step.
        steps: Vec<PlanStep>,
        /// The step it was proposed on, committed for the same reason a
        /// question's is.
        step: u32,
    },
    /// A call the harness cannot inspect was started and never finished, so
    /// whether it landed is a fact only the operator can establish.
    Recovery {
        /// The journal row [`recover`] takes.
        attempt_id: i64,
        /// The tool that was called.
        tool: String,
        /// The step the call was made on. The operator's account is filed
        /// against *this* step and not the current one, so the resumed run reads
        /// a transcript in which the tool answered where it was asked.
        step: u32,
    },
    /// The process went away mid-loop. Nothing is waiting for a person; the run
    /// simply has committed work and no ending.
    Died {
        /// The last step that committed. The resume starts at the one after it.
        last_step: u32,
    },
    /// The operator stopped the turn themselves. Terminal — see this module's
    /// documentation for why, and [`Failure::Interrupted`] for what to offer
    /// instead.
    Interrupted,
    /// There is nothing to answer and nothing to drive.
    ///
    /// **Including one case that is not quite that, and is worth knowing about.**
    /// A run that paused on a question is recorded with the status `completed` —
    /// only `awaiting_approval` and `awaiting_plan` are recorded as `paused` — so
    /// a question another process answered out of band, through `Attach`, leaves
    /// a run that reads as finished here while io-harness would still drive it.
    /// That is why the drivers check their own pending row before they consult
    /// this classification: an operator in that position needs to be told the
    /// question was answered, not that the run has ended.
    Finished,
}

/// What one resume did, built from what the harness returned and from the one
/// number that has to be read before it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Resumed {
    /// The run that was driven.
    pub run_id: i64,
    /// Why it stopped this time.
    pub outcome: RunOutcome,
    /// The last step that had committed **before** anything was driven, read off
    /// the store at the moment the decision was taken. The first step this
    /// resume drove is this plus one, and a caller that wants to assert where the
    /// run picked up should do that arithmetic rather than trust a literal: the
    /// number depends on how much the previous process got through, which is not
    /// something a test fixture decides.
    pub resumed_after: u32,
    /// The turn this closed, or `None` for a bare run that served no turn.
    pub turn_id: Option<i64>,
    /// What the agent said, as filed on the turn. `None` when the run's last
    /// word was a tool call rather than a message.
    pub reply: Option<String>,
}

/// Why a resume did not happen, or did not finish.
///
/// Every variant is a distinct thing an operator can act on. [`Self::HeadMoved`]
/// is the one that happens *after* the run has already been driven, and it is
/// kept apart from the refusals for that reason: the work was done and paid for,
/// and a surface that reported it as "the resume was refused" would invite the
/// operator to try again and pay twice.
#[derive(Debug)]
pub enum Failure {
    /// The turn was interrupted by the operator, so it is finished rather than
    /// paused and no resume entry point will drive it.
    Interrupted {
        /// The run that was asked about.
        run_id: i64,
    },
    /// The run ended. There is nothing waiting and nothing left to drive.
    Ended {
        /// The run that was asked about.
        run_id: i64,
    },
    /// No run with this id is in the store.
    Unknown {
        /// The id that was asked about.
        run_id: i64,
    },
    /// No question with this id is in the store.
    NoSuchQuestion {
        /// The id that was asked about.
        question_id: i64,
    },
    /// The question is real and belongs to a run that is not this one — and, for
    /// a contained resume, not to any run in this tree either.
    QuestionElsewhere {
        /// The question that was named.
        question_id: i64,
        /// The run it actually belongs to.
        owner: i64,
        /// The run it was offered to.
        run_id: i64,
    },
    /// The question already has an answer, so the run has already acted on one.
    /// Driving it again from a second answer is the silent double-answer the
    /// store's compare-and-swap exists to make impossible.
    QuestionAnswered {
        /// The question that was named.
        question_id: i64,
    },
    /// No plan with this id is in the store.
    NoSuchPlan {
        /// The id that was asked about.
        plan_id: i64,
    },
    /// The plan is real and belongs to another run.
    PlanElsewhere {
        /// The plan that was named.
        plan_id: i64,
        /// The run it actually belongs to.
        owner: i64,
        /// The run it was offered to.
        run_id: i64,
    },
    /// The plan already has a verdict.
    PlanDecided {
        /// The plan that was named.
        plan_id: i64,
    },
    /// The attempt is not open on this run: it was never opened, belongs to
    /// another run, or has already been decided.
    AttemptNotOpen {
        /// The attempt that was named.
        attempt_id: i64,
        /// The run it was offered to.
        run_id: i64,
    },
    /// A contained resume was asked for against a run that is not the top of its
    /// own tree. Containment is a property of a whole tree, so it can only be
    /// applied at the root.
    NotTheRoot {
        /// The run that was named.
        run_id: i64,
        /// The root of its tree, which is what a contained resume takes.
        root: i64,
    },
    /// A contained run's interrupted call has no entry point in this harness.
    /// See this module's documentation.
    ContainedRecovery {
        /// The run that was named.
        run_id: i64,
    },
    /// Nothing recorded the boundary this run executed under, and none was
    /// supplied, so resuming it would mean choosing one on the operator's behalf.
    NoRecordedPolicy {
        /// The run that was named.
        run_id: i64,
    },
    /// The run was driven and its turn was closed, but the session head had moved
    /// on since the resume was decided, so the compare-and-swap refused. The
    /// answer landed; where the conversation continues from did not change.
    HeadMoved {
        /// The session whose head was being moved.
        session_id: i64,
        /// The turn this resume closed, which is where the head would have gone.
        turn_id: i64,
    },
    /// Anything io-harness reported that this module has nothing better to say
    /// about.
    Harness(Error),
}

impl std::fmt::Display for Failure {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Interrupted { run_id } => write!(
                f,
                "run {run_id} was stopped by you rather than paused, so there is nothing \
                 waiting for an answer and no way to carry it on. Use /fork from the turn \
                 before it to take the conversation somewhere else."
            ),
            Self::Ended { run_id } => {
                write!(f, "run {run_id} has ended; there is nothing to continue")
            }
            Self::Unknown { run_id } => {
                write!(f, "no run {run_id} in this store")
            }
            Self::NoSuchQuestion { question_id } => {
                write!(f, "no question {question_id} in this store to answer")
            }
            Self::QuestionElsewhere {
                question_id,
                owner,
                run_id,
            } => write!(
                f,
                "question {question_id} was asked by run {owner}, not by run {run_id}; \
                 answering it here would deliver your answer into another run"
            ),
            Self::QuestionAnswered { question_id } => write!(
                f,
                "question {question_id} has already been answered, and the run has already \
                 acted on that answer; a second answer would drive it again from a decision \
                 nothing recorded"
            ),
            Self::NoSuchPlan { plan_id } => {
                write!(f, "no plan {plan_id} in this store to decide")
            }
            Self::PlanElsewhere {
                plan_id,
                owner,
                run_id,
            } => write!(
                f,
                "plan {plan_id} was proposed by run {owner}, not by run {run_id}; deciding it \
                 here would authorise somebody else's work"
            ),
            Self::PlanDecided { plan_id } => write!(
                f,
                "plan {plan_id} has already been decided, and the run has already moved on it"
            ),
            Self::AttemptNotOpen { attempt_id, run_id } => write!(
                f,
                "run {run_id} has no open call {attempt_id}: it was never started, belongs to \
                 another run, or somebody has already said what happened to it"
            ),
            Self::NotTheRoot { run_id, root } => write!(
                f,
                "run {run_id} is a child of run {root}; a limit on how far a fleet may spread \
                 belongs to the whole tree, so resume it from {root}"
            ),
            Self::ContainedRecovery { run_id } => write!(
                f,
                "run {run_id} is part of a fleet and stopped on a call whose outcome is \
                 unknown. This harness has no way to decide that call and keep the fleet's \
                 limits, and carrying on without them is not something this release will do \
                 quietly."
            ),
            Self::NoRecordedPolicy { run_id } => write!(
                f,
                "nothing recorded what run {run_id} was allowed to do, so carrying it on \
                 would mean choosing its boundary for you; say which policy it ran under"
            ),
            Self::HeadMoved { session_id, turn_id } => write!(
                f,
                "the run was carried on and turn {turn_id} holds its answer, but this \
                 conversation had already moved on somewhere else, so session {session_id} \
                 still points where it did. Nothing was lost; the answer is on that turn."
            ),
            Self::Harness(error) => write!(f, "{error}"),
        }
    }
}

impl std::error::Error for Failure {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Harness(error) => Some(error),
            _ => None,
        }
    }
}

impl From<Error> for Failure {
    fn from(error: Error) -> Self {
        Self::Harness(error)
    }
}

/// Read what `run_id` stopped on, driving nothing.
///
/// A handful of store reads and no provider call, so a picker can classify every
/// parked run in a store without spending anything or taking a lease.
///
/// **The order of the tests is the whole of the correctness here.** An
/// interrupted run is asked about first, because `Ctrl+C` writes the outcome
/// `cancelled` while leaving whatever pending row the run happened to be holding
/// exactly where it was — so a run tested for a question before it is tested for
/// an interruption reads as answerable, and offering it would produce a resume
/// that returns the original outcome having driven nothing. The pending rows come
/// next and the status last, because a run paused on an answered-by-nobody
/// question is recorded with the status `completed`: `Store::finish_run` maps
/// only `awaiting_approval` and `awaiting_plan` to `paused`, so the status alone
/// would report the commonest pause of all as an ended run.
pub fn pending_for(store: &Store, run_id: i64) -> Result<Pending, Error> {
    let Some(status) = store.run_status(run_id)? else {
        return Err(Error::Resume {
            reason: format!("no run with id {run_id} in the store"),
        });
    };

    // The raw outcome string rather than the status, and they are different
    // columns: `Store::status` answers `running`/`paused`/`completed`/`failed`
    // and `Store::outcome` answers what the loop actually recorded. Only the
    // second can tell a cancelled run from any other completed one.
    if store.outcome(run_id)?.as_deref() == Some(CANCELLED) {
        return Ok(Pending::Interrupted);
    }

    if let Some(question) = unresolved_question(store, run_id)? {
        return Ok(Pending::Question {
            question_id: question.id,
            question: question.question,
            context: question.context,
            choices: question.choices,
            step: question.step,
        });
    }
    if let Some(plan) = store.plans(run_id)?.into_iter().find(|p| !p.resolved) {
        return Ok(Pending::Plan {
            plan_id: plan.id,
            steps: plan.plan.steps,
            step: plan.step,
        });
    }
    if let Some(attempt) = store.open_attempts(run_id)?.into_iter().next() {
        return Ok(Pending::Recovery {
            attempt_id: attempt.id,
            tool: attempt.tool,
            step: attempt.step,
        });
    }

    // A run still marked `running` with committed work is one whose process went
    // away: the loop writes an outcome on every ending it reaches, including the
    // ones it escalates on, so `running` after the fact means it reached none of
    // them. With no committed step there is nothing to carry on from either — the
    // run is a row and an intention — so it is reported as finished rather than
    // offered.
    let last_step = store.last_step(run_id)?;
    match status {
        RunStatus::Running if last_step > 0 => Ok(Pending::Died { last_step }),
        _ => Ok(Pending::Finished),
    }
}

/// The prompt the operator typed for the turn `run_id` served, when it served
/// one.
///
/// The only way to recover a goal from this side of the crate boundary:
/// `runs.goal` has no public reader, so a run that served no session turn cannot
/// have its goal recovered at all and the caller has to supply it. `None` says
/// exactly that rather than substituting something plausible.
pub fn goal_for(store: &Store, run_id: i64) -> Result<Option<String>, Error> {
    let Some(turn_id) = store.turn_for_run(run_id)? else {
        return Ok(None);
    };
    Ok(store.session_turn(turn_id)?.map(|turn| turn.prompt))
}

/// Carry on a run whose agent asked the operator something.
///
/// The answer travels as an argument and is written by the harness's own
/// compare-and-swap on the way in, which is what makes "was it me who answered"
/// answerable. Never write it through `Attach` first: on a run that has already
/// stopped that resolves the row, and the swap inside the resume then finds
/// nothing to answer and fails.
///
/// `containment` turns this into a tree resume, which is the arm that matters
/// when a *child* asked: io-harness resolves the question against its own run
/// rather than against the root, so a question belonging to any run in the tree
/// is answerable here and one belonging to another tree is refused.
#[allow(clippy::too_many_arguments)]
pub async fn answer_question<P: Provider>(
    contract: &TaskContract,
    provider: &P,
    store: &Store,
    run_id: i64,
    question_id: i64,
    answer: &str,
    policy: &Policy,
    approver: &dyn io_harness::Approver,
    containment: Option<&Containment>,
    observer: &dyn Observer,
    expected_head: Option<i64>,
) -> Result<Resumed, Failure> {
    // The question is checked before the run is, and the order is the difference
    // between a useful sentence and a true one. A question somebody already
    // answered leaves its run reading as ended — `Store::finish_run` records a
    // question pause as `completed` — so a guard that asked about the run first
    // would answer "this run has ended" to an operator whose actual problem is
    // that their colleague answered it thirty seconds ago.
    let question = store
        .question(question_id)?
        .ok_or(Failure::NoSuchQuestion { question_id })?;
    // A flat resume records the answer against `run_id` itself; a tree resume
    // looks the question's own run up first, so a child's question is answerable
    // from the root. The check follows the same two shapes rather than a single
    // stricter one, which would refuse exactly the case the tree arm exists for.
    let owned = match containment {
        Some(_) => store.run_root(question.run_id)? == run_id,
        None => question.run_id == run_id,
    };
    if !owned {
        return Err(Failure::QuestionElsewhere {
            question_id,
            owner: question.run_id,
            run_id,
        });
    }
    if question.resolved {
        return Err(Failure::QuestionAnswered { question_id });
    }
    refuse_ended(store, run_id)?;
    root_or_refuse(store, run_id, containment)?;

    let resumed_after = store.last_step(run_id)?;
    let result = match containment {
        Some(containment) => {
            io_harness::resume_tree_with_answer_observed(
                contract,
                provider,
                store,
                run_id,
                question_id,
                answer,
                policy,
                approver,
                containment,
                observer,
            )
            .await?
        }
        None => {
            io_harness::resume_with_answer_observed(
                contract,
                provider,
                store,
                run_id,
                question_id,
                answer,
                policy,
                approver,
                observer,
            )
            .await?
        }
    };
    finish(store, result.run_id, result.outcome, resumed_after, expected_head)
}

/// Carry on a run whose agent proposed an approach.
///
/// [`PlanVerdict::Cancel`] goes through this function like the other two and is
/// **not** mapped onto a plain resume. The plan entry point finishes the run as
/// `PlanRejected` without re-entering the loop, which is the difference between
/// "the operator said no" and "the operator said no and the agent was asked to
/// try again anyway" — the second costs the rest of the budget pursuing an
/// approach that was just refused.
///
/// [`PlanVerdict::Revise`] carries the operator's correction into the run as an
/// observation and the loop continues, so it is a resume in the ordinary sense.
///
/// Unlike a question, a plan is decided against the run that proposed it even on
/// the tree arm: io-harness records the verdict against the id it is given rather
/// than looking the owner up, so a child's plan cannot be decided from the root.
#[allow(clippy::too_many_arguments)]
pub async fn decide_plan<P: Provider>(
    contract: &TaskContract,
    provider: &P,
    store: &Store,
    run_id: i64,
    plan_id: i64,
    verdict: PlanVerdict,
    policy: &Policy,
    approver: &dyn io_harness::Approver,
    containment: Option<&Containment>,
    observer: &dyn Observer,
    expected_head: Option<i64>,
) -> Result<Resumed, Failure> {
    // Plan first, run second, for the reason [`answer_question`] gives.
    let plan = store.plan(plan_id)?.ok_or(Failure::NoSuchPlan { plan_id })?;
    if plan.run_id != run_id {
        return Err(Failure::PlanElsewhere {
            plan_id,
            owner: plan.run_id,
            run_id,
        });
    }
    if plan.resolved {
        return Err(Failure::PlanDecided { plan_id });
    }
    refuse_ended(store, run_id)?;
    root_or_refuse(store, run_id, containment)?;

    let resumed_after = store.last_step(run_id)?;
    let result = match containment {
        Some(containment) => {
            io_harness::resume_tree_with_plan_decision_observed(
                contract,
                provider,
                store,
                run_id,
                plan_id,
                verdict,
                policy,
                approver,
                containment,
                observer,
            )
            .await?
        }
        None => {
            io_harness::resume_with_plan_decision_observed(
                contract,
                provider,
                store,
                run_id,
                plan_id,
                verdict,
                policy,
                approver,
                observer,
            )
            .await?
        }
    };
    finish(store, result.run_id, result.outcome, resumed_after, expected_head)
}

/// Carry on a run that stopped on a call nobody can tell landed or not.
///
/// [`RecoveryDecision::Completed`] is the one that needs a word. The operator's
/// account of what the call returned is filed by io-harness against the step the
/// *attempt* was made on, not against the step the run has now reached — so the
/// resumed run assembles a transcript in which the tool answered where it was
/// asked, which is the truth. Nothing validates the text: the operator is
/// asserting a fact about the outside world that no code here can check.
///
/// **`containment` is accepted only to be refused.** See this module's
/// documentation: there is no tree-aware recovery entry point in this harness,
/// and driving a tree root through the flat one would drop the fleet's limits
/// without saying so. The parameter is here rather than absent so that a caller
/// holding a fleet's limits is told they cannot be kept, instead of quietly
/// getting a flat resume from a signature that never offered to take them.
#[allow(clippy::too_many_arguments)]
pub async fn recover<P: Provider>(
    contract: &TaskContract,
    provider: &P,
    store: &Store,
    run_id: i64,
    attempt_id: i64,
    decision: RecoveryDecision,
    policy: &Policy,
    approver: &dyn io_harness::Approver,
    containment: Option<&Containment>,
    observer: &dyn Observer,
    expected_head: Option<i64>,
) -> Result<Resumed, Failure> {
    // A run with a parent is under somebody's containment whether or not this
    // caller was holding the value, so both roads to a contained run are refused
    // here rather than one of them being driven flat.
    if containment.is_some() || store.run_root(run_id)? != run_id {
        return Err(Failure::ContainedRecovery { run_id });
    }
    if !store
        .open_attempts(run_id)?
        .iter()
        .any(|attempt| attempt.id == attempt_id)
    {
        return Err(Failure::AttemptNotOpen { attempt_id, run_id });
    }
    refuse_ended(store, run_id)?;

    let resumed_after = store.last_step(run_id)?;
    let result = io_harness::resume_with_recovery_observed(
        contract, provider, store, run_id, attempt_id, decision, policy, approver, observer,
    )
    .await?;
    finish(store, result.run_id, result.outcome, resumed_after, expected_head)
}

/// Carry on a run whose process went away, with nothing to decide.
///
/// `policy` is `None` for a run whose boundary this caller cannot rebuild, and
/// that is the case the stored-policy entry point exists for: io-harness reads
/// the policy the run was started under back off its own row. It fails where
/// nothing recorded one — a run written by an older harness — and that failure is
/// reported as [`Failure::NoRecordedPolicy`] rather than as a resume error,
/// because the answer is for the operator to say what the run was allowed to do
/// rather than to try again.
#[allow(clippy::too_many_arguments)]
pub async fn carry_on<P: Provider>(
    contract: &TaskContract,
    provider: &P,
    store: &Store,
    run_id: i64,
    policy: Option<&Policy>,
    approver: &dyn io_harness::Approver,
    containment: Option<&Containment>,
    observer: &dyn Observer,
    expected_head: Option<i64>,
) -> Result<Resumed, Failure> {
    refuse_ended(store, run_id)?;
    root_or_refuse(store, run_id, containment)?;
    if policy.is_none() && store.run_policy(run_id)?.is_none() {
        return Err(Failure::NoRecordedPolicy { run_id });
    }

    let resumed_after = store.last_step(run_id)?;
    let result = match (containment, policy) {
        (Some(containment), Some(policy)) => {
            io_harness::resume_tree_observed(
                contract, provider, store, run_id, policy, approver, containment, observer,
            )
            .await?
        }
        (Some(containment), None) => {
            io_harness::resume_tree_from_stored_policy_observed(
                contract, provider, store, run_id, approver, containment, observer,
            )
            .await?
        }
        (None, Some(policy)) => {
            io_harness::resume_with_observed(
                contract, provider, store, run_id, policy, approver, observer,
            )
            .await?
        }
        (None, None) => {
            io_harness::resume_from_stored_policy_observed(
                contract, provider, store, run_id, approver, observer,
            )
            .await?
        }
    };
    finish(store, result.run_id, result.outcome, resumed_after, expected_head)
}

/// The outcome string the run loop writes when an observer stops a run, which is
/// what `Ctrl+C` does. Named once here because it is the difference between a
/// turn that can be carried on and one that cannot, and a literal repeated at
/// each test would be a literal that could drift from the one this module reads.
pub const CANCELLED: &str = "cancelled";

/// The first unanswered question of a run, if it holds one.
fn unresolved_question(store: &Store, run_id: i64) -> Result<Option<PendingQuestion>, Error> {
    Ok(store.questions(run_id)?.into_iter().find(|q| !q.resolved))
}

/// Refuse a run that no resume entry point will drive.
///
/// Every refusal here is a cheap read and every one of them happens before a
/// lease is taken or a provider is built, so an operator who picks the one turn
/// in their history that cannot be continued pays nothing to find out.
fn refuse_ended(store: &Store, run_id: i64) -> Result<(), Failure> {
    // Asked before `pending_for` rather than by reading the error it raises for
    // an unknown run: a caller who typed a number wrong should be told that in
    // those words, not handed a resume error whose reason they have to parse.
    if store.run_status(run_id)?.is_none() {
        return Err(Failure::Unknown { run_id });
    }
    match pending_for(store, run_id)? {
        Pending::Interrupted => Err(Failure::Interrupted { run_id }),
        Pending::Finished => Err(Failure::Ended { run_id }),
        _ => Ok(()),
    }
}

/// A containment applies to a whole tree, so refuse one offered against a child.
fn root_or_refuse(
    store: &Store,
    run_id: i64,
    containment: Option<&Containment>,
) -> Result<(), Failure> {
    if containment.is_none() {
        return Ok(());
    }
    let root = store.run_root(run_id)?;
    if root == run_id {
        return Ok(());
    }
    Err(Failure::NotTheRoot { run_id, root })
}

/// Close the session bookkeeping a free resume does not do.
///
/// The four drivers all end here, and they end here rather than each doing their
/// own half because every one of the three writes below is a write that only
/// `Session::drive` makes and `Session::drive` is private. A resume that skipped
/// them leaves the turn reading as though it were still waiting: an operator
/// scrolling back sees the question they answered an hour ago still open, and the
/// next turn is parented onto a head that never moved.
///
/// `None` for the turn is not a failure. A run started by [`crate::exec`] or by
/// any caller that did not go through a `Session` served no turn, and there is
/// nothing to close — which is a fact about that run rather than a problem with
/// this one.
///
/// **The head write is a compare-and-swap and never the unconditional one.** Two
/// processes on one store both moving a head is a lost update that errors
/// nowhere: the second write wins, and the first process's turn stays in
/// `session_turns` with its parent intact but off the head path — answered,
/// billed, and invisible to the next turn. `set_session_head_if` reports that as
/// `Error::Conflict`, and it is given its own [`Failure::HeadMoved`] here rather
/// than folded into the harness errors, because the two halves of the sentence
/// an operator needs are opposite: the resume worked, and the conversation did
/// not move.
fn finish(
    store: &Store,
    run_id: i64,
    outcome: RunOutcome,
    resumed_after: u32,
    expected_head: Option<i64>,
) -> Result<Resumed, Failure> {
    let Some(turn_id) = store.turn_for_run(run_id)? else {
        return Ok(Resumed {
            run_id,
            outcome,
            resumed_after,
            turn_id: None,
            reply: None,
        });
    };

    let reply = last_said(&store.step_turns(run_id)?);
    // The outcome string the store itself wrote, read back rather than derived
    // here from the `RunOutcome` this function was handed. Two spellings of one
    // ending is how a transcript and an audit come to disagree about what
    // happened, and the store's is the one every other reader sees. `running` for
    // a run that paused again: `Store::finish_run` writes no summary until a run
    // really ends, which is exactly what a turn waiting for a second answer is.
    let recorded = store
        .run_summary(run_id)?
        .map(|summary| summary.outcome)
        .unwrap_or_else(|| "running".to_string());
    store.finish_turn(turn_id, reply.as_deref(), &recorded)?;

    let session_id = store
        .session_turn(turn_id)?
        .ok_or_else(|| Error::Resume {
            reason: format!("run {run_id} names turn {turn_id}, which the store does not hold"),
        })?
        .session_id;
    match store.set_session_head_if(session_id, expected_head, Some(turn_id)) {
        Ok(()) => {}
        Err(Error::Conflict { .. }) => return Err(Failure::HeadMoved { session_id, turn_id }),
        Err(error) => return Err(Failure::Harness(error)),
    }

    Ok(Resumed {
        run_id,
        outcome,
        resumed_after,
        turn_id: Some(turn_id),
        reply,
    })
}

/// The last thing the agent wrote, taken off the run's own recorded turns.
///
/// **Read from the store and never reconstructed.** io-harness extracts the same
/// text by splitting an observation on a sentinel that is `pub(crate)`, so the
/// only way to reproduce it here would be to hardcode the literal — a string this
/// crate cannot see change, in a function that would go on returning a plausible
/// answer for years after it stopped being the right one.
///
/// Scanning backwards for the last turn that *said* something, rather than taking
/// the last turn and accepting its `None`, matches what io-harness's own
/// extraction does: it walks the ledger in reverse for the last message. A
/// resumed run whose final step was a tool call therefore reports the last thing
/// the agent actually wrote, which is what an operator reading the turn expects
/// to find there.
fn last_said(turns: &[AssistantTurn]) -> Option<String> {
    turns.iter().rev().find_map(|turn| {
        let said = turn.text.as_deref()?.trim();
        (!said.is_empty()).then(|| said.to_string())
    })
}
