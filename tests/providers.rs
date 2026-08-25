//! F7, F8 and F9 — the chain, the presets, and the live verification.

use std::sync::{Mutex, MutexGuard};

use io_cli::providers::{self, Credential};
use io_harness::config::Config;

const THREE: &str = "\
[[provider]]
kind = \"openrouter\"
model = \"anthropic/claude-sonnet-4\"

# the cheap one
[[provider]]
kind = \"compatible\"
preset = \"groq\"
model = \"llama-3.3-70b-versatile\"
api_key = \"${env:IO_CLI_TEST_GROQ_KEY}\"

[[provider]]
kind = \"compatible\"
preset = \"ollama\"
model = \"llama3.2\"
";

/// `Config::discover` reads `IO_CONFIG` at call time, and the fixtures below set
/// it; serialised so two tests cannot see each other's file.
fn env_lock() -> MutexGuard<'static, ()> {
    static LOCK: Mutex<()> = Mutex::new(());
    LOCK.lock().unwrap_or_else(|poisoned| poisoned.into_inner())
}

/// A discovered configuration, from a real file.
///
/// `Config::from_toml` is not enough here: the credential column is read from
/// the file the origin names, because io-harness substitutes `${env:…}` while it
/// parses — so a config with no path behind it cannot show an operator which
/// variable they wrote.
struct Fixture {
    _dir: tempfile::TempDir,
    config: Config,
}

fn config(toml: &str) -> Fixture {
    let _guard = env_lock();
    // The variable the fixture points at has to exist, or the load fails
    // outright — io-harness resolves an indirection at parse time and refuses an
    // unset one rather than carrying it through.
    std::env::set_var("IO_CLI_TEST_GROQ_KEY", "gsk-not-a-real-key");

    let dir = tempfile::tempdir().expect("a temporary directory");
    let path = dir.path().join("io.toml");
    std::fs::write(&path, toml).expect("the fixture is written");

    std::env::set_var("IO_CONFIG", &path);
    let config = Config::discover(dir.path()).expect("the fixture parses");
    std::env::remove_var("IO_CONFIG");

    Fixture { _dir: dir, config }
}

#[test]
fn f7_the_chain_is_the_file_s_order_head_and_tail_together() {
    let fixture = config(THREE);
    let config = &fixture.config;
    let chain = providers::chain(config);

    assert_eq!(chain.len(), 3, "the head and the tail, joined");
    assert_eq!(chain[0].kind, "openrouter");
    assert_eq!(chain[1].kind, "groq");
    assert_eq!(chain[2].kind, "ollama");
    for (n, entry) in chain.iter().enumerate() {
        assert_eq!(entry.index, n, "position is the fallback order");
    }

    // And it really is io-harness's own split being rejoined, rather than a
    // second reading of the array.
    assert_eq!(config.fallback_specs().len(), 2);
}

#[test]
fn f7_reordering_the_panel_reorders_the_file_and_therefore_the_chain() {
    let edit = providers::promote(1).expect("an entry that is not already first can be promoted");
    let after = io_cli::edit::apply(THREE, &[edit]).unwrap();

    let moved = config(&after);
    let chain = providers::chain(&moved.config);
    assert_eq!(chain[0].kind, "groq", "the promoted entry is the one now used");
    assert_eq!(chain[1].kind, "openrouter");

    // io-harness agrees, which is the half that matters: the head is what a run
    // takes and the rest is what it falls back to.
    let head = moved.config.provider_spec().expect("a head");
    assert!(
        matches!(head, io_harness::ProviderSpec::Compatible { preset, .. }
            if preset.as_deref() == Some("groq")),
        "io-harness does not agree about which provider a run would use",
    );

    // The entry arrived with its own bytes.
    assert!(after.contains("# the cheap one"), "a comment was left behind");
    assert!(after.contains("IO_CLI_TEST_GROQ_KEY"), "the indirection was lost");
}

#[test]
fn f7_the_first_entry_cannot_be_promoted_and_the_last_cannot_be_demoted() {
    // Expressed as `None` rather than as a no-op edit, so a caller cannot draw a
    // control that does nothing.
    assert!(providers::promote(0).is_none());
    assert!(providers::demote(2, 3).is_none());
    assert!(providers::promote(2).is_some());
    assert!(providers::demote(0, 3).is_some());
}

#[test]
fn f7_an_added_provider_goes_last_because_that_is_the_end_of_the_chain() {
    let after = io_cli::edit::apply(
        THREE,
        &[providers::add("compatible", "qwen-max", Some("qwen"))],
    )
    .unwrap();
    let chain = providers::chain(&config(&after).config);

    assert_eq!(chain.len(), 4);
    assert_eq!(
        chain[3].kind, "qwen",
        "a new provider was inserted ahead of an existing one, which silently \
         changes which provider a run uses"
    );
    assert_eq!(chain[0].kind, "openrouter", "the head moved");
}

#[test]
fn f7_removing_an_entry_leaves_the_rest_in_order() {
    let after = io_cli::edit::apply(THREE, &[providers::remove(1)]).unwrap();
    let chain = providers::chain(&config(&after).config);

    let kinds: Vec<&str> = chain.iter().map(|e| e.kind.as_str()).collect();
    assert_eq!(kinds, vec!["openrouter", "ollama"]);
}

