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
//!
//! # Reading a foreign bundle is half of it; [`generate`] is the other half
//!
//! Everything above answers *what is in this repository*. A bundle that is only
//! listed is a bundle nobody can use: io-harness loads a directory with a
//! `plugin.toml` at its root and nothing else, so a Claude Code plugin an operator
//! found in a marketplace has to acquire one before it can contribute anything.
//! [`generate`] writes it.
//!
//! **The clone is never written to.** [`crate::marketplace`]'s docs state that the
//! stranger's checkout is not touched, and an adapter would be the first thing to
//! touch it — a file io wrote inside somebody's git working tree is a dirty tree
//! at their next `git pull`. The manifest goes to [`crate::home::adapters`]
//! instead, and every path inside it is **absolute** and points back into the
//! clone. That is not a convenience:
//! [`Plugin::skills_dir`](io_harness::Plugin::skills_dir) is `self.root.join(d)`
//! (`io-harness-0.71.0/src/plugin.rs:268`), and `Path::join` of an absolute
//! argument discards the base — so an absolute `skills` is the one spelling that
//! reaches a directory outside the manifest's own root.
//!
//! **This module writes TOML and still parses none.** `src/edit.rs` is the crate's
//! only permitted TOML parser by path (`tests/dependencies.rs`), and that rule is
//! about *meaning* — the spelling of a value goes through `toml`'s own serializer
//! here for the same reason [`crate::edit::array`] argues for it, and anything that
//! reads a manifest back reads it through [`crate::edit::value_at`] like every
//! other file. What [`generate`] hands back about the file it wrote comes from
//! io-harness's own accessors and never from io-cli's account of what it spelled.
//!
//! **Translate what maps; disclose what does not.** Four kinds map — `skills/`,
//! `commands/`, `agents/*.md` and `.mcp.json`. A frontmatter key with no slot in
//! an [`AgentDef`], and a hooks file, are reported on [`Adapter::disclosed`] so a
//! surface can name them. Nothing is silently dropped and nothing is approximated:
//! [`Hook`]'s own docs give the argument for why a hook is shown and never
//! translated, and it applies to every key here.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use io_harness::config::Scope;
use io_harness::{AgentDef, McpServer, McpTransport, Plugins, PLUGIN_FILE};
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

/// An index entry's name as a plugin id io-harness accepts, or `None` where no
/// such id is derivable from it.
///
/// **An id is not a label, and that is why this refuses far more than it maps.**
/// It is the word an operator types at `plugin add`, and io-harness namespaces
/// every name a bundle contributes with it — a bundle whose id is `rust-review`
/// contributes `rust-review__reviewer`. A mapping that dropped or invented
/// characters would hand a person an id they never saw in the index and cannot
/// guess from it: `My.Plugin` quietly installed as `my-plugin` is one name in the
/// author's file, a second in the listing, and a third thing entirely in every
/// namespaced name the bundle goes on to contribute.
///
/// So the line is drawn at what a reader undoes by eye:
///
/// - A name that is already an id is the id, unchanged.
/// - A name that becomes one under an ASCII case fold alone is folded. `Rust` and
///   `rust` are the same word to anyone reading them, and the fold is the one
///   transformation a person performs without being told it happened.
/// - Everything else refuses — a dot, a space, an underscore, a slash, a leading
///   hyphen, a non-ASCII letter, or a name longer than
///   [`io_harness::MAX_ID`]. [`crate::marketplace`] reports it by name.
///
/// Refusing costs the index's author one renamed key. Mangling costs every
/// operator of that marketplace a name they cannot type.
///
/// The rule is restated here rather than called because io-harness's own check is
/// private and answers about a `plugin.toml` that does not exist yet — an index
/// entry has no file to be refused against. [`io_harness::MAX_ID`] is taken from
/// the crate rather than copied, so the one part of the rule that can move is not
/// spelled twice.
#[must_use]
pub fn normalised(name: &str) -> Option<String> {
    // ASCII, not Unicode: a Unicode fold can turn one character into several
    // (`İ` folds to `i` plus a combining dot), which is exactly the invented
    // character this refuses. Byte length is the id's length because an id that
    // passes the character rule is ASCII, and it is what io-harness measures.
    let id = name.to_ascii_lowercase();
    let usable = !id.is_empty()
        && id.len() <= io_harness::MAX_ID
        && id.starts_with(|glyph: char| glyph.is_ascii_lowercase() || glyph.is_ascii_digit())
        && id
            .chars()
            .all(|glyph| glyph.is_ascii_lowercase() || glyph.is_ascii_digit() || glyph == '-');
    usable.then_some(id)
}

