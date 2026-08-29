//! Bringing a marketplace onto the disk — the one `git` this crate runs.
//!
//! **This is the second module in `src/` permitted a process spawn, and
//! `tests/dependencies.rs` names it by path to permit it.** The gate is amended
//! rather than relaxed: the permitted set is a set of *exact paths*, a third file
//! naming the literal still fails and fails naming both permitted paths, and the
//! aliased spellings stay banned everywhere — this module included, which is why
//! the import below is written out one name per line. The shape is 0.17.0's and
//! 0.26.0's amendment of the provider gate: an exemption by path, held to
//! properties that are asserted rather than promised.
//!
//! **Why a spawn at all.** io-harness owns git and publishes no way to run it:
//! `Git`, `Git::run` and `GitCmd` are all `pub(crate)` in its `tools/git.rs`, and
//! that file's own tests assert that no argv the harness can build ever carries
//! `clone`, `fetch` or `push`. Its engine exists for a workspace the agent is
//! already inside, not for bringing a new one down, so there is no governed call
//! to reach for. The alternative is an HTTP client — an eleventh direct
//! dependency, a TLS stack under it, and a second network path beside the one
//! io-harness owns, which is the thing this product exists not to grow. Cloning
//! also makes updating a marketplace a pull and pinning one a ref rather than two
//! features this crate would otherwise have to invent.
//!
//! `product.yaml:253-258` records the price honestly: that `git` is present is an
//! **assumption about developer machines, not a fact this repository has
//! checked**. So a machine without it gets [`Fetched::NoGit`] and the sentence
//! [`Fetched::sentence`] writes, never a panic and never a stack trace — and the
//! route that installs a plugin from a directory the operator already has does not
//! go through here at all.
//!
//! **The argv is a value, not a string.** [`argv`] is a pure function returning
//! five owned elements — `clone`, `--depth`, `1`, the URL, the destination — and
//! nothing in this file ever splices a name, a URL or a path into a larger string
//! for a program to take apart again. No shell is invoked, so there is no quoting
//! question to get wrong and no metacharacter in a repository name to think about.
//! `tests/fetch.rs` asserts the URL and the path arrive as their own elements,
//! byte for byte, rather than trusting that they do, and `tests/dependencies.rs`
//! asserts this file builds no argument with `format!` — which is the sabotage F10
//! names by name.
//!
//! **Nothing io-harness drives can reach this.** The module names none of the
//! types the event stream, the conversation or the trace are made of, exactly as
//! `src/shell.rs` names none of them, and `tests/dependencies.rs` asserts that of
//! both permitted modules in one loop. A marketplace is fetched because the
//! operator typed a name.
//!
//! **A failure at any point leaves no half-cloned directory presented as a
//! marketplace, and the mechanism is a rename rather than a cleanup.** git clones
//! into `~/.io-cli/.fetching/<pid>` and the finished tree is moved into
//! `~/.io-cli/marketplaces/<owner>/<repo>` in one step. Both paths are inside the
//! operator's own home, so the move is a same-filesystem rename and there is no
//! cross-device copy that could itself stop half way.
//!
//! Removing the destination on failure would have been fewer lines, and it covers
//! every failure this process survives to see. It covers none of the ones it does
//! not: a `kill -9`, a panic in another thread, or the machine losing power in the
//! middle of a clone is exactly how a directory holding half a `plugin.toml` ends
//! up under a path the listing walks and the installer trusts. The staging
//! directory is dot-named and sits *outside* the marketplaces tree for the same
//! reason — whatever is left in it after a kill can never be counted as a
//! marketplace, because nothing walks it.

use std::ffi::OsString;
use std::path::{Path, PathBuf};

// One name per line, and `std::process::Command` written out in full. A braced or
// aliased import would put a spawn in a file where the literal never appears,
// which is the evasion `tests/dependencies.rs` forbids in every file — this one
// included, because a permission that can be spelled around is not a permission.
use std::process::Command;
use std::process::Stdio;

