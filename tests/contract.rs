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
#[test]
fn nothing_configured_is_the_contract_the_session_built_before() {
    let built = session(
        "bring the docs up to date",
        root(),
        &Capabilities::default(),
    );
    let default = TaskContract::workspace("bring the docs up to date", root());

    assert_eq!(format!("{built:?}"), format!("{default:?}"));
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
    assert!(
        text.contains(
            "session.turn_steered(text, provider, store, policy, &approver, &observer, &inbox)"
        ),
        "the flat arm is still the steered turn, taking the text and the inbox",
    );
    assert!(
        !text.contains("turn_contained_observed("),
        "the contract-less contained entry point is gone; every contained turn takes one",
    );

    // The decision, not its formatting: the contract exists only where a
    // containment does, whatever shape the closure that builds it is written in.
    let built = text
        .split_once("let contract = containment")
        .expect("the contract is built from the containment and nothing else")
        .1;
    let head = &built[..120.min(built.len())];
    assert!(
        head.contains(".map(") && head.contains("io_cli::contract::session("),
        "a session with no containment builds no contract at all: {head:?}",
    );
}
