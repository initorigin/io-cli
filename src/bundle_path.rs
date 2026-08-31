//! Where a bundle's declared program goes, so that a run can find it.
//!
//! **io-harness places nothing, and says so in its own contract.** It parses a
//! `[[bin]]`, refuses one declared from a scope that may not contribute an
//! executing thing, validates the path lexically, and offers the result on
//! [`Plugin::bin`](io_harness::Plugin::bin) — an accessor it never calls itself.
//! Where a contributed binary goes is stated there as the host's decision, so the
//! whole of the mechanism is here.
//!
//! **Appended, never prepended.** A bundle is a stranger's code and `PATH` is
//! resolved by first match, so a prepended entry would let anything an operator
//! installed answer to `git`, `cargo` or `ls` for every tool call in the process
//! — including the calls io-harness makes on the model's behalf, whose permission
//! gate matches a *binary name* and would be satisfied by the wrong program under
//! the right name. Appended, a collision resolves to the system command and the
//! bundle's own program is unreachable under that name, which is the failure that
//! can be read rather than the one that cannot.
//!
//! **Nothing is created on disk.** The entry put on `PATH` is the directory the
//! declaring file already sits in, so resolution is by that file's own name. A
//! declaration whose `name` differs from its file is *reported* — see
//! [`mismatched`] — rather than made true by writing a link under the name a
//! stranger chose. Creating one is io-cli installing a program, which this
//! product's contract excludes.
//!
//! The three functions that decide anything are pure and take what they read, so
//! the order, the deduplication and the appending are all assertable without a
//! process environment.

use std::collections::BTreeSet;
use std::ffi::{OsStr, OsString};
use std::path::PathBuf;

use io_harness::Plugins;

/// The environment variable a program is looked up in.
pub const PATH_VAR: &str = "PATH";

/// The directories to append, in the order they will be appended.
///
/// One entry per directory rather than one per declaration: two programs in one
/// `bin/` directory are one `PATH` entry, and a duplicate entry is a lookup
/// performed twice for the same answer. Sorted, so the value written is the same
/// on two runs over the same configuration — a `PATH` that reorders itself
/// between sessions makes a collision intermittent, which is the worst way for
/// one to present.
///
/// Only loaded bundles contribute. A bundle declared `enabled = false` comes back
/// on [`Plugins::disabled`] as a fully parsed plugin contributing nothing, and
/// putting its programs on `PATH` would be the one contribution a disabled bundle
/// still made.
pub fn entries(plugins: &Plugins) -> Vec<PathBuf> {
    let mut dirs = BTreeSet::new();
    for plugin in plugins.iter() {
        for (_, path) in plugin.bin() {
            if let Some(parent) = path.parent() {
                if !parent.as_os_str().is_empty() {
                    dirs.insert(parent.to_path_buf());
                }
            }
        }
    }
    dirs.into_iter().collect()
}

/// The declarations whose `name` is not the name their file answers to.
///
/// Returned as the declared name and the file's own name, for a surface to say
/// plainly. Nothing is placed under the declared name — appending a directory
/// puts a file on `PATH` under the name it already has — so a declaration that
/// renames its program does not resolve, and an operator reading `plugin list`
/// should be told that rather than left to discover it when the model reports
/// that a command does not exist.
pub fn mismatched(plugins: &Plugins) -> Vec<(String, String)> {
    let mut out = Vec::new();
    for plugin in plugins.iter() {
        for (declared, path) in plugin.bin() {
            let actual = path
                .file_name()
                .map(OsStr::to_string_lossy)
                .unwrap_or_default()
                .to_string();
            if !answers_to(&actual, declared) {
                out.push((declared.to_string(), actual));
            }
        }
    }
    out
}

/// Whether a file called `actual` is found by typing `declared`.
///
/// **The answer is the platform's, not this crate's opinion.** On unix a program
/// is found by its whole file name, extension included: `review.sh` is not
/// `review`. On Windows the shell appends each `PATHEXT` extension in turn, so a
/// bundle shipping `review.exe` and declaring `review` is correctly authored —
/// and it is the *only* thing it can ship there. Reporting that as a mismatch
/// would print a sentence that is false on the platform it is printed on, once
/// per `io exec`, on a door a script reads.
///
/// `PATHEXT` is read rather than assumed, because an operator may have added to
/// it; its documented default is used when it is unset or empty.
fn answers_to(actual: &str, declared: &str) -> bool {
    if actual == declared {
        return true;
    }
    if !cfg!(windows) {
        return false;
    }
    let Some(rest) = actual
        .get(..declared.len())
        .filter(|head| head.eq_ignore_ascii_case(declared))
        .and(actual.get(declared.len()..))
    else {
        return false;
    };
    let pathext = std::env::var("PATHEXT").unwrap_or_default();
    let pathext = if pathext.trim().is_empty() {
        DEFAULT_PATHEXT.to_string()
    } else {
        pathext
    };
    pathext
        .split(';')
        .any(|ext| !ext.is_empty() && rest.eq_ignore_ascii_case(ext))
}

