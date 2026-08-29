//! F7, F8, F9 and F10 — the chain, the presets, the live verification, and
//! changing a link that already exists.

use std::sync::{Mutex, MutexGuard};

use io_cli::providers::{self, At, Credential, Endpoint, Key};
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

/// An entry whose credential is a literal, which is what a rotation edits.
const WRITTEN: &str = "\
[[provider]]
kind = \"openai\"
model = \"gpt-4o\"
api_key = \"sk-supersecret-literal\"
";

#[test]
fn f10_editing_a_model_changes_one_key_and_leaves_the_file_alone() {
    // The narrow claim, asserted at the only resolution that proves it: the
    // whole file, byte for byte, is the original with one value replaced. A
    // sibling entry rewritten, a comment lost, or a blank line normalised all
    // fail here — and each of those is what a surface that rebuilt the array
    // from io-cli's model of it would do, silently, on every edit.
    let at = At::of(Scope::User, THREE, 1).expect("the middle link");
    let edit = providers::edit(
        &at,
        "model",
        &io_cli::servers::quoted("llama-3.1-8b-instant"),
    )
    .expect("`model` is one of the keys this surface changes");
    let after = io_cli::edit::apply(THREE, &[edit]).unwrap();

    assert_eq!(
        after,
        THREE.replace("llama-3.3-70b-versatile", "llama-3.1-8b-instant"),
        "the edit reached past the one value it names",
    );

    // And io-harness reads back the link the operator meant, in its own place.
    let chain = providers::chain(&config(&after).config);
    assert_eq!(chain[1].model, "llama-3.1-8b-instant");
    assert_eq!(
        chain[1].kind, "groq",
        "the edit changed the entry's identity"
    );
    assert_eq!(chain[0].kind, "openrouter");
    assert_eq!(chain[2].model, "llama3.2");
    assert_eq!(
        chain[1].credential,
        Credential::Indirect("${env:IO_CLI_TEST_GROQ_KEY}".into()),
        "editing the model disturbed the credential beside it",
    );
}

#[test]
fn f10_a_credential_is_written_in_whichever_of_the_three_shapes_was_chosen() {
    // Shape one — from the environment. For a vendor kind this is the ABSENCE of
    // the key, and the absence is the whole value of the shape: there is nothing
    // in `io.toml` to leak. Asserted as absent rather than as empty, because
    // `api_key = ""` reads back as a key that is set.
    let written = Key::Environment.written(Endpoint::OpenRouter);
    assert!(
        written.is_none(),
        "a vendor kind was given text to write, so the secret-free shape writes \
         something into the file after all",
    );
    let after = io_cli::edit::apply(
        "",
        &[providers::add(
            Endpoint::OpenRouter,
            "m",
            written.as_deref(),
        )],
    )
    .unwrap();
    assert!(!after.contains("api_key"), "an empty credential: {after}");
    assert_eq!(
        providers::chain(&config(&after).config)[0].credential,
        Credential::FromEnvironment("OPENROUTER_API_KEY"),
    );

    // The same intention on a `compatible` entry is a DIFFERENT spelling, and
    // this is the split `Key::written` exists to resolve. There is no variable
    // for io-harness to fall back to here, so an absent key would be an
    // unauthenticated request rather than an authenticated one.
    assert_eq!(
        Key::Environment
            .written(Endpoint::Preset("groq"))
            .as_deref(),
        Some("${env:GROQ_API_KEY}"),
    );
    // Except where there is genuinely no credential to name.
    assert!(Key::Environment
        .written(Endpoint::Preset("ollama"))
        .is_none());
    assert!(Key::Environment
        .written(Endpoint::BaseUrl("http://localhost:8080/v1"))
        .is_none());

    // Shape two — an indirection, written as written and read back as written.
    let groq = Endpoint::Preset("groq");
    let indirect = Key::Indirect("${env:IO_CLI_TEST_GROQ_KEY}");
    let after = io_cli::edit::apply(
        "",
        &[providers::add(
            groq,
            "llama-3.3-70b-versatile",
            indirect.written(groq).as_deref(),
        )],
    )
    .unwrap();
    assert_eq!(
        providers::chain(&config(&after).config)[0].credential,
        Credential::Indirect("${env:IO_CLI_TEST_GROQ_KEY}".into()),
        "the variable's name is the information, and it did not survive",
    );

    // Shape three — a literal, which is the shape that puts a secret on disk.
    const SECRET: &str = "sk-proj-supersecret-literal";
    let literal = Key::Literal(SECRET);
    let after = io_cli::edit::apply(
        "",
        &[providers::add(
            Endpoint::OpenAi,
            "gpt-4o",
            literal.written(Endpoint::OpenAi).as_deref(),
        )],
    )
    .unwrap();
    assert!(
        after.contains(SECRET),
        "a literal the operator asked for was not the key that was written",
    );
    let chain = providers::chain(&config(&after).config);
    assert_eq!(chain[0].credential, Credential::Written);

    // And having been written, it still reaches no rendered surface.
    for row in providers::rows(&chain) {
        let text = format!("{} {}", row.label, row.detail.unwrap_or_default());
        assert!(!text.contains(SECRET), "a credential reached a row: {text}");
        assert!(!text.contains("supersecret"));
    }
    assert!(!chain[0].credential.word().contains("supersecret"));
}

