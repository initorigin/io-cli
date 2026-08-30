//! F1 to F4 — capability bundles: what a declared directory puts into a turn,
//! what a broken one costs, what a project-scoped one may not contribute, and the
//! two writes `/plugin` makes.
//!
//! **Every claim about what a bundle contributed goes through io-harness's own
//! call**, never through io-cli's account of it. `Config::plugins()` re-reads each
//! declared directory from disk on every call, `Plugins::apply_to` is what merges
//! a bundle's agents and servers into a contract, and the namespacing that turns
//! `reviewer` into `rust-review__reviewer` happens inside `load_one` before io-cli
//! has seen a byte of it. A test that asserted on `pluginview::View` alone would
//! be asserting that io-cli copied fields correctly, which is not the question:
//! the question is whether the turn the operator gets carries the bundle.
//!
//! **Nothing here touches the environment.** `Config::discover` reads a user-scope
//! file where one exists, so every fixture that must be exact about the *absence*
//! of something uses `Config::from_toml`, which has no scope but the text; the
//! rest declare their bundles in a temporary root of their own and run in
//! parallel.

use std::path::{Path, PathBuf};

use io_cli::pluginview;
use io_harness::config::{Config, LOCAL_FILE, PROJECT_FILE};
use io_harness::PLUGIN_FILE;

/// The manifest io-harness's own module docs open with: a bundle contributing
/// something of five of the six kinds, and nothing that runs a program.
///
/// Written out here rather than assembled from parts, because it is the shape a
/// bundle author copies out of the documentation and it has to keep loading.
const MINIMAL: &str = r#"
name = "rust-review"
description = "Everything our Rust reviews need."
skills = "skills"
templates = "templates"

[[agent]]
name = "reviewer"
model = "cheap-model"
deny_write = true

[policy]
layers = [
    { name = "no-secrets", rules = [{ act = "write", effect = "deny", pattern = "secrets/**" }] },
]
"#;

/// A temporary root, kept alive by the returned guard.
fn root() -> (tempfile::TempDir, PathBuf) {
    let dir = tempfile::tempdir().expect("a temporary directory");
    let root = dir.path().to_path_buf();
    (dir, root)
}

/// A bundle directory holding `manifest`, with the two directories a manifest can
/// name already on disk.
fn bundle(root: &Path, at: &str, manifest: &str) -> PathBuf {
    let dir = root.join(at);
    std::fs::create_dir_all(dir.join("skills")).expect("the skills directory");
    std::fs::create_dir_all(dir.join("templates")).expect("the templates directory");
    // `PLUGIN_FILE` rather than a literal: the name a directory is recognised by
    // is io-harness's to state, and a fixture that spelled it itself would keep
    // passing through a release that renamed it.
    std::fs::write(dir.join(PLUGIN_FILE), manifest).expect("the manifest");
    dir
}

/// A directory that is declared as a bundle and holds no manifest at all.
fn empty_bundle(root: &Path, at: &str) -> PathBuf {
    let dir = root.join(at);
    std::fs::create_dir_all(&dir).expect("the bundle directory");
    dir
}

/// Write `file` in `root` declaring each named directory as a bundle, in order.
fn declaring(root: &Path, file: &str, paths: &[&str]) {
    let text: String = paths
        .iter()
        .map(|path| format!("[[plugin]]\npath = \"{path}\"\n\n"))
        .collect();
    std::fs::write(root.join(file), text).expect("the configuration");
}

/// The dropped row for the bundle at `at`, by the directory the entry named.
///
/// Keyed on the path rather than on `Dropped::id`, because a bundle whose manifest
/// could not be read has no id — the field falls back to the directory's own name
/// — and keying on it would make a test about unparseable TOML depend on the
/// fallback rather than on the refusal.
fn dropped_at<'a>(plugins: &'a io_harness::Plugins, at: &str) -> &'a io_harness::plugin::Dropped {
    plugins
        .dropped()
        .iter()
        .find(|dropped| dropped.path.ends_with(at))
        .unwrap_or_else(|| {
            panic!(
                "no bundle at `{at}` was dropped; dropped: {:?}",
                plugins
                    .dropped()
                    .iter()
                    .map(|d| d.path.display().to_string())
                    .collect::<Vec<_>>()
            )
        })
}

// ---------------------------------------------------------------------------
// F1 — a declared bundle reaches the turn
// ---------------------------------------------------------------------------

/// **F1.** A declared bundle loads, and what it contributed is on the contract
/// under the namespaced name io-harness gave it.
///
/// Sabotage: drop the `plugins.apply_to(contract)` line from
/// `contract::configured`. Under it only this test fails, and it fails silently in
/// the field: `/plugin` still lists the bundle, `Config::plugins()` still loads it,
/// the trace still reports it — and the agent it defines cannot be spawned, its
/// skills never reach the run, and the operator has no way to tell a bundle that
/// did nothing from one that was never read.
#[test]
fn f1_a_declared_bundle_reaches_the_turn_with_every_name_namespaced() {
    let (_dir, root) = root();
    bundle(&root, "bundles/rust-review", MINIMAL);
    declaring(&root, PROJECT_FILE, &["bundles/rust-review"]);

    let config = Config::discover(&root).expect("the configuration loads");

    // io-harness's own verdict first: this bundle loaded and nothing was dropped.
    let plugins = config.plugins();
    assert_eq!(plugins.names(), vec!["rust-review"]);
    assert_eq!(plugins.len(), 1);
    assert!(!plugins.is_empty());
    assert!(
        plugins.dropped().is_empty(),
        "a manifest out of io-harness's own documentation was refused: {:?}",
        plugins.dropped(),
    );

    let plugin = plugins.get("rust-review").expect("loaded by id");
    assert_eq!(plugin.id(), "rust-review");
    assert_eq!(plugin.root(), root.join("bundles/rust-review"));
    assert_eq!(
        plugin.description(),
        Some("Everything our Rust reviews need.")
    );
    assert_eq!(plugin.version(), None, "the manifest declared none");
    assert_eq!(
        plugin.skills_dir(),
        Some(root.join("bundles/rust-review/skills")),
    );
    assert_eq!(
        plugin.templates_dir(),
        Some(root.join("bundles/rust-review/templates")),
    );
    assert_eq!(
        plugin.contributions(),
        vec!["skills", "templates", "agents", "policy"],
        "the kinds io-harness reports, in its own fixed order",
    );
    assert_eq!(plugin.agents()[0].name, "rust-review__reviewer");
    assert_eq!(plugin.policy_layers()[0].name, "rust-review__no-secrets");
    assert!(plugin.mcp_servers().is_empty());

    // And the contract a turn actually carries.
    let contract =
        io_cli::contract::configured("review this", root.clone(), &config, &config.plugins());
    assert!(
        contract.agents.get("rust-review__reviewer").is_some(),
        "the bundle's agent is not on the contract, so nothing can spawn it: {:?}",
        contract.agents.names(),
    );
    assert!(
        contract.agents.get("reviewer").is_none(),
        "the bare name is on the roster, so the trace and the roster spell the same \
         agent two different ways: {:?}",
        contract.agents.names(),
    );
    assert_eq!(
        contract.plugins.len(),
        1,
        "`contract.plugins` is what `discover_skills` reads at run start to fold a \
         bundle's skills in, and it is empty",
    );

    // io-cli's own surface agrees with the harness it read.
    let view = pluginview::view(&config.plugins());
    assert!(!view.is_empty());
    assert_eq!(view.plugins.len(), 1);
    assert!(view.refused.is_empty());
    assert_eq!(view.plugins[0].agents, vec!["rust-review__reviewer"]);
}

// ---------------------------------------------------------------------------
// F2 — a broken bundle is one row, never an error
// ---------------------------------------------------------------------------

/// **F2.** Each of the five ways a bundle can be refused costs exactly that
/// bundle, and `Config::plugins()` returns rather than failing.
///
/// Sabotage: the `if let Some(bad) = plugins.dropped().first() { return Err(...) }`
/// io-harness's own module docs offer, written into `contract::configured` — or,
/// equivalently, a `/plugin` that returned a `Result`. Under it only this test
/// fails, and it fails the way a shared `io.toml` fails a team: one operator
/// commits a bundle with a typo in its `[policy]` block and every session in the
/// repository stops starting, including the four bundles that were fine.
#[test]
fn f2_five_ways_a_bundle_breaks_each_cost_exactly_that_bundle() {
    let (_dir, root) = root();

    bundle(&root, "good", MINIMAL);
    // 1. A directory declared as a bundle with no manifest in it.
    empty_bundle(&root, "no-manifest");
    // 2. A manifest that is not TOML.
    bundle(&root, "unparseable", "this is not a toml document at all\n");
    // 3. An id outside `[a-z0-9][a-z0-9-]{0,31}`.
    bundle(&root, "bad-id", "name = \"Rust_Review\"\n");
    // 4. A `[policy]` carrying a `defaults` table — accepted by the parser and
    //    refused by name, because a default decides every action no rule mentions.
    bundle(
        &root,
        "policy-defaults",
        "name = \"defaulting\"\n\n[policy.defaults]\nwrite = \"allow\"\n",
    );
    // 5. A `[policy]` rule whose effect is not `deny`. A bundle may take capability
    //    away and may never hand it out.
    bundle(
        &root,
        "policy-allow",
        "name = \"widening\"\n\n[policy]\nlayers = [{ name = \"open\", rules = [\
         { act = \"write\", effect = \"allow\", pattern = \"**\" }] }]\n",
    );

    declaring(
        &root,
        LOCAL_FILE,
        &[
            "good",
            "no-manifest",
            "unparseable",
            "bad-id",
            "policy-defaults",
            "policy-allow",
        ],
    );

    // The call returns. That is the first half of the criterion and it is asserted
    // by there being a line after it.
    let config = Config::discover(&root).expect("a broken bundle is not a broken configuration");
    let plugins = config.plugins();

    assert_eq!(
        plugins.names(),
        vec!["rust-review"],
        "the one sound bundle did not load beside five broken ones",
    );
    assert_eq!(plugins.dropped().len(), 5);

    // Each one, by the reason io-harness gave for it.
    for (at, phrase) in [
        (
            "no-manifest",
            "a plugin is a directory with a manifest at its root",
        ),
        ("unparseable", PLUGIN_FILE),
        ("bad-id", "is not a usable plugin id"),
        ("policy-defaults", "key `policy.defaults`"),
        ("policy-allow", "may only contribute `deny`"),
    ] {
        let dropped = dropped_at(&plugins, at);
        assert!(
            dropped.error.contains(phrase),
            "the bundle at `{at}` was dropped for something other than the reason \
             under test: {}",
            dropped.error,
        );
        assert!(
            plugins.get(&dropped.id).is_none(),
            "`{at}` is both dropped and loaded",
        );
    }

    // And io-cli's surface holds both facts at once rather than one instead of the
    // other — which is what `View::is_empty` reading both lists exists for.
    let view = pluginview::view(&config.plugins());
    assert_eq!(view.plugins.len(), 1);
    assert_eq!(view.refused.len(), 5);
    assert!(!view.is_empty());
}

