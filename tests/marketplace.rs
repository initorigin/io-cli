//! F1, F2 and F3 — a marketplace is added by name through one parse, holds the
//! directories that carry a manifest, and takes nothing declared with it when it
//! goes.
//!
//! Every test here drives `io_cli::marketplace` and `io_cli::manage` the way both
//! doors drive them: a token slice starting at the surface word, and a path passed
//! in rather than looked up. That is the whole reason the module takes its
//! directory as an argument — a decision behind `crate::home` is a decision nothing
//! under `tests/` can reach without moving `HOME` out from under a suite running in
//! parallel, and this crate has shipped untestable driver logic in three releases.
//!
//! **Nothing here spawns `git` and nothing here reaches a network.** `tests/fetch.rs`
//! owns the clone itself, both its endings and the staging directory; what is left
//! for this file is everything that happens to a directory once it is on the disk,
//! which is answerable against a fixture and is therefore answered against one.

use std::path::{Path, PathBuf};

use io_cli::fetch::Named;
use io_cli::manage::{self, MarketVerb, PluginVerb, Request};
use io_cli::marketplace::{self, Market, Went};
use io_harness::config::Scope;

/// The token slice a shell hands `io`, spelled the way a test can read.
fn argv(words: &[&str]) -> Vec<String> {
    words.iter().map(|word| word.to_string()).collect()
}

/// The name used throughout, spelled once.
fn named() -> Named {
    Named {
        owner: "zeroonething".to_string(),
        repo: "ultraship".to_string(),
    }
}

/// One of this crate's own source files, with its comments taken out.
///
/// **The comments go, and 0.19.0 and 0.25.0 are why.** These gates assert that a
/// call does *not* appear in a file; a file that explains which call it is
/// deliberately not making would fail its own gate, and a rule that forbids a
/// module from documenting itself is a rule that gets deleted instead of obeyed.
/// The property is about what the code does, so the check reads code.
fn code_of(name: &str) -> String {
    let path = Path::new(env!("CARGO_MANIFEST_DIR")).join(name);
    // Normalised: a Windows checkout has `\r\n`, and a gate that sliced on `"\n"`
    // matched nothing and panicked on a green product in 0.19.0 and 0.23.0.
    std::fs::read_to_string(&path)
        .unwrap_or_else(|error| panic!("{name} is beside the tests: {error}"))
        .replace("\r\n", "\n")
        .lines()
        .filter(|line| !line.trim_start().starts_with("//"))
        .collect::<Vec<_>>()
        .join("\n")
}

/// Write `text` into `dir/plugin.toml`, creating the directory.
fn manifest(dir: &Path, text: &str) {
    std::fs::create_dir_all(dir).expect("the bundle directory");
    std::fs::write(dir.join(io_cli::pluginview::MANIFEST), text).expect("the manifest");
}

// --- F1: one name, one parse, one destination ---------------------------------

/// **F1 — the slash form and the argv form are one token slice and one request.**
///
/// The comparison is against the **value** both must produce rather than
/// `parse(X)` against `parse(X)`: the token assertion above it has already
/// established that the two slices are the same slice, so comparing the two parses
/// would be a tautology — which is exactly the vacuity `tests/manage.rs` records
/// having shipped in its own F7 gate.
///
/// The name is carried as a `Named` and not as text, so nothing downstream can
/// read it a second time with a second opinion about what a name may contain.
///
/// Sabotage: give the argv form its own branch — see the structural test below,
/// which is where that arm dies, because a byte comparison cannot see two code
/// paths that were kept in step by hand until the release where one grew a flag.
#[test]
fn f1_the_slash_form_and_the_argv_form_are_one_parse() {
    let typed = manage::tokens("/plugin marketplace add zeroonething/ultraship");
    assert_eq!(
        typed,
        argv(&["plugin", "marketplace", "add", "zeroonething/ultraship"]),
        "the composer's tokeniser must leave what a shell leaves after the binary name",
    );

    let request = manage::parse(&typed).expect("the slash form parses");
    assert_eq!(
        request,
        Request::Plugin(PluginVerb::Marketplace(MarketVerb::Add(named()))),
        "the slash form must parse to the request the argv form does, and to this one",
    );

    // And a `.git` suffix and a trailing slash — what a paste leaves behind —
    // reach the same request rather than a second marketplace with a different
    // directory. The judging is `fetch::resolve`'s, in one place.
    assert_eq!(
        manage::parse(&argv(&[
            "plugin",
            "marketplace",
            "add",
            "zeroonething/ultraship.git/"
        ]))
        .expect("a pasted name parses"),
        request,
    );

    // And the slash surface routes all three to that parse rather than opening the
    // bundle panel over a line that asked for something else. Without this the
    // composer form never reaches `manage` at all and the criterion is about a
    // door nobody can open.
    for line in [
        "plugin marketplace add zeroonething/ultraship",
        "plugin marketplace list",
        "plugin marketplace remove zeroonething/ultraship",
        // A bare `marketplace` too: refused by name in the parse, which is a
        // better answer than the list of bundles.
        "plugin marketplace",
        // The plural spelling io-cli already admits for this surface.
        "plugins marketplace list",
    ] {
        assert!(
            matches!(
                io_cli::commands::parse(line, &io_cli::keys::Keys::default(), &io_cli::theme::DARK),
                io_cli::commands::Action::Manage(_),
            ),
            "`/{line}` must reach `manage::parse`, or the slash form and `io …` can disagree",
        );
    }
}

