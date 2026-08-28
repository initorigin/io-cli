//! F3 — the branch is read from `.git/HEAD`, in every shape a real one takes.
//!
//! Every fixture here is a real `.git`, written with the same bytes git writes:
//! the trailing newline it always leaves on `HEAD`, the `gitdir:` file a linked
//! worktree gets instead of a directory, the per-worktree `commondir` beside it.
//! Nothing is mocked, because the only thing this criterion can get wrong is the
//! file format, and a fixture that invents the format asserts the invention.
//!
//! The shapes that matter are the ones a developer's own checkout does not have.
//! A symbolic ref works on the first try in anybody's clone; a detached head, a
//! linked worktree and a directory that was never a repository are the three that
//! reach an operator's screen without ever having reached the author's, so they
//! get more tests here than the ordinary case does.
//!
//! **`None` and `Some("")` are not the same answer**, and half of this file exists
//! to keep them apart. `None` draws no field at all; an empty string draws a label
//! with a hole where the branch should be, which reads as "you are nowhere" rather
//! than "this is not a repository". Every malformed input below is asserted absent
//! rather than merely falsy.

use std::path::Path;

use io_cli::repo;

/// A real sha1 object id, and what seven characters of it look like.
///
/// Written out rather than generated so the expected value below is a constant a
/// reader can check by eye against the fixture.
const ID: &str = "9fceb02d0ae598e95dc970b74767f19372d61af8";
const SHORT: &str = "9fceb02";

fn write(path: &Path, contents: &str) {
    std::fs::create_dir_all(path.parent().expect("a parent")).expect("the directory");
    std::fs::write(path, contents).expect("the file");
}

/// Write what `git init` leaves on disk under `root`, with `head` as the literal
/// contents of `.git/HEAD`.
///
/// The `config`, `objects/` and `refs/heads/` entries are not read by anything
/// under test. They are here so that every fixture is a repository git itself
/// would recognise — a test whose fixture is only the one file it reads cannot
/// notice the day the reading widens.
fn checkout(root: &Path, head: &str) {
    let git = root.join(".git");
    write(&git.join("HEAD"), head);
    write(
        &git.join("config"),
        "[core]\n\trepositoryformatversion = 0\n\tbare = false\n",
    );
    std::fs::create_dir_all(git.join("objects")).expect("the object store");
    std::fs::create_dir_all(git.join("refs").join("heads")).expect("the ref store");

    // A symbolic head names a branch, and a branch git wrote has a loose ref
    // holding its tip. Writing it keeps the fixture honest for the nested case in
    // particular, where the ref is a nested *directory* under `refs/heads/`.
    // The empty name is skipped rather than written: `refs/heads/` with nothing
    // after it is one of the malformed heads asserted below, and git has no file
    // for it either.
    match head.trim().strip_prefix("ref: refs/heads/") {
        Some(name) if !name.is_empty() => {
            write(
                &git.join("refs").join("heads").join(name),
                &format!("{ID}\n"),
            );
        }
        _ => {}
    }
}

/// Link a worktree at `tree` to the repository at `root`, the way
/// `git worktree add` does.
///
/// The worktree's `.git` is a **file**, and its head lives in the repository's
/// `worktrees/<name>/` directory rather than beside it — which is the whole reason
/// following the indirection is a criterion. `named` is what that file says, so a
/// caller can hand it either the absolute path git usually writes or the relative
/// one it writes under `worktree.useRelativePaths`.
fn worktree(root: &Path, tree: &Path, name: &str, head: &str, named: &str) {
    let inner = root.join(".git").join("worktrees").join(name);
    write(&inner.join("HEAD"), head);
    write(&inner.join("commondir"), "../..\n");
    write(
        &inner.join("gitdir"),
        &format!("{}\n", tree.join(".git").display()),
    );
    write(&tree.join(".git"), &format!("gitdir: {named}\n"));
}

#[test]
fn f3_a_symbolic_ref_is_the_branch_name() {
    let dir = tempfile::tempdir().expect("a temporary directory");
    checkout(dir.path(), "ref: refs/heads/main\n");

    assert_eq!(repo::branch(dir.path()).as_deref(), Some("main"));
}

