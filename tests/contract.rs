//! **F1 and F2** — what a session turn's contract carries, and what it must not.

use std::path::PathBuf;
use std::sync::Arc;

use io_cli::contract::{session, Capabilities};
use io_cli::settings::CliSettings;
use io_harness::TaskContract;

fn root() -> PathBuf {
    PathBuf::from("/tmp/io-cli-contract")
}

/// **F2 — a session that configures nothing is unchanged.**
///
/// io-harness's own `default_contract` is `TaskContract::workspace(text, root)`
/// and nothing else, so a contract built from an empty configuration must be
/// that, field for field. Debug rather than `PartialEq` because a contract holds
/// `Arc<dyn Responder>` and `Arc<dyn PlanGate>`, which no derive can compare —
/// and both traits require `Debug`, so a field set by accident shows up here.
///
/// Sabotage: set any one field unconditionally in `contract::session`, under
/// which only this test fails — and it fails by changing every existing
/// operator's turn without saying so.
///
/// **Two fields are deliberately not io-harness's own.** The first is the step
/// cap.
/// `TaskContract::workspace` caps a turn at twelve, which a turn that reads a
/// repository and writes a file reaches with the work half done — an operator
/// saw `error: step_cap_reached` under an unfinished answer, which is a ceiling
/// reported as a failure. The second is the responder, which 0.12.0 puts on every
/// turn rather than only a contained one — a question asked on an ordinary turn
/// used to pause the run with nobody offered it. Everything else still has to
/// match, and this test is what says so.
#[test]
fn nothing_configured_is_the_contract_the_session_built_before() {
    let (answerer, _questions) = io_cli::intent::channel();
    let responder: Arc<dyn io_harness::Responder> = Arc::new(answerer);
    let built = session(
        "bring the docs up to date",
        root(),
        &Capabilities::default(),
        responder.clone(),
        None,
    );
    let default = TaskContract::workspace("bring the docs up to date", root())
        .with_max_steps(io_cli::contract::MAX_STEPS)
        .with_responder(responder);

    assert_eq!(format!("{built:?}"), format!("{default:?}"));
    // The number is a judgement and may move; what may not is that a turn stops
    // for a reason other than the cap. `a_turn_is_not_capped_at_twelve_steps`
    // reads it off the built contract, which is where it can actually be wrong.
}

/// Absent configuration is an absent capability, not an empty one that reads as
/// configured — the difference the status line depends on.
#[test]
fn a_file_with_no_sections_asks_for_nothing() {
    let caps = Capabilities::stored(Some(&CliSettings::default()));

    assert_eq!(caps, Capabilities::default());
    assert!(!caps.any());
    assert_eq!(
        Capabilities::stored(None),
        caps,
        "no file at all and a file with nothing in it are the same request",
    );
}

/// **F1 — every turn can answer a question, and no turn plans unless asked.**
///
/// The two seams that used to ride `[app.io-cli.containment]` together, now
/// separated: the responder is unconditional because io-harness resolves it
/// inside the tool dispatch on any run, and the gate is absent unless the caller
/// hands one over, because `plan_gate.is_some()` is the entire condition for
/// io-harness's planning phase.
///
/// Sabotage: make the responder parameter an `Option` and pass `None` from an
/// uncontained turn — under which only this test and F1's live arm fail, and they
/// fail by leaving a run stopped on a question nobody was shown.
#[test]
fn the_responder_is_unconditional_and_the_gate_is_not() {
    let (answerer, _questions) = io_cli::intent::channel();
    let bare = session(
        "bring the docs up to date",
        root(),
        &Capabilities::default(),
        Arc::new(answerer),
        None,
    );

    assert!(
        bare.responder.is_some(),
        "a question reaches the operator on an ordinary turn, not only a contained one",
    );
    assert!(
        bare.plan_gate.is_none(),
        "registering a gate turns the planning phase on; an operator who did not ask for one \
         must not get every turn stopped for a plan",
    );
}

