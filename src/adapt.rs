//! The manifest formats io-cli did not invent.
//!
//! A capability bundle in the field is a Claude Code plugin or a Codex plugin.
//! `plugin.toml` is a format this project writes and nobody else does, so the
//! marketplace surface [`crate::marketplace`] built could be stocked only by a
//! bundle the operator had written themselves — which is the one case that never
//! needed a marketplace. Five marketplaces publishing 304 plugins were surveyed
//! on one machine while this module was written and not one of them carried a
//! `.toml`.
//!
//! **This module is the only place in the crate that parses JSON that arrived
//! from somewhere else**, beside [`crate::import`], which has read the operator's
//! own Claude and Codex files since 0.21.0. `tests/dependencies.rs` names the two
//! by exact path and fails when a third module deserializes JSON, for the reason
//! the TOML rule already gives in its own words: a second reader of a stranger's
//! file is a second opinion about what that file means. Serializing is not that
//! problem and is not gated — `src/exec.rs` writes io-cli's own event lines and
//! reads nobody.
//!
//! **Three formats, and the precedence between them is the point.** A directory
//! carrying a native `plugin.toml` is read by [`crate::marketplace::manifest`]
//! exactly as it was before this module existed; nothing here can win against it.
//! Where a repository carries `.claude-plugin/marketplace.json`, that file is the
//! author's own statement of what the repository publishes and it is the answer —
//! the directory walk does not also run, because a union of the two would list
//! bundles the author did not publish beside the ones they did and give an
//! operator no way to tell which was which. Where a repository carries neither, a
//! `.claude-plugin/plugin.json` or a `.codex-plugin/plugin.json` is read as a
//! manifest wherever [`crate::pluginview::candidates`] would have looked for a
//! `plugin.toml`.
//!
//! **The dotted-directory skip in that walk is preserved and this module does not
//! weaken it.** `.claude-plugin` is read at a known path relative to a directory
//! the walk already visited; the walk itself still never descends into a dot
//! directory, which is what keeps `.git` out of a marketplace listing.
//!
//! **Everything read here is the same trust class as a subprocess's stderr.** A
//! stranger's description may carry raw newlines or control characters, and on
//! this renderer the scrollback is the transcript — a forged line is read as
//! io-cli's own. Every value except a hook's command goes through
//! [`crate::marketplace::plain`] and [`crate::marketplace::bounded`] on the way
//! out of this module rather than at each surface, so a new surface cannot forget.
//! A hook's command is filtered and **not** bounded, because it is argv an
//! operator is being asked to consent to and a shortened argv is the one thing
//! that surface must never show.
//!
//! **An entry this module cannot read is reported, never skipped.** A marketplace
//! index is written by somebody else and a shape nobody anticipated is found by an
//! operator rather than by a fixture; a listing that silently dropped it would
//! make that failure invisible and unreportable.

use std::path::{Path, PathBuf};

use serde::Deserialize;

use crate::marketplace::{bounded, plain};

/// The directory a Claude Code plugin keeps its manifests in.
pub const CLAUDE_DIR: &str = ".claude-plugin";

/// The directory a Codex plugin keeps its manifest in.
///
/// A separate constant rather than a list, because the two are not
/// interchangeable: `zeroonething/caveman` carries `.codex` and no
/// `.codex-plugin`, and `zeroonething/ponytail` carries `.codex-plugin`. A reader
/// that treated any dot directory starting with `.codex` as this one would read
/// the first repository's Cursor and Cline rules as a plugin manifest.
pub const CODEX_DIR: &str = ".codex-plugin";

/// The index file, inside [`CLAUDE_DIR`].
pub const INDEX_FILE: &str = "marketplace.json";

/// The manifest file, inside either [`CLAUDE_DIR`] or [`CODEX_DIR`].
pub const MANIFEST_FILE: &str = "plugin.json";

/// Where a plugin an index names actually lives.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Source {
    /// A path inside the clone. `"./"` is the clone's own root, and
    /// `"./plugins/x"` a directory in it. 53 of the official marketplace's 291
    /// entries are this.
    Local(String),
    /// Another repository, to be fetched at install time. 238 of that index's 291
    /// entries are this.
    Remote(Remote),
}

/// A plugin published in a repository other than the one the index is in.
///
/// **One shape for both of the tags the field uses.** An index spells this
/// `{"source": "url", …}` 153 times and `{"source": "git-subdir", …}` 85 times,
/// and the two carry the same keys — a `url`, an optional `path` into it, an
/// optional `ref`, an optional `sha`. Reading them through one type keyed on the
/// presence of `url` rather than on the tag is deliberate: a second reader for a
/// second name is the second-implementation defect this product has paid for in
/// `servers::edit`, in `providers::edit` and in the guided browser that built a
/// command string by hand.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Remote {
    /// The repository to clone.
    pub url: String,
    /// The directory inside it the bundle is rooted at, where the entry names one.
    pub path: Option<String>,
    /// A tag or branch. Spelled `ref` in the file, which is a Rust keyword.
    pub reference: Option<String>,
    /// A commit to pin to. Every one of the official index's 238 remote entries
    /// carries one; `superpowers-marketplace`'s ten carry none, so it is optional.
    pub sha: Option<String>,
}

