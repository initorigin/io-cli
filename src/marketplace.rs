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
//! That function answers in **source bytes**, quotes included, so the value is
//! unquoted here exactly the way `src/pluginview.rs:727` already unquotes a
//! declared path — one rule for the whole crate rather than two spellings of it.
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
//! written by [`crate::pluginview::add`], the edit `/plugin add` already had.
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
    pub dir: PathBuf,
    /// The manifest's `name`, unquoted, or `None` where it carries none this
    /// module could read.
    pub name: Option<String>,
    /// The manifest's `description`, unquoted, or `None`.
    pub description: Option<String>,
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
        match (&self.name, &self.description) {
            (Some(_), Some(said)) => said.clone(),
            (Some(_), None) => "its plugin.toml carries no description".to_string(),
            (None, Some(said)) => {
                let mut out = String::from("its plugin.toml does not name it; ");
                out.push_str(said);
                out
            }
            (None, None) => {
                "its plugin.toml does not name it, and carries no description".to_string()
            }
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
            0 => format!("no directory in it carries a {MANIFEST}"),
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
    })
}

/// One top-level key of a manifest, unquoted.
///
/// [`crate::edit::value_at`] answers in the value's **source bytes**, so a string
/// arrives with its quotes on. They are trimmed the way `src/pluginview.rs:727`
/// trims a declared path — the same two calls in the same order, so there is one
/// rule in this crate for turning TOML source back into a value rather than two
/// that can drift.
///
/// An empty result is `None` rather than `Some("")`: a key present and blank names
/// a bundle no better than a key that is absent, and collapsing them here means
/// [`Bundle::label`] has one case to answer instead of two.
fn declared(text: &str, key: &str) -> Option<String> {
    let raw = crate::edit::value_at(text, key)?;
    let value = raw.trim().trim_matches('"').trim();
    if value.is_empty() {
        return None;
    }
    Some(value.to_string())
}

/// Every bundle inside a clone, the clone's own root included.
///
/// **The walk is [`crate::pluginview::candidates`]'s**, not a second one written
/// here: it already skips `target`, `node_modules` and every dotted directory —
/// `.git`, which every clone has, most of all — and it already orders by depth and
/// then by path so two calls on one machine answer the same way. A repository
/// laying its bundles out in a way that walk cannot see is a repository `/plugin
/// add` could not see either, and one walk that is sometimes too shallow is better
/// than two that disagree about which.
///
/// The root is checked separately because that function only ever looks at a
/// directory's *children*, and a marketplace that is itself one bundle — a single
/// plugin published as its own repository — is the shape it would otherwise miss
/// entirely.
#[must_use]
pub fn holdings(clone: &Path) -> Vec<Bundle> {
    let mut dirs = Vec::new();
    if clone.join(MANIFEST).is_file() {
        dirs.push(clone.to_path_buf());
    }
    dirs.extend(crate::pluginview::candidates(clone));
    dirs.iter().filter_map(|dir| manifest(dir)).collect()
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
#[must_use]
pub fn warning(inside: &[PathBuf]) -> Option<String> {
    if inside.is_empty() {
        return None;
    }
    let mut said = if inside.len() == 1 {
        String::from("1 declared bundle lives inside it")
    } else {
        format!("{} declared bundles live inside it", inside.len())
    };
    said.push_str(
        ": the `[[plugin]]` entries are left exactly as they are, and io-harness will \
         report them as missing from the next turn — ",
    );
    said.push_str(
        &inside
            .iter()
            .map(|path| path.display().to_string())
            .collect::<Vec<_>>()
            .join(", "),
    );
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
/// thing. The refusal spells both qualified forms, so the fix is a paste rather
/// than a lookup.
pub fn locate(markets: &[Market], query: &str) -> Result<PathBuf, String> {
    let (name, qualifier) = asked(query);
    let hits: Vec<(&Market, &Bundle)> = markets
        .iter()
        .flat_map(|market| market.bundles.iter().map(move |bundle| (market, bundle)))
        .filter(|(market, bundle)| {
            bundle.label() == name
                && qualifier
                    .is_none_or(|which| which == market.name() || which == market.named.repo)
        })
        .collect();
    match hits.as_slice() {
        [(_, bundle)] => Ok(bundle.dir.clone()),
        [] => Err(unheld(markets, query)),
        several => {
            let spellings = several
                .iter()
                .map(|(market, _)| format!("`{name}@{}`", market.name()))
                .collect::<Vec<_>>()
                .join(" and ");
            Err(format!(
                "{} marketplaces here hold a bundle called `{name}`, and installing whichever \
                 was found first would install code you did not choose; say which one: {spellings}",
                several.len()
            ))
        }
    }
}

/// What to say when no marketplace holds the name that was asked for.
///
/// The list of what *is* here is the whole point of the sentence: a bare "not
/// found" over a set of clones an operator fetched weeks ago leaves them running
/// `plugin marketplace list` and then descending into each one. Every bundle is
/// named in the qualified form, which is the form that always resolves.
fn unheld(markets: &[Market], query: &str) -> String {
    let held: Vec<String> = markets
        .iter()
        .flat_map(|market| {
            market
                .bundles
                .iter()
                .map(move |bundle| format!("`{}@{}`", bundle.label(), market.name()))
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
) -> Result<PathBuf, String> {
    match crate::pluginview::refusal(dir) {
        None => Ok(dir.to_path_buf()),
        Some(refused) => {
            locate(&markets(), text).map_err(|missing| format!("{refused} — {missing}"))
        }
    }
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
/// One finished line each, and the first field is the **qualified spelling**, which
/// is exactly what `plugin add` takes: the answer to "what is out there" is then
/// also the thing to paste, with no second lookup to work out which marketplace the
/// hit came from.
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
        .map(|(market, bundle)| format!("{}@{} · {}", bundle.label(), market.name(), bundle.line()))
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
