//! What the operator already told another agent tool, brought across once.
//!
//! An operator arriving at io has usually been somewhere else first, and that
//! somewhere holds four things they would otherwise retype: standing
//! instructions, MCP servers, skills, and a model id. This module finds them,
//! translates the three that have an honest translation, and writes only what it
//! was handed back.
//!
//! # The promise, and it is the only reason to trust this module at all
//!
//! **It never opens a credential file and never reads a credential value.** That
//! is not a convention kept by care; it is kept by the types. `~/.codex/auth.json`
//! is not in [`files`], so no code path here can reach it. And a server's `env`
//! map is deserialised as [`IgnoredAny`] values — serde walks each value and
//! throws it away without ever building a `String` from it — so the only thing
//! this process ever holds is the KEY. What gets written is `${env:NAME}`, the
//! name pointing at itself, which is what io-harness resolves from the operator's
//! own environment at the moment a run needs it.
//!
//! The same shape covers everything else in those files. `~/.claude.json` is a
//! whole application's state — OAuth material included — and it is read through
//! narrow structs rather than a `serde_json::Value`, so every field this module
//! does not name is skipped by the parser instead of being materialised and then
//! politely ignored.
//!
//! # Two things this module refuses to translate
//!
//! **An allowlist is read, described, and translated into nothing.** Codex's
//! `~/.codex/rules/default.rules` says `prefix_rule(pattern=["bun","install"],
//! decision="allow")` and Claude's `permissions.ask` says `Bash(cargo yank *)` —
//! both of which match a *command line*. io-harness's `Act::Exec` matches a
//! **binary name and nothing else**; it has no argument matching at all
//! (`io-harness-0.69.0/src/policy.rs:62`). So the nearest thing to a faithful
//! import of `bun install` is a blanket allow on `bun`, which is a wider
//! permission than the operator ever granted, written by a tool they were
//! trusting to be careful. A boundary half-imported is worse than one left alone,
//! so [`Kind::Allowlist`] produces a sentence and never a `Rule`, never a
//! `[policy]` table, and never a `[[policy.layers]]` entry.
//!
//! **A model id is carried, not written.** `[[provider]]` needs a vendor and a
//! foreign tool's model string does not name one — `gpt-5` could be OpenAI or any
//! of the twenty-one presets in [`crate::providers::PRESETS`] pointed at a
//! compatible endpoint. So the plan records the id and [`Item::provider_edit`]
//! hands the caller the [`crate::edit::Edit`] once a vendor has been chosen,
//! built by [`crate::providers::add`] rather than by a second speller here.
//!
//! # The ceiling is the feature
//!
//! `Skills::discover` rejects a directory holding more than
//! [`io_harness::skills::MAX_SKILLS`] — **the whole set, not the excess** — and
//! `TaskContract::discover_skills` propagates that with `?` before the first
//! completion. So an operator sitting at 63 skills who imports three more does
//! not get 64 skills and a warning; they get a dead session, on their next turn,
//! with no visible cause. Counting before writing is therefore the point of the
//! skills half rather than a nicety, and it is why an import that would go over
//! writes **nothing** and says so: an import is a set the operator accepted as a
//! set, and two thirds of one is a state they cannot reason about.
//!
//! # Why this module parses no TOML
//!
//! `tests/dependencies.rs` permits `toml::from_str` in `src/edit.rs` alone,
//! because a second module that parses a configuration file is a second opinion
//! about what one means. Codex's `config.toml` is read through
//! [`crate::edit::value_at`], which quotes a named value's own bytes and decides
//! nothing, and [`crate::edit::sections`], which lists header paths. The one
//! thing that machinery cannot do is enumerate the KEYS of a table — and that is
//! the one place a line scan is used, for `[mcp_servers.<name>.env]`, where key
//! names are all this module is allowed to want anyway.
//!
//! # No driver in here
//!
//! No ratatui, no `App`, no keyboard. [`detect`] reports what is on the machine,
//! [`plan`] builds a `Vec<Item>` **whole, before anything is written**, and
//! [`apply`] writes the items it is handed. The overlay that draws the plan and
//! collects an operator's yes is a different file's job, and this one is testable
//! without a terminal because of it.

use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};

use io_harness::config::Scope;
use io_harness::{McpServer, McpTransport};
use serde::de::IgnoredAny;

/// Claude's directory under the operator's home.
const CLAUDE_DIR: &str = ".claude";

/// Claude's state file, which is **not** under [`CLAUDE_DIR`] and is where the
/// MCP servers actually live — under `projects["<absolute path>"].mcpServers`
/// and a top-level `mcpServers`. Looking for them beside `settings.json`, which
/// is where they read as though they ought to be, finds nothing at all.
const CLAUDE_STATE: &str = ".claude.json";

/// Codex's directory. `auth.json` sits in it and is deliberately not named
/// anywhere in this file.
const CODEX_DIR: &str = ".codex";

/// Gemini's directory.
const GEMINI_DIR: &str = ".gemini";

/// The file that makes a directory a skill, for every tool involved and for
/// io-harness too.
const SKILL_FILE: &str = "SKILL.md";

/// How far the skill walk descends.
///
/// Claude's plugins nest five and six levels deep, so a shallow walk finds
/// nothing. The cap is here because the walk follows directory symlinks — which
/// it must, since `~/.claude/skills/*` entries are usually links to somewhere
/// else — and a link pointing at an ancestor is an infinite descent.
///
// ponytail: a depth cap rather than a visited-inode set. Eight is comfortably
// past the deepest real layout and the failure mode is "a skill nobody found"
// rather than a hang. Swap in a `BTreeSet<(dev, ino)>` if a real tree ever
// needs more than eight.
const MAX_DEPTH: usize = 8;

// ---------------------------------------------------------------------------
// A. What is on the machine
// ---------------------------------------------------------------------------

