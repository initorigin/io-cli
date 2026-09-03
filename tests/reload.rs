//! **F3 and F4** — the configuration is re-discovered once per turn, and a file
//! that stops parsing does not stop the session.
//!
//! Every fixture here is a real directory with a real `io.toml` and a real
//! `AGENTS.md` in it, driven through [`io_cli::reload::Configuration`]. Nothing
//! is stubbed: `[instructions]` is applied from a field only `Config::discover`
//! populates — `Config::from_toml` has no root to resolve a file name against —
//! so a fixture built in memory would assert an empty instruction list against a
//! repository that names one, and pass. `tests/contract.rs` records the same
//! constraint for the same reason.

use std::path::Path;
use std::sync::{Mutex, MutexGuard, OnceLock};

use io_cli::reload::Configuration;

/// Held by every test in this file.
///
/// `Config::discover` reads `$IO_CONFIG`, `$IO_CONFIG_HOME`, `$XDG_CONFIG_HOME`
/// and `$HOME` at call time, the environment is process-wide, and these tests
/// share a process — so two of them discovering at once would each be reading
/// the other's fixture. Intermittently, on a loaded machine, which is the most
/// expensive kind of failure to diagnose.
///
/// The guard is taken for the *whole* of each test rather than around one call,
/// because a `Configuration` re-discovers on every `refresh` and every one of
/// those reads the same variables.
///
/// This is the shape `tests/configure.rs`, `tests/contract.rs` and
/// `tests/wizard.rs` already keep. It is deliberately not a sleep or a retry:
/// `tests/timing.rs` forbids both, and neither would fix a race anyway.
fn env_lock() -> MutexGuard<'static, ()> {
    static LOCK: Mutex<()> = Mutex::new(());
    LOCK.lock().unwrap_or_else(|poisoned| poisoned.into_inner())
}

/// One empty user scope for this whole file, set once.
///
/// Pointed at a directory holding no file, the user scope is skipped and the
/// workspace's own `io.toml` is the whole of the configuration — otherwise every
/// assertion below would be about whatever the person running the suite happens
/// to have configured. `IO_CONFIG` names a file outright and would win over the
/// directory, so an inherited one has to go or the fixture is not the fixture.
///
/// Lifted from `tests/contract.rs::no_user_scope`, which is where this
/// repository settled the question.
fn no_user_scope() {
    static HOME: OnceLock<tempfile::TempDir> = OnceLock::new();
    HOME.get_or_init(|| {
        let dir = tempfile::tempdir().expect("a temporary directory");
        std::env::set_var("IO_CONFIG_HOME", dir.path());
        std::env::remove_var("IO_CONFIG");
        dir
    });
}

/// A workspace on disk, and the [`Configuration`] a driver would be holding after
/// startup had read it.
///
/// Built through `io_cli::configure::reload` — the same call the type under test
/// makes every turn — so the fixture and the behaviour cannot disagree about what
/// a successful read produces.
///
/// The caller holds [`env_lock`], and holds the returned directory for as long as
/// the configuration is used: `TempDir` deletes the workspace when it drops.
fn workspace(io_toml: &str, agents: &str) -> (tempfile::TempDir, Configuration) {
    no_user_scope();
    let dir = tempfile::tempdir().expect("a workspace");
    std::fs::write(dir.path().join("io.toml"), io_toml).expect("the project file");
    std::fs::write(dir.path().join("AGENTS.md"), agents).expect("the instruction file");
    // The root is taken before `dir` is moved into the tuple, which is the only
    // order that compiles.
    let root = dir.path().to_path_buf();
    let (config, settings) =
        io_cli::configure::reload(&root).expect("the fixture configuration parses");
    (dir, Configuration::new(root, config, settings))
}

/// Whether any composed instruction carries `line`.
///
/// io-harness words each instruction file as `Project instructions from
/// `AGENTS.md`:\n<text>` (`io-harness-0.76.0/src/config.rs:2822-2825`), so the
/// assertion is a containment and not an equality — this file is not the place
/// that pins the harness's wording.
fn instructs(held: &Configuration, line: &str) -> bool {
    held.config()
        .instructions()
        .iter()
        .any(|i| i.contains(line))
}