// ---------------------------------------------------------------------------
// Writing the manifest io-harness reads
// ---------------------------------------------------------------------------

/// The conventional directory a bundle keeps its skills in.
///
/// A Codex manifest names it (`"./skills/"`) and a Claude one generally does not,
/// so [`Manifest::skills`] is preferred where it is there and this is the answer
/// where it is not — which is every Claude bundle surveyed.
pub const SKILLS_DIR: &str = "skills";

/// The conventional directory a bundle keeps its slash commands in.
///
/// io-harness calls the same thing a **template**, and the two formats are the
/// same file: markdown, optional frontmatter, `$ARGUMENTS` for what the operator
/// typed. `Templates::discover` takes every `*.md` in a directory
/// (`io-harness-0.71.0/src/template.rs:279`), so what is translated is the
/// directory and never the files in it — nothing is copied and nothing is
/// rewritten.
pub const COMMANDS_DIR: &str = "commands";

/// The conventional directory a bundle keeps its agent definitions in.
pub const AGENTS_DIR: &str = "agents";

/// The MCP server file a Claude Code bundle publishes at its root.
pub const MCP_FILE: &str = ".mcp.json";

/// The one substitution a bundle writes that io can honestly expand.
///
/// It means the bundle's own root, which is the directory being adapted — so the
/// answer is already known and nothing of the host is read to produce it. Every
/// other `${...}` is refused by name; see [`expanded`].
pub const PLUGIN_ROOT: &str = "${CLAUDE_PLUGIN_ROOT}";

/// One generated manifest: where it is, what io-harness reads out of it, and what
/// the bundle carried that it does not.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Adapter {
    /// The `plugin.toml` that was written.
    pub manifest: PathBuf,
    /// What the bundle contributes, in io-harness's own words and its own fixed
    /// order.
    ///
    /// [`io_harness::Plugin::contributions`] on the file that was just written —
    /// not io-cli's account of what it spelled. The two can only agree if the
    /// manifest loaded, and it is read back through io-harness precisely so that
    /// a surface reporting "skills, templates, agents, mcp" is reporting a fact
    /// rather than an intention.
    pub contributes: Vec<String>,
    /// One line per thing the bundle carried that a `plugin.toml` has no slot for.
    ///
    /// Reported rather than dropped, for the module docs' reason: a key that
    /// silently does nothing is a capability the bundle's author believes in.
    pub disclosed: Vec<String>,
}

/// The adapter directory for one bundle, under an adapters root.
///
/// Three levels — `<owner>/<repo>/<name>` — which is [`crate::marketplace::at`]'s
/// own two-level layout with the bundle's own name under it, because one clone
/// publishes many bundles and a `plugin.toml` is recognised by sitting at a
/// directory's root.
///
/// The root is an argument rather than a call into [`crate::home::adapters`], for
/// the reason `src/marketplace.rs`'s docs give for the same shape: a decision that
/// lives behind [`crate::home`] cannot be reached by a test without moving the
/// operator's home out from under a suite running in parallel.
#[must_use]
pub fn at(root: &Path, owner: &str, repo: &str, name: &str) -> PathBuf {
    root.join(owner).join(repo).join(name)
}

