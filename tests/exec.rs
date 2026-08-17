//! F1 — `io exec` runs a goal to completion with no terminal.
//! F2 — the exit status is the harness's outcome, and the mapping is total.
//! F3 — a ceiling exits 3 rather than 0.
//!
//! The two mappings that look like taste are the release's research. A clean
//! headless run ends in `RunOutcome::Finished` and never `Success`, because the
//! contract this subcommand builds carries no verification criterion; and every
//! ceiling comes back as `Ok`, so a status derived from the `Result` reports
//! success on all four of them.

mod support;

use io_harness::{
    CompletionRequest, CompletionResponse, Config, Ignore, Policy, Provider, RunOutcome, Session,
    Store,
};
use io_cli::exec;

/// A workspace and a store, with no configuration file anywhere near the
/// developer's own.
///
/// `Config::from_toml` is what makes that true: it parses in memory and reads no
/// file at all, whereas `Config::discover` would consult `IO_CONFIG`,
/// `IO_CONFIG_HOME`, `XDG_CONFIG_HOME` and `HOME` and could pick up whatever the
/// person running the suite happens to have configured.
fn workspace(toml: &str) -> (tempfile::TempDir, Store, Session, Config) {
    let dir = tempfile::tempdir().expect("a workspace");
    let store = Store::memory().expect("an in-memory store");
    let session = Session::open(&store, dir.path()).expect("a session");
    let config = Config::from_toml(toml).expect("the configuration parses");
    (dir, store, session, config)
}

#[tokio::test]
async fn f1_a_goal_runs_to_completion_and_its_reply_comes_back() {
    let (dir, store, mut session, config) = workspace("");
    let provider = support::Scripted::writing(&[("notes.txt", "hello")]);

    let result = exec::turn(
        &provider,
        &store,
        &mut session,
        &config,
        &Policy::permissive(),
        "write the note".into(),
        &Ignore,
    )
    .await
    .expect("the turn runs");

    assert_eq!(
        result.reply.as_deref(),
        Some("done"),
        "the agent's reply is what a headless run has to hand back",
    );
    assert_eq!(
        std::fs::read_to_string(dir.path().join("notes.txt")).expect("the file was written"),
        "hello",
        "the run did the work, rather than only talking about it",
    );
    // The run is in the same store an interactive session writes to, which is what
    // lets `/resume` list a run that CI started.
    assert!(
        store.runs().expect("the store lists runs").contains(&result.run_id),
        "the headless run should be recorded in the ordinary store",
    );
}

#[tokio::test]
async fn f1_a_clean_run_finishes_rather_than_succeeding() {
    let (_dir, store, mut session, config) = workspace("");
    let provider = support::Scripted::writing(&[]);

    let result = exec::turn(
        &provider,
        &store,
        &mut session,
        &config,
        &Policy::permissive(),
        "say something".into(),
        &Ignore,
    )
    .await
    .expect("the turn runs");

    // Not an incidental assertion. `TaskContract::workspace` sets
    // `Verification::None`, so `Success` is unreachable from this subcommand and
    // `Finished` is what every good run returns. A table that mapped only
    // `Success` to zero would fail here — and only here, on the ordinary case.
    assert!(
        matches!(result.outcome, RunOutcome::Finished { .. }),
        "a contract with no verification criterion finishes, it does not succeed: {:?}",
        result.outcome,
    );
    assert_eq!(exec::code(&result.outcome), exec::OK);
}

