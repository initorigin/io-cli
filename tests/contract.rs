//! **F1 and F2** — what a session turn's contract carries, and what it must not.

use std::path::PathBuf;
use std::sync::Arc;

use io_cli::contract::{session, Capabilities, PROMPT};
use io_cli::settings::CliSettings;
use io_harness::TaskContract;

mod support;

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
/// **Three fields are deliberately not io-harness's own.** The first is the step
/// cap.
/// `TaskContract::workspace` caps a turn at twelve, which a turn that reads a
/// repository and writes a file reaches with the work half done — an operator
/// saw `error: step_cap_reached` under an unfinished answer, which is a ceiling
/// reported as a failure. The second is the responder, which 0.12.0 puts on every
/// turn rather than only a contained one — a question asked on an ordinary turn
/// used to pause the run with nobody offered it. The third is 0.13.0's own system
/// prompt, which every turn carries because `SystemPrompt::Builtin` says what the
/// tools are and nothing about how to answer. Everything else still has to match,
/// and this test is what says so.
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
        .with_responder(responder)
        .with_system_prompt(io_harness::SystemPrompt::Append(PROMPT.to_string()));

    assert_eq!(format!("{built:?}"), format!("{default:?}"));
    // The number is a judgement and may move; what may not is that a turn stops
    // for a reason other than the cap. `a_turn_is_not_capped_at_twelve_steps`
    // reads it off the built contract, which is where it can actually be wrong.
}

/// **F1 — the prompt is on the contract itself, so both arms carry it.**
///
/// The difference the test above allows, asserted as the one it is: a contract
/// built here is `Append` with io-cli's own constant, and it is that whether the
/// turn can fan out or not — `f6_both_arms_are_handed_one_contract` is what makes
/// "the contract" singular, and this is what puts the prompt on it.
///
/// Sabotage: attach the prompt inside one arm's branch in `src/main.rs` — under
/// which this fails, and it fails by recreating the arm drift 0.12.0's F6 exists
/// to prevent.
#[test]
fn every_turn_carries_io_clis_own_system_prompt() {
    let (answerer, _questions) = io_cli::intent::channel();
    let responder: Arc<dyn io_harness::Responder> = Arc::new(answerer);
    let built = session(
        "bring the docs up to date",
        root(),
        &Capabilities::default(),
        responder,
        None,
    );

    assert_eq!(
        built.prompt,
        io_harness::SystemPrompt::Append(PROMPT.to_string()),
        "the manner is appended to io-harness's own description, not put in place of it",
    );
}

/// **F2 — the prompt names no provider and no model.**
///
/// io-cli is pointed at a catalogue of four hundred models by a flag, so a prompt
/// that told one of them what it was would be wrong on every other. The needles
/// are assembled from fragments at run time so this file does not match itself,
/// the way `tests/timing.rs` does it.
///
/// Sabotage: write `You are Claude` into the constant — under which only this
/// fails, and it fails on the one property that makes the prompt shippable
/// against a catalogue nobody here chose.
#[test]
fn the_prompt_names_no_vendor_and_no_model() {
    let lowered = PROMPT.to_ascii_lowercase();
    let vendors = [
        ["open", "router"].concat(),
        ["anth", "ropic"].concat(),
        ["open", "ai"].concat(),
        ["cla", "ude"].concat(),
        ["g", "pt"].concat(),
        ["deep", "seek"].concat(),
        ["gem", "ini"].concat(),
        ["ll", "ama"].concat(),
        ["mis", "tral"].concat(),
        ["gr", "ok"].concat(),
        ["qw", "en"].concat(),
    ];
    for vendor in &vendors {
        assert!(
            !lowered.contains(vendor.as_str()),
            "the prompt names `{vendor}`, and the model reading it is chosen by a flag",
        );
    }

    // And no first-person claim about a family, which is the same defect written
    // without a brand name in it.
    for claim in [
        ["i am a", " language model"].concat(),
        ["i am an", " ai assistant"].concat(),
        ["trained by", " "].concat(),
    ] {
        assert!(
            !lowered.contains(claim.trim_end()),
            "the prompt claims `{claim}` on behalf of a model it does not know",
        );
    }
}

