//! F8 and F9 — the `[[hook]]` tables: the scope that may declare one, the
//! configurations that get a `Hooks` at all, and the validation io-cli must not
//! reimplement.
//!
//! **io-cli has no opinion about what a hook is.** Every rule below —  which scope
//! may declare one, `on` against `at`, exactly one of `append` and `run`, whether
//! `tools` means anything without a lifecycle point — is enforced inside
//! `Config::discover`, and this file asserts io-harness's own sentences reach the
//! operator rather than asserting that io-cli reproduced them. A second copy of
//! those rules in this crate would be a second thing to keep true, and the release
//! it drifted in would be the one where a hook loads in io-cli and is refused by
//! the run.
//!
//! **What io-cli does own is the guard**, and it is the subject of F9. io-harness
//! disables read speculation on any run carrying a `Hooks` value at all — even one
//! holding no hooks — so whether that value is attached is a decision about every
//! operator who has never written a hook, which is nearly all of them.
//!
//! Fixtures that must be exact about the *absence* of a hook use
//! `Config::from_toml`, which reads only the text it is given.
//! `Config::discover` also reads a user-scope file where the machine has one.

use std::path::PathBuf;

use io_harness::config::{Config, LOCAL_FILE, PROJECT_FILE};
use io_harness::PLUGIN_FILE;

/// A temporary root, kept alive by the returned guard.
fn root() -> (tempfile::TempDir, PathBuf) {
    let dir = tempfile::tempdir().expect("a temporary directory");
    let root = dir.path().to_path_buf();
    (dir, root)
}

/// A root whose `file` holds `text`.
fn written(file: &str, text: &str) -> (tempfile::TempDir, PathBuf) {
    let (dir, root) = root();
    std::fs::write(root.join(file), text).expect("the configuration");
    (dir, root)
}

/// A hook that watches an event and runs a program. Valid in every scope but the
/// project one.
const EVENT_HOOK: &str = "[[hook]]\non = [\"finished\"]\nrun = [\"true\"]\n";

// ---------------------------------------------------------------------------
// F8 — the scope that may declare a hook
// ---------------------------------------------------------------------------

/// **F8.** A `[[hook]]` in the committed `io.toml` fails discovery, and io-cli's
/// refusal carries io-harness's own sentence and names the directory it came from.
///
/// Both halves matter and neither substitutes for the other. The sentence is the
/// only thing that tells an operator what to do — move the table to
/// `io.local.toml` — and the directory is the only thing that tells them *which*
/// checkout is refusing, which is the whole question when a session will not start
/// in one of six clones on the same machine.
///
/// Sabotage: reword `configure::refusal` to "your io.toml is invalid", the sort of
/// tidy-up that reads as an improvement in review. Under it only this test fails,
/// and what ships is an operator staring at a session that will not start, with no
/// key named, no rule explained and no file to open.
#[test]
fn f8_a_project_scoped_hook_is_refused_and_io_cli_names_the_rule_and_the_root() {
    let (_dir, root) = written(PROJECT_FILE, EVENT_HOOK);

    let error = Config::discover(&root)
        .err()
        .expect("a hook in the file a `git clone` delivers is refused");
    let said = io_cli::configure::refusal(&root, &error);

    // io-harness's own words, not a paraphrase of them.
    assert!(
        said.contains(
            "a project-scoped file may not declare hooks, because a hook runs or writes on \
             this machine"
        ),
        "io-cli reworded the refusal: {said}",
    );
    assert!(
        said.contains(LOCAL_FILE),
        "the operator is not told where the table may live: {said}",
    );
    assert!(
        said.contains(&root.display().to_string()),
        "the refusal names no directory, so an operator with six checkouts cannot tell \
         which one refused: {said}",
    );

    // The same table one file over is the operator's own machine talking, and it
    // loads — so the test above is about the scope rather than about hooks.
    let (_dir, local) = written(LOCAL_FILE, EVENT_HOOK);
    let config = Config::discover(&local).expect("a local hook is the operator's own");
    assert!(!config.hooks().is_empty());
}

// ---------------------------------------------------------------------------
// F9 — the guard, which is the most expensive line in this release to get wrong
// ---------------------------------------------------------------------------