// ---------------------------------------------------------------------------
// F3 — the trust rule, and that it costs the whole bundle
// ---------------------------------------------------------------------------

/// A bundle that contributes an agent, skills, and one thing that runs a program.
fn executing(kind: &str) -> String {
    let block = match kind {
        "hook" => "[[hook]]\non = [\"finished\"]\nrun = [\"touch\", \"ran\"]\n",
        "mcp" => "[[mcp]]\nid = \"docs\"\ntransport = \"stdio\"\ncommand = \"mcp-docs\"\n",
        other => panic!("no such contribution: {other}"),
    };
    format!("name = \"runner\"\nskills = \"skills\"\n\n[[agent]]\nname = \"reviewer\"\n\n{block}")
}

/// **F3.** A bundle declared in the committed `io.toml` that contributes a
/// `[[hook]]` or an `[[mcp]]` is dropped **whole**.
///
/// The assertion is deliberately not "the hook was skipped". A half-applied
/// stranger's manifest is the failure the rule exists to prevent: the bundle's
/// agent, its skills directory and its policy come from the same file as the argv,
/// and a loader that kept them would be trusting the author it just refused.
///
/// Sabotage: change `refuse_executing_contributions` from an error into a filter
/// that clears `manifest.hook` and `manifest.mcp` and lets the rest load — which is
/// the reading a reviewer would call the friendlier fix. Under it only this test
/// fails, and what ships is a `git clone` that installs an agent definition and a
/// skills directory of the cloned repository's choosing into every session run in
/// it.
#[test]
fn f3_a_bundle_that_names_a_program_is_dropped_whole_from_the_committed_file() {
    for kind in ["hook", "mcp"] {
        let (_dir, root) = root();
        bundle(&root, "runner", &executing(kind));
        declaring(&root, PROJECT_FILE, &["runner"]);

        let config = Config::discover(&root).expect("a refused bundle is not a refused file");
        let plugins = config.plugins();

        assert!(
            plugins.is_empty(),
            "{kind}: the bundle loaded from the file a `git clone` delivers",
        );
        let dropped = dropped_at(&plugins, "runner");
        assert!(
            dropped
                .error
                .contains(&format!("may not contribute `[[{kind}]]`")),
            "{kind}: dropped for something else: {}",
            dropped.error,
        );

        // **Nothing at all, rather than everything but the offending array.**
        let contract = io_cli::contract::configured("go", root.clone(), &config, &config.plugins());
        assert!(
            contract.agents.get("runner__reviewer").is_none(),
            "{kind}: the refused bundle's agent reached the turn anyway: {:?}",
            contract.agents.names(),
        );
        assert!(
            contract.plugins.is_empty(),
            "{kind}: the refused bundle is on the contract, so its skills directory \
             is folded in at run start",
        );
        assert!(
            !contract
                .mcp
                .iter()
                .any(|server| server.id.contains("runner")),
            "{kind}: the refused bundle's server reached the turn",
        );
        assert!(
            io_cli::contract::hooks(&config, &config.plugins(), &root).is_none(),
            "{kind}: the refused bundle's hooks are installed",
        );
    }
}

/// **F3, the other half.** The same manifest declared in `io.local.toml` loads,
/// with all six kinds.
///
/// Without this the rule above is indistinguishable from "a bundle may never
/// contribute a hook or a server", which would make the feature useless rather
/// than safe — and a fix that refused everywhere would pass every assertion in the
/// test above.
///
/// Sabotage: apply `refuse_executing_contributions` in every scope rather than
/// under `if scope == Scope::Project`. Under it only this test fails, and the
/// product ships a bundle format whose two most useful contributions can never be
/// declared by anyone.
#[test]
fn f3_the_same_manifest_declared_locally_contributes_all_of_it() {
    for kind in ["hook", "mcp"] {
        let (_dir, root) = root();
        bundle(&root, "runner", &executing(kind));
        // The local file, which does not travel with a clone — and no `io.toml` at
        // all, so nothing but the scope differs from the test above.
        declaring(&root, LOCAL_FILE, &["runner"]);

        let config = Config::discover(&root).expect("the configuration loads");
        let plugins = config.plugins();

        assert!(
            plugins.dropped().is_empty(),
            "{kind}: dropped from the local scope: {:?}",
            plugins.dropped(),
        );
        assert_eq!(plugins.names(), vec!["runner"]);
        let plugin = plugins.get("runner").expect("loaded");
        assert!(
            plugin.contributions().contains(&kind_name(kind)),
            "{kind}: loaded without the contribution under test: {:?}",
            plugin.contributions(),
        );

        let contract = io_cli::contract::configured("go", root.clone(), &config, &config.plugins());
        assert!(
            contract.agents.get("runner__reviewer").is_some(),
            "{kind}: the bundle loaded and its agent did not reach the turn: {:?}",
            contract.agents.names(),
        );
        assert_eq!(contract.plugins.len(), 1);
        match kind {
            "mcp" => assert!(
                contract.mcp.iter().any(|s| s.id == "runner__docs"),
                "the namespaced server is not on the contract: {:?}",
                contract.mcp.iter().map(|s| &s.id).collect::<Vec<_>>(),
            ),
            _ => assert!(
                io_cli::contract::hooks(&config, &config.plugins(), &root).is_some(),
                "the bundle's hook is declared and nothing will run it",
            ),
        }
    }
}

/// What `Plugin::contributions` calls each kind, which is not what the TOML array
/// is called.
fn kind_name(kind: &str) -> &'static str {
    match kind {
        "hook" => "hooks",
        "mcp" => "mcp",
        other => panic!("no such contribution: {other}"),
    }
}

// ---------------------------------------------------------------------------
// F4 — the two writes
// ---------------------------------------------------------------------------

/// A file with a comment, a bundle already declared, and two sections io-cli does
/// not model.
const OPERATORS: &str = "\
# the bundles this repository uses
[[plugin]]
path = \"fixtures/nowhere/alpha\"