/// **F1 — a marketplace verb writes no configuration at all.**
///
/// `Ok(None)` is what `plan` answers for a read, and adding a marketplace is not a
/// read — it is a change to the disk that is not a change to any file io-harness
/// layers. The alternative shapes are both worse and both named in `plan`'s own
/// docs: an empty `Plan` makes `configure::write` create a file and report a write
/// that never happened, and a second member on `Plan` is the second write path the
/// module forbids.
///
/// Sabotage: give the three verbs a `Plan`. Under it `io plugin marketplace list`
/// creates the operator's user configuration file as a side effect of a question.
#[test]
fn f1_a_marketplace_verb_plans_no_configuration_write() {
    let root = tempfile::tempdir().expect("a temporary directory");
    for line in [
        "plugin marketplace add zeroonething/ultraship",
        "plugin marketplace list",
        "plugin marketplace remove zeroonething/ultraship",
    ] {
        let request = manage::parse(&manage::tokens(line)).expect(line);
        assert!(
            manage::plan(root.path(), &request)
                .expect("a marketplace verb plans")
                .is_none(),
            "{line} planned a write to a configuration file",
        );
    }
    // And nothing was created on the way past — the failure `Ok(None)` exists to
    // prevent leaves a file behind rather than an assertion.
    assert!(
        std::fs::read_dir(root.path())
            .expect("the root is readable")
            .next()
            .is_none(),
        "a marketplace verb wrote something into the discovery root",
    );
}

/// **F1 — the destination is one layout, spelled in one place.**
///
/// `fetch::at` builds `<home>/marketplaces/<owner>/<repo>` and
/// `marketplace::at` builds `<root>/<owner>/<repo>`; if the two ever disagree an
/// `add` writes to one path and a `list` and a `remove` look in another, and the
/// marketplace an operator just fetched is invisible to the surface that fetched
/// it.
#[test]
fn f1_the_clone_sits_under_owner_and_repo() {
    let root = PathBuf::from("/somewhere/marketplaces");
    assert_eq!(
        marketplace::at(&root, &named()),
        root.join("zeroonething").join("ultraship"),
    );

    // `fetch::at` answers `None` only where the operator has no home at all, which
    // is not something a test may create; where there is one, the two functions
    // must agree component for component.
    if let (Some(home), Some(fetched)) = (io_cli::home::marketplaces(), io_cli::fetch::at(&named()))
    {
        assert_eq!(fetched, marketplace::at(&home, &named()));
        assert!(
            fetched.ends_with("marketplaces/zeroonething/ultraship"),
            "the clone is two levels under the marketplaces directory: {}",
            fetched.display(),
        );
    }
}