/// Write the `plugin.toml` that makes `bundle` loadable, and report what it says.
///
/// `bundle` is a directory inside a clone and is **only read**; `into` is where
/// the manifest goes, and it is regenerated rather than edited, so an adapter that
/// is already there is replaced. `name` is the id the manifest declares and the
/// word every name the bundle contributes is namespaced by — it goes through
/// [`normalised`], which refuses far more than it maps and says why.
///
/// # The four kinds, and the one rule that decides the paths
///
/// `skills/` becomes `skills`, `commands/` becomes `templates`, each
/// `agents/*.md` becomes an `[[agent]]`, and each server in `.mcp.json` becomes an
/// `[[mcp]]`. Every path written is **absolute** and points into the clone:
/// `Plugin::skills_dir` is `self.root.join(d)`
/// (`io-harness-0.71.0/src/plugin.rs:268`) and `Path::join` of an absolute
/// argument discards the base, so an absolute path is what lets a manifest in
/// io's home name a directory in somebody else's checkout.
///
/// # Validated through io-harness, on the bytes that were written
///
/// [`io_harness::Plugins::inspect`] is the loader
/// [`Config::plugins`](io_harness::Config::plugins) runs, reached without a
/// `[[plugin]]` entry — the id grammar, the trust rule, the narrowing rule and the
/// substitution refusal, all of them, on the file that is on disk. A manifest it
/// refuses is **removed before this returns**, so nothing an operator could go on
/// to declare is left behind: the whole point of adapting a bundle is that
/// declaring it is then safe.
///
/// [`Scope::User`] because that is where an adapter is declared from. A bundle
/// that contributes an `[[mcp]]` may not be named by a project-scoped `io.toml`
/// at all, and validating under a scope the adapter will never be declared from
/// would refuse manifests that work.
///
/// # Errors
///
/// A string naming what stopped it, for every case: an unusable id, a bundle that
/// cannot be read, a `${...}` that cannot honestly be expanded, an MCP transport
/// io-harness does not speak, and a manifest io-harness itself refuses.
pub fn generate(bundle: &Path, name: &str, into: &Path) -> Result<Adapter, String> {
    let id = normalised(name).ok_or_else(|| {
        format!(
            "{name:?} is not a usable plugin id, so no adapter is written. An id is 1 to {} \
             characters of `a-z`, `0-9` and `-`, starting with a letter or a digit — it is the \
             word every name this bundle contributes is namespaced by, and io-cli refuses to \
             invent one, because a bundle installed under a name nobody saw in the index cannot \
             be typed by the operator who found it.",
            io_harness::MAX_ID,
        )
    })?;

    // Canonical, because the manifest is written elsewhere and every path in it
    // has to still mean this directory when io-harness joins it. A relative
    // `bundle`, or one reached through a symlinked temporary directory, would
    // otherwise be resolved against whatever the process's working directory
    // happened to be at load time rather than at generation time.
    let root = std::fs::canonicalize(bundle).map_err(|e| {
        format!(
            "{}: {e}. A bundle is a directory in a clone that is already on disk.",
            bundle.display()
        )
    })?;

    let mut lines = vec![format!("name = {}", spelled(&id))];
    let mut disclosed: Vec<String> = Vec::new();

    let declared = manifest_at(&root);
    if let Some(read) = &declared {
        if let Some(said) = &read.description {
            lines.push(format!(
                "description = {}",
                spelled(&expanded(said, &root, "description")?)
            ));
        }
        if let Some(version) = &read.version {
            lines.push(format!(
                "version = {}",
                spelled(&expanded(version, &root, "version")?)
            ));
        }
        if let Some(hooks) = &read.hooks {
            // The one kind that is stated and never translated. `Hook`'s own docs
            // carry the argument: a Claude hook is a shell string, an event
            // io-harness does not have, and a refused substitution, all at once,
            // and an approximated hook is a program running on the operator's
            // machine that nobody described accurately.
            disclosed.push(format!(
                "the bundle declares hooks in `{hooks}` — a hook names a program this machine \
                 would run, and io shows one rather than translating it",
            ));
        }
    }

    let skills = directory(
        &root,
        declared.as_ref().and_then(|r| r.skills.as_deref()),
        SKILLS_DIR,
    );
    if let Some(dir) = &skills {
        lines.push(format!("skills = {}", spelled(&shown(dir))));
    }
    if let Some(dir) = directory(&root, None, COMMANDS_DIR) {
        lines.push(format!("templates = {}", spelled(&shown(&dir))));
    }

    let (agents, said) = agents_in(&root.join(AGENTS_DIR), &root)?;
    disclosed.extend(said);
    for agent in &agents {
        lines.push(format!("\n[[agent]]\n{}", item(agent, "an agent")?));
    }

    for server in servers_in(&root.join(MCP_FILE), &root)? {
        lines.push(format!("\n[[mcp]]\n{}", item(&server, "an MCP server")?));
    }

    let text = format!("{}\n", lines.join("\n"));

    // **The backstop, and it is not decoration.** Every value above went through
    // `expanded`, which refuses a substitution by the field it was written in —
    // this catches the case that check has not been wired to yet. io-harness
    // refuses a `${` anywhere in a manifest, in every scope
    // (`io-harness-0.71.0/src/plugin.rs:774`), and the refusal takes the whole
    // bundle rather than the one key: a single missed value would cost the
    // operator every capability the bundle carries, reported against a file they
    // did not write. Nothing is written when this fires.
    if let Some(found) = substitution(&text) {
        return Err(format!(
            "the manifest io would write for `{id}` still carries `{found}`, and io-harness \
             refuses a substitution inside a plugin.toml in every scope — the refusal takes the \
             whole bundle, not the one key. Nothing was written.",
        ));
    }

    crate::home::create(into).map_err(|e| format!("{}: {e}", into.display()))?;
    let manifest = into.join(PLUGIN_FILE);
    std::fs::write(&manifest, text.as_bytes())
        .map_err(|e| format!("{}: {e}", manifest.display()))?;

    let plugin = match Plugins::inspect(Scope::User, into) {
        Ok(plugin) => plugin,
        Err(refused) => {
            // Removed rather than left for an operator to find: a manifest
            // io-harness refuses is one that would be reported on
            // `Plugins::dropped` against a path they never chose, and a directory
            // holding a broken adapter reads exactly like one holding a working
            // adapter until something declares it.
            let _ = std::fs::remove_file(&manifest);
            return Err(refused.to_string());
        }
    };

    Ok(Adapter {
        manifest,
        contributes: plugin
            .contributions()
            .into_iter()
            .map(str::to_string)
            .collect(),
        disclosed,
    })
}

