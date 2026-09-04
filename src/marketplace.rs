//! The marketplaces on disk — what each one is, what it holds, and taking one
//! away.
//!
//! [`crate::fetch`] brings a repository down; this module is everything that
//! happens once it is here. The split is the one `src/fetch.rs`'s own docs argue
//! for: that file is the crate's second permitted process spawn and is held to
//! properties `tests/dependencies.rs` asserts over its text, so every line that
//! does *not* need to be inside that boundary is kept out of it. Listing a clone,
//! reading a manifest and deleting a directory need no program at all.
//!
//! # Everything here takes its paths as arguments
//!
//! `~/.io-cli/marketplaces` is reached in exactly three functions — [`add`],
//! [`remove`] and [`installed`] — and each is a wrapper of two or three lines over
//! something that takes the directory as an argument. That is `fetch::clone`'s own
//! arrangement and it is here for the same reason: a decision that lives behind
//! [`crate::home`] is a decision nothing under `tests/` can reach without moving
//! the operator's home out from under a suite running in parallel, and this crate
//! has shipped untestable driver logic in three releases and paid for it in the
//! release after each.
//!
//! # This module reads a `plugin.toml` and io-cli's other surfaces do not
//!
//! [`crate::pluginview`] deliberately opens no manifest: every fact it draws comes
//! from [`io_harness::Config::plugins`], because a second reader of what a manifest
//! *means* is a second opinion about somebody else's file. Nothing is different
//! here about that rule — what is different is the question. A bundle inside a
//! marketplace is **not declared by any configuration**, so `Config::plugins()` has
//! never heard of it and cannot be asked. The only thing that can answer *what is
//! in this repository* is the repository.
//!
//! So the reading is kept to the two keys a listing draws — `name` and
//! `description` — and it goes through [`crate::edit::value_at`], because
//! `src/edit.rs` is this crate's only permitted TOML parser by path
//! (`tests/dependencies.rs`, `f7_the_configuration_is_read_through_the_harness_…`).
//! That function answers in **source bytes**, quotes included, so every value read
//! here goes through this module's own `declared`, which unquotes it and then
//! strips it of control
//! characters and bounds its length.
//!
//! **A manifest is a stranger's file and is treated as one.** `src/fetch.rs:446`
//! states the rule for git's stderr — output from a program io-cli did not write,
//! on a renderer whose scrollback is the transcript — and a marketplace manifest is
//! the same trust class. TOML permits raw newlines inside a `"""` string, so a
//! `description` can otherwise put forged extra lines on the very surface an
//! operator reads to decide whose code to install. `plain` is that filter and
//! every value out of a manifest passes through it. `plain` is `pub(crate)` for
//! exactly one reason: a hook's argv reaches the consent surface through
//! io-harness's accessors now rather than through this module, and it is the same
//! trust class — so [`crate::pluginview`] filters it with this function rather
//! than growing a second one that drifts from it.
//!
//! **A manifest that does not name itself is still a bundle.** io-harness would
//! refuse to load it, and that refusal is io-harness's to make when somebody
//! declares it; a listing that dropped the directory would be answering "what is in
//! this repository" with a filtered list and no mark saying so, which is the silent
//! absence `pluginview`'s module docs exist to end. It is listed under its
//! directory's own name, with [`Bundle::line`] saying the manifest did not name it.
//!
//! # A bundle is asked for by name, and the disk decides which reading that was
//!
//! `plugin add <word>` took a path and from 0.29.0 it takes a name as well, which
//! is one verb with two readings of one word. [`chosen`] is the whole rule and it
//! asks the disk rather than the spelling: a word that resolves to a directory
//! carrying a manifest is a path, and every other word is a name looked up with
//! [`locate`]. A rule keyed on a `/` or a leading `.` would make one word mean
//! different things in different working directories; this one cannot, and a real
//! relative path can never become unreachable under it.
//!
//! **A bare name two marketplaces carry is refused.** They are two repositories'
//! code, and resolving to whichever the walk reached first installs something the
//! operator did not choose — so the refusal spells `<name>@<owner>/<repo>` for each
//! of them instead. [`matching`] is the same lists read the other way round, and it
//! reads *every* marketplace: a second one is added precisely because the first did
//! not hold what was wanted.
//!
//! Nothing here writes: [`chosen`] answers with a directory and the entry is
//! written by [`crate::pluginview`], the edits `/plugin add` already had.
//!
//! # A marketplace install discloses before it writes, and io-harness does the reading
//!
//! A bundle out of a marketplace is a directory **nobody on this machine has
//! read**, and it contributes to seven subsystems at once. Every other bundle io-cli
//! declares came from a directory the operator typed the path of; this one came
//! from a stranger's repository, and [`Chosen`] is where the two part.
//!
//! io-harness 0.71.0 publishes [`io_harness::Plugins::inspect`], which is
//! `load_one` — the loader `Config::plugins` itself runs — reached without a
//! `[[plugin]]` entry. So the install reads the directory **first**: every check
//! runs, every name is namespaced, and [`disclosure`] renders the returned
//! [`io_harness::Plugin`] through the same accessors `/plugin`'s own pane uses.
//! The names in it are already `rust-review__reviewer`, which is the whole reason
//! the reading must be io-harness's rather than the manifest's: the manifest says
//! `reviewer`, and `reviewer` is a name the operator will never see again.
//!
//! **Nothing is written until the operator consents, and that is what 0.30.0
//! changed.** Through 0.29.0 there was no loader taking a directory, so the only
//! way to have a stranger's bundle validated was to declare it — the install wrote
//! the entry `enabled = false`, re-discovered, and disclosed off the result. A
//! bundle io-harness refused therefore left an entry behind in a file the operator
//! had agreed to nothing about. Now `inspect` answers with the file untouched, and
//! the write is [`crate::pluginview::add`], once, after consent. Declining writes
//! no byte at all.
//!
//! **A bundle io-harness refuses is refused before anything is written**, in
//! io-harness's own sentence — the string that would otherwise have landed on
//! `Plugins::dropped`. Nothing is offered to switch on, because there is nothing
//! that would load. A `${env:}`, `${file:}` or `${cmd:}` substitution inside a
//! manifest is one of those refusals in **every** scope from 0.71.0: a manifest is
//! the one file here nobody has agreed to, and resolving one would read this
//! machine's environment or its files, or run a program on it, for a directory
//! still under consideration.
//!
//! **So a marketplace install goes to user scope**, and [`disclosure`] is told
//! which scope it is: io-harness's `refuse_executing_contributions` rejects a
//! `Scope::Project` bundle carrying any `[[hook]]` or `[[mcp]]` and the refusal
//! takes the whole bundle. That is already `manage`'s default for a new entry, and
//! is a reason not to change it — and the asymmetry is exactly the fact an install
//! owes an operator before it writes.
//!
//! # The hooks are io-harness's too
//!
//! [`io_harness::Plugin::hooks`] and a public [`io_harness::Hook`] arrived in
//! 0.71.0, so this module's own `[[hook]]` reader is gone. It could not see an
//! inline `hook = [{…}]` array or a `[[hook]]` header with a trailing comment on
//! it — both ordinary TOML, both accepted by io-harness — and drew a bundle with
//! hooks as a bundle with none, on the contribution kind that **runs programs**.
//! The rows are built in [`crate::pluginview`] now, off the accessors.
//!
//! # Removing a marketplace removes a clone and nothing else
//!
//! [`discard`] deletes a directory. It does not touch a `[[plugin]]` entry, it
//! cannot reach one, and that is criterion F3 rather than an omission: an operator
//! who declared a bundle out of a marketplace made a decision about their
//! configuration, and a cache being emptied is not a reason to undo it. What the
//! surface owes them instead is the *consequence*, which is why [`dependents`] and
//! [`warning`] exist — a bundle declared at a path inside the clone stays declared
//! and stops loading, and being told that is the whole difference between a cache
//! eviction and a session that quietly lost its agents.

use std::path::{Path, PathBuf};

use io_harness::config::Scope;

use crate::fetch::{Fetched, Named};
use crate::glyphs::Glyphs;
use crate::picker::{fit, fit_left, Row};
use crate::pluginview::MANIFEST;

/// The mark on a marketplace that is on the disk.
///
/// [`crate::pluginview::LOADED_MARK`]'s own character and for its own reason: one
/// ASCII glyph in both sets, already meaning *present* everywhere else in this
/// product. A marketplace has no second state to distinguish it from, so it needs
/// no second mark — the mark is here so a marketplace row cannot be mistaken for a
/// bundle row when the two lists are read one after the other.
pub const HERE_MARK: &str = crate::pluginview::LOADED_MARK;

/// One directory inside a marketplace that carries a [`MANIFEST`].
///
/// Owned, and the two fields that come out of the manifest are `Option` rather
/// than defaulted: "this manifest has no description" and "this manifest's
/// description is empty" are one fact to a reader and two to a writer, and the
/// listing says which. See the module docs for why a nameless manifest is here at
/// all.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Bundle {
    /// The directory the manifest sits in.
    ///
    /// For a bundle an index places in **another** repository this is the clone
    /// the index itself is in, because the bundle's own directory does not exist
    /// until the install fetches it. [`Bundle::source`] is what says which of the
    /// two this is, and nothing draws a label off this field for an index bundle —
    /// an index always names its entries, so [`Bundle::label`] never reaches the
    /// directory case for one.
    pub dir: PathBuf,
    /// The manifest's `name`, unquoted, or `None` where it carries none this
    /// module could read.
    pub name: Option<String>,
    /// The manifest's `description`, unquoted, or `None`.
    pub description: Option<String>,
    /// Whether the author wrote a manifest io reads, or io generated one.
    pub origin: Origin,
    /// Where an index said the bundle is, and `None` for a bundle found by the
    /// walk — which is by definition already at [`Bundle::dir`].
    pub source: Option<crate::adapt::Source>,
}

/// Whether a bundle's manifest is the author's or io's.
///
/// **Drawn rather than inferred.** An adapted bundle works through a
/// `plugin.toml` io generated from somebody else's `plugin.json`, and the
/// difference between what an author wrote and what io made of it must never be
/// something an operator has to work out — least of all when a bundle is dropped
/// and the file to look at is the generated one.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Origin {
    /// The directory carries a [`MANIFEST`]. Read exactly as it was before io-cli
    /// read any foreign format, and it wins its own directory.
    Native,
    /// Read from a Claude Code or Codex manifest. io writes the `plugin.toml`.
    Adapted,
}

impl Bundle {
    /// What to call it: the manifest's `name`, and the directory's own name only
    /// where the manifest gave none.
    ///
    /// **The manifest wins, and F2's sabotage is returning the directory
    /// instead.** io-harness namespaces every contribution a bundle makes by the
    /// manifest's `name` — a bundle in a directory called `plugins/reviewer` whose
    /// manifest says `rust-review` contributes `rust-review__reviewer`, and a
    /// listing that showed `reviewer` would have named nothing an operator will
    /// ever see again.
    #[must_use]
    pub fn label(&self) -> String {
        match (&self.name, self.dir.file_name()) {
            (Some(name), _) => name.clone(),
            (None, Some(dir)) => dir.to_string_lossy().into_owned(),
            // A directory with no last component at all — a bare `/`, or a path
            // ending in `..`. Neither can be a bundle, and a row drawn with an
            // empty label is worse than one drawn with the whole path.
            (None, None) => self.dir.display().to_string(),
        }
    }