/// **F9.** A configuration that declares no hook gets no `Hooks` at all — not an
/// empty one.
///
/// **The sabotage is returning `Some` unconditionally**, and it is the reason this
/// is the most important test in the file. Nothing fails under it. Every hook that
/// was ever written still runs, every test about hook behaviour stays green, and
/// `/config` looks identical. What changes is that io-harness stops speculating
/// reads on every run carrying a `Hooks` value — so the product gets slower for the
/// overwhelming majority of operators who have never written a hook, with nothing
/// on screen connecting the loss to this release and nothing in the suite to catch
/// it on the way out.
///
/// Both io-cli's callers are asserted, because the guard is made twice — once in
/// `contract::configured` for the lifecycle half and once in `contract::hooks` for
/// the fan-out's event half — and a fix to one is not a fix to the other.
#[test]
fn f9_a_configuration_with_no_hook_attaches_no_hooks_at_all() {
    let (_dir, root) = root();

    // The text-only reading, which no file on this machine can influence.
    let bare = Config::from_toml("").expect("an empty configuration");
    assert!(
        io_cli::contract::hooks(&bare, &root).is_none(),
        "a configuration with nothing in it was handed a Hooks",
    );
    assert!(
        io_cli::contract::configured("go", root.clone(), &bare)
            .tool_hooks
            .is_none(),
        "an empty configuration put a Hooks on the contract, which turns off read \
         speculation for every operator who has never written one",
    );

    // And a configuration that says plenty, none of it a hook — so `None` is not
    // `None` because there was no file to read.
    let (_dir, busy) = written(
        LOCAL_FILE,
        "[run]\nmax_steps = 40\n\n[[mcp]]\nid = \"docs\"\ntransport = \"stdio\"\n\
         command = \"mcp-docs\"\n",
    );
    let config = Config::discover(&busy).expect("the configuration loads");
    assert!(config.hooks().is_empty());
    assert!(
        io_cli::contract::hooks(&config, &busy).is_none(),
        "a configuration with servers and a step cap and no hook was handed a Hooks",
    );
    let contract = io_cli::contract::configured("go", busy.clone(), &config);
    assert!(
        contract.tool_hooks.is_none(),
        "the same, on the contract the turn carries",
    );
    assert_eq!(
        contract.max_steps, 40,
        "the fixture's other keys did not apply, so this proves nothing about a busy file",
    );
}

/// **F9, the other side.** A configuration that declares one gets it, on both
/// roads.
///
/// Without this the guard above is indistinguishable from `None` always — which
/// would pass every assertion in it, and would ship a product where `[[hook]]` is
/// a section io-harness validates, io-cli reports, and nothing ever runs.
///
/// Sabotage: invert the guard, or drop `with_tool_hooks` while keeping the
/// fan-out install. Under the second only the contract assertion here fails, and it
/// fails by accepting half the file: an `on = [...]` table runs and an
/// `at = "before_tool"` table is silently never consulted, so a hook written to
/// refuse a tool call refuses nothing.
#[test]
fn f9_a_declared_hook_reaches_both_the_contract_and_the_fan_out() {
    let (_dir, root) = written(
        LOCAL_FILE,
        "[[hook]]\nat = \"before_tool\"\ntools = [\"write_file\"]\nrun = [\"true\"]\n\n\
         [[hook]]\non = [\"finished\"]\nappend = \"audit.jsonl\"\n",
    );
    let config = Config::discover(&root).expect("the configuration loads");

    let hooks = io_cli::contract::hooks(&config, &root).expect("the file declares two hooks");
    assert!(!hooks.is_empty());

    let contract = io_cli::contract::configured("go", root.clone(), &config);
    assert!(
        contract.tool_hooks.is_some(),
        "the lifecycle half of the file is accepted and never consulted",
    );
}

/// **F9, through a bundle.** A hook a capability bundle contributes counts as a
/// declared hook, even though the configuration file itself declares none.
///
/// Sabotage: build the value from `config.hooks()` alone and drop the
/// `plugins().apply_to_hooks` merge. Under it only this test fails, and it fails
/// silently in the direction that matters least visibly: `/plugin` still draws the
/// bundle's `hooks` row, the manifest still declares them, and not one of them ever
/// runs — for a contribution kind whose entire purpose is to run programs.
#[test]
fn f9_a_hook_a_bundle_contributes_is_a_declared_hook() {
    let (_dir, root) = root();
    let bundle = root.join("runner");
    std::fs::create_dir_all(&bundle).expect("the bundle directory");
    std::fs::write(
        bundle.join(PLUGIN_FILE),
        "name = \"runner\"\n\n[[hook]]\nat = \"before_tool\"\nrun = [\"true\"]\n",
    )
    .expect("the manifest");
    std::fs::write(root.join(LOCAL_FILE), "[[plugin]]\npath = \"runner\"\n")
        .expect("the configuration");

    let config = Config::discover(&root).expect("the configuration loads");
    assert!(
        config.hooks().is_empty(),
        "the file itself declares a hook, so this test would pass without the merge",
    );
    assert!(
        config.plugins().dropped().is_empty(),
        "the bundle did not load: {:?}",
        config.plugins().dropped(),
    );

    assert!(
        io_cli::contract::hooks(&config, &root).is_some(),
        "the bundle's hook is declared and nothing in this session will run it",
    );
    assert!(
        io_cli::contract::configured("go", root.clone(), &config)
            .tool_hooks
            .is_some(),
        "the same, on the contract the turn carries",
    );
}