/// A tool io-cli knows how to read.
///
/// Two of these are repository files rather than tools, and they are here rather
/// than folded into one "workspace" variant because an operator recognises the
/// name of the thing they wrote: `.cursorrules` and `CONVENTIONS.md` are
/// different documents with different audiences, and a row saying "workspace"
/// would make them guess which one io-cli found.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum Source {
    /// Claude Code: `~/.claude` plus `~/.claude.json`.
    Claude,
    /// Codex: `~/.codex`.
    Codex,
    /// Gemini: `~/.gemini`.
    Gemini,
    /// `.cursorrules` at the workspace root.
    Cursor,
    /// `CONVENTIONS.md` at the workspace root.
    Conventions,
}

impl Source {
    /// Every source, in the order a surface lists them.
    pub const ALL: [Source; 5] = [
        Source::Claude,
        Source::Codex,
        Source::Gemini,
        Source::Cursor,
        Source::Conventions,
    ];

    /// What this source is called, to an operator.
    #[must_use]
    pub fn word(self) -> &'static str {
        match self {
            Source::Claude => "Claude",
            Source::Codex => "Codex",
            Source::Gemini => "Gemini",
            Source::Cursor => ".cursorrules",
            Source::Conventions => "CONVENTIONS.md",
        }
    }
}

/// A source that is on this machine, and every file this module would read of it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Found {
    /// Which tool.
    pub source: Source,
    /// The files, in the order [`files`] names them. Never empty — a source with
    /// no files is not a [`Found`] at all.
    pub paths: Vec<PathBuf>,
    /// What they hold, in bytes, added up.
    pub bytes: u64,
}

impl Found {
    /// **The files are there and there is nothing in them.**
    ///
    /// A distinct state from absent, and the common one: on the machine this
    /// module was written against, all three of Gemini's files exist and every
    /// one is zero bytes. Collapsing the two would offer an operator an import of
    /// nothing and then report success, which is the failure they cannot see from
    /// a surface.
    #[must_use]
    pub fn empty(&self) -> bool {
        self.bytes == 0
    }

    /// One line naming what was found.
    #[must_use]
    pub fn says(&self) -> String {
        if self.empty() {
            return format!(
                "{}: {} file(s), all empty — nothing to bring across",
                self.source.word(),
                self.paths.len()
            );
        }
        format!(
            "{}: {} file(s), {} bytes",
            self.source.word(),
            self.paths.len(),
            self.bytes
        )
    }
}

/// Every file this module would read from `source`, that is actually there.
///
/// **The roots are parameters and `$HOME` is never read here.** [`crate::home`]
/// resolves the operator's home for the driver; this module takes what it is
/// given, so a fixture directory drives it exactly as the real machine does and
/// a test can never touch the operator's own tools.
///
/// `~/.codex/auth.json` is not in this list and must never be added to it. See
/// the module note.
#[must_use]
pub fn files(source: Source, home_root: &Path, workspace: &Path) -> Vec<PathBuf> {
    let mut out = Vec::new();
    match source {
        Source::Claude => {
            let root = home_root.join(CLAUDE_DIR);
            push_file(&mut out, root.join("CLAUDE.md"));
            push_file(&mut out, root.join("settings.json"));
            push_file(&mut out, home_root.join(CLAUDE_STATE));
            skill_files(&root.join("skills"), 0, &mut out);
            skill_files(&root.join("plugins"), 0, &mut out);
        }
        Source::Codex => {
            let root = home_root.join(CODEX_DIR);
            push_file(&mut out, root.join("AGENTS.md"));
            push_file(&mut out, root.join("memories").join("MEMORY.md"));
            push_file(&mut out, root.join("config.toml"));
            push_file(&mut out, root.join("rules").join("default.rules"));
        }
        Source::Gemini => {
            let root = home_root.join(GEMINI_DIR);
            push_file(&mut out, root.join("GEMINI.md"));
            push_file(&mut out, root.join("antigravity").join("mcp_config.json"));
        }
        Source::Cursor => push_file(&mut out, workspace.join(".cursorrules")),
        Source::Conventions => push_file(&mut out, workspace.join("CONVENTIONS.md")),
    }
    out
}

fn push_file(out: &mut Vec<PathBuf>, path: PathBuf) {
    if path.is_file() {
        out.push(path);
    }
}

/// Every skill file under `root`, flattened.
///
/// Two rules, and the second is the one that keeps a plugin tree from becoming a
/// hundred skills:
///
/// 1. A directory holding a `SKILL.md` **is** one skill, and the walk does not
///    descend past it. Whatever else is in there is that skill's own material.
/// 2. A loose `*.md` counts only at the immediate top of a skills root, which is
///    where the tools that use that spelling put them. Deeper down, a `.md` file
///    is a README or a reference and importing it would offer the model a
///    document that was never a skill.
///
/// `is_dir` and `is_file` both follow symlinks, which is required rather than
/// incidental: `~/.claude/skills/<name>` is usually a link to a checkout
/// somewhere else, and a walk that used `symlink_metadata` would find an empty
/// directory.
fn skill_files(root: &Path, depth: usize, out: &mut Vec<PathBuf>) {
    if depth > MAX_DEPTH {
        return;
    }
    let manifest = root.join(SKILL_FILE);
    if manifest.is_file() {
        out.push(manifest);
        return;
    }
    let Ok(entries) = std::fs::read_dir(root) else {
        return;
    };
    // Sorted, because `read_dir` order is the filesystem's and a plan that came
    // out in a different order on two machines would be a plan nobody could
    // review against a previous run.
    let mut found: Vec<PathBuf> = entries.filter_map(Result::ok).map(|e| e.path()).collect();
    found.sort();
    for path in found {
        if path.is_dir() {
            skill_files(&path, depth + 1, out);
        } else if depth == 0 && is_markdown(&path) {
            out.push(path);
        }
    }
}

fn is_markdown(path: &Path) -> bool {
    path.extension()
        .is_some_and(|ext| ext.eq_ignore_ascii_case("md"))
}