    /// The one line beside the label: the description, or what is missing.
    ///
    /// A nameless manifest says so here rather than being drawn as an ordinary
    /// row under a directory name, because the label is then io-cli's guess and
    /// the operator has no other way to tell.
    #[must_use]
    pub fn line(&self) -> String {
        // The file named is the one io actually read, which since 0.31.0 is not
        // always a `plugin.toml`. A sentence telling an operator to look in a file
        // that is not there is worse than the missing description it is reporting.
        let read = self.read_from();
        match (&self.name, &self.description) {
            (Some(_), Some(said)) => said.clone(),
            (Some(_), None) => format!("its {read} carries no description"),
            (None, Some(said)) => {
                let mut out = format!("its {read} does not name it; ");
                out.push_str(said);
                out
            }
            (None, None) => format!("its {read} does not name it, and carries no description"),
        }
    }

    /// The name of the manifest file this bundle's two keys came out of.
    #[must_use]
    pub fn read_from(&self) -> &'static str {
        // **An index entry's two keys came out of the index**, not out of any
        // `plugin.json`, and in the common case that file does not exist at the
        // path the sentence would send an operator to. `source` is what says which
        // read this was: the walk sets it `None`, `from_entry` sets it `Some`.
        match (self.origin, self.source.is_some()) {
            (Origin::Native, _) => MANIFEST,
            (Origin::Adapted, true) => crate::adapt::INDEX_FILE,
            (Origin::Adapted, false) => crate::adapt::MANIFEST_FILE,
        }
    }
}

/// One marketplace, as it is on the disk.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Market {
    /// The `<owner>/<repo>` it was fetched by. The same value
    /// [`crate::fetch::resolve`] produces, so the name a listing draws is the name
    /// `plugin marketplace remove` takes.
    pub named: Named,
    /// The clone's own directory.
    pub root: PathBuf,
    /// Every directory in it that carries a [`MANIFEST`], the root itself
    /// included. See [`holdings`].
    pub bundles: Vec<Bundle>,
}

impl Market {
    /// `<owner>/<repo>`, which is both the label and the word an operator types.
    #[must_use]
    pub fn name(&self) -> String {
        let mut out = self.named.owner.clone();
        out.push('/');
        out.push_str(&self.named.repo);
        out
    }

    /// How many bundles it holds, said in words rather than as a bare number.
    ///
    /// **Zero is a real answer and gets its own sentence.** A marketplace whose
    /// repository holds no `plugin.toml` at all is either the wrong repository or
    /// one whose bundles live deeper than the walk goes, and "0 bundles" leaves an
    /// operator unable to tell those apart from a fetch that failed.
    #[must_use]
    pub fn held(&self) -> String {
        match self.bundles.len() {
            // Three formats are read now, so the sentence names the shape rather
            // than one filename. Before 0.31.0 this said "no directory in it
            // carries a plugin.toml", which was true and was the answer for every
            // marketplace in the field — the release exists because of it.
            0 => format!(
                "nothing in it is a bundle — no {MANIFEST}, no {}/{}, no {}",
                crate::adapt::CLAUDE_DIR,
                crate::adapt::INDEX_FILE,
                crate::adapt::MANIFEST_FILE,
            ),
            1 => "1 bundle".to_string(),
            many => format!("{many} bundles"),
        }
    }
}

/// The clone directory of `named` under a marketplaces directory.
///
/// Two levels — `<owner>/<repo>` — which is [`crate::fetch::at`]'s own layout with
/// the home passed in rather than looked up. The two are asserted equal in
/// `tests/marketplace.rs`: a second spelling of the destination is how an `add`
/// writes to one path and a `remove` looks in another.
#[must_use]
pub fn at(root: &Path, named: &Named) -> PathBuf {
    root.join(&named.owner).join(&named.repo)
}

/// One manifest's two listed keys, or `None` where `dir` carries no manifest.
///
/// The existence check is the same one [`crate::pluginview::refusal`] makes and it
/// is what decides whether a directory is a bundle at all. Everything after it is
/// best effort: an unreadable or unparseable manifest yields a [`Bundle`] with
/// neither key, which [`Bundle::line`] reports as a manifest that does not name it
/// — the honest answer, and the one io-harness will give in its own words if the
/// operator goes on to declare it.
#[must_use]
pub fn manifest(dir: &Path) -> Option<Bundle> {
    let file = dir.join(MANIFEST);
    if !file.is_file() {
        return None;
    }
    let text = std::fs::read_to_string(&file).unwrap_or_default();
    Some(Bundle {
        dir: dir.to_path_buf(),
        name: declared(&text, "name"),
        description: declared(&text, "description"),
        origin: Origin::Native,
        source: None,
    })
}

/// One directory's foreign manifest as a [`Bundle`], or `None` where it carries
/// none.
///
/// [`manifest`]'s counterpart for the two formats io-cli did not invent, and it is
/// only ever consulted where that function has already answered `None` — a native
/// `plugin.toml` wins its own directory, which is F2.
#[must_use]
pub fn adapted(dir: &Path) -> Option<Bundle> {
    let read = crate::adapt::manifest_at(dir)?;
    Some(Bundle {
        dir: dir.to_path_buf(),
        name: read.name,
        description: read.description,
        origin: Origin::Adapted,
        source: None,
    })
}

/// One index entry as a [`Bundle`], under the id [`published`] derived for it.
///
/// The name is the index's, never the directory's, and that is a decision rather
/// than a convenience: the index is the author's own statement of what the
/// repository publishes, it is the word an operator types at `plugin add`, and an
/// entry whose source is another repository has no local directory to take a name
/// from until the install has already happened.
///
/// **It is the id and not the entry's own spelling**, for [`Bundle::label`]'s
/// reason: the label is the word the listing draws and therefore the word
/// [`locate`] is asked for, and io-harness namespaces the bundle's contributions
/// with the id. An entry spelled `Rust-Review` drawn under that spelling would
/// name a bundle no install could resolve. The two differ only by a case fold —
/// [`crate::adapt::normalised`] refuses anything wider — so the row still reads
/// as the entry an operator found in the index.
fn from_entry(clone: &Path, id: String, entry: crate::adapt::Entry) -> Bundle {
    let dir = match &entry.source {
        crate::adapt::Source::Local(said) => local_dir(clone, said),
        crate::adapt::Source::Remote(_) => clone.to_path_buf(),
    };
    Bundle {
        dir,
        name: Some(id),
        description: entry.description,
        origin: Origin::Adapted,
        source: Some(entry.source),
    }
}

/// The bundles an index publishes, and one line for every entry it does not.
///
/// **One reader answering both, because [`holdings`] and [`unreadable`] must not
/// disagree about which entry is which.** An entry counted as a bundle by one and
/// reported as a refusal by the other is a row an operator can see, cannot
/// install, and is told nothing about. The two stay separate at the surface — a
/// refusal is not a bundle and a listing must not offer it as one — and share the
/// decision underneath.
///
/// Two refusals are made here on top of what [`crate::adapt`] itself could not
/// read, and neither is a drop:
///
/// 1. A name [`crate::adapt::normalised`] cannot make an id is refused **by
///    name**. It joins the reader's own unreadable lines because the rule there
///    already is "reported, never skipped" — an entry that vanished would leave
///    an operator comparing a listing against a file to work out which of the two
///    is lying.
/// 2. Two entries reaching one id are refused **naming both**, in [`locate`]'s
///    words for the same class of problem: one name, two different repositories'
///    code, and whichever was found first is code the operator did not choose.
///    Neither is offered, and that is where this parts from `locate` — `locate`
///    can offer a qualifier that tells two marketplaces apart, and inside one
///    index there is no second spelling to offer. The index's author is the only
///    party who can resolve it.
fn published(clone: &Path, index: crate::adapt::Index) -> (Vec<Bundle>, Vec<String>) {
    let mut said = index.unreadable;
    let mut kept: Vec<(String, String, Bundle)> = Vec::new();
    for entry in index.entries {
        let written = entry.name.clone();
        let Some(id) = crate::adapt::normalised(&written) else {
            said.push(format!(
                "{written} is not a usable plugin id and io does not invent one — an id is \
                 what you type at `plugin add` and what namespaces every name the bundle \
                 contributes, so it must be 1 to {} characters of `a-z`, `0-9` and `-`, \
                 starting with a letter or a digit",
                io_harness::MAX_ID,
            ));
            continue;
        };
        kept.push((id.clone(), written, from_entry(clone, id, entry)));
    }

    // The ids more than one entry reached, in the order the index first names
    // them, so two reads of one file report in one order. Quadratic over the
    // entries and deliberately so: the largest index in the field carries 291 of
    // them, and a map keyed by id would still have to be walked in the file's
    // order to report in it.
    let mut clashing: Vec<String> = Vec::new();
    for (id, _, _) in &kept {
        if !clashing.contains(id) && kept.iter().filter(|held| held.0 == *id).count() > 1 {
            clashing.push(id.clone());
        }
    }
    for id in &clashing {
        let names: Vec<String> = kept
            .iter()
            .filter(|held| held.0 == *id)
            .map(|held| format!("`{}`", held.1))
            .collect();
        let count = names.len();
        // ` and ` is `locate`'s own joiner for its own list of spellings. A second
        // joiner for the same class of sentence is a second phrasing an operator
        // has to learn twice.
        let spellings = names.join(" and ");
        said.push(format!(
            "{count} entries in this index are the plugin id `{id}` — {spellings} — and \
             installing whichever was found first would install code you did not choose; \
             none of them is offered until the index names them apart"
        ));
    }

    let bundles = kept
        .into_iter()
        .filter(|held| !clashing.contains(&held.0))
        .map(|held| held.2)
        .collect();
    (bundles, said)
}

/// A `"./"` or `"./plugins/x"` source resolved against the clone.
///
/// **Every component that is not a plain name is dropped**, so a source of
/// `"../../etc"` or an absolute path cannot address anything outside the clone.
/// A marketplace index is written by a stranger and this is the one value in it
/// that becomes a path io reads from; the directory it names is inside the clone
/// or the entry addresses the clone's own root.
fn local_dir(clone: &Path, said: &str) -> PathBuf {
    let mut dir = clone.to_path_buf();
    for part in Path::new(said).components() {
        if let std::path::Component::Normal(name) = part {
            if name != "." {
                dir.push(name);
            }
        }
    }
    dir
}

/// One top-level key of a manifest, unquoted, filtered and bounded.
///
/// [`crate::edit::value_at`] answers in the value's **source bytes**, so a string
/// arrives with its quotes on and its escapes unresolved. `unquote` takes them
/// off, `plain` takes the control characters out and `bounded` caps the length
/// — in that order, because decoding an escape is what can *produce* a control
/// character and a filter that ran first would not see it.
///
/// An empty result is `None` rather than `Some("")`: a key present and blank names
/// a bundle no better than a key that is absent, and collapsing them here means
/// [`Bundle::label`] has one case to answer instead of two.
fn declared(text: &str, key: &str) -> Option<String> {
    let raw = crate::edit::value_at(text, key)?;
    let value = plain(&unquote(raw.trim()));
    let value = value.trim();
    if value.is_empty() {
        return None;
    }
    Some(bounded(value))
}