[instructions]
files = [\"AGENTS.md\"]

[run]
max_steps = 30
";

/// **F4.** `add` appends a whole `[[plugin]]` entry, and everything else in the
/// file survives it byte for byte.
///
/// This crate splices bytes rather than re-serialising, precisely so a key it has
/// no model for survives a write to its neighbour — and that property has to be
/// asserted rather than assumed, because the alternative implementation is the
/// obvious one.
///
/// Sabotage: implement `add` by deserializing the file into io-cli's own types,
/// pushing an entry and writing the result back. Under it only this test fails,
/// and it fails by deleting the operator's comment, their `[instructions]` table
/// and every key of a section a later io-harness added — silently, on a keystroke
/// they pressed to add one line.
#[test]
fn f4_add_appends_an_entry_and_the_rest_of_the_file_survives_untouched() {
    let after = io_cli::edit::apply(
        OPERATORS,
        &[pluginview::add(Path::new("fixtures/nowhere/beta"))],
    )
    .expect("the edit applies");

    // Everything that was there is still there, line for line — the comment
    // included.
    for line in OPERATORS.lines().filter(|line| !line.trim().is_empty()) {
        assert!(after.contains(line), "line lost: {line:?}");
    }

    // The result is still the harness's schema, and the harness's own reader is
    // what says which bundles it now declares and in what order.
    let config = Config::from_toml(&after).expect("the written file loads");
    let declared: Vec<String> = config
        .plugins()
        .dropped()
        .iter()
        .map(|dropped| dropped.path.display().to_string().replace('\\', "/"))
        .collect();
    assert_eq!(
        declared,
        vec!["./fixtures/nowhere/alpha", "./fixtures/nowhere/beta"],
        "the new entry is not the last declaration, so it does not stack last",
    );
}

/// **F4.** `remove` takes the indexed entry whole and leaves its siblings and the
/// sections around it.
///
/// By index rather than by id, and the fixture says why: the second entry names a
/// directory with no manifest, so it has no id to be removed by — and it is
/// exactly the entry an operator opens `/plugin` to be rid of.
///
/// Sabotage: `Edit::remove("plugin")` without the index, or an index off by one.
/// Under it only this test fails, and it fails by removing a bundle the operator
/// did not pick — or by taking the `[run]` table that followed the last entry with
/// it, which is a step cap silently back to io-harness's twelve.
#[test]
fn f4_remove_takes_the_indexed_entry_and_leaves_its_siblings() {
    const TWO: &str = "\
[[plugin]]
path = \"fixtures/nowhere/alpha\"

[[plugin]]
path = \"fixtures/nowhere/beta\"

[run]
max_steps = 30
";
    let after = io_cli::edit::apply(TWO, &[pluginview::remove(0)]).expect("the edit applies");

    let config = Config::from_toml(&after).expect("the written file loads");
    let declared: Vec<String> = config
        .plugins()
        .dropped()
        .iter()
        .map(|dropped| dropped.path.display().to_string().replace('\\', "/"))
        .collect();
    assert_eq!(
        declared,
        vec!["./fixtures/nowhere/beta"],
        "the removal did not reach exactly the entry it named",
    );
    assert!(
        after.contains("max_steps = 30"),
        "the section after the last entry was taken with it",
    );
}

/// **F11 (0.30.0).** A bundle is switched off and back on, and the entry survives.
///
/// Not the F11 the `f11_candidates_*` tests further down this file cover — that is
/// 0.28.0's criterion about resolving the word an operator typed. The contracts
/// number independently and this file now holds tests from both; the doc comment is
/// what says which, since the name cannot.
///
/// `pluginview::disable` writes `false` where `enable` writes `true`, into the same
/// one key of the same one entry. Everything else about the declaration — the path
/// it names, and therefore the bundle's identity — is the same bytes afterwards,
/// which is what separates this verb from `remove`: the bundle goes on being listed
/// under `DISABLED_MARK` instead of vanishing, and the way back on is the same
/// keystroke again rather than typing the directory out.
///
/// The whole round trip goes through io-harness: the file is written, re-discovered
/// and read back through `Plugins::disabled()`, because `[[plugin]]` *is* held to
/// `deny_unknown_fields` and a fixture that only grepped the text would pass on a
/// key the harness never honoured.
///
/// Sabotage: have `disable` call `remove`. The bundle stops being declared at all —
/// `view.plugins` no longer names it, the `DISABLED_MARK` row is not there, and the
/// path assertion on the file fails first.
#[test]
fn f11_a_bundle_is_switched_off_and_back_on_and_the_entry_survives() {
    let (_dir, root) = root();
    bundle(&root, "good", MINIMAL);
    declaring(&root, LOCAL_FILE, &["good"]);
    let file = root.join(LOCAL_FILE);
    let before = std::fs::read_to_string(&file).expect("the configuration");

    let off = io_cli::edit::apply(&before, &[pluginview::disable(0)]).expect("the edit applies");
    assert!(
        off.contains("path = \"good\""),
        "the declaration was taken away rather than switched off: {off}",
    );
    assert!(
        off.contains("enabled = false"),
        "the key is not a TOML boolean, and io-harness refuses `enabled = \"false\"`: {off}",
    );
    std::fs::write(&file, &off).expect("the configuration");

    let config = Config::discover(&root).expect("the configuration loads");
    let view = pluginview::view(&config.plugins());
    let listed = view
        .plugins
        .iter()
        .find(|plugin| plugin.id == "rust-review")
        .expect("a switched-off bundle is still declared and still listed");
    assert!(
        !listed.enabled,
        "the bundle is still loading, so nothing was switched off",
    );

    // The mark, because a row that vanished and a row that says so are the two
    // answers this verb chooses between.
    let rows = pluginview::rows(&view, 120, &io_cli::glyphs::ASCII);
    let row = rows
        .iter()
        .find(|row| row.label == "rust-review")
        .expect("a row per declared bundle");
    assert_eq!(
        row.mark,
        Some(pluginview::DISABLED_MARK),
        "a switched-off bundle must be drawn under its own mark rather than \
         disappearing or reading as loaded",
    );

    // And back on, from exactly where it was.
    let on = io_cli::edit::apply(&off, &[pluginview::enable(0)]).expect("the edit applies");
    assert!(on.contains("enabled = true"), "{on}");
    assert!(on.contains("path = \"good\""), "{on}");
    std::fs::write(&file, &on).expect("the configuration");
    let config = Config::discover(&root).expect("the configuration loads");
    assert!(
        pluginview::view(&config.plugins())
            .plugins
            .iter()
            .any(|plugin| plugin.id == "rust-review" && plugin.enabled),
        "a bundle switched off cannot be switched back on, which is the half \
         0.29.0 could not ship",
    );
}

/// **F12, the `[[plugin]]` half.** The sentence this write costs an older binary
/// is not the sentence the `[[mcp]]` write costs.
///
/// Stated on both surfaces because an operator who has met one will assume the
/// other behaves the same way, and here they are opposites: a 0.69.0 binary refuses
/// the *whole configuration file* over `enabled` in a `[[plugin]]`, and silently
/// ignores it in an `[[mcp]]` — starting a server that was switched off. The full
/// pairing is asserted in `tests/servers_enabled.rs`; this is the half that lives
/// beside the write it describes.
///
/// Sabotage: use `pluginview::OLDER_BINARY` for both. This test and its twin fail
/// together, and the one that matters is the MCP side, where the operator is told
/// they will notice something they will not.
#[test]
fn f12_the_plugin_older_binary_sentence_is_about_the_whole_file() {
    let plugin = pluginview::OLDER_BINARY;
    assert!(
        plugin.contains("refuses the whole configuration file"),
        "the cost of this key on an older binary is total, not partial: {plugin}",
    );
    assert_ne!(
        plugin,
        io_cli::servers::OLDER_BINARY,
        "the two `enabled` writes cost opposite things and cannot share a sentence",
    );
}

// ---------------------------------------------------------------------------
// F2/F3 rendering — the sentence, and the index mapping
// ---------------------------------------------------------------------------

/// A root with one bundle that loads and one that does not.
fn one_of_each() -> (tempfile::TempDir, PathBuf, Config) {
    let (dir, root) = root();
    bundle(&root, "good", MINIMAL);
    empty_bundle(&root, "broken");
    declaring(&root, LOCAL_FILE, &["good", "broken"]);
    let config = Config::discover(&root).expect("the configuration loads");
    (dir, root, config)
}

/// **F2/F3 rendering.** A refused bundle's row carries io-harness's sentence
/// unmodified, and the row index maps straight back into the two lists.
///
/// The sentence is the operator's only instruction. It names the directory, names
/// the key, and for the project-scope refusal explains in two clauses why a cloned
/// `io.toml` may not name a program this machine will run. io-cli could not write
/// that better and must not try.
///
/// Sabotage, either half: reword the row to "this plugin could not be loaded", or
/// insert a heading row above the refused group. Under the first, an operator is
/// told a bundle is broken and never told what to change. Under the second, every
/// index past the heading is off by one and `/plugin` acts on the wrong bundle —
/// which is the defect `rows` is heading-free to avoid, and which the unit tests
/// in `src/pluginview.rs` cannot see because they build their `View` by hand.
#[test]
fn f2_a_refused_row_carries_the_harness_sentence_and_the_index_still_maps() {
    let (_dir, _root, config) = one_of_each();
    let view = pluginview::view(&config.plugins());
    assert_eq!(view.plugins.len(), 1);
    assert_eq!(view.refused.len(), 1);

    for glyphs in [&io_cli::glyphs::UNICODE, &io_cli::glyphs::ASCII] {
        // Wide enough that nothing has to be shortened, so "verbatim" is a
        // comparison rather than a prefix check.
        let rows = pluginview::rows(&view, 400, glyphs);
        assert_eq!(
            rows.len(),
            view.plugins.len() + view.refused.len(),
            "{}: `rows` is one row per bundle and no headings",
            glyphs.name,
        );

        // The positional contract, asserted as the contract states it.
        assert_eq!(rows[0].label, view.plugins[0].id, "{}", glyphs.name);
        assert_eq!(rows[0].mark, Some(pluginview::LOADED_MARK));
        assert_eq!(rows[1].label, view.refused[0].id, "{}", glyphs.name);
        assert_eq!(rows[1].mark, Some(pluginview::REFUSED_MARK));
        assert!(
            rows.iter().all(|row| !row.heading),
            "{}: a heading is an index that maps to no bundle",
            glyphs.name,
        );

        let detail = rows[1].detail.clone().expect("a refused row has a detail");
        assert_eq!(
            detail,
            format!("not loaded{}{}", glyphs.separator, view.refused[0].error),
            "{}: the row is not io-harness's sentence, whole",
            glyphs.name,
        );
        assert!(
            view.refused[0]
                .error
                .contains("a plugin is a directory with a manifest at its root"),
            "the sentence io-cli carried is not the one io-harness wrote: {}",
            view.refused[0].error,
        );
    }
}

/// **N4.** Every row fits eighty columns in both glyph sets, and the ASCII set
/// draws nothing an ASCII terminal cannot.
///
/// Asserted over a `View` built from a real configuration rather than by hand, so
/// what is being fitted is a real tempdir path and io-harness's real sentence —
/// both far longer than anything a hand-built fixture would think to use, and both
/// what an operator will actually have on the row.
///
/// Sabotage: budget the row against `width` instead of `width - 4`, which is the
/// picker's marker and gap. Under it only this test fails, and it fails on the
/// operator's terminal rather than in the suite: every `/plugin` row wraps, the
/// list doubles in height, and the panel scrolls where it used to fit.
#[test]
fn n4_every_row_fits_eighty_columns_in_both_glyph_sets() {
    let (_dir, _root, config) = one_of_each();
    let view = pluginview::view(&config.plugins());

    for glyphs in [&io_cli::glyphs::UNICODE, &io_cli::glyphs::ASCII] {
        for row in pluginview::rows(&view, 80, glyphs) {
            let detail = row.detail.clone().expect("every bundle row has a detail");
            assert!(
                row.label.chars().count() + detail.chars().count() + 4 <= 80,
                "{}: the row does not fit: {} / {detail}",
                glyphs.name,
                row.label,
            );
            if glyphs.name == io_cli::glyphs::ASCII.name {
                for character in detail.chars().chain(row.label.chars()) {
                    assert!(
                        character.is_ascii(),
                        "the ASCII set drew {character:?}, which the terminal that \
                         needed the fallback cannot: {detail}",
                    );
                }
            }
        }
    }
}

// ---------------------------------------------------------------------------
// `declared_at` — the bridge from a row on screen to an entry in a file
// ---------------------------------------------------------------------------
//
// `/plugin`'s remove is `edit::Edit::remove("plugin[i]")`, and `i` is an index
// into one file's `[[plugin]]` array. Nothing io-harness returns carries that
// number: `Config::plugins()` hands back loaded bundles and dropped ones as two
// lists ordered by neither the file nor the entry, and it never says which of the
// three scopes declared any of them. So the only thing a row and an entry share is
// the path, and `declared_at` is the whole of the crossing. Everything below is
// about the two ways that crossing can be wrong — landing on the wrong entry, or
// claiming to have found one that is not there — because both are silent: the
// operator loses a bundle they never mentioned and finds out a week later when its
// skills stop being offered.
//
// These fixtures declare paths inside a fresh `tempfile` root, so the user-scope
// file `declared_at` also reads on the machine running the suite cannot name one
// of them and cannot answer instead.

/// A `[[plugin]]` entry naming `path`, spelled as a TOML basic string.
///
/// The escaping is not decoration: `pluginview::add` writes the value through
/// `quoted` for the same reason, and an unescaped absolute Windows path in
/// `path = "C:\Users\..."` is a different path or a parse error. A fixture that
/// wrote `format!("path = \"{}\"", ...)` would be green on Unix and would test the
/// wrong string on the one platform the escaping exists for.
///
/// **Both of `quoted`'s escapes, in `quoted`'s order.** A directory name may hold a
/// `"` as legally as a `\` — the directories inside a clone are named by whoever
/// wrote the clone — and a fixture that escaped only the backslash would write a
/// `path` value that ends early and a file that does not parse.
fn declaration(path: &Path) -> String {
    format!(
        "[[plugin]]\npath = \"{}\"\n\n",
        path.display()
            .to_string()
            .replace('\\', "\\\\")
            .replace('"', "\\\"")
    )
}

/// The entry's index is the entry's own, not the position of anything else.
///
/// The target sits second of three in the file, and the two either side are there
/// so that an off-by-one, a `.position()` over the loaded list, or a row number
/// from `rows` cannot accidentally agree with the right answer.
///
/// Sabotage: return the row's index — `rows(&view, ..)` position — instead of
/// walking the file, which is exactly the shortcut this function exists to refuse
/// and which reads as obviously equivalent in review. Under it only this test
/// fails, and what ships is `/plugin` confirming the bundle on screen and deleting
/// the `[[plugin]]` entry above or below it.
#[test]
fn declared_at_finds_the_entrys_own_index_and_not_a_position_in_some_other_list() {
    let (_dir, root) = root();
    let target = root.join("bundles/rust-review");
    let text = format!(
        "{}{}{}",
        declaration(&root.join("bundles/first")),
        declaration(&target),
        declaration(&root.join("bundles/third")),
    );
    std::fs::write(root.join(LOCAL_FILE), text).expect("the configuration");

    assert_eq!(
        pluginview::declared_at(&root, &target),
        Some((io_harness::config::Scope::Local, 1)),
        "the second of three entries is entry one",
    );
    // The neighbours resolve to themselves, so the answer above is a match rather
    // than a constant.
    assert_eq!(
        pluginview::declared_at(&root, &root.join("bundles/third")),
        Some((io_harness::config::Scope::Local, 2)),
    );
}

/// A declaration written relative and one written absolute name the same
/// directory, and both are found.
///
/// Both spellings are legal and both are common — `path = "bundles/rust-review"`
/// is what an operator types, and `pluginview::add` writes whatever the picker
/// resolved, which is absolute. What comes back from `Config::plugins()` is always
/// the resolved directory, so a comparison against the raw string alone would find
/// the relative entry never, and a comparison against the resolved value alone
/// would find nothing in a file whose author wrote an absolute path with a
/// trailing difference. Both roads are walked and this asserts both.
///
/// Sabotage: drop the `resolved` half and compare only `declared == bundle`. Under
/// it only this test fails, and it fails in the direction that costs most: every
/// relative declaration — the spelling a human writes by hand — becomes a bundle
/// `/plugin` can list and can never remove, so the panel's remove key refuses on a
/// row that is plainly there.
#[test]
fn declared_at_matches_a_relative_declaration_and_an_absolute_one_alike() {
    let bundle = "bundles/rust-review";

    let (_dir, relative) = root();
    std::fs::write(
        relative.join(LOCAL_FILE),
        format!("[[plugin]]\npath = \"{bundle}\"\n"),
    )
    .expect("the configuration");
    assert_eq!(
        pluginview::declared_at(&relative, &relative.join(bundle)),
        Some((io_harness::config::Scope::Local, 0)),
        "a relative declaration is resolved against the root before it is compared",
    );

    let (_dir, absolute) = root();
    let resolved = absolute.join(bundle);
    std::fs::write(absolute.join(LOCAL_FILE), declaration(&resolved)).expect("the configuration");
    assert_eq!(
        pluginview::declared_at(&absolute, &resolved),
        Some((io_harness::config::Scope::Local, 0)),
        "an absolute declaration is compared as written",
    );
}

/// **What `quoted` wrote is what `declared_at` reads back, escapes and all.**
///
/// The write half escapes `\` and `"` into a TOML basic string; until 0.29.0 the
/// read half was `raw.trim().trim_matches('"')`, which decodes nothing. So a bundle
/// whose directory name holds either character was written one way and read back as
/// a different path — and the directories inside a marketplace clone are named by
/// whoever wrote the clone, not by `resolve`, whose alphabet governs only
/// `<owner>/<repo>`. What shipped was `/plugin remove` refusing a row that is
/// plainly on screen, with the generic "no configuration file declares …" sentence
/// rather than the real reason.
///
/// Sabotage: read the value back with `PathBuf::from(raw.trim().trim_matches('"'))`
/// and compare paths, which is the shape this replaced. Under it the quoted name
/// below comes back with its escape still in it — and its two trailing quotes
/// trimmed off as well — so `declared_at` answers `None` for an entry it wrote
/// itself, and only this test fails.
///
/// The plain sibling is asserted in the same test so the failure is a decoding one
/// rather than a fixture that stopped writing a readable file at all.
#[test]
fn declared_at_reads_back_the_escapes_that_quoted_wrote() {
    let (_dir, root) = root();
    // A `"` rather than a `\`: both are escaped by `quoted` and both were lost by
    // the old read, and a quote is a legal directory name on every platform this
    // ships to while a backslash inside one component is Unix-only.
    let quoted_name = root.join("bundles").join("a\"b");
    let plain = root.join("bundles").join("plain");
    std::fs::write(
        root.join(LOCAL_FILE),
        format!("{}{}", declaration(&plain), declaration(&quoted_name)),
    )
    .expect("the configuration");

    assert_eq!(
        pluginview::declared_at(&root, &plain),
        Some((io_harness::config::Scope::Local, 0)),
        "the ordinary entry is not found either, so the fixture is the problem",
    );
    assert_eq!(
        pluginview::declared_at(&root, &quoted_name),
        Some((io_harness::config::Scope::Local, 1)),
        "an entry this surface wrote itself cannot be found again, so `/plugin \
         remove` refuses a bundle that is on the panel",
    );
}

/// A path no file names is `None`, and that is the answer the caller has to be
/// able to get.
///
/// **The most important test here, and the one whose absence is invisible.** The
/// caller's alternative to `None` is removing *something* — the nearest index, row
/// zero, the last entry it looked at — and every one of those deletes a
/// `[[plugin]]` line the operator never pointed at. There is no error, no
/// confirmation that reads wrong, and no way back: the bundle is simply gone from
/// the file, and the next session is missing skills, agents and MCP servers nobody
/// removed.
///
/// The fixture declares a real entry, so `None` here is a miss rather than an
/// empty file — and the near-miss path is a sibling of the declared one, because
/// the mistakes this guards against are prefix and suffix confusions rather than
/// wild pointers.
///
/// Sabotage: end the scope loop with `Some((Scope::Local, 0))` instead of `None`
/// — a "we must have meant the first one" fallback, the kind that gets added to
/// silence an `Option` the caller found awkward. Under it only this test fails.
#[test]
fn declared_at_refuses_a_path_no_file_names_rather_than_guessing() {
    let (_dir, root) = root();
    std::fs::write(
        root.join(LOCAL_FILE),
        declaration(&root.join("bundles/rust-review")),
    )
    .expect("the configuration");

    for missing in [
        root.join("bundles/rust-reviewer"),
        root.join("bundles/rust-review/skills"),
        root.join("bundles"),
        root.join("elsewhere/rust-review"),
    ] {
        assert_eq!(
            pluginview::declared_at(&root, &missing),
            None,
            "{} is named by no file, and the caller was handed an entry to delete",
            missing.display(),
        );
    }

    // The declared one is still found, so the four answers above are misses rather
    // than a function that has stopped working.
    assert!(pluginview::declared_at(&root, &root.join("bundles/rust-review")).is_some());
}

/// Declared in two scopes, the local file answers.
///
/// Local-first is the precedence io-harness itself applies, so the entry that was
/// actually deciding is the entry that gets removed. Removing the project one
/// instead is worse than doing nothing twice over: the bundle keeps loading —
/// `io.local.toml` still names it — and the operator has silently edited a file
/// that is committed and shared with everyone else on the repository.
///
/// Sabotage: iterate `[Scope::User, Scope::Project, Scope::Local]`, which is the
/// order the enum declares its variants in and therefore the order a rewrite
/// naturally reaches for. Under it only this test fails.
#[test]
fn declared_at_answers_the_local_file_when_two_scopes_declare_the_same_bundle() {
    let (_dir, root) = root();
    let target = root.join("bundles/rust-review");
    // Two entries in the project file, so a scope confusion would also land on a
    // different index and cannot be mistaken for a coincidence.
    std::fs::write(
        root.join(PROJECT_FILE),
        format!(
            "{}{}",
            declaration(&root.join("bundles/first")),
            declaration(&target)
        ),
    )
    .expect("the project configuration");
    std::fs::write(root.join(LOCAL_FILE), declaration(&target)).expect("the local configuration");

    assert_eq!(
        pluginview::declared_at(&root, &target),
        Some((io_harness::config::Scope::Local, 0)),
        "the file that was deciding is the file that is edited",
    );
}

/// A configuration with no `[[plugin]]` at all is `None`, and so is no
/// configuration at all.
///
/// The walk stops at the first index `value_at` cannot answer, on the argument
/// that an array of tables is contiguous. At index zero of a file that has no such
/// array that argument has to terminate the loop rather than run it — and a file
/// that is not there at all has to be skipped rather than end the search of the
/// scopes after it.
///
/// Sabotage: `for index in 0..` with a `continue` on the miss instead of a
/// `break`. Nothing fails: the suite is green, `/plugin` works, and the process
/// hangs on the first configuration that declares no bundle — which is every
/// configuration in the world until somebody writes their first `[[plugin]]`.
/// Under this test it fails by never returning, which is what a hang looks like in
/// a suite and is the only way this shape can be caught at all.
#[test]
fn declared_at_is_none_where_no_file_declares_a_plugin_array() {
    let (_dir, empty) = root();
    std::fs::write(empty.join(LOCAL_FILE), "[run]\nmax_steps = 40\n").expect("the configuration");
    assert_eq!(
        pluginview::declared_at(&empty, &empty.join("bundles/rust-review")),
        None,
    );

    // And with nothing written at all, so an unreadable scope file is skipped
    // rather than ending the search.
    let (_dir, bare) = root();
    assert_eq!(
        pluginview::declared_at(&bare, &bare.join("bundles/rust-review")),
        None,
    );
}

// F11 — `/plugin add` refuses a directory that is not a bundle, before writing.
//
// The whole criterion turns on the word *before*. io-harness has no error path
// here: `Config::plugins()` is infallible and an entry naming a directory with no
// manifest is dropped — recorded and otherwise silently absent — so an add that
// wrote first and discovered afterwards would produce exactly the state
// `src/pluginview.rs:15-24` says this surface exists to end.

/// A directory with no manifest is named and refused.
///
/// Sabotage: write the `[[plugin]]` entry and let `io_harness::Plugins` drop it.
/// Under it this test fails, and it fails by producing the silently-absent bundle
/// the module docs open with.
#[test]
fn f11_a_directory_with_no_manifest_is_refused_by_name() {
    let (_guard, root) = root();
    let plain = root.join("not-a-bundle");
    std::fs::create_dir_all(&plain).expect("the directory");

    let refusal = pluginview::refusal(&plain).expect("a directory with no manifest is refused");
    // Named, because "not a bundle" tells an operator nothing about what to do.
    assert!(
        refusal.contains(&plain.display().to_string()),
        "the refusal must name the directory: {refusal}"
    );
    assert!(
        refusal.contains(pluginview::MANIFEST),
        "the refusal must name the file it looked for: {refusal}"
    );
    // And it must say nothing was written, because nothing was.
    assert!(
        refusal.contains("nothing was written"),
        "the refusal must say the file is unchanged: {refusal}"
    );
}

/// A path that is not a directory at all is refused, and differently.
///
/// A file and an empty directory are two different mistakes and the sentence that
/// fixes one does not fix the other.
#[test]
fn f11_a_path_that_is_not_a_directory_is_refused_as_one() {
    let (_guard, root) = root();
    let file = root.join("plugin.toml");
    std::fs::write(&file, "name = \"x\"").expect("the file");

    let refusal = pluginview::refusal(&file).expect("a file is refused");
    assert!(
        refusal.contains("is not a directory"),
        "a file must be refused as a file: {refusal}"
    );
}

/// A directory that is a bundle is accepted.
#[test]
fn f11_a_bundle_directory_is_not_refused() {
    let (_guard, root) = root();
    let dir = bundle(&root, "bundles/rust-review", MINIMAL);

    assert_eq!(pluginview::refusal(&dir), None);
}

/// The candidates are the directories that carry a manifest, and nothing else.
///
/// Sabotage: return every directory below the root. Under it this test fails on
/// the plain directory, and the surface offers rows that are all refused when
/// chosen — a menu whose entries do not work.
#[test]
fn f11_candidates_are_the_directories_carrying_a_manifest() {
    let (_guard, root) = root();
    let review = bundle(&root, "bundles/rust-review", MINIMAL);
    std::fs::create_dir_all(root.join("bundles/notes")).expect("a plain directory");

    let found = pluginview::candidates(&root);
    assert!(
        found.contains(&review),
        "the bundle must be offered: {found:?}"
    );
    assert!(
        !found.contains(&root.join("bundles/notes")),
        "a directory with no manifest must not be offered: {found:?}"
    );
}

/// `target`, `node_modules` and dotted directories are not walked.
///
/// Not a tidiness preference: `target` is the one that makes this walk expensive,
/// and it is walked on a settings screen where an operator is waiting.
#[test]
fn f11_the_candidate_walk_skips_the_directories_nobody_keeps_a_bundle_in() {
    let (_guard, root) = root();
    let kept = bundle(&root, "bundles/rust-review", MINIMAL);
    bundle(&root, "target/debug/rust-review", MINIMAL);
    bundle(&root, "node_modules/rust-review", MINIMAL);
    bundle(&root, ".cache/rust-review", MINIMAL);

    let found = pluginview::candidates(&root);
    assert_eq!(
        found,
        vec![kept],
        "only the bundle outside the skipped directories is offered"
    );
}

/// The walk is bounded, and a bundle below the bound is not offered.
///
/// Asserted so the bound is a decision rather than an accident: an operator whose
/// bundle sits deeper has the typed path, and it is refused by the same check.
#[test]
fn f11_the_candidate_walk_is_bounded() {
    let (_guard, root) = root();
    let near = bundle(&root, "bundles/rust-review", MINIMAL);
    bundle(&root, "a/b/c/d/deep", MINIMAL);

    let found = pluginview::candidates(&root);
    assert!(found.contains(&near));
    assert!(
        !found.contains(&root.join("a/b/c/d/deep")),
        "a bundle below the depth bound is not walked to: {found:?}"
    );
}

/// The candidate order is stable, so the row an operator picked yesterday is the
/// row in the same place today.
#[test]
fn f11_the_candidate_order_is_stable() {
    let (_guard, root) = root();
    bundle(&root, "zeta", MINIMAL);
    bundle(&root, "alpha", MINIMAL);
    bundle(&root, "bundles/rust-review", MINIMAL);

    // **`candidates(&root) == candidates(&root)` was here and asserted nothing**:
    // the same pure call twice, over a function that ends in a sort. It could not
    // fail, and "the order is stable" is not what it was checking. The order is a
    // property of the sort key, so the sort key is what has to be pinned — both
    // halves of it, because dropping the path tiebreak leaves the depth half
    // sorted and lets same-depth rows follow whatever order `read_dir` happened
    // to return on that machine.
    let found = pluginview::candidates(&root);
    let depths: Vec<usize> = found.iter().map(|path| path.components().count()).collect();
    let mut sorted = depths.clone();
    sorted.sort_unstable();
    assert_eq!(depths, sorted, "shallower bundles come first: {found:?}");

    // The tiebreak: `alpha` and `zeta` sit at the same depth, so only the path
    // half of the key can order them, and it must order them the same way twice.
    let same_depth: Vec<String> = found
        .iter()
        .filter(|path| path.components().count() == found[0].components().count())
        .map(|path| {
            path.file_name()
                .unwrap_or_default()
                .to_string_lossy()
                .to_string()
        })
        .collect();
    let mut expected = same_depth.clone();
    expected.sort();
    assert_eq!(
        same_depth, expected,
        "two bundles at one depth must be ordered by their path, or the row an operator picked \
         yesterday is somewhere else today: {found:?}"
    );
}

/// A bundle below the root is written relative, so a committed `io.toml` works for
/// everyone who clones it.
///
/// Sabotage: always write the absolute path. Under it this test fails, and the
/// entry it writes names a directory that exists on exactly one machine.
#[test]
fn f11_a_bundle_below_the_root_is_declared_relative() {
    let (_guard, root) = root();
    let dir = bundle(&root, "bundles/rust-review", MINIMAL);

    assert_eq!(
        pluginview::declared(&root, &dir),
        Path::new("bundles/rust-review"),
    );
}

/// A bundle outside the root keeps its absolute path, because a relative one would
/// resolve against the discovery root and name somewhere else entirely.
#[test]
fn f11_a_bundle_outside_the_root_is_declared_absolute() {
    let (_elsewhere_guard, elsewhere) = root();
    let (_guard, here) = root();
    let dir = bundle(&elsewhere, "rust-review", MINIMAL);

    assert_eq!(pluginview::declared(&here, &dir), dir);
}

/// The written entry re-reads as a loaded bundle through io-harness's own reader.
///
/// The end-to-end half of F11: the check passed, the edit applied, and the thing
/// the operator was promised is what `Config::plugins()` reports.
#[test]
fn f11_an_added_bundle_loads_through_the_harness() {
    let (_guard, root) = root();
    let dir = bundle(&root, "bundles/rust-review", MINIMAL);
    assert_eq!(pluginview::refusal(&dir), None);

    let written = pluginview::declared(&root, &dir);
    let text = io_cli::edit::apply("", &[pluginview::add(&written)]).expect("the edit applies");
    std::fs::write(root.join(PROJECT_FILE), &text).expect("the project file");

    let config = Config::discover(&root).expect("the written file loads");
    let view = pluginview::view(&config.plugins());
    // By id, for the reason `listed` below states: `Config::discover` layers the
    // user file of whoever is running the suite over this root, and a length or a
    // whole-list comparison would answer about their bundles as well as this one.
    assert!(
        !view
            .refused
            .iter()
            .any(|refused| refused.id == "rust-review"),
        "the added bundle must not be refused: {:?}",
        view.refused
    );
    assert!(
        view.plugins.iter().any(|p| p.id == "rust-review"),
        "the added bundle did not load: {:?}",
        view.plugins
            .iter()
            .map(|p| p.id.as_str())
            .collect::<Vec<_>>(),
    );
}

// ---------------------------------------------------------------------------
// F7 — a bundle declared `enabled = false` is declared, off, and visible
// ---------------------------------------------------------------------------
//
// io-harness 0.70.0 splits what a configuration declared into three buckets:
// `Plugins::iter` is what loaded, `Plugins::dropped` is what was refused, and
// `Plugins::disabled` is what was written `enabled = false` — read, parsed, held
// to the whole trust rule, contributing nothing. `Plugins::len` and
// `Plugins::is_empty` say so in their own rustdoc: they answer about the loaded
// bucket alone.
//
// Everything below is about the half of F7 that io-cli owns. The declining verb
// that writes the key is a separate criterion; these tests write the key by hand,
// which is also how an operator with an editor arrives at this state, and assert
// that `/plugin` can see it. A bundle absent from every listing reads exactly like
// one nobody ever declared — and until 0.29.0 `pluginview::view` read `iter()` and
// `dropped()` and nothing else, so it was absent from every listing io-cli has.

/// The id every F7 fixture declares, named once so the assertions below can key on
/// it instead of counting.
const OFF: &str = "rust-review";

/// The row `/plugin` drew for `id`, or a failure naming what it drew instead.
///
/// **By id and never by index or by count**, which is the rule `tests/marketplace.rs`
/// states for itself and which these fixtures need for the same reason:
/// `Config::discover` layers the operator's own user file over this root, so a
/// developer whose `~/.io-cli/io.toml` declares a bundle adds rows this fixture
/// never wrote. `pluginview::view` chains the loaded bundles **ahead** of the
/// switched-off ones, so on such a machine `view.plugins[0]` is that developer's
/// bundle and an assertion about the flag fails while F7 itself holds — a red suite
/// for a reason that has nothing to do with the criterion, on the machine where the
/// change is being written and nowhere on CI.
///
/// Addressing by id keeps every failure meaning intact rather than weakening it:
/// this panics where the bundle is absent, which is F7's first sabotage, and the
/// caller still reads the flag off the row, which is its second.
fn listed<'a>(view: &'a pluginview::View, id: &str) -> &'a pluginview::Listed {
    view.plugins
        .iter()
        .find(|row| row.id == id)
        .unwrap_or_else(|| {
            panic!(
                "`{id}` is on no list `/plugin` draws, so the panel says nothing about \
                 an `io.toml` that declares it; listed: {:?}",
                view.plugins
                    .iter()
                    .map(|row| row.id.as_str())
                    .collect::<Vec<_>>(),
            )
        })
}