/// Which sources are on this machine.
///
/// Absent sources are simply not in the answer; a source that is there and holds
/// nothing is a [`Found`] whose [`Found::empty`] is true. See that method for why
/// the two are kept apart.
#[must_use]
pub fn detect(home_root: &Path, workspace: &Path) -> Vec<Found> {
    Source::ALL
        .into_iter()
        .filter_map(|source| {
            let paths = files(source, home_root, workspace);
            if paths.is_empty() {
                return None;
            }
            let bytes = paths
                .iter()
                .filter_map(|path| std::fs::metadata(path).ok())
                .map(|meta| meta.len())
                .sum();
            Some(Found {
                source,
                paths,
                bytes,
            })
        })
        .collect()
}

// ---------------------------------------------------------------------------
// B. The plan
// ---------------------------------------------------------------------------

/// What one item brings across.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Kind {
    /// Standing guidance, appended to an instructions file.
    Instructions,
    /// One MCP server, as a `[[mcp]]` entry.
    Mcp,
    /// One skill, materialised flat under `<home>/skills`.
    Skill,
    /// A model id. Carried for [`crate::providers`], never written here.
    Model,
    /// A command allowlist. **Reported and never translated** — see the module
    /// note.
    Allowlist,
}

impl Kind {
    /// The word a surface labels the row with.
    #[must_use]
    pub fn word(self) -> &'static str {
        match self {
            Kind::Instructions => "instructions",
            Kind::Mcp => "mcp",
            Kind::Skill => "skill",
            Kind::Model => "model",
            Kind::Allowlist => "allowlist",
        }
    }

    /// Whether accepting this item causes [`apply`] to write anything.
    ///
    /// `false` for the two kinds this module deliberately does not write, so a
    /// surface can draw them as an account of what was found rather than as a
    /// checkbox that does nothing when ticked.
    #[must_use]
    pub fn writes(self) -> bool {
        !matches!(self, Kind::Model | Kind::Allowlist)
    }
}

/// Where an accepted item lands.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Destination {
    /// Appended to the instructions file for this scope, through
    /// [`crate::memory::remember`].
    Instructions(Scope),
    /// A `[[mcp]]` entry in the configuration file for this scope, through
    /// [`crate::configure::write`].
    Config(Scope),
    /// A file written at exactly this path.
    File(PathBuf),
    /// **Nothing is written.** An allowlist, a model, or an item this module
    /// refused at plan time — a name already taken, a set over the ceiling. The
    /// reason is in [`Item::says`].
    Nowhere,
}

impl Destination {
    /// The file this lands in, for a session rooted at `root`.
    ///
    /// `None` for [`Destination::Nowhere`], and for a scope that has no path on
    /// this machine — the same answer [`crate::memory::path`] gives, for the same
    /// reason.
    #[must_use]
    pub fn path(&self, root: &Path) -> Option<PathBuf> {
        match self {
            Destination::Instructions(scope) => crate::memory::path(root, *scope),
            Destination::Config(scope) => crate::configure::scope_path(root, *scope),
            Destination::File(path) => Some(path.clone()),
            Destination::Nowhere => None,
        }
    }
}

/// One thing an operator accepts or declines.
///
/// The translated form is carried in the item rather than recomputed at write
/// time, and that is what makes a plan a plan: everything that could fail to
/// *read* has already happened, so accepting an item is a write and nothing else.
/// It is also why a skill's whole text is here — the symlink was followed while
/// the plan was built, so what gets written is a real file whatever happened to
/// the link since.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Item {
    /// Which tool this came from.
    pub source: Source,
    /// What it is.
    pub kind: Kind,
    /// The file it came out of.
    pub from: PathBuf,
    /// One sentence, for an operator deciding.
    pub says: String,
    /// Where it goes.
    pub to: Destination,
    /// The exact bytes that will be written, where anything is: the TOML body of
    /// a `[[mcp]]` entry, the text of an instruction, a skill's whole file, or a
    /// model id. `None` for an item that writes nothing at all.
    pub form: Option<String>,
}

impl Item {
    /// The model id, for a [`Kind::Model`] item.
    #[must_use]
    pub fn model(&self) -> Option<&str> {
        if self.kind != Kind::Model {
            return None;
        }
        self.form.as_deref()
    }

    /// The `[[provider]]` edit for this model, once a caller has decided which
    /// vendor serves it.
    ///
    /// **The vendor is the caller's to supply and cannot be inferred here.** A
    /// foreign tool's model string names a model, not a provider: `gpt-5` is
    /// OpenAI's own name for it and equally what a `compatible` endpoint in front
    /// of it would be asked for. Guessing would produce a `[[provider]]` that
    /// resolves, authenticates against the wrong account, and fails on the first
    /// turn — so the guess is the caller's, made in front of the operator.
    ///
    /// Built by [`crate::providers::add`] rather than spelled here, so there is
    /// one place in this crate that knows what a provider entry looks like.
    #[must_use]
    /// **The endpoint is the caller's, and it cannot be spelled wrongly.** A
    /// foreign model id names a model and never a vendor, so which endpoint
    /// answers for it is a guess — and `crate::providers::Endpoint` is what makes
    /// the guess unable to produce an entry io-harness refuses. The credential is
    /// `None` here on purpose: a key is never carried across, and where one is
    /// wanted the caller passes the variable name it already lives behind.
    pub fn provider_edit(
        &self,
        endpoint: crate::providers::Endpoint<'_>,
    ) -> Option<crate::edit::Edit> {
        Some(crate::providers::add(endpoint, self.model()?, None))
    }
}

