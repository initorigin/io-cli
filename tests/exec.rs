//! F1 — `io exec` runs a goal to completion with no terminal.
//! F2 — the exit status is the harness's outcome, and the mapping is total.
//! F3 — a ceiling exits 3 rather than 0.
//! F4 — `--json` writes one `RunEvent` per line and nothing else on stdout.
//! F5 — the JSON is the harness's own shape, not io-cli's.
//! F6 — `--sandbox` and `--policy` reach the run.
//! F7 — `--policy ask-writes` is refused, and says what to use instead.
//! F8 — a run with no configuration file works from the environment.
//! F9 — an approval becomes a refusal the agent is told about, not a hang.
//! F11 — `--sandbox full-access` announces itself on stderr.
//! N1 — no interface code on the headless path.
//! N6 — the plain output is not composed to a width.
//!
//! The two mappings that look like taste are the release's research. A clean
//! headless run ends in `RunOutcome::Finished` and never `Success`, because the
//! contract this subcommand builds carries no verification criterion; and every
//! ceiling comes back as `Ok`, so a status derived from the `Result` reports
//! success on all four of them.

mod support;

use std::sync::atomic::{AtomicUsize, Ordering};

use clap::ValueEnum;
use io_cli::cli::{FromEnv, PolicyFlag};
use io_cli::settings::Posture;
use io_cli::{exec, provider};
use io_harness::{
    Act, CompletionRequest, CompletionResponse, Config, Effect, EventKind, ExecMode, Flow, Ignore,
    Observer, Policy, Provider, RunEvent, RunOutcome, Session, Store,
};

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
        None,
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
        store
            .runs()
            .expect("the store lists runs")
            .contains(&result.run_id),
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
        None,
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
    assert_eq!(
        codes,
        vec![0, 2, 3, 4, 5],
        "0, 2, 3, 4 and 5 are reachable from an outcome; 1 is reserved for never reaching one"
    );
}

#[test]
fn f2_a_paused_run_names_what_was_parked_and_nothing_else_does() {
    // Exit 4 is reachable, and the first end-to-end run of the release binary is
    // what proved it: the agent asked a question and the run stopped. Denying
    // approvals makes `AwaitingApproval` unreachable and does nothing to a
    // question about intent or a proposed plan, neither of which passes through
    // an approver at all.
    let waiting = exec::parked(
        &RunOutcome::AwaitingAnswer {
            question_id: 2,
            steps: 8,
        },
        41,
    )
    .expect("a paused run names what was parked");
    assert!(waiting.contains("41"), "{waiting}");
    assert!(
        waiting.contains("not in this release"),
        "it must say that answering it is not possible yet, rather than implying \
         the run is lost: {waiting}",
    );

    assert!(exec::parked(
        &RunOutcome::AwaitingPlan {
            plan_id: 1,
            steps: 2
        },
        7
    )
    .is_some());
    assert_eq!(exec::parked(&RunOutcome::Finished { steps: 3 }, 7), None);
    assert_eq!(
        exec::parked(&RunOutcome::StepCapReached { steps: 3 }, 7),
        None
    );
    assert_eq!(exec::parked(&RunOutcome::Cancelled { steps: 3 }, 7), None);
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
    async fn complete(
        &self,
        _request: CompletionRequest,
    ) -> io_harness::Result<CompletionResponse> {
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
        None,
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

/// Counts what the observer was handed, so a line count can be compared against
/// it rather than against a number a test decided in advance.
struct Counting<'a> {
    inner: &'a dyn Observer,
    seen: AtomicUsize,
}

impl Observer for Counting<'_> {
    fn event(&self, event: &RunEvent) -> Flow {
        self.seen.fetch_add(1, Ordering::SeqCst);
        self.inner.event(event)
    }
}

