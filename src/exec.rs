//! `io exec` — one goal, run to completion, with no terminal — and, since
//! 0.23.0, `io resume`, which carries on a run that stopped for a person.
//!
//! The second subcommand lives here rather than in [`crate::resume`] for the
//! reason the first one lives here rather than in `main.rs`: that module is the
//! library half — it classifies a pause and drives the harness's resume entry
//! points — and this one is the half that reads a command line, chooses a
//! provider, writes to two streams and returns an exit status. The exit statuses
//! are the same six, through the same [`code`] table, because a resumed run ends
//! the way any other run ends.
//!
//! This is a second **consumer** of io-harness, not a second program. It opens
//! the same store an interactive session opens, creates a session in it the same
//! way, hands the harness a policy the same way, and reads the same events back.
//! What it does not do is draw: nothing in this module reaches the renderer, the
//! composer, the picker or the theme, and `tests/exec.rs` asserts that rather
//! than trusting it.
//!
//! **It takes the contract-shaped entry point, and it stopped being the only arm
//! that does.** Through 0.13.1 this module was the only place `[sandbox]` limits
//! and `[run]` budgets had any effect, and the reason given was that an
//! interactive turn went through `Session::turn_steered`, which builds its own
//! `TaskContract` internally in order to accept a steer inbox. That reason went
//! stale in 0.11.0, when the flat turn moved to
//! `Session::turn_bounded_observed` — which takes a caller's contract — and
//! `Ctrl+C` survived the move because an interrupt travels as `Flow::Cancel` on
//! the observer rather than through a steer inbox. 0.14.0 deletes what was left
//! of the asymmetry: [`contract`] and `contract::session` are both built from
//! [`crate::contract::configured`], so what the file says reaches either arm.
//!
//! **And the trade that started it is gone entirely.** Since io-harness 0.67.0 a
//! session turn takes a caller's contract *and* a `SteerInbox` on one call, so
//! nothing is given up for either — an interactive turn is driven through
//! `Session::turn_bounded_steered` or `Session::turn_contained_bounded_steered`
//! and can be spoken to while it runs. This module still takes
//! `turn_bounded_observed`: there is no operator at a keyboard to say anything,
//! and an inbox nobody can write to is a parameter with no sender.

use std::io::Write;
use std::sync::Mutex;

use io_harness::{
    Config, DenyAll, ExecMode, Flow, Ignore, Observer, PlanVerdict, Policy, Provider, ProviderSpec,
    RecoveryDecision, RunEvent, RunOutcome, Session, Store, TaskContract, TurnResult,
};

use crate::cli::{PlanFlag, PolicyFlag, RecoveryFlag};
use crate::provider::{self, WithProvider};
use crate::resume::Pending;
use crate::settings::{self, Posture};

/// The run ended of its own accord.
pub const OK: u8 = 0;
/// It never got that far: a bad credential, no provider, an unreadable
/// configuration, a harness error.
pub const FAILED: u8 = 1;
/// A boundary said no. Nothing is broken; the policy did its job.
pub const REFUSED: u8 = 2;
/// A ceiling was reached — steps, time, tokens, or the tree's shared budget.
pub const CEILING: u8 = 3;
/// The run stopped needing a human.
pub const PAUSED: u8 = 4;
/// It ended without finishing.
pub const UNFINISHED: u8 = 5;
/// The agent finished and the work does not hold up.
///
/// Added in 0.24.0, the release that let an operator say what "done" means. It
/// renumbers nothing: the six below have meant what they mean since 0.5.0, and
/// this is the first ending this subcommand has ever been able to tell apart
/// from them.
///
/// **io-harness#212 is closed, and 0.70.0 is where this code stops being an
/// inference.** Until that release the harness had no `RunOutcome` variant for a
/// run whose criterion answered no: such a run surfaced as `StepCapReached` —
/// the criterion is re-evaluated after every step and the loop only ends early
/// when it *passes*, so a gate that never passes spends the whole budget — and
/// the verdict had to be read from the store afterwards by [`verified_code`].
/// 0.70.0 adds `RunOutcome::VerificationFailed { steps }` for exactly that run,
/// and narrows `StepCapReached` to mean only that nothing judged the work.
///
/// So [`code`] now decides it directly, and [`verified_code`] stays as the
/// second route rather than the only one. **The two must agree, and they do:**
/// both answer this constant. Keeping the store route is not redundancy — a
/// gate can fail on a run that ended for some other reason entirely, and only
/// the recorded `GateOutcome` sees that one.
pub const UNVERIFIED: u8 = 6;

/// The exit status for `io mcp probe`.
///
/// **A probe that came back with nothing must not exit zero**, and until this
/// function existed it did: the sentence on stdout was right and the status was
/// `0` for every outcome, so a script could tell an answering server from a dead
/// one only by parsing prose. Found by running the built binary against a real
/// server, not by a test — every offline gate was green, because every offline
/// gate asserted the sentence.
///
/// Three statuses rather than two, and the third earns its place: a server the
/// **policy** refused is a different problem from one that was asked and did not
/// answer. The first is fixed by editing a rule and the second by fixing the
/// server, and a script that retries is right to retry only one of them.
///
/// * [`OK`] — it answered.
/// * [`REFUSED`] — the policy would not let it start. Nothing was spawned or
///   dialled.
/// * [`FAILED`] — everything else: switched off, would not start, unreachable,
///   timed out, or a state a newer io-harness reports that this build does not
///   model.
///
/// The `_` arm is mandatory: `McpProbe` is `#[non_exhaustive]`. It answers
/// `FAILED` because "this build does not know what happened" is not a success,
/// and the sentence beside it says so in words.
#[must_use]
pub fn probe_code(probe: &io_harness::McpProbe) -> u8 {
    match probe {
        io_harness::McpProbe::Answered { .. } => OK,
        io_harness::McpProbe::Refused { .. } => REFUSED,
        _ => FAILED,
    }
}

/// The exit status for a run that reached the harness.
///
/// **The `_` arm is mandatory now, and it is not free.** Until io-harness 0.64
/// `RunOutcome` was exhaustive, so a variant a later harness added broke this
/// build rather than being folded into one of the six codes, and that break was
/// the only thing keeping a table published as public contract true across a pin
/// bump. 0.65 made the enum `#[non_exhaustive]`; the compiler now insists on a
/// catch-all and has nothing left to say. The property moved to a test —
/// `the_outcome_table_names_every_outcome_the_locked_harness_declares` reads
/// the variants out of the locked source, so a new one fails a test that names it
/// instead of arriving here as `UNFINISHED`.
///
/// An unrecognised outcome maps to `UNFINISHED` because it is the only honest
/// answer: the run reached the harness and did not report finishing, and a code
/// that claimed success or a crash would both be inventions.
///
/// Two of the mappings are the release's research rather than its taste.
/// `Finished` is `OK` because a contract with no verification criterion returns
/// `Finished` and never `Success`, and a table that treated only `Success` as zero
/// would fail every successful ungated run. **0.24.0 makes `Success` reachable
/// and changes neither mapping**: an operator who configures `[app.io-cli.gates]`
/// gets a real criterion on the contract from [`crate::contract::configured`], and
/// a run that passes it comes back `Success`. Both still mean the run ended of its
/// own accord, which is what `0` says. And the four ceilings need codes at all
/// only because the harness returns them as `Ok`, so a status read off the
/// `Result` reports success on every one of them.
pub fn code(outcome: &RunOutcome) -> u8 {
    match outcome {
        RunOutcome::Success { .. } | RunOutcome::Finished { .. } => OK,

        RunOutcome::Denied { .. }
        | RunOutcome::Refused { .. }
        | RunOutcome::PlanRejected { .. } => REFUSED,

        RunOutcome::StepCapReached { .. }
        | RunOutcome::TimeBudgetExceeded { .. }
        | RunOutcome::CostBudgetExceeded { .. }
        | RunOutcome::BudgetCeilingReached { .. } => CEILING,

        // io-harness 0.70.0, and it is this crate's own issue #212 coming back
        // implemented. It is **not** a ceiling: the run did reach its step cap,
        // but the fact worth reporting is that the work was judged and did not
        // hold up, which is what `UNVERIFIED` has meant since 0.24.0. Mapping it
        // to `CEILING` would move exactly the runs that code was invented for
        // from 6 to 3 on a pin bump — a published exit table changing meaning
        // underneath an operator's CI. `StepCapReached` above now means only
        // that nothing judged the work, which is why the two can be told apart
        // here at all.
        RunOutcome::VerificationFailed { .. } => UNVERIFIED,

        // `AwaitingApproval` stays unreachable from here while approvals are
        // denied rather than deferred; the other two are reachable, because a
        // question about intent and a proposed plan pass through no approver at
        // all. **The bet these codes were numbered on has now been settled.**
        // 0.5.0 mapped all three when none of them could happen yet — its own
        // release notes say the mapping exists "so that adding that subcommand
        // later renumbers nothing". 0.23.0 added it, and nothing here moved.
        // (Written as 0.13.0 here until 0.23.0, which was wrong: 0.13.0 is a
        // later release and the CHANGELOG puts the sentence in 0.5.0's notes.)
        RunOutcome::AwaitingApproval { .. }
        | RunOutcome::AwaitingAnswer { .. }
        | RunOutcome::AwaitingPlan { .. } => PAUSED,

        // 0.65.0 — a resume that found a call started and never finished. It is a
        // pause needing a decision, so it belongs with the other three rather than
        // with the failures. **Not unreachable, and the reason given here was
        // wrong until 0.23.0.** It said a session turn registers no tool and no
        // MCP server and cannot journal an attempt. `crate::contract::session`
        // attaches `[[mcp]]` servers, an MCP tool declares no `recovery` so it
        // takes the harness's default of `Indeterminate`, and an attempt on one
        // is therefore journalable on an ordinary interactive turn. The
        // conclusion survives its reason: either arm reaches this outcome, and
        // `io exec` reaches it the same way, since both build their contract from
        // the same configuration.
        RunOutcome::AwaitingRecovery { .. } => PAUSED,

        RunOutcome::Stalled { .. }
        | RunOutcome::Escalated { .. }
        | RunOutcome::Cancelled { .. } => UNFINISHED,

        _ => UNFINISHED,
    }
}