/// One plugin an index names.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Entry {
    /// The author's own name for it, and the word an operator types at
    /// `plugin add`. Filtered and bounded.
    pub name: String,
    /// Filtered and bounded, or `None` where the entry carries none.
    pub description: Option<String>,
    /// Displayed and never resolved — io-harness resolves no versions and this
    /// release does not become the layer that does.
    pub version: Option<String>,
    /// Where the plugin is.
    pub source: Source,
}

/// A repository's own statement of what it publishes.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Index {
    /// The entries this module could read, in the order the file lists them.
    pub entries: Vec<Entry>,
    /// One line per entry it could not, naming what was wrong. Reported rather
    /// than dropped: see the module docs.
    pub unreadable: Vec<String>,
}

/// One bundle manifest, in either of the two foreign formats.
///
/// Every field is optional because every field is optional in the wild — a
/// `plugin.json` naming nothing is a file this module reports rather than one it
/// refuses, exactly as [`crate::marketplace::manifest`] treats a `plugin.toml`
/// that names nothing.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Manifest {
    /// Filtered and bounded.
    pub name: Option<String>,
    /// Filtered and bounded.
    pub description: Option<String>,
    /// Filtered and bounded.
    pub version: Option<String>,
    /// A relative path to a skills directory, where the manifest names one. Codex
    /// manifests write `"./skills/"`; Claude manifests generally leave it out and
    /// the directory is found by its conventional name.
    pub skills: Option<String>,
    /// A relative path to a hooks file, where the manifest names one.
    pub hooks: Option<String>,
    /// Which directory it was read from, so a surface can say whether it was the
    /// Claude manifest or the Codex one without guessing.
    pub from: PathBuf,
}

/// One hook a foreign manifest declares.
///
/// Read to be **shown** and never to be translated. io-harness's `Hook.run` is
/// argv and deliberately never a shell string, its `on` takes the harness's own
/// event tags, and 0.71.0 refuses `${env:}`, `${file:}` and `${cmd:}` inside a
/// manifest in every scope. A Claude hook is
/// `"\"${CLAUDE_PLUGIN_ROOT}/hooks/run-hook.cmd\" session-start"` — a shell
/// string, an unknown event and a refused substitution, all three at once. No
/// adapter closes that; an approximated hook is a program running on the
/// operator's machine that nobody described accurately.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Hook {
    /// The event the foreign format names, verbatim.
    pub event: String,
    /// The command it would run, filtered of control characters and **not**
    /// bounded. See the module docs.
    pub command: String,
}

/// The index a clone publishes, or `None` where it publishes none.
#[must_use]
pub fn index_at(clone: &Path) -> Option<Index> {
    let file = clone.join(CLAUDE_DIR).join(INDEX_FILE);
    let text = std::fs::read_to_string(&file).ok()?;
    let wire: WireIndex = serde_json::from_str(&text).ok()?;

    let mut entries = Vec::new();
    let mut unreadable = Vec::new();
    for slot in wire.plugins {
        match slot {
            Slot::Read(entry) => match read_entry(entry) {
                Ok(entry) => entries.push(entry),
                Err(said) => unreadable.push(said),
            },
            Slot::Unread(bad) => unreadable.push(bad.said()),
        }
    }
    Some(Index {
        entries,
        unreadable,
    })
}

/// The foreign manifest in `dir`, Claude's preferred over Codex's.
///
/// **Claude first because it is the format more repositories publish**, not
/// because it says more: the two carry the same keys for everything this module
/// reads. Where a repository publishes both, they agree in every case surveyed,
/// and a rule that picked one is better than a merge that could produce a bundle
/// neither file describes.
#[must_use]
pub fn manifest_at(dir: &Path) -> Option<Manifest> {
    for holder in [CLAUDE_DIR, CODEX_DIR] {
        let file = dir.join(holder).join(MANIFEST_FILE);
        let Ok(text) = std::fs::read_to_string(&file) else {
            continue;
        };
        let Ok(wire) = serde_json::from_str::<WireManifest>(&text) else {
            continue;
        };
        return Some(Manifest {
            name: value(wire.name),
            description: value(wire.description),
            version: value(wire.version),
            skills: value(wire.skills),
            hooks: value(wire.hooks),
            from: file,
        });
    }
    None
}

/// Every hook a hooks file declares, in the order it declares them.
///
/// An empty vector for a file that is not there, does not parse, or declares
/// none: this answers a disclosure surface, and a bundle with no hooks and a
/// bundle whose hooks file is malformed both cross the same amount of nothing.
/// What must not happen is a hook that exists and is not drawn, and every shape
/// below that yields rows yields one row per hook.
#[must_use]
pub fn hooks_in(file: &Path) -> Vec<Hook> {
    let Ok(text) = std::fs::read_to_string(file) else {
        return Vec::new();
    };
    let Ok(wire) = serde_json::from_str::<WireHooks>(&text) else {
        return Vec::new();
    };
    let mut found = Vec::new();
    for (event, matchers) in wire.hooks {
        for matcher in matchers {
            for hook in matcher.hooks {
                let Some(command) = hook.command else {
                    continue;
                };
                found.push(Hook {
                    // Filtered, never bounded. The event is the file's own word
                    // and is bounded, because it is a label rather than argv.
                    event: bounded(&plain(&event)),
                    command: plain(&command),
                });
            }
        }
    }
    found
}