#[tokio::test]
async fn f4_json_writes_one_event_per_line_and_every_line_round_trips() {
    let (_dir, store, mut session, config) = workspace("");
    let provider = support::Scripted::writing(&[("notes.txt", "hello")]);

    let json = exec::Ndjson::new(Vec::new());
    let counting = Counting {
        inner: &json,
        seen: AtomicUsize::new(0),
    };

    let result = exec::turn(
        &provider,
        &store,
        &mut session,
        &config,
        &Policy::permissive(),
        "write the note".into(),
        None,
        &counting,
    )
    .await
    .expect("the turn runs");

    let seen = counting.seen.load(Ordering::SeqCst);
    let written = String::from_utf8(json.into_inner()).expect("the stream is UTF-8");
    let lines: Vec<&str> = written.lines().collect();

    assert!(seen > 0, "the run should have emitted events at all");
    assert_eq!(
        lines.len(),
        seen,
        "every event the observer saw should be exactly one line, and nothing \
         else should reach the stream",
    );

    // The round trip is the criterion. A line that merely parses as JSON proves
    // nothing; a line that deserializes back into the harness's own type proves
    // the shape was not re-modelled on the way out.
    for line in &lines {
        let event: RunEvent =
            serde_json::from_str(line).unwrap_or_else(|error| panic!("{line}\n{error}"));
        let _ = event.run_id;
    }

    // The reply belongs to stdout in plain mode and must not also be in the
    // stream as prose. It may appear inside a `finished` or `token` payload,
    // which is the harness's business, so this asserts on the line's shape rather
    // than on the absence of the word.
    assert!(result.reply.is_some());
    for line in &lines {
        assert!(
            line.starts_with('{') && line.ends_with('}'),
            "a line that is not one JSON object is a line `jq` cannot read: {line}",
        );
    }
}

#[test]
fn f5_the_json_is_the_harness_shape_with_no_envelope_of_our_own() {
    let json = exec::Ndjson::new(Vec::new());
    json.event(&RunEvent::new(
        7,
        3,
        EventKind::Step {
            decision: "wrote a file".into(),
            tool_call: "write_file".into(),
            tokens: 42,
            changed: true,
        },
    ));
    let written = String::from_utf8(json.into_inner()).expect("UTF-8");
    let value: serde_json::Value = serde_json::from_str(written.trim()).expect("one JSON object");

    // `kind` is flattened, so the variant's fields sit beside the envelope's
    // rather than under a key of their own. This is the harness's own documented
    // assertion about its serialization, restated here because it is what a
    // published shape means.
    assert_eq!(value["run_id"], 7);
    assert_eq!(value["step"], 3);
    assert_eq!(value["depth"], 0);
    assert_eq!(value["event"], "step");
    assert_eq!(value["tokens"], 42);
    assert_eq!(value["changed"], true);
    assert!(
        value.get("kind").is_none(),
        "`kind` is flattened; a `kind` key means a second serialization: {written}",
    );
}

#[test]
fn f5_an_event_the_renderer_cannot_draw_still_reaches_the_stream() {
    // The whole difference between forwarding a stream and re-modelling one.
    // io-cli's renderer handles eleven of `EventKind`'s fifty variants; a struct
    // of io-cli's own would pass every test written from the renderer's
    // vocabulary and drop this one silently.
    let json = exec::Ndjson::new(Vec::new());
    json.event(&RunEvent::new(
        1,
        0,
        EventKind::MemoryWrote {
            key: "what-the-renderer-never-draws".into(),
        },
    ));
    json.event(&RunEvent::new(
        1,
        0,
        EventKind::Speculated {
            started: 3,
            used: 1,
            discarded: 2,
        },
    ));
    let written = String::from_utf8(json.into_inner()).expect("UTF-8");
    let lines: Vec<&str> = written.lines().collect();

    assert_eq!(lines.len(), 2, "both events should reach the stream");
    let first: serde_json::Value = serde_json::from_str(lines[0]).expect("JSON");
    assert_eq!(first["event"], "memory_wrote");
    assert_eq!(first["key"], "what-the-renderer-never-draws");
    let second: serde_json::Value = serde_json::from_str(lines[1]).expect("JSON");
    assert_eq!(second["event"], "speculated");
    assert_eq!(second["discarded"], 2);
}

#[test]
fn f4_nothing_but_the_stream_reaches_stdout_under_json() {
    // The decision lives in the library rather than in a match arm in the binary,
    // because a decision in the binary has no automated coverage at all and this
    // is the one that keeps `io exec --json | jq` working.
    assert_eq!(exec::to_stdout(false, Some("the reply")), Some("the reply"));
    assert_eq!(exec::to_stdout(false, None), None);
    assert_eq!(
        exec::to_stdout(true, Some("the reply")),
        None,
        "under --json the stream is the whole of stdout; a reply printed beside \
         it is a line no JSON reader can take",
    );
}

