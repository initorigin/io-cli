//! F7, F8 and F9 — the chain, the presets, and the live verification.

use std::sync::{Mutex, MutexGuard};

use io_cli::providers::{self, At, Credential, Endpoint};
use io_harness::config::{Config, Scope};

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
    let at = At::of(Scope::User, THREE, 1).expect("the file declares three entries");
    let edit = providers::promote(&at).expect("an entry that is not already first can be promoted");
    let after = io_cli::edit::apply(THREE, &[edit]).unwrap();

    let moved = config(&after);
    let chain = providers::chain(&moved.config);
    assert_eq!(
        chain[0].kind, "groq",
        "the promoted entry is the one now used"
    );
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
    assert!(
        after.contains("# the cheap one"),
        "a comment was left behind"
    );
    assert!(
        after.contains("IO_CLI_TEST_GROQ_KEY"),
        "the indirection was lost"
    );
}

#[test]
fn f7_the_first_entry_cannot_be_promoted_and_the_last_cannot_be_demoted() {
    // Expressed as `None` rather than as a no-op edit, so a caller cannot draw a
    // control that does nothing.
    let at = |index| At::of(Scope::User, THREE, index).expect("a declared position");
    assert!(providers::promote(&at(0)).is_none());
    assert!(providers::demote(&at(2)).is_none());
    assert!(providers::promote(&at(2)).is_some());
    assert!(providers::demote(&at(0)).is_some());

    // **The bound is read from the file, not passed in.** `demote` used to take
    // the length as a second argument, so the answer to "is there anywhere to
    // move to" came from whatever the caller happened to be counting — a
    // filtered view, or a chain rendered from a different `Config`. A position
    // past the end is now refused where it is built, once, rather than becoming
    // an `Edit` that fails somewhere further down.
    assert!(
        At::of(Scope::User, THREE, 3).is_none(),
        "a position past the end of the array was accepted",
    );
    assert!(At::of(Scope::User, "", 0).is_none());
}

