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
//! `[instructions]`' business and the caller's.** [`install`] is where that
//! business is settled, and [`view`] is how a surface reports the outcome
//! honestly rather than optimistically.
//!
//! # Appending, editing, and the one rule all three verbs follow
//!
//! Through 0.29.0 everything here was an append and this paragraph said so. It no
//! longer is: [`amend`] replaces one line and [`forget`] removes one. What has not
//! changed is why the paragraph exists. These files are the operator's own prose —
//! a person wrote them, another agent may have written into them, and a release of
//! this crate rewriting one is a release that eats somebody's notes.
//!
//! So the rule the two new verbs follow is **splice, never rewrite**: the file is
//! read as bytes, exactly one byte range is replaced, and every other byte is
//! written back as it was found — the indent somebody used, the `*` they wrote
//! where this module writes `-`, the `\r\n` a Windows checkout holds, and a last
//! line with no newline after it. A rewrite assembled from the parsed lines is the
//! obvious implementation and it normalises all four in silence. [`remember`] is
//! still an append, and its `prelude` is the same rule reached from the other end:
//! the only byte it ever adds ahead of its own bullet is a newline the previous
//! author left off.

use std::io::Write as _;
use std::path::{Path, PathBuf};

use io_harness::config::{Config, Scope};

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
    if let Some(parent) = path
        .parent()
        .filter(|parent| !parent.as_os_str().is_empty())
    {
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

// ---------------------------------------------------------------------------
// Reading the bullets back, and the two verbs on one of them
// ---------------------------------------------------------------------------

/// One bullet in one guidance file, and the address of the line it is on.
///
/// **The address is read back out of the file's own bytes and is never a row
/// index**, which is the shape [`crate::servers::At`] already has for the same
/// reason: the list a surface draws is not the list in the file. The header is
/// not a bullet, the blank line under it is not a bullet, and a picker filters —
/// so "the third row" and "the third line" name different lines, and acting on
/// the difference edits somebody else's instruction. `line` is therefore private
/// and [`notes`] is the only thing that can make one.
///
/// [`scope`](Note::scope) rides along for the second half of that: `AGENTS.md`
/// and `IO.md` are different files that both have a line 4, and a position lifted
/// from one and applied to the other would be a write into a file the operator
/// was not looking at.
///
/// `text` is carried for a third reason, and it is the one that survives a race:
/// the file can be edited — by hand, by another agent, by a second io — between
/// the page being drawn and the verb being chosen. [`amend`] and [`forget`]
/// compare it against what is on that line now and refuse when the two differ, so
/// a stale address is a refusal rather than a silently mangled note.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Note {
    /// Which of the three files holds it, and therefore who else reads it.
    pub scope: Scope,
    /// The bullet's own text: the indent and the `-` are the file's, not the
    /// note's, and neither is offered for editing.
    pub text: String,
    /// Where the line is, counting from zero. Private — see the type note.
    line: usize,
    /// How many lines below this bullet belong to it as a lazy continuation.
    ///
    /// **Zero for almost every note, and very much not zero for the ones
    /// [`crate::import`] writes.** `import` brings a foreign tool's whole
    /// instructions file across in a *single* [`remember`] call: one bullet
    /// reading `imported from …:` followed by the entire document at column zero.
    /// Markdown treats every following non-blank, non-bullet line as part of that
    /// list item, so it is one note — and [`forget`] on it removes one *line*,
    /// leaving the imported document behind as loose prose that is still read into
    /// every prompt and no longer inside any list item.
    ///
    /// Recorded rather than fixed by widening `forget`, because a verb that
    /// sometimes deletes one line and sometimes deletes four hundred is a worse
    /// answer than a surface that says which one this is.
    carries: usize,
}

impl Note {
    /// The line's number as an editor counts them, for a sentence that names it.
    ///
    /// From one, because the only reader of this number is a person with the file
    /// open somewhere else, and every editor ever written counts from one.
    #[must_use]
    pub fn numbered(&self) -> usize {
        self.line + 1
    }

    /// How many further lines this bullet carries with it — see the field.
    ///
    /// A surface offering [`forget`] on a note where this is non-zero is offering
    /// a verb that does a fraction of what it looks like it does, and has to say
    /// so.
    #[must_use]
    pub fn carries(&self) -> usize {
        self.carries
    }
}