#[test]
fn f6_the_sandbox_flag_reaches_the_contract_the_harness_is_given() {
    let (_dir, store, session, config) = workspace("");
    let _ = &store;

    // Read off the contract at the call boundary, not off the flag: the flag
    // being parsed proves nothing about what the run was handed.
    let plain = exec::contract(&config, &session, "do the thing".into(), None);
    assert_eq!(
        plain.exec_sandbox.mode,
        ExecMode::WorkspaceWrite,
        "`TaskContract::workspace` starts at workspace-write",
    );

    let confined = exec::contract(
        &config,
        &session,
        "do the thing".into(),
        Some(ExecMode::ReadOnly),
    );
    assert_eq!(confined.exec_sandbox.mode, ExecMode::ReadOnly);

    let open = exec::contract(
        &config,
        &session,
        "do the thing".into(),
        Some(ExecMode::FullAccess),
    );
    assert_eq!(open.exec_sandbox.mode, ExecMode::FullAccess);
}

#[test]
fn f6_the_flag_beats_the_file_and_keeps_the_file_s_limits() {
    let (_dir, _store, session, config) =
        workspace("[sandbox]\nmode = \"read-only\"\n\n[sandbox.limits]\nmax_wall_secs = 30\n");

    let from_file = exec::contract(&config, &session, "go".into(), None);
    assert_eq!(
        from_file.exec_sandbox.mode,
        ExecMode::ReadOnly,
        "[sandbox] should reach the contract — this is the first path in the \
         product where that section has any effect",
    );
    assert_eq!(from_file.exec_sandbox.limits.max_wall_secs, Some(30));

    let overridden = exec::contract(
        &config,
        &session,
        "go".into(),
        Some(ExecMode::WorkspaceWrite),
    );
    assert_eq!(overridden.exec_sandbox.mode, ExecMode::WorkspaceWrite);
    assert_eq!(
        overridden.exec_sandbox.limits.max_wall_secs,
        Some(30),
        "the flag changes the mode; the limits are the operator's and are not \
         this flag's to discard",
    );
}

#[test]
fn f6_the_policy_flag_reaches_the_policy_the_run_is_given() {
    let (_dir, _store, _session, config) = workspace("");

    let read_only = exec::policy_for(&config, Some(Posture::ReadOnly));
    assert_eq!(read_only.defaults.write, Effect::Deny);
    assert_eq!(read_only.defaults.exec, Effect::Deny);
    assert_eq!(read_only.defaults.read, Effect::Allow);

    let workspace_posture = exec::policy_for(&config, Some(Posture::Workspace));
    assert_eq!(workspace_posture.defaults.write, Effect::Allow);
    assert_eq!(workspace_posture.defaults.net, Effect::Deny);

    // A posture replaces the tier defaults and never the layers, so the
    // harness's own secret denials survive a flag.
    assert!(
        read_only
            .layers
            .iter()
            .any(|layer| layer.name == "builtin-secrets"),
        "a --policy flag must not drop the built-in secret denials",
    );
    assert_eq!(read_only.check(Act::Read, ".env").effect, Effect::Deny);
}

#[test]
fn f7_ask_writes_is_refused_and_names_the_two_that_work() {
    let error = exec::posture_for(PolicyFlag::AskWrites)
        .expect_err("ask-writes cannot be honoured without a terminal");

    assert!(error.contains("ask-writes"), "{error}");
    assert!(error.contains("workspace"), "{error}");
    assert!(error.contains("read-only"), "{error}");
    assert!(
        error.contains("denied"),
        "the refusal must say what would otherwise happen, not merely decline: {error}",
    );

    // The two that do work are not refused.
    assert_eq!(
        exec::posture_for(PolicyFlag::Workspace).expect("workspace works"),
        Posture::Workspace,
    );
    assert_eq!(
        exec::posture_for(PolicyFlag::ReadOnly).expect("read-only works"),
        Posture::ReadOnly,
    );
}