/// A configuration declaring `path` as a bundle switched off, and nothing else.
fn declaring_off(root: &Path, path: &str) {
    std::fs::write(
        root.join(LOCAL_FILE),
        format!("[[plugin]]\npath = \"{path}\"\nenabled = false\n"),
    )
    .expect("the configuration");
}

/// **F7, the visibility half.** A configuration declaring exactly one bundle, off,
/// is not a configuration with no bundles — `View::is_empty` is `false` and the
/// bundle is in the list, flagged.
///
/// The three assertions are three different claims and each one can fail alone:
/// io-harness put the bundle on `disabled()` rather than loading or dropping it,
/// io-cli carried that bucket into the view, and `is_empty` therefore answers for
/// a declaration rather than for a load.
///
/// Sabotage: drop the `.chain(plugins.disabled()…)` from `pluginview::view`. Under
/// it `view.plugins` is empty, `View::is_empty` returns `true`, and `/plugin`
/// prints "no capability bundles are declared yet" over an `io.toml` declaring
/// one — which is the false sentence `src/pluginview.rs`'s module docs exist to
/// end, told about a bundle the operator switched off themselves and can switch
/// back on in one keystroke if they can see it.
///
/// Second sabotage: mark the disabled bundles `enabled: true` in `view`. Under it
/// this test still passes on `is_empty` and fails on the flag — which is the
/// assertion that makes the flag load-bearing rather than decorative, since
/// `bundle_skills` in `src/main.rs` filters the skills palette on it.
///
/// Every bucket is addressed by id rather than by length, for the reason [`listed`]
/// states: `Config::discover` layers the user file of whoever is running the suite.
#[test]
fn f7_a_configuration_declaring_only_a_switched_off_bundle_is_not_empty() {
    let (_dir, root) = root();
    bundle(&root, "bundles/rust-review", MINIMAL);
    declaring_off(&root, "bundles/rust-review");

    let config = Config::discover(&root).expect("a switched-off bundle is not a broken file");
    let plugins = config.plugins();

    // io-harness's three buckets, asserted where they are — so a change of
    // bucket in a future pin fails here rather than somewhere downstream.
    assert!(
        !plugins.names().contains(&OFF),
        "a switched-off bundle loaded: {:?}",
        plugins.names(),
    );
    assert!(
        !plugins.dropped().iter().any(|d| d.id == OFF),
        "`enabled = false` was treated as a failure rather than a choice: {:?}",
        plugins
            .dropped()
            .iter()
            .map(|d| d.error.clone())
            .collect::<Vec<_>>(),
    );
    assert!(
        plugins.disabled().iter().any(|p| p.id() == OFF),
        "the bundle is in none of the three buckets, so io-harness lost it: {:?}",
        plugins
            .disabled()
            .iter()
            .map(io_harness::Plugin::id)
            .collect::<Vec<_>>(),
    );

    // And io-cli's surface, which is what the criterion is about. `listed` fails
    // by name where the bundle is on no list at all.
    let view = pluginview::view(&config.plugins());
    let off = listed(&view, OFF);
    assert!(
        !view.refused.iter().any(|refused| refused.id == OFF),
        "a switched-off bundle was carried as a refusal, which tells the operator \
         to fix something that is not broken: {:?}",
        view.refused,
    );
    assert!(
        !off.enabled,
        "the bundle is listed as loaded, so nothing on this surface says the \
         operator switched it off",
    );
    assert!(
        !view.is_empty(),
        "`View::is_empty` is true for a configuration declaring a bundle",
    );
}

