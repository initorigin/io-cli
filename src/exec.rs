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

use io_harness::{
    Config, DenyAll, Ignore, Observer, Policy, Provider, RunOutcome, Session, Store, TaskContract,
    TurnResult,
};

use crate::provider::{self, WithProvider};
use crate::settings;

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
    observer: &dyn Observer,
) -> Result<TurnResult, String> {
    session
        .turn_bounded_observed(
            &contract(config, session, goal),
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
fn contract(config: &Config, session: &Session, goal: String) -> TaskContract {
    let contract = TaskContract::workspace(goal, session.root().to_path_buf());
    let contract = config.apply_to(contract);
    match config.sandbox() {
        Some(sandbox) => contract.with_contained_exec(sandbox),
        None => contract,
    }
}

/// `io exec`, from the command line to an exit status.
pub async fn main(
    args: crate::cli::Exec,
    config: Config,
    root: std::path::PathBuf,
    model_override: Option<String>,
) -> Result<u8, String> {
    let Some(spec) = config.provider_spec().cloned() else {
        return Err(
            "no provider is configured; run `io setup`, or set one up in io.toml".into(),
        );
    };
    let store = settings::store_path().ok_or("no place to keep the run store")?;
    let store = Store::open(&store).map_err(|error| error.to_string())?;
    let session = Session::open(&store, &root).map_err(|error| error.to_string())?;
    let policy = config.policy().unwrap_or_default();

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
        let result = turn(
            &provider,
            &self.store,
            &mut self.session,
            &self.config,
            &self.policy,
            self.args.goal.clone(),
            &Ignore,
        )
        .await?;

        // stdout is the data and stderr is everything else, so that
        // `io exec --json … | jq` needs no filtering and a plain run can be
        // captured with `$(…)` without catching a status line.
        if !self.args.json {
            if let Some(reply) = &result.reply {
                let mut out = std::io::stdout().lock();
                let _ = writeln!(out, "{reply}");
                let _ = out.flush();
            }
        }
        eprintln!("io: {}", describe(&result.outcome));
        Ok(code(&result.outcome))
    }
}
