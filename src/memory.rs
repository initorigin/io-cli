//! What the operator asks io to remember, written where the next session reads it.
//!
//! There is already a memory in this product and this is not it. io-harness keys
//! a store per run — `EventKind::MemoryWrote`, `[memory] max_entries` — and that
//! one belongs to the agent: it writes it, a rewind takes it back
//! ([`crate::rewind`]), and nobody types into it. This module is the other half,
//! the one a person writes by hand: a line of guidance appended to a markdown
//! file that io-harness reads as an instruction at the start of every run.
//!
//! # One word, one meaning
//!
//! The scope is [`io_harness::config::Scope`], the same three the configuration
//! surface already uses, and deliberately not a second enum that happens to have
//! the same variant names. [`crate::configure::scope_path`] maps them to
//! `io.toml` / `io.toml` / `io.local.toml`; this maps them to `IO.md` /
//! `AGENTS.md` / `AGENTS.local.md`. An operator who has learnt what "local"
//! means once has learnt it for both surfaces, and a second enum is how the two
//! drift until "project" means one thing in `/config` and another in `/memory`.
//!
//! The user scope anchors on [`crate::home::in_force`] rather than on
//! [`crate::home::path`], for the reason that function exists: under `$IO_CONFIG`
//! the configuration file is somewhere io-cli did not choose, and `IO.md` belongs
//! beside the `io.toml` actually in force, not beside the one this crate would
//! have picked.
//!
//! # What io-harness will read back, which is not all three
//!
//! Worth knowing before a surface promises otherwise. `read_instructions` joins
//! each name in `[instructions] files` to the **discovery root**
//! (`io-harness-0.69.0/src/config.rs:1888`), and the default list is exactly
//! `["AGENTS.md"]` (`config.rs:158`). So `AGENTS.md` is read by every project
//! with no configuration at all; `AGENTS.local.md` is read only where a file
//! names it; and `IO.md` is not reachable by a bare name at all, because a
//! relative name is resolved against the workspace and the home is not the
//! workspace. **This module writes the file. Making the harness read it is
//! `[instructions]`' business and the caller's.**
//!
//! # Appending, and why it is spelled out
//!
//! Everything here is an append. These files are the operator's own prose — a
//! person wrote them, another agent may have written into them, and a release of
//! this crate rewriting one is a release that eats somebody's notes. So the file
//! is opened for append rather than read-modify-written, and the only byte this
//! module ever adds ahead of its own bullet is a newline the previous author left
//! off.

use std::io::Write as _;
use std::path::{Path, PathBuf};

use io_harness::config::Scope;

/// The committed one. io-harness discovers this name with no configuration at
/// all, which is why it is the scope a `/memory` with no argument should mean.
const PROJECT: &str = "AGENTS.md";

/// The uncommitted sibling, named the way `io.local.toml` is named — the same
/// stem with `.local` before the extension, so an operator who has seen one
/// spelling can guess the other.
const LOCAL: &str = "AGENTS.local.md";

/// The file each scope keeps its guidance in.
///
/// `&'static str` and not a `PathBuf`: a name is not a location, and the two
/// project scopes are the same name under different roots.
#[must_use]
pub fn file_name(scope: Scope) -> &'static str {
    match scope {
        Scope::User => crate::home::MEMORY,
        Scope::Project => PROJECT,
        Scope::Local => LOCAL,
    }
}

/// Where that file is, for a session rooted at `root`.
///
/// `None` for [`Scope::User`] where there is no home to speak of — the same
/// answer [`crate::home::in_force`] gives, for the same reason: a program that
/// invents a path when it has no home writes into somebody else's. The two
/// project scopes always have one, because the root is the directory the session
/// is already running in.
#[must_use]
pub fn path(root: &Path, scope: Scope) -> Option<PathBuf> {
    match scope {
        Scope::User => Some(crate::home::in_force()?.0.join(file_name(scope))),
        Scope::Project | Scope::Local => Some(root.join(file_name(scope))),
    }
}