#[test]
fn f3_a_nested_branch_keeps_every_component_after_refs_heads() {
    let dir = tempfile::tempdir().expect("a temporary directory");
    checkout(dir.path(), "ref: refs/heads/feature/nested-name\n");

    // The tempting implementation takes the last path component, and it is wrong
    // in the exact place this product lives: every release branch here is
    // `feat/<version>`, which that implementation would report as the version
    // alone, on every screen, for the whole release.
    assert_eq!(
        repo::branch(dir.path()).as_deref(),
        Some("feature/nested-name"),
    );
}

#[test]
fn f3_a_detached_head_is_a_short_object_id() {
    let dir = tempfile::tempdir().expect("a temporary directory");
    checkout(dir.path(), &format!("{ID}\n"));

    let head = repo::branch(dir.path()).expect("a detached head still says where it is");
    assert_eq!(head, SHORT);
    // Seven is not an arbitrary number: it is what `git log --oneline` and every
    // code host abbreviate to, so the id here is the id the operator will match
    // against somewhere else.
    assert_eq!(head.len(), repo::SHORT_ID);
}

#[test]
fn f3_a_head_without_a_trailing_newline_reads_the_same() {
    let with = tempfile::tempdir().expect("a temporary directory");
    let without = tempfile::tempdir().expect("a temporary directory");
    checkout(with.path(), "ref: refs/heads/main\n");
    // git always leaves the newline. An editor, a script, or a filesystem that
    // truncated it does not make the branch unknown, and neither does a CRLF from
    // a file written on Windows.
    checkout(without.path(), "ref: refs/heads/main");

    assert_eq!(repo::branch(with.path()), repo::branch(without.path()));
    assert_eq!(repo::branch(without.path()).as_deref(), Some("main"));

    let crlf = tempfile::tempdir().expect("a temporary directory");
    checkout(crlf.path(), "ref: refs/heads/main\r\n");
    assert_eq!(repo::branch(crlf.path()).as_deref(), Some("main"));
}

#[test]
fn f3_a_linked_worktree_is_followed_to_its_own_head() {
    let dir = tempfile::tempdir().expect("a temporary directory");
    let root = dir.path().join("main");
    let tree = dir.path().join("tree");
    checkout(&root, "ref: refs/heads/main\n");
    std::fs::create_dir_all(&tree).expect("the worktree");
    worktree(
        &root,
        &tree,
        "tree",
        "ref: refs/heads/feat/0.25.0\n",
        &root
            .join(".git")
            .join("worktrees")
            .join("tree")
            .display()
            .to_string(),
    );

    // Reading the worktree's own head rather than the repository's is the point:
    // a `worktree = true` child agent is on a different branch to its parent, and
    // reporting the parent's would be a confident wrong answer rather than none.
    assert_eq!(repo::branch(&tree).as_deref(), Some("feat/0.25.0"));
    assert_eq!(repo::branch(&root).as_deref(), Some("main"));
}

#[test]
fn f3_a_relative_gitdir_is_resolved_against_the_worktree() {
    let dir = tempfile::tempdir().expect("a temporary directory");
    let root = dir.path().join("main");
    let tree = dir.path().join("tree");
    checkout(&root, "ref: refs/heads/main\n");
    std::fs::create_dir_all(&tree).expect("the worktree");
    // What git writes under `worktree.useRelativePaths`, and what a checkout that
    // has been moved as a whole carries. Relative to the directory holding the
    // `.git` file, which is the worktree root — not to the process's current
    // directory, which is where this would go wrong silently.
    worktree(
        &root,
        &tree,
        "tree",
        &format!("{ID}\n"),
        "../main/.git/worktrees/tree",
    );

    assert_eq!(repo::branch(&tree).as_deref(), Some(SHORT));
}

#[test]
fn f3_a_directory_that_is_not_a_repository_has_no_field() {
    let dir = tempfile::tempdir().expect("a temporary directory");

    // io-cli runs in plenty of these and must not become worse in them. No `.git`
    // at all is the ordinary case, not an error.
    assert_eq!(repo::branch(dir.path()), None);
}

#[test]
fn f3_a_missing_head_is_no_field() {
    let dir = tempfile::tempdir().expect("a temporary directory");
    std::fs::create_dir_all(dir.path().join(".git").join("objects")).expect("a git directory");

    assert_eq!(repo::branch(dir.path()), None);
}