#[test]
fn n2_a_key_never_prints_itself() {
    // `Key` is one `{:?}` away from a log line, a panic message, or somebody's
    // CI output on a failed `assert_eq!`. A derived `Debug` would put the
    // literal in all three, and it is the sort of leak nothing fails on until it
    // is already public.
    const SECRET: &str = "sk-proj-supersecret-literal";
    let shown = format!("{:?}", Key::Literal(SECRET));
    assert!(
        !shown.contains("supersecret") && !shown.contains(SECRET),
        "a credential printed itself: {shown}",
    );
    assert!(shown.contains("Literal"), "the shape is the useful part");

    // The variable's NAME is not a secret and stays legible — the same line
    // `configure::redact` draws.
    assert!(format!("{:?}", Key::Indirect("${env:GROQ_API_KEY}")).contains("GROQ_API_KEY"));
    assert_eq!(format!("{:?}", Key::Environment), "Environment");
}

#[test]
fn f10_unsetting_a_credential_deletes_the_line_rather_than_emptying_it() {
    let at = At::of(Scope::User, WRITTEN, 0).expect("the only entry");
    let after = io_cli::edit::apply(
        WRITTEN,
        &[providers::edit(&at, "api_key", "").expect("`api_key` can be unset")],
    )
    .unwrap();

    // Absent, not empty — asserted through the same reader the panel shows the
    // value with, so this is the question a surface would ask.
    assert!(
        io_cli::edit::value_at(&after, "provider[0].api_key").is_none(),
        "the key is still in the file: {after}",
    );
    assert!(!after.contains("api_key"), "{after}");
    assert!(
        !after.contains("supersecret"),
        "the literal survived: {after}"
    );

    // And the entry is still a link, which is the half a deletion could break:
    // `unset` takes one line and leaves the `[[provider]]` around it standing.
    let loaded = Config::from_toml(&after).expect("the entry still loads");
    let Some(io_harness::ProviderSpec::OpenAi { model, api_key }) = loaded.provider_spec() else {
        panic!("the entry stopped being an openai provider");
    };
    assert_eq!(model, "gpt-4o");
    assert_eq!(
        *api_key, None,
        "no key written is what io-harness reads as `OPENAI_API_KEY`",
    );

    // A cleared input field spells empty as `""`, and that is the same request.
    let same = io_cli::edit::apply(
        WRITTEN,
        &[providers::edit(&at, "api_key", "\"\"").expect("the same request, spelled in TOML")],
    )
    .unwrap();
    assert_eq!(same, after);

    // **The sabotage, run rather than described.** Writing the empty string is
    // what the obvious implementation does, and it does not fail loudly — it
    // produces a key that IS set, to nothing. `provider::key_for` hands that
    // back as a valid credential, the request carries an empty bearer token, and
    // the vendor answers 401 for a reason nothing in this program will name.
    let sabotage = io_cli::edit::apply(
        WRITTEN,
        &[io_cli::edit::Edit::set("provider[0].api_key", "\"\"")],
    )
    .unwrap();
    assert!(sabotage.contains("api_key = \"\""));
    let sabotaged =
        Config::from_toml(&sabotage).expect("an empty key still parses, which is the problem");
    let Some(io_harness::ProviderSpec::OpenAi { api_key, .. }) = sabotaged.provider_spec() else {
        panic!("an openai provider");
    };
    assert_eq!(
        api_key.as_deref(),
        Some(""),
        "an empty key is no longer read as a key that is set, so the unset above \
         is no longer the thing standing between an operator and a silent 401",
    );

    // `model` is required on every variant, so it has no unset: a verb that can
    // only ever produce an entry that fails to load is refused where it is
    // spelled rather than on the round trip.
    assert!(providers::edit(&at, "model", "").is_none());
}