/// The exit status once the operator's own criterion has had its say.
///
/// A wrapper around [`code`] rather than an arm inside it, and deliberately so.
/// `tests/exec.rs` extracts that function's body by splitting on its exact
/// signature line, and its table is a statement about `RunOutcome` alone — which
/// carries no verification verdict. Putting this decision inside it would both
/// reformat the one function a test reads as text and make a table about
/// outcomes answer a question outcomes do not contain.
///
/// **Only a `GateOutcome::Failed` earns [`UNVERIFIED`].** A gate that
/// `Errored` never answered at all — the criterion could not run, the reviewer
/// returned nothing parsable, the program was refused — and reporting that as
/// "the work does not hold up" would claim a judgement nobody made. Such a run
/// keeps whatever its outcome maps to, which is the honest report that it ran
/// and was not verified either way. A run with no criterion configured has no
/// standing at all and is untouched.
pub fn verified_code(outcome: &RunOutcome, standing: Option<&crate::gates::Standing>) -> u8 {
    match standing {
        Some(standing) if matches!(standing.outcome, io_harness::GateOutcome::Failed) => UNVERIFIED,
        _ => code(outcome),
    }
}

/// The layer io-harness 0.74.0 stamps on a refusal from its local-address floor.
///
/// **Matched rather than re-derived, and that is the whole design of this
/// notice.** The floor refuses a loopback, link-local, CGNAT, ULA or RFC 1918
/// provider endpoint — and `localhost`, `*.localhost`, `*.local` — before the
/// run's first step, *whatever the policy says*. io-cli could inspect the
/// configured endpoint and decide for itself which of those it is; it does not,
/// because 0.30.0 shipped a copy of one of this dependency's address checks and it
/// **failed open** on five shapes, including a URL where a bracketed host
/// swallowed the real one. A copy's test table is written from the copier's
/// imagination rather than from the original's bug list. Reading the layer off the
/// refusal the harness actually produced is right for every shape it refuses now
/// and every shape it adds later.
///
/// The string is spelled here because io-harness's own `FLOOR_LAYER` is
/// `pub(crate)`. `f11_the_local_address_floor_layer_is_spelled_as_io_harness_spells_it`
/// holds this against the locked source, so a rename upstream fails a test that
/// names it rather than silently losing the remedy.
pub const LOCAL_ADDRESS_FLOOR: &str = "local-address floor";

/// What to do about it, which the harness deliberately does not say.
///
/// io-harness gives this lift **no configuration key** on purpose: it is meant to
/// be an operator's explicit, per-invocation choice rather than something a file
/// can grant. io-cli respects that — it names the variable and sets nothing.
/// `f11_io_cli_never_sets_the_local_address_variable` is the gate, and it is a
/// source-text gate because an absence has no other site.
const LOCAL_ADDRESS_REMEDY: &str = "io: this is the harness's local-address floor, and no \
     configuration key lifts it. If you meant to reach a model on this machine — Ollama, LM \
     Studio, llama.cpp — set IO_HARNESS_ALLOW_LOCAL_ADDRESSES=1 for the run that should be \
     allowed out to it.";

/// A failure that reached the harness, carrying the exit status it earns.
///
/// **It exists because `to_string()` at the two headless doors threw away the one
/// thing the table above needs.** io-harness answers a boundary refusal with a
/// typed `Error::Refused`, and both doors flattened every harness error to a
/// `String` before an exit code was chosen — so `main.rs`, which has only
/// `Err(_) -> FAILED` to work with, exited `1` for a run the policy refused. That
/// is the same class of defect 0.34.1 removed from the other end of this table: a
/// script is told the wrong thing by the one surface this product offers to be
/// scripted against. Documented as a known defect by 0.34.1 rather than fixed,
/// because it is a behaviour change and 0.34.1 was a patch.
///
/// `message` is the error's own `Display` and nothing else, so the sentence an
/// operator reads is byte-identical to the one they read before — only the status
/// beside it moves.
#[derive(Debug)]
pub struct Ending {
    /// The exit status: [`REFUSED`] for a boundary refusal, [`FAILED`] for
    /// everything else.
    pub code: u8,
    /// What the operator is told, taken from the error itself.
    pub message: String,
}

impl std::fmt::Display for Ending {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.message)
    }
}

impl From<io_harness::Error> for Ending {
    /// **The arm is one variant wide, and that is the whole audit.**
    ///
    /// `Error::Refused` is the only variant io-harness types as a boundary
    /// saying no — its own documentation says it is "typed separately from
    /// `Error::Config` so a refusal is distinguishable from a malfunction". The
    /// two neighbours a wildcard would swallow are the two that must not move:
    /// `Error::Sandbox` is "the sandbox failed to start", typed apart from
    /// `Error::Io` so a caller can tell "the sandbox never ran the code" from
    /// "the code ran and failed"; and `Error::Config` is "configuration was
    /// missing or invalid", which the crate's own example handles as "fix the
    /// configuration". Reporting either as `REFUSED` would tell a CI job a
    /// boundary had spoken when nothing had — the mirror of the defect this
    /// removes, and the third time this table would have been given away by a
    /// convenient catch-all.
    ///
    /// So everything else — including every variant a later harness adds, which
    /// `#[non_exhaustive]` guarantees there will be — is [`FAILED`]: the run
    /// reached the harness and did not run, and only a refusal has a code of its
    /// own to claim.
    fn from(error: io_harness::Error) -> Self {
        Self {
            code: match &error {
                io_harness::Error::Refused { .. } => REFUSED,
                _ => FAILED,
            },
            message: match &error {
                io_harness::Error::Refused {
                    layer: Some(layer), ..
                } if layer == LOCAL_ADDRESS_FLOOR => {
                    format!("{error}\n{LOCAL_ADDRESS_REMEDY}")
                }
                _ => error.to_string(),
            },
        }
    }
}