/// The program, and the whole of it.
///
/// A constant rather than a literal at the call site so that
/// `tests/dependencies.rs` can assert both halves of the property F10 states: that
/// this is the string `git`, and that the single `Command::new` in this file takes
/// *this* and not a name computed from anything. A program that came from a
/// variable is how a spawn stops being a spawn of git.
pub const PROGRAM: &str = "git";

/// The only forge a bare `<owner>/<repo>` resolves against.
///
/// **A stated ceiling, not an oversight.** The release adds a marketplace *by
/// name*, and one host is what makes a name short enough to type and to say out
/// loud. An operator on another forge is not served by this release, and the
/// upgrade is one more accepted grammar in [`resolve`] — not a change here, and
/// not a URL passed through unread: an argument that reaches `git clone` having
/// been typed by somebody is how `ext::sh -c …` becomes a remote shell, and
/// [`resolve`] refuses everything that is not two ordinary path segments precisely
/// so that this string is the only host that can ever be reached.
const HOST: &str = "https://github.com/";

/// The variable that stops git asking for a credential on a terminal io-cli owns.
///
/// **A `/dev/null` stdin is not enough, and that is the whole reason this is
/// here.** git does not prompt on stdin: it opens `/dev/tty` directly, which is
/// still this process's terminal — in raw mode, with an inline viewport whose
/// absolute screen row was computed once and is maintained afterwards by
/// arithmetic alone. A credential prompt written there scrolls the screen behind
/// ratatui's back, and on this renderer the transcript *is* the scrollback and
/// cannot be redrawn, which is the damage `src/shell.rs` documents at length as
/// its reason never to hand the terminal over. Setting this makes git fail with a
/// sentence instead, and that sentence arrives here as the clone's own stderr.
///
/// It is an environment variable and not an argument, so the argv stays exactly
/// the five elements [`argv`] returns.
const NO_PROMPT: &str = "GIT_TERMINAL_PROMPT";

/// What an operator is told when there is no git to run.
///
/// Named, and about their machine rather than about this program. The second
/// sentence is the part that matters: this is the *only* thing io-cli does that
/// needs git, so the answer is not "io-cli does not work here".
const NO_GIT: &str = "no `git` on PATH — a marketplace is a repository io-cli clones, so \
                      adding one needs git installed. Installing a plugin from a directory \
                      you already have does not.";

/// A marketplace named the way an operator says it: an owner and a repository.
///
/// Two owned strings rather than one borrowed `&str`, because both halves become
/// path components and an argv element that outlives the text they were parsed
/// from. Kept separate rather than stored joined so that neither the URL nor the
/// destination has to take a string apart again to find them.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Named {
    /// The account or organisation that holds the repository.
    pub owner: String,
    /// The repository itself, with no `.git` on the end — [`resolve`] takes it
    /// off, so the name that reaches a path and the name an operator typed are
    /// the same name.
    pub repo: String,
}

/// What a fetch did.
///
/// Four endings, and each is a different sentence to a different person: it
/// worked; it was already here; this machine has no git; git ran and said no. The
/// last two are deliberately not one variant — "git could not be started" and
/// "git started and refused" are the difference between installing a tool and
/// fixing a name, and folding them together is how an operator is told to install
/// something they already have.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Fetched {
    /// The clone is in place, at this path.
    Cloned(PathBuf),
    /// Something is already at the destination, and it was left exactly as it
    /// was. **Never cloned over**: the directory may hold a marketplace an
    /// operator has installed plugins from, and a `[[plugin]] path` in their
    /// configuration points into it.
    Already(PathBuf),
    /// There is no `git` on `PATH`.
    NoGit,
    /// git ran and did not succeed — or could not be started for a reason that is
    /// not its absence, which is the same thing to the operator and a different
    /// thing to anyone reading a bug report.
    Failed {
        /// The exit code, or `None` where the platform reported none — which on
        /// unix means a signal ended it, and here also means the failure was not
        /// an exit at all.
        status: Option<i32>,
        /// What git printed, with control characters dropped.
        ///
        /// **Cleaned at construction rather than at the point it is drawn.** This
        /// is output from a program io-cli did not write, and on this renderer the
        /// scrollback is the transcript: a `\x1b[3J` inside it would erase the
        /// session's whole history. The same rule, and the same reason, as the
        /// captured output of a `!` line.
        stderr: String,
    },
}

