//! **F1 and F2** — what a session turn's contract carries, and what it must not.

use std::path::PathBuf;
use std::sync::{Arc, Mutex, MutexGuard, OnceLock};
use std::time::Duration;

use io_cli::contract::{server_notices, session, Capabilities, PROMPT};
use io_cli::settings::CliSettings;
use io_harness::{Config, TaskContract};

mod support;

fn root() -> PathBuf {
    PathBuf::from("/tmp/io-cli-contract")
}

/// A configuration with nothing in it and no file behind it.
///
/// For every assertion in this file that is about something other than the
/// configuration. `Config::from_toml` parses in memory and reads nothing off
/// disk, so it cannot pick up whatever the person running the suite happens to
/// have written — which is the reason `tests/exec.rs` uses it too. It is **not**
/// good enough for a fixture that asserts on `[instructions]`; see
/// [`discovered`].
fn nothing() -> Config {
    Config::from_toml("").expect("an empty configuration parses")
}

/// Held by every test in this file that calls `Config::discover`.
///
/// The environment is process-wide and these tests share a process, so two of
/// them pointing `IO_CONFIG_HOME` at different directories at once would make
/// each other's discovery wrong — intermittently, on a loaded machine, which is
/// the most expensive kind of failure to diagnose. The same shape
/// `tests/wizard.rs` uses, and for the same reason.
fn env_lock() -> MutexGuard<'static, ()> {
    static LOCK: Mutex<()> = Mutex::new(());
    LOCK.lock().unwrap_or_else(|poisoned| poisoned.into_inner())
}

/// One empty user scope for this whole file, set once.
///
/// `Config::discover` reads `$IO_CONFIG`, `$IO_CONFIG_HOME`, `$XDG_CONFIG_HOME`
/// and `$HOME` before it reads the project's own `io.toml`, so a fixture built on
/// a developer's own machine would be asserting on whatever that person happens
/// to have configured. Pointed at a directory holding no file, the user scope is
/// skipped and the project file is the whole of the configuration.
fn no_user_scope() {
    static HOME: OnceLock<tempfile::TempDir> = OnceLock::new();
    HOME.get_or_init(|| {
        let dir = tempfile::tempdir().expect("a temporary directory");
        std::env::set_var("IO_CONFIG_HOME", dir.path());
        // `IO_CONFIG` names a file outright and would win over the directory, so
        // an inherited one has to go or the fixture is not the fixture.
        std::env::remove_var("IO_CONFIG");
        dir
    });
}

/// A workspace on disk with the named files in it, read the way `io` reads one.
///
/// **Through `Config::discover` and never `Config::from_toml`**, because
/// `[instructions]` is applied from a field only discovery populates: a
/// `from_toml` fixture asserts an empty instruction list against a file that
/// names one, and passes. Every fixture below that asserts on a section goes
/// through here.
///
/// The caller holds [`env_lock`] for the call, and holds the returned directory
/// for as long as the configuration is used — `TempDir` deletes the workspace
/// when it drops.
fn discovered(files: &[(&str, &str)]) -> (tempfile::TempDir, Config) {
    no_user_scope();
    let dir = tempfile::tempdir().expect("a workspace");
    for (name, body) in files {
        std::fs::write(dir.path().join(name), body).expect("the fixture file");
    }
    let config = Config::discover(dir.path()).expect("the fixture configuration parses");
    (dir, config)
}

/// A file whose every applicable section is set to a value distinguishable from
/// both io-harness's default and io-cli's own floor.
///
/// A value that happens to equal a default proves nothing, so none of these is
/// one: the step cap is neither twelve nor a thousand, the exec timeout is not
/// nine hundred seconds, the retry base is not five hundred milliseconds, the
/// stall window is not three, and the sandbox limits are none of the four a
/// default `SandboxConfig` carries.
///
/// `[browser]` is deliberately absent: io-harness refuses it in a project-scoped
/// file, because it names a program to execute on this machine and `io.toml`
/// arrives with a `git clone`. `run.templates` is absent because `apply_to` does
/// not apply it — it is reachable only through `Config::templates()`, which is
/// where `src/commands.rs` already reads it from.
const EVERY_SECTION: &str = r#"
[run]
max_steps = 44
max_duration_secs = 909
max_tokens = 123456
max_retries = 9
exec_timeout_secs = 111
skills = "/tmp/io-cli-fixture-skills"
max_read_chars = 4321
max_wait_secs = 77