#[test]
fn f7_a_configured_posture_that_asks_warns_rather_than_denying_in_silence() {
    // The same defect the flag is refused for, arriving through the
    // configuration file. Found by running the release binary against this
    // machine's own config, which asks — the posture the wizard recommends — and
    // watching every write get denied with nothing said about why.
    let (_dir, _store, _session, config) = workspace("");

    let asking = exec::policy_for(&config, Some(Posture::AskWrites));
    let line = exec::asks_nobody_can_answer(&asking)
        .expect("a posture that asks must say so before the run");
    assert!(line.contains("denied"), "{line}");
    assert!(line.contains("workspace"), "{line}");
    assert!(line.contains("read-only"), "{line}");

    // A posture that does not ask says nothing, so a run that is going to work
    // is not decorated with a warning about a problem it does not have.
    assert_eq!(
        exec::asks_nobody_can_answer(&exec::policy_for(&config, Some(Posture::Workspace))),
        None,
    );
    assert_eq!(
        exec::asks_nobody_can_answer(&exec::policy_for(&config, Some(Posture::ReadOnly))),
        None,
    );
}

#[test]
fn f7_the_warning_names_only_what_actually_asks() {
    // A file can ask about writes and not commands, or the reverse. Naming both
    // when only one asks is the kind of true-sounding sentence that teaches the
    // wrong thing about the boundary in force.
    // `exec = "deny"` rather than `"allow"`: `Config::from_toml` parses at
    // PROJECT scope, where a value that widens the boundary is refused outright,
    // because a repository you cloned must not be able to grant itself
    // permission. Narrowing is always allowed.
    let write_only = Config::from_toml(
        "[policy.defaults]\nread = \"allow\"\nwrite = \"ask\"\nexec = \"deny\"\nnet = \"deny\"\n",
    )
    .expect("the configuration parses");
    let line = exec::asks_nobody_can_answer(&exec::policy_for(&write_only, None))
        .expect("a write that asks still warns");
    assert!(line.contains("every write will be denied"), "{line}");

    let exec_only = Config::from_toml(
        "[policy.defaults]\nread = \"allow\"\nwrite = \"deny\"\nexec = \"ask\"\nnet = \"deny\"\n",
    )
    .expect("the configuration parses");
    let line = exec::asks_nobody_can_answer(&exec::policy_for(&exec_only, None))
        .expect("a command that asks still warns");
    assert!(line.contains("every command will be denied"), "{line}");
}

#[test]
fn f7_the_flag_speaks_the_same_words_as_the_status_line() {
    // The flag's value names are `Posture::short()`, reused rather than
    // re-spelled. A flag that disagrees with the status line teaches the wrong
    // vocabulary, and this is what stops the two drifting.
    let flags: Vec<String> = PolicyFlag::value_variants()
        .iter()
        .map(|flag| {
            flag.to_possible_value()
                .expect("every variant is selectable")
                .get_name()
                .to_string()
        })
        .collect();
    let postures: Vec<String> = Posture::ALL
        .iter()
        .map(|posture| posture.short().to_string())
        .collect();
    assert_eq!(flags, postures);
}

#[test]
fn f11_full_access_announces_itself_and_nothing_else_does() {
    let line = exec::widening(Some(ExecMode::FullAccess)).expect("full-access announces itself");
    assert!(line.contains("full-access"), "{line}");
    assert!(
        line.contains("not confined"),
        "the line must say what it costs, not merely name the flag: {line}",
    );

    assert_eq!(exec::widening(None), None);
    assert_eq!(exec::widening(Some(ExecMode::ReadOnly)), None);
    assert_eq!(exec::widening(Some(ExecMode::WorkspaceWrite)), None);
}

#[test]
fn f8_a_provider_comes_from_the_environment_with_no_file_anywhere() {
    let spec = provider::spec_from(
        FromEnv::Anthropic,
        Some("a-key".into()),
        Some("claude-sonnet-4".into()),
    )
    .expect("a key and a model are all it needs");

    match spec {
        io_harness::ProviderSpec::Anthropic { model, api_key } => {
            assert_eq!(model, "claude-sonnet-4");
            // The credential is deliberately not carried in the spec: `key_for`
            // reads the same variable a moment later, so a key travels one path
            // whether it came from a file or from the shell.
            assert_eq!(api_key, None);
        }
        other => panic!("expected an anthropic spec, got {other:?}"),
    }
}

