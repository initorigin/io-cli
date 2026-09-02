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
//!
//! **io-harness 0.74.0 moved the line this file is about.** `[[hook]]` was refused
//! from `io.toml` and permitted in `io.local.toml`; it is now refused from both,
//! because `io.local.toml` is a path in the workspace root and a run's own agent
//! writes paths in the workspace root — one `write_file` of it declared an argv the
//! next `Config::discover` would run, outside the `Policy` and outside the sandbox.
//! The user scope is the one file no workspace can reach and is now the only place
//! a `[[hook]]` may be written, so every fixture below that declares one goes
//! through [`support::user_scope`] rather than writing into the root.

mod support;

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

/// A hook that watches an event and runs a program. Valid in the user scope, and
/// since io-harness 0.74.0 in no other.
const EVENT_HOOK: &str = "[[hook]]\non = [\"finished\"]\nrun = [\"true\"]\n";

// ---------------------------------------------------------------------------
// F8 — the scope that may declare a hook
// ---------------------------------------------------------------------------

/// **F8.** A `[[hook]]` in the committed `io.toml` fails discovery, and io-cli's
/// refusal carries io-harness's own sentence and names the directory it came from.
///
/// Both halves matter and neither substitutes for the other. The sentence is the
/// only thing that tells an operator what to do — move the table to the user-scope
/// file — and the directory is the only thing that tells them *which* checkout is
/// refusing, which is the whole question when a session will not start in one of
/// six clones on the same machine.
///
/// **The way out changed in io-harness 0.74.0 and the assertion changed with it.**
/// Until then the sentence named `io.local.toml`; that file is now held to the same
/// rule, so what the refusal names is `$IO_CONFIG` and its two fallbacks. The
/// substring is `IO_CONFIG` because the sentence is spelled per platform —
/// `%IO_CONFIG%` on Windows, `$IO_CONFIG` everywhere else — and both carry it.
///
/// Sabotage: reword `configure::refusal` to "your io.toml is invalid", the sort of
/// tidy-up that reads as an improvement in review. Under it only this test fails,
/// and what ships is an operator staring at a session that will not start, with no
/// key named, no rule explained and no file to open.
#[test]
fn f8_a_project_scoped_hook_is_refused_and_io_cli_names_the_rule_and_the_root() {
    // **The one test in this file that reads a scope it did not write, so it is the
    // one that has to hold the lock across the read.** `Config::discover` consults
    // `$IO_CONFIG` before it reaches the workspace, and the fixtures beside this one
    // point that variable at files that are *deliberately* unreadable — a malformed
    // `[[hook]]`, a misspelled event name. A discovery that landed inside one of
    // those windows would fail on the user scope and this test would assert the
    // wrong refusal, on CI, intermittently. Held for the whole body, and the
    // fixture form that does not take it again is the one called below.
    let _guard = support::env_lock();

    let (_dir, root) = written(PROJECT_FILE, EVENT_HOOK);

    let error =
        Config::discover(&root).expect_err("a hook in the file a `git clone` delivers is refused");
    let said = io_cli::configure::refusal(&root, &error);

    // io-harness's own words, not a paraphrase of them.
    assert!(
        said.contains(
            "a project-scoped file may not declare hooks — a hook runs an argv on this \
             machine, or appends to a path the file itself chose, on an event the file \
             itself picks"
        ),
        "io-cli reworded the refusal: {said}",
    );
    assert!(
        said.contains("IO_CONFIG"),
        "the operator is not told where the table may live: {said}",
    );
    assert!(
        said.contains(&root.display().to_string()),
        "the refusal names no directory, so an operator with six checkouts cannot tell \
         which one refused: {said}",
    );

    // **`io.local.toml` is refused too, and the refusal says a different thing.**
    // Both files are inside the workspace root and both are therefore untrusted;
    // what separates them is why, and an operator reading the wrong one of the two
    // sentences goes looking in the wrong place. Asserted here rather than left
    // implicit, because "the project scope is the strict one" is what this test
    // said for six releases and it is no longer true.
    let (_dir, local) = written(LOCAL_FILE, EVENT_HOOK);
    let error = Config::discover(&local)
        .expect_err("0.74.0 holds `io.local.toml` to the same rule: the agent can write it");
    let said = error.to_string();
    assert!(
        said.contains("a workspace-root `io.local.toml` may not declare hooks"),
        "the local-scope refusal is not io-harness's own: {said}",
    );

    // The same table in the one file no workspace can reach is the operator's own
    // machine talking, and it loads — so the two refusals above are about the scope
    // rather than about hooks.
    let scope = support::user_scope_locked(EVENT_HOOK, false);
    assert!(!scope.config.hooks().is_empty());
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
        io_cli::contract::hooks(&bare, &bare.plugins(), &root).is_none(),
        "a configuration with nothing in it was handed a Hooks",
    );
    assert!(
        io_cli::contract::configured("go", root.clone(), &bare, &bare.plugins())
            .tool_hooks
            .is_none(),
        "an empty configuration put a Hooks on the contract, which turns off read \
         speculation for every operator who has never written one",
    );

    // And a configuration that says plenty, none of it a hook — so `None` is not
    // `None` because there was no file to read. User-scoped, because `[[mcp]]` is
    // a refused section in every scope a workspace can supply since 0.74.0: an MCP
    // server is a command, an argv and an environment this process spawns at run
    // start.
    let scope = support::user_scope(
        "[run]\nmax_steps = 40\n\n[[mcp]]\nid = \"docs\"\ntransport = \"stdio\"\n\
         command = \"mcp-docs\"\n",
    );
    let busy = scope.root().to_path_buf();
    let config = &scope.config;
    assert!(config.hooks().is_empty());
    assert!(
        io_cli::contract::hooks(config, &config.plugins(), &busy).is_none(),
        "a configuration with servers and a step cap and no hook was handed a Hooks",
    );
    let contract = io_cli::contract::configured("go", busy.clone(), config, &config.plugins());
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
    let scope = support::user_scope(
        "[[hook]]\nat = \"before_tool\"\ntools = [\"write_file\"]\nrun = [\"true\"]\n\n\
         [[hook]]\non = [\"finished\"]\nappend = \"audit.jsonl\"\n",
    );
    let root = scope.root().to_path_buf();
    let config = &scope.config;

    let hooks = io_cli::contract::hooks(config, &config.plugins(), &root)
        .expect("the file declares two hooks");
    assert!(!hooks.is_empty());

    let contract = io_cli::contract::configured("go", root.clone(), config, &config.plugins());
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
/// **The bundle sits outside the workspace and is declared from the user scope,
/// which is what an installed bundle actually is.** io-harness 0.74.0 decides what
/// a manifest may contribute by *where the manifest is* rather than by which file
/// named it: a `plugin.toml` inside the workspace root may not carry a `[[hook]]`,
/// an `[[mcp]]` or a `[[bin]]` whatever declared it, because the run's own agent
/// can write that path. A bundle kept elsewhere and named from `$IO_CONFIG` is the
/// one arrangement that still contributes a hook — and it is where `plugin add`
/// puts one.
#[test]
fn f9_a_hook_a_bundle_contributes_is_a_declared_hook() {
    let (_bundles, bundles) = root();
    let bundle = bundles.join("runner");
    std::fs::create_dir_all(&bundle).expect("the bundle directory");
    std::fs::write(
        bundle.join(PLUGIN_FILE),
        "name = \"runner\"\n\n[[hook]]\nat = \"before_tool\"\nrun = [\"true\"]\n",
    )
    .expect("the manifest");

    // A TOML *literal* string, so a Windows path's backslashes are the path rather
    // than a run of escapes. A directory name cannot contain a `'` on the platforms
    // this ships to, and `tempfile` never puts one there.
    let scope = support::user_scope(&format!("[[plugin]]\npath = '{}'\n", bundle.display()));
    let root = scope.root().to_path_buf();
    let config = &scope.config;
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
        io_cli::contract::hooks(config, &config.plugins(), &root).is_some(),
        "the bundle's hook is declared and nothing in this session will run it",
    );
    assert!(
        io_cli::contract::configured("go", root.clone(), config, &config.plugins())
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
    // **User-scoped, or every arm below is refused for the wrong reason.**
    // `refuse_widening` runs against the raw table *before* anything deserializes,
    // so in any scope a workspace can supply the four malformed tables come back
    // with "may not declare hooks" and this test would pass while asserting nothing
    // about `Hook::check`. The lock is taken once and the fixture form that does
    // not take it again is used, because `std::sync::Mutex` is not reentrant.
    let _guard = support::env_lock();

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
        let (_dir, root) = root();
        let error = support::try_user_scope_locked(table, false)
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
    // One guard across both halves, and the forms that do not take it again: the
    // scope has to be the user's or the refusal is the section rule rather than the
    // event-name rule, and taking the lock twice on one thread is a deadlock.
    let _guard = support::env_lock();

    let error =
        support::try_user_scope_locked("[[hook]]\non = [\"finsihed\"]\nrun = [\"true\"]\n", false)
            .expect_err("a misspelled event name is a hook that would never fire");
    assert!(
        error
            .to_string()
            .contains("is not an event this crate emits"),
        "{error}",
    );

    // The correctly spelled one loads, so the assertion above is about the
    // spelling rather than about the key.
    let scope = support::user_scope_locked(EVENT_HOOK, false);
    let spelled = scope.root().to_path_buf();
    let config = &scope.config;
    assert!(io_cli::contract::hooks(config, &config.plugins(), &spelled).is_some());
}