/// Build the whole plan.
///
/// **Nothing is written and nothing can be.** Every failure available to this
/// function is a file that would not read or would not decode, and the answer to
/// each is the same: no item. An operator seeing a shorter list than they
/// expected can look at [`detect`]'s output to see what was there; an operator
/// seeing a plan that half-applied could not.
///
/// `scope` is where instructions and configuration go — [`Scope::Project`] is the
/// answer for a repository the operator is standardising, [`Scope::User`] for a
/// machine they are moving onto.
#[must_use]
pub fn plan(found: &[Found], home: &Path, scope: Scope) -> Vec<Item> {
    let mut items: Vec<Item> = Vec::new();
    // Gathered across every source and planned in one go, because the ceiling is
    // a property of the destination directory rather than of any one tool: three
    // skills from Claude and one from a plugin tree are four skills to
    // `Skills::discover`.
    let mut skills: Vec<(Source, PathBuf)> = Vec::new();

    for one in found {
        for path in &one.paths {
            let name = path
                .file_name()
                .map(|name| name.to_string_lossy().to_string())
                .unwrap_or_default();
            match (one.source, name.as_str()) {
                (
                    _,
                    "CLAUDE.md" | "AGENTS.md" | "MEMORY.md" | "GEMINI.md" | ".cursorrules"
                    | "CONVENTIONS.md",
                ) => items.extend(instructions(one.source, path, scope)),
                (Source::Claude, "settings.json") => items.extend(claude_settings(path)),
                (Source::Claude, ".claude.json") => {
                    items.extend(foreign_mcp(one.source, path, scope));
                }
                (Source::Codex, "config.toml") => items.extend(codex_config(path, scope)),
                (Source::Codex, "default.rules") => items.extend(codex_rules(path)),
                (Source::Gemini, "mcp_config.json") => {
                    items.extend(foreign_mcp(one.source, path, scope));
                }
                // Everything [`files`] can produce is named above except the
                // skill files, so this arm is exactly those. Keep it that way:
                // a new anchor file added to `files` without a case here would
                // silently arrive as a skill.
                _ => skills.push((one.source, path.clone())),
            }
        }
    }

    items.extend(skill_plan(&skills, home));
    items
}

// ---------------------------------------------------------------------------
// C. Translation
// ---------------------------------------------------------------------------

/// One instructions file, as a single appended block.
///
/// **The whole file goes through one [`crate::memory::remember`] call, not one
/// per line.** `remember` writes a markdown bullet, and a bullet per line would
/// turn the operator's headings, code fences and nested lists into a flat list of
/// fragments — the document would still be read by the model and would no longer
/// mean what it said. So the bullet is one sentence of provenance and the
/// original text follows it verbatim, at column zero, which is a lazy
/// continuation in markdown and therefore still inside the list item.
///
/// A file that is nothing but whitespace produces no item. `remember` refuses a
/// blank line anyway; refusing it here means the operator is never offered a row
/// that cannot do anything.
fn instructions(source: Source, path: &Path, scope: Scope) -> Vec<Item> {
    let Ok(text) = std::fs::read_to_string(path) else {
        return Vec::new();
    };
    if text.trim().is_empty() {
        return Vec::new();
    }
    let lines = text.lines().count();
    vec![Item {
        source,
        kind: Kind::Instructions,
        from: path.to_path_buf(),
        says: format!(
            "{lines} lines of {} instructions, appended to {}",
            source.word(),
            crate::memory::file_name(scope)
        ),
        to: Destination::Instructions(scope),
        form: Some(format!(
            "imported from {}:\n\n{}",
            path.display(),
            text.trim_end()
        )),
    }]
}

/// One MCP server, as a foreign tool spells it.
///
/// `env` is a map of **names to nothing**: [`IgnoredAny`] means serde parses each
/// value and discards it without ever constructing it. Reading a value here would
/// be the one thing this module promises not to do, and the promise is kept by the
/// type rather than by remembering.
#[derive(serde::Deserialize)]
struct ForeignServer {
    #[serde(default)]
    command: Option<String>,
    #[serde(default)]
    args: Vec<String>,
    #[serde(default)]
    env: BTreeMap<String, IgnoredAny>,
}

/// A JSON file that carries MCP servers.
///
/// Narrow on purpose: `~/.claude.json` is a whole application's state and holds
/// material this module must not touch. Every field not named here is skipped by
/// serde rather than being deserialised into a `Value` and then ignored.
#[derive(serde::Deserialize, Default)]
#[serde(rename_all = "camelCase")]
struct ForeignJson {
    #[serde(default)]
    mcp_servers: BTreeMap<String, ForeignServer>,
    /// Claude keys servers per absolute workspace path, and this is where they
    /// actually are — the top-level map is usually empty.
    #[serde(default)]
    projects: BTreeMap<String, ForeignProject>,
}

#[derive(serde::Deserialize, Default)]
#[serde(rename_all = "camelCase")]
struct ForeignProject {
    #[serde(default)]
    mcp_servers: BTreeMap<String, ForeignServer>,
}

/// Every stdio MCP server in a JSON file, translated.
///
/// The top-level map first, then each project's, and **the first entry to claim a
/// name wins**. Two `[[mcp]]` entries with one `id` is a configuration
/// io-harness reads as two servers under one name, and every tool call after that
/// is ambiguous — so the duplicate is dropped here rather than written and
/// discovered later.
fn foreign_mcp(source: Source, path: &Path, scope: Scope) -> Vec<Item> {
    let Ok(text) = std::fs::read_to_string(path) else {
        return Vec::new();
    };
    let Ok(file) = serde_json::from_str::<ForeignJson>(&text) else {
        return Vec::new();
    };

    let mut seen: BTreeSet<String> = BTreeSet::new();
    let mut items = Vec::new();
    let every = file.mcp_servers.into_iter().chain(
        file.projects
            .into_values()
            .flat_map(|project| project.mcp_servers.into_iter()),
    );
    for (id, server) in every {
        // No command is an HTTP server, or an entry shaped some other way. Left
        // alone rather than guessed at: `McpTransport::Http` wants a URL this
        // struct deliberately does not read, and inventing a `command` from a
        // URL would write a `[[mcp]]` entry that spawns nothing.
        let Some(command) = server.command else {
            continue;
        };
        if !seen.insert(id.clone()) {
            continue;
        }
        let names: Vec<String> = server.env.into_keys().collect();
        items.push(mcp_item(
            source,
            path,
            scope,
            &id,
            &command,
            server.args,
            &names,
        ));
    }
    items
}