/// The directory `named` points at inside `root`, or the conventional one, or
/// `None` where neither is a directory.
///
/// Canonical for [`generate`]'s reason, and it also tidies the spelling: a Codex
/// manifest writes `"./skills/"`, and `root.join("./skills/")` is a path that
/// works and reads as though io-cli did not know where it was pointing. The
/// uncanonical path where that call fails, rather than `None`: `root` is already
/// canonical and the directory was just seen, so a failure here is a race, and
/// dropping the key would answer it by leaving a capability out of the manifest
/// with nobody told.
fn directory(root: &Path, named: Option<&str>, conventional: &str) -> Option<PathBuf> {
    for candidate in named.into_iter().chain(std::iter::once(conventional)) {
        let at = root.join(candidate);
        if at.is_dir() {
            return Some(std::fs::canonicalize(&at).unwrap_or(at));
        }
    }
    None
}

/// One path, as a manifest spells it.
///
/// `to_string_lossy` rather than a refusal on a non-UTF-8 path: the value is
/// about to go through [`spelled`], which escapes whatever it is handed, and a
/// bundle unreachable because one byte of its path is not UTF-8 is a worse answer
/// than one whose path was written with the replacement character in it — which
/// io-harness will then report by name when the directory is not there.
fn shown(path: &Path) -> String {
    path.to_string_lossy().into_owned()
}

