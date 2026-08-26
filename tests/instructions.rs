//! Making io-harness read all three guidance files, and reporting honestly which
//! of them it is really reading.
//!
//! Writing `IO.md`, `AGENTS.md` and `AGENTS.local.md` is [`io_cli::memory`]'s
//! other half and `tests/memory.rs` covers it. This file covers the half that
//! decides whether any of it reaches a model: `[instructions] files`, which is
//! the only mechanism io-harness has for finding two of the three
//! (`io-harness-0.69.0/src/config.rs:158` — the automatic set is exactly
//! `["AGENTS.md"]`).
//!
//! Every test sets `IO_CONFIG_HOME`, because that is the only way to move the
//! user scope — both `memory::path`'s answer and
//! `io_harness::config::user_path`'s — without writing into the machine running
//! the suite. The environment is process-wide and this binary's tests share a
//! process, so every one of them takes the lock below. That is the shape
//! `tests/memory.rs`, `tests/home.rs`, `tests/contract.rs` and
//! `tests/configure.rs` already use, and copying it is deliberate: an existing
//! pattern that serialises the writers is worth more than a clever one that
//! avoids them.

use std::path::{Path, PathBuf};
use std::sync::{Mutex, MutexGuard};

use io_cli::memory::{self, Instruction};
use io_harness::config::{Config, Scope};

/// Held by every test in this file. See the module note.
fn env_lock() -> MutexGuard<'static, ()> {
    static LOCK: Mutex<()> = Mutex::new(());
    LOCK.lock().unwrap_or_else(|poisoned| poisoned.into_inner())
}

/// The three, in one place, so a test that means "all of them" cannot quietly
/// mean two.
const SCOPES: [Scope; 3] = [Scope::User, Scope::Project, Scope::Local];

/// Point io's home at `home` and clear the variable that would win over it.
///
/// `IO_CONFIG` names a file outright and beats `IO_CONFIG_HOME`
/// (`config.rs:1668-1673`), so a developer who has one exported would otherwise
/// have this suite writing into their own configuration.
fn home_at(home: &Path) {
    std::env::remove_var(io_harness::config::CONFIG_VAR);
    std::env::set_var(io_harness::config::CONFIG_HOME_VAR, home);
}

/// A workspace root and an io home, in one temporary directory that cleans
/// itself up.
fn fixture() -> (tempfile::TempDir, PathBuf) {
    let dir = tempfile::tempdir().expect("a temporary directory");
    let root = dir.path().join("workspace");
    std::fs::create_dir_all(&root).expect("the workspace");
    home_at(&dir.path().join("home"));
    (dir, root)
}

fn at(root: &Path, scope: Scope) -> PathBuf {
    memory::path(root, scope).expect("every scope has a path once a home is named")
}

/// Put one identifiable line in a scope's file, through the module's own writer
/// — so the file is exactly the shape `/memory` produces rather than a shape
/// invented here.
fn seed(root: &Path, scope: Scope, line: &str) -> PathBuf {
    memory::remember(root, scope, line).expect("the line lands")
}

fn row(view: &[Instruction], scope: Scope) -> &Instruction {
    view.iter()
        .find(|row| row.scope == scope)
        .unwrap_or_else(|| panic!("the view has no row for {scope:?}"))
}

/// **The list always names `AGENTS.md`, and the user file's entry is absolute.**
///
/// The two facts the whole feature rests on, asserted on their own so a failure
/// says which one broke.
///
/// `files` REPLACES io-harness's default rather than adding to it
/// (`config.rs:1879-1882`: `Some(files) => files.clone()`), so a list written to
/// reach `AGENTS.local.md` and `IO.md` alone would silently stop the
/// repository's own `AGENTS.md` being read — no error, because a name that
/// resolves to nothing is skipped in silence (`config.rs:1886`).
///
/// And `IO.md` cannot be named relatively at all: every name is resolved
/// `root.join(&name)` against the discovery root (`config.rs:1885`), and io's
/// home is not the workspace.
#[test]
fn the_list_names_the_committed_file_and_reaches_the_home_by_absolute_path() {
    let _guard = env_lock();
    let (_dir, root) = fixture();

    let files = memory::files(&root);

    assert_eq!(
        files.first().map(PathBuf::as_path),
        Some(Path::new("AGENTS.md")),
        "the committed file is not in the list — naming `files` replaces \
         io-harness's default `[\"AGENTS.md\"]` instead of adding to it, so \
         leaving it out stops the repository's own instructions being read and \
         says nothing: {files:?}",
    );
    assert!(
        files.contains(&PathBuf::from("AGENTS.local.md")),
        "the uncommitted sibling is not in the list, and nothing else discovers \
         it: {files:?}",
    );

    let user = at(&root, Scope::User);
    assert!(
        files.contains(&user),
        "the user file is not in the list by its absolute path: {files:?}",
    );
    assert!(
        user.is_absolute(),
        "a relative name would resolve against the workspace, and io's home is \
         not the workspace: {}",
        user.display(),
    );
}