/// **F1's named sabotage — neither door may read a marketplace name of its own.**
///
/// A byte comparison of two results cannot see this: two branches kept in step by
/// hand pass it until the release where one of them gains a flag. So the property
/// is asserted over the sources as text, which is the instrument `tests/manage.rs`
/// and `tests/servers.rs` already use on `src/main.rs` — nothing under `tests/`
/// links the driver.
///
/// Sabotage: resolve the name in `manage_main` and clone it there. Under it
/// `fetch::resolve` or `fetch::fetch` appears in `src/main.rs` and this fails
/// naming the door that grew the branch.
#[test]
fn f1_the_driver_resolves_no_name_and_clones_nothing_itself() {
    let manage = code_of("src/manage.rs");
    let driver = code_of("src/main.rs");
    let module = code_of("src/marketplace.rs");

    assert_eq!(
        manage.matches("fetch::resolve(").count(),
        1,
        "a marketplace name is judged in exactly one place, and it is the parse",
    );
    assert_eq!(
        driver.matches("fetch::resolve(").count(),
        0,
        "src/main.rs judges a marketplace name itself, so the argv door has grown a \
         reading of its own and the two doors can disagree about what a name may contain",
    );
    assert_eq!(
        driver.matches("fetch::fetch(").count(),
        0,
        "src/main.rs clones a marketplace itself rather than through \
         `marketplace::add`, which is the branch F1 forbids",
    );
    assert_eq!(
        module.matches("fetch::fetch(").count(),
        1,
        "the fetch has one call site, so neither door can reach a different one",
    );

    // **The two verbs have different door counts, and the asymmetry is real rather
    // than an oversight.** The first draft of this gate asserted two for each, on
    // the assumption that the panel and the argument form were the two doors for
    // both. They are not:
    //
    // - `add` has **two**. The panel's "add a marketplace" row prefills the
    //   composer with `/plugin marketplace add ` rather than acting, because a
    //   name is free text and there is nothing to choose from — so that row
    //   arrives back through the typed door and adds no call site of its own.
    // - `remove` has **three**. The panel *can* offer a specific marketplace,
    //   because by then it is a row the operator picked, so it acts directly after
    //   the `store::LEAVE_IT` confirmation. That is a real third door.
    //
    // What the numbers guard is unchanged: every one of them goes through the
    // library rather than doing the work in the driver. A count one higher than
    // the door list below is a surface acting without the parse; a count of zero
    // is a door that cannot do it at all.
    for (verb, doors) in [("marketplace::add(", 2), ("marketplace::remove(", 3)] {
        assert_eq!(
            driver.matches(verb).count(),
            doors,
            "`{verb}` is called from {} places in src/main.rs, not {doors}. The doors \
             are: the typed slash form, the argument form, and — for `remove` only — \
             the panel's confirmed removal of a marketplace the operator picked. \
             Every one goes through the library.",
            driver.matches(verb).count(),
        );
    }
}

/// Every refusal names what was wrong and what is accepted instead.
///
/// The sub-verb list and the surface's own verb list are two sentences that both
/// have to know that `marketplace` exists — `src/manage.rs`'s own note records
/// that a stale refusal tells an operator a verb does not exist.
#[test]
fn a_marketplace_refusal_names_what_is_accepted() {
    for (line, expected) in [
        // The surface's verb list must carry the new verb.
        ("plugin wat", "marketplace"),
        ("plugin", "marketplace"),
        // The sub-verb's own list.
        ("plugin marketplace", "add <owner/repo>"),
        ("plugin marketplace wat", "add <owner/repo>"),
        // The name, judged by `fetch::resolve` and refused by name.
        (
            "plugin marketplace add https://github.com/a/b",
            "<owner>/<repo>",
        ),
        ("plugin marketplace add ../../etc", "<owner>/<repo>"),
        ("plugin marketplace add a/b/c", "<owner>/<repo>"),
        // A leading dash never even reaches `resolve`: the scan refuses it as a
        // flag io does not have, which is the earlier of the two guards against a
        // name that would become an option.
        ("plugin marketplace add -rf", "two dashes"),
        // A marketplace has no scope, because it is written into no file.
        ("plugin marketplace add a/b --scope user", "no flags at all"),
        // And the list verb takes nothing.
        ("plugin marketplace list extra", "takes no arguments"),
        // The surface's verb list carries the two verbs 0.29.0 adds as well. A
        // refusal that omits one tells an operator at a terminal that it does not
        // exist, which is the trip to the documentation these refusals exist to
        // save — `src/manage.rs`'s `verbs` says so in its own note.
        ("plugin wat", "install"),
        ("plugin wat", "search"),
        ("plugin", "search"),
        // And `search` says what it wants, since it is the one verb here whose
        // argument is neither a name nor a path.
        ("plugin search", "some text to look for"),
    ] {
        let refusal = manage::parse(&manage::tokens(line)).expect_err(line);
        assert!(
            refusal.contains(expected),
            "`{line}` was refused without naming `{expected}`: {refusal}",
        );
    }
}

// --- F2: what a marketplace holds, by the manifest's own names ----------------

/// The fixture both F2 tests read: two marketplaces, one of them its own bundle.
fn fixture(root: &Path) {
    let clone = root.join("zeroonething").join("ultraship");
    // A bundle whose directory and whose `name` are deliberately different. This
    // is F2's sabotage arm: returning the directory's own filename answers
    // `rust` here, and io-harness would namespace nothing by that word.
    manifest(
        &clone.join("plugins").join("rust"),
        "name = \"rust-review\"\ndescription = \"Everything our Rust reviews need.\"\n\
         version = \"1.2.0\"\n",
    );
    // A manifest that names nothing. Still a bundle, listed under its directory,
    // with the row saying the manifest did not name it.
    manifest(&clone.join("plugins").join("bare"), "version = \"0.1.0\"\n");
    // Not a bundle: no manifest.
    std::fs::create_dir_all(clone.join("plugins").join("docs")).expect("a plain directory");
    std::fs::write(clone.join("plugins").join("docs").join("README.md"), "hi").expect("a file");
    // A dotted directory carrying a manifest. Every clone has a `.git`, and a
    // walk that counted what is inside one would report a marketplace's own
    // metadata as a capability bundle.
    manifest(
        &clone.join(".git").join("hooks"),
        "name = \"not-a-bundle\"\n",
    );

    // A marketplace that IS one bundle — a plugin published as its own
    // repository. `pluginview::candidates` only ever looks at a directory's
    // children, so this is the shape a walk without the root check misses whole.
    manifest(
        &root.join("otherowner").join("single"),
        "name = \"single\"\n",
    );
}