/// One string value, spelled the way TOML spells it.
///
/// **The escaping goes through `toml`'s own serializer and never through a format
/// string**, which is [`crate::edit::array`]'s argument at the one other place
/// this crate spells a value: an absolute Windows path is full of backslashes and
/// `\U` opens an escape in a TOML basic string, so `format!("\"{value}\"")` is
/// either a parse error or a *different* path — and a `skills` key that parses to
/// a directory nobody named is the quietest failure this crate can ship.
///
/// `format!("{value:?}")` is not adequate either, and it is the near miss worth
/// naming because it looks as though it would be. `Debug` for `str` escapes for
/// Rust's grammar rather than TOML's: it writes an escaped apostrophe, which TOML
/// rejects outright, and it spells a control character in Rust's own brace form
/// where TOML wants a `u` and four fixed hex digits with no braces at all. Either
/// produces a manifest io-harness refuses, out of a `description` field a
/// stranger filled in.
fn spelled(value: &str) -> String {
    toml::Value::String(value.to_string()).to_string()
}

/// The `key = value` lines of one array-of-tables entry.
///
/// **The bytes come out of io-harness's own type through serde**, which is
/// `src/import.rs`'s argument for the same call and it holds harder here:
/// [`McpServer`] is `#[serde(flatten)]` over a `#[serde(tag = "transport")]` enum,
/// so the discriminant sits flat beside `id` and a hand-written body that forgot
/// it loads with `missing field transport`. Serializing the value io-harness
/// itself deserializes is what makes "what was written" and "what will be read"
/// one question.
///
/// Each value is rendered in `toml::Value`'s own inline form, which is why an
/// `env` or a `headers` map comes out as an inline table: an `[[mcp]]` block
/// cannot carry an `[mcp.env]` header after it without that header attaching to
/// the wrong entry.
fn item(value: &impl serde::Serialize, what: &str) -> Result<String, String> {
    let value =
        toml::Value::try_from(value).map_err(|e| format!("{what} will not serialise: {e}"))?;
    let table = value
        .as_table()
        .ok_or_else(|| format!("{what} is not a table"))?;
    Ok(table
        .iter()
        .map(|(key, value)| format!("{key} = {value}"))
        .collect::<Vec<_>>()
        .join("\n"))
}

/// The first `${...}` in `text`, as it is written, or `None`.
fn substitution(text: &str) -> Option<String> {
    let at = text.find("${")?;
    let rest = &text[at..];
    Some(rest.find('}').map_or(rest, |end| &rest[..=end]).to_string())
}

/// `raw` with [`PLUGIN_ROOT`] expanded, or a refusal naming `field`.
///
/// **One substitution is expandable and every other is a refusal.**
/// `${CLAUDE_PLUGIN_ROOT}` means the bundle's root, which is the directory being
/// adapted, so expanding it reads nothing of the host — it writes down a path io
/// already knew. `${env:…}`, `${file:…}`, `${cmd:…}` and every shell variable a
/// bundle writes are the opposite: io-harness refuses each of them inside a
/// manifest in every scope (`io-harness-0.71.0/src/plugin.rs:774`), because a
/// bundle is a third party's directory even when the file naming it is the
/// operator's own.
///
/// A passthrough is not an option and neither is dropping the value. The refusal
/// io-harness makes takes the **whole bundle** — one unexpanded `${` costs the
/// operator every skill, template, agent and server the bundle carries, reported
/// against a file they never wrote. Refusing here costs them the same bundle and
/// names the field, which is the difference between something they can act on and
/// something they cannot.
fn expanded(raw: &str, root: &Path, field: &str) -> Result<String, String> {
    let said = raw.replace(PLUGIN_ROOT, &shown(root));
    match substitution(&said) {
        None => Ok(said),
        Some(found) => Err(format!(
            "{field}: `{found}` is a substitution io cannot expand. `{PLUGIN_ROOT}` means the \
             bundle's own root and is written out; anything else would have io-harness read this \
             machine's environment or its files, or run a program on it, for a directory nobody \
             has agreed to — so it refuses one inside a plugin.toml in every scope, and the \
             refusal takes the whole bundle. Write the value out in the bundle, or declare this \
             server in your own io.toml where a substitution is resolved.",
        )),
    }
}

