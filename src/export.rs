//! The conversation as markdown, and the run's canonical trace, verbatim.
//!
//! A session is a conversation that ends when the terminal closes. The review
//! that matters usually happens somewhere else — in a pull request, in a text
//! editor, in a message to somebody who was not there — and until this release
//! there was no way to get it out. `/export` writes two files: the conversation
//! for a human to read, and one run's canonical trace for a machine to compare.
//!
//! # The trace is written verbatim, and that is the whole rule
//!
//! [`Store::canonical_trace`] returns a string whose entire value is that it is
//! **canonical**: io-harness excludes wall-clock stamps, measured durations, the
//! argv's ephemeral tempdir path and `AUTOINCREMENT` ids from it precisely so
//! that two runs of one case can be compared, and its documentation says that
//! excluding a field is a decision the crate cannot promise rather than a
//! convenience.
//!
//! So this module does not parse it, does not reserialise it, does not
//! pretty-print it and adds no field of its own. A trace io-cli reformatted
//! compares against nothing, which is why `tests/export.rs` asserts a
//! **byte-identical** string.
//!
//! **It is not JSON, and this release's plan said it was.** The format is
//! pipe-delimited lines built by hand — `step {n} | tokens {n} | decision … `
//! per step, then `context {step} | {kind} | {detail}` per context event
//! (`state/trace.rs:1234-1251`). io-harness offers no JSON alternative;
//! `StepRecord` does not even derive `Serialize`. io-cli *could* build one —
//! `serde_json` is among its ten dependencies — and must not, because a document
//! this crate assembled would be a second format that compares against nothing,
//! which is the very thing the verbatim rule exists for. So the export takes a
//! `.txt` extension: a file named `.json` that is not JSON is a defect for
//! anyone who pipes it to a JSON tool. Recorded as `US-IO-CLI-0.27.0-I03`, and
//! found by a sabotage that killed nothing.
//!
//! # The markdown is for a person, and nothing reads it back
//!
//! It carries no schema version, nothing in this product parses it, and it is
//! not an interchange format. It is built from [`Store::session_turns`], whose
//! [`io_harness::Turn`] carries `prompt`, `reply`, `outcome`, `run_id` and
//! `created_at` — every one of them written by the store, and the timestamp
//! passed through untouched, because this crate reads no clock outside
//! `src/main.rs` and `tests/timing.rs` enforces it.
//!
//! **A turn with no reply is written as a turn with no reply.** `Turn::reply` is
//! `Option<String>`, and `None` means the turn did not finish — a run that was
//! interrupted, that died, or that is still going. Rendering it as an empty
//! answer would make an unfinished conversation look like one the agent had
//! nothing to say to.
//!
//! **There is no public reader for `runs.goal`.** Recorded in 0.23.0 and still
//! true in 0.69, so a run is named here by its id and its timestamp rather than
//! by the goal it was given.
//!
//! # Both files go through the workspace, under the session's own policy
//!
//! [`Workspace::write_file`] and never `std::fs::write`: an export is a file
//! this product writes on the operator's behalf, so it is subject to exactly the
//! path policy every other write is, and a path outside the workspace is refused
//! by io-harness rather than by a check invented here.
//!
//! **An existing file is refused rather than overwritten.** An export is a
//! snapshot, and the second one an operator takes is a different snapshot; the
//! command that silently replaced the first would destroy the thing they were
//! about to compare against. [`Refused::Exists`] names the file and stops.

use io_harness::tools::{Workspace, Wrote};
use io_harness::{Error, Store};

/// The extension a conversation export takes.
pub const MARKDOWN: &str = "md";

/// The extension a trace export takes.
///
/// `txt` and not `json`: [`Store::canonical_trace`] is pipe-delimited text. See
/// the module note and `US-IO-CLI-0.27.0-I03`.
pub const TRACE: &str = "txt";