#[test]
fn f8_a_missing_variable_is_named_rather_than_guessed_at() {
    let no_key = provider::spec_from(FromEnv::OpenRouter, None, Some("a-model".into()))
        .expect_err("a provider with no credential cannot run");
    assert!(no_key.contains("OPENROUTER_API_KEY"), "{no_key}");

    let no_model = provider::spec_from(FromEnv::OpenAi, Some("a-key".into()), None)
        .expect_err("a provider with no model cannot run");
    assert!(no_model.contains("OPENAI_MODEL"), "{no_model}");
    assert!(
        no_model.contains("-m"),
        "the other way to supply a model should be named too: {no_model}",
    );

    // An empty variable is not a set one. A CI job that exports a name with no
    // value is the ordinary way this goes wrong.
    let empty = provider::spec_from(FromEnv::Anthropic, Some(String::new()), Some("m".into()))
        .expect_err("an empty credential is not a credential");
    assert!(empty.contains("ANTHROPIC_API_KEY"), "{empty}");
}

#[test]
fn f8_the_variables_are_the_harness_own_names() {
    // Not io-cli's invention. These are the pairs `OpenRouter::from_env`,
    // `Anthropic::from_env` and `OpenAi::from_env` read, so a shell that already
    // works with io-harness works here unchanged.
    assert_eq!(
        FromEnv::OpenRouter.vars(),
        ("OPENROUTER_API_KEY", "OPENROUTER_MODEL")
    );
    assert_eq!(
        FromEnv::Anthropic.vars(),
        ("ANTHROPIC_API_KEY", "ANTHROPIC_MODEL")
    );
    assert_eq!(FromEnv::OpenAi.vars(), ("OPENAI_API_KEY", "OPENAI_MODEL"));

    // `compatible` is absent by decision, not by oversight: io-harness gives it
    // no `from_env`, because a base URL has to come from somewhere.
    assert_eq!(FromEnv::value_variants().len(), 3);
}

#[test]
fn f8_the_provider_names_are_spelled_the_way_the_harness_spells_them() {
    // clap derives a kebab-case name from the variant, which would make this
    // flag take `open-router` and `open-ai` — names io-harness does not use, the
    // README does not document, and nobody would guess. Found by running the
    // release binary, because no test links one.
    let names: Vec<String> = FromEnv::value_variants()
        .iter()
        .map(|which| {
            which
                .to_possible_value()
                .expect("every variant is selectable")
                .get_name()
                .to_string()
        })
        .collect();
    assert_eq!(names, vec!["openrouter", "anthropic", "openai"]);

    // The same words io-harness's own `ProviderSpec` is tagged with, so one
    // vocabulary spans the configuration file and the command line.
    for name in &names {
        let toml = format!("[[provider]]\nkind = \"{name}\"\nmodel = \"m\"\n");
        assert!(
            Config::from_toml(&toml).is_ok(),
            "io-harness does not know a provider called `{name}`",
        );
    }
}

#[tokio::test]
async fn f8_no_provider_and_no_flag_fails_with_a_sentence_rather_than_a_prompt() {
    // The interactive binary opens the wizard when nothing is configured. A
    // headless run must never reach it: in a container there is nobody to answer
    // it, and a prompt on a pipe is a hang rather than an error.
    let dir = tempfile::tempdir().expect("a workspace");
    let error = exec::main(
        io_cli::cli::Exec {
            goal: "do the thing".into(),
            json: false,
            sandbox: None,
            policy: None,
            provider: None,
        },
        Config::from_toml("").expect("an empty configuration"),
        dir.path().to_path_buf(),
        None,
    )
    .await
    .expect_err("a headless run with no provider cannot start");

    assert!(
        error.contains("--provider"),
        "the error should name the way out: {error}",
    );
    assert!(!dir.path().join("io.toml").exists(), "nothing was written");
}