/// The theme the held pair reports, which is the `CliSettings` half.
///
/// Used as the witness throughout because it comes from `Config::app`, which is
/// the half a reload that refreshed only the `Config` would leave stale — the
/// failure `io_cli::configure::reload`'s own doc comment exists to prevent.
fn theme(held: &Configuration) -> Option<&str> {
    held.settings().and_then(|s| s.theme.as_deref())
}

/// A file rewritten behind the type's back.
///
/// **Deliberately not through `io_cli::configure::write`.** F3 asks that a change
/// made by *another process* reach the next turn, and the property that delivers
/// it is that `Configuration` holds no watcher, no invalidation hook and no
/// record of who wrote: `refresh` goes back to `Config::discover`, which reads
/// the bytes on disk. A test that changed the file through io-cli's own writer
/// would pass on an implementation that only ever noticed its own writes, which
/// is exactly the implementation F3 rules out.
fn behind_its_back(path: &Path, text: &str) {
    std::fs::write(path, text).expect("the fixture file is writable");
}

/// An unterminated table header — the shape an editor leaves halfway through a
/// save, and the case F4 exists for.
const BROKEN: &str = "[app.io-cli\ntheme = \"dark\"\n";

/// A *different* kind of refusal, so the "distinct failure" arm is comparing two
/// sentences the parser genuinely words differently rather than two positions in
/// the same one. A duplicate key and a missing bracket are different objections.
const BROKEN_OTHERWISE: &str = "[run]\nmax_steps = 10\nmax_steps = 11\n";

/// **F3.** A line written into an instruction file after the session started is
/// in the configuration the next turn is built from, with nothing restarted.
#[test]
fn f3_a_line_written_mid_session_reaches_the_next_turn() {
    let _guard = env_lock();
    let (dir, mut held) = workspace(
        "[app.io-cli]\ntheme = \"dark\"\n",
        "Prefer the smaller diff.\n",
    );

    // What the session started on.
    assert!(instructs(&held, "Prefer the smaller diff."));
    assert!(
        !instructs(&held, "Never touch the lockfile."),
        "the fixture must not already carry the line the test is about to write",
    );

    behind_its_back(
        &dir.path().join("AGENTS.md"),
        "Prefer the smaller diff.\nNever touch the lockfile.\n",
    );

    // The turn boundary, and nothing else. No restart, no re-launch, no call
    // into the writer.
    assert_eq!(
        held.refresh(),
        None,
        "a configuration that still parses has nothing to report",
    );
    assert!(
        instructs(&held, "Never touch the lockfile."),
        "the line written mid-session is not in the configuration the next turn \
         builds from; io-harness snapshots `[instructions]` inside \
         `Config::discover` and there is no other way to pick a change up",
    );
    assert!(
        instructs(&held, "Prefer the smaller diff."),
        "the re-read replaced the instructions rather than re-reading them",
    );
}

/// **F3, the other half.** A change to `io.toml` reaches *both* what the harness
/// decides and what io-cli's own section says, because they are re-read as a
/// pair.
#[test]
fn f3_the_cli_section_is_re_read_with_the_rest() {
    let _guard = env_lock();
    let (dir, mut held) = workspace("[app.io-cli]\ntheme = \"dark\"\n", "Anything.\n");
    assert_eq!(theme(&held), Some("dark"));

    behind_its_back(
        &dir.path().join("io.toml"),
        "[app.io-cli]\ntheme = \"light\"\n",
    );

    assert_eq!(held.refresh(), None);
    assert_eq!(
        theme(&held),
        Some("light"),
        "the `Config` was refreshed and `CliSettings` was not, so `/config` would \
         report a theme no turn is drawing with",
    );
}

