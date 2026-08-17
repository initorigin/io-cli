//! `io exec` — one goal, run to completion, with no terminal.
//!
//! This is a second **consumer** of io-harness, not a second program. It opens
//! the same store an interactive session opens, creates a session in it the same
//! way, hands the harness a policy the same way, and reads the same events back.
//! What it does not do is draw: nothing in this module reaches the renderer, the
//! composer, the picker or the theme, and `tests/exec.rs` asserts that rather
//! than trusting it.
//!
//! **It takes the contract-shaped entry point, and that is the whole reason the
//! sandbox and the budgets work here.** An interactive turn goes through
//! `Session::turn_steered`, which builds its own `TaskContract` internally in
//! order to accept a steer inbox — so it cannot be told about `[sandbox]` limits
//! or `[run]` budgets, which is the limitation this product has carried since
//! 0.2.0. A headless run has nobody to steer it, so it can hand in a contract of
//! its own, and `Session::turn_bounded_observed` takes both that contract and an
//! observer.

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
/// **This match has no `_` arm on purpose.** `io_harness::RunOutcome` is not
/// `#[non_exhaustive]`, so a variant added by a later harness breaks this build
/// rather than being silently folded into one of the six codes — which is the
/// only way a table published as public contract stays true across a pin bump.
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

        RunOutcome::Denied { .. } | RunOutcome::Refused { .. } | RunOutcome::PlanRejected { .. } => {
            REFUSED
        }

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

        RunOutcome::Stalled { .. } | RunOutcome::Escalated { .. } | RunOutcome::Cancelled { .. } => {
            UNFINISHED
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
        RunOutcome::TimeBudgetExceeded { steps } => ("stopped at the time budget", steps),
        RunOutcome::CostBudgetExceeded { steps } => ("stopped at the token budget", steps),
        RunOutcome::BudgetCeilingReached { steps } => ("stopped at the tree's budget ceiling", steps),
        RunOutcome::Denied { steps } => ("was denied", steps),
        RunOutcome::Refused { steps } => ("was refused before it began", steps),
        RunOutcome::PlanRejected { steps } => ("had its plan rejected", steps),
        RunOutcome::AwaitingApproval { steps, .. } => ("is waiting for an approval", steps),
        RunOutcome::AwaitingAnswer { steps, .. } => ("is waiting for an answer", steps),
        RunOutcome::AwaitingPlan { steps, .. } => ("is waiting for a plan decision", steps),
        RunOutcome::Stalled { steps } => ("stalled", steps),
        RunOutcome::Escalated { steps, .. } => ("escalated", steps),
        RunOutcome::Cancelled { steps } => ("was cancelled", steps),
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
/// file, and nothing else in this release.
///
/// The order matters and one step of it is easy to assume wrongly:
/// **`Config::apply_to` applies `[run]` but neither `[policy]` nor
/// `[sandbox]`.** The policy travels as its own argument to the turn, and the
/// sandbox has to be attached here by hand.
///
/// `[sandbox]` is attached only when the file actually has one. A default
/// `SandboxConfig` carries default resource limits, while
/// `TaskContract::workspace` deliberately starts from `SandboxLimits::none()` —
/// so attaching one unconditionally would impose caps on a run whose operator
/// never asked for any.
pub fn contract(
    config: &Config,
    session: &Session,
    goal: String,
    sandbox: Option<ExecMode>,
) -> TaskContract {
    let contract = TaskContract::workspace(goal, session.root().to_path_buf());
    let contract = config.apply_to(contract);
    let contract = match config.sandbox() {
        Some(sandbox) => contract.with_contained_exec(sandbox),
        None => contract,
    };
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
    matches!(sandbox, Some(ExecMode::FullAccess)).then_some(
        "--sandbox full-access: commands in this run are not confined to the workspace",
    )
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

    let Some(spec) = config.provider_spec().cloned() else {
        return Err(
            "no provider is configured; run `io setup`, or set one up in io.toml".into(),
        );
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

        let json = Ndjson::new(std::io::stdout());
        let observer: &dyn Observer = if self.args.json { &json } else { &Ignore };

        let result = turn(
            &provider,
            &self.store,
            &mut self.session,
            &self.config,
            &self.policy,
            self.args.goal.clone(),
            self.args.sandbox.map(crate::cli::Sandbox::mode),
            observer,
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
        Ok(code(&result.outcome))
    }
}