#[tokio::test]
async fn f9_an_ask_becomes_a_refusal_and_the_run_still_ends() {
    // Under a posture whose default is `Ask`, a write reaches an approver. There
    // is no approver here that can say yes — `DenyAll` is the harness's own
    // choice for an unattended job — so the ask becomes a refusal the agent is
    // told about and adapts to, exactly as a policy refusal already does.
    //
    // The failure this guards against is not a wrong answer but a hang: an
    // approver that blocks on a channel nothing drains never returns, and a test
    // cannot assert its way out of that. Reaching the assertions at all is half
    // of what this proves.
    let (dir, store, mut session, config) = workspace("");
    let provider = support::Scripted::writing(&[("notes.txt", "hello")]);
    let policy = exec::policy_for(&config, Some(Posture::AskWrites));
    assert_eq!(policy.defaults.write, Effect::Ask);

    let json = exec::Ndjson::new(Vec::new());
    let result = exec::turn(
        &provider,
        &store,
        &mut session,
        &config,
        &policy,
        "write the note".into(),
        None,
        &json,
    )
    .await
    .expect("the turn ends rather than blocking on a question nobody can answer");

    assert_eq!(
        exec::code(&result.outcome),
        exec::OK,
        "a denied write is not a failed run: the agent was told and carried on",
    );
    assert!(
        !dir.path().join("notes.txt").exists(),
        "the write was denied, so the file must not be on disk",
    );

    // The denial is in the stream, so a CI job can see why nothing was written.
    let written = String::from_utf8(json.into_inner()).expect("UTF-8");
    let events: Vec<serde_json::Value> = written
        .lines()
        .map(|line| serde_json::from_str(line).expect("each line is JSON"))
        .collect();
    let kinds: Vec<&str> = events
        .iter()
        .filter_map(|event| event["event"].as_str())
        .collect();
    assert!(
        kinds.contains(&"approval_requested") || kinds.contains(&"refused"),
        "the operator must be able to see that something was asked: {kinds:?}",
    );
    assert!(
        kinds.contains(&"approval_decided") || kinds.contains(&"refused"),
        "and that it was answered: {kinds:?}",
    );
}

/// One source file, read for what it does not contain.
fn source(name: &str) -> String {
    std::fs::read_to_string(
        std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("src")
            .join(name),
    )
    .unwrap_or_else(|_| panic!("src/{name} should exist"))
}

#[test]
fn n1_the_headless_path_reaches_no_interface_code() {
    // Asserted rather than intended. The headless path renders nothing, and the
    // way that stops being true is somebody reaching for a Theme to colour an
    // error — which would put ANSI in a stream a machine is reading.
    let forbidden = [
        "crate::term",
        "crate::app",
        "crate::picker",
        "crate::composer",
        "crate::status",
        "crate::theme",
        "crate::splash",
        "crate::diff",
        "ratatui",
        "crossterm",
    ];
    for module in ["exec.rs", "provider.rs"] {
        let text = source(module);
        for name in forbidden {
            assert!(
                !text.contains(name),
                "src/{module} reaches `{name}`. The headless path draws nothing, \
                 and a renderer on it is how ANSI ends up in a stream that is \
                 being parsed.",
            );
        }
    }
}

#[test]
fn n1_no_escape_sequence_is_written_by_the_headless_path() {
    for module in ["exec.rs", "provider.rs"] {
        let text = source(module);
        assert!(
            !text.contains("\u{1b}") && !text.contains("\\x1b") && !text.contains("\\e["),
            "src/{module} contains an escape sequence",
        );
    }
}

#[test]
fn n1_the_headless_path_is_chosen_before_the_wizard_can_be_reached() {
    // `main.rs` has no automated coverage by construction — no integration test
    // links a binary — so this decision cannot be sabotaged. What can be checked
    // is its ORDER, which is the whole of its correctness: the exec arm must
    // return before the block that runs the wizard, or a container with no
    // configuration file gets a prompt nobody can answer.
    let main = source("main.rs");
    let exec_arm = main
        .find("Subcommand::Exec(args)")
        .expect("main routes the exec subcommand");
    let wizard = main
        .find("Screen::attach_with(io_cli::term::WIZARD_VIEWPORT_HEIGHT)")
        .expect("main opens the wizard viewport somewhere");
    let refusal = main
        .find("stdout().is_terminal()")
        .expect("main still refuses a non-TTY session");

    assert!(
        exec_arm < wizard,
        "`io exec` must leave before the wizard can be reached",
    );
    assert!(
        exec_arm < refusal,
        "`io exec` must leave before the non-TTY refusal, which is what it is \
         the answer to rather than a victim of",
    );
}