[run.retry]
base_ms = 2500
max_ms = 61000

[run.stall]
window = 8
max_replans = 4

[run.context]
max_tokens = 31000
share = 0.25

[run.commit_identity]
name = "fixture committer"
email = "fixture@io-cli.invalid"

[sandbox]
mode = "read-only"

[sandbox.limits]
max_cpu_secs = 1800
max_wall_secs = 3600
max_memory_bytes = 1073741824
max_processes = 33
max_open_files = 99

[[mcp]]
id = "fixture-docs"
transport = "stdio"
command = "mcp-fixture-docs"

[[lsp]]
id = "fixture-analyzer"
command = "lsp-fixture"
extensions = ["rs"]

[[agent]]
name = "fixture-searcher"
model = "a-cheap-model"
deny_write = true

[web]
search = true
fetch = true
max_uses = 6
allowed_domains = ["docs.rs"]
blocked_domains = ["example.invalid"]

[memory]
max_entries = 12
max_chars = 3456
max_entry_chars = 789

[instructions]
files = ["GUIDE.md"]
"#;

/// What `[instructions] files = ["GUIDE.md"]` points at.
const GUIDE: &str = "never widen the boundary in a fixture";

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
///
/// **0.14.0 — this is the criterion that decides whether the release ships.**
/// `contract::session` now calls `Config::apply_to` unconditionally and attaches
/// `[sandbox]`, and both of those are ways to change every existing operator's
/// turn without saying so. The workspace is an **empty** temporary directory
/// rather than a path that happens not to exist, so no `AGENTS.md` is discovered
/// and no instruction arrives from a repository nobody chose.
///
/// Sabotage: attach `[sandbox]` when the file has none — under which only this
/// test fails, on a contract carrying a default `SandboxConfig`'s real ceilings
/// (sixty CPU seconds, a hundred and twenty wall seconds, two gibibytes, five
/// hundred and twelve descriptors) where it carried `SandboxLimits::none()`.
#[test]
fn f2_nothing_configured_is_the_contract_the_session_built_before() {
    let _guard = env_lock();
    let (dir, config) = discovered(&[]);
    let root = dir.path().to_path_buf();

    let (answerer, _questions) = io_cli::intent::channel();
    let responder: Arc<dyn io_harness::Responder> = Arc::new(answerer);
    let built = session(
        "bring the docs up to date",
        root.clone(),
        &config,
        &Capabilities::default(),
        responder.clone(),
        None,
    );
    let default = TaskContract::workspace("bring the docs up to date", root)
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
        &nothing(),
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
        &nothing(),
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
        &nothing(),
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
    let contract = session(
        "add a test",
        root(),
        &nothing(),
        &caps,
        Arc::new(answerer),
        None,
    );
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
        &nothing(),
        &Capabilities::default(),
        responder.clone(),
        None,
    );
    let two = session(
        "a goal",
        root(),
        &nothing(),
        &Capabilities::default(),
        responder,
        None,
    );
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
        &nothing(),
        &Capabilities::default(),
        responder.clone(),
        None,
    );
    assert!(built.max_steps >= 1_000, "{}", built.max_steps);

    // And the operator can still say otherwise, in either direction.
    let asked = session(
        "bring the docs up to date",
        root(),
        &nothing(),
        &Capabilities {
            max_steps: Some(25),
            ..Capabilities::default()
        },
        responder,
        None,
    );
    assert_eq!(asked.max_steps, 25);
}