/// What a file this module creates says about itself.
///
/// Three headers rather than one, and the difference between them is the only
/// thing an operator has to know before typing into any of them: **who else
/// reads this**. A `AGENTS.md` line goes to everyone who clones the repository;
/// an `AGENTS.local.md` line goes nowhere; an `IO.md` line follows the operator
/// into every project on the machine. Getting that wrong is how a private note
/// about a colleague ends up in a pull request, so it is written into the file
/// at the moment the file is made rather than into a document nobody opens.
fn header(scope: Scope) -> &'static str {
    match scope {
        Scope::User => {
            "# IO.md\n\n\
             What io remembers for this operator. It sits beside `io.toml` in io's \
             own home, applies to every project on this machine, and is part of no \
             repository.\n\n"
        }
        Scope::Project => {
            "# AGENTS.md\n\n\
             Instructions for agents working in this repository. This file is \
             committed, so everything written here is shared with everyone who \
             clones it.\n\n"
        }
        Scope::Local => {
            "# AGENTS.local.md\n\n\
             Instructions for agents working in this checkout alone. This file is \
             not committed and nobody else sees it.\n\n"
        }
    }
}

/// Append one line of guidance, and answer with the file it went into.
///
/// The line is written as a markdown bullet, because the files are markdown and
/// the next thing to read them is a model that has been trained on markdown.
///
/// Refuses a line that is empty or nothing but whitespace. That refusal is the
/// point rather than a nicety: remembering a blank line succeeds silently, tells
/// the operator their instruction was recorded, and records nothing — which is
/// the one failure they cannot see from the surface.
pub fn remember(root: &Path, scope: Scope, line: &str) -> Result<PathBuf, String> {
    let text = line.trim();
    if text.is_empty() {
        return Err("there is nothing to remember".to_string());
    }

    let path = path(root, scope)
        .ok_or_else(|| "there is no path for that scope on this machine".to_string())?;

    // The user scope's parent is io-cli's own home, which is `0700` — so the
    // directory is made the way `home::adopt` makes it rather than with a bare
    // `create_dir_all`, and `IO.md` is protected by the directory around it the
    // same way `io.toml` beside it is. For the project scopes this is a no-op:
    // the root is the directory the session is running in, and an existing
    // directory's mode is not touched. The filter is the relative-root case:
    // `Path::new("x").parent()` is `Some("")`, and creating *that* fails rather
    // than doing nothing.
    if let Some(parent) = path.parent().filter(|parent| !parent.as_os_str().is_empty()) {
        crate::home::create(parent).map_err(|error| format!("{}: {error}", parent.display()))?;
    }

    let prelude = match std::fs::read(&path) {
        // ponytail: the whole file is read to look at its last byte. These are
        // guidance files a person maintains by hand — kilobytes — and the
        // alternative is a `SeekFrom::End(-1)` that has to special-case an empty
        // file anyway. Upgrade to the seek if this ever appends to something
        // machine-generated and large.
        //
        // **Without this the previous author's last line and the new bullet
        // become one line.** A file that ends `remember to run the linter` with
        // no newline, appended to, reads `remember to run the linter- and the
        // formatter` — one instruction turned into a different one, in a file
        // that goes to the model on every run.
        Ok(bytes) if bytes.last().is_some_and(|byte| *byte != b'\n') => "\n",
        Ok(_) => "",
        // The file is created below, and this is the only chance to say what it
        // is and who else will read it.
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => header(scope),
        Err(error) => return Err(format!("{}: {error}", path.display())),
    };

    // `append`, not `write` and not `truncate`: every byte already in the file
    // stays where it is, and the position is resolved by the kernel at each
    // write rather than from what was read above.
    let mut file = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(&path)
        .map_err(|error| format!("{}: {error}", path.display()))?;
    writeln!(file, "{prelude}- {text}").map_err(|error| format!("{}: {error}", path.display()))?;

    Ok(path)
}