impl From<crate::resume::Failure> for Ending {
    /// The resume door's own error type, which carries the harness's inside
    /// [`crate::resume::Failure::Harness`] and answers for the rest itself.
    ///
    /// Every other variant is io-cli's own refusal to drive a resume — a run that
    /// was interrupted, a question that belongs elsewhere, a head that moved —
    /// and each is [`FAILED`] for the reason `io exec --policy ask-writes` is:
    /// the boundary never got a chance to say anything.
    ///
    /// This door is not a copy of the other one's problem, it is the same problem
    /// with more ways in. All four drivers funnel into
    /// `io_harness::resume_with_observed`, which calls `authorize_provider` before
    /// it carries anything on — so the provider-endpoint refusal that reaches
    /// `io exec` reaches every resume too. io-harness 0.74.0 then adds two shapes
    /// only a resume can raise, a persisted approval it cannot replay and one
    /// rewritten since the checkpoint; `io` produces neither today, for the reason
    /// it produces no approval pause at all, and both are typed the same way and
    /// would land here already handled.
    fn from(failure: crate::resume::Failure) -> Self {
        match failure {
            crate::resume::Failure::Harness(error) => Self::from(error),
            other => Self {
                code: FAILED,
                message: other.to_string(),
            },
        }
    }
}

/// One line for stderr, naming the outcome and the harness's own step count.
///
/// The number is read off the returned value and never recounted from the store:
/// a count taken afterwards is true whether or not the run did what it says.
pub fn describe(outcome: &RunOutcome) -> String {
    let (what, steps) = match outcome {
        RunOutcome::Success { steps } => ("finished, and its verification passed", steps),
        RunOutcome::Finished { steps } => ("finished", steps),
        RunOutcome::StepCapReached { steps } => ("stopped at the step cap", steps),
        RunOutcome::VerificationFailed { steps } => (
            "stopped at the step cap, and its verification failed",
            steps,
        ),
        RunOutcome::TimeBudgetExceeded { steps } => ("stopped at the time budget", steps),
        RunOutcome::CostBudgetExceeded { steps } => ("stopped at the token budget", steps),
        RunOutcome::BudgetCeilingReached { steps } => {
            ("stopped at the tree's budget ceiling", steps)
        }
        RunOutcome::Denied { steps } => ("was denied", steps),
        RunOutcome::Refused { steps } => ("was refused before it began", steps),
        RunOutcome::PlanRejected { steps } => ("had its plan rejected", steps),
        RunOutcome::AwaitingApproval { steps, .. } => ("is waiting for an approval", steps),
        RunOutcome::AwaitingAnswer { steps, .. } => ("is waiting for an answer", steps),
        RunOutcome::AwaitingPlan { steps, .. } => ("is waiting for a plan decision", steps),
        RunOutcome::Stalled { steps } => ("stalled", steps),
        RunOutcome::Escalated { steps, .. } => ("escalated", steps),
        RunOutcome::Cancelled { steps } => ("was cancelled", steps),
        RunOutcome::AwaitingRecovery { steps, .. } => ("is waiting for a recovery decision", steps),
        // An outcome added by a later harness. Every arm above reads its own
        // `steps` field out of its own variant, and there is no field to read
        // here — so this returns early rather than printing a count of zero,
        // which would be a number this build made up.
        _ => return "the run ended in a way this build does not have a name for".to_string(),
    };
    let plural = if *steps == 1 { "step" } else { "steps" };
    format!("the run {what}, after {steps} {plural}")
}

/// The `--json` stream: one `RunEvent` per line.
///
/// **No shape is defined here.** The line is `serde_json::to_string` of
/// io-harness's own `RunEvent`, which derives `Serialize` in that crate with
/// `kind` flattened over an `event` tag — so a line reads
/// `{"run_id":…,"step":…,"depth":…,"event":"step",…}`. It is the same
/// serialization io-harness's `[[hook]]` writer appends to a file and the same
/// string its store keeps in the `json` column of its `run_events` table; a
/// consumer that can read one can read all three.
///
/// That is also why this forwards rather than matches. `EventKind` is
/// `#[non_exhaustive]` and io-cli's renderer names only some of its variants; a
/// struct of io-cli's own with the fields the renderer knows would pass every
/// test written from the renderer's vocabulary and silently drop every kind the
/// renderer has no way to draw. The two counts that used to stand here are gone
/// rather than corrected: nothing checks a number written in prose, and both had
/// gone stale by the 0.71.0 pin.
///
/// The write is on the run's critical path, because `Observer::event` is
/// synchronous and runs on the run's own task. For a headless run that is the
/// right place for it: the stream *is* the output, and a consumer that reads
/// slowly should slow the run down rather than have events buffered without
/// bound or dropped. That is the opposite of the interactive `Bridge`, which
/// hands events to an unbounded channel precisely so a slow terminal cannot
/// stall the agent.
pub struct Ndjson<W: Write + Send> {
    out: Mutex<W>,
}

impl<W: Write + Send> Ndjson<W> {
    pub fn new(out: W) -> Self {
        Self {
            out: Mutex::new(out),
        }
    }

    /// The writer back, for a test that needs to read what was written.
    pub fn into_inner(self) -> W {
        self.out.into_inner().expect("the stream is not poisoned")
    }
}

impl<W: Write + Send> Observer for Ndjson<W> {
    fn event(&self, event: &RunEvent) -> Flow {
        // A serialization failure and a broken pipe are both reasons to stop
        // writing, and neither is a reason to cancel the run: the work is the
        // operator's, the stream is a report on it, and `Flow::Cancel` here would
        // let a closed `head -1` abort an agent mid-edit.
        if let Ok(line) = serde_json::to_string(event) {
            if let Ok(mut out) = self.out.lock() {
                let _ = writeln!(out, "{line}");
                let _ = out.flush();
            }
        }
        Flow::Continue
    }
}

/// The posture a `--policy` value names, or a reason it is refused.
///
/// **`ask-writes` cannot mean what it says without a person.** Its whole content
/// is `write: Ask, exec: Ask`, and the only thing that answers an ask in an
/// unattended run is a denial — so honouring it would silently turn *ask before
/// writes* into *deny writes*. That is not hypothetical: it is what this product
/// shipped through 0.1.0 and 0.1.1, recorded in `settings::Posture::detail`,
/// and 0.2.0 is the release that fixed it. Refusing the value is the honest
/// answer; a posture whose name lies is worse than one that is missing.
pub fn posture_for(flag: PolicyFlag) -> Result<Posture, String> {
    match flag {
        PolicyFlag::Workspace => Ok(Posture::Workspace),
        PolicyFlag::ReadOnly => Ok(Posture::ReadOnly),
        PolicyFlag::AskWrites => Err(format!(
            "`--policy {}` cannot be honoured without a terminal: nothing in a \
             headless run can answer an approval, so every write would be denied \
             rather than asked about. Use `--policy {}` or `--policy {}`.",
            Posture::AskWrites.short(),
            Posture::Workspace.short(),
            Posture::ReadOnly.short(),
        )),
    }
}

/// The policy a run is given: the file's, or the posture the flag names.
///
/// A posture replaces only the tier defaults. The layers stay, so the harness's
/// own `builtin-secrets` denials on `.env`, `*.pem` and the rest survive a flag
/// exactly as they survive a configuration file — `Config::policy` stacks onto
/// `Policy::default()` for the same reason.
pub fn policy_for(config: &Config, posture: Option<Posture>) -> Policy {
    let mut policy = config.policy().unwrap_or_default();
    if let Some(posture) = posture {
        policy.defaults = posture.defaults();
    }
    policy
}

/// The warning a policy that asks earns, before the run starts.
///
/// **This is the same defect F7 refuses for the flag, arriving by a different
/// route.** `--policy ask-writes` is rejected outright because nothing in a
/// headless run can answer an approval, so *ask* becomes *deny* without saying
/// so. A configuration file can express the identical posture — and it is the
/// one the wizard recommends and most people have — so refusing it would make
/// `io exec` unusable out of the box for exactly the operators who took the
/// safe advice.
///
/// So this discloses rather than prevents, which is the standard 0.4.0's rewind
/// set for a cost that cannot be designed away. The line names what will happen
/// and both ways to change it. `None` when nothing asks, so a run that is going
/// to work says nothing.
pub fn asks_nobody_can_answer(policy: &Policy) -> Option<String> {
    let write = policy.defaults.write == io_harness::Effect::Ask;
    let exec = policy.defaults.exec == io_harness::Effect::Ask;
    let what = match (write, exec) {
        (true, true) => "every write and command",
        (true, false) => "every write",
        (false, true) => "every command",
        (false, false) => return None,
    };
    Some(format!(
        "the configured posture asks before writes and nothing in a headless run \
         can answer, so {what} will be denied. Pass `--policy {}` to allow them, \
         or `--policy {}` to say so plainly.",
        Posture::Workspace.short(),
        Posture::ReadOnly.short(),
    ))
}