#[test]
fn f7_an_added_provider_goes_last_because_that_is_the_end_of_the_chain() {
    let after = io_cli::edit::apply(
        THREE,
        &[providers::add(Endpoint::Preset("qwen"), "qwen-max", None)],
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
    let at = At::of(Scope::User, THREE, 1).expect("the middle link");
    let after = io_cli::edit::apply(THREE, &[providers::remove(&at)]).unwrap();
    let chain = providers::chain(&config(&after).config);

    let kinds: Vec<&str> = chain.iter().map(|e| e.kind.as_str()).collect();
    assert_eq!(kinds, vec!["openrouter", "ollama"]);
}

#[test]
fn f7_demoting_the_head_hands_the_run_to_the_next_link() {
    // The other direction of the same claim, and it is the one with something at
    // stake: the FIRST entry is the provider in force, so demoting it changes
    // which vendor the next turn bills to. Asserted by reading back which entry
    // is first — through io-harness's own `provider_spec`, not by looking at
    // where the text moved to.
    let at = At::of(Scope::User, THREE, 0).expect("the head");
    let edit = providers::demote(&at).expect("a head with a chain behind it can be demoted");
    let after = io_cli::edit::apply(THREE, &[edit]).unwrap();

    let moved = config(&after);
    let chain = providers::chain(&moved.config);
    assert_eq!(chain.len(), 3, "a link was lost in the move");
    assert_eq!(
        chain[0].kind, "groq",
        "the provider in force did not change"
    );
    assert_eq!(chain[1].kind, "openrouter");
    assert_eq!(chain[2].kind, "ollama");

    let head = moved.config.provider_spec().expect("a head");
    assert!(
        matches!(head, io_harness::ProviderSpec::Compatible { preset, .. }
            if preset.as_deref() == Some("groq")),
        "io-harness does not agree about which provider a run would use",
    );
}

#[test]
fn f7_removing_the_head_promotes_the_next_link_and_removing_the_only_one_leaves_none() {
    // Removing the first entry is the removal that changes what a run uses, and
    // it is not spelled differently from any other — the chain IS the array's
    // order, so the second link becomes the provider by arithmetic.
    let at = At::of(Scope::User, THREE, 0).expect("the head");
    let after = io_cli::edit::apply(THREE, &[providers::remove(&at)]).unwrap();
    let chain = providers::chain(&config(&after).config);
    assert_eq!(chain[0].kind, "groq");
    assert_eq!(chain.len(), 2);
    assert!(
        !after.contains("openrouter"),
        "the removed entry left bytes behind",
    );

    // The last of several: the tail comes out and the head is untouched.
    let at = At::of(Scope::User, THREE, 2).expect("the tail");
    let after = io_cli::edit::apply(THREE, &[providers::remove(&at)]).unwrap();
    let chain = providers::chain(&config(&after).config);
    let kinds: Vec<&str> = chain.iter().map(|e| e.kind.as_str()).collect();
    assert_eq!(kinds, vec!["openrouter", "groq"]);

    // The only one: a configuration with no `[[provider]]` has NO head, rather
    // than a defaulted vendor the operator never named — io-harness says so and
    // this surface must not paper over it.
    const ONLY: &str = "[[provider]]\nkind = \"openai\"\nmodel = \"gpt-4o\"\n";
    let at = At::of(Scope::User, ONLY, 0).expect("the only entry");
    assert!(
        At::of(Scope::User, ONLY, 1).is_none(),
        "a second position was invented for a one-entry chain",
    );
    let after = io_cli::edit::apply(ONLY, &[providers::remove(&at)]).unwrap();
    assert_eq!(after, "", "the last entry left bytes behind");
    assert!(Config::from_toml(&after)
        .expect("a file with nothing in it is a configuration")
        .provider_spec()
        .is_none());
}

#[test]
fn f7_a_compatible_entry_naming_both_bases_or_neither_cannot_be_written() {
    // io-harness takes EXACTLY ONE of `preset` and `base_url` and refuses both
    // and neither, by index, at load (`config.rs:456`). `configure::write`'s
    // round trip would catch it and roll back, which is a good failure — but the
    // pair of `Option`s that made it expressible is gone: `Endpoint` has one
    // variant per shape, so three of the four combinations cannot be spelled.
    //
    // The refusals are written by hand here, because that is the only way left
    // to produce them — and without them this test would be asserting that a
    // constraint io-harness might have dropped is still being satisfied.
    let neither = "[[provider]]\nkind = \"compatible\"\nmodel = \"m\"\n";
    let both = "[[provider]]\nkind = \"compatible\"\nmodel = \"m\"\n\
                preset = \"groq\"\nbase_url = \"https://example.com/v1\"\n";
    for bad in [neither, both] {
        assert!(
            Config::from_toml(bad).is_err(),
            "io-harness accepted a `compatible` entry that names both bases or \
             neither, so `Endpoint`'s split is no longer load-bearing: {bad}",
        );
    }

    // And every shape `Endpoint` CAN build is one io-harness loads.
    for endpoint in [
        Endpoint::OpenRouter,
        Endpoint::Anthropic,
        Endpoint::OpenAi,
        Endpoint::Preset("groq"),
        Endpoint::BaseUrl("https://example.com/v1"),
    ] {
        let written =
            io_cli::edit::apply("", &[providers::add(endpoint, "a-model", None)]).unwrap();
        assert!(
            Config::from_toml(&written).is_ok(),
            "{endpoint:?} produced an entry io-harness refuses:\n{written}",
        );
    }
}

#[test]
fn f7_an_added_provider_can_name_its_credential_without_carrying_it() {
    // The gap that made `add` unusable for every hosted vendor: there was no way
    // to write `api_key` at all, so an operator adding one had to open the file
    // by hand — which is the thing this surface exists not to make them do.
    //
    // Written as the indirection rather than the key, which is the form the
    // panel then shows back: the variable's NAME is the information.
    let after = io_cli::edit::apply(
        "",
        &[providers::add(
            Endpoint::Preset("groq"),
            "llama-3.3-70b-versatile",
            Some("${env:IO_CLI_TEST_GROQ_KEY}"),
        )],
    )
    .unwrap();

    let chain = providers::chain(&config(&after).config);
    assert_eq!(
        chain[0].credential,
        Credential::Indirect("${env:IO_CLI_TEST_GROQ_KEY}".into()),
    );
    assert_eq!(chain[0].kind, "groq");
    assert_eq!(chain[0].model, "llama-3.3-70b-versatile");

    // And an entry with no key states none, rather than an empty one — which is
    // a different configuration: for `compatible` there is no environment
    // variable to fall back to, and `api_key = ""` is a key that is set.
    let bare = io_cli::edit::apply(
        "",
        &[providers::add(Endpoint::Preset("ollama"), "llama3.2", None)],
    )
    .unwrap();
    assert!(
        !bare.contains("api_key"),
        "an empty credential was written: {bare}"
    );
    assert_eq!(
        providers::chain(&config(&bare).config)[0].credential,
        Credential::NotNeeded,
    );
}

#[test]
fn f7_a_pasted_credential_is_escaped_rather_than_breaking_the_file() {
    // `api_key` and `base_url` are pasted text. A `format!("\"{}\"")` would turn
    // a backslash into an escape and a quote into the end of the value — the
    // second is a way to write keys the operator did not ask for.
    let after = io_cli::edit::apply(
        "",
        &[providers::add(
            Endpoint::OpenAi,
            "gpt-4o",
            Some("sk-a\\b\"c\nmodel = \"smuggled\""),
        )],
    )
    .unwrap();

    let config = Config::from_toml(&after).expect("the written file still parses");
    let Some(io_harness::ProviderSpec::OpenAi { model, api_key }) = config.provider_spec() else {
        panic!("the file named an openai provider");
    };
    assert_eq!(
        model, "gpt-4o",
        "a second `model` was smuggled into the entry"
    );
    assert_eq!(api_key.as_deref(), Some("sk-a\\b\"c\nmodel = \"smuggled\""));
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
    assert_eq!(
        mine, theirs,
        "io-cli's preset list has drifted from io-harness's"
    );
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
        assert!(
            !text.contains("supersecret"),
            "a credential reached a row: {text}"
        );
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

/// A file whose top-level chain and whose profile's chain are DIFFERENT lists.
///
/// The profile declares one entry; the top level declares three. io-harness does
/// not append a profile's `provider` onto the top-level array, it replaces it —
/// and then rewrites the origins so the overlaid config still names this file.
const PROFILE_REPLACES_THE_CHAIN: &str = r#"
[[provider]]
kind = "openrouter"
model = "the-operators-real-primary"

[[provider]]
kind = "anthropic"
model = "the-operators-first-fallback"

[[provider]]
kind = "openai"
model = "the-operators-second-fallback"

[[profile.fast.provider]]
kind = "openrouter"
model = "something-small-and-quick"
"#;

/// **A position is confirmed against the entry it is supposed to name.**
///
/// Under a profile the rows on screen come from the resolved chain while the
/// array a write would splice is the file's top-level one, and those are not the
/// same list. Every positional check still passes — `decided` names this file,
/// the index is in bounds — so nothing but the content can tell the two apart.
///
/// Without the check this is a silent wrong delete: the profile shows one link,
/// `remove` is aimed at `provider[0]`, and `provider[0]` is the operator's real
/// primary provider, which was never on screen.
#[test]
fn f7_a_profiles_link_is_never_addressed_by_a_position_in_the_top_level_array() {
    let fixture = config(PROFILE_REPLACES_THE_CHAIN);
    let overlaid = io_cli::configure::with_profile(&fixture.config, "fast")
        .expect("the fixture declares the profile");

    let chain = providers::chain(&overlaid);
    assert_eq!(
        chain.len(),
        1,
        "the profile replaces the chain rather than extending it; if this is 4 the \
         premise of this test is gone and the guard below is testing nothing",
    );
    assert_eq!(chain[0].model, "something-small-and-quick");

    assert!(
        providers::declared_at(&overlaid, &chain[0]).is_none(),
        "the profile's link is not the entry at that position in the top-level array, \
         so there is no position to hand a writer — answering with one would aim a \
         removal at `the-operators-real-primary`, which is not on screen",
    );

    // The control: with no profile in force the two lists ARE the same list, and
    // every link still answers with its own position. A guard that refused
    // everything would pass the assertion above and break the surface.
    let plain = providers::chain(&fixture.config);
    assert_eq!(plain.len(), 3);
    for entry in &plain {
        let at = providers::declared_at(&fixture.config, entry)
            .expect("a top-level link is addressable");
        assert_eq!(at.index(), entry.index);
    }
}