// ---------------------------------------------------------------------------
// Validation — io-cli's job is to carry the refusal, not to make it
// ---------------------------------------------------------------------------

/// A malformed `[[hook]]` is refused by io-harness, and io-cli adds no check and
/// no wording of its own.
///
/// The four shapes are the ones the table's keys make representable and its
/// meaning forbids. Each is checked in `Hook::check`, none of them is expressible
/// in the type — `deny_unknown_fields` cannot say "exactly one of these two" — and
/// every one of them would otherwise be a hook that loads, installs and never
/// fires, which is the failure this module can least afford.
///
/// Sabotage: give io-cli its own validator, however small — an `if
/// hook.at.is_some() && !hook.on.is_empty()` in `configure`, or a friendlier
/// message wrapped around the harness's. Under it only this test fails, and it
/// fails on the assertion that the refusal is io-harness's string and nothing
/// else. What it prevents is the release where io-cli's copy and io-harness's rule
/// disagree, and a hook the panel accepts is refused by the run that carries it.
#[test]
fn a_malformed_hook_is_refused_by_io_harness_and_io_cli_adds_no_check_of_its_own() {
    for (table, phrase) in [
        (
            "[[hook]]\non = [\"finished\"]\nat = \"before_tool\"\nrun = [\"true\"]\n",
            "a hook attaches to events or to a lifecycle point",
        ),
        (
            "[[hook]]\nat = \"before_tool\"\nappend = \"audit.jsonl\"\n",
            "a lifecycle hook decides whether a call happens, so it needs `run`",
        ),
        (
            "[[hook]]\non = [\"finished\"]\nappend = \"audit.jsonl\"\nrun = [\"true\"]\n",
            "a hook has one action",
        ),
        (
            "[[hook]]\non = [\"finished\"]\ntools = [\"write_file\"]\nrun = [\"true\"]\n",
            "`tools` filters a lifecycle hook, and this one has no `at`",
        ),
    ] {
        let (_dir, root) = written(LOCAL_FILE, table);
        let error = Config::discover(&root)
            .err()
            .unwrap_or_else(|| panic!("io-harness accepted a malformed hook: {table}"));

        assert!(
            error.to_string().contains(phrase),
            "refused for something other than the shape under test: {error}",
        );

        // **The whole of io-cli's contribution, asserted as an equality.** Anything
        // io-cli added — a rewording, a hint, a check of its own that fired first —
        // makes this fail, which is the point: the operator reads the words of
        // whoever enforced the rule.
        assert_eq!(
            io_cli::configure::refusal(&root, &error),
            format!(
                "the configuration in {} could not be read:\n{error}",
                root.display()
            ),
            "io-cli put something of its own between the operator and the refusal",
        );
    }
}

/// A hook that names an event io-harness does not emit is refused rather than
/// installed.
///
/// The one validation whose absence is pure silence: a misspelled tag deserializes
/// perfectly, installs perfectly, and matches no event for the life of the
/// session. There is nothing to notice.
///
/// Sabotage: drop the `EVENT_NAMES` loop from `Hook::check`. Under it only this
/// test fails, and what an operator gets is a hook they wrote, that `/config`
/// lists, that io-cli attaches to the fan-out, and that never runs once.
#[test]
fn a_hook_naming_an_event_that_does_not_exist_is_refused_rather_than_installed() {
    let (_dir, root) = written(LOCAL_FILE, "[[hook]]\non = [\"finsihed\"]\nrun = [\"true\"]\n");
    let error = Config::discover(&root)
        .err()
        .expect("a misspelled event name is a hook that would never fire");
    assert!(
        error.to_string().contains("is not an event this crate emits"),
        "{error}",
    );

    // The correctly spelled one loads, so the assertion above is about the
    // spelling rather than about the key.
    let (_dir, spelled) = written(LOCAL_FILE, EVENT_HOOK);
    let config = Config::discover(&spelled).expect("the configuration loads");
    assert!(io_cli::contract::hooks(&config, &spelled).is_some());
}
