//! `@` completion: the workspace, one directory at a time, under the session's
//! own policy.
//!
//! This is the one completion surface that carries the product's thesis, and it
//! carries it by not being written here. The walk is
//! [`io_harness::tools::Workspace`], constructed [`Workspace::with_policy`] from
//! the policy the turn would run under, and `list_dir` drops a denied entry
//! before it ever returns — so **a path the posture denies is never offered**,
//! and it is never offered because nothing in io-cli had to remember to hide it.
//! That is also why no directory-walking crate is in `Cargo.toml`: the walk that
//! is already policy-aware is the one that ships.
//!
//! **One directory per call.** [`entries`] is `list_dir` and nothing more, so the
//! cost of opening the picker is the cost of the directory the operator is
//! looking at rather than of the tree beneath it. `find` exists next door and
//! walks the whole tree, sorted and uncapped, in one allocation — over the
//! io-harness checkout next door that is nearly five thousand paths for one
//! keystroke, and it is not used here at any depth.
//!
//! The shape is the slash palette's, deliberately: the driver owns the picker,
//! [`opens`] is the condition it branches on, [`rows`] is what the picker is
//! given and [`pick`] is what a chosen index stands for. There is no second
//! overlay mechanism, and `src/main.rs` holds no decision a test cannot reach.

use std::path::Path;

use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use io_harness::tools::{Entry, EntryKind, Workspace};
use io_harness::Policy;

use crate::picker::Row;

/// How many entries of one directory the picker is given.
///
/// **The harness does not bound a listing and should not** — a model reading a
/// directory wants all of it — so the bound belongs to the surface that puts it
/// in front of a person, which is this one.
///
/// Two hundred, and the number is measured rather than round. The largest
/// hand-authored directory in this workspace and in the io-harness checkout
/// beside it holds ninety-four entries, so nothing an operator wrote is ever
/// cut; two hundred labels ranked by [`crate::fuzzy`] on every keystroke is
/// nothing. What sails past this is a build output or a dependency tree, and
/// there scrolling was never going to be how the file was found — the note
/// saying the list was cut is the more useful answer, and typing is how the row
/// is reached.
///
/// It is a bound on rows and not on the read: `list_dir` has already returned by
/// the time this applies. Bounding the read is the harness's call to make, not
/// this file's.
pub const MAX_ENTRIES: usize = 200;

/// Whether this keystroke opens the completion picker.
///
/// **At a word boundary, and only there**: an empty prompt, or a prompt whose
/// last character is whitespace. `@` anywhere else is an ordinary character, so
/// `you@example.com` types as itself and nothing takes the keyboard away in the
/// middle of an address — the same rule the palette applies to `/`, which stays
/// a letter inside a line because a path and a fraction both contain one.
///
/// The boundary is read off the end of the prompt rather than off the cursor,
/// because the cursor is the composer's own and it does not expose one. That is
/// exact for text typed at the end, which is where a prompt is written, and it
/// errs towards *not* opening in the middle of an edit — the safe direction: a
/// completion that did not open costs one keystroke, and one that opened over a
/// half-written sentence costs the sentence.
///
/// `@` at a word boundary in front of something that was never a path — a
/// decorator pasted into a question — does open it, and it cannot be told apart
/// at the moment the key is pressed, because nothing has been typed after it
/// yet. `Esc` is the whole cost, and it leaves the prompt exactly as it was:
/// the keystroke is taken in front of [`crate::app::App::key`] and never reaches
/// the composer, which is what makes backing out free.
///
/// `armed` declines while a chord is half-pressed, for the reason
/// [`crate::commands::opens_palette`] gives at length: a keystroke that never
/// reaches `App::key` clears nothing, and the one sequence this product ships
/// changes the operator's files on its second press.
pub fn opens(key: KeyEvent, prompt: &str, armed: bool) -> bool {
    key.code == KeyCode::Char('@')
        // A `Ctrl` or `Alt` chord is a command somebody meant, not a letter they
        // typed — the same rule `Picker::key` applies to its own filter.
        && !key.modifiers.contains(KeyModifiers::CONTROL)
        && !key.modifiers.contains(KeyModifiers::ALT)
        && prompt.chars().next_back().is_none_or(char::is_whitespace)
        && !armed
}