/// The extra line a paused run gets, naming the handle that addresses the pause
/// and — for three of the four — the invocation that acts on it.
///
/// The fourth is an approval, which no `io resume` entry point takes, so that arm
/// names the request and says so rather than offering a command that does not
/// exist. Written out here as well as in the arm because the unqualified version
/// of this sentence is what `docs/CONTRACT.md` and the headless guide both
/// carried until 0.34.1, and a summary that over-claims is how it got into two
/// pages in the first place.
///
/// **The run id is not that handle, and through 0.22.0 this line printed only the
/// run id.** Every resume entry point in io-harness takes a second number — the
/// question, the plan, the journalled call — and each is the row a compare-and-swap
/// is made against, which is what makes "was it me who answered" answerable at
/// all. The outcome carries that number and this function used to discard it, so
/// an operator was told where their run went and given nothing to reach it with.
///
/// `None` for every outcome that did not pause.
pub fn parked(outcome: &RunOutcome, run_id: i64) -> Option<String> {
    let (waiting_on, how) = match outcome {
        RunOutcome::AwaitingAnswer { question_id, .. } => (
            format!("question {question_id}"),
            format!("io resume {run_id} --answer \"<your answer>\""),
        ),
        RunOutcome::AwaitingPlan { plan_id, .. } => (
            format!("plan {plan_id}"),
            format!("io resume {run_id} --plan approve"),
        ),
        RunOutcome::AwaitingRecovery { attempt_id, .. } => (
            format!("call {attempt_id}, whose outcome nobody recorded"),
            format!("io resume {run_id} --recovery retry"),
        ),
        // The one pause with no `io resume` behind it. An approval is answered by
        // the person the run asked, at the terminal it asked from, and there is no
        // resume entry point that takes one — so this names the request and says
        // that rather than offering an invocation that does not exist.
        RunOutcome::AwaitingApproval { request_id, .. } => {
            return Some(format!(
                "run {run_id} is parked on approval {request_id}, which is answered \
                 at the terminal that asked for it and not by `io resume`"
            ))
        }
        _ => return None,
    };
    Some(format!(
        "run {run_id} is parked on {waiting_on}; carry it on with `{how}`"
    ))
}

/// What reaches stdout once the turn is done.
///
/// `None` means nothing at all. It exists as a function rather than as an `if`
/// inside the run because a decision inside the binary has no automated coverage
/// — no integration test links a binary — and this is the decision that keeps
/// `io exec --json | jq` working. Sabotaging an `if` there would fail no test;
/// sabotaging this fails one by name.
pub fn to_stdout(json: bool, reply: Option<&str>) -> Option<&str> {
    if json {
        // The stream is the output. A reply printed beside it is a line that is
        // not a JSON object, which is exactly what a reader chokes on.
        None
    } else {
        reply
    }
}

/// Run one goal to completion.
///
/// Separate from the printing so a test can assert on the outcome without
/// capturing a process's streams.
///
/// Eight arguments rather than a struct, matching `loop_over` on the other
/// entry point: every one of them is a distinct thing the harness needs and a
/// wrapper would only move the list somewhere else.
#[allow(clippy::too_many_arguments)]
pub async fn turn<P: Provider>(
    provider: &P,
    store: &Store,
    session: &mut Session,
    config: &Config,
    policy: &Policy,
    goal: String,
    sandbox: Option<ExecMode>,
    observer: &dyn Observer,
) -> Result<TurnResult, Ending> {
    session
        .turn_bounded_observed(
            &contract(config, session, goal, sandbox),
            provider,
            store,
            policy,
            // **This is about approvals and nothing else, and until 0.23.0 it
            // also spoke for questions, which it had no business doing.** An
            // approver decides an *ask* raised by the policy, and nothing in an
            // unattended run can answer one — so the harness's own documented
            // choice for a headless job is the right one: the ask becomes a
            // refusal the agent is told about and adapts to, exactly as a policy
            // refusal already does, and the run carries on. An approver that
            // blocked instead would hang forever.
            //
            // A question the agent asks the operator never reaches an approver at
            // all. It ends the run at `AwaitingAnswer` with the question written
            // to the store, which is a pause and not a refusal, and nothing here
            // adapts to anything — see `parked` above, which is the line that
            // names it and now names the way back in.
            &DenyAll,
            observer,
        )
        .await
        .map_err(Ending::from)
}

/// The task contract, assembled from the harness's defaults, the configuration
/// file, and the `--sandbox` flag.
///
/// **The config-derived half is [`crate::contract::configured`], which an
/// interactive session builds from too.** Both the order of precedence and the
/// two steps that are easy to assume wrongly — `Config::apply_to` applies
/// neither `[policy]` nor `[sandbox]`, and a `[sandbox]` attached where the file
/// has none imposes real ceilings on a run that asked for none — are documented
/// there rather than restated here, so there is one place either can be wrong.
///
/// 0.14.0 — a headless run takes io-cli's step floor as well, which it did not
/// before. `io exec` ran on io-harness's own twelve and reported the ceiling it
/// hit as `error: step_cap_reached` under half-finished work, with nobody
/// watching. A `[run] max_steps` in the file still beats the floor.
///
/// What stays here is the flag, because a flag is not a property of the project.
pub fn contract(
    config: &Config,
    session: &Session,
    goal: String,
    sandbox: Option<ExecMode>,
) -> TaskContract {
    // Through the same module as the session's, because the gate that keeps the
    // interactive path honest is a path allow-list and admits no exceptions for a
    // caller that happens to be short-lived.
    //
    // **Twice per run, not once, and the comment used to claim otherwise.** The
    // contract is built here and the hooks are built where the run is assembled,
    // and threading one resolution between the two would reshape `Headless`'s
    // construction for a process that resolves, runs one goal and exits. Bounded
    // and stated rather than claimed away — the adversarial review caught the
    // claim.
    let resolved = crate::resolved::Resolved::load(config);
    let contract = crate::contract::configured(
        goal,
        session.root().to_path_buf(),
        config,
        resolved.loaded(),
    );
    // The flag last, so it beats the file, and applied with `with_exec_mode`
    // rather than by replacing the whole `SandboxConfig` — the limits the file
    // set are the operator's and are not this flag's to discard.
    match sandbox {
        Some(mode) => contract.with_exec_mode(mode),
        None => contract,
    }
}

/// The line `--sandbox full-access` prints, or `None` for every other value.
///
/// It exists because `Config::from_toml` refuses `full-access` at project scope
/// while the typed builder carries no such guard — so this flag reaches a
/// setting a checked-in configuration file is not allowed to express. That is
/// correct, since a flag is a person and a file in a repository is not, but it
/// is said out loud rather than reached in silence. On stderr, so `--json`
/// stdout stays parseable.
pub fn widening(sandbox: Option<ExecMode>) -> Option<&'static str> {
    matches!(sandbox, Some(ExecMode::FullAccess))
        .then_some("--sandbox full-access: commands in this run are not confined to the workspace")
}

/// The provider a headless entry point runs on: the flag's, or the file's.
///
/// `--provider` wins over the file, and is the only path that works when there is
/// no file at all — which is the CI case, and the case where an interactive `io`
/// would open the wizard nobody can answer. Shared by [`main`] and
/// [`resume_main`] rather than written twice, because the sentence a run with no
/// provider gets is the one thing both of them are certain to hit on a machine
/// that has never run `io setup`.
///
/// **`--provider` replaces the whole chain rather than heading it.** An operator
/// naming a provider on the command line has said which endpoint this run uses;
/// silently keeping the file's fallbacks underneath would let a run they scoped to
/// one vendor spend at another. The file's own chain is head-plus-fallbacks, which
/// is what [`provider::chain_of`] answers.
pub fn spec_for(
    which: Option<crate::cli::FromEnv>,
    config: &Config,
    model_override: Option<&str>,
) -> Result<Vec<ProviderSpec>, String> {
    match (which, config.provider_spec().is_some()) {
        (Some(which), _) => {
            let (key_var, model_var) = which.vars();
            provider::spec_from(
                which,
                std::env::var(key_var).ok(),
                model_override
                    .map(str::to_string)
                    .or_else(|| std::env::var(model_var).ok()),
            )
            .map(|spec| vec![spec])
        }
        (None, true) => Ok(provider::chain_of(config)),
        (None, false) => Err(
            "no provider is configured; run `io setup`, or pass `--provider` with \
             its credential in the environment"
                .into(),
        ),
    }
}

