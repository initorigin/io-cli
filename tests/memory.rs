//! The operator memory writer: three scopes, three files, and never a byte lost.
//!
//! Every test in this file writes `IO_CONFIG_HOME`, because that is the only way
//! to move [`memory::path`]'s answer for [`Scope::User`] without touching the
//! machine the suite is running on — `home::in_force` reads the environment at
//! call time, deliberately, so that a `/status` typed an hour into a session
//! answers about the directory in force rather than one cached at startup.
//!
//! The environment is process-wide and this binary's tests share a process, so
//! every one of them takes the lock below. That is the shape `tests/home.rs`,
//! `tests/wizard.rs`, `tests/contract.rs` and `tests/docs.rs` already use, and it
//! is why this file invents nothing: an existing pattern that serialises the
//! writers is worth more than a clever one that avoids them.

use std::path::{Path, PathBuf};
use std::sync::{Mutex, MutexGuard};

use io_cli::memory;
use io_harness::config::Scope;

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
/// `IO_CONFIG` names a file outright and beats `IO_CONFIG_HOME`, so a developer
/// who has one exported would otherwise have this suite writing `IO.md` next to
/// their own configuration file.
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

fn read(path: &Path) -> String {
    std::fs::read_to_string(path).unwrap_or_else(|error| panic!("{}: {error}", path.display()))
}

fn at(root: &Path, scope: Scope) -> PathBuf {
    memory::path(root, scope).expect("every scope has a path once a home is named")
}

/// The three names, and the two roots they sit under.
#[test]
fn each_scope_names_its_own_file_beside_the_configuration_it_belongs_to() {
    let _guard = env_lock();
    let dir = tempfile::tempdir().expect("a temporary directory");
    let root = dir.path().join("workspace");
    let home = dir.path().join("home");
    home_at(&home);

    assert_eq!(memory::file_name(Scope::User), "IO.md");
    assert_eq!(memory::file_name(Scope::Project), "AGENTS.md");
    assert_eq!(memory::file_name(Scope::Local), "AGENTS.local.md");

    assert_eq!(
        at(&root, Scope::User),
        home.join("IO.md"),
        "the user file sits beside the io.toml in force, not in the workspace",
    );
    assert_eq!(at(&root, Scope::Project), root.join("AGENTS.md"));
    assert_eq!(at(&root, Scope::Local), root.join("AGENTS.local.md"));
}

/// **The sabotage arm.** A line remembered in one scope lands in that scope's
/// file and in no other.
///
/// This is the criterion. An implementation that ignored the scope and wrote the
/// committed `AGENTS.md` every time would satisfy every other test in this file
/// — the bullet is there, the bytes are preserved, the order is right — and
/// would put a private note into a pull request the first time an operator typed
/// one. So each scope is written in a fixture of its own and the other two paths
/// are asserted absent, which is the only assertion that can tell the two
/// implementations apart.
#[test]
fn a_line_lands_in_the_scope_it_was_written_for_and_nowhere_else() {
    let _guard = env_lock();

    for scope in SCOPES {
        let (_dir, root) = fixture();

        let written = memory::remember(&root, scope, "prefer small diffs").expect("the line lands");
        assert_eq!(written, at(&root, scope), "remember answers with the file");
        assert!(
            read(&written).contains("- prefer small diffs"),
            "{written:?} does not contain the line it was given",
        );

        for other in SCOPES.into_iter().filter(|s| *s != scope) {
            let path = at(&root, other);
            assert!(
                !path.exists(),
                "a line remembered for {scope:?} created {} — the scope decides the \
                 file, and writing the committed one whatever was asked is how a \
                 private note reaches a pull request",
                path.display(),
            );
        }
    }
}

/// A file that did not exist is created saying what it is and, above all, who
/// else will read it.
#[test]
fn an_absent_file_is_created_with_a_header_that_says_whether_it_is_committed() {
    let _guard = env_lock();

    // The phrase each header has to carry. Not the whole sentence — that is
    // prose and may be reworded — but the fact an operator has to know before
    // typing into the file, which may not be dropped.
    for (scope, said) in [
        (Scope::User, "every project"),
        (Scope::Project, "shared with everyone who clones"),
        (Scope::Local, "not committed"),
    ] {
        let (_dir, root) = fixture();

        let written = memory::remember(&root, scope, "a first line").expect("the line lands");
        let text = read(&written);

        assert!(
            text.starts_with(&format!("# {}\n", memory::file_name(scope))),
            "{scope:?}'s new file does not name itself:\n{text}",
        );
        assert!(
            text.contains(said),
            "{scope:?}'s header does not say `{said}` — whether a guidance file is \
             committed is the whole difference between the three, and the moment \
             the file is made is the only time this module gets to say it:\n{text}",
        );
        assert!(
            text.ends_with("- a first line\n"),
            "the header comes before the first bullet, and the file ends with it:\n{text}",
        );
    }
}