/// One directory of the workspace, and whether the list was cut.
///
/// `root` is [`io_harness::Session::root`] — the root `io -C` set — and it is a
/// parameter rather than something read from the environment here. That is the
/// whole of the 0.3.0 defect stated as a signature: resolving against the
/// process working directory agrees with the session root right up until `io -C`
/// sets one, which is the case an operator actually runs and the case a fixture
/// built out of the current directory cannot see.
///
/// `dir` is relative to that root, `/`-separated, and empty for the root itself.
/// It is exactly what an [`Entry::path`] from the level above carries, so a
/// descent joins nothing.
///
/// The policy is the one the next turn would run under — posture and all — so
/// what the picker offers and what the agent may read are the same set by
/// construction. `list_dir` checks the directory itself and then drops every
/// denied entry inline; nothing here filters, and nothing here could forget to.
pub fn entries(root: &Path, policy: &Policy, dir: &str) -> Result<(Vec<Entry>, bool), String> {
    let workspace = Workspace::with_policy(root, policy.clone());
    // The harness's own sentence: it names the path, which is the part an
    // operator needs and the part any rewording here would lose.
    let mut found = workspace.list_dir(dir).map_err(|error| error.to_string())?;
    let cut = found.len() > MAX_ENTRIES;
    found.truncate(MAX_ENTRIES);
    Ok((found, cut))
}

/// The title over a listing, which is where the operator reads what they are
/// looking at.
///
/// Load-bearing rather than decorative, because [`rows`] labels an entry with its
/// last component: at one level down, `app.rs` under `src` and `app.rs` under
/// `tests` are the same three rows of characters, and the directory is the only
/// thing on the screen that tells them apart.
pub fn title(dir: &str) -> String {
    if dir.is_empty() {
        "Which path?".to_string()
    } else {
        format!("Which path under {dir}?")
    }
}

/// The picker's rows: one per entry, in the order `list_dir` sorted them.
///
/// **The label is the entry's last component, not its path**, and that is a
/// matching decision rather than a cosmetic one — the same one
/// [`crate::commands::palette`] makes when it strips the leading slash. Every
/// entry of `src` begins `src/`, so with the path left whole every label shares
/// a prefix, no query the operator can type is ever an exact name or a prefix of
/// one, and both of [`crate::fuzzy`]'s top tiers are unreachable. Trimmed,
/// typing a file's name puts that file first.
///
/// A directory keeps a trailing separator, which is what says it is one. It is a
/// path separator rather than a glyph — it is what the operator would have typed
/// — so it is spelled here and not taken off a theme.
///
/// No detail. The detail is the first thing the picker drops on a narrow
/// terminal, and a size or a kind is not worth a row that reads differently
/// depending on how wide the window is.
pub fn rows(entries: &[Entry]) -> Vec<Row> {
    entries
        .iter()
        .map(|entry| {
            let name = entry.path.rsplit('/').next().unwrap_or(&entry.path);
            match entry.kind {
                EntryKind::Dir => Row::new(format!("{name}/")),
                EntryKind::File | EntryKind::Symlink => Row::new(name),
            }
        })
        .collect()
}

/// The line that says the listing was cut, or `None` when it was not.
///
/// A silently truncated list reads as a complete one, and here it would read as
/// *the file is not there* — which is the one conclusion this surface must never
/// invite, because it is indistinguishable from the policy having hidden it. It
/// names what is on screen rather than [`MAX_ENTRIES`], for the reason
/// [`crate::sessions::cut_note`] gives: the number the operator is looking at is
/// the honest one.
pub fn cut_note(cut: bool, shown: usize) -> Option<String> {
    cut.then(|| format!("the first {shown} of a larger directory; type to narrow it"))
}

/// What choosing a row does.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Picked {
    /// A directory: list it, at one more level of depth.
    Descend(String),
    /// A path, relative to the session root, for the prompt.
    Insert(String),
}

/// What the row at `index` stands for.
///
/// The index is the one [`crate::picker::Outcome::Chosen`] carries, which
/// addresses the rows the picker was given — and those are [`rows`]'s, one per
/// entry in order. So this reads the listing back positionally, the same way
/// `/resume` and `/fork` read their id lists.
///
/// A symlink inserts rather than descends. The harness reports a link as the
/// link and never follows one, and descending would be io-cli deciding to follow
/// it — the one place this file could put a walk of its own back in.
///
/// `None` for an index past the end, which is the row saying the list was cut.
/// Nothing behind it, so nothing happens.
pub fn pick(entries: &[Entry], index: usize) -> Option<Picked> {
    entries.get(index).map(|entry| match entry.kind {
        EntryKind::Dir => Picked::Descend(entry.path.clone()),
        EntryKind::File | EntryKind::Symlink => Picked::Insert(entry.path.clone()),
    })
}