// ---------------------------------------------------------------------------
// The empty-root guard — a `Hooks` is a write, so building one is a decision
// ---------------------------------------------------------------------------
//
// `io_harness::Hooks::new` creates every `append` path it is given, empty, as the
// value is constructed (`io-harness-0.69.0/src/hooks.rs`) — deliberately, so that
// "the filter matched nothing" stays distinguishable from "the hook was never
// installed". That makes *constructing* a `Hooks` a write to the filesystem, and
// it resolves a relative `append` against the directory it was handed. Handed
// `PathBuf::new()`, that directory is the process working directory.
//
// Which would be a curiosity except that `contract::server_notices` calls
// `configured(String::new(), PathBuf::new(), config)` at startup — with an empty
// root on purpose, because all it wants is the merged `[[mcp]]` and `[[lsp]]`
// lists off a throwaway contract that will never run a turn. So an operator with
// `[[hook]] append = "audit.jsonl"` in their configuration got a stray empty
// `audit.jsonl` dropped in whatever directory they happened to launch `io` from,
// every single start, in a session where no hook ran and no turn was taken.

/// A configuration whose one hook appends to a path relative to the root.
const APPENDING_HOOK: &str = "[[hook]]\non = [\"finished\"]\nappend = \"audit.jsonl\"\n";