/// **F7 rendering.** The switched-off bundle draws under its own mark, with the
/// state leading the row rather than a list of contributions this session has not
/// got.
///
/// Sabotage, either half: drop the disabled bucket from `pluginview::view` and
/// there is no row at all — the row lookup fails by name before the mark is read.
/// Draw the row under `LOADED_MARK` and the list says the bundle is contributing
/// while `Config::plugins()` says it contributes nothing, which is two surfaces
/// disagreeing about one bundle in the one direction an operator cannot detect:
/// the panel is the only place they look.
///
/// The row is found by its label rather than taken at index zero, for the reason
/// [`listed`] states — the suite runs on machines whose user file declares bundles
/// of their own, and those draw rows too.
#[test]
fn f7_a_switched_off_bundle_draws_under_its_own_mark() {
    let (_dir, root) = root();
    bundle(&root, "bundles/rust-review", MINIMAL);
    declaring_off(&root, "bundles/rust-review");
    let config = Config::discover(&root).expect("the configuration loads");
    let view = pluginview::view(&config.plugins());

    for glyphs in [&io_cli::glyphs::UNICODE, &io_cli::glyphs::ASCII] {
        // Wide enough that nothing is shortened, so the words below are compared
        // rather than prefix-matched.
        let rows = pluginview::rows(&view, 400, glyphs);
        let row = rows.iter().find(|row| row.label == OFF).unwrap_or_else(|| {
            panic!(
                "{}: the declared bundle drew no row; drawn: {:?}",
                glyphs.name,
                rows.iter().map(|row| row.label.clone()).collect::<Vec<_>>(),
            )
        });
        assert_eq!(
            row.mark,
            Some(pluginview::DISABLED_MARK),
            "{}: the switched-off bundle is not marked apart from a loaded one",
            glyphs.name,
        );
        assert_ne!(
            pluginview::DISABLED_MARK,
            pluginview::LOADED_MARK,
            "the two states share a mark, so the row cannot say which it is",
        );
        assert_ne!(
            pluginview::DISABLED_MARK,
            pluginview::REFUSED_MARK,
            "a switched-off bundle wears the mark that means something is broken",
        );

        let detail = row
            .detail
            .clone()
            .expect("a switched-off bundle has a detail");
        assert_eq!(
            detail.split(glyphs.separator).next(),
            Some("switched off"),
            "{}: the row leads with what the bundle contributed, which it did \
             not: {detail}",
            glyphs.name,
        );
        assert!(
            detail.contains("skills, templates, agents, policy"),
            "{}: what switching it back on would bring is not on the row: {detail}",
            glyphs.name,
        );
    }
}