/// Codex's `config.toml`: the model, and every `[mcp_servers.<name>]` table.
///
/// Read through [`crate::edit::value_at`] and [`crate::edit::sections`] rather
/// than by parsing — see the module note on why this file may not hold a second
/// TOML reader.
fn codex_config(path: &Path, scope: Scope) -> Vec<Item> {
    let Ok(text) = std::fs::read_to_string(path) else {
        return Vec::new();
    };
    let mut items = Vec::new();

    if let Some(model) = crate::edit::value_at(&text, "model")
        .map(|value| unquoted(&value))
        .filter(|model| !model.is_empty())
    {
        items.push(model_item(Source::Codex, path, &model));
    }

    for name in crate::edit::sections(&text)
        .into_iter()
        .filter(|section| section.len() == 2 && section[0] == "mcp_servers")
        .map(|section| section[1].clone())
    {
        let Some(command) = crate::edit::value_at(&text, &format!("mcp_servers.{name}.command"))
            .map(|v| unquoted(&v))
        else {
            continue;
        };
        let args = crate::edit::value_at(&text, &format!("mcp_servers.{name}.args"))
            .map(|value| strings(&value))
            .unwrap_or_default();
        let names = env_names(&text, &name);
        items.push(mcp_item(
            Source::Codex,
            path,
            scope,
            &name,
            &command,
            args,
            &names,
        ));
    }

    items
}

/// The KEY NAMES of `[mcp_servers.<name>.env]`, and nothing else.
///
/// **The one line scan in this module, and it exists because no value is allowed
/// to be read.** [`crate::edit::value_at`] answers "what is at this path" and
/// there is no public way to ask "what paths are in this table"; a scan that took
/// values would have been a reason to want one. This takes the identifier to the
/// left of the first `=` on each line and stops at the next header.
///
// ponytail: a header inside a multi-line string would end the scan early, which
// `crate::edit::regions` handles properly with a character state machine. A
// config.toml with a `"""` block above its env table is not a thing that exists;
// if one turns up, expose `regions` from `edit` and use it here.
fn env_names(text: &str, server: &str) -> Vec<String> {
    let header = format!("[mcp_servers.{server}.env]");
    let mut names = Vec::new();
    let mut inside = false;
    for line in text.lines() {
        let line = line.trim();
        if line.starts_with('[') {
            inside = line == header;
            continue;
        }
        if !inside {
            continue;
        }
        let Some((key, _)) = line.split_once('=') else {
            continue;
        };
        let key = key.trim().trim_matches('"').trim_matches('\'').trim();
        if !key.is_empty() && !key.starts_with('#') {
            names.push(key.to_string());
        }
    }
    names
}

/// One translated server, as the `[[mcp]]` body io-harness will read back.
///
/// **The bytes come out of the real type through serde, not out of a format
/// string.** [`McpServer`] is `#[serde(flatten)]` over a
/// `#[serde(tag = "transport")]` enum, so the discriminant sits flat beside `id`
/// and a hand-written body that forgot it would load with `missing field
/// transport`. Serialising the value that io-harness itself deserialises is
/// what makes "what was written" and "what will be read" the same question.
///
/// Each key is rendered with `toml::Value`'s own inline form, which is why `env`
/// comes out as an inline table: an appended `[[mcp]]` block cannot carry a
/// `[mcp.env]` header after it without that header attaching to the wrong entry.
fn mcp_item(
    source: Source,
    from: &Path,
    scope: Scope,
    id: &str,
    command: &str,
    args: Vec<String>,
    env: &[String],
) -> Item {
    let mut server = McpServer::stdio(id, command).with_args(args);
    if let McpTransport::Stdio { env: into, .. } = &mut server.transport {
        // **The name pointing at itself.** io-harness resolves `${env:NAME}` from
        // the operator's own environment when it reads the file, so the secret
        // stays where it already was and this process never holds it.
        *into = env
            .iter()
            .map(|name| (name.clone(), format!("${{env:{name}}}")))
            .collect();
    }

    let form = body(&server);
    let note = if env.is_empty() {
        String::new()
    } else {
        format!(
            "; {} kept as ${{env:…}} — set {} in your shell or io-harness refuses the file",
            many(env.len(), "secret"),
            env.join(", ")
        )
    };

    Item {
        source,
        kind: Kind::Mcp,
        from: from.to_path_buf(),
        says: format!(
            "{} MCP server `{id}` running `{command}`{note}",
            source.word()
        ),
        to: if form.is_some() {
            Destination::Config(scope)
        } else {
            Destination::Nowhere
        },
        form,
    }
}

/// The `key = value` lines of one `[[mcp]]` entry.
///
/// `None` where the value will not serialise, which is a shape io-harness could
/// not have produced and this module will not invent a spelling for.
fn body(server: &McpServer) -> Option<String> {
    let value = toml::Value::try_from(server).ok()?;
    let table = value.as_table()?;
    Some(
        table
            .iter()
            .map(|(key, value)| format!("{key} = {value}"))
            .collect::<Vec<_>>()
            .join("\n"),
    )
}

/// Claude's `settings.json`: a model, and an allowlist that is described and
/// never translated.
#[derive(serde::Deserialize, Default)]
struct ClaudeSettings {
    #[serde(default)]
    model: Option<String>,
    #[serde(default)]
    permissions: ClaudePermissions,
}

#[derive(serde::Deserialize, Default)]
struct ClaudePermissions {
    /// Patterns like `Bash(cargo yank *)`. **Counted, never decoded.**
    #[serde(default)]
    ask: Vec<String>,
}

fn claude_settings(path: &Path) -> Vec<Item> {
    let Ok(text) = std::fs::read_to_string(path) else {
        return Vec::new();
    };
    let Ok(settings) = serde_json::from_str::<ClaudeSettings>(&text) else {
        return Vec::new();
    };

    let mut items = Vec::new();
    if let Some(model) = settings.model.filter(|model| !model.trim().is_empty()) {
        items.push(model_item(Source::Claude, path, &model));
    }
    if !settings.permissions.ask.is_empty() {
        items.push(allowlist_item(
            Source::Claude,
            path,
            settings.permissions.ask.len(),
        ));
    }
    items
}