/// A caller with no root gets no `Hooks`, on both roads, even though the
/// configuration declares one.
///
/// **The two assertions are the whole of the guard and neither covers the other**
/// — it is made once in `contract::hooks` for the fan-out's event half and once in
/// `contract::configured` for the lifecycle half, and `server_notices` goes through
/// the second. `f9_a_declared_hook_reaches_both_the_contract_and_the_fan_out`
/// above is the other side of this pair: the same file, with a root, gets its
/// hooks. So `None` here is about the root and nothing else.
///
/// **What this proves about the file, and what it does not.** The first half below
/// is the positive control: with a real root the `Hooks` is built and io-harness
/// creates `audit.jsonl` inside it, which is the mechanism the guard exists for and
/// which would otherwise be a claim in a comment. Given that, `None` from the two
/// rootless calls means no `Hooks` was constructed at all — and a file that is only
/// ever created by that constructor cannot appear when it is never run. The test
/// does not `set_current_dir` to watch the empty directory stay empty: the process
/// working directory is shared by every test in this binary and they run in
/// parallel, so a test that moved it would be reaching into the others.
///
/// Sabotage: delete `if root.as_os_str().is_empty() { return None; }` from
/// `contract::hooks`, or the `dir.as_os_str().is_empty()` arm from
/// `contract::configured`. Nothing else in the suite fails — every hook that was
/// ever written still runs, `/config` is identical, the contract that carries a
/// turn is unchanged — and what ships is io-cli littering an empty file into the
/// operator's home, their repository, or whatever directory they started in, once
/// per launch, for a hook that never fired.
#[test]
fn a_rootless_caller_builds_no_hooks_and_therefore_writes_no_append_file() {
    let scope = support::user_scope(APPENDING_HOOK);
    // The workspace the configuration was discovered against, which is deliberately
    // empty — so the `audit.jsonl` looked for below can only have been created by
    // the `Hooks` this test is about.
    let root = scope.root().to_path_buf();
    let config = &scope.config;

    // The positive control: this is what building a `Hooks` costs.
    assert!(
        !root.join("audit.jsonl").exists(),
        "the fixture already had the file, so its appearance below proves nothing",
    );
    assert!(
        io_cli::contract::hooks(config, &config.plugins(), &root).is_some(),
        "a declared hook with a root is a Hooks",
    );
    assert!(
        root.join("audit.jsonl").exists(),
        "io-harness no longer creates `append` paths as the value is built, which is \
         the entire reason the guard below exists — reread it before deleting either",
    );

    // And the same configuration with no root writes nothing, because it builds
    // nothing.
    assert!(
        io_cli::contract::hooks(config, &config.plugins(), std::path::Path::new("")).is_none(),
        "a rootless caller was handed a Hooks, which created `audit.jsonl` in \
         whatever directory `io` was launched from",
    );
    assert!(
        io_cli::contract::configured("", PathBuf::new(), config, &config.plugins())
            .tool_hooks
            .is_none(),
        "the road `server_notices` actually takes at startup: an empty root put a \
         Hooks on a throwaway contract that will never run a turn",
    );
}