#[test]
fn f10_an_edit_names_a_key_this_surface_knows() {
    let at = At::of(Scope::User, WRITTEN, 0).expect("the only entry");

    assert_eq!(
        providers::KEYS,
        ["model", "api_key"].as_slice(),
        "the editable keys changed; `KEYS`'s own documentation argues for each of \
         the five omissions, so a change here needs that argument changed with it",
    );
    assert!(providers::edit(&at, "model", "\"gpt-4o-mini\"").is_some());
    assert!(providers::edit(&at, "api_key", "\"${env:OPENAI_API_KEY}\"").is_some());

    for key in [
        // The link's identity. A different vendor is a different link, and
        // `preset` written onto a `base_url` entry is the both-bases entry
        // io-harness refuses by index at load.
        "kind",
        "preset",
        "base_url",
        // Nothing on this surface asks for them, and `reference_prices` turns on
        // an outbound request to a host the file did not name.
        "auth",
        "name",
        "reference_prices",
        // A typo, and the empty string, which is what an unguarded prompt returns.
        "modle",
        "api-key",
        "",
    ] {
        assert!(
            providers::edit(&at, key, "\"x\"").is_none(),
            "`{key}` was accepted as a key of a `[[provider]]` entry",
        );
    }
}

#[test]
fn f10_an_edit_is_aimed_at_the_array_the_row_came_from() {
    // The 0.21.0 hazard, on the new verb. Under a profile the rows on screen and
    // the array a write would splice are different lists, and every positional
    // check still passes — so there must be no `At` to hand `edit` at all.
    let fixture = config(PROFILE_REPLACES_THE_CHAIN);
    let overlaid = io_cli::configure::with_profile(&fixture.config, "fast")
        .expect("the fixture declares the profile");
    let shown = providers::chain(&overlaid);
    assert_eq!(shown.len(), 1, "the profile no longer replaces the chain");
    assert!(
        providers::declared_at(&overlaid, &shown[0]).is_none(),
        "a profile's row answered with a position in the top-level array, so an \
         edit aimed at it would rewrite `the-operators-real-primary`",
    );

    // The control, and the half that says the guard is not simply refusing
    // everything: with no profile in force the row and the array agree, and the
    // write lands on the entry the row named — the second one.
    let plain = providers::chain(&fixture.config);
    let at = providers::declared_at(&fixture.config, &plain[1])
        .expect("a top-level link is addressable");
    let after = io_cli::edit::apply(
        PROFILE_REPLACES_THE_CHAIN,
        &[providers::edit(&at, "model", "\"a-different-fallback\"").expect("`model`")],
    )
    .unwrap();

    assert!(
        after.contains("the-operators-real-primary"),
        "the head was rewritten by an edit aimed at the first fallback",
    );
    assert!(
        !after.contains("the-operators-first-fallback"),
        "the addressed entry was not the one that changed",
    );
    assert!(
        after.contains("the-operators-second-fallback")
            && after.contains("something-small-and-quick"),
        "an entry nobody named was rewritten",
    );
    assert_eq!(
        providers::chain(&config(&after).config)[1].model,
        "a-different-fallback",
    );
}