/// **F1 — one contract, built once, over the fields both arms share.**
///
/// Asserted field by field rather than by both arms calling the same function,
/// because the assertion has to survive somebody adding a builder to one call
/// site. The two builders were never assemblies of the same fields — through
/// 0.13.1 they overlapped on `TaskContract::workspace` and nothing else — so what
/// is held together here is the config-derived half, and the three fields that
/// are arm-specific by construction are asserted to **differ** rather than being
/// left out:
///
/// - the **responder**, because a session has a person behind it and `io exec`
///   has nothing that can answer; a responder it could not serve would pause the
///   run rather than refuse,
/// - the **plan gate**, because `/plan on` is a session keystroke,
/// - the **system prompt**, because [`PROMPT`] tells the model it is rendered in
///   an eighty-column pane whose earlier output has scrolled, which is false of
///   `io exec --json`.
///
/// The step cap is **not** in that list from 0.14.0: both arms take io-cli's
/// floor, which `f4_the_step_floor_sits_under_the_file_and_over_the_harness`
/// asserts the ordering of.
///
/// Sabotage: apply `[run]` in only one arm — under which only this test fails,
/// naming the field that differs.
#[test]
fn f1_both_arms_carry_the_same_configuration_field_by_field() {
    let _guard = env_lock();
    let (dir, config) = discovered(&[("io.toml", EVERY_SECTION), ("GUIDE.md", GUIDE)]);

    // The harness's own session, because `exec::contract` reads its root off one
    // — and the interactive arm is handed that same root, so a difference here
    // cannot be a difference of workspace.
    let store = io_harness::Store::memory().expect("an in-memory store");
    let opened = io_harness::Session::open(&store, dir.path()).expect("a session");
    let root = opened.root().to_path_buf();

    let (answerer, _questions) = io_cli::intent::channel();
    let (gate, _plans) = io_cli::plan::channel();
    let interactive = session(
        "make the suite pass",
        root,
        &config,
        &Capabilities::default(),
        Arc::new(answerer),
        Some(Arc::new(gate) as Arc<dyn io_harness::PlanGate>),
    );
    let headless = io_cli::exec::contract(&config, &opened, "make the suite pass".into(), None);

    // The twelve `[run]` keys.
    assert_eq!(interactive.max_steps, headless.max_steps, "max_steps");
    assert_eq!(interactive.max_duration, headless.max_duration, "duration");
    assert_eq!(interactive.max_tokens, headless.max_tokens, "tokens");
    assert_eq!(interactive.max_retries, headless.max_retries, "retries");
    assert_eq!(
        interactive.exec_timeout, headless.exec_timeout,
        "exec_timeout",
    );
    assert_eq!(interactive.skills, headless.skills, "skills");
    assert_eq!(interactive.retry, headless.retry, "retry");
    assert_eq!(interactive.stall, headless.stall, "stall");
    assert_eq!(interactive.context, headless.context, "context");
    assert_eq!(
        interactive.max_read_chars, headless.max_read_chars,
        "max_read_chars",
    );
    assert_eq!(
        interactive.max_wait_secs, headless.max_wait_secs,
        "max_wait_secs",
    );
    assert_eq!(
        interactive.commit_identity, headless.commit_identity,
        "commit_identity",
    );

    // And the sections beside `[run]`.
    assert_eq!(
        interactive.exec_sandbox, headless.exec_sandbox,
        "[sandbox], which `Config::apply_to` does not carry and each arm used to \
         attach for itself",
    );
    assert_eq!(interactive.agents, headless.agents, "[[agent]]");
    assert_eq!(interactive.web, headless.web, "[web]");
    assert_eq!(interactive.memory, headless.memory, "[memory]");
    assert_eq!(
        interactive.instructions, headless.instructions,
        "[instructions]",
    );
    assert_eq!(interactive.mcp, headless.mcp, "[[mcp]]");
    assert_eq!(interactive.lsp, headless.lsp, "[[lsp]]");
    assert_eq!(
        interactive.browser, headless.browser,
        "[browser] — absent in both, because io-harness refuses the table in a \
         project-scoped file and this fixture is one",
    );

    // The three that are arm-specific by construction, asserted as differences
    // rather than skipped: a field nobody asserts is a field that can quietly
    // become the same on both arms, which for the prompt would mean telling an
    // `io exec --json` reader about a pane that has scrolled.
    assert!(
        interactive.responder.is_some() && headless.responder.is_none(),
        "`io exec` has nobody to answer a question, so it is given no responder",
    );
    assert!(
        interactive.plan_gate.is_some() && headless.plan_gate.is_none(),
        "`/plan on` is a session keystroke and registering a gate is what turns \
         io-harness's planning phase on",
    );
    assert_eq!(
        interactive.prompt,
        io_harness::SystemPrompt::Append(PROMPT.to_string()),
    );
    assert_eq!(headless.prompt, io_harness::SystemPrompt::Builtin);
}