/// Codex's `~/.codex/rules/default.rules`.
///
/// A Python-call DSL — `prefix_rule(pattern=["bun","install"], decision="allow")`
/// — and this function counts its rules and stops. See the module note: there is
/// no honest translation of an argument-matching allowlist into a policy that
/// matches binary names.
fn codex_rules(path: &Path) -> Vec<Item> {
    let Ok(text) = std::fs::read_to_string(path) else {
        return Vec::new();
    };
    let rules = text
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty() && !line.starts_with('#'))
        .filter(|line| line.contains("decision"))
        .count();
    if rules == 0 {
        return Vec::new();
    }
    vec![allowlist_item(Source::Codex, path, rules)]
}

/// The one sentence an allowlist produces.
///
/// `Destination::Nowhere` and `form: None`, both load-bearing: an item with a
/// destination is an item [`apply`] can write, and this one must never be
/// writable however a surface treats it.
fn allowlist_item(source: Source, path: &Path, rules: usize) -> Item {
    Item {
        source,
        kind: Kind::Allowlist,
        from: path.to_path_buf(),
        says: format!(
            "{} {} in {} — reported, not imported. io-harness matches an exec rule on the \
             BINARY NAME alone and does no argument matching, so `bun install` could only \
             become a blanket allow on `bun`. Review them and write the ones you want with \
             /permissions.",
            many(rules, "rule"),
            source.word(),
            path.display()
        ),
        to: Destination::Nowhere,
        form: None,
    }
}

/// The one sentence a model produces, and the id itself in `form`.
fn model_item(source: Source, path: &Path, model: &str) -> Item {
    Item {
        source,
        kind: Kind::Model,
        from: path.to_path_buf(),
        says: format!(
            "{} is set to `{model}` — carried, not written: a `[[provider]]` entry needs a \
             vendor, and a model id does not name one. Pick one with /provider.",
            source.word()
        ),
        to: Destination::Nowhere,
        form: Some(model.to_string()),
    }
}

/// Every skill, flattened, name-checked, and counted against the ceiling.
///
/// The order of the three checks is the order that costs the operator least: a
/// name already claimed is refused individually, because the rest of the import
/// is still good; a set that would go over the ceiling refuses **every** skill,
/// because `Skills::discover` rejects a directory rather than truncating it and
/// a partial import is a state nobody can reason about.
///
/// The oracle for what is already there is [`io_harness::Skills::discover`], the
/// same walk the run will do — not a `read_dir` of io-cli's own, which would
/// disagree with the run in exactly the case the check exists for: a file called
/// anything at all whose frontmatter claims the name.
fn skill_plan(candidates: &[(Source, PathBuf)], home: &Path) -> Vec<Item> {
    if candidates.is_empty() {
        return Vec::new();
    }
    let dir = crate::skills::dir(home);
    // A directory that does not discover is one with nothing in it to collide
    // with. `discover` errors on a directory that is not there at all, which is
    // an install whose home has not been adopted yet.
    let existing = io_harness::Skills::discover(&dir).ok();
    let held = existing.as_ref().map_or(0, |skills| skills.len());

    let mut taken: BTreeSet<String> = BTreeSet::new();
    let mut items: Vec<Item> = Vec::new();
    let mut incoming = 0usize;

    for (source, path) in candidates {
        let name = skill_name(path);
        // **A frontmatter `name:` is somebody else's text and it is about to
        // become a path component.** `skill_name` answers with the declared name
        // because that is the name a skill is *addressed* by — but `Path::join`
        // treats `../../../x` as an escape and an absolute path as a replacement
        // for everything to its left, so the declared name reaching `join`
        // unchecked lets a third-party bundle write a file anywhere the process
        // can reach. Nothing else on the path catches it: the collision guard
        // asks whether the target exists, and creating a *new* file somewhere
        // else answers that happily.
        //
        // Refused rather than sanitised. A name is how the model addresses a
        // skill, so quietly rewriting it would install a skill under a name the
        // operator's other tool does not use — and a skill whose name cannot be a
        // directory is a skill io-harness could not have discovered in the source
        // tree either. Saying so names the file, which is the thing worth knowing.
        if !one_path_component(&name) {
            items.push(Item {
                source: *source,
                kind: Kind::Skill,
                from: path.clone(),
                says: format!(
                    "skill `{name}` is not imported: a skill's name becomes a directory here, \
                     and that one is not a single path component. Rename it in {} and import \
                     again.",
                    path.display()
                ),
                to: Destination::Nowhere,
                form: None,
            });
            continue;
        }
        let target = dir.join(&name);
        let claimed = existing
            .as_ref()
            .is_some_and(|skills| skills.get(&name).is_some())
            || target.exists()
            || dir.join(format!("{name}.md")).exists()
            || !taken.insert(name.clone());
        if claimed {
            items.push(Item {
                source: *source,
                kind: Kind::Skill,
                from: path.clone(),
                says: format!(
                    "skill `{name}` is not imported: {} already answers to that name. \
                     Two skills with one name is `Error::Config` at run start, so this one \
                     is left where it is.",
                    dir.display()
                ),
                to: Destination::Nowhere,
                form: None,
            });
            continue;
        }
        // Read here rather than at write time: this is where a symlink is
        // followed, so what gets written is a real file whatever becomes of the
        // link afterwards.
        let Ok(text) = std::fs::read_to_string(path) else {
            continue;
        };
        incoming += 1;
        items.push(Item {
            source: *source,
            kind: Kind::Skill,
            from: path.clone(),
            says: format!("skill `{name}` from {}", source.word()),
            to: Destination::File(target.join(SKILL_FILE)),
            form: Some(text),
        });
    }

    if held + incoming > io_harness::skills::MAX_SKILLS {
        let sentence = format!(
            "no skill is imported: {} already holds {held} and {} more would be {}, over the \
             {} io-harness allows in one directory. Over the ceiling it rejects the WHOLE \
             directory rather than the excess, so every skill you have would stop loading. \
             Remove some first.",
            dir.display(),
            incoming,
            held + incoming,
            io_harness::skills::MAX_SKILLS,
        );
        for item in &mut items {
            if matches!(item.to, Destination::File(_)) {
                item.to = Destination::Nowhere;
                item.form = None;
                item.says = sentence.clone();
            }
        }
    }

    items
}