/// The longest a single value out of a stranger's manifest may be.
///
/// A [`matching`] hit is one finished line of `<spelling> · <description>`, and a
/// description is a field a repository fills in — nothing stops it holding a
/// megabyte on one line, and a picker row and a scrollback line are both worse than
/// useless when it does. Two hundred characters is longer than any terminal is wide
/// and shorter than anything that can bury the row above it.
///
/// **A hook's command is deliberately not bounded by this** — see
/// [`crate::pluginview::detail`], which draws the hook rows: it is argv an
/// operator is consenting to, and a shortened argv is the one thing that surface
/// must never show.
const LONGEST: usize = 200;

/// `value` cut to [`LONGEST`] characters.
///
/// Plain dots rather than [`Glyphs::ellipsis`]: this is a property of the data and
/// runs before any renderer is chosen, and a value that arrives at both glyph sets
/// already carrying a `…` is a value that is wrong in one of them.
pub(crate) fn bounded(value: &str) -> String {
    if value.chars().count() <= LONGEST {
        return value.to_string();
    }
    let mut out: String = value.chars().take(LONGEST).collect();
    out.push_str("...");
    out
}

/// A manifest value made safe to put in a terminal.
///
/// `src/fetch.rs:446`'s rule, on the same trust class for the same reason: this is
/// content from a file io-cli did not write, and on this renderer the scrollback is
/// the transcript. A `description` spelled as a TOML multi-line string may carry
/// raw newlines, and one of those on the consent surface is a forged line the
/// operator reads as io-cli's own.
///
/// A control character becomes a **space** rather than being dropped, which is the
/// one place this differs from `fetch.rs`: that function keeps line structure
/// because `Fetched::sentence` reads the last line, and everything here has to end
/// up on one line — where dropping a newline would fuse the two words either side
/// of it into a word neither the file nor io-cli ever spelled.
///
/// Unhandled on purpose: the bidirectional format characters (`U+202E` and its
/// neighbours) are not control characters and are left as they are. They can
/// reverse how a run of text *displays* without changing what it says, which is a
/// separate problem from this one and belongs wherever this crate decides it for
/// every surface at once rather than in the marketplace reader alone.
pub(crate) fn plain(value: &str) -> String {
    value
        .chars()
        .map(|glyph| if glyph.is_control() { ' ' } else { glyph })
        .collect()
}

/// The TOML source of a string value, turned back into the string.
///
/// **`trim_matches('"')` understands one of TOML's four string forms**, and
/// `src/edit.rs:764` already documents that hazard for its own segment splitter —
/// the fix made there was never applied here. `name = 'tools'` produced the label
/// `'tools'` while io-harness namespaces that bundle's contributions with `tools`,
/// so the bundle was unreachable by the only word that matters and the quotes were
/// drawn on the row.
///
/// Handled: literal strings (`'…'`, `'''…'''`), basic strings (`"…"`), multi-line
/// basic strings (`"""…"""`) including the newline TOML trims immediately after the
/// opening delimiter, and the escapes `escapes` names.
///
/// **Not handled, and stated rather than implied:** the `\uXXXX` / `\UXXXXXXXX`
/// escapes and the line-ending backslash of a multi-line basic string are left
/// exactly as the file spells them, so they show as the text they are written as
/// and never as the character they denote. That is the safe direction — an escape
/// this function does not decode cannot become a control character — and full TOML
/// string decoding belongs in `src/edit.rs`, which is this crate's only permitted
/// TOML parser by rule and is the file that already owns `toml`'s own decoder.
/// Anything that is not a quoted string at all — a number, an array, a hook's `run`
/// — comes back untouched.
fn unquote(raw: &str) -> String {
    for fence in ["\"\"\"", "'''"] {
        if raw.len() >= 2 * fence.len() && raw.starts_with(fence) && raw.ends_with(fence) {
            let inner = &raw[fence.len()..raw.len() - fence.len()];
            // TOML: a newline immediately after the opening delimiter is not part
            // of the value.
            let inner = inner
                .strip_prefix("\r\n")
                .or_else(|| inner.strip_prefix('\n'))
                .unwrap_or(inner);
            return if fence.starts_with('"') {
                escapes(inner)
            } else {
                inner.to_string()
            };
        }
    }
    if raw.len() >= 2 && raw.starts_with('\'') && raw.ends_with('\'') {
        // A literal string takes no escapes at all, which is the whole of what
        // makes it literal.
        return raw[1..raw.len() - 1].to_string();
    }
    if raw.len() >= 2 && raw.starts_with('"') && raw.ends_with('"') {
        return escapes(&raw[1..raw.len() - 1]);
    }
    raw.to_string()
}

/// The escape sequences of a TOML basic string, resolved.
///
/// An escape this function does not know is copied through with its backslash
/// rather than being dropped or guessed at: the text then says what the file says,
/// which is the honest answer and the one that cannot invent a character. See
/// `unquote` for which those are.
fn escapes(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut glyphs = text.chars();
    while let Some(glyph) = glyphs.next() {
        if glyph != '\\' {
            out.push(glyph);
            continue;
        }
        match glyphs.next() {
            Some('n') => out.push('\n'),
            Some('t') => out.push('\t'),
            Some('r') => out.push('\r'),
            Some('b') => out.push('\u{8}'),
            Some('f') => out.push('\u{c}'),
            Some('"') => out.push('"'),
            Some('\\') => out.push('\\'),
            Some(other) => {
                out.push('\\');
                out.push(other);
            }
            None => out.push('\\'),
        }
    }
    out
}

/// Every bundle inside a clone, the clone's own root included.
///
/// **Three formats, and the precedence between them is the whole of this
/// function.**
///
/// 1. A native `plugin.toml` at the clone's own root **suppresses the index**. A
///    repository that has written io's own manifest has said what it publishes in
///    the format this crate owns, and a foreign index must not speak over it. That
///    is what lets the author of a Claude or Codex bundle take back control by
///    adding one file, and [`crate::adapt`]'s generator never writes inside a
///    clone precisely so this stays the author's decision. It suppresses the index
///    and nothing else: the walk still runs, so a repository that carries a root
///    manifest **and** bundles in subdirectories lists all of them exactly as it
///    did before this crate read any foreign format.
/// 2. Otherwise `.claude-plugin/marketplace.json`, where the clone carries one.
///    **The walk does not also run**, and F3 asserts the count rather than the
///    membership for exactly that reason: a union would list bundles the author
///    did not publish beside the ones they did, with no way for an operator to
///    tell which was which. The index is the author's own statement, and it is the
///    file both Claude Code and Codex read. Not every entry becomes a bundle —
///    [`published`] holds back the ones that name no usable plugin id, and
///    [`unreadable`] is where each of those says so by name.
/// 3. Otherwise the walk, with each directory read as a native manifest first and
///    a foreign one only where it carries none.
///
/// **The walk is [`crate::pluginview::candidates`]'s**, not a second one written
/// here: it already skips `target`, `node_modules` and every dotted directory —
/// `.git`, which every clone has, most of all — and it already orders by depth and
/// then by path so two calls on one machine answer the same way. A repository
/// laying its bundles out in a way that walk cannot see is a repository `/plugin
/// add` could not see either, and one walk that is sometimes too shallow is better
/// than two that disagree about which. Reading `.claude-plugin/plugin.json` at a
/// directory the walk visited does **not** weaken its dotted-directory skip: the
/// path is known and relative to a directory already admitted, and the walk itself
/// still never descends into one.
///
/// The root is checked separately because that function only ever looks at a
/// directory's *children*, and a marketplace that is itself one bundle — a single
/// plugin published as its own repository — is the shape it would otherwise miss
/// entirely.
#[must_use]
pub fn holdings(clone: &Path) -> Vec<Bundle> {
    if !clone.join(MANIFEST).is_file() {
        if let Some(index) = crate::adapt::index_at(clone) {
            return published(clone, index).0;
        }
    }
    // `visited` and not `candidates`: the latter is this filtered by "carries a
    // plugin.toml", and a directory holding only a `.claude-plugin/plugin.json`
    // carries none — so iterating `candidates` looking for a foreign manifest
    // would find one exactly never.
    let mut dirs = vec![clone.to_path_buf()];
    dirs.extend(crate::pluginview::visited(clone));
    dirs.iter()
        .filter_map(|dir| manifest(dir).or_else(|| adapted(dir)))
        .collect()
}

/// Every entry of an index that yields no bundle, for the surface that lists it.
///
/// Three kinds of line, and one list because an operator reading a listing has one
/// question — which entries are not here, and why: an entry [`crate::adapt`] could
/// not read at all, a name that is no plugin id, and a set of names that reach one
/// id. [`published`] writes all three.
///
/// Separate from [`holdings`] rather than a field on it, because a refusal is not
/// a bundle and a listing that mixed the two would offer a row that cannot be
/// installed. Empty for a clone with no index, which is the same answer as an
/// index that read cleanly — the distinction that matters is whether there is
/// something to report, and there is not.
#[must_use]
pub fn unreadable(clone: &Path) -> Vec<String> {
    if clone.join(MANIFEST).is_file() {
        return Vec::new();
    }
    crate::adapt::index_at(clone)
        .map(|index| published(clone, index).1)
        .unwrap_or_default()
}

/// Every marketplace under `root`, ordered by owner and then by repository.
///
/// Exactly two levels, because that is the layout [`crate::fetch::at`] writes and
/// nothing else in that tree is a marketplace. A deeper walk would find the
/// bundles of every clone and report each as a marketplace of its own; a shallower
/// one would report an owner. Anything that is not a directory, and anything whose
/// name starts with a dot, is skipped at both levels — `.fetching` is a sibling of
/// this tree rather than inside it, but a dot-named leftover of any kind is not
/// something an operator asked for and must never be counted as one.
#[must_use]
pub fn markets(root: &Path) -> Vec<Market> {
    let mut found = Vec::new();
    for owner in ordinary(root) {
        for repo in ordinary(&owner) {
            // Built out of the two directory names rather than out of the path, so
            // the value carried is the same shape `fetch::resolve` produces and a
            // listing row can be handed straight back to `remove`.
            let (Some(owner_name), Some(repo_name)) = (name_of(&owner), name_of(&repo)) else {
                continue;
            };
            found.push(Market {
                named: Named {
                    owner: owner_name,
                    repo: repo_name,
                },
                bundles: holdings(&repo),
                root: repo,
            });
        }
    }
    found.sort_by(|a, b| (&a.named.owner, &a.named.repo).cmp(&(&b.named.owner, &b.named.repo)));
    found
}

/// The plain directories directly inside `dir`, unsorted.
fn ordinary(dir: &Path) -> Vec<PathBuf> {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return Vec::new();
    };
    entries
        .flatten()
        .map(|entry| entry.path())
        .filter(|path| path.is_dir())
        .filter(|path| !name_of(path).is_some_and(|name| name.starts_with('.')))
        .collect()
}

/// A path's last component as a `String`, or `None` where it has none.
fn name_of(path: &Path) -> Option<String> {
    Some(path.file_name()?.to_string_lossy().into_owned())
}