impl Fetched {
    /// Which ending an [`std::io::Error`] from the spawn itself is.
    ///
    /// **The distinction is the whole reason this is a function.** `NotFound` from
    /// a spawn means the program is not on `PATH`, which is the one failure this
    /// release has to name in the operator's own words; every other kind —
    /// `PermissionDenied` on a `git` that is not executable, and everything the
    /// platform can invent — is a real error whose text is more useful than any
    /// sentence written here would be. Mapping them all to [`Fetched::NoGit`]
    /// would tell an operator with git installed to install git, and
    /// `tests/fetch.rs` asserts that it does not.
    #[must_use]
    pub fn unstartable(error: &std::io::Error) -> Self {
        if error.kind() == std::io::ErrorKind::NotFound {
            return Fetched::NoGit;
        }
        Fetched::Failed {
            status: None,
            stderr: error.to_string(),
        }
    }

    /// One line for the operator, or `None` where there is nothing to say.
    ///
    /// `None` for [`Fetched::Cloned`] on purpose: the surface that asked for the
    /// fetch says what it now has, and a success line written here as well would
    /// be the same fact twice in two vocabularies.
    ///
    /// Assembled by pushing rather than by interpolation, which is the same
    /// discipline the argv is held to and is asserted the same way — this file
    /// builds no string out of a name with `format!`, anywhere, so there is one
    /// rule to read rather than an exception to check.
    #[must_use]
    pub fn sentence(&self) -> Option<String> {
        match self {
            Fetched::Cloned(_) => None,
            Fetched::Already(here) => {
                let mut said = String::from("that marketplace is already here: ");
                said.push_str(&here.display().to_string());
                Some(said)
            }
            Fetched::NoGit => Some(String::from(NO_GIT)),
            Fetched::Failed { status, stderr } => {
                let mut said = String::from("git could not clone that");
                // The **last** non-blank line, because git narrates on stderr —
                // `Cloning into '…'` first, then a `fatal:` that says what
                // actually went wrong. Taking the first line would report the
                // narration and hide the reason.
                match stderr.lines().rev().find(|line| !line.trim().is_empty()) {
                    Some(reason) => {
                        said.push_str(": ");
                        said.push_str(reason.trim());
                    }
                    None => {
                        // A silent failure still gets a number, because "git could
                        // not clone that" on its own is a sentence an operator can
                        // do nothing with.
                        if let Some(code) = status {
                            said.push_str(" (exited ");
                            said.push_str(&code.to_string());
                            said.push(')');
                        }
                    }
                }
                Some(said)
            }
        }
    }
}

/// What an operator typed, if it is an owner and a repository.
///
/// **A pure function, and the only place a name is judged.** `None` means "that is
/// not a marketplace name" and never "that marketplace does not exist" — nothing
/// here touches the network or the disk, so this is answerable, and asserted, with
/// no fixture at all.
///
/// Accepted: `owner/repo`, with a trailing `/` and a trailing `.git` taken off,
/// because both are what a paste leaves behind. Everything else is refused,
/// including a full URL — a URL has more than two segments and lands in the same
/// refusal, which is the outcome that matters: the only string that can reach
/// `git clone` is one this function built out of `HOST`, this module's own
/// constant.
///
/// **Two of the refusals are the load-bearing ones.** A segment starting with `-`
/// would become an *option* the moment it were passed to a program that parses
/// options, whatever the argv looked like when it left here. A segment containing
/// `..` would leave the marketplaces directory the moment it were joined onto a
/// path. Neither is a hypothetical: both are what a name is for, if the person
/// choosing the name is choosing it against you.
#[must_use]
pub fn resolve(text: &str) -> Option<Named> {
    let text = text.trim().trim_end_matches('/');
    let (owner, repo) = text.split_once('/')?;
    let repo = repo.strip_suffix(".git").unwrap_or(repo);
    // A third segment is a URL, a path, or a name for something this release does
    // not have. Refused here rather than silently keeping the first two.
    if repo.contains('/') {
        return None;
    }
    if !segment(owner) || !segment(repo) {
        return None;
    }
    Some(Named {
        owner: owner.to_string(),
        repo: repo.to_string(),
    })
}