/// Why an export did not happen.
///
/// Both variants are refusals rather than errors: the operator asked for
/// something reasonable and is being told why it did not happen, in words that
/// name the next thing they can do.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Refused {
    /// There is already a file at that path. An export is a snapshot and the
    /// next one is a different snapshot; overwriting silently would destroy the
    /// thing the operator was about to compare against.
    Exists(String),
    /// The session holds no turns, so there is no conversation to write. Writing
    /// an empty document would be a file that says the session was empty when it
    /// may simply not have started.
    Nothing,
}

impl Refused {
    /// What to say about it.
    pub fn said(&self) -> String {
        match self {
            Refused::Exists(path) => format!(
                "{path} is already there — an export is a snapshot, so this one is \
                 not written over it; name another path"
            ),
            // "no turns", not "no finished turns": `conversation` answers `None`
            // only when the conversation is empty, and a turn that never finished
            // is exported with a line saying so.
            Refused::Nothing => {
                "there is nothing to export yet — this conversation has no turns".to_string()
            }
        }
    }
}

/// What one export did.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Written {
    /// The path, relative to the workspace root, as it was written.
    pub path: String,
    /// How many bytes went into it.
    pub bytes: usize,
    /// What io-harness's own writer said happened. Always [`Wrote::Created`]
    /// here, because an existing path is refused before the write — carried
    /// anyway so that a future path that stops refusing cannot silently start
    /// reporting a creation for an overwrite.
    pub wrote: Wrote,
}

/// The conversation, as markdown.
///
/// Every turn the store holds for the session, in the order it returns them,
/// each with its prompt and — when the turn finished — its reply. A turn that
/// never finished says so.
pub fn conversation(store: &Store, session: &io_harness::Session) -> Result<Option<String>, Error> {
    // **`Session::history` and never `Store::session_turns`.** io-harness
    // documents the second as "the whole tree, not one path through it"
    // (`state/sessions.rs:265-266`), and a rewind moves the head back **without
    // deleting the turn** (`src/rewind.rs`), so a session that was undone once
    // and carried on holds turns that are not part of the conversation at all.
    //
    // Exporting the tree flat would put those turns in the document, in
    // sequence, with nothing marking them — in the one artifact whose stated
    // purpose is being read by somebody who was not there. It would
    // misrepresent both what the agent was asked and what it answered. Found by
    // the adversarial review; the first implementation had exactly that defect.
    let turns = session.history(store)?;
    let session_id = session.id();
    if turns.is_empty() {
        return Ok(None);
    }

    let mut out = format!("# Session {session_id}\n");
    for turn in turns {
        // The store's own stamp, passed through. Never an age, never a
        // reformatting: `tests/timing.rs` forbids this crate reading a clock, and
        // a document exported twice from the same store must be the same document.
        out.push_str(&format!(
            "\n## Turn {} · run {} · {}\n\n",
            turn.id, turn.run_id, turn.created_at
        ));
        out.push_str("### Prompt\n\n");
        out.push_str(&turn.prompt);
        out.push_str("\n\n### Reply\n\n");
        match &turn.reply {
            Some(reply) => {
                out.push_str(reply);
                out.push('\n');
            }
            // Not an empty section. `None` means the turn did not finish, and an
            // interrupted conversation must not read as one the agent had nothing
            // to say to.
            None => out.push_str("*this turn did not finish*\n"),
        }
        if let Some(outcome) = &turn.outcome {
            out.push_str(&format!("\n*outcome: {outcome}*\n"));
        }
    }
    Ok(Some(out))
}

/// One run's canonical trace, exactly as io-harness produced it.
///
/// Passed through. See the module note: the value of this string is that it is
/// canonical, and anything this crate did to it would end that.
pub fn trace(store: &Store, run_id: i64) -> Result<String, Error> {
    store.canonical_trace(run_id)
}

/// The path an export proposes for a session's conversation.
///
/// Proposed rather than imposed: the operator can name their own, and this is
/// what they get when they do not. Named after what it holds so two exports of
/// two sessions do not collide, which is the same reason it is refused rather
/// than overwritten when it is already there.
pub fn conversation_path(session_id: i64) -> String {
    format!("io-session-{session_id}.{MARKDOWN}")
}