/// Every capability the operator asked for reaches the contract, and each one is
/// io-harness's own type rather than a spelling of io-cli's.
#[test]
fn what_the_file_asks_for_is_what_the_contract_carries() {
    let settings: CliSettings = toml::from_str(
        r#"
        skills = "/tmp/io-cli-skills"

        [browser]
        binary = "/usr/bin/chromium"

        [[mcp]]
        id = "docs"
        transport = "stdio"
        command = "mcp-docs"

        [[lsp]]
        id = "rust-analyzer"
        command = "rust-analyzer"
        extensions = ["rs"]
        "#,
    )
    .expect("io-harness's own types deserialize from `[app.io-cli]`");

    let caps = Capabilities::stored(Some(&settings));
    assert!(caps.any());

    let (answerer, _questions) = io_cli::intent::channel();
    let contract = session("add a test", root(), &caps, Arc::new(answerer), None);
    assert_eq!(contract.mcp.len(), 1, "the server reaches the turn");
    assert_eq!(contract.lsp.len(), 1);
    assert!(contract.browser.is_some());
    assert_eq!(contract.skills, Some(PathBuf::from("/tmp/io-cli-skills")));
    assert_eq!(
        contract.goal, "add a test",
        "and the goal is still the operator's text",
    );
}

/// **F6 — the two arms cannot drift apart again.**
///
/// The coupling 0.12.0 removes survived a whole release unnoticed because nothing
/// asserted the two turns were given the same thing. Three claims, and together
/// they make a difference between the arms unrepresentable rather than merely
/// absent:
///
/// 1. `contract::session` takes no containment and cannot branch on one, so the
///    value it returns is the same whichever arm asks for it. That is a fact
///    about the signature, and the test below reads it off two built contracts.
/// 2. `src/main.rs` calls it exactly once per turn.
/// 3. Both arms are handed `&contract` — the same binding, not two of them.
///
/// Sabotage: attach any capability inside one arm's match branch — under which
/// only this test fails, and it fails by recreating the coupling whose five stale
/// doc surfaces are the other half of this release.
#[test]
fn f6_both_arms_are_handed_one_contract() {
    // (1) Same inputs, same contract — there is no containment to differ on.
    let (answerer, _questions) = io_cli::intent::channel();
    let responder: Arc<dyn io_harness::Responder> = Arc::new(answerer);
    let one = session(
        "a goal",
        root(),
        &Capabilities::default(),
        responder.clone(),
        None,
    );
    let two = session("a goal", root(), &Capabilities::default(), responder, None);
    assert_eq!(format!("{one:?}"), format!("{two:?}"));

    let driver = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("src/main.rs");
    let text = std::fs::read_to_string(driver).expect("the driver");

    // (2) One call, so there is no second contract to get wrong — and nothing
    // bolted onto it afterwards. `with_responder` and `with_plan_gate` used to be
    // called here, inside `if containment.is_some()`, which is exactly how an
    // uncontained turn ended up unable to answer a question. Both are
    // `contract::session`'s arguments now, where the test above reads them off a
    // value instead of off this file.
    assert_eq!(
        text.matches("io_cli::contract::session(").count(),
        1,
        "one contract is built per turn, not one per arm",
    );
    assert!(
        !text.contains("with_responder") && !text.contains("with_plan_gate"),
        "the capabilities are the contract builder's arguments, not a second step here",
    );
    assert!(
        !text.contains("turn_contained_observed(") && !text.contains("turn_steered("),
        "the two entry points that build their own contract are gone from this product",
    );

    // (3) And both arms take that binding. Matched with the whitespace removed,
    // because rustfmt decides where these argument lists break and an assertion
    // that a newline sits in a particular place is an assertion about formatting
    // — it would go quietly blind the first time one of them grew an argument.
    let squashed: String = text.chars().filter(|c| !c.is_whitespace()).collect();
    assert!(
        squashed.contains("session.turn_contained_bounded_observed(&contract,"),
        "the contained arm is handed the contract",
    );
    assert!(
        squashed.contains("session.turn_bounded_observed(&contract,"),
        "and so is the flat one",
    );
}

/// The step cap an ordinary turn runs under is io-cli's, and it is not twelve.
///
/// The number itself is a judgement and is allowed to move; what this pins is
/// that the cap stopped being the thing that ends a turn. A real session ended
/// on `error: step_cap_reached` with a file half written, which is a ceiling
/// reported to an operator as a failure.
#[test]
fn a_turn_is_not_capped_at_twelve_steps() {
    let (answerer, _questions) = io_cli::intent::channel();
    let responder: Arc<dyn io_harness::Responder> = Arc::new(answerer);
    let built = session(
        "bring the docs up to date",
        root(),
        &Capabilities::default(),
        responder.clone(),
        None,
    );
    assert!(built.max_steps >= 1_000, "{}", built.max_steps);

    // And the operator can still say otherwise, in either direction.
    let asked = session(
        "bring the docs up to date",
        root(),
        &Capabilities {
            max_steps: Some(25),
            ..Capabilities::default()
        },
        responder,
        None,
    );
    assert_eq!(asked.max_steps, 25);
}