/// The bullets one scope's file holds, each one addressable.
///
/// Empty for a file that is not there, and empty for one this process cannot
/// read — the same answer [`view`] gives, for the same reason: [`Instruction`]
/// already reports `exists` beside this list, and a second opinion about whether
/// the file is there is a second thing that has to be kept in agreement.
///
/// **Only bullets are notes.** The header this module writes, the blank lines
/// around it and whatever else a person put in the file are not offered: a
/// surface that let an operator "edit" the file's own heading would be a text
/// editor, and this is a list of instructions. [`Instruction::lines`] still
/// counts every line, and the two numbers are allowed to differ because they
/// answer different questions.
#[must_use]
pub fn notes(root: &Path, scope: Scope) -> Vec<Note> {
    let Some(path) = path(root, scope) else {
        return Vec::new();
    };
    let Ok(text) = std::fs::read_to_string(&path) else {
        return Vec::new();
    };

    let lines = spans(&text);
    // A line that is neither blank nor a bullet of its own belongs to the bullet
    // above it — markdown's lazy continuation, and the shape `import` writes.
    let continues = |span: &std::ops::Range<usize>| {
        let body = content(&text, span);
        let line = &text[body.start..body.end];
        !line.trim().is_empty() && bullet(line).is_none()
    };

    lines
        .iter()
        .enumerate()
        .filter_map(|(line, span)| {
            let body = content(&text, span);
            let from = bullet(&text[body.start..body.end])?;
            Some(Note {
                scope,
                text: text[body.start + from..body.end].to_string(),
                line,
                carries: lines
                    .iter()
                    .skip(line + 1)
                    .take_while(|s| continues(s))
                    .count(),
            })
        })
        .collect()
}

/// Replace one note's text, leaving every other byte of the file alone.
///
/// The indent, the marker and the line ending are the file's own bytes and are
/// not touched: a note somebody wrote as `  * check the lockfile` stays indented
/// and stays a `*`, because this module is editing an instruction and not
/// reformatting a document it did not write.
///
/// Refuses a replacement that is empty or nothing but whitespace, for the reason
/// [`remember`] refuses one: a note quietly blanked reports success and leaves the
/// operator with a bullet that says nothing. Refuses one holding a line break
/// too — a note is a line, and a replacement that split it in two would put a line
/// in the file that [`notes`] cannot address.
pub fn amend(root: &Path, note: &Note, line: &str) -> Result<PathBuf, String> {
    let text = line.trim();
    if text.is_empty() {
        return Err("there is nothing to remember".to_string());
    }
    if text.contains('\n') {
        return Err("a note is one line, and that one holds a line break".to_string());
    }
    splice(root, note, Some(text))
}

/// Remove one note's line entirely, leaving every other byte of the file alone.
///
/// The line and its terminator go together, so the bullet above it does not
/// acquire a blank line and the file does not grow one. Everything else — the
/// header, the prose, every sibling bullet — is the bytes that were there.
pub fn forget(root: &Path, note: &Note) -> Result<PathBuf, String> {
    splice(root, note, None)
}

/// The one write both verbs go through: replace one byte range, keep the rest.
///
/// `Some(text)` replaces the note's own text and keeps its line; `None` takes the
/// line and its terminator away.
///
/// **The file is spliced, never rebuilt.** Reading it into lines, changing one and
/// writing them back joined is the obvious implementation, and it silently
/// normalises four things this crate has no business normalising: a `\r\n`
/// checkout, a last line with no newline after it, an indent, and a `*` written
/// where this module writes `-`. Two of those change what io-harness reads at the
/// start of every run, and none of them would show up as an error.
///
/// Refuses when the line has moved. See [`Note`]: the address is only as good as
/// the read it came from, and the file is one a person and another agent are both
/// entitled to be editing.
fn splice(root: &Path, note: &Note, replacement: Option<&str>) -> Result<PathBuf, String> {
    let path = path(root, note.scope)
        .ok_or_else(|| "there is no path for that scope on this machine".to_string())?;
    // ponytail: the whole file, for the same reason `remember` reads the whole
    // file to look at its last byte — these are guidance files a person maintains
    // by hand, and they are kilobytes. Read once and written once, so nothing
    // here can half-write a file.
    let mut text =
        std::fs::read_to_string(&path).map_err(|error| format!("{}: {error}", path.display()))?;

    // Built up front rather than in three closures, so the sentence an operator
    // reads is one sentence and not three that drifted apart.
    let stale = format!(
        "{} line {} no longer says `{}`, so nothing was written — it has been edited since \
         the page was drawn",
        path.display(),
        note.numbered(),
        note.text,
    );

    let span = spans(&text)
        .get(note.line)
        .cloned()
        .ok_or_else(|| stale.clone())?;
    let body = content(&text, &span);
    let from = body.start + bullet(&text[body.start..body.end]).ok_or_else(|| stale.clone())?;
    if text[from..body.end] != *note.text {
        return Err(stale);
    }

    match replacement {
        Some(line) => text.replace_range(from..body.end, line),
        None => text.replace_range(span, ""),
    }
    std::fs::write(&path, text).map_err(|error| format!("{}: {error}", path.display()))?;
    Ok(path)
}