/// **F3 — every applicable section reaches an interactive turn.**
///
/// One assertion per key, so a key that stops arriving is named rather than
/// folded into a single failure, and every value in [`EVERY_SECTION`] is
/// distinguishable from both the harness default and io-cli's own floor.
///
/// **The fixture is built through `Config::discover` and never
/// `Config::from_toml`**: `[instructions]` is applied from a field only discovery
/// populates, so a `from_toml` fixture asserts an empty instruction list against
/// a file that names one, and passes.
///
/// Sabotage: drop the `Config::apply_to` call — under which all of this fails and
/// F2 passes, which is the pair that distinguishes "nothing is applied" from "the
/// wrong thing is applied".
#[test]
fn f3_every_applicable_section_of_the_file_reaches_a_session_turn() {
    let _guard = env_lock();
    let (dir, config) = discovered(&[("io.toml", EVERY_SECTION), ("GUIDE.md", GUIDE)]);

    let (answerer, _questions) = io_cli::intent::channel();
    let contract = session(
        "make the suite pass",
        dir.path().to_path_buf(),
        &config,
        &Capabilities::default(),
        Arc::new(answerer),
        None,
    );

    // `[run]`, all twelve applicable keys. `run.templates` is the thirteenth and
    // `Config::apply_to` does not apply it — it is reachable only through
    // `Config::templates()`, which is where `commands::templates` reads it — so
    // its absence here is a known fact rather than a later surprise.
    assert_eq!(contract.max_steps, 44, "[run] max_steps");
    assert_eq!(
        contract.max_duration,
        Some(Duration::from_secs(909)),
        "[run] max_duration_secs",
    );
    assert_eq!(contract.max_tokens, Some(123_456), "[run] max_tokens");
    assert_eq!(contract.max_retries, 9, "[run] max_retries");
    assert_eq!(
        contract.exec_timeout,
        Duration::from_secs(111),
        "[run] exec_timeout_secs",
    );
    assert_eq!(
        contract.skills,
        Some(PathBuf::from("/tmp/io-cli-fixture-skills")),
        "[run] skills",
    );
    assert_eq!(
        contract.retry.base,
        Duration::from_millis(2_500),
        "[run.retry] base_ms",
    );
    assert_eq!(
        contract.retry.max,
        Duration::from_millis(61_000),
        "[run.retry] max_ms",
    );
    assert_eq!(contract.stall.window, 8, "[run.stall] window");
    assert_eq!(contract.stall.max_replans, 4, "[run.stall] max_replans");
    assert_eq!(
        contract.context.max_tokens, 31_000,
        "[run.context] max_tokens",
    );
    assert_eq!(contract.context.share, 0.25, "[run.context] share");
    assert_eq!(contract.max_read_chars, Some(4_321), "[run] max_read_chars");
    assert_eq!(contract.max_wait_secs, Some(77), "[run] max_wait_secs");
    assert_eq!(
        contract.commit_identity.name, "fixture committer",
        "[run.commit_identity] name — inside `[run]`, and therefore behind \
         `apply_to`'s early return on a file with no `[run]` table",
    );
    assert_eq!(
        contract.commit_identity.email, "fixture@io-cli.invalid",
        "[run.commit_identity] email",
    );

    // `[sandbox]`, which `Config::apply_to` does not carry at all.
    assert_eq!(
        contract.exec_sandbox.mode,
        io_harness::ExecMode::ReadOnly,
        "[sandbox] mode",
    );
    assert_eq!(
        contract.exec_sandbox.limits.max_cpu_secs,
        Some(1_800),
        "[sandbox.limits] max_cpu_secs",
    );
    assert_eq!(
        contract.exec_sandbox.limits.max_wall_secs,
        Some(3_600),
        "[sandbox.limits] max_wall_secs",
    );
    assert_eq!(
        contract.exec_sandbox.limits.max_memory_bytes,
        Some(1_073_741_824),
        "[sandbox.limits] max_memory_bytes",
    );
    assert_eq!(
        contract.exec_sandbox.limits.max_processes,
        Some(33),
        "[sandbox.limits] max_processes",
    );
    assert_eq!(
        contract.exec_sandbox.limits.max_open_files,
        Some(99),
        "[sandbox.limits] max_open_files",
    );

    // `[[agent]]`.
    assert_eq!(contract.agents.names(), ["fixture-searcher"], "[[agent]]");
    assert!(
        contract
            .agents
            .get("fixture-searcher")
            .expect("the roster carries the fixture agent")
            .deny_write,
        "[[agent]] deny_write",
    );

    // `[web]`, which is a capability rather than a preference: it grants the
    // model provider-executed search and fetch, and the vendor dials the URL, so
    // the local policy's `net` rule is not what governs it.
    assert_eq!(
        contract.web,
        Some(
            io_harness::WebAccess::search()
                .with_fetch()
                .max_uses(6)
                .allow("docs.rs")
                .block("example.invalid")
        ),
        "[web]",
    );

    // `[memory]`.
    assert_eq!(contract.memory.max_entries, 12, "[memory] max_entries");
    assert_eq!(contract.memory.max_chars, 3_456, "[memory] max_chars");
    assert_eq!(
        contract.memory.max_entry_chars, 789,
        "[memory] max_entry_chars",
    );

    // `[instructions]`, the one section a `Config::from_toml` fixture cannot
    // assert: the list would be empty on both sides and the assertion vacuous.
    assert_eq!(
        contract.instructions,
        vec![format!("Project instructions from `GUIDE.md`:\n{GUIDE}")],
        "[instructions]",
    );
}