/// What Windows uses when `PATHEXT` is unset, per its own documentation.
const DEFAULT_PATHEXT: &str = ".COM;.EXE;.BAT;.CMD;.VBS;.JS;.WS;.MSC";

/// `current` with `dirs` appended, skipping any already present.
///
/// Returns `None` when there is nothing to add, so a caller can tell "no bundle
/// declares a program" from "the variable was rewritten with the same value" and
/// leave the environment alone in the first case.
///
/// Idempotent by construction: an entry already on `PATH` is not appended a second
/// time, so re-resolving a session's bundles cannot grow the variable without
/// bound. That matters because the inventory is re-read at every turn boundary.
pub fn appended(current: Option<&OsStr>, dirs: &[PathBuf]) -> Option<OsString> {
    let existing: Vec<PathBuf> = current
        .map(|value| std::env::split_paths(value).collect())
        .unwrap_or_default();
    let mut all = existing;
    let before = all.len();
    for dir in dirs {
        if !all.contains(dir) {
            all.push(dir.clone());
        }
    }
    if all.len() == before {
        return None;
    }
    // **A join that fails is not "nothing to add", and the two must not wear the
    // same value.** A directory containing the platform's own separator cannot be
    // put on `PATH` at all; swallowing that into `None` would place *no* bundle's
    // program and leave the operator with "command not found" from the model and
    // nothing said. `install` reports it.
    std::env::join_paths(all).ok()
}

/// Resolve `config`'s bundles and place their programs, reporting any mismatch.
///
/// **The headless doors' way in, and they need their own because they leave
/// before the session's does.** `io exec` and `io resume` return from `main` long
/// before the interactive path resolves anything, so a placement written once on
/// the session's startup reaches neither — which is this product's two-entry-point
/// asymmetry, and it was caught here by a live run rather than by the suite: the
/// model found the program by walking the workspace and reported honestly that it
/// was *not* on `PATH`, so a check that grepped for the program's own output
/// would have passed over a placement that never happened.
pub fn install_for(config: &io_harness::config::Config) -> Vec<String> {
    let holdings = crate::resolved::Resolved::load(config);
    notices_for(holdings.loaded())
}

/// Place every loaded bundle's programs and return everything worth saying.
///
/// Takes the resolved set rather than a `Config` so a caller that already holds
/// one does not pay for a second full parse of every declared manifest — which is
/// what `src/resolved.rs` exists to prevent.
pub fn notices_for(plugins: &Plugins) -> Vec<String> {
    let mut notices: Vec<String> = mismatched(plugins)
        .into_iter()
        .map(|(declared, actual)| mismatch_notice(&declared, &actual))
        .collect();
    notices.extend(install(plugins));
    notices
}

/// What an operator is told when a declaration renames its own program.
///
/// One sentence, written once, so the session and the two headless doors say the
/// same thing about the same manifest.
pub fn mismatch_notice(declared: &str, actual: &str) -> String {
    format!(
        "a bundle declares the program {declared} but ships it as {actual} — io puts the \
         directory on the path and writes nothing, so it answers to {actual} and not to {declared}",
    )
}

/// Put every loaded bundle's program directory on this process's `PATH`.
///
/// The process's own variable and not a field on a contract, because that is
/// where io-harness looks: its executable resolver reads `PATH` out of the
/// environment and the commands it spawns inherit it. `src/home.rs` sets io-cli's
/// configuration home the same way and for the same reason.
///
/// Returns nothing when every directory was already there — which is the
/// ordinary case at a turn boundary — and a sentence when a directory could not
/// be put on `PATH` at all.
pub fn install(plugins: &Plugins) -> Option<String> {
    let dirs = entries(plugins);
    if dirs.is_empty() {
        return None;
    }
    let current = std::env::var_os(PATH_VAR);
    let already = dirs.iter().all(|dir| {
        current
            .as_deref()
            .map(|value| std::env::split_paths(value).any(|on| &on == dir))
            .unwrap_or(false)
    });
    match appended(current.as_deref(), &dirs) {
        Some(value) => {
            std::env::set_var(PATH_VAR, value);
            None
        }
        // Nothing added and nothing already there means the join refused it, and
        // the operator hears about it rather than meeting it as a missing command.
        None if !already => Some(format!(
            "a bundle's program directory could not be put on {PATH_VAR} — a path \
             holding this platform's own separator cannot go on it, so no bundle's \
             program is reachable by name in this session",
        )),
        None => None,
    }
}