/// **F7, and the bug the flag exists to stop.** A switched-off bundle is on the
/// list and on no turn: nothing it declares reaches the contract, and its skills
/// directory is still readable — so the flag, not an absent field, is the only
/// thing keeping it out of the `/skills` palette.
///
/// Sabotage: drop the `.filter(|listed| listed.enabled)` from `bundle_skills` in
/// `src/main.rs`. Under it `/skills` offers the model a skill from a bundle
/// `TaskContract::discover_skills` never folded in, and the run fails on a name
/// the surface said was there. That filter is not visible from an integration
/// test — `bundle_skills` is private to the binary — so what is asserted here is
/// the fact that makes it necessary: `Listed::skills` is `Some` for a bundle
/// contributing nothing.
#[test]
fn f7_a_switched_off_bundle_reaches_no_turn_while_its_directories_still_read() {
    let (_dir, root) = root();
    bundle(&root, "bundles/rust-review", MINIMAL);
    declaring_off(&root, "bundles/rust-review");
    let config = Config::discover(&root).expect("the configuration loads");

    let contract =
        io_cli::contract::configured("review this", root.clone(), &config, &config.plugins());
    assert!(
        contract.agents.get("rust-review__reviewer").is_none(),
        "a switched-off bundle put an agent on the contract: {:?}",
        contract.agents.names(),
    );
    assert!(
        !contract.plugins.names().contains(&OFF),
        "`contract.plugins` is what `discover_skills` folds a bundle's skills in \
         from, and a switched-off bundle is in it: {:?}",
        contract.plugins.names(),
    );

    // By id rather than by index, for the reason `listed` states: the user file of
    // whoever is running the suite is layered over this root and its bundles are
    // chained ahead of the switched-off ones.
    let view = pluginview::view(&config.plugins());
    let off = listed(&view, OFF);
    assert!(
        off.skills.is_some(),
        "the switched-off bundle's skills directory did not read, so this test \
         would pass for the wrong reason and `bundle_skills` would look safe",
    );
    assert_eq!(
        off.agents,
        vec!["rust-review__reviewer"],
        "io-harness namespaces a switched-off bundle's names too, so the detail \
         pane can say what switching it on would bring",
    );
}