/// **F4 — precedence runs weakest to strongest, and the floor does not outrank
/// the file.**
///
/// Sabotage: apply the floor after `Config::apply_to` — under which only the
/// first of the three below fails, which is the ordering defect that would
/// otherwise ship looking like a working feature.
#[test]
fn f4_the_step_floor_sits_under_the_file_and_over_the_harness() {
    let _guard = env_lock();
    let (answerer, _questions) = io_cli::intent::channel();
    let responder: Arc<dyn io_harness::Responder> = Arc::new(answerer);

    // A file that lowers the cap is obeyed. The floor is applied first, so a
    // `[run] max_steps` the operator actually wrote overwrites it — a floor
    // applied last would honour a file that raises the cap and ignore one that
    // lowers it, which is the half-working feature this arm exists to catch.
    let (written, config) = discovered(&[("io.toml", "[run]\nmax_steps = 20\n")]);
    let asked = session(
        "a goal",
        written.path().to_path_buf(),
        &config,
        &Capabilities::default(),
        responder.clone(),
        None,
    );
    assert_eq!(
        asked.max_steps, 20,
        "`[run] max_steps` beats io-cli's floor"
    );

    // A file with no `[run]` table takes the floor, not io-harness's twelve.
    let (bare, empty) = discovered(&[]);
    let unasked = session(
        "a goal",
        bare.path().to_path_buf(),
        &empty,
        &Capabilities::default(),
        responder.clone(),
        None,
    );
    assert_eq!(
        unasked.max_steps,
        io_cli::contract::MAX_STEPS,
        "a file that says nothing gets io-cli's floor, not the harness's twelve",
    );

    // And `[app.io-cli] max_steps` is the strongest layer, over a `[run]` that
    // named a number. Two spellings for one number, with the more specific scope
    // winning; the key is deprecated in this release and removed in 0.16.0.
    let deprecated = session(
        "a goal",
        written.path().to_path_buf(),
        &config,
        &Capabilities {
            max_steps: Some(7),
            ..Capabilities::default()
        },
        responder,
        None,
    );
    assert_eq!(
        deprecated.max_steps, 7,
        "`[app.io-cli] max_steps` is applied after `Config::apply_to` and beats it",
    );
}