/// What a skill file resolves to, the way `Skills::discover` will resolve it.
///
/// [`crate::skillview::describe`] reads the frontmatter `name:` and falls back to
/// the file stem — which is right for a loose `foo.md` and wrong for
/// `foo/SKILL.md`, whose stem is the literal word `SKILL`. io-harness's own
/// `default_name` special-cases exactly that (`skills.rs:345-356`), so this does
/// too; a plan that named three imported skills `SKILL` would collide all three
/// against each other.
/// Whether `name` is exactly one ordinary path component.
///
/// The question is asked of a name that came out of somebody else's file and is
/// about to be joined onto io's own skills directory. It is answered by asking
/// `Path` itself rather than by scanning for `/` and `..`, because the separators
/// and the meaning of a prefix are the platform's business — `C:` and a backslash
/// are components on Windows and are not here, and a hand-rolled check would be
/// right on the machine it was written on.
///
/// Empty is refused, `.` and `..` are refused, an absolute or prefixed path is
/// refused, and anything with more than one component is refused. A NUL is
/// refused too: it cannot reach the filesystem and would otherwise fail far from
/// here with an error naming neither the skill nor the file it came from.
fn one_path_component(name: &str) -> bool {
    if name.is_empty() || name.contains('\0') {
        return false;
    }
    let mut parts = Path::new(name).components();
    matches!(parts.next(), Some(std::path::Component::Normal(_))) && parts.next().is_none()
}

fn skill_name(path: &Path) -> String {
    let (name, _) = crate::skillview::describe(path);
    if name.eq_ignore_ascii_case("SKILL") {
        if let Some(parent) = path.parent().and_then(Path::file_name) {
            return parent.to_string_lossy().to_string();
        }
    }
    name
}

// ---------------------------------------------------------------------------
// E. Applying
// ---------------------------------------------------------------------------

/// What [`apply`] did, in the order it did it.
///
/// The shape [`crate::home::Report`] uses, for the reason that one uses it: the
/// report is owed until somebody delivers it, so every failure is collected and
/// the rest of the plan carries on. An import that stopped at the first bad item
/// would leave the operator with a half-applied plan and no list of which half.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Report {
    /// Every destination written, and what went there. **This is the undo
    /// list** — an operator who wants the import back out needs the file and the
    /// entry, and one line saying "wrote io.toml" three times is not that.
    pub written: Vec<(PathBuf, String)>,
    /// Every item that was accepted and could not be written, with the reason.
    pub refused: Vec<String>,
    /// Every item the plan carries that this module never writes: a model
    /// waiting on a vendor, an allowlist reported and not translated, a skill
    /// refused at plan time.
    pub carried: Vec<String>,
}

impl Report {
    /// One line per outcome, written first, then what was carried, then what
    /// failed.
    #[must_use]
    pub fn lines(&self) -> Vec<String> {
        let mut out =
            Vec::with_capacity(self.written.len() + self.carried.len() + self.refused.len() + 1);
        for (path, what) in &self.written {
            out.push(format!("wrote {what} into {}", path.display()));
        }
        out.extend(self.carried.iter().cloned());
        out.extend(self.refused.iter().cloned());
        if out.is_empty() {
            out.push("nothing was imported".to_string());
        }
        out
    }
}