/// **F3 — the prompt states no capability io-harness has not granted.**
///
/// What the agent may reach is composed around this text by the harness, from the
/// contract — so a prompt naming a tool would be lying on every turn whose
/// contract omits it, which for a browser tool is every default session. The
/// names are read out of the locked io-harness source rather than copied here,
/// through the reader `tests/support/mod.rs` already has for `EventKind` and
/// `RunOutcome`.
///
/// Sabotage: add "you can browse the web" to the constant — under which only this
/// fails.
#[test]
fn the_prompt_names_no_tool_the_contract_may_not_carry() {
    let lowered = PROMPT.to_ascii_lowercase();
    let names = support::harness_tool_names();
    assert!(
        names.len() > 20,
        "the reader found {} tool names, which is not the workspace set",
        names.len(),
    );
    for name in &names {
        assert!(
            !lowered.contains(name.as_str()),
            "the prompt names the `{name}` tool, which a turn's contract may not carry",
        );
        // The spaced spelling too: "read file" is the same claim as `read_file`
        // written for a person, and it is the one a prose prompt reaches for.
        let spaced = name.replace('_', " ");
        assert!(
            !lowered.contains(spaced.as_str()),
            "the prompt names the `{name}` tool in words",
        );
    }
}

/// The prompt is a bounded cost, and the bound is written down.
///
/// It is prepaid on every turn of every session, so its size is a fact about the
/// product rather than a detail of its prose. The number may move when the words
/// do; what may not is that nobody notices it moving.
#[test]
fn the_prompt_is_bounded_and_the_number_is_written_down() {
    assert!(
        PROMPT.len() <= 1_600,
        "the prompt is {} bytes and the bound is 1600",
        PROMPT.len(),
    );
    assert!(
        PROMPT.len() >= 300,
        "the prompt is {} bytes, which is not a manner",
        PROMPT.len(),
    );
}

