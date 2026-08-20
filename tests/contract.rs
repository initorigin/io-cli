//! **F1 and F2** — what a session turn's contract carries, and what it must not.

use std::path::PathBuf;

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
/// **One field is deliberately not io-harness's own**, and it is the step cap.
/// `TaskContract::workspace` caps a turn at twelve, which a turn that reads a
/// repository and writes a file reaches with the work half done — an operator
/// saw `error: step_cap_reached` under an unfinished answer, which is a ceiling
/// reported as a failure. Everything else still has to match, and this test is
/// what says so.
#[test]
fn nothing_configured_is_the_contract_the_session_built_before() {
    let built = session(
        "bring the docs up to date",
        root(),
        &Capabilities::default(),
    );
    let default = TaskContract::workspace("bring the docs up to date", root())
        .with_max_steps(io_cli::contract::MAX_STEPS);

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

    let contract = session("add a test", root(), &caps);
    assert_eq!(contract.mcp.len(), 1, "the server reaches the turn");
    assert_eq!(contract.lsp.len(), 1);
    assert!(contract.browser.is_some());
    assert_eq!(contract.skills, Some(PathBuf::from("/tmp/io-cli-skills")));
    assert_eq!(
        contract.goal, "add a test",
        "and the goal is still the operator's text",
    );
}

/// **F1 — the contract reaches the contained arm and the flat arm is untouched.**
///
/// A source gate rather than a behavioural one, and deliberately: `src/main.rs`
/// is a binary and nothing under `tests/` can link it, so the routing decision is
/// asserted where it is written. It is the same shape `tests/dependencies.rs`
/// uses for every other structural claim about this crate.
///
/// Sabotage: route the unconfigured session through the contained entry point
/// too, under which only this test fails — and it fails by taking `Ctrl+C` away
/// from every operator who never asked for a fan-out.
#[test]
fn the_flat_turn_is_still_the_steered_one() {
    let driver = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("src/main.rs");
    let text = std::fs::read_to_string(driver).expect("the driver");

    assert!(
        text.contains("turn_contained_bounded_observed("),
        "the contained arm takes the caller's contract",
    );
    // **Both arms take a contract since 0.11.0.** The flat one was
    // `turn_steered`, which builds `TaskContract::workspace` inside io-harness
    // and takes none from the caller — so its step cap was twelve and nothing
    // configured here could reach it. `turn_bounded_observed` takes a contract,
    // streams, and is not contained; what it gives up is the steer inbox, and
    // the only thing io-cli ever sent through one was an interrupt, which the
    // observer's own cancel does at the same step boundary.
    assert!(
        text.contains(
            "session.turn_bounded_observed(&contract, provider, store, policy, &approver, &observer)"
        ),
        "the flat arm carries a contract too, through the bounded turn",
    );
    assert!(
        !text.contains("turn_contained_observed("),
        "the contract-less contained entry point is gone; every contained turn takes one",
    );

    // The decision, not its formatting: one contract is built, unconditionally,
    // and both arms are handed it. A contract built only where a containment
    // was is what left the ordinary turn at io-harness's twelve steps.
    let built = text
        .split_once("let contract = io_cli::contract::session(")
        .expect("one contract is built for every turn")
        .1;
    let head = &built[..160.min(built.len())];
    assert!(
        head.contains("with_responder") && head.contains("with_plan_gate"),
        "the responder and the plan gate ride the same contract: {head:?}",
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
    let built = session(
        "bring the docs up to date",
        root(),
        &Capabilities::default(),
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
    );
    assert_eq!(asked.max_steps, 25);
}