/// Where a bundle an index placed in another repository is cloned to.
///
/// **Dot-named, and inside the marketplaces tree on purpose.** [`markets`] skips
/// anything dot-named at both of its two levels, so a repository fetched for one
/// entry can never be counted as a marketplace of its own — which it is not: the
/// operator added one marketplace and this is a directory that marketplace's index
/// pointed at. Inside the tree rather than beside it so that
/// `plugin marketplace remove` takes it away with everything else the marketplace
/// brought down.
const ENTRIES: &str = ".entries";

/// Where the repositories one marketplace's index pointed at are kept.
///
/// `<marketplaces>/.entries/<owner>/<repo>` — the marketplace's own name, so
/// [`discard`] removes an index's fetched repositories with the index itself
/// rather than leaving them in a directory no surface lists. Dot-named, so
/// [`markets`] cannot count one as a marketplace the operator added.
fn entries(marketplaces: &Path, market: &Named) -> PathBuf {
    at(marketplaces.join(ENTRIES).as_path(), market)
}

/// The directory a bundle is actually in, fetching it where it is not here yet.
///
/// **A local source is already on the disk and this does nothing at all.** 53 of
/// the official index's 291 entries are that shape. The other 238 name another
/// repository, and this is where that repository is brought down — through
/// [`crate::fetch`], which owns every spawn, at the commit or the tag the index
/// named, with the URL re-derived from [`crate::fetch::from_url`] so the string
/// reaching git is one io-cli built.
///
/// The `path` an entry names is joined **component by component, plain names
/// only**, exactly as a local source is: it is a value out of a stranger's file
/// and the directory it addresses is inside the clone or the entry addresses the
/// clone's root.
pub fn fetched(
    bundle: &Bundle,
    market: &Named,
    marketplaces: &Path,
    staging: &Path,
) -> Result<PathBuf, String> {
    let Some(crate::adapt::Source::Remote(remote)) = &bundle.source else {
        return Ok(bundle.dir.clone());
    };
    let named = crate::fetch::from_url(&remote.url).ok_or_else(|| {
        let mut out = String::from("io reads a marketplace entry only from ");
        out.push_str(crate::fetch::HOST);
        out.push_str("<owner>/<repo>, and ");
        out.push_str(&bounded(&plain(&remote.url)));
        out.push_str(" is not that, so it is not fetched");
        out
    })?;
    let pin = crate::fetch::Pin::named(remote.reference.as_deref(), remote.sha.as_deref())
        .ok_or_else(|| {
            let mut out = String::from("the index pins ");
            out.push_str(&bundle.label());
            out.push_str(" to something io will not put on a command line, so it is not fetched");
            out
        })?;

    // **Keyed on the marketplace that named it and on the pin, not on the
    // repository alone**, and both halves are defects this would otherwise have.
    //
    // `clone_at` answers `Already` the instant the destination exists, before the
    // pin is consulted, and nothing records which commit a directory holds. Two
    // entries naming one repository at two commits — which is ordinary; 85 of the
    // official index's entries are `git-subdir` into a handful of repositories —
    // would give the second entry the first one's code, adapted from a tree the
    // index did not name for it, with the disclosure truthfully describing the
    // wrong commit. The pin in the path makes `Already` mean *this* commit.
    //
    // Under the marketplace's own `<owner>/<repo>` so that `discard` can take the
    // entries away with the clone that named them. Keyed the other way they would
    // outlive every removal in a directory nothing lists.
    let into = at(&entries(marketplaces, market), &named).join(pin.spelling());
    match crate::fetch::clone_at(&crate::fetch::url(&named), &into, staging, &pin) {
        crate::fetch::Fetched::Cloned(dir) | crate::fetch::Fetched::Already(dir) => {
            Ok(match &remote.path {
                Some(said) => local_dir(&dir, said),
                None => dir,
            })
        }
        // `sentence` answers `None` only for `Cloned`, which the arm above takes,
        // so this is the failure's own words and never a default. The fallback is
        // written all the same rather than unwrapped: an ending added upstream
        // would otherwise turn a refusal into a panic on the consent surface.
        other => Err(other
            .sentence()
            .unwrap_or_else(|| String::from("the fetch did not finish and said nothing"))),
    }
}

/// What io-cli says when it has no home of its own to keep a marketplace in.
///
/// A constant because two surfaces say it: the act itself, and the panel that has
/// no list to draw. [`crate::home::path`]'s own shape — a program that invents a
/// path when it has no home writes into somebody else's — as a sentence rather
/// than a panic or a silent no-op.
pub const NOWHERE: &str =
    "io has no home directory of its own, so there is nowhere to keep a marketplace";

/// How an act ended.
///
/// **Three endings and not a `bool`, because the two that did not change the disk
/// are not the same thing to either door.** A marketplace that was already here is
/// a fine thing to have asked for — the argv form must exit zero and the session
/// form must not draw a refusal over it — while a machine with no git, a clone git
/// rejected and a name that is not here are all failures an operator has to act
/// on. Folding them together is how a script stops being able to tell "it is
/// there" from "it could not be fetched", which is [`crate::fetch::Fetched`]'s own
/// argument one layer down for keeping `NoGit` and `Failed` apart.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Went {
    /// The disk changed.
    Acted,
    /// Nothing changed and nothing is wrong.
    Already,
    /// Nothing changed and something is.
    Refused,
}

/// What an act did, and the one line the operator is owed for it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Outcome {
    /// Which of the three endings it was. The argv door's exit status and the
    /// session door's tone are both this and nothing else, so the two cannot
    /// disagree about whether something happened.
    pub went: Went,
    /// The finished sentence. Never a fragment a caller has to complete: a door
    /// that had to add words would be a second place the wording lives.
    pub said: String,
}

impl Outcome {
    /// The answer when io-cli has no home of its own to work in. See [`NOWHERE`].
    fn homeless() -> Self {
        Outcome {
            went: Went::Refused,
            said: NOWHERE.to_string(),
        }
    }
}

/// What a finished fetch means, as a sentence.
///
/// Split out of [`add`] so that every ending has a test: `add` itself resolves the
/// operator's real home and is therefore three lines nothing under `tests/` can
/// drive without moving `HOME` out from under a suite running in parallel.
///
/// `Cloned` is the one ending that counts what arrived, and it counts it because
/// the question an operator asks straight after adding a marketplace is what is in
/// it. Every other ending carries [`crate::fetch::Fetched::sentence`] verbatim —
/// the words of whoever refused, which for a failed clone is git's own last line.
#[must_use]
pub fn told(named: &Named, fetched: &Fetched) -> Outcome {
    let name = {
        let mut out = named.owner.clone();
        out.push('/');
        out.push_str(&named.repo);
        out
    };
    match fetched {
        Fetched::Cloned(root) | Fetched::Already(root) => {
            let market = Market {
                named: named.clone(),
                bundles: holdings(root),
                root: root.clone(),
            };
            let mut said = name;
            said.push_str(if matches!(fetched, Fetched::Cloned(_)) {
                " is here: "
            } else {
                " was already here: "
            });
            said.push_str(&market.held());
            said.push_str(" in ");
            said.push_str(&root.display().to_string());
            Outcome {
                // **`Already` did not act, and saying otherwise is the lie this
                // field exists to prevent.** `fetch::clone` answers an existing
                // destination before it spawns anything and never clones over it,
                // so an operator who expected an update got none — and it is
                // `Already` rather than `Refused` because having it is what they
                // asked for.
                went: if matches!(fetched, Fetched::Cloned(_)) {
                    Went::Acted
                } else {
                    Went::Already
                },
                said,
            }
        }
        // Carried whole. `sentence` answers `None` only for `Cloned`, which is
        // handled above, so the fallback is unreachable and is written as the
        // fact rather than as an `unwrap`.
        other => Outcome {
            went: Went::Refused,
            said: other
                .sentence()
                .unwrap_or_else(|| String::from("git could not clone that")),
        },
    }
}

/// Delete a marketplace's clone, and nothing else.
///
/// **Criterion F3 lives in what this function cannot reach.** It takes a directory
/// and a name; it holds no configuration, names no scope, and builds no
/// [`crate::edit::Edit`], so there is no path by which it could take a
/// `[[plugin]]` entry away with the clone even if a later edit tried. The sabotage
/// F3 names — removing the entries alongside the clone — would have to add an
/// argument to this signature to be written at all.
///
/// A marketplace that is not there is reported rather than treated as a success:
/// "removed" over a name that was never here tells an operator their typo worked.
#[must_use]
pub fn discard(root: &Path, named: &Named) -> Outcome {
    let clone = at(root, named);
    if !clone.is_dir() {
        let mut said = String::from("no marketplace called ");
        said.push_str(&named.owner);
        said.push('/');
        said.push_str(&named.repo);
        said.push_str(" is here; `plugin marketplace list` shows the ones that are");
        return Outcome {
            went: Went::Refused,
            said,
        };
    }
    // **The repositories this marketplace's index pointed at go with it.** They
    // were fetched only because this index named them, they sit under
    // `.entries/<owner>/<repo>` keyed on this marketplace for exactly that reason,
    // and nothing lists them — a removal that left them behind would leave clones
    // on the disk the operator has no surface to find or delete. Best effort and
    // before the clone, so the sentence below is not written unless the clone
    // itself went.
    let _ = std::fs::remove_dir_all(entries(root, named));
    match std::fs::remove_dir_all(&clone) {
        Ok(()) => {
            let mut said = String::from("the clone is gone: ");
            said.push_str(&clone.display().to_string());
            said.push_str(
                ". No `[[plugin]]` entry was touched — a bundle declared out of this \
                 marketplace is still declared",
            );
            Outcome {
                went: Went::Acted,
                said,
            }
        }
        Err(error) => {
            let mut said = String::from("that marketplace could not be deleted: ");
            said.push_str(&error.to_string());
            Outcome {
                went: Went::Refused,
                said,
            }
        }
    }
}

/// The declared bundles whose directory is inside `clone`.
///
/// **What removing a marketplace actually costs, computed before it is removed.**
/// A bundle declared straight out of a marketplace — which is what `/plugin add`
/// over one of its directories writes — has a `[[plugin]] path` pointing inside
/// the clone. Deleting the clone leaves that entry exactly where it was, which is
/// F3's promise, and leaves io-harness dropping it on the next turn, which is
/// F3's promise read the other way round. Neither half is a defect; being told
/// only the first half is.
///
/// Both of `view`'s lists are read, because a bundle inside the clone may already
/// be refused — a broken manifest does not stop it being the thing that is about
/// to disappear.
///
/// **Compared on resolved paths, and that is not tidiness.** `~` on macOS is
/// reached through `/var` while the same directory canonicalises to `/private/var`,
/// and a `$HOME` that is a symlink is ordinary on a managed machine — so a raw
/// prefix comparison answers "nothing depends on this" for a bundle that is about
/// to stop loading, which is the one wrong answer this function can give.
#[must_use]
pub fn dependents(view: &crate::pluginview::View, clone: &Path) -> Vec<PathBuf> {
    let real = resolved(clone);
    view.plugins
        .iter()
        .map(|listed| listed.root.clone())
        .chain(view.refused.iter().map(|refused| refused.path.clone()))
        .filter(|root| resolved(root).starts_with(&real))
        .collect()
}