/// **F2 — a marketplace's contents are the directories that carry a manifest, and
/// their own names.**
///
/// Sabotage: return the directory's own filename instead of the manifest's `name`
/// — the `rust` / `rust-review` assertion below fails, and it fails on the fact
/// that matters, because io-harness namespaces every contribution a bundle makes
/// by the manifest's name and never by the directory's.
#[test]
fn f2_a_marketplace_holds_the_directories_that_carry_a_manifest() {
    let dir = tempfile::tempdir().expect("a temporary directory");
    fixture(dir.path());

    let markets = marketplace::markets(dir.path());
    assert_eq!(
        markets.len(),
        2,
        "exactly two levels are a marketplace: {:?}",
        markets
            .iter()
            .map(marketplace::Market::name)
            .collect::<Vec<_>>(),
    );
    // Ordered by owner and then by repository, so the row an operator picked
    // yesterday is the row in the same place today.
    assert_eq!(markets[0].name(), "otherowner/single");
    assert_eq!(markets[1].name(), "zeroonething/ultraship");

    // The marketplace that is itself one bundle. Its own root is the bundle.
    assert_eq!(markets[0].bundles.len(), 1);
    assert_eq!(markets[0].bundles[0].dir, markets[0].root);
    assert_eq!(markets[0].bundles[0].label(), "single");
    assert_eq!(markets[0].held(), "1 bundle");

    let held = &markets[1];
    assert_eq!(
        held.bundles.len(),
        2,
        "a directory with no manifest is not a bundle, and a dotted directory is \
         never walked: {:?}",
        held.bundles
            .iter()
            .map(|b| b.dir.clone())
            .collect::<Vec<_>>(),
    );
    assert_eq!(held.held(), "2 bundles");
    assert!(
        held.bundles.iter().all(|bundle| !bundle
            .dir
            .components()
            .any(|part| part.as_os_str().to_string_lossy() == ".git")),
        "a clone's own `.git` was counted as holding a bundle",
    );

    let review = held
        .bundles
        .iter()
        .find(|bundle| bundle.dir.ends_with("rust"))
        .expect("the bundle in `plugins/rust`");
    assert_eq!(
        review.name.as_deref(),
        Some("rust-review"),
        "the manifest's `name`, unquoted — not the directory's own filename",
    );
    assert_eq!(review.label(), "rust-review");
    assert_eq!(
        review.description.as_deref(),
        Some("Everything our Rust reviews need."),
        "the manifest's `description`, unquoted",
    );
    assert_eq!(review.line(), "Everything our Rust reviews need.");

    let bare = held
        .bundles
        .iter()
        .find(|bundle| bundle.dir.ends_with("bare"))
        .expect("a manifest that names nothing is still a bundle");
    assert_eq!(bare.name, None);
    assert_eq!(
        bare.label(),
        "bare",
        "a nameless manifest is listed under its directory rather than dropped",
    );
    assert!(
        bare.line().contains("does not name it"),
        "the row must say the label is io-cli's guess: {}",
        bare.line(),
    );
}

/// **F2 — the rows draw the manifest's name and the count, in both glyph sets.**
///
/// The label is what an operator reads and what they will type; the count is the
/// answer to "what is in it" and is the one field on a marketplace row with no
/// other home, so it is unconditional and the path is what gives way.
#[test]
fn f2_the_rows_carry_the_manifest_name_and_the_count() {
    let dir = tempfile::tempdir().expect("a temporary directory");
    fixture(dir.path());
    let markets = marketplace::markets(dir.path());

    for glyphs in [&io_cli::glyphs::UNICODE, &io_cli::glyphs::ASCII] {
        let rows = marketplace::rows(&markets, 200, glyphs);
        assert_eq!(rows.len(), markets.len(), "{}", glyphs.name);
        assert_eq!(rows[1].label, "zeroonething/ultraship", "{}", glyphs.name);
        let detail = rows[1]
            .detail
            .clone()
            .expect("a marketplace row has detail");
        assert!(detail.starts_with("2 bundles"), "{}: {detail}", glyphs.name);

        // A narrow terminal drops the path and keeps the count, which is the
        // field with no other home. The picker's own budget: marker, label, gap.
        let narrow = marketplace::rows(&markets, 40, glyphs);
        let detail = narrow[1]
            .detail
            .clone()
            .expect("a marketplace row has detail");
        assert!(detail.starts_with("2 bundles"), "{}: {detail}", glyphs.name);
        assert!(
            narrow[1].label.chars().count() + detail.chars().count() + 4 <= 40,
            "{}: the row fits: {detail}",
            glyphs.name,
        );

        let bundles = marketplace::bundle_rows(&markets[1], 200, glyphs);
        let labels: Vec<&str> = bundles.iter().map(|row| row.label.as_str()).collect();
        assert!(
            labels.contains(&"rust-review"),
            "{}: the manifest's name is the label: {labels:?}",
            glyphs.name,
        );
        assert!(
            !labels.contains(&"rust"),
            "{}: the directory's own filename was drawn as the bundle's name: {labels:?}",
            glyphs.name,
        );
    }
}