/// **F2.** A distinct line in each of the three files, and io-harness reads all
/// three.
///
/// The sabotage arm: name only `AGENTS.local.md` and `IO.md` in `files` and this
/// test alone fails, on the committed file that every other agent reads too.
#[test]
fn all_three_files_reach_the_configuration_once_the_list_is_installed() {
    let _guard = env_lock();
    let (_dir, root) = fixture();

    seed(&root, Scope::Project, "the repository rule");
    seed(&root, Scope::Local, "the checkout rule");
    seed(&root, Scope::User, "the operator rule");

    assert!(
        memory::install(&root).expect("the list is written"),
        "the first install has a list to write",
    );

    let config = Config::discover(&root).expect("the written list parses");
    let read = config.instructions().join("\n");

    for line in [
        "the repository rule",
        "the checkout rule",
        "the operator rule",
    ] {
        assert!(
            read.contains(line),
            "`{line}` did not reach the configuration — a guidance file that is \
             written and never read is worse than one that was never written, \
             because the surface said it was recorded:\n{read}",
        );
    }
    assert_eq!(
        config.instructions().len(),
        3,
        "one constraint per file, and no file counted twice:\n{read}",
    );
}

/// The write is a no-op the second time.
///
/// This runs from a command an operator types repeatedly. A write per
/// invocation would churn `io.toml`'s bytes and its mtime for a change that
/// never happened, and would do it to the one file in this product that carries
/// their credentials.
#[test]
fn installing_a_list_that_is_already_correct_writes_nothing() {
    let _guard = env_lock();
    let (_dir, root) = fixture();

    assert!(memory::install(&root).expect("the first install"));

    let path = io_cli::configure::scope_path(&root, Scope::User).expect("the user path");
    let after_first = std::fs::read_to_string(&path).expect("the user configuration");

    assert!(
        !memory::install(&root).expect("the second install"),
        "the second install reported a change it did not need to make",
    );
    assert_eq!(
        std::fs::read_to_string(&path).expect("the user configuration"),
        after_first,
        "the second install rewrote the file byte for byte identically, which is \
         still a write",
    );
}

/// The absolute path is written literally, and never as `${env:HOME}/...` or
/// with a `~`.
///
/// io-harness does substitute `${env:…}`, and an unset variable is a **hard
/// error** that fails the whole parse (`config.rs:1983-1989`) — on Windows
/// `HOME` frequently is unset, and a configuration that refuses to parse is a
/// session that does not start. `~` is worse: nothing in `config.rs` expands
/// one, so it would be taken as a directory literally named `~`.
#[test]
fn the_user_path_is_written_literally_and_not_as_a_substitution() {
    let _guard = env_lock();
    let (_dir, root) = fixture();

    memory::install(&root).expect("the list is written");

    let path = io_cli::configure::scope_path(&root, Scope::User).expect("the user path");
    let text = std::fs::read_to_string(&path).expect("the user configuration");

    assert!(
        !text.contains("${"),
        "a substitution in this list is a hard parse error wherever the variable \
         is unset, which takes the whole session down rather than one file:\n{text}",
    );
    assert!(
        !text.contains('~'),
        "io-harness expands no `~`, so this would name a directory called `~`:\n{text}",
    );
}

/// **F5.** The view says what each file holds and which are actually read.
#[test]
fn the_view_reports_what_each_file_holds_and_that_it_is_read() {
    let _guard = env_lock();
    let (_dir, root) = fixture();

    for scope in SCOPES {
        seed(&root, scope, "one rule");
    }
    memory::install(&root).expect("the list is written");

    let config = Config::discover(&root).expect("the written list parses");
    let view = memory::view(&root, &config);
    assert_eq!(view.len(), 3, "three files, three rows: {view:?}");

    for scope in SCOPES {
        let row = row(&view, scope);
        assert_eq!(row.path, at(&root, scope), "{scope:?} names the wrong file");
        assert!(
            row.exists,
            "{scope:?} was written and the view says it is absent"
        );
        assert!(
            row.read,
            "{scope:?} exists, is named in the list, and is not read"
        );
        assert_eq!(
            row.lines,
            std::fs::read_to_string(&row.path)
                .expect("the file")
                .lines()
                .count(),
            "{scope:?} reports a line count that is not the file's",
        );
        assert!(
            row.lines > 1,
            "{scope:?} has a header and a bullet: {row:?}"
        );
    }
}