/// The adapter directories that stop working when `clone` goes.
///
/// **[`dependents`] cannot see these, and that is a property of the design rather
/// than an oversight.** A bundle installed from a `plugin.toml` is declared at a
/// path *inside* the clone, so removing the clone is visibly removing it. A bundle
/// installed from a Claude Code or Codex manifest is declared at a path under
/// `~/.io-cli/adapters`, which is not inside the clone at all — the generated
/// manifest merely *points* there. Removing the marketplace would leave that entry
/// declared, loading a manifest whose every path has just been deleted, and
/// `dependents` would have said nothing.
///
/// So the link is read where it actually is: the generated manifest's own
/// `skills` and `templates` values, through [`crate::edit::value_at`], which is
/// this crate's only permitted TOML reader. An adapter naming a path inside the
/// clone is an adapter this removal orphans.
///
/// **The `[[plugin]]` entry is not touched and neither is the adapter directory.**
/// 0.29.0's rule is that a cache being emptied does not undo a configuration
/// decision, and an adapter is io's own file rather than the operator's — but it
/// is still not this verb's to delete, because the entry that names it survives.
/// What is owed is the consequence, said before anything goes.
#[must_use]
pub fn orphaned(view: &crate::pluginview::View, clone: &Path, adapters: &Path) -> Vec<PathBuf> {
    let real = resolved(clone);
    let under = resolved(adapters);
    view.plugins
        .iter()
        .map(|listed| listed.root.clone())
        .chain(view.refused.iter().map(|refused| refused.path.clone()))
        .filter(|root| resolved(root).starts_with(&under))
        .filter(|root| points_into(root, &real))
        .collect()
}

/// Whether the adapter at `root` names a path inside `clone`.
///
/// Both of the generated manifest's directory keys are checked, not one: a bundle
/// publishing only `commands/` has a `templates` and no `skills`, and a rule that
/// read `skills` alone would call that adapter unaffected by a removal that guts
/// it.
fn points_into(root: &Path, clone: &Path) -> bool {
    let Ok(text) = std::fs::read_to_string(root.join(MANIFEST)) else {
        return false;
    };
    ["skills", "templates"].iter().any(|key| {
        crate::edit::value_at(&text, key)
            .map(|raw| unquote(raw.trim()))
            .is_some_and(|said| resolved(Path::new(&said)).starts_with(clone))
    })
}

/// A path with its symlinks followed, or the path itself where it cannot be.
///
/// The fallback is the honest one: a path that does not exist cannot be resolved,
/// and refusing to compare it at all would drop a declared bundle whose directory
/// is already gone out of a list that is about to say what is going.
fn resolved(path: &Path) -> PathBuf {
    std::fs::canonicalize(path).unwrap_or_else(|_| path.to_path_buf())
}

/// The line that says what a removal is about to cost, or `None` when it costs
/// nothing.
///
/// One function so that the two doors cannot word this differently, and `None`
/// rather than an empty string so that neither door has to decide whether to draw
/// it — an empty warning drawn as a row is a row an operator reads as a warning.
/// What removing `clone` costs, in one sentence, or `None` when it costs nothing.
///
/// **The one call a door makes**, so the three surfaces that offer this removal
/// cannot ask different questions. Before 0.31.0 each of them called
/// [`dependents`] and then [`warning`]; a release that added a second way for a
/// bundle to depend on a clone would have had to be remembered at all three, and
/// `src/main.rs` is linked by nothing under `tests/`, so forgetting one would have
/// been invisible to every gate.
///
/// `adapters` is an `Option` because [`crate::home::adapters`] is: an operator
/// with no home directory has no adapters, and a `None` there is not an error, it
/// is an empty list.
#[must_use]
pub fn removal_cost(
    view: &crate::pluginview::View,
    clone: &Path,
    adapters: Option<&Path>,
) -> Option<String> {
    let inside = dependents(view, clone);
    let orphans = adapters
        .map(|at| orphaned(view, clone, at))
        .unwrap_or_default();
    warning(&inside, &orphans)
}

/// The line that says what a removal is about to cost, given both lists.
#[must_use]
pub fn warning(inside: &[PathBuf], orphans: &[PathBuf]) -> Option<String> {
    if inside.is_empty() && orphans.is_empty() {
        return None;
    }
    // **Counted together and listed together.** The two lists differ in where the
    // declaration points and in nothing an operator cares about: both are bundles
    // that stop loading, both keep their `[[plugin]]` entry, and both are named so
    // the operator can decide before anything goes. Two sentences would ask them
    // to work out which kind theirs was in order to read the consequence.
    let all: Vec<&PathBuf> = inside.iter().chain(orphans.iter()).collect();
    let mut said = if all.len() == 1 {
        String::from("1 declared bundle stops loading")
    } else {
        format!("{} declared bundles stop loading", all.len())
    };
    said.push_str(
        ": the `[[plugin]]` entries are left exactly as they are, and io-harness will \
         report them as missing from the next turn — ",
    );
    said.push_str(
        &all.iter()
            .map(|path| path.display().to_string())
            .collect::<Vec<_>>()
            .join(", "),
    );
    if !orphans.is_empty() {
        // Named as adapters rather than left to look like ordinary directories: an
        // operator who goes looking finds a `plugin.toml` io wrote, in a directory
        // they never made, and the sentence is the only thing that explains it.
        said.push_str(
            ". The ones under `adapters/` are manifests io generated; they are not \
             removed, and they stop loading because the clone they name is going",
        );
    }
    Some(said)
}

// --- asking for a bundle by name ----------------------------------------------

/// A query split into `<name>` and the `@<marketplace>` that qualifies it.
///
/// `rsplit_once`, so the **last** `@` is the separator: a bundle whose own name
/// carries one is still reachable, and the qualifier is the part nothing else can
/// contain. A query with nothing on one side of the `@` — `@thing`, `thing@` — is
/// read as a whole name rather than as half a qualification, because the half that
/// is missing is the half that would decide, and guessing it is how a name resolves
/// to a marketplace nobody asked for.
fn asked(query: &str) -> (&str, Option<&str>) {
    match query.rsplit_once('@') {
        Some((name, market)) if !name.is_empty() && !market.is_empty() => (name, Some(market)),
        _ => (query, None),
    }
}

/// The directory of the bundle called `query`, across every marketplace here.
///
/// The name is [`Bundle::label`]'s — the manifest's `name`, and a nameless
/// manifest's directory — because that is the word the listing drew and therefore
/// the word an operator has to type. A qualifier is matched against **both**
/// spellings of a marketplace, `<owner>/<repo>` and the repository alone, since the
/// second is what a hand reaches for and the first is what is always unique.
///
/// **Two marketplaces carrying one name is a refusal and never a first match**, and
/// that is F4's named sabotage. The two bundles are two different repositories'
/// code: resolving to whichever the walk happened to reach first installs something
/// the operator did not choose, silently, under a name they believed meant one
/// thing. The refusal spells every way of choosing (`offer`), so the fix is a
/// paste rather than a lookup.
///
/// **A qualifier that names one bundle's own directory beats one that names the
/// whole marketplace.** One clone can hold two bundles under one label — a
/// repository that ships `plugins/rust-review` and a fixture copy of it under
/// `tests/` is the ordinary shape of that — and the marketplace's name then
/// qualifies neither of them. Without this narrowing the refusal offered one
/// spelling twice and both copies answered to it, which made the bundle
/// permanently uninstallable by name: every spelling on offer returned the refusal
/// that offered it. So `qualified` is matched too, and an exact hit on one
/// directory wins over the marketplace-wide reading it also satisfies — which is
/// what makes `<name>@<owner>/<repo>` keep resolving to a clone's own root bundle
/// when that root shares its label with something deeper.
pub fn locate(markets: &[Market], query: &str) -> Result<PathBuf, String> {
    located(markets, query).map(|(_, bundle)| bundle.dir.clone())
}

/// [`locate`], keeping the marketplace and the bundle rather than the path alone.
///
/// **The install needs all three and a `PathBuf` throws two of them away.** An
/// adapted bundle's directory is not the directory a `[[plugin]]` entry names —
/// the entry names the generated manifest, which lives under
/// `<adapters>/<owner>/<repo>/<name>`, and the owner and the repository come from
/// the [`Market`] while the name comes from the [`Bundle`]. A remote entry's
/// directory does not exist at all until [`fetched`] has run, and what says so is
/// [`Bundle::source`].
///
/// One matcher and two returns, so `plugin add` and every listing agree about
/// which bundle a word names. [`locate`] is this, narrowed.
pub fn located<'a>(markets: &'a [Market], query: &str) -> Result<(&'a Market, &'a Bundle), String> {
    let (name, qualifier) = asked(query);
    let hits: Vec<(&Market, &Bundle)> = markets
        .iter()
        .flat_map(|market| market.bundles.iter().map(move |bundle| (market, bundle)))
        .filter(|hit| {
            let (market, bundle) = *hit;
            bundle.label() == name
                && qualifier.is_none_or(|which| {
                    which == market.name()
                        || which == market.named.repo
                        || which == qualified(market, bundle)
                })
        })
        .collect();
    let precise: Vec<(&Market, &Bundle)> = match qualifier {
        Some(which) => hits
            .iter()
            .copied()
            .filter(|hit| which == qualified(hit.0, hit.1))
            .collect(),
        None => Vec::new(),
    };
    let hits = if precise.len() == 1 { precise } else { hits };
    match hits.as_slice() {
        [(market, bundle)] => Ok((market, bundle)),
        [] => Err(unheld(markets, query)),
        several => {
            let spellings = several
                .iter()
                .map(|hit| format!("`{}`", offer(hit.0, hit.1)))
                .collect::<Vec<_>>()
                .join(" and ");
            // **The count is of marketplaces only when there is more than one of
            // them.** Counting hits and spelling the qualifier from the
            // marketplace's name told an operator that "2 marketplaces" held the
            // name when one did, and then named that one marketplace twice.
            let mut which: Vec<String> = several.iter().map(|hit| hit.0.name()).collect();
            which.sort();
            which.dedup();
            let held = if let [only] = which.as_slice() {
                let count = several.len();
                format!("{count} bundles in the marketplace `{only}` are called `{name}`")
            } else {
                let count = which.len();
                format!("{count} marketplaces here hold a bundle called `{name}`")
            };
            Err(format!(
                "{held}, and installing whichever was found first would install code you did \
                 not choose; say which one: {spellings}"
            ))
        }
    }
}

/// `<owner>/<repo>` and then the bundle's own path inside that clone.
///
/// The one qualifier that is unique for every bundle on the disk: a marketplace
/// name is unique by the layout [`crate::fetch::at`] writes, and no two bundles in
/// one clone sit in one directory. Always `/` between the segments, whatever the
/// platform spells a path with, because this is a word an operator types and pastes
/// out of a refusal rather than a path anything opens.
///
/// A clone's own root bundle has nothing to add and is qualified by the marketplace
/// name alone — which is what [`locate`]'s narrowing needs it to be, so that the
/// shortest spelling still names something exactly.
fn qualified(market: &Market, bundle: &Bundle) -> String {
    let mut out = market.name();
    let inside = bundle
        .dir
        .strip_prefix(&market.root)
        .unwrap_or(bundle.dir.as_path());
    for part in inside.components() {
        out.push('/');
        out.push_str(&part.as_os_str().to_string_lossy());
    }
    out
}