/// Whether one half of a name is a name at all.
///
/// The permitted alphabet is what a forge permits and no more: letters, digits,
/// `-`, `_` and `.`. Stated as what is *allowed* rather than as a list of what is
/// forbidden, which is the only direction that stays correct when somebody thinks
/// of a character nobody here did.
fn segment(text: &str) -> bool {
    !text.is_empty()
        && !text.starts_with('-')
        && !text.starts_with('.')
        && !text.contains("..")
        && text.chars().all(ordinary)
}

/// Whether one character may appear in a name.
fn ordinary(glyph: char) -> bool {
    glyph.is_ascii_alphanumeric() || glyph == '-' || glyph == '_' || glyph == '.'
}

/// The repository a name points at.
///
/// One element of the argv, built once, from a [`Named`] that [`resolve`] has
/// already judged — so there is no character in it that could mean anything to
/// anything downstream.
#[must_use]
pub fn url(named: &Named) -> String {
    let mut url = String::from(HOST);
    url.push_str(&named.owner);
    url.push('/');
    url.push_str(&named.repo);
    url.push_str(".git");
    url
}

/// Where a marketplace lives once it is here.
///
/// Two levels — `<owner>/<repo>` — rather than one flattened name, so two owners
/// may carry a repository of the same name and neither has to be qualified on
/// disk. `None` where the operator's home directory cannot be determined, which is
/// the answer [`crate::home::path`] gives for the same reason.
#[must_use]
pub fn at(named: &Named) -> Option<PathBuf> {
    Some(
        crate::home::marketplaces()?
            .join(&named.owner)
            .join(&named.repo),
    )
}

/// The argv, as five owned elements.
///
/// **This is the property F10 is about, and it is a function so that it can be
/// asserted rather than described.** `url` and `into` are handed through as whole
/// elements — not quoted, not escaped, not joined to anything, not spliced into a
/// line for something else to split again. There is no shell between here and the
/// program, so a space, a quote or a semicolon in either is a space, a quote or a
/// semicolon in one argument and cannot be anything else.
///
/// `OsString` rather than `String` because a destination is a path, and a path on
/// neither unix nor Windows is guaranteed to be UTF-8. Converting it to a `String`
/// to build an argument would be the first step of exactly the interpolation this
/// function exists to make impossible.
#[must_use]
pub fn argv(url: &str, into: &Path) -> Vec<OsString> {
    vec![
        OsString::from("clone"),
        // Shallow, which is the whole of the bound: a marketplace is read for the
        // directories in it, and its history is somebody else's.
        OsString::from("--depth"),
        OsString::from("1"),
        OsString::from(url),
        into.as_os_str().to_os_string(),
    ]
}