/// A headless goal, with a prompt template expanded where one was named.
///
/// **`[run] templates` was never unapplied — it was applied on one door only, and
/// the limits page named the wrong thing as the gap.** `commands::templates` has
/// read the key through its own accessor since the palette learned templates, but
/// only an interactive session ever called it, so `io exec` ignored a key
/// `docs/config.example.toml` documents. A configuration key that works in the
/// terminal and silently does nothing in CI is the asymmetry this product deleted
/// in 0.14.0, arriving again through a different door.
///
/// **`/name` and nothing else, which is the session's own spelling.** A goal that
/// does not begin with `/` is a prompt and is passed through untouched — the
/// overwhelmingly common case, and one this must not change. A goal that does is
/// rendered through [`crate::commands::expand`], the *same* function the palette
/// calls, with the same empty argument list: two doors rendering one template two
/// ways is exactly the divergence this exists to remove.
///
/// The notice is returned rather than printed, because this module prints on
/// stderr from one place and a library function that writes to a stream is one no
/// test can read back.
pub fn goal_for(config: &Config, goal: &str) -> Result<(String, Option<String>), String> {
    let Some(name) = goal.strip_prefix('/') else {
        return Ok((goal.to_string(), None));
    };
    let (templates, notice) = crate::commands::templates(config);
    let rendered = crate::commands::expand(&templates, name)?;
    Ok((rendered, notice))
}

/// `io exec`, from the command line to an exit status.
pub async fn main(
    args: crate::cli::Exec,
    config: Config,
    root: std::path::PathBuf,
    model_override: Option<String>,
) -> Result<u8, String> {
    // Before a store is opened, a session is created or a provider is built, so
    // a refused posture costs nothing and leaves no run behind.
    let posture = args.policy.map(posture_for).transpose()?;

    // And before those too: a template that does not resolve is a goal that does
    // not exist, and a run started on one would spend a provider call to say so.
    let (goal, templates_notice) = goal_for(&config, &args.goal)?;
    if let Some(notice) = templates_notice {
        eprintln!("io: {notice}");
    }
    let args = crate::cli::Exec { goal, ..args };

    // **A bundle's own program, placed here as well as on the session's startup.**
    // This door returns from `main` before the interactive path resolves
    // anything, so the placement written there reaches neither headless door.
    for notice in crate::bundle_path::install_for(&config) {
        eprintln!("{notice}");
    }

    let spec = spec_for(args.provider, &config, model_override.as_deref())?;
    let store = settings::store_path().ok_or("no place to keep the run store")?;
    let store = Store::open(&store).map_err(|error| error.to_string())?;
    let session = Session::open(&store, &root).map_err(|error| error.to_string())?;
    let policy = policy_for(&config, posture);

    provider::build(
        spec,
        model_override,
        Headless {
            store,
            session,
            config,
            policy,
            args,
        },
    )
    .await?
}

/// The headless run, as something [`provider::build`] can run.
struct Headless {
    store: Store,
    session: Session,
    config: Config,
    policy: Policy,
    args: crate::cli::Exec,
}

impl WithProvider for Headless {
    type Out = Result<u8, String>;

    async fn call<P: Provider>(
        mut self,
        make: impl Fn(&str) -> Result<P, String>,
        model: String,
    ) -> Self::Out {
        let provider = make(&model)?;

        // The two observers are the whole difference between the modes. Built
        // here rather than inside `turn` so that a test can hand in a writer it
        // can read back.
        if let Some(line) = widening(self.args.sandbox.map(crate::cli::Sandbox::mode)) {
            eprintln!("io: {line}");
        }
        if let Some(line) = asks_nobody_can_answer(&self.policy) {
            eprintln!("io: {line}");
        }

        let json = Ndjson::new(std::io::stdout());
        let observer: &dyn Observer = if self.args.json { &json } else { &Ignore };

        // **The same composition the session arm builds, and it is here for the
        // reason the asymmetry this product deleted in 0.14.0 was a defect.** A
        // hook is configuration, and configuration that reaches a terminal and
        // not CI is worse than configuration that reaches neither: an audit log
        // set up for unattended runs would record nothing in exactly the place
        // nobody is watching a screen to notice. So `[[hook]]` runs under
        // `io exec` on the same terms.
        //
        // `Broadcast` is here too, and for `io exec` it is the more useful half:
        // a headless run is the one somebody else's process is most likely to
        // want to attach to.
        let resolved = crate::resolved::Resolved::load(&self.config);
        let hooks = crate::contract::hooks(&self.config, resolved.loaded(), self.session.root());
        let mut observers: Vec<&dyn Observer> = vec![observer];
        if let Some(hooks) = &hooks {
            observers.push(hooks);
        }
        let fanout = crate::fanout::Fanout::new(observers);
        let durable =
            crate::settings::store_path().and_then(|path| io_harness::Store::open(&path).ok());
        let broadcast = durable.map(|store| io_harness::Broadcast::new(store, &fanout));
        let watcher: &dyn Observer = match &broadcast {
            Some(broadcast) => broadcast,
            None => &fanout,
        };

        let result = turn(
            &provider,
            &self.store,
            &mut self.session,
            &self.config,
            &self.policy,
            self.args.goal.clone(),
            self.args.sandbox.map(crate::cli::Sandbox::mode),
            watcher,
        )
        .await;
        // **Printed here and returned as a status, rather than propagated as an
        // `Err`.** `main.rs` has one answer for an `Err` and it is [`FAILED`], so a
        // `?` on this line is what made a refused run exit `1`. The sentence is the
        // same sentence `main.rs` would have printed, on the same stream, with the
        // same prefix — only the number beside it is now the one the table
        // publishes.
        let result = match result {
            Ok(result) => result,
            Err(ending) => {
                eprintln!("io: {ending}");
                return Ok(ending.code);
            }
        };

        // stdout is the data and stderr is everything else, so that
        // `io exec --json … | jq` needs no filtering and a plain run can be
        // captured with `$(…)` without catching a status line.
        if let Some(reply) = to_stdout(self.args.json, result.reply.as_deref()) {
            let mut out = std::io::stdout().lock();
            let _ = writeln!(out, "{reply}");
            let _ = out.flush();
        }
        eprintln!("io: {}", describe(&result.outcome));
        if let Some(parked) = parked(&result.outcome, result.run_id) {
            eprintln!("io: {parked}");
        }
        // **A run that was meant to be gated and was not says so, on the arm where
        // nobody is watching a screen.** A hand-edited section that refuses, or a
        // reviewer that will not build, leaves the contract ungated — and until
        // this line the only surfaces that reported it were `/gates` and the
        // `/config` write, both of which need a terminal. An unattended job that
        // silently stopped verifying is the exact failure this release exists to
        // prevent, arriving through the release itself.
        if let Some(notice) = crate::contract::gate_notice(&self.config) {
            eprintln!("io: {notice}");
        }
        // **The criterion has the last word on the exit status, and it is read
        // from the store rather than from the outcome.** io-harness has no
        // `RunOutcome` variant for a run whose gate answered no, so a run that
        // spent its budget failing one reports `StepCapReached` and a run that
        // stopped early believing itself done reports `Finished` — `3` and `0`,
        // neither of which is what happened. Reported upstream as
        // io-harness#212.
        let standing = gate_standing(
            &self.store,
            result.run_id,
            &self.config,
            self.session.root(),
        );
        if let Some(standing) = &standing {
            eprintln!("io: {}", gate_line(standing));
        }
        Ok(verified_code(&result.outcome, standing.as_ref()))
    }
}