/// The spelling of `bundle` that [`locate`] resolves to it and to nothing else.
///
/// **Every surface that offers a spelling offers this one**, so a refusal, the
/// list of what is here, `plugin search` and the guided browser's prefilled
/// `/plugin add` cannot disagree about what to type — and none of them can hand an
/// operator a string that returns the refusal it came from, which is what the
/// marketplace's name alone did for two bundles sharing a label inside one clone.
/// Public for that last one: the driver has a `Market` and a `Bundle` in hand and
/// spelling `<label>@<market>` there a second time is how the two drift.
///
/// The shortest spelling that is unambiguous: the marketplace's name where the
/// label is unique in it, and the bundle's own directory where it is not. A
/// qualifier longer than it needs to be is one an operator retypes wrongly.
///
/// ponytail: the scan is over one marketplace's bundles per call, so a listing is
/// quadratic in a single clone's bundle count. A clone holding enough bundles for
/// that to be measurable is a clone whose listing is unreadable for other reasons.
#[must_use]
pub fn offer(market: &Market, bundle: &Bundle) -> String {
    let label = bundle.label();
    let copies = market
        .bundles
        .iter()
        .filter(|other| other.label() == label)
        .count();
    let which = if copies > 1 {
        qualified(market, bundle)
    } else {
        market.name()
    };
    format!("{label}@{which}")
}

/// What to say when no marketplace holds the name that was asked for.
///
/// The list of what *is* here is the whole point of the sentence: a bare "not
/// found" over a set of clones an operator fetched weeks ago leaves them running
/// `plugin marketplace list` and then descending into each one. Every bundle is
/// named in the qualified form (`offer`), which is the form that always resolves.
fn unheld(markets: &[Market], query: &str) -> String {
    let held: Vec<String> = markets
        .iter()
        .flat_map(|market| {
            market
                .bundles
                .iter()
                .map(move |bundle| format!("`{}`", offer(market, bundle)))
        })
        .collect();
    if held.is_empty() {
        return format!(
            "no marketplace here holds a bundle called `{query}`, and no marketplace here holds \
             any bundle at all; `plugin marketplace add <owner>/<repo>` fetches one"
        );
    }
    format!(
        "no marketplace here holds a bundle called `{query}`; the bundles that are here are {}",
        held.join(", ")
    )
}

/// Which directory `plugin add <word>` means: the path, or the bundle of that name.
///
/// **The rule is one question asked of the disk, and it is asked of the path
/// first.** A word is a path when it resolves to a directory carrying a
/// [`MANIFEST`] — which is [`crate::pluginview::refusal`]'s own check, the one this
/// verb already had to pass before it wrote anything — and it is a name in every
/// other case. Nothing about the *shape* of the word is read: a rule keyed on a
/// `/`, a leading `.` or an extension would make `plugin add rust-review` mean a
/// directory on a machine that has one and a marketplace bundle on a machine that
/// does not, which is the same word doing two things depending on the operator's
/// working directory. Asking the disk cannot drift, and it cannot make a real
/// relative path unusable: a directory that is a bundle is always read as one.
///
/// The two readings' refusals are joined rather than chosen between, because by the
/// time both have failed io-cli genuinely does not know which the operator meant,
/// and picking one to report is picking which half of the answer to hide.
///
/// `markets` is a closure so that an ordinary `plugin add ./bundles/rust-review`
/// walks no marketplace at all — the tree is only read once the path has already
/// failed.
pub fn chosen(
    dir: &Path,
    markets: impl FnOnce() -> Vec<Market>,
    text: &str,
    at: Homes<'_>,
) -> Result<Chosen, String> {
    match crate::pluginview::refusal(dir) {
        None => Ok(Chosen::Path(dir.to_path_buf())),
        Some(refused) => prepared(&markets(), text, at.marketplaces, at.staging, at.adapters)
            .map(Chosen::Held)
            .map_err(|missing| format!("{refused} — {missing}")),
    }
}

/// The three directories an install may need, passed in rather than looked up.
///
/// One argument instead of three, because [`chosen`] already takes three and a
/// fourth, fifth and sixth positional `&Path` is a call nobody can read. Grouped
/// rather than resolved inside for this module's standing reason: a decision
/// behind [`crate::home`] is a decision nothing under `tests/` can reach without
/// moving the operator's home out from under a suite running in parallel.
#[derive(Debug, Clone, Copy)]
pub struct Homes<'a> {
    /// Where marketplaces are cloned to.
    pub marketplaces: &'a Path,
    /// Where a clone is assembled before it is renamed into place.
    pub staging: &'a Path,
    /// Where io writes the manifests it generates.
    pub adapters: &'a Path,
}

/// A bundle brought to the point where a `[[plugin]]` entry can name it.
///
/// Three directories rather than one, because for an adapted bundle they are three
/// different places and every surface downstream needs a different one.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Prepared {
    /// What the `[[plugin]]` entry names. The bundle's own directory for a native
    /// bundle; the generated manifest's directory for an adapted one.
    pub declare: PathBuf,
    /// The bundle's own directory — the author's files. What the disclosure reads
    /// hooks from, because the generated manifest carries none.
    pub from: PathBuf,
    /// The adapter io built **for this install**, standing beside
    /// [`Prepared::declare`] and not yet in it. `None` for a native bundle, which
    /// needed nothing generated.
    ///
    /// **This replaced `made`, which was the directory a decline deleted**, and
    /// the reason it could be replaced is the reason the field existed. `made` was
    /// `None` for an adapter that was already there, because the generator had
    /// swapped the new adapter in before anyone was asked and deleting the
    /// directory on a decline would have taken away the bundle the operator
    /// installed last week. That distinction has no work left to do: the swap is
    /// [`crate::adapt::Staged::commit`] and it runs *after* the answer, so on a
    /// first install and on an update alike a decline discards a staging directory
    /// and `declare` is untouched either way. One field, one answer, and no case
    /// where declining reaches a directory a `[[plugin]]` entry names.
    pub staged: Option<crate::adapt::Staged>,
    /// The directories the adapter copied out of the clone, in
    /// [`crate::adapt::Adapter::copied`]'s own words — `skills`, `templates`, or
    /// both. Empty for a native bundle, which is declared where it sits.
    ///
    /// **Carried because an adapter is a snapshot and an operator has to be told
    /// so.** io-harness 0.74.0 refuses a `skills` or `templates` pointing out of
    /// the bundle in every scope, so an adapter ships copies rather than pointing
    /// into the clone; a `git pull` there therefore changes nothing until the
    /// bundle is installed again. [`adapted_disclosure`] is where that is said, in
    /// the one place the operator is already reading before they consent.
    pub copied: Vec<String>,
}

/// Resolve a word to a bundle and bring it to the point of being installable.
///
/// **This is the step 0.29.0 and 0.30.0 did not need and 0.31.0 cannot do
/// without.** Until this release every bundle a marketplace held already carried
/// a `plugin.toml`, so resolving a name to a directory *was* the install. A bundle
/// published as a Claude Code or Codex plugin carries no manifest io-harness
/// reads, and one in another repository is not on the disk at all, so the word has
/// to be resolved, fetched and adapted before there is anything an entry can name.
///
/// Native bundles are untouched by every line of it: [`fetched`] returns their own
/// directory unchanged and the generation is skipped, so `plugin add` on a
/// `plugin.toml` marketplace does exactly what it did in 0.30.2.
///
/// **Installing a bundle again is how an adapter is updated, and it is the only
/// way.** [`crate::adapt::generate`] rebuilds the adapter directory on every run —
/// it does not merge, so a file the author withdrew upstream cannot survive in
/// io's home and go on being read into the model's prompt — and since 0.35.0 that
/// directory holds *copies* of the bundle's `skills/` and `commands/` rather than
/// a manifest pointing at the clone. io-harness 0.74.0 is why: it refuses a
/// `skills` or `templates` that leaves the bundle, in every scope, because every
/// `*.md` under one reaches the model's system prompt. So a clone the operator
/// pulled is a clone this session has not read, and running the install again is
/// what moves it across. [`Prepared::copied`] is what moved, and
/// [`adapted_disclosure`] says so before the operator answers.
///
/// **The adapter is built before consent and put in place after it.** The
/// disclosure is io-harness reading the bundle, and io-harness has no loader that
/// takes a foreign manifest — so there is nothing to read until the manifest io
/// generates exists, and building it is not a thing consent can come before. What
/// consent decides is the *swap*: until 0.38.0 [`crate::adapt::generate`] ended by
/// replacing the destination, so declining a refresh left the new adapter over the
/// one the operator already had. [`Prepared::staged`] is what a declined install
/// discards, [`Prepared::declare`] is untouched until an accepted one commits, and
/// the operator's configuration file is not opened either way.
pub fn prepared(
    markets: &[Market],
    query: &str,
    marketplaces: &Path,
    staging: &Path,
    adapters: &Path,
) -> Result<Prepared, String> {
    let (market, bundle) = located(markets, query)?;
    let from = fetched(bundle, &market.named, marketplaces, staging)?;
    // **The disk decides, not how the bundle was listed.** `from_entry` stamps
    // `Origin::Adapted` on every index entry, because an index is a foreign format
    // — but the directory it points at may still carry io's own manifest, and a
    // remote entry's repository is not even read until the fetch above has run. A
    // rule keyed on `origin` would generate an adapter over an author's real
    // `plugin.toml`, silently dropping the `[[hook]]`, `[[mcp]]` and `[[agent]]`
    // blocks the generator has no source for. `holdings` says a native manifest
    // wins its own directory; this is that same sentence on the install path.
    if from.join(MANIFEST).is_file() {
        return Ok(Prepared {
            declare: from.clone(),
            from,
            staged: None,
            copied: Vec::new(),
        });
    }
    // The **normalised id**, not the label. It is what the generated manifest
    // declares and what io-harness namespaces by, so a directory named anything
    // else would put two spellings of one bundle on the disk — and on a
    // case-insensitive filesystem two bundles labelled `Foo` and `foo` would share
    // one adapter directory and the second install would overwrite the first.
    let name = crate::adapt::normalised(&bundle.label()).ok_or_else(|| {
        format!(
            "`{}` is not a usable plugin id, so io cannot name a directory for it",
            bundle.label()
        )
    })?;
    let into = crate::adapt::at(adapters, &market.named.owner, &market.named.repo, &name);
    // **An install and a re-install are no longer told apart here, and nothing
    // downstream asks.** Until 0.38.0 `into.is_dir()` was read *before* the
    // generator, because the generator swapped the new adapter in on the way past
    // and there was no way to tell the two cases apart afterwards — and the answer
    // decided whether declining was allowed to delete the directory. `generate`
    // now leaves the adapter staged, so declining reaches `into` in neither case
    // and the question has no consequence left to have.
    let written = crate::adapt::generate(&from, &name, &into)?;
    Ok(Prepared {
        declare: into,
        from,
        staged: Some(written.staged),
        copied: written.copied,
    })
}