// --- F3: removing a marketplace touches nothing that is declared ---------------

/// **F3 — a marketplace is removed, and removing it does not touch anything
/// installed from it.**
///
/// Two bundles are declared: one copied out of the marketplace into the workspace,
/// which is what installing one means, and one left inside the clone, which is what
/// declaring one in place means. After the removal the configuration file is
/// **byte-identical**, both entries are still declared, and the copied bundle is
/// still loaded.
///
/// Sabotage: remove the `[[plugin]]` entries alongside the clone. The byte
/// comparison fails first and names it; `discard` could not express that edit
/// anyway, because it holds no configuration and builds no `Edit`.
#[test]
fn f3_removing_a_marketplace_leaves_every_declaration_alone() {
    let work = tempfile::tempdir().expect("a workspace");
    let store = tempfile::tempdir().expect("a marketplaces directory");
    // Resolved, because a temporary directory on macOS is reached through `/var`
    // and canonicalises to `/private/var`: a fixture that declared the unresolved
    // path would be comparing two spellings of one directory.
    let root = std::fs::canonicalize(work.path()).expect("the workspace resolves");
    let store = std::fs::canonicalize(store.path()).expect("the store resolves");

    // Both bundles contribute something real, so that "it loaded" is a claim about
    // io-harness having accepted a bundle rather than about a manifest carrying a
    // name. A skills directory is the cheapest contribution a local-scope bundle
    // may make; it exists, because a bundle naming a directory that does not is a
    // different failure and would answer this test for the wrong reason.
    let clone = marketplace::at(&store, &named());
    let inside = clone.join("plugins").join("rust");
    manifest(&inside, "name = \"rust-review\"\nskills = \"skills\"\n");
    std::fs::create_dir_all(inside.join("skills")).expect("the skills directory");
    // Installed: the same bundle, copied out of the marketplace and into the
    // operator's own workspace. This is the one F3 says stays loaded.
    let installed = root.join("bundles").join("rust-review");
    manifest(
        &installed,
        "name = \"rust-review-copy\"\nskills = \"skills\"\n",
    );
    std::fs::create_dir_all(installed.join("skills")).expect("the skills directory");

    let file = root.join("io.local.toml");
    // `{:?}` on the rendered path, which is a TOML basic string with its
    // backslashes escaped — the same reason `pluginview::quoted` exists, and the
    // reason a Windows checkout does not write a file that no longer parses.
    let text = format!(
        "[[plugin]]\npath = {:?}\n\n[[plugin]]\npath = {:?}\n",
        installed.display().to_string(),
        inside.display().to_string(),
    );
    std::fs::write(&file, &text).expect("the configuration");

    let config = io_harness::Config::discover(&root).expect("the configuration loads");
    let view = io_cli::pluginview::view(&config);
    // By id and never by count: `Config::discover` layers the operator's own user
    // file over this one, so a developer whose `~/.io-cli` declares a bundle would
    // fail a length assertion for a reason that has nothing to do with the
    // criterion. `tests/plugins.rs` and `pluginview`'s own test read this the same
    // way and for the same reason.
    for id in ["rust-review", "rust-review-copy"] {
        assert!(
            view.plugins.iter().any(|listed| listed.id == id),
            "{id} did not load before the removal: {view:?}",
        );
    }

    // What the removal is about to cost, named before it happens.
    let depends = marketplace::dependents(&view, &clone);
    assert_eq!(
        depends.len(),
        1,
        "exactly the bundle declared inside the clone depends on it: {depends:?}",
    );
    let warned = marketplace::warning(&depends).expect("a dependent bundle is warned about");
    assert!(
        warned.contains("left exactly as they are"),
        "the warning must say the entries survive: {warned}",
    );
    assert!(
        marketplace::warning(&[]).is_none(),
        "an empty warning drawn as a row is a row an operator reads as a warning",
    );

    let outcome = marketplace::discard(&store, &named());
    assert_eq!(outcome.went, Went::Acted, "{}", outcome.said);
    assert!(!clone.exists(), "the clone is what goes");

    // **The assertion F3's sabotage dies on.** Not "the entries are still there"
    // read back through a parser that might normalise them — the same bytes.
    assert_eq!(
        std::fs::read_to_string(&file).expect("the configuration is still there"),
        text,
        "removing a marketplace rewrote the configuration file",
    );

    let after = io_cli::pluginview::view(
        &io_harness::Config::discover(&root).expect("the configuration still loads"),
    );
    assert!(
        after
            .plugins
            .iter()
            .any(|listed| listed.id == "rust-review-copy"),
        "a bundle installed out of a marketplace stops loading when the marketplace \
         goes: {after:?}",
    );
    // And the honest other half: an entry declared *inside* the clone is still
    // declared — the bytes above say so — and io-harness now reports it as
    // missing rather than silently forgetting it. It is a refusal, not a
    // deletion, which is the whole distinction F3 draws.
    assert!(
        after
            .refused
            .iter()
            .any(|refused| refused.path.starts_with(&clone)),
        "the entry declared inside the clone was dropped from the configuration \
         rather than reported: {after:?}",
    );
}