/// Every line of `text` as a byte range **including its terminator**.
///
/// `split_inclusive` rather than `lines`, because the terminator is exactly the
/// byte a splice must not invent or drop. It also counts the way `lines` counts —
/// a file ending in one newline has one line, not two — so a [`Note`]'s line
/// number and [`Instruction::lines`] are numbers in the same system.
fn spans(text: &str) -> Vec<std::ops::Range<usize>> {
    let mut at = 0;
    text.split_inclusive('\n')
        .map(|line| {
            let span = at..at + line.len();
            at = span.end;
            span
        })
        .collect()
}

/// One line's range with its terminator taken off, whichever of the two it is.
///
/// `\r\n` and `\n` both, and neither invented: a file that had no newline on its
/// last line still has none afterwards, which is the fixture the splice rule
/// exists for.
fn content(text: &str, span: &std::ops::Range<usize>) -> std::ops::Range<usize> {
    let mut end = span.end;
    if text[span.start..end].ends_with('\n') {
        end -= 1;
        if text[span.start..end].ends_with('\r') {
            end -= 1;
        }
    }
    span.start..end
}

/// Where a markdown bullet's own text starts, if the line is a bullet at all.
///
/// `-`, `*` or `+` after any indent, then whitespace or the end of the line —
/// which is markdown's own rule and is what keeps `---` and a word like
/// `-Werror` out of the list. The three markers rather than the one this module
/// writes, because the operator's file is the operator's: a list they wrote with
/// `*` is a list, and offering them only the bullets io wrote would be a surface
/// that hides half the file it is claiming to show.
///
/// ponytail: no block state, so a `- like this` inside a fenced code block reads
/// as a bullet. It is offered for editing and edits correctly; it is only listed
/// where it should not be. Track the fences if an operator's file ever turns up
/// in a bug report over it.
fn bullet(line: &str) -> Option<usize> {
    let space = |c: char| c == ' ' || c == '\t';
    let body = line.trim_start_matches(space);
    let indent = line.len() - body.len();
    let after = body.strip_prefix(['-', '*', '+'])?;
    if !after.is_empty() && !after.starts_with(space) {
        return None;
    }
    Some(indent + 1 + (after.len() - after.trim_start_matches(space).len()))
}

// ---------------------------------------------------------------------------
// Making io-harness read all three
// ---------------------------------------------------------------------------

/// The order the three files are named in, and it is the order they reach the
/// model in: `read_instructions` pushes one constraint per name in list order
/// (`io-harness-0.69.0/src/config.rs:1884-1899`).
///
/// Project, then local, then user — widest audience first, so the operator's own
/// standing note is the last thing said. It also matches the order the three
/// headers in [`header`] are worth reading in.
const ORDER: [Scope; 3] = [Scope::Project, Scope::Local, Scope::User];

/// How a scope's file has to be spelled inside `[instructions] files`.
///
/// **Not the same thing as [`path`], and the difference is the whole reason
/// `IO.md` needs its own case.** io-harness resolves every name it is given with
/// `root.join(&name)` against the discovery root (`config.rs:1885`), so a bare
/// name means "in the workspace". That is exactly right for the two project
/// files and unreachable for `IO.md`, which lives in io-cli's home — a directory
/// that is not the workspace and has no relative spelling from it. So the user
/// entry is the **absolute** path.
///
/// Absolute and not `${env:HOME}/.io-cli/IO.md`, which would read as the tidier
/// answer and is a trap: the harness does substitute `${env:…}`, and an unset
/// variable is a **hard error** that fails the whole parse (`config.rs:1983-1989`).
/// On Windows `HOME` is routinely unset. A configuration file that refuses to
/// parse is a session that does not start, and it would fail on the machine of
/// whoever copied the file rather than on the machine that wrote it. `~` is not
/// an option either: the harness expands it nowhere.
fn entry(root: &Path, scope: Scope) -> Option<PathBuf> {
    match scope {
        Scope::Project | Scope::Local => Some(PathBuf::from(file_name(scope))),
        Scope::User => path(root, scope),
    }
}