/// `server_notices` — the caller the guard was written for — reads its lists
/// without leaving a file in the directory `io` was launched from.
///
/// Asserted through the real function rather than through the empty root it
/// happens to pass today, because the guard is only as good as the call that
/// reaches it: a `server_notices` that started resolving a root before asking
/// would pass every assertion in the test above and reintroduce the write at a
/// different address.
///
/// **The relative path is watched where it would actually land, and the process
/// working directory is not moved to do it.** `Hooks::new` resolves
/// `append = "audit.jsonl"` against the empty directory it is handed, which is the
/// bare relative path `audit.jsonl` — so a broken guard drops the file into
/// whatever directory the suite is running in, and that is where this looks. It is
/// recorded before and compared after rather than asserted absent outright, so a
/// checkout that happens to hold such a file is not a red test about the wrong
/// thing. Nothing is moved and nothing global is set, so this stays safe beside
/// every other test in this binary running at the same time.
///
/// The `[[mcp]]` entry is here so the call has something to find: a
/// `server_notices` that returned early on a configuration it had nothing to say
/// about would never reach `configured`, and this would be asserting that an
/// unentered branch writes no file.
///
/// Sabotage: either guard — `contract::hooks`'s or `contract::configured`'s — or a
/// `server_notices` that names a root. Under the second only this test fails, and
/// it fails by leaving the very file it is describing.
#[test]
fn server_notices_leaves_no_append_file_in_the_directory_io_was_launched_from() {
    // User-scoped on both counts: a `[[hook]]` and an `[[mcp]]` are each refused
    // from every file a workspace can supply since io-harness 0.74.0.
    let scope = support::user_scope(&format!(
        "{APPENDING_HOOK}\n[[mcp]]\nid = \"docs\"\ntransport = \"stdio\"\n\
         command = \"mcp-docs\"\n"
    ));
    let config = &scope.config;
    assert!(!config.hooks().is_empty(), "the fixture declares a hook");

    // Exactly what `Hooks::new` would `create` for this hook under an empty root.
    let stray = std::path::Path::new("audit.jsonl");
    let before = stray.exists();

    let _ = io_cli::contract::server_notices(
        config,
        &config.plugins(),
        &io_cli::contract::Capabilities::default(),
    );

    assert_eq!(
        stray.exists(),
        before,
        "reading the merged server lists at startup created {} in the working \
         directory — which is what every launch of `io` does, for a hook that \
         never fires",
        stray.display(),
    );
}