/// Removing something that is not here says so rather than reporting success.
///
/// "Removed" over a name that was never there tells an operator their typo
/// worked, and the next thing they do is look for a marketplace they think is
/// gone.
#[test]
fn f3_removing_a_marketplace_that_is_not_here_is_not_a_success() {
    let store = tempfile::tempdir().expect("a marketplaces directory");
    let outcome = marketplace::discard(store.path(), &named());
    assert_eq!(outcome.went, Went::Refused);
    assert!(
        outcome.said.contains("zeroonething/ultraship") && outcome.said.contains("list"),
        "the refusal names what was asked for and how to see what is here: {}",
        outcome.said,
    );
}

/// **`discard` cannot reach a configuration file, and that is where F3 lives.**
///
/// A gate over the module's own text, because the criterion is about something the
/// code must not be able to do rather than about an outcome: an `Edit`, a `Scope`
/// or a `configure::write` in this module would be the sabotage arm compiling.
#[test]
fn f3_the_module_builds_no_edit_and_names_no_scope() {
    let module = code_of("src/marketplace.rs");
    for forbidden in [
        "edit::Edit",
        "Edit::set",
        "Edit::remove",
        "configure::write",
        "Scope::",
    ] {
        assert!(
            !module.contains(forbidden),
            "src/marketplace.rs names `{forbidden}`. Removing a marketplace deletes a \
             clone and nothing else; a module that can build an edit is a module that \
             can take a `[[plugin]]` entry away with it.",
        );
    }
}

/// A fetch that ended without cloning is reported as what it was.
///
/// Three endings, three sentences, and the middle one is the trap: a marketplace
/// that was already here is what the operator asked for, so it must not be drawn
/// as a refusal and must not exit non-zero — while a machine with no git and a
/// clone git rejected both must.
#[test]
fn a_fetch_that_did_not_clone_says_which_of_the_three_it_was() {
    let dir = tempfile::tempdir().expect("a temporary directory");
    let clone = marketplace::at(dir.path(), &named());
    manifest(
        &clone.join("plugins").join("rust"),
        "name = \"rust-review\"\n",
    );

    let already = marketplace::told(&named(), &io_cli::fetch::Fetched::Already(clone.clone()));
    assert_eq!(
        already.went,
        Went::Already,
        "a marketplace that was already here is not a refusal: {}",
        already.said,
    );
    assert!(
        already.said.contains("1 bundle"),
        "the count is what an operator asks for next: {}",
        already.said,
    );

    let cloned = marketplace::told(&named(), &io_cli::fetch::Fetched::Cloned(clone));
    assert_eq!(cloned.went, Went::Acted);
    assert!(
        cloned.said.contains("zeroonething/ultraship"),
        "{}",
        cloned.said
    );

    let missing = marketplace::told(&named(), &io_cli::fetch::Fetched::NoGit);
    assert_eq!(missing.went, Went::Refused);
    assert!(
        missing.said.contains("git"),
        "the sentence is `Fetched`'s own, about the machine: {}",
        missing.said,
    );

    let failed = marketplace::told(
        &named(),
        &io_cli::fetch::Fetched::Failed {
            status: Some(128),
            stderr: "Cloning into 'x'...\nfatal: repository not found\n".to_string(),
        },
    );
    assert_eq!(failed.went, Went::Refused);
    assert!(
        failed.said.contains("repository not found"),
        "git's own last line, carried: {}",
        failed.said,
    );
}

// --- F4 and F5: a bundle by name, and a search across every marketplace --------