#[test]
fn n6_the_plain_output_is_never_composed_to_a_width() {
    // The opposite discipline from every viewport surface, and worth asserting
    // because three releases running have paid for a row whose important half was
    // the half that got clipped. Here nothing measures and nothing truncates: the
    // terminal wraps the line, or the pipe does not.
    let text = source("exec.rs");
    for measuring in ["width", "truncate", "chars().take(", "unicode_width"] {
        assert!(
            !text.contains(measuring),
            "src/exec.rs measures its output (`{measuring}`). A headless stream \
             is not a viewport: clipping it loses data a machine was going to read.",
        );
    }
}

#[test]
fn the_global_flags_are_accepted_on_either_side_of_the_subcommand() {
    use clap::Parser;

    // `src/main.rs` has no automated coverage, so a flag that parses only in one
    // position is invisible to every other test here — and this one was: the
    // README documented `io exec -m <model> "…"` while clap rejected it, and the
    // first end-to-end run of the release binary is what found it.
    for argv in [
        vec!["io", "-m", "a-model", "exec", "the goal"],
        vec!["io", "exec", "-m", "a-model", "the goal"],
        vec!["io", "exec", "the goal", "-m", "a-model"],
    ] {
        let cli = io_cli::cli::Cli::try_parse_from(&argv)
            .unwrap_or_else(|error| panic!("{argv:?} should parse\n{error}"));
        assert_eq!(cli.model.as_deref(), Some("a-model"), "{argv:?}");
        match cli.command {
            Some(io_cli::cli::Command::Exec(exec)) => assert_eq!(exec.goal, "the goal"),
            other => panic!("{argv:?} should be an exec command, got {other:?}"),
        }
    }

    for argv in [
        vec!["io", "-C", "/tmp/x", "exec", "the goal"],
        vec!["io", "exec", "-C", "/tmp/x", "the goal"],
    ] {
        let cli = io_cli::cli::Cli::try_parse_from(&argv)
            .unwrap_or_else(|error| panic!("{argv:?} should parse\n{error}"));
        assert_eq!(cli.dir.as_deref(), Some(std::path::Path::new("/tmp/x")));
    }
}

/// 0.8.0 F9 — the outcome table names every outcome the locked io-harness declares.
///
/// This test exists because a compile error stopped existing. `RunOutcome` was
/// exhaustive until io-harness 0.65 and `exec::code`'s match had no `_` arm, so a
/// variant a later harness added broke the build. 0.65 marked the enum
/// `#[non_exhaustive]`, the catch-all became mandatory, and with it a new outcome
/// would arrive as `UNFINISHED` with nothing said. The property is asserted here
/// instead: the table is compared against the variants declared in the source this
/// crate is locked to, so a pin bump that adds one fails a test that names it.
#[test]
fn the_outcome_table_names_every_outcome_the_locked_harness_declares() {
    let declared = support::harness_run_outcomes();
    assert!(
        declared.contains(&"AwaitingRecovery".to_string()),
        "io-harness 0.65 declares AwaitingRecovery; found {declared:?}"
    );

    let source = std::fs::read_to_string("src/exec.rs").expect("this crate's source is readable");
    let table = source
        .split_once("pub fn code(outcome: &RunOutcome) -> u8 {")
        .expect("the exit-code table is here")
        .1
        .split_once("\n}\n")
        .expect("the table is closed")
        .0;

    let missing: Vec<&String> = declared
        .iter()
        .filter(|variant| !table.contains(&format!("RunOutcome::{variant}")))
        .collect();
    assert!(
        missing.is_empty(),
        "io-harness declares outcomes the exit-code table does not name, so each of \
         them exits as UNFINISHED with nothing said about it: {missing:?}"
    );
}

/// 0.8.0 F9 — the recovery pause is a pause, and an unnamed outcome invents no count.
#[test]
fn a_recovery_pause_exits_paused_and_is_described_as_a_pause() {
    let outcome = RunOutcome::AwaitingRecovery {
        attempt_id: 7,
        steps: 3,
    };
    assert_eq!(exec::code(&outcome), exec::PAUSED);
    assert_eq!(
        exec::describe(&outcome),
        "the run is waiting for a recovery decision, after 3 steps"
    );
}
