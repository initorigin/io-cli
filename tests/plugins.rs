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
    let contract = io_cli::contract::configured("review this", root.clone(), &config);
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
    let view = pluginview::view(&config);
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
    let view = pluginview::view(&config);
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
        let contract = io_cli::contract::configured("go", root.clone(), &config);
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
            io_cli::contract::hooks(&config, &root).is_none(),
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

        let contract = io_cli::contract::configured("go", root.clone(), &config);
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
                io_cli::contract::hooks(&config, &root).is_some(),
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
    let view = pluginview::view(&config);
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
    let view = pluginview::view(&config);

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
/// The backslash escaping is not decoration: `pluginview::add` writes the value
/// through `quoted` for the same reason, and an unescaped absolute Windows path in
/// `path = "C:\Users\..."` is a different path or a parse error. A fixture that
/// wrote `format!("path = \"{}\"", ...)` would be green on Unix and would test the
/// wrong string on the one platform the escaping exists for.
fn declaration(path: &Path) -> String {
    format!(
        "[[plugin]]\npath = \"{}\"\n\n",
        path.display().to_string().replace('\\', "\\\\")
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