/// Clone `url` into `into`, assembling it at `staging` first.
///
/// Both paths are the caller's, which is what lets `tests/fetch.rs` drive every
/// ending of this function against a temporary directory and a local repository —
/// a decision that lives in the driver is a decision nothing under `tests/` can
/// reach, and this crate has shipped that mistake before.
///
/// The order is the property: an existing destination is answered **before**
/// anything is spawned, so a marketplace an operator already has is never cloned
/// over and never even risked; the staging directory is cleared before the clone,
/// because a previous run that was killed may have left one; and it is cleared
/// again afterwards whatever happened, so the only thing that can survive this
/// call is a complete tree at `into`.
pub fn clone(url: &str, into: &Path, staging: &Path) -> Fetched {
    if into.exists() {
        return Fetched::Already(into.to_path_buf());
    }

    // Best effort, and deliberately not fatal on its own: git creates the leading
    // directories of its destination itself, and if it cannot, its own error is a
    // better sentence than one invented here.
    if let Some(parent) = staging.parent() {
        let _ = crate::home::create(parent);
    }
    // Whatever a killed run left. git refuses a destination that exists and is not
    // empty, so this is not tidiness — without it the second attempt after a crash
    // fails for a reason that has nothing to do with the marketplace.
    let _ = std::fs::remove_dir_all(staging);

    // All three streams are spelled out even though `output()` already wires them
    // this way. The wiring is the property that keeps the viewport safe — the
    // child gets no tty and writes nothing to the screen — and a reader checking
    // that should not have to know a default.
    let output = Command::new(PROGRAM)
        .args(argv(url, staging))
        .env(NO_PROMPT, "0")
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .output();

    let fetched = match output {
        Err(error) => Fetched::unstartable(&error),
        Ok(output) if output.status.success() => match settle(staging, into) {
            Ok(()) => Fetched::Cloned(into.to_path_buf()),
            // A clone that worked and a move that did not is still a failure with
            // nothing at the destination, which is the only shape this module
            // promises. The reason is the filesystem's own words.
            Err(error) => Fetched::Failed {
                status: None,
                stderr: error.to_string(),
            },
        },
        Ok(output) => Fetched::Failed {
            status: output.status.code(),
            stderr: readable(&output.stderr),
        },
    };

    // Unconditional, and a no-op on the path that succeeded — the rename already
    // took the directory away. On every other path this is what makes the promise
    // true for the failures this process lives to see; the rename is what makes it
    // true for the ones it does not.
    let _ = std::fs::remove_dir_all(staging);
    fetched
}

/// Move a finished clone into place.
///
/// A rename, and the reason it can be one is that both paths are inside the
/// operator's own home: there is no cross-device case to fall back from, so unlike
/// [`crate::home`]'s own mover this needs no copy-and-verify path. The
/// destination's parent is created first, because a rename onto a path whose
/// parent does not exist fails on every platform.
fn settle(staging: &Path, into: &Path) -> std::io::Result<()> {
    if let Some(parent) = into.parent() {
        crate::home::create(parent)?;
    }
    std::fs::rename(staging, into)
}

/// Captured stderr, made safe to put in a terminal.
///
/// Text and only text: the control characters go, for the reason
/// [`Fetched::Failed`]'s field documents. Line structure is kept, because
/// [`Fetched::sentence`] reads the last line and a single squashed string has no
/// last line.
fn readable(bytes: &[u8]) -> String {
    String::from_utf8_lossy(bytes)
        .lines()
        .map(|line| line.chars().filter(|glyph| !glyph.is_control()).collect())
        .collect::<Vec<String>>()
        .join("\n")
}

/// Fetch the marketplace `named` into the operator's own home.
///
/// `None` where the home cannot be located — the answer [`crate::home::path`]
/// gives, for the reason it gives it: a program that invents a path when it has no
/// home writes into somebody else's.
///
/// **The staging directory carries this process's own id, and that is not
/// belt-and-braces.** io-cli's session lock is keyed on a conversation rather than
/// on the home (`src/lock.rs`), so two terminals in two repositories are two
/// processes that contend over nothing and share one `~/.io-cli`. A fixed staging
/// path would have them assembling two clones in one directory, and the one that
/// finished second would find the first one's tree.
pub fn fetch(named: &Named) -> Option<Fetched> {
    let into = at(named)?;
    let staging = crate::home::staging()?.join(std::process::id().to_string());
    Some(clone(&url(named), &into, &staging))
}