/// What a run's own gate attempts say, if it had a criterion at all.
///
/// **The criterion has to be resolved here, and reading the rows without it is
/// how this release nearly shipped exit `6` for every operator alive.**
/// io-harness evaluates the contract's criterion after every step on which the
/// agent called a tool, and for `Verification::None` that evaluation is `false` —
/// so an ungated run leaves `phase = "none", Failed` rows behind, and a naive
/// read of them says the gate failed on a session that never had one.
/// [`crate::gates::gate_attempts`] is the fold that drops them, and it is the same
/// call the interactive arm makes: one path, not two.
///
/// It is also what makes a bare `file` criterion work headlessly at all. That one
/// maps to `Verification::None`, so io-harness records nothing for it and io-cli
/// answers it itself — without this the terminal would hold an operator to a
/// standard that CI quietly ignored, which is the 0.14.0 asymmetry this product
/// deleted once already.
///
/// A store that cannot be read answers `None`, which leaves the exit status
/// exactly what it was before this release. A failure to read the verdict is not
/// evidence that the work is bad.
fn gate_standing(
    store: &Store,
    run_id: i64,
    config: &Config,
    root: &std::path::Path,
) -> Option<crate::gates::Standing> {
    let attempts = store.gate_attempts(run_id).ok()?;
    let criterion = crate::contract::criterion_of(config);
    crate::gates::standing(&crate::gates::gate_attempts(
        attempts,
        criterion.as_ref(),
        root,
    ))
}

/// One line for stderr naming what the gate did.
///
/// The phase is the harness's own word for the criterion that ran, and the
/// outcome is `GateOutcome::as_str` passed through rather than re-spelled — a
/// second vocabulary for the same fact is how two surfaces come to disagree.
fn gate_line(standing: &crate::gates::Standing) -> String {
    let attempt = if standing.attempt > 1 {
        format!(" on attempt {}", standing.attempt)
    } else {
        String::new()
    };
    format!(
        "the {} gate {}{attempt}",
        standing.phase,
        standing.outcome.as_str()
    )
}

/// One row of `io resume --list`, or `None` for a run nothing is waiting on.
///
/// `json` splits the same two streams `io exec --json` splits: an object per
/// line on stdout, everything else on stderr, so `io resume --list --json | jq`
/// needs no filtering.
///
/// **`step` means the same thing in all four shapes** — the last step that
/// committed. A question, a plan and an interrupted call each record the step
/// they stopped on, which is committed, and a run whose process went away
/// records the last one it got through; a resume of any of them starts at that
/// number plus one.
///
/// An interrupted turn and an ended one are not rows here. Neither can be
/// carried on, so listing them under a heading that means "waiting for you"
/// would offer work that does not exist — [`decision_for`] is where an operator
/// who names one by hand is told why.
///
/// **A batched ask is still `waiting_on: "question"` with one id**, because that
/// is what the store holds: io-harness 0.72.0 parks a whole batch as one
/// `pending_questions` row answered through one `question_id`. Renaming it would
/// break every script written against this listing to describe a difference the
/// resume door does not have. What is added instead is `questions` — how many
/// questions that one row is waiting on — because the operator's next command is
/// a single `--answer` and the one thing they cannot see from `waiting_on` alone
/// is how much that one answer has to cover. It is `1` for an ordinary question
/// and `null` for the three pauses that are not questions, which is the shape
/// `id` already uses for the run whose process went away.
pub fn listed(run_id: i64, pending: &Pending, json: bool) -> Option<String> {
    let (waiting_on, id, step, asked) = match pending {
        Pending::Question {
            question_id,
            step,
            questions,
            ..
        } => (
            "question",
            Some(*question_id),
            *step,
            // `max(1)` rather than the length: a singular ask carries no batch at
            // all, and a row waiting on one question is waiting on one question.
            Some(questions.len().max(1)),
        ),
        Pending::Plan { plan_id, step, .. } => ("plan", Some(*plan_id), *step, None),
        Pending::Recovery {
            attempt_id, step, ..
        } => ("recovery", Some(*attempt_id), *step, None),
        Pending::Died { last_step } => ("died", None, *last_step, None),
        Pending::Interrupted | Pending::Finished => return None,
    };
    Some(if json {
        // Built through `serde_json` rather than formatted, so an answer holding
        // a quote is escaped by the same code that escapes the event stream.
        serde_json::json!({
            "run_id": run_id,
            "waiting_on": waiting_on,
            "id": id,
            "step": step,
            "questions": asked,
        })
        .to_string()
    } else {
        // Only when there is more than one. A `1 question` on every row of a
        // listing is the mark nobody reads, and the plain stream is the one being
        // skimmed rather than parsed.
        let several = match asked {
            Some(n) if n > 1 => format!("  {n} questions"),
            _ => String::new(),
        };
        match id {
            Some(id) => format!("run {run_id}  {waiting_on} {id}  step {step}{several}"),
            None => format!("run {run_id}  {waiting_on}  step {step}"),
        }
    })
}

/// What the operator's flags mean, once checked against what the run is actually
/// waiting on, and carrying the id that addresses that pause.
///
/// The id travels with the decision rather than beside it, for the reason
/// [`crate::resume::Pending`] keeps its four ids under four names: a question id
/// handed to the plan driver is one operator's answer delivered into somebody
/// else's run, and a shape in which that cannot be typed is worth more than a
/// check that it was not.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Decision {
    /// Answer the question the run asked.
    ///
    /// **One id and one text answer a batched ask too**, and that is the store's
    /// shape rather than a simplification: io-harness parks a whole batch as one
    /// row under one `question_id`, and `resume_with_answer_observed` records one
    /// reply against it. So this needs no plural twin — what it needs, and what
    /// the refusal sentence gives it, is a line telling the operator their one
    /// text has to cover every question in the batch.
    Answer {
        /// The row the answer is written against.
        question_id: i64,
        /// What the operator said.
        answer: String,
    },
    /// Decide the plan the run proposed.
    Plan {
        /// The row the verdict is written against.
        plan_id: i64,
        /// The verdict.
        verdict: PlanVerdict,
    },
    /// Say what happened to the call the run stopped in the middle of.
    Recovery {
        /// The journal row the account is filed against.
        attempt_id: i64,
        /// What the operator established.
        decision: RecoveryDecision,
    },
    /// Nothing to decide: the process went away and the run simply has committed
    /// work and no ending.
    CarryOn,
}

/// The plan verdict `--plan` and `--correction` name together.
///
/// A correction is refused on the two verdicts that cannot carry one, rather than
/// dropped: `--plan approve --correction "…"` is somebody who meant `revise`, and
/// running the plan they were trying to change is the worst reading of it.
pub fn verdict_for(flag: PlanFlag, correction: Option<&str>) -> Result<PlanVerdict, String> {
    match (flag, correction) {
        (PlanFlag::Revise, Some(correction)) if !correction.trim().is_empty() => {
            Ok(PlanVerdict::revise(correction))
        }
        (PlanFlag::Revise, _) => Err(
            "`--plan revise` sends the plan back to be changed, so it needs to say what to \
             change: add `--correction \"<what to do differently>\"`."
                .into(),
        ),
        (_, Some(_)) => Err(
            "`--correction` says what a plan should do differently, and only `--plan revise` \
             asks for a different plan. Use `--plan revise --correction \"…\"`, or drop it."
                .into(),
        ),
        (PlanFlag::Approve, None) => Ok(PlanVerdict::Approve),
        (PlanFlag::Cancel, None) => Ok(PlanVerdict::Cancel),
    }
}

/// The recovery decision `--recovery` and `--account` name together.
///
/// `completed` is the one that needs words. io-harness files the operator's
/// account against the step the *call* was made on rather than the step the run
/// has now reached, so the resumed run reads a transcript in which the tool
/// answered where it was asked. Nothing validates it — this is an assertion about
/// the outside world — which is exactly why an empty one is refused.
pub fn recovery_for(flag: RecoveryFlag, account: Option<&str>) -> Result<RecoveryDecision, String> {
    match (flag, account) {
        (RecoveryFlag::Completed, Some(account)) if !account.trim().is_empty() => {
            Ok(RecoveryDecision::Completed {
                observation: account.to_string(),
            })
        }
        (RecoveryFlag::Completed, _) => Err(
            "`--recovery completed` tells the agent the call landed, so it needs to say what \
             the call returned: add `--account \"<what it returned>\"`."
                .into(),
        ),
        (_, Some(_)) => Err(
            "`--account` is what a call returned, and only `--recovery completed` says a call \
             returned anything. Use `--recovery completed --account \"…\"`, or drop it."
                .into(),
        ),
        (RecoveryFlag::Retry, None) => Ok(RecoveryDecision::Retry),
        (RecoveryFlag::Abandon, None) => Ok(RecoveryDecision::Abort),
    }
}