/// Put what an install built in place, because the operator said yes.
///
/// A no-op for a native bundle, which built nothing: it is declared where it sits.
/// The pair of [`unmake`], and one of the two runs on every path out of a
/// disclosure — see [`crate::adapt::Staged`].
///
/// # Errors
///
/// [`crate::adapt::Staged::commit`]'s, naming the destination and what the rename
/// said. A caller that gets one must not go on to write the `[[plugin]]` entry:
/// there is no adapter at the path it would name.
pub fn make(staged: Option<&crate::adapt::Staged>) -> Result<(), String> {
    staged.map_or(Ok(()), crate::adapt::Staged::commit)
}

/// Throw away what an install built, because the operator said no.
///
/// A no-op for a native bundle, which built nothing. Best effort by design: the
/// operator has already declined, the entry was never written and the adapter they
/// already had was never touched, so a directory that cannot be removed is a stale
/// cache rather than anything they consented to, and a refusal reported *after* a
/// refusal is noise about io's own housekeeping.
pub fn unmake(staged: Option<&crate::adapt::Staged>) {
    if let Some(staged) = staged {
        staged.discard();
    }
}

/// Which reading of the word won, and the directory it named.
///
/// **The reading is the disclosure rule, and it is recorded here so that nothing
/// downstream has to ask the question twice.** A directory the operator typed is a
/// directory the operator has; a bundle resolved out of a marketplace is a
/// stranger's code that no one on this machine has read. The first is declared on,
/// which is `/plugin add`'s behaviour since 0.28.0 and is not a thing to change
/// under someone who typed a path they trust; the second is declared
/// `enabled = false` and disclosed before it is switched on. See the module docs.
///
/// The distinction rides on [`chosen`]'s own answer rather than on the shape of
/// the word, for [`chosen`]'s own reason: a rule keyed on a `/` or a leading `.`
/// would make one word disclose in one working directory and not in another.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Chosen {
    /// A directory on this machine, named by the operator.
    Path(PathBuf),
    /// A bundle a marketplace holds, resolved by name, fetched where it was
    /// elsewhere and adapted where its manifest is not one io-harness reads.
    Held(Prepared),
}

impl Chosen {
    /// The directory a `[[plugin]]` entry names.
    #[must_use]
    pub fn dir(&self) -> &Path {
        match self {
            Self::Path(dir) => dir,
            Self::Held(prepared) => &prepared.declare,
        }
    }

    /// The bundle's **own** directory, which for an adapted bundle is not
    /// [`Chosen::dir`].
    ///
    /// What the disclosure reads hooks from: the generated manifest carries none,
    /// so asking io-harness what the bundle contributes cannot answer what its
    /// author declared.
    #[must_use]
    pub fn from(&self) -> &Path {
        match self {
            Self::Path(dir) => dir,
            Self::Held(prepared) => &prepared.from,
        }
    }

    /// The directory io-harness is asked to load **now**, which for an adapted
    /// bundle is not [`Chosen::dir`].
    ///
    /// A staged adapter is not at its destination yet: on a first install that
    /// destination does not exist, and on an update it still holds the *previous*
    /// adapter — so a disclosure read from [`Chosen::dir`] would describe either
    /// nothing or last week's bundle, and the operator would be answering about a
    /// directory that is not the one they are being offered. [`Chosen::dir`] stays
    /// what the `[[plugin]]` entry names and what the operator is shown, because
    /// that is where the adapter is a moment after they say yes.
    #[must_use]
    pub fn read(&self) -> &Path {
        match self {
            Self::Path(dir) => dir,
            Self::Held(prepared) => prepared
                .staged
                .as_ref()
                .map_or(prepared.declare.as_path(), |staged| {
                    staged.staging.as_path()
                }),
        }
    }

    /// What an accepted install commits and a declined one discards, if this
    /// install built anything.
    #[must_use]
    pub fn staged(&self) -> Option<&crate::adapt::Staged> {
        match self {
            Self::Path(_) => None,
            Self::Held(prepared) => prepared.staged.as_ref(),
        }
    }

    /// The directories this install copied out of the clone. See
    /// [`Prepared::copied`].
    ///
    /// Empty for a directory the operator typed, which nothing copied anywhere.
    #[must_use]
    pub fn copied(&self) -> &[String] {
        match self {
            Self::Path(_) => &[],
            Self::Held(prepared) => &prepared.copied,
        }
    }

    /// Whether declaring it owes the operator a disclosure before it loads.
    ///
    /// One method rather than a `matches!` at the call site, so the rule has one
    /// home and a sabotage of it fails everywhere at once.
    #[must_use]
    pub fn discloses(&self) -> bool {
        matches!(self, Self::Held(_))
    }
}

/// What a bundle turned out to be, before anything was written about it.
///
/// The id is io-harness's — the manifest's `name`, which is also what every
/// contribution is namespaced by — so the confirmation is titled with the word the
/// operator will see in a trace, and never with the directory it happened to sit
/// in.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Disclosure {
    /// The bundle's id, for the confirmation's title.
    pub id: String,
    /// One row per fact, in io-harness's own contribution order — what `/plugin`'s
    /// own pane draws, so the two surfaces cannot say different things about one
    /// bundle.
    ///
    /// **One row is not the pane's**, and it is here rather than in a field of its
    /// own so that no door has to learn about it: the last row of an adapted
    /// bundle's disclosure names the directories io copied out of the clone. See
    /// [`adapted_disclosure`] for why an operator is owed it.
    ///
    /// Rows rather than finished lines because the glyph set belongs to the door:
    /// the TUI writes into a terminal whose set the operator chose, and the argv
    /// form writes down a pipe in ASCII. [`Disclosure::said`] is the fold.
    pub rows: Vec<Row>,
    /// One line per hook the bundle declares that io will **not** carry across,
    /// each with its event, its command unshortened, and the reason.
    ///
    /// **Empty for a native bundle, and not because the field does not apply.** A
    /// `plugin.toml` declares its hooks to io-harness, which runs them; there is
    /// nothing being withheld and nothing to say. This list is only ever the hooks
    /// of a Claude Code or Codex bundle, which cannot cross — see
    /// [`crate::adapt::Hook`] for why no adapter closes that gap.
    ///
    /// A separate field rather than more [`Disclosure::rows`], so the count of
    /// withheld hooks is a number a test can assert. One row per hook, and F11
    /// asserts the count rather than a `contains`, because one row satisfies a
    /// `contains` forever and a hook that exists and is not drawn is the failure
    /// this exists to prevent.
    pub withheld: Vec<String>,
}

impl Disclosure {
    /// One finished line per row, in the caller's own glyph set.
    ///
    /// Written into the scrollback a line at a time rather than drawn as a list to
    /// choose from: every row of a confirmation past index 0 **acts**
    /// ([`crate::store::acts`]), so a fact drawn as a row would be a fact an
    /// operator could accidentally consent with.
    #[must_use]
    pub fn said(&self, glyphs: &Glyphs) -> Vec<String> {
        self.rows
            .iter()
            .map(|row| match &row.detail {
                Some(detail) => format!("{}{}{detail}", row.label, glyphs.separator),
                None => row.label.clone(),
            })
            .collect()
    }
}

/// What declaring the bundle at `dir` from `scope` would bring, or io-harness's
/// own sentence saying it would bring nothing.
///
/// **Nothing has been written when this is called, and that is the whole of
/// criterion F17.** [`io_harness::Plugins::inspect`] (0.71.0) runs `load_one` —
/// the same loader `Config::plugins` runs, so a preflight and a load cannot
/// disagree — against a directory that is in no configuration file. Through
/// 0.29.0 there was no such entry point, so the install wrote a `[[plugin]]` entry
/// `enabled = false`, re-discovered, and read the answer off the resulting `View`:
/// a bundle io-harness refused left its entry behind in a file the operator never
/// agreed to have edited. Now the refusal happens with the file untouched, and the
/// write is [`crate::pluginview::add`], once, on consent.
///
/// **`scope` is the answer and not a formality.** It is the scope the entry would
/// be declared from, and io-harness answers differently by it on purpose: at
/// `User` and `Local` a bundle's `[[hook]]` and `[[mcp]]` are its own business,
/// and at `Project` — the committed `io.toml` a `git clone` delivers — the same
/// manifest is refused **whole** rather than shortened. A bundle that would only
/// load from one of the two is exactly what an install has to say before it writes
/// anything, so the scope handed here is the scope the caller is about to write.
///
/// `Err` is io-harness's sentence, whole and re-worded by nobody — including the
/// one for a `${env:}`, `${file:}` or `${cmd:}` substitution, which 0.71.0 refuses
/// inside a manifest in **every** scope and names by the offending key's dotted
/// path. A manifest is the one file here nobody has agreed to, and resolving one of
/// those would read this machine, or run a program on it, for a directory the
/// operator is still deciding about.
///
/// Every name is already namespaced — `inspect` rewrites an agent, a server id and
/// a layer name to `<plugin>__<name>` exactly as a load does — so the operator
/// consents to the strings a refusal, a call and a spawn will use. A disclosure
/// composed out of the manifest would say `reviewer` for a bundle that contributes
/// `rust-review__reviewer`, which is a name they will never see again.
///
/// The rows come from [`crate::pluginview::detail`], the renderer `/plugin`'s own
/// pane uses, so the pane an operator opens afterwards says the same thing in the
/// same order.
///
/// # Nothing on a consent surface is elided, and the hook rows are why
///
/// [`crate::pluginview::detail`] shortens a row's detail to the width it is given,
/// and on an eighty-column terminal that cuts a hook's `run` array with an
/// ellipsis — so the operator consented to a truncated argv, on the one
/// contribution kind that runs programs. There is therefore no width on this
/// signature to get wrong: the renderer is handed `u16::MAX`, and
/// [`Disclosure::said`] hands the lines to the scrollback one at a time where the
/// terminal wraps them, rather than into a fixed-width picker.
///
/// **That is also why no glyph set is taken.** At `u16::MAX` nothing is shortened,
/// so the only glyph a row could carry is the ellipsis that says it was — and
/// there is none. The set the renderer is handed cannot reach the output, and the
/// one that *does* show, the separator between a row's two fields, is applied by
/// the door in [`Disclosure::said`].
pub fn disclosure(scope: Scope, dir: &Path) -> Result<Disclosure, String> {
    adapted_disclosure(scope, dir, dir, None, &[])
}

/// The sentence every withheld hook carries, spelled once.
///
/// **Three reasons and not one, because an operator who is told "unsupported"
/// cannot tell whether to wait for a release or to stop expecting it.**
/// io-harness's `Hook.run` is argv and deliberately never a shell string; its `on`
/// takes the harness's own event tags; and 0.71.0 refuses `${env:}`, `${file:}`
/// and `${cmd:}` inside a manifest in every scope. A Claude hook is a shell
/// string, an unknown event and a refused substitution at once, so no adapter
/// closes it — and an approximated hook is a program running on this machine that
/// nobody described accurately.
pub const NOT_CARRIED: &str = "io does not run this — a hook here is a shell line for another \
                               tool's events, and io-harness takes argv against its own";