/// Every name `[instructions] files` has to hold for all three files to be read.
///
/// **`AGENTS.md` is always in it, and that is a correctness requirement rather
/// than a courtesy.** `files` REPLACES the default list, it does not add to it:
/// `read_instructions` falls back to `DEFAULT_INSTRUCTIONS` — exactly
/// `["AGENTS.md"]` (`config.rs:158`) — only when the table is absent or names
/// nothing, and otherwise takes `Some(files) => files.clone()` verbatim
/// (`config.rs:1879-1882`). So a list written to reach `AGENTS.local.md` and
/// `IO.md` and no more would **silently stop the repository's own `AGENTS.md`
/// being read** — no error, no warning, just a model that no longer knows the
/// project's rules. Nothing on the surface would show it, because a missing
/// instruction file is skipped without a word (`config.rs:1886`).
///
/// Two entries rather than three where there is no home to put `IO.md` in, which
/// is the same answer [`path`] gives for the same reason. The project pair never
/// goes missing: the root is the directory the session is already running in.
#[must_use]
pub fn files(root: &Path) -> Vec<PathBuf> {
    ORDER
        .into_iter()
        .filter_map(|scope| entry(root, scope))
        .collect()
}

/// What `[instructions] files` currently says in one file, as that file spells it.
///
/// Read from the file's own bytes rather than from a merged [`Config`], because
/// [`install`] needs to know whether **this scope** already holds the list — a
/// merged view cannot tell "the user file says it" from "some other file says
/// it", and writing on the strength of the second would rewrite the user file on
/// every call.
///
/// It goes through [`crate::edit::value_at`] rather than parsing here, and that
/// is a boundary rather than a convenience: `tests/dependencies.rs` permits TOML
/// in `src/edit.rs` alone, because a second module that parses a configuration
/// file is a second opinion about what one means. This module decides *which
/// files* belong in the list; `edit` decides how a list is spelled. The value
/// comes back as its own source text, so the comparison in [`install`] is
/// between the bytes that are there and the bytes that would be written.
fn declared(path: &Path) -> Option<String> {
    let text = std::fs::read_to_string(path).ok()?;
    crate::edit::value_at(&text, "instructions.files")
}

/// Name all three files in the **user** scope, and report whether that changed
/// anything.
///
/// # Why the user scope, and never a project one
///
/// The list contains an absolute path into this operator's home. A project
/// `io.toml` is committed, so writing it there would put one person's
/// `/Users/somebody/.io-cli/IO.md` into everybody else's checkout: harmless to
/// them only because the file does not exist and is skipped in silence
/// (`config.rs:1886`), and a leak of the operator's account name to every reader
/// of the repository regardless. `~/.io-cli/io.toml` is the one file in this
/// product that is never committed, so it is the only honest place for a path
/// that is true on exactly one machine.
///
/// The cost is stated rather than hidden: `["instructions","files"]` is not in
/// io-harness's `APPENDING` set (`config.rs:2052`), so a project or local file
/// that names `files` itself replaces this list wholesale rather than adding to
/// it — the scopes are merged in the order they are listed at
/// `config.rs:688-693`, and a later one's value simply overwrites
/// (`config.rs:2142-2144`), so Local > Project > User. That is the
/// harness's rule and this module does not fight it — [`view`] reports the
/// result instead, so an operator whose project overrode the list sees that it
/// did rather than being told all three are read.
///
/// # Idempotence
///
/// `Ok(false)` and not one byte written when the list is already exactly right.
/// This is reached from a command an operator types repeatedly, and a write per
/// invocation would churn `io.toml`'s bytes, its mtime and any backup watching
/// it for a change that never happened.
///
/// The comparison is on the value's **source text**, so a list an operator wrote
/// by hand with different spacing is rewritten once into this crate's spelling
/// and is stable from then on. That is the honest trade for not parsing here: one
/// write that changes no meaning, rather than a second TOML reader in a module
/// that has no business being one.
pub fn install(root: &Path) -> Result<bool, String> {
    let want = files(root);
    let at = crate::configure::scope_path(root, Scope::User)
        .ok_or_else(|| "there is no path for that scope on this machine".to_string())?;

    let mut names = Vec::with_capacity(want.len());
    for name in &want {
        // A TOML file is text, so a path that is not text cannot go in one. Said
        // out loud rather than papered over with `to_string_lossy`, which would
        // write a path with U+FFFD in it that resolves to nothing and is skipped
        // in silence — the failure this module keeps refusing to ship.
        names.push(name.to_str().ok_or_else(|| {
            format!(
                "{} is not valid UTF-8, and a TOML file cannot name it",
                name.display()
            )
        })?);
    }

    let value = crate::edit::array(&names);
    if declared(&at).is_some_and(|found| found == value) {
        return Ok(false);
    }

    let edits = [crate::edit::Edit::set("instructions.files", value)];
    crate::configure::write(root, Scope::User, &edits)?;
    Ok(true)
}