#[test]
fn f3_a_head_that_is_not_an_object_id_is_no_field() {
    let dir = tempfile::tempdir().expect("a temporary directory");
    // The failure mode this guards is not a crash. It is seven characters of
    // whatever the file happened to hold arriving on the status line looking
    // exactly like a commit.
    for garbage in [
        "not a reference at all\n",
        "gitdir: /somewhere/else\n",
        "zzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzz\n",
        // Thirty-nine hex characters and one that is not. The check has to cover
        // the whole line, not the seven characters that get reported.
        "9fceb02d0ae598e95dc970b74767f19372d61afg\n",
    ] {
        checkout(dir.path(), garbage);
        assert_eq!(
            repo::branch(dir.path()),
            None,
            "a HEAD reading {garbage:?} is not an object id",
        );
    }
}

#[test]
fn f3_a_head_too_short_to_be_an_object_id_is_no_field() {
    let dir = tempfile::tempdir().expect("a temporary directory");
    // Six hex characters. Every character is legal and the whole thing is still
    // not an id, so the length is checked before the slice rather than after —
    // taking seven characters from six is where a truncating implementation
    // panics or, worse, quietly reports the six.
    checkout(dir.path(), "9fceb0\n");

    assert_eq!(repo::branch(dir.path()), None);
}

#[test]
fn f3_a_head_that_names_something_other_than_a_branch_is_no_field() {
    let dir = tempfile::tempdir().expect("a temporary directory");
    for reference in [
        "ref: refs/tags/v0.25.0\n",
        "ref: refs/remotes/origin/main\n",
    ] {
        checkout(dir.path(), reference);
        assert_eq!(
            repo::branch(dir.path()),
            None,
            "{reference:?} does not put the tree on a branch",
        );
    }
}

#[test]
fn f3_a_gitdir_file_naming_nothing_is_no_field() {
    let dir = tempfile::tempdir().expect("a temporary directory");
    for broken in ["gitdir:\n", "gitdir:   \n", "", "gitdir: /nowhere/at/all\n"] {
        write(&dir.path().join(".git"), broken);
        assert_eq!(
            repo::branch(dir.path()),
            None,
            "a .git file reading {broken:?} points at no repository",
        );
    }
}

#[test]
fn f3_no_head_ever_yields_an_empty_name() {
    let dir = tempfile::tempdir().expect("a temporary directory");
    // The sabotage this file is scored against reports a detached head as absent.
    // This is the mirror of it: the field going blank rather than going away. An
    // operator reading a label with nothing after it concludes something about
    // where they are; an operator reading no label at all concludes nothing, which
    // is the truth here.
    for empty in ["", "\n", "   \n", "ref:\n", "ref: \n", "ref: refs/heads/\n"] {
        checkout(dir.path(), empty);
        let answer = repo::branch(dir.path());
        assert_eq!(
            answer, None,
            "a HEAD reading {empty:?} named no branch, and an empty field is not \
             how that is said",
        );
    }
}

/// **F3 — a workspace below the repository root still knows its branch.**
///
/// `io -C src` is an ordinary invocation, and git itself walks up from the
/// working directory — so the agent's seven built-ins find the repository while a
/// check that looked only at `<root>/.git` found nothing and said, permanently,
/// that a perfectly readable checkout had no branch. Found by the adversarial
/// review; none of the arms above reached it, because every one of them builds
/// its fixture at the root it then asks about.
#[test]
fn f3_a_directory_below_the_repository_root_still_reads_the_branch() {
    let dir = tempfile::tempdir().expect("a workspace");
    let root = dir.path();
    checkout(root, "ref: refs/heads/feat/0.25.0\n");

    let nested = root.join("src").join("deep");
    std::fs::create_dir_all(&nested).expect("the nested directory");

    assert_eq!(
        repo::branch(&nested).as_deref(),
        Some("feat/0.25.0"),
        "a directory inside the checkout must read the checkout's branch; git \
         walks up and so must this",
    );

    // And a directory that is genuinely outside any repository still answers
    // nothing, rather than walking up far enough to find someone else's.
    let elsewhere = tempfile::tempdir().expect("a directory that is not a checkout");
    assert_eq!(
        repo::branch(elsewhere.path()),
        None,
        "walking up must stop at the filesystem, not borrow an unrelated repository",
    );
}
