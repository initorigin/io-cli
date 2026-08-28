//! The branch the working tree is on.
//!
//! **This module is not called `git`, and the name is the point.** io-harness owns
//! the git engine — `Git`, `GitCmd` and `GitOutcome` are all `pub(crate)` there, so
//! nothing in this crate can build a governed argv or run a git command — and a
//! crate-root module named for it would send a reader looking here for something
//! that is not here. The same reasoning named [`crate::picture`] rather than
//! `image` in 0.9.0, and kept [`crate::verify`] for the wizard's credential check
//! in 0.24.0.
//!
//! **Nothing here spawns anything.** `tests/dependencies.rs` permits a process
//! spawn in `src/shell.rs` alone, identified by path, and that assertion is
//! unamended by this release. That gate matches raw text, comments included, so
//! the forbidden name is not spelled out here either — not even to say this module
//! does not use it, which is a sentence that reddens the gate it is describing.
//! A branch name does not need a subprocess anyway: git writes it into `.git/HEAD`
//! as text, in a format that has not changed in the lifetime of the tool, and the
//! standard library reads a file.
//!
//! The four shapes this has to survive, only one of which a developer's own
//! checkout usually has:
//!
//! - A **symbolic ref** — `ref: refs/heads/<name>` — which is the ordinary case.
//! - A **detached head**, where `HEAD` holds a raw object id. Reported as a short
//!   id, because reporting nothing is how a surface goes blank at the moment an
//!   operator most needs to know where they are.
//! - A **linked worktree**, where `.git` is a *file* holding `gitdir: <path>`
//!   rather than a directory. This is exactly what a `worktree = true` child gets,
//!   so the release that adds that switch cannot be the release that fails to read
//!   it.
//! - A directory that is **not a repository at all**, which answers `None` — io-cli
//!   runs in plenty of them and must not become worse there.

/// How long a detached head's object id is rendered.
///
/// Seven is what `git log --oneline` and every code host use, so the id here is
/// the id an operator will recognise elsewhere.
pub const SHORT_ID: usize = 7;

/// The branch the tree at `root` is on, or `None` when there is no answer.
///
/// `None` means "this is not a repository, or its `HEAD` cannot be read" — never
/// "the branch is empty". A caller draws nothing at all for `None`, which is what
/// keeps io-cli silent in a directory that was never a checkout.
pub fn branch(root: &std::path::Path) -> Option<String> {
    let head = git_dir(root)?.join("HEAD");
    let text = std::fs::read_to_string(head).ok()?;
    read_head(&text)
}

/// The real git directory for `root`, following the one indirection git uses for a
/// linked worktree.
///
/// In an ordinary checkout `.git` is a directory and this is it. In a linked
/// worktree — which is what `AgentDef::worktree` gives a child agent — `.git` is a
/// *file* whose contents are `gitdir: <path>`, and the path may be relative to the
/// worktree. Both are resolved here so no caller has to know the difference.
pub fn git_dir(root: &std::path::Path) -> Option<std::path::PathBuf> {
    // **Walked upward, because a workspace root is not always a repository root.**
    // `io -C src` is an ordinary thing to do, and git itself walks up from the
    // working directory — so the agent's own seven tools find the repository
    // while a check that looked only at `root/.git` found nothing and reported,
    // permanently, that a perfectly readable checkout had no branch. The first
    // ancestor holding a `.git` wins, which is git's own rule.
    root.ancestors().find_map(git_dir_at)
}

/// The git directory named by exactly this directory's `.git`, if it has one.
fn git_dir_at(root: &std::path::Path) -> Option<std::path::PathBuf> {
    let dot = root.join(".git");
    let meta = std::fs::metadata(&dot).ok()?;
    if meta.is_dir() {
        return Some(dot);
    }
    let text = std::fs::read_to_string(&dot).ok()?;
    // First line only, for the same reason `read_head` takes one: the file holds a
    // single `gitdir:` line, and anything after it is not part of the path. Taking
    // the whole text would fold a stray second line into the directory name and
    // turn a readable worktree into a `None`.
    let named = text.lines().next()?.strip_prefix("gitdir:")?.trim();
    if named.is_empty() {
        return None;
    }
    let path = std::path::Path::new(named);
    Some(if path.is_absolute() {
        path.to_path_buf()
    } else {
        root.join(path)
    })
}

/// What a `HEAD` file says, as a branch name or a short object id.
///
/// Split from [`branch`] because this is the half that has to be right: the file
/// read above it either works or answers `None`, while every wrong answer this
/// module could give is a misparse in here. `tests/repo.rs` still drives it
/// through a real `.git` on disk rather than calling it directly — the format is
/// the thing under test, and a caller that hands it a string it invented is
/// asserting its own invention.
pub fn read_head(text: &str) -> Option<String> {
    let line = text.lines().next()?.trim();
    if line.is_empty() {
        return None;
    }
    match line.strip_prefix("ref:") {
        // A symbolic ref. Only the `refs/heads/` prefix comes off — everything
        // after it is the name, so `refs/heads/feature/x` reads as `feature/x`
        // and not as `x`. Taking the last path component instead would rename
        // every nested branch in the product to its own tail, which is precisely
        // the case a developer's own checkout usually does not have.
        //
        // A ref that is not under `refs/heads/` — a tag, a remote — is not a
        // branch, and answering `None` for it says so rather than inventing a
        // name for a head that has none.
        Some(reference) => {
            let name = reference.trim().strip_prefix("refs/heads/")?;
            (!name.is_empty()).then(|| name.to_string())
        }
        // A detached head. Reported short, and only when the WHOLE line is an
        // object id — anything else is a `HEAD` this function does not
        // understand, and truncating it to seven characters anyway would put
        // arbitrary file contents on the status line dressed as a commit. Hence
        // the check before the slice: a line too short to be an id, or carrying a
        // single non-hex character, is no answer rather than a plausible-looking
        // wrong one.
        None => (line.len() >= SHORT_ID && line.chars().all(|c| c.is_ascii_hexdigit()))
            .then(|| line.chars().take(SHORT_ID).collect()),
    }
}