/// A read value, filtered, bounded, and `None` where it is blank.
///
/// The same collapse [`crate::marketplace`]'s own reader makes and for the same
/// reason: a key present and empty names a bundle no better than a key that is
/// absent, and one case is easier to draw than two.
fn value(raw: Option<String>) -> Option<String> {
    let said = plain(raw?.trim());
    let said = said.trim();
    if said.is_empty() {
        return None;
    }
    Some(bounded(said))
}

/// One wire entry turned into an [`Entry`], or the sentence saying why not.
fn read_entry(wire: WireEntry) -> Result<Entry, String> {
    let name = value(Some(wire.name.clone()))
        .ok_or_else(|| format!("an entry named {:?} carries no usable name", wire.name))?;
    let source = match wire.source {
        WireSource::Local(said) => {
            let said = plain(said.trim());
            let said = said.trim();
            if said.is_empty() {
                return Err(format!("{name} names an empty source path"));
            }
            Source::Local(bounded(said))
        }
        WireSource::Remote {
            url,
            path,
            reference,
            sha,
        } => Source::Remote(Remote {
            url: value(Some(url)).ok_or_else(|| format!("{name} names an empty url"))?,
            path: value(path),
            reference: value(reference),
            sha: value(sha),
        }),
    };
    Ok(Entry {
        name,
        description: value(wire.description),
        version: value(wire.version),
        source,
    })
}

/// The index file as it is written.
///
/// `plugins` defaults so a file carrying only `name` and `owner` reads as an index
/// holding nothing rather than as a file that does not parse — which is the honest
/// answer, and the one a repository mid-edit produces.
#[derive(Deserialize)]
struct WireIndex {
    #[serde(default)]
    plugins: Vec<Slot>,
}

/// One element of `plugins`, whether or not this module can read it.
///
/// **Untagged, with a fallback that matches any object**, so one entry in a shape
/// nobody anticipated costs that entry and not the whole index. `serde`'s untagged
/// enum tries the variants in order, so [`WireEntry`] is attempted first and
/// [`Unread`] — every field optional — accepts whatever is left.
#[derive(Deserialize)]
#[serde(untagged)]
enum Slot {
    Read(WireEntry),
    Unread(Unread),
}

/// An entry this module could not read, kept only for the name to report it by.
#[derive(Deserialize)]
struct Unread {
    #[serde(default)]
    name: Option<String>,
}

impl Unread {
    /// The line a listing draws for it.
    fn said(&self) -> String {
        match self.name.as_deref().map(plain) {
            Some(name) if !name.trim().is_empty() => format!(
                "{} names a source io does not read",
                bounded(name.trim()),
            ),
            _ => "an entry with no name names a source io does not read".to_string(),
        }
    }
}

/// One readable entry, as it is written.
#[derive(Deserialize)]
struct WireEntry {
    name: String,
    #[serde(default)]
    description: Option<String>,
    #[serde(default)]
    version: Option<String>,
    source: WireSource,
}

/// The two spellings of `source`.
///
/// The object arm names no `source` tag of its own on purpose: `url` is what
/// decides the arm, and both `"url"` and `"git-subdir"` carry one. Keying on the
/// tag instead would refuse a third name for a shape this already reads correctly.
#[derive(Deserialize)]
#[serde(untagged)]
enum WireSource {
    Local(String),
    Remote {
        url: String,
        #[serde(default)]
        path: Option<String>,
        #[serde(rename = "ref", default)]
        reference: Option<String>,
        #[serde(default)]
        sha: Option<String>,
    },
}

/// A bundle manifest, as it is written, in either foreign format.
///
/// Every field optional and every unknown key ignored. The surveyed manifests
/// carry `author`, `homepage`, `repository`, `license`, `keywords`, `interface`,
/// `lspServers` and more; a reader that refused an unknown key would refuse
/// almost every real file. This is the opposite of the rule for the manifest io
/// **writes**, which io-harness parses under `deny_unknown_fields` — reading
/// somebody else's file forgives, writing one does not.
#[derive(Deserialize)]
struct WireManifest {
    #[serde(default)]
    name: Option<String>,
    #[serde(default)]
    description: Option<String>,
    #[serde(default)]
    version: Option<String>,
    #[serde(default)]
    skills: Option<String>,
    #[serde(default)]
    hooks: Option<String>,
}

/// A hooks file, as it is written.
#[derive(Deserialize)]
struct WireHooks {
    #[serde(default)]
    hooks: std::collections::BTreeMap<String, Vec<WireMatcher>>,
}

/// One matcher group under an event.
#[derive(Deserialize)]
struct WireMatcher {
    #[serde(default)]
    hooks: Vec<WireHook>,
}

/// One hook inside a matcher group.
#[derive(Deserialize)]
struct WireHook {
    #[serde(default)]
    command: Option<String>,
}