/// Which files io-harness **actually read**, for a configuration it already
/// resolved.
///
/// Recovered from [`Config::instructions`] rather than by re-deriving the
/// precedence rules, and that choice is the point: every entry is wrapped
/// `"Project instructions from `{name}`:\n{text}"` (`config.rs:1895-1898`), so
/// the list of names is right there in the strings the harness built. Reading it
/// back says what **was read**; re-implementing the merge would say what io-cli
/// believes it asked for, and the two differ in exactly the case a surface
/// exists to show — a project file that replaced the list.
///
/// `name` is the name as it was written in `files`, not the resolved location,
/// so it is joined to the discovery root here the same way `config.rs:1885`
/// joins it. `Path::join` returns an absolute argument unchanged, so the one
/// expression covers both the two relative names and `IO.md`'s absolute one.
fn reading(root: &Path, config: &Config) -> Vec<PathBuf> {
    config
        .instructions()
        .iter()
        .filter_map(|text| {
            let rest = text.strip_prefix("Project instructions from `")?;
            // ponytail: an exact match on the name the harness recorded. A
            // configuration naming `./AGENTS.md` reads as a different file here
            // and would be reported unread while being read. Nothing this crate
            // writes spells a name that way; normalise if an operator's own
            // `files` ever turns up in a bug report.
            let (name, _) = rest.split_once("`:\n")?;
            Some(root.join(name))
        })
        .collect()
}

/// One guidance file, as a surface has to show it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Instruction {
    /// Which of the three this is, and therefore who else reads it.
    pub scope: Scope,
    /// Where the file is. Absolute for [`Scope::User`], under the session root
    /// for the other two.
    pub path: PathBuf,
    /// Whether there is a file there at all.
    pub exists: bool,
    /// How many lines it holds. `0` for a file that is not there, which `exists`
    /// is what distinguishes from a file that is there and empty.
    pub lines: usize,
    /// **Whether io-harness is reading it**, recovered from what it actually
    /// read rather than from what was configured — so a project file that
    /// replaced the list shows up here as `false` instead of being argued away.
    pub read: bool,
}

/// All three files, what each holds, and which of them the session is really
/// reading.
///
/// The two right-hand columns are deliberately independent. A file that exists
/// and is not being read is the whole reason this exists, and it happens for
/// three ordinary causes: a project `[instructions] files` replaced the user
/// list (`config.rs:2052` — this key does not append), [`install`] was never
/// run, or the file holds nothing but whitespace and was skipped
/// (`config.rs:1892`). A view that inferred `read` from `exists` would report
/// all three green in every one of those cases, which is worse than no view: it
/// would be the surface an operator trusts while their `AGENTS.md` goes unread.
#[must_use]
pub fn view(root: &Path, config: &Config) -> Vec<Instruction> {
    let reading = reading(root, config);

    ORDER
        .into_iter()
        .filter_map(|scope| {
            let at = path(root, scope)?;
            let text = std::fs::read_to_string(&at).ok();
            Some(Instruction {
                scope,
                // `is_file`, the same question io-harness asks at
                // `config.rs:1886`, and not "did the read succeed": a file this
                // process cannot read is still there, and reporting it absent
                // would send the operator to create one that already exists.
                exists: at.is_file(),
                lines: text.as_deref().map_or(0, |text| text.lines().count()),
                read: reading.contains(&at),
                path: at,
            })
        })
        .collect()
}