/// What this run is waiting on, and the invocation that decides it.
///
/// One sentence-maker for two questions an operator asks in the same breath —
/// "what does this run want" and "then what do I type" — so a missing flag and a
/// flag for the wrong pause are answered identically rather than by two sentences
/// that could drift apart.
fn waiting_on(run_id: i64, pending: &Pending) -> String {
    match pending {
        // **A batch is one `--answer`, and this sentence has to say what that
        // answer has to be.** io-harness parks the whole ask as one row, and a
        // resume records one text against it — `PendingQuestion::answers`, the
        // per-question breakdown, is written only by a `Responder` inside the
        // running process and stays empty for every answer that arrives this way.
        // So the door *can* answer a batch in one invocation; what it cannot do is
        // take the questions apart, and an operator who is told only
        // `--answer "<your answer>"` will send one sentence to a five-part ask.
        Pending::Question {
            question_id,
            questions,
            ..
        } if questions.len() > 1 => format!(
            "run {run_id} is waiting on question {question_id}, which is {n} questions the \
             agent asked in one go. One `--answer` answers all {n}: there is no per-question \
             flag, because io-harness parks a batch as a single row and records a single \
             reply against it. Number your answers to match the questions and send them as \
             one text — `io resume {run_id} --answer \"1. <…> 2. <…>\"`. The questions \
             themselves are not on this command line; `io` shows them when you resume the \
             run there",
            n = questions.len(),
        ),
        Pending::Question { question_id, .. } => format!(
            "run {run_id} is waiting on question {question_id}; answer it with \
             `io resume {run_id} --answer \"<your answer>\"`"
        ),
        Pending::Plan { plan_id, .. } => format!(
            "run {run_id} is waiting on plan {plan_id}; decide it with \
             `io resume {run_id} --plan approve`"
        ),
        Pending::Recovery {
            attempt_id, tool, ..
        } => format!(
            "run {run_id} stopped on call {attempt_id} to `{tool}`, whose outcome nobody \
             recorded; say what happened with `io resume {run_id} --recovery retry`"
        ),
        Pending::Died { last_step } => format!(
            "run {run_id}'s process went away after step {last_step} and there is nothing to \
             decide; carry it on with `io resume {run_id}`"
        ),
        Pending::Interrupted | Pending::Finished => {
            format!("there is nothing waiting on run {run_id}")
        }
    }
}

/// The decision to drive, or the sentence that refuses to drive one.
///
/// **Two refusals here are the point rather than a side effect.**
///
/// A run the operator interrupted is *finished*, not paused: `Ctrl+C` returns
/// `Flow::Cancel`, the loop records the outcome `cancelled`, and every resume
/// entry point short-circuits on a completed run and hands back the original
/// outcome having driven nothing. So it is refused before a provider is built,
/// in the same words the interactive surface uses — which point at `/fork` from
/// the turn before, the honest neighbouring answer.
///
/// And a flag for a pause the run is not on is refused rather than ignored. clap
/// cannot see which pause a run is waiting on; only the store can, and
/// `--plan approve` typed at a run holding a question is an operator acting on
/// the wrong thing.
pub fn decision_for(
    run_id: i64,
    pending: &Pending,
    args: &crate::cli::Resume,
) -> Result<Decision, String> {
    let wanted = match pending {
        Pending::Question { .. } => "--answer",
        Pending::Plan { .. } => "--plan",
        Pending::Recovery { .. } => "--recovery",
        Pending::Died { .. } => "",
        // Both sentences are `crate::resume::Failure`'s own, so the one an
        // operator gets from `io resume` is the one they get from `/resume`.
        Pending::Interrupted => {
            return Err(crate::resume::Failure::Interrupted { run_id }.to_string())
        }
        Pending::Finished => return Err(crate::resume::Failure::Ended { run_id }.to_string()),
    };
    for (flag, given) in [
        ("--answer", args.answer.is_some()),
        ("--plan", args.plan.is_some()),
        ("--recovery", args.recovery.is_some()),
    ] {
        if given && flag != wanted {
            return Err(format!(
                "`{flag}` decides something run {run_id} is not waiting on. {}",
                waiting_on(run_id, pending),
            ));
        }
    }

    match pending {
        Pending::Question { question_id, .. } => {
            let answer = args
                .answer
                .clone()
                .ok_or_else(|| waiting_on(run_id, pending))?;
            Ok(Decision::Answer {
                question_id: *question_id,
                answer,
            })
        }
        Pending::Plan { plan_id, .. } => {
            let flag = args.plan.ok_or_else(|| waiting_on(run_id, pending))?;
            Ok(Decision::Plan {
                plan_id: *plan_id,
                verdict: verdict_for(flag, args.correction.as_deref())?,
            })
        }
        Pending::Recovery { attempt_id, .. } => {
            let flag = args.recovery.ok_or_else(|| waiting_on(run_id, pending))?;
            Ok(Decision::Recovery {
                attempt_id: *attempt_id,
                decision: recovery_for(flag, args.account.as_deref())?,
            })
        }
        Pending::Died { .. } => Ok(Decision::CarryOn),
        // Both returned above; repeated here because the compiler asks and an
        // `unreachable!()` in a function an operator's typing reaches is a panic
        // waiting for the next variant.
        Pending::Interrupted | Pending::Finished => {
            Err(crate::resume::Failure::Ended { run_id }.to_string())
        }
    }
}

/// The goal a resumed run carries on under, or the sentence that refuses to
/// invent one.
///
/// **A bare run has no recoverable goal, and running it against an empty one is
/// the failure this refuses.** `runs.goal` has no public reader, so
/// `crate::resume::goal_for` recovers the operator's own words from the session
/// turn a run served and answers `None` for a run that served none — which is
/// every run `io exec` starts. A contract built from `None` would hand the agent
/// a task nobody set and spend a budget on it, so the goal is asked for instead.
///
/// `--goal` beats a recovered one when both are there. That is the operator
/// re-aiming their own run, which is theirs to do, and a flag silently ignored
/// because the store happened to hold something is worse than one that acts.
pub fn goal_or_refusal(
    run_id: i64,
    recovered: Option<String>,
    supplied: Option<&str>,
) -> Result<String, String> {
    if let Some(goal) = supplied.filter(|goal| !goal.trim().is_empty()) {
        return Ok(goal.to_string());
    }
    recovered.ok_or_else(|| {
        format!(
            "run {run_id} served no session turn, so what it was asked to do cannot be read \
             back — `runs.goal` has no public reader and only a turn keeps the operator's own \
             words. Carrying it on against an empty goal would set the agent a task nobody \
             asked for: say what it was for with \
             `io resume {run_id} --goal \"<what the run was for>\"`."
        )
    })
}