/// Every byte that was there is still there, in the order it was in.
///
/// The file is somebody's prose — a person wrote it, and another agent may have
/// written into it. A release that rewrote one rather than appending would eat
/// notes nobody has another copy of, and would do it silently.
#[test]
fn every_byte_already_in_the_file_is_still_there() {
    let _guard = env_lock();

    for scope in SCOPES {
        let (_dir, root) = fixture();
        let path = at(&root, scope);
        std::fs::create_dir_all(path.parent().expect("a parent")).expect("the directory");

        // Deliberately not the shape this module writes: a heading it did not
        // author, a bullet with different spacing, an indented block, a trailing
        // blank line. Anything that normalises rather than appends loses one of
        // these.
        let before = "# notes somebody wrote by hand\n\n  * an old bullet\n\n\
                      ```\n  a fenced block\n```\n\n";
        std::fs::write(&path, before).expect("the existing file");

        memory::remember(&root, scope, "and one more").expect("the line lands");

        let after = read(&path);
        assert!(
            after.starts_with(before),
            "{scope:?} did not append — the original bytes are no longer a prefix \
             of the file:\n{after}",
        );
        assert_eq!(
            after,
            format!("{before}- and one more\n"),
            "and nothing but the bullet was added",
        );
    }
}

/// A file whose last byte is not a newline does not have its last line joined to
/// the new one.
///
/// `remember to run the linter` with no trailing newline, appended to naively,
/// becomes `remember to run the linter- and the formatter`: one instruction
/// turned into a different one, in a file that reaches the model on every run.
#[test]
fn a_file_that_does_not_end_in_a_newline_is_not_joined_to() {
    let _guard = env_lock();

    for scope in SCOPES {
        let (_dir, root) = fixture();
        let path = at(&root, scope);
        std::fs::create_dir_all(path.parent().expect("a parent")).expect("the directory");
        std::fs::write(&path, "- remember to run the linter").expect("the existing file");

        memory::remember(&root, scope, "and the formatter").expect("the line lands");

        let after = read(&path);
        assert_eq!(
            after, "- remember to run the linter\n- and the formatter\n",
            "{scope:?} joined the previous author's last line to the new bullet",
        );
        assert!(
            !after.contains("linter- "),
            "the two lines are still two lines:\n{after}",
        );
    }
}

/// An empty line is refused, and refusing creates nothing.
///
/// A blank line remembered successfully is the failure an operator cannot see:
/// the surface says it was recorded, the file says nothing, and the next session
/// behaves as though they never typed it.
#[test]
fn an_empty_or_whitespace_only_line_is_refused_and_creates_no_file() {
    let _guard = env_lock();

    for scope in SCOPES {
        for blank in ["", "   ", "\t\n  \n"] {
            let (_dir, root) = fixture();

            let refused = memory::remember(&root, scope, blank);
            assert!(
                refused.is_err(),
                "{scope:?} accepted {blank:?}, which records nothing while reporting \
                 success",
            );
            assert!(
                !at(&root, scope).exists(),
                "{scope:?} created a file for {blank:?} — a refusal that leaves an \
                 empty guidance file behind is a file the operator has to wonder \
                 about",
            );
        }
    }
}

/// Guidance is a list and a list has an order. Two lines come back in the order
/// they were given.
#[test]
fn two_lines_appear_in_the_order_they_were_remembered() {
    let _guard = env_lock();

    for scope in SCOPES {
        let (_dir, root) = fixture();

        memory::remember(&root, scope, "first").expect("the first line");
        let path = memory::remember(&root, scope, "second").expect("the second line");

        let text = read(&path);
        let first = text.find("- first").expect("the first line is in the file");
        let second = text
            .find("- second")
            .expect("the second line is in the file");
        assert!(
            first < second,
            "{scope:?} wrote the second line before the first:\n{text}",
        );
        assert_eq!(
            text.matches("# ").count(),
            1,
            "the header is written once, when the file is made, and not again on \
             every line:\n{text}",
        );
    }
}

/// The line is trimmed before it is written, so a bullet is a bullet.
#[test]
fn surrounding_whitespace_does_not_reach_the_file() {
    let _guard = env_lock();
    let (_dir, root) = fixture();

    let path =
        memory::remember(&root, Scope::Project, "  prefer small diffs \n").expect("the line lands");

    assert!(
        read(&path).ends_with("- prefer small diffs\n"),
        "a pasted line brings its own whitespace, and the file is markdown",
    );
}