/// **F4.** A configuration that stops parsing does not stop the session: the next
/// turn runs on the last one that discovered cleanly, and the operator is told
/// which file refused and what it said.
///
/// This is the arm the F4 sabotage has to fail. An implementation that propagated
/// the discovery error — dropping the held pair, or replacing it with an empty
/// configuration — passes every other test in this file and fails the three
/// assertions below.
#[test]
fn f4_a_file_that_stops_parsing_leaves_the_last_good_configuration_in_force() {
    let _guard = env_lock();
    let (dir, mut held) = workspace(
        "[app.io-cli]\ntheme = \"dark\"\n",
        "Prefer the smaller diff.\n",
    );

    behind_its_back(&dir.path().join("io.toml"), BROKEN);

    let refusal = held
        .refresh()
        .expect("a configuration that no longer parses is something to say");
    assert!(
        refusal.contains("io.toml"),
        "the operator has to be told WHICH file refused: {refusal}",
    );
    // And what it said, not only which file. The tail is taken after the name
    // rather than by comparing against the path, because macOS resolves a
    // temporary directory through a symlink and the two spellings would not
    // match.
    let (_, said) = refusal
        .rsplit_once("io.toml")
        .expect("the refusal names the file");
    assert!(
        !said.trim_start_matches(':').trim().is_empty(),
        "the refusal must carry what the file said, not only its name: {refusal}",
    );

    // The session continues, on the last configuration that read cleanly.
    assert_eq!(
        theme(&held),
        Some("dark"),
        "the held settings were surrendered to a file that no longer parses",
    );
    assert!(
        instructs(&held, "Prefer the smaller diff."),
        "the held configuration was surrendered to a file that no longer parses",
    );
}

/// **F4, the repair.** When the file parses again the new configuration is picked
/// up, with nothing further asked of anybody.
#[test]
fn f4_a_repaired_file_is_picked_up_with_nothing_asked() {
    let _guard = env_lock();
    let (dir, mut held) = workspace("[app.io-cli]\ntheme = \"dark\"\n", "Anything.\n");
    let io_toml = dir.path().join("io.toml");

    behind_its_back(&io_toml, BROKEN);
    assert!(held.refresh().is_some());

    behind_its_back(&io_toml, "[app.io-cli]\ntheme = \"light\"\n");
    assert_eq!(
        held.refresh(),
        None,
        "a repaired file is not an occasion for a notice",
    );
    assert_eq!(
        theme(&held),
        Some("light"),
        "the repaired file's content is what the next turn is built from",
    );
}

/// **F4, the reporting rule.** The same refusal is said once, and said again
/// after the file has been good in between.
///
/// The refresh happens at the top of every turn, so a file left broken for six
/// prompts would otherwise produce the same sentence six times — which is how a
/// product teaches operators to stop reading its notices.
#[test]
fn f4_a_refusal_is_reported_once_and_again_after_a_success() {
    let _guard = env_lock();
    let (dir, mut held) = workspace("[app.io-cli]\ntheme = \"dark\"\n", "Anything.\n");
    let io_toml = dir.path().join("io.toml");
    let good = "[app.io-cli]\ntheme = \"dark\"\n";

    behind_its_back(&io_toml, BROKEN);
    let first = held.refresh().expect("the first refusal is news");
    assert_eq!(
        held.refresh(),
        None,
        "the same refusal on the next turn is the same sentence again",
    );
    assert_eq!(held.refresh(), None, "and on the turn after that");

    // A refusal that says something different is a different thing to say.
    behind_its_back(&io_toml, BROKEN_OTHERWISE);
    let second = held
        .refresh()
        .expect("a refusal the operator has not seen is news");
    assert_ne!(first, second, "the fixture's two refusals must differ");

    // Good again, then broken the same way as the first time. The second break is
    // news even though its text has been said before.
    behind_its_back(&io_toml, good);
    assert_eq!(held.refresh(), None);
    behind_its_back(&io_toml, BROKEN);
    assert_eq!(
        held.refresh().as_deref(),
        Some(first.as_str()),
        "a file that breaks, is fixed, and breaks again must report twice; \
         a success has to clear what was reported",
    );
}