/// Write the accepted items, and say what happened to every one of them.
///
/// `plan` here is the **accepted** subset — a surface filters the plan and hands
/// back what the operator said yes to. Nothing is read from a source in this
/// function: every byte that will be written was translated in [`plan`], so a
/// tool the operator uninstalled between the two calls cannot change what lands.
///
/// **Every failure is collected.** A `[[mcp]]` entry io-harness refuses does not
/// stop the instructions from being appended, and it does not leave the
/// configuration file changed either — [`crate::configure::write`] round-trips
/// through `Config::discover` and rolls the file back when the harness says no.
/// That is what catches the one trap in this whole module: `${env:NAME}` for a
/// variable that is not set is a **hard parse error**, so a server whose secret
/// the operator has not exported yet is refused loudly at import time rather than
/// killing their next session.
/// **No `home` argument, and that is the design rather than an omission.** Every
/// destination is resolved when the plan is *built*, so the absolute path an
/// operator read on the review surface is the byte-for-byte path written here.
/// Re-deriving one from the home at write time would open a gap between what was
/// shown and what happens, on the one surface where that gap is the whole risk.
///
/// A `Kind::Model` item is reported as **carried, not written**: a foreign model
/// id names a model and never a vendor, and `[[provider]]` needs both. The
/// endpoint is the caller's to ask for, in front of the operator, and the write
/// goes through [`Item::provider_edit`].
pub fn apply(plan: &[Item], root: &Path) -> Report {
    let mut report = Report::default();
    // `[instructions] files` is only worth writing for the scopes that need it.
    // `AGENTS.md` is io-harness's `DEFAULT_INSTRUCTIONS` and is read by a project
    // with no configuration at all; the other two are unreachable without the
    // list, which is what `memory::install` writes.
    let mut needs_install = false;

    for item in plan {
        match (item.kind, &item.to) {
            (_, Destination::Nowhere) | (Kind::Model, _) | (Kind::Allowlist, _) => {
                report.carried.push(item.says.clone());
            }
            (Kind::Instructions, Destination::Instructions(scope)) => {
                let Some(form) = item.form.as_deref() else {
                    report.refused.push(nothing_to_write(item));
                    continue;
                };
                match crate::memory::remember(root, *scope, form) {
                    Ok(path) => {
                        if *scope != Scope::Project {
                            needs_install = true;
                        }
                        report.written.push((path, item.says.clone()));
                    }
                    Err(error) => report.refused.push(failed(item, &error)),
                }
            }
            (Kind::Mcp, Destination::Config(scope)) => {
                let Some(form) = item.form.as_deref() else {
                    report.refused.push(nothing_to_write(item));
                    continue;
                };
                // **Asked of the file, once per server, immediately before the
                // append.** The dedupe inside the readers is per-source, so the
                // same server declared by two tools — `context7` and `playwright`
                // are in both Claude's and Gemini's files on a real machine —
                // arrived twice; and nothing at all compared the plan against what
                // `io.toml` already declared, so a second `/import`, or the
                // first-run offer followed by `/import` later, appended a second
                // entry under an id that was already there. io-harness validates
                // no such thing, so `configure::write`'s round trip accepts it and
                // rolls nothing back.
                //
                // Checking here rather than at plan time is what makes it cover
                // both: writes happen one server at a time, so the first of a
                // within-plan pair is on disk by the time the second is asked
                // about. `servers::declared_in` was written for this caller.
                let id = crate::edit::value_at(form, "id")
                    .map(|value| value.trim().trim_matches('"').to_string());
                if let Some(id) = id.filter(|id| crate::servers::declared_in(root, id).is_some()) {
                    report.refused.push(format!(
                        "MCP server `{id}` is not imported: a server of that id is already \
                         declared. Nothing was changed — `/mcp` shows the one in force."
                    ));
                    continue;
                }
                // One write per server, deliberately. A batch would put every
                // entry behind one `Config::discover`, so a single server naming
                // an unset variable would take the whole import down with it.
                let edits = [crate::edit::Edit::append("mcp", form)];
                match crate::configure::write(root, *scope, &edits) {
                    Ok(()) => {
                        let at = crate::configure::scope_path(root, *scope)
                            .unwrap_or_else(|| PathBuf::from("io.toml"));
                        report.written.push((at, item.says.clone()));
                    }
                    Err(error) => report.refused.push(failed(item, &error)),
                }
            }
            (Kind::Skill, Destination::File(path)) => {
                let Some(form) = item.form.as_deref() else {
                    report.refused.push(nothing_to_write(item));
                    continue;
                };
                // Asked again here and not only at plan time. The plan may be
                // minutes old, and overwriting somebody's skill is the one
                // outcome this half of the module exists to prevent.
                if path.exists() {
                    report
                        .refused
                        .push(failed(item, "there is already a file there"));
                    continue;
                }
                let made = path.parent().map_or(Ok(()), crate::home::create);
                if let Err(error) = made.and_then(|()| std::fs::write(path, form)) {
                    report.refused.push(failed(item, &error.to_string()));
                    continue;
                }
                report.written.push((path.clone(), item.says.clone()));
            }
            // A kind and a destination that do not go together. Reported rather
            // than written somewhere plausible: an item this function does not
            // understand is one it must not guess about.
            _ => report.refused.push(failed(
                item,
                "this item's destination does not match what it is",
            )),
        }
    }

    if needs_install {
        // Best effort and never fatal: the guidance is already on disk, and what
        // a failure here costs is io-harness reading it. Reported so the operator
        // knows which of the two happened.
        if let Err(error) = crate::memory::install(root) {
            report.refused.push(format!(
                "the instructions were written but `[instructions] files` was not updated, \
                 so io-harness may not read them: {error}"
            ));
        }
    }

    report
}

fn failed(item: &Item, error: &str) -> String {
    format!(
        "could not import {} from {}: {error}",
        item.kind.word(),
        item.from.display()
    )
}

fn nothing_to_write(item: &Item) -> String {
    format!(
        "{} from {} carries nothing to write",
        item.kind.word(),
        item.from.display()
    )
}

// ---------------------------------------------------------------------------
// Small readers
// ---------------------------------------------------------------------------

/// `1 rule` or `3 rules`.
fn many(n: usize, noun: &str) -> String {
    if n == 1 {
        format!("1 {noun}")
    } else {
        format!("{n} {noun}s")
    }
}

/// Every string in the source text of a TOML array.
///
/// **The source text, because that is all [`crate::edit::value_at`] returns** —
/// it quotes a value's own bytes and says nothing about what they mean, which is
/// the line `tests/dependencies.rs` draws around TOML in this crate. Decoding a
/// list of strings out of those bytes is the same kind of act as
/// [`crate::providers`] taking the quotes off one.
///
/// Both quote styles, because TOML has both and Codex's own file uses basic
/// strings for commands and literal strings for Windows paths.
///
// ponytail: no `\uXXXX`, no `\U`, no multi-line `"""` element. An MCP argument
// list is program arguments — flags and paths — and none of the three has ever
// appeared in one. Reach for a real decoder the first time a bug report shows an
// escape this drops.
fn strings(source: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut chars = source.chars();
    loop {
        let Some(open) = chars.next() else {
            return out;
        };
        if open != '"' && open != '\'' {
            continue;
        }
        let mut item = String::new();
        while let Some(c) = chars.next() {
            if c == open {
                break;
            }
            // A literal string has no escapes at all — that is what makes it
            // literal, and treating `\` as one there would eat a Windows path
            // separator.
            if c == '\\' && open == '"' {
                let Some(escaped) = chars.next() else { break };
                item.push(match escaped {
                    'n' => '\n',
                    't' => '\t',
                    'r' => '\r',
                    other => other,
                });
                continue;
            }
            item.push(c);
        }
        out.push(item);
    }
}

/// One TOML string value, with its quotes taken off.
///
/// Falls back to the trimmed source for a value that is not a string at all, so
/// a `model = 3` in somebody's file reads as `3` rather than as nothing.
fn unquoted(source: &str) -> String {
    strings(source)
        .into_iter()
        .next()
        .unwrap_or_else(|| source.trim().to_string())
}