/// `io resume`, from the command line to an exit status.
///
/// The listing happens before a provider is chosen and before a credential is
/// read, so `io resume --list` works on a machine that has never run `io setup`
/// and costs nothing.
pub async fn resume_main(
    args: crate::cli::Resume,
    config: Config,
    root: std::path::PathBuf,
    model_override: Option<String>,
) -> Result<u8, String> {
    // Refused first, as `main` refuses it, so a posture nothing can honour costs
    // no store, no session and no provider.
    let posture = args.policy.map(posture_for).transpose()?;

    // The second headless door, and it resumes runs that execute things too.
    for notice in crate::bundle_path::install_for(&config) {
        eprintln!("{notice}");
    }

    let path = settings::store_path().ok_or("no place to keep the run store")?;
    let store = Store::open(&path).map_err(|error| error.to_string())?;

    if args.list {
        // One classification per run, each a handful of store reads and no
        // provider call. That is linear in the store's whole history rather than
        // in the parked runs, which is the right cost while a store holds
        // hundreds; the way past it is a store-side query for the pending rows,
        // which io-harness does not publish.
        let mut out = std::io::stdout().lock();
        for run_id in store.runs().map_err(|error| error.to_string())? {
            let pending =
                crate::resume::pending_for(&store, run_id).map_err(|error| error.to_string())?;
            if let Some(row) = listed(run_id, &pending, args.json) {
                let _ = writeln!(out, "{row}");
            }
        }
        let _ = out.flush();
        return Ok(OK);
    }

    // clap guarantees this through `required_unless_present`, and the sentence is
    // here rather than an `expect` because a parser's guarantee is not a reason to
    // panic in front of an operator if it ever stops holding.
    let run_id = args
        .run
        .ok_or("`io resume` needs a run id, or `--list` to see which runs have one")?;

    // Everything that can refuse, before anything is built: the classification is
    // a few store reads, and an operator who names the one run that cannot be
    // carried on pays nothing to find out.
    let pending = crate::resume::pending_for(&store, run_id).map_err(|error| error.to_string())?;
    let decision = decision_for(run_id, &pending, &args)?;
    let goal = goal_or_refusal(
        run_id,
        crate::resume::goal_for(&store, run_id).map_err(|error| error.to_string())?,
        args.goal.as_deref(),
    )?;

    let spec = spec_for(args.provider, &config, model_override.as_deref())?;
    let policy = policy_for(&config, posture);

    provider::build(
        spec,
        model_override,
        Resuming {
            store,
            config,
            policy,
            // `None` unless the operator named a posture, which is what lets
            // `carry_on` use the boundary the run itself recorded rather than one
            // chosen for it now — the only driver of the four that can.
            chosen: posture.is_some(),
            root,
            run_id,
            goal,
            decision,
            json: args.json,
        },
    )
    .await?
}

/// The resumed run, as something [`provider::build`] can run.
struct Resuming {
    store: Store,
    config: Config,
    policy: Policy,
    chosen: bool,
    root: std::path::PathBuf,
    run_id: i64,
    goal: String,
    decision: Decision,
    json: bool,
}

impl WithProvider for Resuming {
    type Out = Result<u8, String>;

    async fn call<P: Provider>(
        self,
        make: impl Fn(&str) -> Result<P, String>,
        model: String,
    ) -> Self::Out {
        let provider = make(&model)?;

        if let Some(line) = asks_nobody_can_answer(&self.policy) {
            eprintln!("io: {line}");
        }

        let json = Ndjson::new(std::io::stdout());
        let observer: &dyn Observer = if self.json { &json } else { &Ignore };

        // The same composition `Headless` builds, and for the same reason: a
        // `[[hook]]` that fired on the run and then went quiet the moment it was
        // carried on would leave an audit log with a hole in exactly the half
        // nobody watched happen.
        let resolved = crate::resolved::Resolved::load(&self.config);
        let hooks = crate::contract::hooks(&self.config, resolved.loaded(), &self.root);
        let mut observers: Vec<&dyn Observer> = vec![observer];
        if let Some(hooks) = &hooks {
            observers.push(hooks);
        }
        let fanout = crate::fanout::Fanout::new(observers);
        let durable = settings::store_path().and_then(|path| Store::open(&path).ok());
        let broadcast = durable.map(|store| io_harness::Broadcast::new(store, &fanout));
        let watcher: &dyn Observer = match &broadcast {
            Some(broadcast) => broadcast,
            None => &fanout,
        };

        // Read *before* anything is driven. `crate::resume` closes the turn with a
        // compare-and-swap against this value, so a head taken afterwards would be
        // the head this resume is about to move and the swap would never refuse —
        // which is the lost update the swap exists to catch.
        let head = self
            .store
            .turn_for_run(self.run_id)
            .ok()
            .flatten()
            .and_then(|turn_id| self.store.session_turn(turn_id).ok().flatten())
            .and_then(|turn| self.store.session_head(turn.session_id).ok().flatten());

        let resolved = crate::resolved::Resolved::load(&self.config);
        let contract = crate::contract::configured(
            self.goal,
            self.root.clone(),
            &self.config,
            resolved.loaded(),
        );
        // `None` on every arm: a containment is a fleet's shared budget, this
        // subcommand takes no flag that expresses one, and `crate::resume::recover`
        // refuses a contained run outright because io-harness publishes no
        // tree-aware recovery entry point to keep those limits with.
        //
        // **Still true at 0.73.0, and the shape of the gap is worth naming.** Every
        // other pause kind has both forms — `resume_tree_with_answer` beside
        // `resume_with_answer`, `resume_tree_with_plan_decision` beside its flat
        // one, `resume_tree_with_decision` beside `resume_with_decision`
        // (`io-harness-0.73.0/src/run.rs:1770`, `:2089`, `:3003`). Recovery has
        // `resume_with_recovery_observed` (`:2551`) and nothing tree-aware, so it
        // is the one pause a contained run cannot be resumed from. Not an oversight
        // this crate can route around: a fleet's shared ceiling lives in the tree
        // entry points, and resuming through the flat one would drop it.
        let resumed = match self.decision {
            Decision::Answer {
                question_id,
                answer,
            } => {
                crate::resume::answer_question(
                    &contract,
                    &provider,
                    &self.store,
                    self.run_id,
                    question_id,
                    &answer,
                    &self.policy,
                    &DenyAll,
                    None,
                    watcher,
                    head,
                )
                .await
            }
            Decision::Plan { plan_id, verdict } => {
                crate::resume::decide_plan(
                    &contract,
                    &provider,
                    &self.store,
                    self.run_id,
                    plan_id,
                    verdict,
                    &self.policy,
                    &DenyAll,
                    None,
                    watcher,
                    head,
                )
                .await
            }
            Decision::Recovery {
                attempt_id,
                decision,
            } => {
                crate::resume::recover(
                    &contract,
                    &provider,
                    &self.store,
                    self.run_id,
                    attempt_id,
                    decision,
                    &self.policy,
                    &DenyAll,
                    None,
                    watcher,
                    head,
                )
                .await
            }
            Decision::CarryOn => {
                crate::resume::carry_on(
                    &contract,
                    &provider,
                    &self.store,
                    self.run_id,
                    self.chosen.then_some(&self.policy),
                    &DenyAll,
                    None,
                    watcher,
                    head,
                )
                .await
            }
        }
        .map_err(Ending::from);
        // The same seam `Headless::call` has, for the same reason: this door reaches
        // a boundary refusal too, and 0.74.0 gave it two of its own.
        let resumed = match resumed {
            Ok(resumed) => resumed,
            Err(ending) => {
                eprintln!("io: {ending}");
                return Ok(ending.code);
            }
        };

        // stdout is the data and stderr is everything else, the split `io exec`
        // already makes.
        if let Some(reply) = to_stdout(self.json, resumed.reply.as_deref()) {
            let mut out = std::io::stdout().lock();
            let _ = writeln!(out, "{reply}");
            let _ = out.flush();
        }
        // **`+ 1`, and the interactive arm has always had it.** `resumed_after`
        // is the last step that had *committed* before anything was driven, so
        // the step this resume carried on from is the one after it. Printed bare,
        // this told the operator it resumed at 12 when it resumed at 13, and
        // disagreed with what the session said about the same run.
        eprintln!(
            "io: carried run {} on from step {}",
            resumed.run_id,
            resumed.resumed_after + 1
        );
        eprintln!("io: {}", describe(&resumed.outcome));
        // A resumed run can pause again, on a second question or on a plan the
        // agent proposes next, and the line that says so names the new handle.
        if let Some(parked) = parked(&resumed.outcome, resumed.run_id) {
            eprintln!("io: {parked}");
        }
        // A resumed run is gated by the same criterion the original was — the
        // contract is rebuilt from the same configuration — so its exit status
        // answers the same question. Reading the standing off the run this
        // resume drove rather than off the original is the point: the whole
        // reason to resume is that the work is not the same work any more.
        let standing = gate_standing(&self.store, resumed.run_id, &self.config, &self.root);
        if let Some(standing) = &standing {
            eprintln!("io: {}", gate_line(standing));
        }
        Ok(verified_code(&resumed.outcome, standing.as_ref()))
    }
}