#[test]
fn f8_the_preset_list_is_the_harness_s_own() {
    // io-cli carries its own list because `preset_names()` is `pub(crate)`. This
    // is what stops that list drifting into an operator selecting a vendor and
    // being told "unknown provider preset".
    let harness = providers::harness_presets();

    assert!(
        !harness.is_empty(),
        "the refusal stopped naming the presets, so this gate cannot fail and is \
         no longer a control — read `Compatible::preset`'s error before trusting it",
    );
    assert_eq!(
        harness.len(),
        21,
        "io-harness's own test asserts 21: 13 hosted vendors and 8 local runtimes",
    );

    let mut mine: Vec<String> = providers::PRESETS.iter().map(|s| s.to_string()).collect();
    let mut theirs = harness.clone();
    mine.sort();
    theirs.sort();
    assert_eq!(mine, theirs, "io-cli's preset list has drifted from io-harness's");
}

#[test]
fn f8_every_preset_resolves_to_the_endpoint_the_harness_gives_it() {
    // The panel shows where a preset points, and it asks io-harness rather than
    // holding a second table of base URLs.
    for preset in providers::PRESETS {
        let endpoint = providers::endpoint_of(preset)
            .unwrap_or_else(|| panic!("{preset} resolves to no endpoint"));
        assert!(
            endpoint.starts_with("http"),
            "{preset} resolved to {endpoint:?}",
        );
    }
}

#[test]
fn f8_a_local_runtime_is_told_apart_from_a_hosted_vendor() {
    // Which decides whether a credential is asked for at all, and it is derived
    // from the base URL io-harness resolves rather than from a second list here.
    assert!(providers::is_local("ollama"));
    assert!(providers::is_local("lmstudio"));
    assert!(!providers::is_local("groq"));

    let local = providers::PRESETS
        .iter()
        .filter(|p| providers::is_local(p))
        .count();
    assert_eq!(local, 8, "io-harness's table is 13 hosted and 8 local");
}

#[test]
fn f8_a_preset_io_cli_invented_would_fail_the_gate() {
    // The control for the control: if io-cli's list gained a name io-harness
    // does not know, `harness_presets` would not contain it. Asserted directly
    // so the gate above is known to be able to fail.
    let harness = providers::harness_presets();
    assert!(!harness.contains(&"not-a-real-vendor".to_string()));
    assert!(harness.contains(&"groq".to_string()));
}

#[test]
fn n2_a_credential_is_described_and_never_shown() {
    let chain = providers::chain(&config(THREE).config);

    // No key written: the provider's own variable answers, and naming it is the
    // useful thing.
    assert_eq!(
        chain[0].credential,
        Credential::FromEnvironment("OPENROUTER_API_KEY")
    );
    // An indirection is shown as written — the variable's NAME is the information.
    assert_eq!(
        chain[1].credential,
        Credential::Indirect("${env:IO_CLI_TEST_GROQ_KEY}".into())
    );
    // A local runtime needs none, and is not reported as missing one.
    assert_eq!(chain[2].credential, Credential::NotNeeded);

    // And a key that IS written never reaches a rendered row.
    let written = config(
        "[[provider]]\nkind = \"openai\"\nmodel = \"gpt-4o\"\napi_key = \"sk-supersecret\"\n",
    );
    let chain = providers::chain(&written.config);
    assert_eq!(chain[0].credential, Credential::Written);
    for row in providers::rows(&chain) {
        let text = format!("{} {}", row.label, row.detail.unwrap_or_default());
        assert!(!text.contains("supersecret"), "a credential reached a row: {text}");
    }
}

#[test]
fn f9_the_verifier_is_the_wizard_s_and_there_is_only_one() {
    // F9's substance is that `/provider` and the wizard share one verifier
    // rather than growing a second. Asserted structurally, because the call
    // itself is a live endpoint and belongs in the release's live arm: the
    // functions exist, are public, and are the ones the wizard uses.
    // The driver is where both surfaces make the call — `src/wizard.rs` holds
    // the steps and `src/main.rs` runs them — so that is where the sharing is
    // visible. Nothing under `tests/` links the binary, so this reads the file,
    // which is how this repository already pins decisions made in it.
    let driver = std::fs::read_to_string("src/main.rs").expect("the driver");
    assert!(
        driver.contains("verify::credential"),
        "nothing calls `verify::credential` any more, so `/provider` sharing the \
         wizard's verifier is no longer the same claim",
    );
    assert!(
        driver.matches("verify::catalogue").count() >= 2,
        "the model catalogue is fetched from one place only, so the wizard and \
         `/provider` are not sharing it",
    );

    // And the panel's own module does not carry a second one.
    let panel = std::fs::read_to_string("src/providers.rs").expect("the panel");
    for forbidden in ["CompletionRequest", "reqwest", ".complete("] {
        assert!(
            !panel.contains(forbidden),
            "src/providers.rs names `{forbidden}`, which is a second verifier",
        );
    }
}