/// [`disclosure`], plus the hooks a foreign bundle declares and io will not carry.
///
/// `from` is the bundle's **own** directory — the stranger's checkout — where
/// `dir` is the adapter, which for an adapted bundle is the generated manifest's
/// directory rather than the author's. The two differ precisely because the adapter
/// carries no hooks: asking io-harness what the bundle contributes therefore
/// cannot answer what it *declared*, and reading the author's own file is the only
/// way to name what is being left behind.
///
/// `None` for a native bundle, where the two are the same directory and nothing is
/// withheld.
///
/// `copied` is [`Prepared::copied`] — the directories this install moved out of the
/// clone, in the generator's own words rather than in a second reading of the
/// adapter directory. It becomes one more row, and it is the answer to the question
/// an operator asks a week later: they edited a skill inside the clone, or pulled
/// the repository, and the session went on using what io copied. Empty for every
/// bundle that copied nothing, where the row would be a warning about nothing.
///
/// **`read` is the directory io-harness is handed and `dir` is the one the
/// operator is told about, and they differ for exactly one reason.** A staged
/// adapter ([`crate::adapt::Staged`]) is not at `dir` yet — the swap is what
/// consent buys — so reading `dir` on an update would disclose the bundle the
/// operator already has rather than the one they are being offered, and on a first
/// install would refuse a directory that is not there. [`Chosen::read`] is the one
/// place that answer is worked out. Pass `dir` twice where nothing is staged.
///
/// Every path io-harness answers with is then re-rooted from `read` onto `dir`, so
/// what the operator is shown is where the adapter will be rather than where it
/// was read from — the two rows would otherwise name two different directories for
/// one bundle.
pub fn adapted_disclosure(
    scope: Scope,
    dir: &Path,
    read: &Path,
    from: Option<&Path>,
    copied: &[String],
) -> Result<Disclosure, String> {
    let plugin = io_harness::Plugins::inspect(scope, read).map_err(|error| error.to_string())?;
    // `true`, because nothing has been written: this is what the directory *is*,
    // not what some `[[plugin]]` entry said about it. See `pluginview::copy_out`.
    let mut listed = crate::pluginview::copy_out(&plugin, true);
    // **Re-rooted onto the destination, because io-harness answered about the
    // directory it was handed.** `detail` draws the contributed directories as
    // whole paths, and for a staged adapter every one of them is under a hidden
    // sibling that exists only until the operator answers — so the rows would send
    // them somewhere that is gone a moment later, and would disagree with the
    // `copied` row below, which names where the adapter lands. The prefix stripped
    // is `Plugin::root` rather than `read`, so the swap holds whether or not
    // io-harness canonicalised what it was given. Nothing else in `listed` is a
    // path under the adapter: an `[[mcp]]` argv points into the clone, and the
    // generated manifest declares no `[[bin]]` and no `[[hook]]` at all.
    if read != dir {
        let under = std::mem::replace(&mut listed.root, dir.to_path_buf());
        listed.skills = listed.skills.take().map(|at| rerooted(&at, &under, dir));
        listed.templates = listed.templates.take().map(|at| rerooted(&at, &under, dir));
    }
    let mut rows = crate::pluginview::detail(&listed, u16::MAX, &crate::glyphs::ASCII);
    // A row rather than a field of its own, because a field is a line only a door
    // that knows about it prints — and both doors already print every row of this,
    // in the order they come. The paths are whole: this surface elides nothing (see
    // above), and the adapter's path is the directory an operator goes looking for
    // when they want to know what io is reading.
    if let Some(source) = from.filter(|_| !copied.is_empty()) {
        rows.push(Row::with_detail(
            format!("copied `{}`", copied.join("` and `")),
            format!(
                "out of {} into {} — an adapter ships the directories it contributes rather than \
                 pointing at somebody else's checkout, so an edit or a `git pull` in the clone \
                 reaches this session when the bundle is installed again, and not before",
                source.display(),
                dir.display(),
            ),
        ));
    }
    Ok(Disclosure {
        id: listed.id.clone(),
        rows,
        withheld: from.map(withheld_hooks).unwrap_or_default(),
    })
}

/// `path` with its `from` prefix replaced by `to`, or unchanged where it has none.
///
/// Unchanged rather than refused, because this answers a consent surface: a path
/// that is one directory out of date is worth less than the disclosure it would
/// otherwise take down.
fn rerooted(path: &Path, from: &Path, to: &Path) -> PathBuf {
    path.strip_prefix(from)
        .map_or_else(|_| path.to_path_buf(), |rest| to.join(rest))
}

/// One line per hook the bundle at `from` declares, with its command unshortened.
///
/// **The command is filtered and never bounded**, which is the one place this
/// module's `LONGEST` deliberately does not apply: it is argv the operator is
/// being asked to consent to being *absent*, and a shortened argv on a consent
/// surface is the single thing that surface must never show.
fn withheld_hooks(from: &Path) -> Vec<String> {
    let mut found = Vec::new();
    for at in hook_files(from) {
        for hook in crate::adapt::hooks_in(&at) {
            let mut said = hook.event.clone();
            said.push_str(" — ");
            said.push_str(&hook.command);
            said.push_str(" — ");
            said.push_str(NOT_CARRIED);
            found.push(said);
        }
    }
    found
}

/// The hooks files a foreign bundle declares or keeps where they are conventional.
///
/// **Two places, and the manifest's own answer comes first.** A Codex manifest
/// names `"hooks": "./hooks/hooks.json"`, and a Claude bundle generally names
/// nothing and keeps the file at that same conventional path — so looking only at
/// the manifest would miss most bundles and looking only at the convention would
/// miss a bundle that moved it. The conventional path is not added twice when the
/// manifest already named it.
///
/// The declared path is joined **component by component, plain names only**, the
/// way every other value out of a stranger's file is: it decides which file io
/// opens, and `../../.ssh/id_ed25519` must address nothing.
///
/// Path discovery only. The reading is [`crate::adapt`]'s, which is the module
/// permitted to parse JSON.
fn hook_files(from: &Path) -> Vec<PathBuf> {
    let mut found = Vec::new();
    if let Some(said) = crate::adapt::manifest_at(from).and_then(|read| read.hooks) {
        found.push(local_dir(from, &said));
    }
    let conventional = from.join("hooks").join("hooks.json");
    if !found.contains(&conventional) {
        found.push(conventional);
    }
    found.retain(|path| path.is_file());
    found
}

/// Every bundle whose name or description carries `text`, in [`markets`]' order.
///
/// **Across every marketplace, and F5's named sabotage is stopping at the first
/// one.** An operator adds a second marketplace precisely because the first did not
/// have what they needed, so a search that never reaches it answers "nothing" about
/// the one repository they fetched for this.
///
/// Matched case-insensitively over the label and the description, because a
/// description is a sentence somebody wrote for a human and the name is a
/// lowercase identifier — a case-sensitive search over the pair finds one or the
/// other and never both.
///
/// One finished line each, and the first field is `offer`'s **qualified
/// spelling**, which is exactly what `plugin add` takes: the answer to "what is out
/// there" is then also the thing to paste, with no second lookup to work out which
/// marketplace the hit came from — and never the same string twice, which is what
/// the marketplace's name alone printed for two bundles sharing a label inside one
/// clone.
///
/// The description is [`Bundle::line`]'s and is therefore already
/// control-character-free and bounded — see `declared`. A hit is one line, and a
/// stranger's manifest does not get to decide how many.
#[must_use]
pub fn matching(markets: &[Market], text: &str) -> Vec<String> {
    let needle = text.to_lowercase();
    markets
        .iter()
        .flat_map(|market| market.bundles.iter().map(move |bundle| (market, bundle)))
        .filter(|(_, bundle)| {
            bundle.label().to_lowercase().contains(&needle)
                || bundle
                    .description
                    .as_deref()
                    .is_some_and(|said| said.to_lowercase().contains(&needle))
        })
        .map(|(market, bundle)| format!("{} · {}", offer(market, bundle), bundle.line()))
        .collect()
}

// --- the three that reach the operator's home ---------------------------------

/// Fetch `named` into `~/.io-cli/marketplaces`.
///
/// The whole of what both doors run for `plugin marketplace add`, so neither can
/// grow a fetch of its own — which is F1's named sabotage.
#[must_use]
pub fn add(named: &Named) -> Outcome {
    match crate::fetch::fetch(named) {
        None => Outcome::homeless(),
        Some(fetched) => told(named, &fetched),
    }
}

/// Delete `named`'s clone from `~/.io-cli/marketplaces`. See [`discard`].
#[must_use]
pub fn remove(named: &Named) -> Outcome {
    match crate::home::marketplaces() {
        None => Outcome::homeless(),
        Some(root) => discard(&root, named),
    }
}

/// Every marketplace in `~/.io-cli/marketplaces`, or `None` with no home.
///
/// `None` and an empty `Vec` are different answers and the surfaces say so
/// differently: nowhere to keep one, against nothing kept yet.
#[must_use]
pub fn installed() -> Option<Vec<Market>> {
    Some(markets(&crate::home::marketplaces()?))
}

// --- rows ---------------------------------------------------------------------

/// The picker rows for the marketplaces, one each, in [`markets`]' order.
///
/// **The contract is positional and index `i` is `markets[i]`**, with no headings
/// and no interleaving — the rule `pluginview::rows` states for the same reason:
/// the picker hands a chosen index straight back into the list, and a row that
/// maps to no marketplace is the off-by-one that shipped a wrong delete in 0.20.0.
///
/// The count is unconditional, because it is the answer to "what is in it" and is
/// the only field here with no other home; the path takes what is left and is
/// shortened from the left, since every clone on one machine shares the first
/// several segments of its path.
pub fn rows(markets: &[Market], width: u16, glyphs: &Glyphs) -> Vec<Row> {
    let separator = glyphs.separator;
    let separator_width = separator.chars().count();
    markets
        .iter()
        .map(|market| {
            let name = market.name();
            let mut detail = market.held();
            // The picker's own arithmetic, mirrored rather than guessed: two cells
            // of marker, the label, two cells of gap.
            let left = (width as usize)
                .saturating_sub(4)
                .saturating_sub(name.chars().count())
                .saturating_sub(detail.chars().count());
            if left > separator_width + glyphs.ellipsis.chars().count() {
                detail.push_str(separator);
                detail.push_str(&fit_left(
                    &market.root.display().to_string(),
                    left - separator_width,
                    glyphs,
                ));
            }
            Row::marked(HERE_MARK, name, detail)
        })
        .collect()
}

/// The picker rows for one marketplace's bundles, one each, in [`holdings`]'
/// order.
///
/// **The label is the manifest's `name`** — see [`Bundle::label`] — and the detail
/// is [`Bundle::line`], so a bundle whose manifest names nothing says so on the row
/// rather than being drawn under a directory name as if that were its id.
///
/// Index `i` is `market.bundles[i]`, with the same no-headings rule [`rows`]
/// states.
pub fn bundle_rows(market: &Market, width: u16, glyphs: &Glyphs) -> Vec<Row> {
    market
        .bundles
        .iter()
        .map(|bundle| {
            let label = bundle.label();
            let room = (width as usize)
                .saturating_sub(4)
                .saturating_sub(label.chars().count());
            Row::with_detail(label, fit(&bundle.line(), room, glyphs))
        })
        .collect()
}