/// Every agent `dir` defines, and one line for every frontmatter key with no slot
/// in an [`AgentDef`].
///
/// **What maps is translated and what does not is disclosed.** io-harness's
/// definition carries a name, a role, a model, a step cap, an effort and three
/// narrowing switches; a Claude agent's frontmatter carries a name, a
/// description, a tool list and a colour. One key means the same thing in both —
/// `name`, the word a `spawn_agent` asks for — and the file's **body** is the
/// role, because the markdown after the closing fence already *is* the child's
/// system prompt, which is what `AgentDef::role` is prepended to.
///
/// Everything else is reported by name rather than approximated, and `model` is
/// the one worth saying out loud: a Claude agent writes `sonnet`, `opus` or
/// `inherit`, which are one vendor's family aliases, while `AgentDef::model` is a
/// model id a provider published. Writing one into the other invents a fact and
/// the run pays for it — a call asking a provider for a model it has never heard
/// of — so it is disclosed, and the definition asks for whatever the run's
/// provider was built with.
///
/// A directory that is not there yields nothing and is not an error: a bundle
/// with no agents and a bundle whose `agents/` is missing contribute the same
/// nothing, and the manifest simply carries no `[[agent]]`.
fn agents_in(dir: &Path, root: &Path) -> Result<(Vec<AgentDef>, Vec<String>), String> {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return Ok((Vec::new(), Vec::new()));
    };
    // Sorted, so a manifest generated twice from one bundle is byte for byte the
    // same file: `read_dir` answers in whatever order the filesystem holds, and a
    // regenerated adapter that differs from the one before it is a diff an
    // operator has to read to find out nothing changed.
    let mut files: Vec<PathBuf> = entries
        .flatten()
        .map(|entry| entry.path())
        .filter(|path| {
            path.extension()
                .is_some_and(|ext| ext.eq_ignore_ascii_case("md"))
        })
        .collect();
    files.sort();

    let mut defs = Vec::new();
    let mut disclosed = Vec::new();
    for file in files {
        let called = file.file_name().map_or_else(
            || file.display().to_string(),
            |name| name.to_string_lossy().into_owned(),
        );
        let Ok(text) = std::fs::read_to_string(&file) else {
            disclosed.push(format!(
                "{called} could not be read and contributes no agent"
            ));
            continue;
        };
        let Some((keys, body)) = front_matter(&text) else {
            disclosed.push(format!(
                "{called} opens with no `---` frontmatter, so it names no agent and contributes \
                 none",
            ));
            continue;
        };

        // The file's own stem where the frontmatter names nothing. An agent
        // definition with no name cannot be spawned, and the filename is already
        // what a reader of the bundle calls it.
        let name = keys
            .iter()
            .find(|(key, _)| key == "name")
            .and_then(|(_, said)| value(Some(said.clone())))
            .or_else(|| {
                file.file_stem()
                    .map(|stem| stem.to_string_lossy().into_owned())
            })
            .filter(|name| !name.is_empty());
        let Some(name) = name else {
            disclosed.push(format!(
                "{called} yields no agent name and contributes no agent"
            ));
            continue;
        };

        let mut def = AgentDef::new(name);
        // **Not filtered through `plain`, and that is the one place this differs
        // from every other value out of a stranger's bundle.** `plain` turns a
        // control character into a space because those values are drawn on a
        // terminal where the scrollback is the transcript; a role is never drawn,
        // it is prepended to a child's system prompt, and flattening its newlines
        // would rewrite the prompt the bundle's author wrote.
        let role = body.trim();
        if !role.is_empty() {
            def = def.with_role(expanded(role, root, &called)?);
        }
        defs.push(def);

        for (key, _) in keys.iter().filter(|(key, _)| key != "name") {
            disclosed.push(format!(
                "{called}: `{key}` has no slot in an io-harness agent definition and is not \
                 translated",
            ));
        }
    }
    Ok((defs, disclosed))
}