/// The fixture both read: two marketplaces, one name they both carry, and one
/// bundle only the second holds.
///
/// **`markets` sorts by owner, so `otherowner/mirror` is the first marketplace and
/// `zeroonething/ultraship` is the second.** Everything F5 has to find lives in the
/// second one and everything F4 installs by a bare name lives there too — which is
/// what makes "read only the first marketplace" and "resolve to the first match"
/// fail rather than pass by accident.
fn two(root: &Path) -> Vec<Market> {
    let mirror = root.join("otherowner").join("mirror");
    manifest(
        &mirror.join("plugins").join("shared"),
        "name = \"shared\"\ndescription = \"A bundle both marketplaces carry.\"\n",
    );
    // A marketplace that is itself one bundle, which is also what gives
    // `ultraship@ultraship` a repository name to be qualified by.
    let ultraship = root.join("zeroonething").join("ultraship");
    manifest(
        &ultraship,
        "name = \"ultraship\"\ndescription = \"Release choreography for io-cli.\"\n",
    );
    manifest(
        &ultraship.join("plugins").join("shared"),
        "name = \"shared\"\ndescription = \"The other copy.\"\n",
    );
    marketplace::markets(root)
}

/// **F4 — a bundle is installed by name, and `install` is `add`.**
///
/// The three spellings resolve to one directory and are written as one
/// `[[plugin]]` entry, through `pluginview::add` — the edit `/plugin add` already
/// had. A second writer for the named form is the thing this test would not catch
/// and `src/manage.rs`'s own rule forbids; what it does catch is the three
/// spellings drifting apart.
///
/// Sabotage: read the *shape* of the word — a `/`, a leading `.` — to tell a path
/// from a name. The `shared` directory below is a real bundle whose name two
/// marketplaces also carry, and under that rule it resolves to somebody else's
/// code.
#[test]
fn f4_a_bundle_is_installed_by_name_and_install_is_add() {
    let store = tempfile::tempdir().expect("a marketplaces directory");
    let work = tempfile::tempdir().expect("a workspace");
    let markets = two(store.path());

    // Compared against the value both must produce rather than one parse against
    // another: `parse(X) == parse(X)` is the vacuity `tests/manage.rs` records
    // having shipped, and two spellings of one verb is exactly where it hides.
    let expected = Request::Plugin(PluginVerb::Add {
        path: "ultraship".into(),
        scope: Scope::User,
    });
    for line in ["plugin add ultraship", "plugin install ultraship"] {
        assert_eq!(
            manage::parse(&manage::tokens(line)).expect(line),
            expected,
            "`/{line}` must be the same request as the other spelling, and this one",
        );
    }
    // And the composer routes both words, or the slash door never reaches the
    // parse at all and the criterion is about a door nobody can open.
    for line in ["plugin install ultraship", "plugin search release"] {
        assert!(
            matches!(
                io_cli::commands::parse(line, &io_cli::keys::Keys::default(), &io_cli::theme::DARK),
                io_cli::commands::Action::Manage(_),
            ),
            "`/{line}` must reach `manage::parse`, or the slash form and `io …` can disagree",
        );
    }

    // One directory, whichever way it is spelled: bare, by repository, and fully
    // qualified.
    let bundle = store.path().join("zeroonething").join("ultraship");
    let spellings = [
        "ultraship",
        "ultraship@ultraship",
        "ultraship@zeroonething/ultraship",
    ];
    for query in spellings {
        assert_eq!(
            marketplace::locate(&markets, query).expect(query),
            bundle,
            "`{query}` named a different directory",
        );
    }

    // And one entry. The bytes rather than the paths, because "the same file" is
    // the criterion and a path comparison would pass over two edits that render
    // differently.
    let written: Vec<String> = spellings
        .iter()
        .map(|query| {
            let dir = marketplace::locate(&markets, query).expect(query);
            let edit = io_cli::pluginview::add(&io_cli::pluginview::declared(work.path(), &dir));
            io_cli::edit::apply("", std::slice::from_ref(&edit)).expect("the entry is written")
        })
        .collect();
    assert!(
        written[0].contains(&format!("path = {:?}", bundle.display().to_string())),
        "the entry names the bundle inside the marketplace: {}",
        written[0],
    );
    assert_eq!(written[0], written[1], "`@<repo>` wrote a different entry");
    assert_eq!(
        written[0], written[2],
        "`@<owner>/<repo>` wrote a different entry",
    );

    // **The discrimination rule, both ways round.** A directory carrying a
    // manifest is a path — even when its own name is one two marketplaces carry,
    // which is the case a rule keyed on the word's shape gets wrong.
    let here = work.path().join("shared");
    manifest(&here, "name = \"shared\"\n");
    assert_eq!(
        marketplace::chosen(&here, || markets.clone(), "shared").expect("a real path is a path"),
        here,
        "a directory carrying a manifest was read as a name and resolved elsewhere",
    );

    // A word that is neither is refused as both, because io-cli genuinely does not
    // know which was meant and reporting one half hides the other.
    let plain = work.path().join("nope");
    std::fs::create_dir_all(&plain).expect("a directory that is not a bundle");
    let refusal = marketplace::chosen(&plain, || markets.clone(), "nope")
        .expect_err("neither reading of `nope` holds");
    assert!(
        refusal.contains(io_cli::pluginview::MANIFEST),
        "the path reading must still name the file it looked for: {refusal}",
    );
    assert!(
        refusal.contains("`shared@otherowner/mirror`"),
        "the name reading must say what IS here: {refusal}",
    );
}

