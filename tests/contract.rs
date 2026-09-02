//! **F1 and F2** — what a session turn's contract carries, and what it must not.

use std::path::PathBuf;
use std::sync::{Arc, MutexGuard, OnceLock};
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
///
/// Delegated to [`support::env_lock`] rather than declared here: two different
/// mutexes in one binary exclude nothing from each other.
fn env_lock() -> MutexGuard<'static, ()> {
    support::env_lock()
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

/// As [`discovered`], with the configuration itself in the **user** scope.
///
/// io-harness 0.74.0 refuses `[[provider]]`, `[[mcp]]`, `[[lsp]]`, `[[hook]]`,
/// provider-executed `[web]` access, ten widening policy values and an absolute
/// `run.skills` from any file that lives inside a workspace — those files arrive
/// with a `git clone`, or sit in a root the run's own agent can write to.
/// [`EVERY_SECTION`] names four of those things, so it cannot be the project file
/// any more.
///
/// **Not `support::user_scope`**, and the difference is the `files` argument: a
/// contract reads `[instructions]` off disk, relative to the *discovery root*
/// (`config.rs:2673`), so the workspace still has to hold `GUIDE.md` while the
/// configuration lives outside it. `support::user_scope` hands back an empty
/// workspace by design.
///
/// The user file is deliberately in a second directory. A file that is both the
/// `IO_CONFIG` target and `root/io.toml` is a candidate twice — once as
/// `Scope::User` and once as `Scope::Project` — and the project read is the one
/// that refuses it.
///
/// The caller holds [`env_lock`] for the call and holds **both** returned
/// directories for as long as the configuration is used.
fn discovered_in_user_scope(
    config_toml: &str,
    files: &[(&str, &str)],
) -> (tempfile::TempDir, tempfile::TempDir, Config) {
    no_user_scope();
    let home = tempfile::tempdir().expect("a directory for the user-scope file");
    let dir = tempfile::tempdir().expect("a workspace");
    for (name, body) in files {
        std::fs::write(dir.path().join(name), body).expect("the fixture file");
    }
    let path = home.path().join("io.toml");
    std::fs::write(&path, config_toml).expect("the user-scope fixture file");

    std::env::set_var("IO_CONFIG", &path);
    let config = Config::discover(dir.path());
    std::env::remove_var("IO_CONFIG");

    let config = config.expect("the fixture configuration parses");
    (home, dir, config)
}