/// **F4 — io-harness's own sections survive the append.**
///
/// `Append` is a name, and a name is not a property. What this asserts is the
/// composed prompt a provider was really handed on a turn that really ran, with a
/// skill discovered, a repository instruction set and a boundary being enforced —
/// every section the harness composes, alongside io-cli's own text.
///
/// It has to be a turn: `run::prompts::compose` is `pub(super)` in io-harness and
/// `EventKind::PromptComposed` carries the prompt's size rather than its text, so
/// `CompletionRequest.system` is the only place the composition is readable from
/// outside that crate.
///
/// Sabotage: switch `Append` to `Replace` in `contract::session` — under which
/// this fails, and it fails on the harness's own description of the request it is
/// building, which io-cli would then be silently standing in for.
#[tokio::test]
async fn f4_io_harnesss_own_sections_survive_the_append() {
    let dir = tempfile::tempdir().expect("a temp dir");
    let skills = dir.path().join("skills");
    std::fs::create_dir(&skills).expect("mkdir");
    std::fs::write(
        skills.join("migrations.md"),
        "---\nname: migrations\ndescription: how this repo changes a schema\n---\nbody\n",
    )
    .expect("write");

    let (answerer, _questions) = io_cli::intent::channel();
    let contract = session(
        "say hello",
        dir.path().to_path_buf(),
        &Capabilities {
            skills: Some(skills),
            ..Capabilities::default()
        },
        Arc::new(answerer),
        None,
    )
    .with_instruction("Project instructions from `AGENTS.md`:\nprefer small diffs");

    let store = io_harness::Store::memory().expect("an in-memory store");
    let mut opened = io_harness::Session::open(&store, dir.path()).expect("a session");
    let provider = support::Capturing::new();
    // Not permissive, so there is a boundary to be told about at all:
    // `boundary_section` returns `None` for a permissive policy outside a
    // container, and a test asserting the section under one would be asserting
    // nothing.
    let policy = io_harness::Policy::permissive()
        .layer("test")
        .deny_read("private/**");
    opened
        .turn_bounded_observed(
            &contract,
            &provider,
            &store,
            &policy,
            &io_harness::ApproveAll,
            &io_harness::observe::Ignore,
        )
        .await
        .expect("a captured turn cannot fail");

    let systems = provider.systems();
    assert_eq!(
        systems.len(),
        2,
        "the turn opened conversationally and then did work, which is two descriptions",
    );

    // io-cli's own block, whole, on **both** — the opening a turn may answer from
    // and the workspace description every later step runs on. A release that
    // appended to one of them would be an agent with a manner only while it was
    // deciding whether to have one.
    for composed in &systems {
        assert!(
            composed.contains(PROMPT),
            "io-cli's manner is not in the composed prompt:\n{composed}",
        );
    }

    let composed = &systems[1];
    // The harness's framing and its tool catalogue, which `Replace` would take.
    for section in [
        "You are an agent working across a repository",
        "read_file",
        "Skills available to you",
        "migrations",
        "This repository carries its own guidance",
        "prefer small diffs",
        "Your boundary.",
    ] {
        assert!(
            composed.contains(section),
            "the composed prompt lost {section:?}:\n{composed}",
        );
    }
    // And the crate's own last word is still last. **Which** last word depends on
    // the turn: a first completion that may still be answered ends with the
    // sentence deciding what a turn is, and a step of decided work ends with the
    // crate's "call tools". Both are io-harness's, and both are emitted after
    // anything an embedder or a repository supplied — which is the property, not
    // the wording.
    let tail = composed.trim_end();
    assert!(
        tail.ends_with("act.") || tail.ends_with("call tools."),
        "the crate's ending is not at the end:\n{composed}",
    );
    assert!(
        !tail.ends_with(PROMPT.trim_end()),
        "io-cli's block has the last word, which is what `Append` exists to prevent",
    );
    // Order, not just presence: io-cli's block sits after the catalogue and
    // before the boundary, which is what makes it an append rather than a
    // preamble that could weaken the sentence deciding how a turn ends.
    let ours = composed.find(PROMPT).expect("io-cli's block");
    let catalogue = composed.find("read_file").expect("the catalogue");
    let boundary = composed.find("Your boundary.").expect("the boundary");
    assert!(
        catalogue < ours && ours < boundary,
        "io-cli's block is at {ours}, the catalogue at {catalogue}, the boundary at {boundary}",
    );
}

/// One constant, one module, one call site.
///
/// A second place the agent's manner is set is a second thing to keep true, which
/// is the rule `contract::session` was built on — and it is how a release that
/// sets a prompt on one arm's turn and not the other's happens.
#[test]
fn the_manner_is_set_in_exactly_one_place() {
    let src = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("src");
    let mut sites = Vec::new();
    for entry in std::fs::read_dir(&src).expect("src/ is readable").flatten() {
        let path = entry.path();
        if path.extension().is_none_or(|ext| ext != "rs") {
            continue;
        }
        let text = std::fs::read_to_string(&path).expect("a source file");
        // The whitespace is removed first: rustfmt decides where a builder chain
        // breaks, and a grep for `with_system_prompt(` would go blind the day it
        // put the argument on its own line.
        let squashed: String = text.chars().filter(|c| !c.is_whitespace()).collect();
        let calls = squashed.matches("with_system_prompt(").count();
        if calls > 0 {
            sites.push((path, calls));
        }
    }

    assert_eq!(
        sites.len(),
        1,
        "the prompt is attached in one module, and these set it: {sites:?}",
    );
    let (path, calls) = &sites[0];
    assert_eq!(*calls, 1, "one call site, not {calls}");
    assert!(
        path.ends_with("contract.rs"),
        "the module that owns the contract owns its manner, not {}",
        path.display(),
    );
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