/// **F4's named sabotage — a bare name two marketplaces carry is refused.**
///
/// Two marketplaces holding one name hold two different repositories' code.
/// Resolving to whichever the walk reached first installs something the operator
/// did not choose, silently, under a name they believed meant one thing.
///
/// Sabotage: take the first match. Then `locate` answers `Ok` here and this test
/// fails on the refusal it did not get.
#[test]
fn f4_a_name_two_marketplaces_carry_is_refused_by_both_spellings() {
    let store = tempfile::tempdir().expect("a marketplaces directory");
    let markets = two(store.path());

    let refusal =
        marketplace::locate(&markets, "shared").expect_err("two marketplaces carry `shared`");
    for spelling in [
        "`shared@otherowner/mirror`",
        "`shared@zeroonething/ultraship`",
    ] {
        assert!(
            refusal.contains(spelling),
            "the refusal must spell every way of choosing, or the operator has to go \
             and look them up: {refusal}",
        );
    }

    // Qualified, it resolves — and to the marketplace that was named rather than
    // to the first, which is the same sabotage read from the other side.
    assert_eq!(
        marketplace::locate(&markets, "shared@zeroonething/ultraship").expect("the qualifier"),
        store
            .path()
            .join("zeroonething")
            .join("ultraship")
            .join("plugins")
            .join("shared"),
    );
    assert_eq!(
        marketplace::locate(&markets, "shared@mirror").expect("the repository alone qualifies"),
        store
            .path()
            .join("otherowner")
            .join("mirror")
            .join("plugins")
            .join("shared"),
    );

    // A name nothing holds says what is here rather than "not found": a bare
    // refusal over clones fetched weeks ago sends an operator into each of them.
    let missing = marketplace::locate(&markets, "rust-review").expect_err("nothing is called that");
    assert!(
        missing.contains("`ultraship@zeroonething/ultraship`"),
        "the refusal must list what is available, qualified: {missing}",
    );

    // And with no marketplace at all it says so rather than listing nothing.
    let empty = marketplace::locate(&[], "ultraship").expect_err("there is nothing here");
    assert!(
        empty.contains("plugin marketplace add"),
        "the refusal must say how to get one: {empty}",
    );
}

/// **F5 — `plugin search` reads across every added marketplace.**
///
/// Sabotage: search only the first marketplace. `choreography` is in the second
/// one and in no other, so the first assertion below fails on an empty result.
/// Second sabotage, the quieter one: search names and not descriptions — the same
/// assertion fails, because that word is only in a description.
#[test]
fn f5_search_reads_across_every_added_marketplace() {
    let store = tempfile::tempdir().expect("a marketplaces directory");
    let markets = two(store.path());
    assert_eq!(
        markets[0].name(),
        "otherowner/mirror",
        "the ordering this test's sabotage turns on",
    );

    let hits = marketplace::matching(&markets, "choreography");
    assert_eq!(
        hits.len(),
        1,
        "a word in the second marketplace's description must be found: {hits:?}",
    );
    assert!(
        hits[0].starts_with("ultraship@zeroonething/ultraship"),
        "the hit names its marketplace, in the spelling `plugin add` takes: {}",
        hits[0],
    );
    // Folded on both sides: a description is a sentence somebody wrote and a name
    // is a lowercase identifier, and a case-sensitive search finds one or the
    // other and never both.
    assert_eq!(marketplace::matching(&markets, "CHOREOGRAPHY"), hits);

    // A name both marketplaces carry is two hits and not one: they are two
    // different bundles, and merging them is the ambiguity F4 refuses, hidden.
    let both = marketplace::matching(&markets, "shared");
    assert_eq!(both.len(), 2, "{both:?}");
    assert!(
        both[0].contains("otherowner/mirror") && both[1].contains("zeroonething/ultraship"),
        "each hit is named by the marketplace that holds it, in `markets` order: {both:?}",
    );

    assert!(
        marketplace::matching(&markets, "nothing-is-called-this").is_empty(),
        "a query nothing matches must answer nothing rather than everything",
    );
}