#[test]
fn f10_the_default_offered_is_the_variable_the_endpoint_already_reads() {
    // The sabotage F10 names is defaulting to the literal, under which a secret
    // lands in a file the operator did not ask to hold one. What makes the other
    // default possible is being able to name the variable and say whether it is
    // set — so both are asserted here, and neither of them ever produces the
    // value.
    assert_eq!(
        providers::variable(Endpoint::OpenRouter).as_deref(),
        Some("OPENROUTER_API_KEY"),
    );
    assert_eq!(
        providers::variable(Endpoint::Anthropic).as_deref(),
        Some("ANTHROPIC_API_KEY"),
    );
    assert_eq!(
        providers::variable(Endpoint::OpenAi).as_deref(),
        Some("OPENAI_API_KEY"),
    );
    // The name io-harness's own documentation writes for this preset.
    assert_eq!(
        providers::variable(Endpoint::Preset("groq")).as_deref(),
        Some("GROQ_API_KEY"),
    );
    assert!(
        providers::variable(Endpoint::BaseUrl("http://localhost:8080/v1")).is_none(),
        "a bare base URL was offered a vendor's variable, which names no vendor",
    );

    // A credential is offered for exactly the endpoints that want one, which is
    // the same split `is_local` already draws — thirteen hosted, eight local.
    for preset in providers::PRESETS {
        assert_eq!(
            providers::variable(Endpoint::Preset(preset)).is_some(),
            !providers::is_local(preset),
            "{preset} was offered the wrong kind of credential row",
        );
    }

    // Whether, never what. Held with the lock the fixtures use, and not while
    // one is being built.
    let _guard = env_lock();
    let var = "IO_CLI_TEST_DEFAULT_KEY";
    std::env::remove_var(var);
    assert!(!providers::variable_is_set(var));
    std::env::set_var(var, "   ");
    assert!(
        !providers::variable_is_set(var),
        "an empty variable was reported as set, so the default row would point at \
         a credential that authenticates nothing",
    );
    std::env::set_var(var, "gsk-not-a-real-key");
    assert!(providers::variable_is_set(var));
    std::env::remove_var(var);
}

/// F10's ordering, asserted against the driver as text.
///
/// **The one gate that has to read `src/main.rs`, and it reads it because nothing
/// under `tests/` can link it.** The library never writes — it builds an `Edit` —
/// so "the verification call is made before any edit" is a property of the driver
/// alone, and a weak instrument aimed at it is the only instrument there is. It is
/// the shape `tests/context_share.rs` and `tests/contract.rs` already use.
///
/// Sabotage: move the `configure::write` above the `verify::credential` in the
/// `Pick::ProviderModel` arm — under which this fails, and the product it
/// describes leaves a rejected credential in the operator's file.
#[test]
fn f10_the_driver_verifies_before_it_writes() {
    // Line endings normalised: a Windows checkout has `\r\n`, and a gate that
    // sliced on `"\n"` matched nothing and panicked on a green product in 0.19.0
    // and again in 0.23.0.
    let driver = std::fs::read_to_string("src/main.rs")
        .expect("the driver is beside the tests")
        .replace("\r\n", "\n");
    let arm = driver
        .find("Pick::ProviderModel { preset, models, at } =>")
        .expect("the model arm is where the add is completed");
    let rest = &driver[arm..];
    // The arm ends where the next one begins.
    let end = rest
        .find("Pick::ProviderVerb {")
        .expect("ProviderVerb follows ProviderModel");
    let arm = &rest[..end];

    let verified = arm
        .find("verify::credential")
        .expect("the add must verify the credential at all");
    let written = arm
        .find("configure::write")
        .expect("the add must write something");
    assert!(
        verified < written,
        "the driver writes before it verifies, so a rejected credential would be left in the \
         operator's file"
    );
    // And the rejection path must not fall through into the write.
    assert!(
        arm.contains("Nothing was written."),
        "a rejected credential must say the file is unchanged"
    );
}