/// The path an export proposes for a run's trace.
pub fn trace_path(run_id: i64) -> String {
    format!("io-run-{run_id}.{TRACE}")
}

/// Write one export, refusing rather than overwriting.
///
/// Through [`Workspace::write_file`], so the path policy the session is running
/// under is the one that applies and a path outside the workspace is io-harness's
/// refusal rather than a check invented here.
///
/// **The refusal is checked here, immediately before the write, and not only by
/// the caller.** The confirmation an operator agrees to is shown after an earlier
/// [`occupied`] call, and between the two keystrokes another `io`, an editor
/// autosave or a build can create the file — after which the write would
/// silently replace it while reporting a creation. The doc on [`Written::wrote`]
/// promised `Wrote::Created` and only this check makes that true. Found by the
/// adversarial review, which is also where the earlier version of this comment —
/// describing a `read_bytes` check that was not in the function — was caught.
pub fn write(workspace: &Workspace, path: &str, content: &str) -> Result<Written, Error> {
    if occupied(workspace, path)? {
        return Err(Error::Config(Refused::Exists(path.to_string()).said()));
    }
    let wrote = workspace.write_file(path, content)?;
    Ok(Written {
        path: path.to_string(),
        bytes: content.len(),
        wrote,
    })
}

/// Whether something is already at `path`, asked through the workspace.
///
/// `Ok(true)` means there is a file there. An `Err` is the path escaping the
/// root, and is propagated rather than treated as absence — answering "nothing
/// is there" for a path that cannot be written would send the operator into a
/// confirmation for a write that was never going to happen.
///
/// **[`Workspace::resolve`] and then `Path::exists`, rather than a read.** The
/// first implementation asked [`Workspace::read_bytes`] and treated `Error::Io`
/// as absence, and that was wrong in the way that matters: io-harness answers a
/// missing file with `Error::Config("no such file: …")`, so *every first export*
/// would have been refused with "could not be checked" — the export that has
/// never been taken being exactly the one an operator takes first. Caught by
/// `tests/export.rs`, never by reading.
///
/// Asking `resolve` also separates the two questions properly. This one is *may
/// this path be addressed at all* — absolute paths and any `..` climbing above
/// the root are refused here, by io-harness, on its own rules. Whether the
/// operator may **write** it is [`Workspace::write_file`]'s own `Act::Write`
/// gate, which is enforced at the write and must not be second-guessed here: a
/// permission check that ran early and a permission check that ran late would be
/// two answers to one question.
pub fn occupied(workspace: &Workspace, path: &str) -> Result<bool, Error> {
    let resolved = workspace.resolve(path)?;
    // **`symlink_metadata` and not `exists()`.** `exists` follows the link, so a
    // **dangling** symlink inside the workspace answers "nothing is there" — and
    // `write_file` then follows it and creates the file wherever it points,
    // outside the root. `Workspace::resolve` cannot catch that: it is purely
    // lexical, and `check_path`'s own symlink branch is guarded by a
    // `canonicalize` that fails on a broken link. Asking about the link itself is
    // what closes it. Found by the adversarial review.
    Ok(resolved.symlink_metadata().is_ok())
}

/// The confirmation `/export` shows before it writes.
///
/// Row 0 declines, as it does in every confirmation this product uses for
/// something it cannot take back — and an export can overwrite nothing, but it
/// does put the conversation on disk where the operator may not have expected a
/// file to appear.
pub fn confirm(path: &str, what: &str) -> (String, Vec<crate::picker::Row>) {
    (
        format!("Write this {what} to {path}?"),
        vec![
            crate::picker::Row::with_detail(crate::store::LEAVE_IT, "nothing is written"),
            crate::picker::Row::with_detail(
                format!("write {path}"),
                "under the session's own path policy; an existing file is refused",
            ),
        ],
    )
}

/// What to say after an export.
pub fn report(written: &Written) -> String {
    format!("wrote {} ({} bytes)", written.path, written.bytes)
}