// ---------------------------------------------------------------------------
// F14 — a bundle whose manifest io wrote is drawn as one
// ---------------------------------------------------------------------------

/// Where a generated manifest sits, under the fixture's own adapters root.
///
/// `<adapters>/<owner>/<repo>/<name>`, which is the layout `adapt::at` writes and
/// the only thing `Listed::adapted` reads. A real adapter is written under
/// `home::adapters()`; the prefix is what decides the answer, so a fixture that
/// reproduces the shape reproduces the case without moving anybody's home.
const ADAPTED_AT: &str = "adapters/zeroonething/ultraship/claude-review";

/// The bundle id the generated manifest declares.
const ADAPTED: &str = "claude-review";

/// A manifest of the shape `adapt::generate` writes: a name, the two directories it
/// found in the clone, and a description long enough to compete with the root for
/// what an eighty-column row has left.
///
/// The description is deliberately not the bundle's name, so a row's detail
/// containing `claude-review` can only have got it from the path.
const GENERATED: &str = r#"
name = "claude-review"
description = "A code review bundle published for another agent, adapted by io."
skills = "skills"
templates = "templates"
"#;

/// A root declaring one adapted bundle and one native one, in that order.
fn one_adapted_one_native() -> (tempfile::TempDir, pluginview::View) {
    let (dir, root) = root();
    bundle(&root, "bundles/rust-review", MINIMAL);
    bundle(&root, ADAPTED_AT, GENERATED);
    declaring(&root, LOCAL_FILE, &["bundles/rust-review", ADAPTED_AT]);
    let config = Config::discover(&root).expect("the configuration loads");
    let mut view = pluginview::view(&config.plugins());
    view.adapters = Some(adapters_of(&view));
    (dir, view)
}

/// The adapters root **as io-harness resolved it**, taken back off the bundle's own
/// root rather than rebuilt from the fixture's directory.
///
/// A temporary directory is reached through a symlink on macOS — `/var/folders/…`
/// and `/private/var/folders/…` are the same directory — and `Path::starts_with`
/// compares components, so a prefix assembled here from `root()` would not be a
/// prefix of the path the harness read and every row would draw as native for a
/// reason that has nothing to do with the criterion.
fn adapters_of(view: &pluginview::View) -> PathBuf {
    listed(view, ADAPTED)
        .root
        .ancestors()
        .nth(3)
        .expect("`<adapters>/<owner>/<repo>/<name>` has three directories above it")
        .to_path_buf()
}

/// **F14.** An adapted bundle draws under its own mark and the native bundle
/// listed beside it does not.
///
/// Asserted as the list of labels wearing `ADAPTED_MARK` rather than as a
/// `contains`, because both failures this gate exists for produce a list that still
/// contains the right row: the mark drawn on nothing gives an empty list, and the
/// mark drawn on everything gives a list of two. A fixture holding one bundle of
/// each kind is what makes the second one visible at all.
///
/// Sabotage: drop the `adapted` arm from the mark in `pluginview::rows`. The
/// adapted bundle draws `+` like any other, an operator has no way to tell a
/// manifest io wrote from one a person did, and the generated file they must open
/// when io-harness refuses the bundle is named nowhere.
#[test]
fn f14_an_adapted_bundle_draws_its_own_mark_and_a_native_one_beside_it_does_not() {
    let (_dir, view) = one_adapted_one_native();

    for glyphs in [&io_cli::glyphs::UNICODE, &io_cli::glyphs::ASCII] {
        // Wide enough that nothing is shortened, so what is read off the row is
        // the mark rather than what survived the fitter.
        let rows = pluginview::rows(&view, 400, glyphs);
        let marked: Vec<&str> = rows
            .iter()
            .filter(|row| row.mark == Some(pluginview::ADAPTED_MARK))
            .map(|row| row.label.as_str())
            .collect();
        assert_eq!(
            marked,
            vec![ADAPTED],
            "{}: the adapted mark is on {} of the drawn rows rather than on the one \
             bundle io wrote a manifest for; drawn: {:?}",
            glyphs.name,
            marked.len(),
            rows.iter()
                .map(|row| (row.mark, row.label.clone()))
                .collect::<Vec<_>>(),
        );

        let native = rows
            .iter()
            .find(|row| row.label == "rust-review")
            .expect("the native bundle drew a row");
        assert_eq!(
            native.mark,
            Some(pluginview::LOADED_MARK),
            "{}: a bundle whose author wrote its `plugin.toml` is drawn as adapted",
            glyphs.name,
        );
    }
}

/// **F14, the half a mark cannot carry.** An adapted row names the directory the
/// generated manifest is in, at the width the row is actually drawn at.
///
/// One character says *io wrote this manifest*; it does not say **where**, and the
/// generated file is under a directory the operator never typed. Eighty columns
/// rather than four hundred, because a fact that is only true on a wide terminal is
/// not one an operator can rely on: the root has a floor and a reserved place ahead
/// of the description, which is a line io copied out of somebody else's metadata.
///
/// Sabotage: drop the reservation in `pluginview::rows` and fit the description
/// against the whole of what is left. This description is longer than the room, so
/// it takes all of it, the root falls below its floor and is dropped whole, and the
/// row that carries the mark stops carrying the path the mark is about.
#[test]
fn f14_an_adapted_row_names_the_directory_the_generated_manifest_is_in() {
    let (_dir, view) = one_adapted_one_native();

    for glyphs in [&io_cli::glyphs::UNICODE, &io_cli::glyphs::ASCII] {
        let rows = pluginview::rows(&view, 80, glyphs);
        let row = rows
            .iter()
            .find(|row| row.label == ADAPTED)
            .expect("the adapted bundle drew a row");
        let detail = row.detail.clone().expect("a listed bundle has a detail");
        assert!(
            detail.contains(ADAPTED),
            "{}: the row wears the adapted mark and does not say which directory \
             the generated manifest is in: {detail}",
            glyphs.name,
        );
        // The picker's own budget: marker, label, gap, detail. A row that names the
        // path by overrunning the terminal has not named it.
        assert!(
            row.label.chars().count() + detail.chars().count() + 4 <= 80,
            "{}: the row does not fit eighty columns: {detail}",
            glyphs.name,
        );
    }
}

/// **The one thing `ADAPTED_MARK` taking the column costs, asserted rather than
/// argued.**
///
/// A row carries one mark, so an adapted bundle that is also **switched off**
/// draws `~` and not `-`. `ADAPTED_MARK`'s own rustdoc says that is safe because a
/// switched-off bundle already leads its detail with the words — and a sentence in
/// a doc comment is not a gate. This is the gate: the state has to survive the
/// column being spent, or the trade the constant argues for is one that lost an
/// operator a fact.
///
/// Sabotage: stop leading a switched-off row's detail with its state. Every other
/// bundle in the product keeps its `-`, so nothing else in the suite goes red —
/// only the row where the mark was spent on something else.
#[test]
fn an_adapted_bundle_that_is_switched_off_still_says_so_in_its_detail() {
    let (_dir, root) = root();
    bundle(&root, ADAPTED_AT, GENERATED);
    declaring_off(&root, ADAPTED_AT);
    let config = Config::discover(&root).expect("the configuration loads");
    let mut view = pluginview::view(&config.plugins());
    view.adapters = Some(adapters_of(&view));

    let rows = pluginview::rows(&view, 200, &io_cli::glyphs::ASCII);
    let row = rows
        .iter()
        .find(|row| row.label == ADAPTED)
        .expect("the switched-off adapted bundle is listed at all");

    assert_eq!(
        row.mark,
        Some(pluginview::ADAPTED_MARK),
        "the adapted mark takes the column from the state mark — that is the \
         decision this test exists to hold to its own terms",
    );
    let detail = row.detail.as_deref().unwrap_or_default();
    assert!(
        detail.starts_with("switched off"),
        "so the state must lead the detail, or spending the column lost it: \
         {detail:?}",
    );
}