/// **F5 — servers in both scopes are merged, and a collision is named.**
///
/// `TaskContract::with_mcp` and `with_lsp` assign the whole collection, so
/// applying `[[mcp]]` and then `[[app.io-cli.mcp]]` in sequence leaves a contract
/// holding one list where it should hold two — an operator with servers in both
/// scopes would silently lose one set.
///
/// Sabotage: apply the two in sequence without merging — under which only this
/// test fails, on a contract holding one list where it should hold two, which is
/// precisely the silent loss this criterion exists to prevent.
#[test]
fn f5_servers_in_both_scopes_are_merged_and_a_collision_is_named() {
    let _guard = env_lock();
    let (dir, config) = discovered(&[(
        "io.toml",
        r#"
        [[mcp]]
        id = "shared"
        transport = "stdio"
        command = "wide-shared"

        [[mcp]]
        id = "wide-only"
        transport = "stdio"
        command = "wide-only"

        [[lsp]]
        id = "shared-lsp"
        command = "wide-lsp"
        extensions = ["rs"]

        [[app.io-cli.mcp]]
        id = "shared"
        transport = "stdio"
        command = "narrow-shared"

        [[app.io-cli.mcp]]
        id = "narrow-only"
        transport = "stdio"
        command = "narrow-only"

        [[app.io-cli.lsp]]
        id = "shared-lsp"
        command = "narrow-lsp"
        extensions = ["rs"]
        "#,
    )]);

    let (stored, complaint) = io_cli::settings::stored(&config);
    assert!(complaint.is_none(), "{complaint:?}");
    let caps = Capabilities::stored(stored.as_ref());

    let (answerer, _questions) = io_cli::intent::channel();
    let contract = session(
        "a goal",
        dir.path().to_path_buf(),
        &config,
        &caps,
        Arc::new(answerer),
        None,
    );

    let ids: Vec<&str> = contract.mcp.iter().map(|s| s.id.as_str()).collect();
    assert_eq!(
        ids,
        ["shared", "wide-only", "narrow-only"],
        "both scopes reach the turn, in the file's own order",
    );
    let shared = contract
        .mcp
        .iter()
        .find(|s| s.id == "shared")
        .expect("the collided id is still on the contract exactly once");
    assert!(
        format!("{shared:?}").contains("narrow-shared"),
        "the `[app.io-cli]` entry wins the collision, being the more specific \
         scope: {shared:?}",
    );

    let lsp: Vec<&str> = contract.lsp.iter().map(|s| s.id.as_str()).collect();
    assert_eq!(lsp, ["shared-lsp"], "and the same for `[[lsp]]`");
    assert_eq!(
        contract.lsp[0].command, "narrow-lsp",
        "with the `[app.io-cli]` entry winning there too",
    );

    // And the dropped entry is named, so an operator who wrote a server twice is
    // told which of the two the session is running rather than discovering it.
    let notices = server_notices(&config, &caps);
    assert_eq!(notices.len(), 2, "one per collision: {notices:?}");
    assert!(
        notices[0].contains("`shared`")
            && notices[0].contains("[[mcp]]")
            && notices[0].contains("[[app.io-cli.mcp]]"),
        "{notices:?}",
    );
    assert!(
        notices[1].contains("`shared-lsp`")
            && notices[1].contains("[[lsp]]")
            && notices[1].contains("[[app.io-cli.lsp]]"),
        "{notices:?}",
    );

    // A file with servers in one scope only loses nothing and says nothing.
    let (quiet, one_scope) = discovered(&[(
        "io.toml",
        "[[mcp]]\nid = \"only\"\ntransport = \"stdio\"\ncommand = \"only\"\n",
    )]);
    let _ = &quiet;
    assert!(
        server_notices(&one_scope, &Capabilities::default()).is_empty(),
        "a notice about a duplicate nobody wrote is noise that teaches operators \
         to stop reading the start-up lines",
    );
}