/// The operator's own home directory, for the length of one test.
///
/// **Every assertion about a contract's `skills` field needs this from 0.15.0**,
/// including the ones that assert the field is empty: `io_cli::home` reads `HOME`
/// (and `USERPROFILE`, which is where Windows keeps the same fact) at call time,
/// and a contract with no configured directory now carries `~/.io-cli/skills`
/// where that directory exists. Without a home of its own, `f2` would pass or
/// fail on whether the person running the suite happens to have made one.
///
/// The caller holds [`env_lock`] for the whole of it — the environment is
/// process-wide and these tests share a process — and the previous values go back
/// when the guard drops, so a test that does not take the lock still sees the
/// home it was started with.
struct HomeFixture {
    dir: tempfile::TempDir,
    previous: [(&'static str, Option<std::ffi::OsString>); 4],
}

impl HomeFixture {
    /// **The fixture names the home in force as well as the operator's home, and
    /// that is what `io` itself does.** `contract::default_skills` anchors on
    /// [`io_cli::home::in_force`] since 0.31.0, and in the binary
    /// `home::adopt()` runs immediately above the first `Config::discover` and
    /// sets `IO_CONFIG_HOME` to `~/.io-cli`. A fixture that set only `HOME` would
    /// therefore be testing a state the product never reaches — and, worse, would
    /// inherit whatever `no_user_scope` had already pointed the variable at, so
    /// the default this file asserts on would be a directory in some other
    /// fixture's temporary tree.
    ///
    /// `IO_CONFIG` names a file outright and beats `IO_CONFIG_HOME`, so an
    /// inherited one has to go or the fixture is not the fixture. Every variable
    /// touched here is restored on drop, this one included.
    fn new() -> Self {
        let dir = tempfile::tempdir().expect("a home directory");
        let home = dir.path().to_path_buf();
        let previous = ["HOME", "USERPROFILE", "IO_CONFIG_HOME", "IO_CONFIG"]
            .map(|var| (var, std::env::var_os(var)));
        std::env::set_var("HOME", &home);
        std::env::set_var("USERPROFILE", &home);
        std::env::set_var("IO_CONFIG_HOME", home.join(".io-cli"));
        std::env::remove_var("IO_CONFIG");
        Self { dir, previous }
    }

    fn path(&self) -> &std::path::Path {
        self.dir.path()
    }

    /// `~/.io-cli/skills`, made on disk.
    ///
    /// Made rather than merely named because `Skills::discover` fails a run on a
    /// directory that is not there, so `contract::default_skills` only offers one
    /// that is — a fixture that skipped the `mkdir` would be asserting the
    /// absence of the default rather than its presence.
    fn skills(&self) -> PathBuf {
        let dir = self.path().join(".io-cli").join("skills");
        std::fs::create_dir_all(&dir).expect("the default skills directory");
        dir
    }
}

impl Drop for HomeFixture {
    fn drop(&mut self) {
        for (var, value) in &self.previous {
            match value {
                Some(value) => std::env::set_var(var, value),
                None => std::env::remove_var(var),
            }
        }
    }
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
    // A home with no `.io-cli/skills` in it, so "unchanged" is a fact about
    // `contract::session` and not about the machine running the suite.
    let _home = HomeFixture::new();
    let (dir, config) = discovered(&[]);
    let root = dir.path().to_path_buf();

    let (answerer, _questions) = io_cli::intent::channel();
    let responder: Arc<dyn io_harness::Responder> = Arc::new(answerer);
    let built = session(
        "bring the docs up to date",
        root.clone(),
        &config,
        &config.plugins(),
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

/// **F1 — the effort level is read by each turn, never taken from the session.**
///
/// A driver-text gate, and it is here because there is no other instrument.
/// Nothing under `tests/` links `src/main.rs`, so the difference between a posture
/// and a one-shot — which is the whole of what `/effort` promises — lives in a
/// binding this suite cannot call. The sibling `fold_next` is deliberately a
/// `std::mem::take` two lines above it, so the wrong shape is not merely
/// imaginable here: it is written out, correctly, immediately adjacent.
///
/// Weak, and the only one available. Four gates of this kind already exist in this
/// file and in `tests/steer.rs` for the same reason.
///
/// Sabotage: change the argument to `std::mem::take(&mut effort)` — under which
/// only this test fails, and it fails by making every level last exactly one turn
/// while every other assertion about `/effort` stays green.
#[test]
fn f1_the_driver_reads_the_effort_level_rather_than_taking_it() {
    let driver = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("src/main.rs");
    let text = std::fs::read_to_string(driver).expect("the driver");
    // Line endings normalised: a Windows checkout has `\r\n`, and slicing on a
    // bare `\n` has now shipped a defect in a test twice — 0.19.0 and 0.23.0.
    let text = text.replace("\r\n", "\n");

    assert!(
        !text.contains("std::mem::take(&mut effort)"),
        "the effort level is a posture: taking it would spend the level on one \
         turn, which is what `fold_next` beside it is supposed to be alone in doing",
    );
    assert!(
        text.contains("io_cli::contract::buying(contract, effort)"),
        "the level reaches the turn through `contract::buying`, which is where \
         the decision is so that it can be asserted at all",
    );
}

/// **F4 — the routing disclosure has callers, and `describe` has one at all.**
///
/// The third driver-text gate, and it exists because tracing callers found this
/// release about to ship `routing::describe` with none — a public function
/// reachable from no keystroke, no subcommand and no event arm, which is exactly
/// what 0.20.0 shipped seven of behind 1,077 passing tests and what this release's
/// own `risks` calls this codebase's proven blind spot. `tests/routing.rs` covered
/// the function thoroughly and could not see that nothing called it.
///
/// Three call sites, because there are three moments an operator can be in this
/// state: at session start, when they open the surface that edits the rules, and
/// when they type `/contain on` and enter the state mid-session. The last was
/// missing too.
#[test]
fn f4_the_routing_disclosure_is_reachable_from_the_driver() {
    let driver = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("src/main.rs");
    let text = std::fs::read_to_string(driver)
        .expect("the driver")
        .replace("\r\n", "\n");

    assert!(
        text.contains("io_cli::routing::describe("),
        "`routing::describe` says what the rules are and nothing calls it — a \
         function no operator can reach is not a surface, it is dead code with \
         tests",
    );
    assert_eq!(
        text.matches("io_cli::routing::inert_under_containment(")
            .count(),
        3,
        "the disclosure belongs at session start, on `/config` where the rules are \
         edited, and on `/contain on`, which is the keystroke that puts an operator \
         into the state it warns about",
    );
}

/// **F7 — the driver asks the turn's own kind and never infers it.**
///
/// The second driver-text gate, and the criterion's named sabotage is exactly what
/// it forbids: a run cancelled before its first step also has zero steps and was
/// not an answer, so a report derived from a step count would call an interrupted
/// turn a conversation.
///
/// Sabotage: report on `result.outcome`'s step count instead — under which only
/// this test fails, because `app::answered_said` would still be correct and simply
/// never called.
#[test]
fn f7_the_driver_reads_the_turn_kind_rather_than_counting_steps() {
    let driver = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("src/main.rs");
    let text = std::fs::read_to_string(driver)
        .expect("the driver")
        .replace("\r\n", "\n");

    assert!(
        text.contains("io_cli::app::answered_said(&result.kind, &result.outcome)"),
        "whether a turn was answered is a fact io-harness reports on \
         `TurnResult::kind` — read WITH the outcome, because a `Reply` also covers \
         a completion refused at the token ceiling — and never something to infer \
         from what the run did not do",
    );
}

/// **F1 — every turn that runs buys the reasoning the session asked for.**
///
/// `contract::buying`'s own note counted three `contract::session` callers that
/// build a contract nothing runs — and there are five sites, not four: the startup
/// reading, the two reporting pages, the turn, and `resume_pending`, which drives
/// real completions and was missed. The consequence was that `/effort high`
/// applied to every turn except the half of the work an operator came back to
/// `/resume` and finish, while the status line went on saying `effort high`.
///
/// **Counted, not `contains`.** The first version of this gate asked whether the
/// call appeared at all, which one site satisfies forever — so it could never have
/// caught the site that was missing. That is the vacuous-gate shape this suite has
/// now recorded three times.
#[test]
fn f1_every_turn_that_runs_applies_the_effort_level() {
    let driver = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("src/main.rs");
    let text = std::fs::read_to_string(driver)
        .expect("the driver")
        .replace("\r\n", "\n");

    assert_eq!(
        text.matches("io_cli::contract::buying(").count(),
        2,
        "two sites drive a turn — the ordinary one and the resumed one — and both \
         buy the level the session asked for. A third `contract::session` caller \
         that runs completions needs one too.",
    );
}

/// **F8 — `conversational = false` makes a greeting a run, and absent changes
/// nothing.**
///
/// Both files, because an assertion on one of them cannot tell a wired key from a
/// default. io-harness decides this for io-cli today —
/// `contract.conversational.unwrap_or(matches!(contract.verify,
/// Verification::None))` at `session.rs:1125-1127` — so with the key absent the
/// contract must carry `None` and leave that decision where it is, and with the key
/// present it must carry exactly what was written.
///
/// Sabotage: pass the key's value only when it is `true` — under which this fails
/// on the `false` file, which is the only file anybody would write the key into.
#[test]
fn f8_the_conversational_key_reaches_the_contract_in_both_directions() {
    let _guard = env_lock();
    let _home = HomeFixture::new();
    let (answerer, _questions) = io_cli::intent::channel();
    let responder: Arc<dyn io_harness::Responder> = Arc::new(answerer);

    let built = |body: &str| {
        let (dir, config) = discovered(&[("io.toml", body)]);
        let root = dir.path().to_path_buf();
        let contract = session(
            "say hello",
            root,
            &config,
            &config.plugins(),
            &Capabilities::default(),
            responder.clone(),
            None,
        );
        (dir, contract.conversational)
    };

    let (_empty, absent) = built("");
    assert_eq!(
        absent, None,
        "with no key, the decision stays io-harness's — it is on for an ungated \
         contract and io-cli must not restate that as a choice of its own",
    );

    let (_off, refused) = built("[app.io-cli]\nconversational = false\n");
    assert_eq!(
        refused,
        Some(false),
        "an operator who wants every prompt to open a run has asked for one",
    );

    let (_on, wanted) = built("[app.io-cli]\nconversational = true\n");
    assert_eq!(wanted, Some(true));
}

/// **F3 — a routing section reaches the contract, and no section leaves it alone.**
///
/// The rules themselves are asserted in `tests/routing.rs` against
/// `io_harness::Routing::model_for`, which is io-harness's own pure function and
/// the only implementation of the decision. What belongs here is the seam: that
/// what the operator wrote arrives on the contract, and that a file which names no
/// rule does not put a default `Routing` where there was nothing.
#[test]
fn f3_a_routing_section_reaches_the_contract_and_an_absent_one_leaves_it_unset() {
    let _guard = env_lock();
    let _home = HomeFixture::new();
    let (answerer, _questions) = io_cli::intent::channel();
    let responder: Arc<dyn io_harness::Responder> = Arc::new(answerer);

    let built = |body: &str| {
        let (dir, config) = discovered(&[("io.toml", body)]);
        let root = dir.path().to_path_buf();
        let contract = session(
            "bring the docs up to date",
            root,
            &config,
            &config.plugins(),
            &Capabilities::default(),
            responder.clone(),
            None,
        );
        (dir, contract.routing)
    };

    let (_none, unset) = built("");
    assert_eq!(unset, None, "a file with no rules asks for no routing");

    let (_empty, still_unset) = built("[app.io-cli.routing]\n");
    assert_eq!(
        still_unset, None,
        "a present but empty section names no rule, and a default `Routing` is a \
         value where there was absence",
    );

    let (_both, routed) = built(
        "[app.io-cli.routing.escalate_after]\nfailures = 3\nmodel = \"stronger\"\n\
         [app.io-cli.routing.downshift_under]\nbytes = 2000\nmodel = \"cheaper\"\n",
    );
    let routed = routed.expect("a section naming both rules routes");
    assert_eq!(routed.escalate_after, Some((3, "stronger".to_string())));
    assert_eq!(routed.downshift_under, Some((2000, "cheaper".to_string())));
}

/// **F7 — a turn that was answered is reported as answered, and a run is not.**
///
/// The `Run` half is the one that must not move: every existing report about an
/// ordinary turn stays byte-identical, so this arm answers `None` and the driver
/// records nothing.
///
/// Sabotage: report `Reply` for any run whose step count is zero — under which this
/// still passes and F7's live arm fails, because a turn cancelled before its first
/// step also has zero steps and was not an answer. That is why the kind is read
/// rather than inferred.
#[test]
fn f7_only_a_reply_is_reported_as_having_been_answered() {
    let finished = io_harness::RunOutcome::Finished { steps: 0 };

    assert!(
        io_cli::app::answered_said(&io_harness::TurnKind::Reply, &finished)
            .is_some_and(|said| said.contains("without opening a run")),
        "a question that was only a question has to say so, or it arrives as silence",
    );
    assert_eq!(
        io_cli::app::answered_said(&io_harness::TurnKind::Run, &finished),
        None,
        "an ordinary turn already accounts for itself",
    );
}

/// **F7 — a `Reply` that said nothing is not an answer.**
///
/// `TurnKind::Reply` carries a second meaning io-harness documents on the variant
/// itself (`session.rs:1440-1443`): a turn whose one completion crossed the token
/// ceiling and was **refused rather than served** is also a `Reply`, because no run
/// was opened either way. Reading the kind alone told an operator with
/// `[run] max_tokens` set that their refused question had been answered — with no
/// answer anywhere on screen and no mention of the budget.
///
/// Found by both adversarial reviewers independently, which is the strongest
/// signal this gate produces.
///
/// The guard is io-harness's own, copied rather than invented: `session.rs:1202`
/// emits its `Answered` event only for a `Reply` whose outcome is `Finished`.
///
/// Sabotage: drop the outcome from the match — under which only this test fails,
/// and it fails by reporting a message nobody wrote.
#[test]
fn f7_a_reply_refused_at_the_token_ceiling_is_not_an_answer() {
    assert_eq!(
        io_cli::app::answered_said(
            &io_harness::TurnKind::Reply,
            &io_harness::RunOutcome::CostBudgetExceeded { steps: 0 },
        ),
        None,
        "a completion refused at the budget was never served, so there is no \
         answer to announce",
    );
}

/// **F1 — a session that never says `/effort` sends no reasoning field.**
///
/// The absent case is not a fourth level. `TaskContract::effort` is an
/// `Option<Effort>`, and `None` sends the pre-0.31.0 request body byte for byte —
/// `openai_wire.rs:1443` and `anthropic.rs:1529`. So the whole of what must be
/// proven here is that [`io_cli::contract::buying`] leaves the contract *identical*
/// when nothing was asked for, which Debug equality states exactly.
///
/// Asserted through the same instrument as
/// [`f2_nothing_configured_is_the_contract_the_session_built_before`] and for the
/// same reason: a field set by accident is invisible to a test that only looks at
/// the field it meant to set.
///
/// Sabotage: give `buying` a default of `Effort::Medium` for the `None` case —
/// under which only this test fails, and it fails by buying reasoning on every
/// turn of every operator who never asked for any.
#[test]
fn f1_no_effort_asked_for_leaves_the_contract_byte_for_byte() {
    let root = std::path::PathBuf::from("/nowhere");
    let contract = TaskContract::workspace("a turn", root);
    let before = format!("{contract:?}");

    let after = io_cli::contract::buying(contract, None);

    assert_eq!(format!("{after:?}"), before);
    assert_eq!(after.effort, None);
}

/// **F1 — a level that was asked for is on the contract, and it is the one asked
/// for.**
///
/// All three levels rather than one, because the mapping is a place a swap is
/// invisible: `Effort` is `Ord`, so `Low` and `High` transposed would still be a
/// valid contract and would still pass a test that only checked `is_some()`.
#[test]
fn f1_the_level_asked_for_is_the_level_the_turn_buys() {
    let root = std::path::PathBuf::from("/nowhere");
    for level in [
        io_harness::Effort::Low,
        io_harness::Effort::Medium,
        io_harness::Effort::High,
    ] {
        let contract =
            io_cli::contract::buying(TaskContract::workspace("a turn", root.clone()), Some(level));
        assert_eq!(contract.effort, Some(level));
    }
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
        &nothing().plugins(),
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
        &nothing().plugins(),
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
        &nothing().plugins(),
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
        &nothing().plugins(),
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
    // One home for both builds: the skills default is read out of the
    // environment, so a test that let it move between them would be comparing two
    // machines rather than two arms.
    let _guard = env_lock();
    let _home = HomeFixture::new();

    // (1) Same inputs, same contract — there is no containment to differ on.
    let (answerer, _questions) = io_cli::intent::channel();
    let responder: Arc<dyn io_harness::Responder> = Arc::new(answerer);
    let one = session(
        "a goal",
        root(),
        &nothing(),
        &nothing().plugins(),
        &Capabilities::default(),
        responder.clone(),
        None,
    );
    let two = session(
        "a goal",
        root(),
        &nothing(),
        &nothing().plugins(),
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
    // **The call sites are named rather than counted, because the property is "one
    // contract per turn", not "one mention of the builder in this file", and
    // 0.14.0 is where those two stopped being the same sentence.** Two sites read
    // a contract without running one: `/status` reports the configured rosters,
    // and the opening one puts the ceilings on the status line before the first
    // prompt. Both have to go through this builder rather than reading `Config`
    // again, because `Config::apply_to` is the only thing that can say what
    // `[[mcp]]` and `[[lsp]]` hold, and `contract::session` is the only call that
    // also merges the `[app.io-cli]` scope and resolves the step-cap precedence —
    // a second answer to either would drift the first time a layer moved.
    //
    // Counting to three instead would admit a genuine second ARM just as readily,
    // which is the failure this test exists to make unrepresentable. Each site is
    // bound to its own name and asserted once, so a fourth still fails and so does
    // a second turn.
    assert_eq!(
        text.matches("let contract = io_cli::contract::session(")
            .count(),
        1,
        "one contract is built per turn, not one per arm",
    );
    // TWO reading sites from 0.17.0, and the number is the point rather than a
    // tolerance: `/status` reports the configured rosters and `/context` reports
    // the window the next turn would run under. Both READ a contract and neither
    // takes a turn with it, which is why they share a binding name that is not
    // `contract` — the assertion above finds the turn's builder by that name, and
    // a page that bound it would hand that assertion the wrong argument list.
    assert_eq!(
        text.matches("let reading = io_cli::contract::session(")
            .count(),
        2,
        "`/status` and `/context` each read one contract to report with, and neither builds a turn",
    );
    // **A third kind of site, added in 0.23.0, and it is neither of the two
    // above.** `resume_pending` takes a run — so it is not a reading site — but
    // it does not build a turn either: it continues one io-harness already has,
    // from the step it stopped at. It goes through this builder for the same
    // reason the reading sites do: `contract::session` is the only call that
    // merges the `[app.io-cli]` scope and resolves the step-cap precedence, and a
    // resumed run that quietly dropped the MCP servers or the roster would behave
    // differently after the pause than before it, which is worse than not
    // resuming at all.
    //
    // Named and asserted once rather than folded into the count above, exactly as
    // that assertion's own comment prescribes: counting to two there would admit a
    // genuine second turn arm, which is the failure it exists to make
    // unrepresentable.
    assert_eq!(
        text.matches("let continuing = io_cli::contract::session(")
            .count(),
        1,
        "one contract is rebuilt for a resume, and only `resume_pending` rebuilds one",
    );
    assert_eq!(
        text.matches("let opening = io_cli::contract::session(")
            .count(),
        1,
        "the session reads one contract at startup to put the ceilings on the line, and runs none",
    );
    // **Five since 0.23.0, and the fifth is the resume.** The total is checked
    // beside the four named counts rather than instead of them, so a site that
    // arrived without a name of its own still fails here even though every named
    // assertion above would pass. That is the whole value of keeping both: the
    // named counts say what each site is, and this one says nothing else exists.
    assert_eq!(
        text.matches("io_cli::contract::session(").count(),
        5,
        "the turn's contract, the resume's, the startup reading and the two reporting pages are \
         the only five, so a sixth is a new arm",
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
    // `_steered` from 0.17.0. io-harness 0.67.0 opened the two entry points that
    // take the caller's contract AND a steer inbox on one call — until then a
    // turn could carry a contract or be steered and never both, which is why
    // 0.11.0 dropped steering. The argument lists are otherwise identical, so
    // what this still asserts is what it always did: one contract, both arms.
    assert!(
        squashed.contains("session.turn_contained_bounded_steered(&contract,"),
        "the contained arm is handed the contract",
    );
    assert!(
        squashed.contains("session.turn_bounded_steered(&contract,"),
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
        &nothing().plugins(),
        &Capabilities::default(),
        responder.clone(),
        None,
    );
    assert!(built.max_steps >= 1_000, "{}", built.max_steps);

    // And the operator can still say otherwise, in either direction — through
    // `[run] max_steps`, which is the only spelling left. `[app.io-cli]
    // max_steps` was removed in 0.16.0; the floor is applied BEFORE
    // `Config::apply_to` precisely so a number the operator wrote beats it.
    let file = io_harness::Config::from_toml("[run]\nmax_steps = 25\n").unwrap();
    let asked = session(
        "bring the docs up to date",
        root(),
        &file,
        &file.plugins(),
        &Capabilities::default(),
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
    let (_home, dir, config) = discovered_in_user_scope(EVERY_SECTION, &[("GUIDE.md", GUIDE)]);

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
        &config.plugins(),
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
        "[browser] — absent in both, because the fixture declares no `[browser]` \
         table at all",
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
    let (_home, dir, config) = discovered_in_user_scope(EVERY_SECTION, &[("GUIDE.md", GUIDE)]);

    let (answerer, _questions) = io_cli::intent::channel();
    let contract = session(
        "make the suite pass",
        dir.path().to_path_buf(),
        &config,
        &config.plugins(),
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
        &config.plugins(),
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
        &empty.plugins(),
        &Capabilities::default(),
        responder.clone(),
        None,
    );
    assert_eq!(
        unasked.max_steps,
        io_cli::contract::MAX_STEPS,
        "a file that says nothing gets io-cli's floor, not the harness's twelve",
    );

    // **There is no longer a second spelling, and that is this release's point.**
    // `[app.io-cli] max_steps` was removed in 0.16.0, so `[run] max_steps` is
    // the whole answer and there is no more specific scope to beat it. Two
    // spellings for one number is what the removal ends.
    let only = session(
        "a goal",
        written.path().to_path_buf(),
        &config,
        &config.plugins(),
        &Capabilities::default(),
        responder,
        None,
    );
    assert_eq!(
        only.max_steps, 20,
        "`[run] max_steps` is the only spelling left and it beats io-cli's floor",
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
    let (_home, dir, config) = discovered_in_user_scope(
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
        &[],
    );

    let (stored, complaint) = io_cli::settings::stored(&config);
    assert!(complaint.is_none(), "{complaint:?}");
    let caps = Capabilities::stored(stored.as_ref());

    let (answerer, _questions) = io_cli::intent::channel();
    let contract = session(
        "a goal",
        dir.path().to_path_buf(),
        &config,
        &config.plugins(),
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
    let notices = server_notices(&config, &config.plugins(), &caps);
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
    //
    // User-scoped like the fixture above, and for the same reason: `[[mcp]]` is
    // one of the sections 0.74.0 refuses from a file that lives in a workspace, so
    // "one scope" can only mean the operator's own file. There is no longer a
    // project scope for a server to be alone in.
    let (_quiet_home, quiet, one_scope) = discovered_in_user_scope(
        "[[mcp]]\nid = \"only\"\ntransport = \"stdio\"\ncommand = \"only\"\n",
        &[],
    );
    let _ = &quiet;
    assert!(
        server_notices(&one_scope, &one_scope.plugins(), &Capabilities::default()).is_empty(),
        "a notice about a duplicate nobody wrote is noise that teaches operators \
         to stop reading the start-up lines",
    );
}

/// **F7 — a tilde is a home directory, never a directory named `~`.**
///
/// io-harness substitutes `${env:…}` and `${file:…}` and nothing else — there is
/// no tilde branch anywhere in `io-harness-0.66.0/src/config.rs` — so a `~` an
/// operator writes in `[run] skills` reaches `Skills::discover` verbatim and the
/// harness looks inside a directory whose name is one character long. The
/// operator's skills sit exactly where they said they would, and the session
/// lists none of them.
///
/// The fixture goes through `Config::discover` and not `Config::from_toml`,
/// because `from_toml` parses at project scope and this is an assertion about
/// what discovery populates.
///
/// Sabotage: pass the tilde through — return the contract from `resolve_skills`
/// untouched — under which only this and the two below fail, and they fail on the
/// literal `~/notes` this asserts against.
#[test]
fn f7_a_tilde_in_run_skills_is_the_operators_home() {
    let _guard = env_lock();
    let home = HomeFixture::new();
    let (dir, config) = discovered(&[("io.toml", "[run]\nskills = \"~/notes\"\n")]);

    let (answerer, _questions) = io_cli::intent::channel();
    let contract = session(
        "read the notes",
        dir.path().to_path_buf(),
        &config,
        &config.plugins(),
        &Capabilities::default(),
        Arc::new(answerer),
        None,
    );

    let carried = contract.skills.clone().expect("a skills directory");
    assert_eq!(
        carried,
        home.path().join("notes"),
        "`[run] skills` reaches the turn as a directory that exists",
    );
    // The inverse of the sabotage, said outright: the thing that must not survive
    // is the character itself, whatever else the path turns out to be.
    assert!(
        carried.is_absolute() && !carried.starts_with("~"),
        "`{}` is what `Skills::discover` would be handed",
        carried.display(),
    );
}

/// **F7 — and the same for io-cli's own table, which is applied later.**
///
/// `[app.io-cli] skills` is set after `Config::apply_to` has had its say, so an
/// expansion written into `configured` alone would leave this one literal. This
/// is the second key the single expansion point exists for, and the assertion is
/// also the precedence one: the narrower table wins.
///
/// Sabotage: expand only inside `configured` — under which this fails and
/// `f7_a_tilde_in_run_skills_is_the_operators_home` passes, which is the shape of
/// a half-applied rule.
#[test]
fn f7_a_tilde_in_the_app_table_is_the_operators_home_and_beats_run() {
    let _guard = env_lock();
    let home = HomeFixture::new();
    let (dir, config) = discovered(&[("io.toml", "[run]\nskills = \"~/from-run\"\n")]);

    let settings: CliSettings =
        toml::from_str("skills = \"~/from-app\"\n").expect("`[app.io-cli]` parses");
    let caps = Capabilities::stored(Some(&settings));

    let (answerer, _questions) = io_cli::intent::channel();
    let contract = session(
        "read the notes",
        dir.path().to_path_buf(),
        &config,
        &config.plugins(),
        &caps,
        Arc::new(answerer),
        None,
    );

    assert_eq!(
        contract.skills,
        Some(home.path().join("from-app")),
        "the narrower table wins, and it wins expanded",
    );
}

/// **F7 — with neither key set, skills live in io-cli's own home.**
///
/// The default an operator gets for making the directory and writing a file in
/// it: no configuration, no path to type, and both arms take it because it is
/// applied in `contract::configured`, which `io exec` builds from too.
///
/// Sabotage: drop the `or_else(default_skills)` — under which only this fails, on
/// a contract carrying no directory while `~/.io-cli/skills` holds skills nobody
/// will be told about.
#[test]
fn f7_skills_default_to_io_clis_own_home() {
    let _guard = env_lock();
    let home = HomeFixture::new();
    let skills = home.skills();
    let (dir, config) = discovered(&[]);

    let (answerer, _questions) = io_cli::intent::channel();
    let contract = session(
        "read the notes",
        dir.path().to_path_buf(),
        &config,
        &config.plugins(),
        &Capabilities::default(),
        Arc::new(answerer),
        None,
    );
    assert_eq!(contract.skills, Some(skills.clone()), "the session arm");

    let headless = io_cli::contract::configured(
        "read the notes",
        dir.path().to_path_buf(),
        &config,
        &config.plugins(),
    );
    assert_eq!(
        headless.skills,
        Some(skills),
        "and `io exec`, which is built from the same half",
    );
}

/// **F7's other half — the default is offered, never imposed.**
///
/// `Skills::discover` does **not** return early on a directory that is not there:
/// it returns `Error::Config("skills directory … does not exist")`, and
/// `TaskContract::discover_skills` propagates it at run start, before the first
/// completion. A default that named `~/.io-cli/skills` unconditionally would
/// therefore fail every turn for every operator who has never made one — a
/// feature that breaks the product for everybody who did not ask for it.
///
/// Sabotage: drop the `is_dir()` test in `contract::default_skills` — under which
/// this fails and `f2_nothing_configured_is_the_contract_the_session_built_before`
/// fails with it, on a contract that no longer matches the one the session built
/// before this release.
#[test]
fn a_home_with_no_skills_directory_is_the_contract_of_the_release_before() {
    let _guard = env_lock();
    let _home = HomeFixture::new();
    let (dir, config) = discovered(&[]);

    let (answerer, _questions) = io_cli::intent::channel();
    let contract = session(
        "read the notes",
        dir.path().to_path_buf(),
        &config,
        &config.plugins(),
        &Capabilities::default(),
        Arc::new(answerer),
        None,
    );

    assert_eq!(
        contract.skills, None,
        "an operator who never made the directory must not have their run refused \
         by a default they did not ask for",
    );
}

/// **F11 — the three ceilings with no other home, on both arms.**
///
/// `max_parallel_reads`, `spawn_background_after` and `detached_spawns` are
/// `TaskContract` fields with **no io-harness configuration key at all** —
/// `RunSection` carries thirteen and none of them is these — so io-cli names them
/// under its own `[app.io-cli]`. A contract field with no surface is a field
/// nobody sets.
///
/// They are applied in `contract::configured` rather than beside the other
/// `[app.io-cli]` keys, and the placement IS the criterion: `configured` is the
/// half a session turn and an `io exec` run share, while `session` is the
/// session's alone. A ceiling applied in `session` would bound a terminal and
/// leave CI running on the defaults, which is the 0.14.0 asymmetry this product
/// already deleted once.
///
/// Sabotage: move the `ceilings` call from `configured` into `session` — under
/// which the interactive arm still passes and the headless arm fails, which is
/// exactly the shape of the bug.
#[test]
fn f11_the_three_ceilings_reach_a_session_turn_and_io_exec_alike() {
    let toml = "\
[app.io-cli]
max_parallel_reads = 3
spawn_background_after_secs = 45
detached_spawns = false
";
    let config = io_harness::Config::from_toml(toml).expect("the fixture parses");

    // The headless arm builds from `configured` and nothing else.
    let headless = io_cli::contract::configured("a goal", root(), &config, &config.plugins());
    assert_eq!(
        headless.max_parallel_reads, 3,
        "io exec: max_parallel_reads"
    );
    assert_eq!(
        headless.spawn_background_after,
        Some(std::time::Duration::from_secs(45)),
        "io exec: spawn_background_after",
    );
    assert!(!headless.detached_spawns, "io exec: detached_spawns");

    // And the session arm, which wraps it.
    let (answerer, _questions) = io_cli::intent::channel();
    let responder: Arc<dyn io_harness::Responder> = Arc::new(answerer);
    let interactive = session(
        "a goal",
        root(),
        &config,
        &config.plugins(),
        &Capabilities::default(),
        responder,
        None,
    );
    assert_eq!(
        interactive.max_parallel_reads, 3,
        "session: max_parallel_reads"
    );
    assert_eq!(
        interactive.spawn_background_after,
        Some(std::time::Duration::from_secs(45)),
        "session: spawn_background_after",
    );
    assert!(!interactive.detached_spawns, "session: detached_spawns");
}

/// **F11 — a file that names none of them reproduces io-harness's own defaults.**
///
/// Field for field, because the failure worth catching is io-cli writing a
/// default back explicitly: that turns an absence into a statement, and the next
/// time io-harness changes one of these it would change for everyone except the
/// people running this interface.
#[test]
fn f11_a_file_that_asks_for_nothing_leaves_every_default_alone() {
    let bare = io_harness::Config::from_toml("").unwrap();
    let built = io_cli::contract::configured("a goal", root(), &bare, &bare.plugins());
    let untouched = io_harness::TaskContract::workspace("a goal", root());

    assert_eq!(
        built.max_parallel_reads, untouched.max_parallel_reads,
        "max_parallel_reads moved without being asked (io-harness's own is 10)",
    );
    assert_eq!(
        built.spawn_background_after, untouched.spawn_background_after,
        "spawn_background_after moved without being asked",
    );
    assert_eq!(
        built.detached_spawns, untouched.detached_spawns,
        "detached_spawns moved without being asked",
    );
}

/// **F11 — `detached_spawns = true` is agreement, not a change.**
///
/// `without_detached_spawns` is the only lever io-harness offers and the default
/// is already true, so only the `false` arm may do anything. A build that called
/// a setter for `true` would be inventing a second way to say the default.
#[test]
fn f11_asking_for_the_default_explicitly_changes_nothing() {
    let agrees = io_harness::Config::from_toml("[app.io-cli]\ndetached_spawns = true\n").unwrap();
    let built = io_cli::contract::configured("a goal", root(), &agrees, &agrees.plugins());
    assert!(built.detached_spawns);
}

/// **N3 — 0.27.0 adds no configuration key, so an operator who runs none of it
/// gets the contract 0.26.0 built.**
///
/// Every surface this release adds is a *command*: `/store`, `/export`, `/undo`.
/// None of them is configured, none is read at startup, and none reaches
/// `contract::session` at all. So the strongest statement of N3 is that the key
/// catalogue did not move — a new key is the only way this release could have
/// changed what a turn is built from, and this is the one place that would show.
///
/// The number is written out rather than derived, which is the same choice
/// `tests/commands.rs` makes about the command inventory and for the same
/// reason: growing the settings surface should be a decision somebody records
/// here, not a line somebody adds elsewhere.
///
/// Sabotage: add a key to `CATALOGUE` — under which only this fails, and it
/// fails by saying a release that promised to add no configuration added one.
#[test]
fn n3_this_release_adds_no_configuration_key() {
    let before_0_27_0 = 37;
    assert_eq!(
        io_cli::configure::CATALOGUE.len(),
        before_0_27_0,
        "0.27.0 adds three commands and no keys; a different number here means a \
         release that promised an operator nothing would change gave them \
         something to configure",
    );
}

/// **F16 — the default skills directory sits under the home in force, beside the
/// memory note.**
///
/// A skill is something the operator wrote, so it belongs wherever they put the
/// rest of what they wrote: an `$IO_CONFIG_HOME` pointed somewhere else moved
/// `io.toml` and `IO.md` there, and a skills default that stayed with io-cli's own
/// default home would read a directory beside a configuration file this session is
/// not using.
///
/// **Both halves are asserted here, in one test, because that is the only thing
/// that keeps them together.** A test for the skills half alone permits
/// `contract::default_skills` and `memory::path` to answer about two different
/// directories, which is the state this release exists to end.
///
/// The fixture makes the *wrong* answer available on purpose: `HomeFixture`
/// creates `~/.io-cli/skills` under a home that is not the one in force, so a
/// regression to `home::path` fails loudly with a path rather than quietly with a
/// `None` that could mean anything.
///
/// Sabotage: put `home::path()` back in `contract::default_skills` — under which
/// this fails on the first assertion, naming the home it followed.
///
/// The lock is held for the whole of it. `IO_CONFIG_HOME` is left set on the way
/// out, the way `tests/memory.rs` leaves it: the directory it names is gone by
/// then, so a later `Config::discover` finds no user scope, which is what every
/// other fixture in this file wants anyway.
#[test]
fn f16_the_skills_default_follows_the_home_in_force() {
    let _guard = env_lock();
    let user = io_harness::config::Scope::User;

    // io-cli's default home, with skills in it — the answer that must NOT win.
    let default_home = HomeFixture::new();
    let unread = default_home.skills();

    // And the home actually in force, which is somewhere else entirely.
    let dir = tempfile::tempdir().expect("the home in force");
    // `IO_CONFIG` names a file outright and beats `IO_CONFIG_HOME`, so a developer
    // who has one exported would otherwise decide this test.
    std::env::remove_var(io_harness::config::CONFIG_VAR);
    std::env::set_var(io_harness::config::CONFIG_HOME_VAR, dir.path());
    let skills = dir.path().join("skills");
    std::fs::create_dir_all(&skills).expect("the skills directory in force");

    assert_eq!(
        io_cli::contract::skills_dir(&nothing(), &Capabilities::default(), root()),
        Some(skills),
        "a contract that names no skills directory takes the one under the home in \
         force, not the {} nobody pointed this session at",
        unread.display(),
    );
    assert_eq!(
        io_cli::memory::path(&root(), user),
        Some(dir.path().join(io_cli::memory::file_name(user))),
        "and the operator's memory note answers about the same directory, which is \
         the point: authored content has one home, not two",
    );
}