/// **F5's sabotage arm.** A project `[instructions] files` replaces the user
/// list, and the view says so rather than reporting every file that exists as
/// read.
///
/// `["instructions","files"]` is not in io-harness's `APPENDING` set
/// (`config.rs:2052`), so a later scope replaces the array wholesale rather than
/// adding to it — the scopes are merged in the order listed at
/// `config.rs:688-693` and a later value overwrites (`config.rs:2142-2144`), so
/// Local beats Project beats User.
/// This is the case a view inferring `read` from `exists` gets wrong, and it is
/// the case the view exists for: the operator's `IO.md` is on disk, was
/// installed correctly, and is not reaching the model.
#[test]
fn a_project_list_replaces_the_user_one_and_the_unread_files_say_so() {
    let _guard = env_lock();
    let (_dir, root) = fixture();

    for scope in SCOPES {
        seed(&root, scope, "one rule");
    }
    memory::install(&root).expect("the list is written");

    // The project's own say. Written as bytes rather than through the edit API,
    // because what is under test is io-harness's merge, not io-cli's writer.
    std::fs::write(
        root.join(io_harness::config::PROJECT_FILE),
        "[instructions]\nfiles = [\"AGENTS.md\"]\n",
    )
    .expect("the project configuration");

    let config = Config::discover(&root).expect("both files parse");
    let view = memory::view(&root, &config);

    assert!(
        row(&view, Scope::Project).read,
        "the one file the project kept is not read: {view:?}",
    );

    for scope in [Scope::Local, Scope::User] {
        let row = row(&view, scope);
        assert!(
            row.exists,
            "{scope:?} is on disk and the view says otherwise"
        );
        assert!(
            !row.read,
            "{scope:?} exists and the view calls it read, but the project's own \
             `[instructions] files` replaced the list it was named in — this is \
             the row an operator needs to see, and reporting it green is worse \
             than having no view at all: {row:?}",
        );
    }
}

/// A file that is not there is skipped without an error, and the view says
/// absent rather than unread-for-some-other-reason.
///
/// io-harness skips a named file that does not exist in silence
/// (`config.rs:1886`), so nothing downstream fails — which is exactly why the
/// view has to distinguish the two.
#[test]
fn a_named_file_that_is_absent_is_skipped_and_reported_absent() {
    let _guard = env_lock();
    let (_dir, root) = fixture();

    seed(&root, Scope::Project, "the only rule there is");
    memory::install(&root).expect("the list is written");

    let config = Config::discover(&root).expect("two of the three names resolve to nothing");
    assert_eq!(
        config.instructions().len(),
        1,
        "only the file that exists is read: {:?}",
        config.instructions(),
    );

    let view = memory::view(&root, &config);
    assert!(row(&view, Scope::Project).read);

    for scope in [Scope::Local, Scope::User] {
        let row = row(&view, scope);
        assert!(!row.exists, "{scope:?} was never written: {row:?}");
        assert_eq!(
            row.lines, 0,
            "a file that is not there holds no lines: {row:?}"
        );
        assert!(!row.read, "{scope:?} cannot be read: {row:?}");
    }
}

/// A file that exists and holds nothing but whitespace is not read, and the view
/// does not pretend it is.
///
/// `read_instructions` trims and skips an empty result (`config.rs:1891-1894`).
/// The row an operator sees is "there, and not reaching the model", which is the
/// only wording that explains why nothing changed after they created it.
#[test]
fn a_whitespace_only_file_exists_and_is_still_not_read() {
    let _guard = env_lock();
    let (_dir, root) = fixture();

    seed(&root, Scope::Project, "the only rule there is");
    std::fs::write(at(&root, Scope::Local), "\n   \n\t\n").expect("the blank file");
    memory::install(&root).expect("the list is written");

    let config = Config::discover(&root).expect("a blank instruction file is not an error");
    let view = memory::view(&root, &config);

    let row = row(&view, Scope::Local);
    assert!(row.exists, "the file is on disk: {row:?}");
    assert!(
        !row.read,
        "io-harness skips a file that trims to nothing, and a view that calls it \
         read leaves the operator with no explanation for why it changed \
         nothing: {row:?}",
    );
}