#[test]
fn f2_every_outcome_maps_to_its_documented_code() {
    // All fifteen variants, written out rather than sampled. `RunOutcome` is not
    // `#[non_exhaustive]` and `exec::code` has no `_` arm, so a variant added by a
    // later harness breaks the build of both — which is the property that keeps a
    // published table true across a pin bump.
    let cases: &[(RunOutcome, u8)] = &[
        (RunOutcome::Success { steps: 3 }, exec::OK),
        (RunOutcome::Finished { steps: 3 }, exec::OK),
        (RunOutcome::Denied { steps: 1 }, exec::REFUSED),
        (RunOutcome::Refused { steps: 0 }, exec::REFUSED),
        (RunOutcome::PlanRejected { steps: 2 }, exec::REFUSED),
        (RunOutcome::StepCapReached { steps: 12 }, exec::CEILING),
        (RunOutcome::TimeBudgetExceeded { steps: 4 }, exec::CEILING),
        (RunOutcome::CostBudgetExceeded { steps: 5 }, exec::CEILING),
        (RunOutcome::BudgetCeilingReached { steps: 6 }, exec::CEILING),
        (
            RunOutcome::AwaitingApproval {
                request_id: 1,
                steps: 2,
            },
            exec::PAUSED,
        ),
        (
            RunOutcome::AwaitingAnswer {
                question_id: 1,
                steps: 2,
            },
            exec::PAUSED,
        ),
        (
            RunOutcome::AwaitingPlan {
                plan_id: 1,
                steps: 2,
            },
            exec::PAUSED,
        ),
        (RunOutcome::Stalled { steps: 7 }, exec::UNFINISHED),
        (
            RunOutcome::Escalated {
                steps: 8,
                retryable: true,
            },
            exec::UNFINISHED,
        ),
        (RunOutcome::Cancelled { steps: 9 }, exec::UNFINISHED),
    ];

    for (outcome, expected) in cases {
        assert_eq!(
            exec::code(outcome),
            *expected,
            "{outcome:?} should exit {expected}",
        );
    }

    // The six codes are distinct and cover 0..=5, so a script can branch on them.
    let mut codes: Vec<u8> = cases.iter().map(|(_, code)| *code).collect();
    codes.sort_unstable();
    codes.dedup();
    assert_eq!(codes, vec![0, 2, 3, 4, 5], "0, 2, 3, 4 and 5 are reachable from an outcome; 1 is reserved for never reaching one");
}

#[test]
fn f2_every_outcome_describes_itself_with_the_harness_step_count() {
    // The summary line names the harness's own number rather than one recounted
    // afterwards, and it pluralises. Asserted on a one-step run because that is
    // the case an unpluralised format string gets wrong.
    let one = exec::describe(&RunOutcome::Finished { steps: 1 });
    assert!(one.contains("1 step,") || one.ends_with("1 step"), "{one}");
    assert!(!one.contains("1 steps"), "{one}");
    let many = exec::describe(&RunOutcome::StepCapReached { steps: 12 });
    assert!(many.contains("12 steps"), "{many}");
    assert!(many.contains("step cap"), "{many}");
}

/// A provider that never stops calling tools, so the only way a turn ends is a
/// ceiling.
struct Endless;

impl Provider for Endless {
    async fn complete(&self, _request: CompletionRequest) -> io_harness::Result<CompletionResponse> {
        Ok(CompletionResponse {
            text: None,
            tool_calls: vec![support::write_call("again.txt", "again")],
            ..Default::default()
        })
    }
}

#[tokio::test]
async fn f3_a_step_ceiling_exits_three_rather_than_zero() {
    // The budget arrives through `[run]`, which `Config::apply_to` applies — the
    // section an interactive turn cannot use, because `turn_steered` builds its
    // own contract. This is the first path in the product where it has an effect.
    let (_dir, store, mut session, config) = workspace("[run]\nmax_steps = 2\n");

    let result = exec::turn(
        &Endless,
        &store,
        &mut session,
        &config,
        &Policy::permissive(),
        "keep going".into(),
        &Ignore,
    )
    .await
    .expect("a ceiling is a normal return, not an error");

    assert!(
        matches!(result.outcome, RunOutcome::StepCapReached { .. }),
        "expected the step cap, got {:?}",
        result.outcome,
    );
    assert_eq!(
        exec::code(&result.outcome),
        exec::CEILING,
        "a ceiling must not exit 0 — the harness returns it as Ok, so a status \
         derived from the Result would call this a success",
    );
}