/// The `---` frontmatter block's keys in file order, and the body after it.
///
/// **Deliberately not a YAML parser, and this crate does not gain one.** The
/// dependency set is ten names asserted in both directions by
/// `tests/dependencies.rs`, and an eleventh to read one key out of a fenced block
/// would be a parser for a format io-cli neither writes nor validates.
///
/// What it reads: a file opening with `---` alone on its first line, up to the
/// next `---` alone on a line; a `key: value` at column zero where the key is
/// `A-Za-z0-9_-`; and a line indented under one of those, folded onto the value
/// before it with a single space — which is what a `>` or `|` block scalar looks
/// like, and how every surveyed `description` is written.
///
/// **What it does not read, stated rather than implied:** nested mappings,
/// sequences (`- item`), flow collections (`[a, b]`), anchors, aliases, tags,
/// comments, multiple documents, and YAML's own quoting and escapes beyond one
/// pair of matching surrounding quotes.
///
/// A value this reads wrongly cannot become a capability the bundle did not
/// declare, and that is what makes the shortcut safe rather than merely small:
/// the only key acted on is `name`, and every other key is reported by name and
/// translated into nothing at all.
fn front_matter(text: &str) -> Option<(Vec<(String, String)>, &str)> {
    let rest = text
        .strip_prefix("---\n")
        .or_else(|| text.strip_prefix("---\r\n"))?;
    let (block, body) = fenced(rest)?;

    let mut keys: Vec<(String, String)> = Vec::new();
    for line in block.lines() {
        if line.trim().is_empty() {
            continue;
        }
        if line.starts_with(' ') || line.starts_with('\t') {
            if let Some((_, said)) = keys.last_mut() {
                if !said.is_empty() {
                    said.push(' ');
                }
                said.push_str(line.trim());
            }
            continue;
        }
        let Some((key, said)) = line.split_once(':') else {
            continue;
        };
        let key = key.trim();
        if key.is_empty()
            || !key
                .bytes()
                .all(|byte| byte.is_ascii_alphanumeric() || byte == b'_' || byte == b'-')
        {
            continue;
        }
        // `>` and `|` open a block scalar whose text is on the indented lines
        // below, which the branch above folds onto this value; the chomping
        // indicators `-` and `+` may follow either of them.
        let said = said.trim().trim_start_matches(['>', '|', '-', '+']).trim();
        keys.push((key.to_string(), unquoted(said).to_string()));
    }
    Some((keys, body))
}

/// The text before the next `---` alone on a line, and the text after that line.
fn fenced(text: &str) -> Option<(&str, &str)> {
    let mut at = 0usize;
    for line in text.split_inclusive('\n') {
        if line.trim_end() == "---" {
            return Some((&text[..at], &text[at + line.len()..]));
        }
        at += line.len();
    }
    None
}

/// One pair of matching surrounding quotes removed, and nothing else.
fn unquoted(said: &str) -> &str {
    for quote in ['"', '\''] {
        if said.len() >= 2 && said.starts_with(quote) && said.ends_with(quote) {
            return &said[1..said.len() - 1];
        }
    }
    said
}