// ---------------------------------------------------------------------------
// N1/N2/N3 — the plugin set is resolved once, and re-resolved only when it moved
// ---------------------------------------------------------------------------

/// **N2 — asking again, with nothing changed, resolves nothing.**
///
/// `Resolved::stale` stats each declared manifest and compares its modified time
/// and its length. It parses no TOML and opens no skill file, so asking costs a
/// bounded number of `metadata` calls rather than the resolution it is deciding
/// whether to repeat — which is what makes `/plugin` and `/skills` free to open
/// when the disk has not moved.
#[test]
fn n2_nothing_on_disk_moved_means_nothing_is_resolved_again() {
    let (_guard, root) = root();
    bundle(&root, "bundles/rust", MINIMAL);
    declaring(&root, io_harness::config::LOCAL_FILE, &["bundles/rust"]);
    let config = io_harness::Config::discover(&root).expect("the configuration");

    let resolved = io_cli::resolved::Resolved::load(&config);
    assert_eq!(
        resolved.loaded().len(),
        1,
        "the fixture declares one bundle"
    );
    for _ in 0..5 {
        assert!(
            !resolved.stale(&config),
            "nothing was written, so nothing may be resolved again",
        );
    }
}

/// **N3 — an edited manifest is seen.**
///
/// The rule is stated in `src/resolved.rs` and in `docs/guide/plugins.md`, and it
/// is asserted here rather than described: an operator who edits a bundle in
/// another window and reopens `/plugin` must see the edit, or the cache has
/// quietly replaced their file with a copy of it.
#[test]
fn n3_an_edited_manifest_is_seen() {
    let (_guard, root) = root();
    let dir = bundle(&root, "bundles/rust", MINIMAL);
    declaring(&root, io_harness::config::LOCAL_FILE, &["bundles/rust"]);
    let config = io_harness::Config::discover(&root).expect("the configuration");

    let resolved = io_cli::resolved::Resolved::load(&config);
    assert!(!resolved.stale(&config));

    // A real edit: the description changes, so the length changes with it.
    std::fs::write(
        dir.join(PLUGIN_FILE),
        MINIMAL.replace(
            "Everything our Rust reviews need.",
            "Everything our Rust reviews need, and then some more besides.",
        ),
    )
    .expect("the edited manifest");

    assert!(
        resolved.stale(&config),
        "an edit to a declared manifest was not seen, so `/plugin` would draw the \
         file the operator no longer has",
    );
}

/// **N3 — a bundle installed or removed is always seen**, whatever the mtimes
/// say, because the declared set itself is what is compared.
#[test]
fn n3_a_bundle_added_or_removed_is_always_seen() {
    let (_guard, root) = root();
    bundle(&root, "bundles/rust", MINIMAL);
    declaring(&root, io_harness::config::LOCAL_FILE, &["bundles/rust"]);
    let config = io_harness::Config::discover(&root).expect("the configuration");
    let resolved = io_cli::resolved::Resolved::load(&config);
    assert!(!resolved.stale(&config));

    bundle(
        &root,
        "bundles/docs",
        &MINIMAL.replace("rust-review", "docs-review"),
    );
    declaring(
        &root,
        io_harness::config::LOCAL_FILE,
        &["bundles/rust", "bundles/docs"],
    );
    let wider = io_harness::Config::discover(&root).expect("the configuration");
    assert!(
        resolved.stale(&wider),
        "a second bundle was declared and the resolution did not notice — the \
         stamps are compared whole for exactly this",
    );
}

/// **N3's stated limit, asserted rather than described.**
///
/// A filesystem whose mtime granularity cannot separate two writes inside one
/// second will not distinguish them, and if the length is unchanged too there is
/// nothing left to compare. This is what the rule in `src/resolved.rs` and
/// `docs/guide/plugins.md` says, and a cache that cannot prove freshness should
/// say so rather than imply a guarantee it has not got.
///
/// Written by forcing the case directly — restoring both the content length and
/// the modified time — because waiting for a same-second write would be a clock
/// in a test, which `tests/timing.rs` forbids and is right to.
#[test]
fn n3_two_writes_in_one_second_of_the_same_length_are_the_documented_limit() {
    let (_guard, root) = root();
    let dir = bundle(&root, "bundles/rust", MINIMAL);
    declaring(&root, io_harness::config::LOCAL_FILE, &["bundles/rust"]);
    let config = io_harness::Config::discover(&root).expect("the configuration");

    let manifest = dir.join(PLUGIN_FILE);
    let before = std::fs::metadata(&manifest)
        .expect("the manifest")
        .modified();
    let resolved = io_cli::resolved::Resolved::load(&config);

    // Same length, and the modified time put back to what it was.
    std::fs::write(&manifest, MINIMAL.replace("cheap-model", "cheep-model"))
        .expect("the second write");
    if let Ok(at) = before {
        let file = std::fs::File::options()
            .write(true)
            .open(&manifest)
            .expect("the manifest");
        let _ = file.set_modified(at);
    }

    assert!(
        !resolved.stale(&config),
        "this is the documented limit, and the documentation is what has to be \
         true: a same-second write of the same length is not distinguishable by a \
         stat, and the rule says so rather than claiming a freshness it cannot \
         prove",
    );
}

// ---------------------------------------------------------------------------
// `ids` and `by_id` — the handle a bundle's name gives on its entry
// ---------------------------------------------------------------------------
//
// `io plugin add <name>` installs by name and then tells the operator that
// removing it takes the same name. `manage::plan` answers that word by asking the
// disk about a directory first and matching declared ids only after — over the
// pairs `ids` builds here, because resolving them inside `plan` would mean
// `Config::plugins()`, a full re-parse of every declared manifest that
// `tests/dependencies.rs` confines to `src/resolved.rs` by exact path.

/// **Every declared bundle is in the pairs** — loaded, switched off, and refused.
///
/// The refused one is the point: it is the entry an operator most wants gone and
/// the one they cannot fix from a manifest that does not parse, and its id is the
/// directory's own name where io-harness could read no manifest at all.
///
/// Sabotage: build the list from `view.plugins` alone. Under it only this test and
/// its sibling in `tests/manage.rs` fail, and what ships is a broken entry
/// `/plugin` lists under a name that removes nothing.
#[test]
fn ids_carries_every_declared_bundle_including_the_ones_that_did_not_load() {
    let (_dir, root) = root();
    bundle(&root, "bundles/rust-review", MINIMAL);
    bundle(
        &root,
        "bundles/off",
        &MINIMAL.replace("rust-review", "switched-off"),
    );
    empty_bundle(&root, "bundles/ghost");
    std::fs::write(
        root.join(LOCAL_FILE),
        "[[plugin]]\npath = \"bundles/rust-review\"\n\n\
         [[plugin]]\npath = \"bundles/off\"\nenabled = false\n\n\
         [[plugin]]\npath = \"bundles/ghost\"\n",
    )
    .expect("the configuration");

    let config = Config::discover(&root).expect("the configuration");
    let pairs = pluginview::ids(&pluginview::view(&config.plugins()));

    for (id, at) in [
        ("rust-review", "bundles/rust-review"),
        ("switched-off", "bundles/off"),
        ("ghost", "bundles/ghost"),
    ] {
        assert!(
            pairs
                .iter()
                .any(|(name, dir)| name == id && dir.ends_with(at)),
            "`{id}` at `{at}` is not among the declared pairs, so `plugin remove \
             {id}` has nothing to match: {pairs:?}",
        );
    }
    // One pair per entry and no entry counted twice. Filtered to this root: the
    // machine running the suite has a user-scope file of its own, which
    // `Config::discover` reads too and which is nothing to assert about.
    let ours = pairs.iter().filter(|(_, dir)| dir.starts_with(&root)).count();
    assert_eq!(ours, 3, "one pair per declared entry: {pairs:?}");
}

/// **Every bundle of that name comes back, and never just the first.**
///
/// An id is unique among the bundles io-harness *loaded*; two declared
/// `enabled = false` share one whenever an operator is swapping `tools-v1` for
/// `tools-v2`. A helper that answered with one of them would decide, here, which
/// `[[plugin]]` entry gets deleted — silently, and the caller could not tell that a
/// choice had been made at all.
///
/// Pure, over a slice written out in the test: the point is the matching, and a
/// fixture on disk would be asserting `Config::discover` again.
///
/// Sabotage: `.find()` in place of `.filter()`. Only this test fails.
#[test]
fn by_id_answers_with_every_bundle_of_that_name() {
    let declared = vec![
        ("twin".to_string(), PathBuf::from("/bundles/one")),
        ("other".to_string(), PathBuf::from("/bundles/two")),
        ("twin".to_string(), PathBuf::from("/bundles/three")),
    ];

    assert_eq!(
        pluginview::by_id(&declared, "twin"),
        vec![Path::new("/bundles/one"), Path::new("/bundles/three")],
        "both bundles of that name, in the order they were declared",
    );
    assert_eq!(
        pluginview::by_id(&declared, "other"),
        vec![Path::new("/bundles/two")],
        "the unique name answers with the one bundle, so the assertion above is a \
         match rather than a constant",
    );
    assert!(
        pluginview::by_id(&declared, "twi").is_empty(),
        "a name is matched whole: a prefix of one is not a bundle",
    );
    assert!(pluginview::by_id(&[], "twin").is_empty());
}
