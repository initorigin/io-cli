//! `io exec` — one goal, run to completion, with no terminal.
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
    Config, DenyAll, ExecMode, Flow, Ignore, Observer, Policy, Provider, RunEvent, RunOutcome,
    Session, Store, TaskContract, TurnResult,
};

use crate::cli::PolicyFlag;
use crate::provider::{self, WithProvider};
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
/// `Finished` is `OK` because a contract with no verification criterion is what
/// this subcommand always builds, so `Finished` — and never `Success` — is what
/// a clean run returns; a table that treated only `Success` as zero would fail
/// every successful run. And the four ceilings need codes at all only because the
/// harness returns them as `Ok`, so a status read off the `Result` reports
/// success on every one of them.
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

        // Not reachable while approvals are denied rather than deferred and there
        // is no `io resume` to continue one. Mapped anyway so the table is total
        // now and adding that subcommand later renumbers nothing.
        RunOutcome::AwaitingApproval { .. }
        | RunOutcome::AwaitingAnswer { .. }
        | RunOutcome::AwaitingPlan { .. } => PAUSED,

        // 0.65.0 — a resume that found a call started and never finished. It is a
        // pause needing a decision, so it belongs with the other three rather than
        // with the failures. **Not unreachable, and the claim is deliberately not
        // made:** a session turn registers no tool and no MCP server and cannot
        // journal an attempt, but `io exec` applies the configuration to its own
        // contract, so a configured `[[mcp]]` server puts a run of this subcommand
        // exactly one interrupted call away from it.
        RunOutcome::AwaitingRecovery { .. } => PAUSED,

        RunOutcome::Stalled { .. }
        | RunOutcome::Escalated { .. }
        | RunOutcome::Cancelled { .. } => UNFINISHED,

        _ => UNFINISHED,
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
/// string its store keeps in `run_events.json`; a consumer that can read one can
/// read all three.
///
/// That is also why this forwards rather than matches. `EventKind` is
/// `#[non_exhaustive]` and io-cli's renderer handles eleven of its fifty
/// variants; a struct of io-cli's own with the fields the renderer knows would
/// pass every test written from the renderer's vocabulary and silently drop the
/// other thirty-nine.
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

/// The extra line a paused run gets, naming what was parked.
///
/// A run that stops for a human is persisted and resumable in principle, but
/// this release has no `io resume` to continue it — so the honest thing is to
/// say where it went rather than to imply it is gone. `None` for every outcome
/// that did not pause.
pub fn parked(outcome: &RunOutcome, run_id: i64) -> Option<String> {
    (code(outcome) == PAUSED).then(|| {
        format!(
            "run {run_id} is parked in the store; answering it and carrying on \
             is not in this release"
        )
    })
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
) -> Result<TurnResult, String> {
    session
        .turn_bounded_observed(
            &contract(config, session, goal, sandbox),
            provider,
            store,
            policy,
            // Nothing here can answer a question, so the harness's own documented
            // choice for an unattended job is the right one: an ask becomes a
            // refusal the agent is told about and adapts to, exactly as a policy
            // refusal already does. An approver that blocks would hang forever.
            &DenyAll,
            observer,
        )
        .await
        .map_err(|error| error.to_string())
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
    let contract = crate::contract::configured(goal, session.root().to_path_buf(), config);
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

    // `--provider` wins over the file, and is the only path that works when
    // there is no file at all — which is the CI case, and the case where an
    // interactive `io` would open the wizard nobody can answer.
    let spec = match (args.provider, config.provider_spec().cloned()) {
        (Some(which), _) => {
            let (key_var, model_var) = which.vars();
            provider::spec_from(
                which,
                std::env::var(key_var).ok(),
                model_override
                    .clone()
                    .or_else(|| std::env::var(model_var).ok()),
            )?
        }
        (None, Some(spec)) => spec,
        (None, None) => {
            return Err(
                "no provider is configured; run `io setup`, or pass `--provider` with \
                 its credential in the environment"
                    .into(),
            )
        }
    };
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
        let hooks = crate::contract::hooks(&self.config, self.session.root());
        let mut observers: Vec<&dyn Observer> = vec![observer];
        if let Some(hooks) = &hooks {
            observers.push(hooks);
        }
        let fanout = crate::fanout::Fanout::new(observers);
        let durable = crate::settings::store_path()
            .and_then(|path| io_harness::Store::open(&path).ok());
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
        .await?;

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
        Ok(code(&result.outcome))
    }
}