/// Every MCP server a `.mcp.json` declares, as io-harness's own type.
///
/// **A transport io-harness does not speak is a refusal naming the server**, not
/// a server quietly left out. io-harness speaks stdio and streamable HTTP
/// (`io-harness-0.71.0/src/mcp.rs:289`), so a bundle declaring `sse` is declaring
/// a capability no adapter can deliver — and a manifest carrying one fewer server
/// than the bundle does is exactly the silent absence this module's docs exist to
/// end. The whole generation stops, because a bundle is installed for what it
/// contributes and half of it is not what the operator was shown.
///
/// A file that is not there yields nothing and is not an error, for
/// [`agents_in`]'s reason. A file that is there and is not JSON **is** an error:
/// the bundle says it publishes servers, and answering "none" would be io-cli
/// deciding a malformed file means the same as an absent one.
fn servers_in(file: &Path, root: &Path) -> Result<Vec<McpServer>, String> {
    let Ok(text) = std::fs::read_to_string(file) else {
        return Ok(Vec::new());
    };
    let read: WireServers = serde_json::from_str(&text).map_err(|e| {
        format!(
            "{}: {e}. The bundle declares MCP servers in a file io cannot read, so nothing was \
             written.",
            file.display()
        )
    })?;

    let mut out = Vec::new();
    for (id, wire) in read.servers {
        let field = |key: &str| format!("`{MCP_FILE}` server `{id}`, key `{key}`");
        // A `type` the file leaves out is stdio, which is what every local server
        // relies on: the key is written only for the two remote transports. A
        // file that leaves it out and names a `url` means the remote one, and
        // reading that as stdio would refuse it for having no `command`.
        let kind = match (wire.kind.as_deref(), wire.url.is_some()) {
            (Some(said), _) => said.to_string(),
            (None, true) => "http".to_string(),
            (None, false) => "stdio".to_string(),
        };
        let server = match kind.as_str() {
            "stdio" => {
                let command = wire.command.ok_or_else(|| {
                    format!(
                        "`{MCP_FILE}` server `{id}` is a stdio server naming no `command`, so \
                         there is no program for io-harness to start and nothing was written. \
                         Give it a `command`, or a `type` and a `url` for a remote server."
                    )
                })?;
                let args = wire
                    .args
                    .iter()
                    .map(|arg| expanded(arg, root, &field("args")))
                    .collect::<Result<Vec<String>, String>>()?;
                let mut built =
                    McpServer::stdio(id.as_str(), expanded(&command, root, &field("command"))?)
                        .with_args(args);
                if let McpTransport::Stdio { env, .. } = &mut built.transport {
                    for (name, said) in &wire.env {
                        env.insert(
                            name.clone(),
                            expanded(said, root, &field(&format!("env.{name}")))?,
                        );
                    }
                }
                built
            }
            "http" | "streamable-http" | "streamableHttp" => {
                let url = wire.url.ok_or_else(|| {
                    format!(
                        "`{MCP_FILE}` server `{id}` is an HTTP server naming no `url`, so there \
                         is nowhere for io-harness to dial and nothing was written."
                    )
                })?;
                let mut built = McpServer::http(id.as_str(), expanded(&url, root, &field("url"))?);
                if let McpTransport::Http { headers, .. } = &mut built.transport {
                    for (name, said) in &wire.headers {
                        headers.insert(
                            name.clone(),
                            expanded(said, root, &field(&format!("headers.{name}")))?,
                        );
                    }
                }
                built
            }
            other => {
                return Err(format!(
                    "`{MCP_FILE}` server `{id}` asks for the `{other}` transport, and io-harness \
                     speaks stdio and streamable HTTP and nothing else — so no `[[mcp]]` entry \
                     can be written for it and nothing was written at all. A bundle installed \
                     with one server missing is a capability the operator was shown and did not \
                     get. Ask the bundle's author for a streamable-HTTP endpoint, or declare the \
                     server yourself in your own io.toml."
                ));
            }
        };
        out.push(server);
    }
    Ok(out)
}

/// A `.mcp.json`, as it is written.
#[derive(Deserialize)]
struct WireServers {
    /// Sorted by the map, so the generated manifest lists servers in one order
    /// whatever order the file wrote them in — see [`agents_in`] for why that
    /// matters.
    #[serde(rename = "mcpServers", default)]
    servers: BTreeMap<String, WireServer>,
}

/// One server in it, every field optional and every unknown key ignored.
///
/// [`WireManifest`]'s rule on the other file, for its reason: reading somebody
/// else's file forgives, and the surveyed files carry `disabled`, `description`
/// and vendor keys this translates nothing from.
#[derive(Deserialize)]
struct WireServer {
    #[serde(rename = "type", default)]
    kind: Option<String>,
    #[serde(default)]
    command: Option<String>,
    #[serde(default)]
    args: Vec<String>,
    #[serde(default)]
    env: BTreeMap<String, String>,
    #[serde(default)]
    url: Option<String>,
    #[serde(default)]
    headers: BTreeMap<String, String>,
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
            Some(name) if !name.trim().is_empty() => {
                format!("{} names a source io does not read", bounded(name.trim()),)
            }
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
