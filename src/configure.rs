//! The configuration file as a surface: what is set, and which file decided it.
//!
//! io-harness has recorded the origin of every key since its 0.30.0 —
//! [`Config::origin`] answers "which file decided this one" and [`Config::origins`]
//! walks the lot — and no io-cli release has ever asked. This module is that
//! question asked, and the answer rendered.
//!
//! # Three scopes, and a fourth answer that is not a file
//!
//! [`Scope`] is `User`, `Project` and `Local`: the operator's own file, the
//! committed `io.toml`, and the gitignored `io.local.toml`. There is no fourth.
//!
//! What there is instead is a key **no file named**, and it is the case worth
//! being careful about. `Config::origin` returns an **empty slice** for it, which
//! is io-harness's own default speaking — and a surface that reached for the
//! lowest-precedence source file to fill the column in would attribute a crate
//! default to a file the operator never wrote it in. That is a lie a reader
//! cannot detect, so [`Decided::Default`] is its own answer and names no path.
//!
//! This crate has paid for that distinction once already: 0.15.0's
//! `home::origin` reported `IO_CONFIG_HOME` for io-cli's own default because
//! `adopt` had set the variable itself, crediting the operator for a choice they
//! never made.
//!
//! # Where a value's text comes from
//!
//! Mostly from io-harness's typed accessors, which are the authority on what a
//! merged configuration means. But **not every section has one**: `MemorySection`
//! is private and there is no `Config::memory()`, so a surface built only on
//! accessors would have a hole exactly where an operator had written something.
//!
//! For those keys the value is **quoted from the file `origin()` names**, through
//! [`crate::edit::value_at`]. Quoting a named file's own bytes is a different act
//! from deciding what a setting means — the origin says which file, and the text
//! is what that file says. Nothing here merges, defaults or interprets.

use std::collections::BTreeSet;
use std::path::PathBuf;

use io_harness::config::{Config, Scope};

/// What decided a key.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Decided {
    /// A file named it. Which scope, and the path as it was read.
    File { scope: Scope, path: PathBuf },
    /// No file named it, so io-harness's own default is in force. This names no
    /// path on purpose.
    Default,
}

impl Decided {
    /// The word shown in the origin column.
    ///
    /// A scope rather than a filename, because three files are called `io.toml`
    /// and only one of them is the operator's own.
    pub fn word(&self) -> &'static str {
        match self {
            Decided::File {
                scope: Scope::User, ..
            } => "user",
            Decided::File {
                scope: Scope::Project,
                ..
            } => "project",
            Decided::File {
                scope: Scope::Local,
                ..
            } => "local",
            Decided::Default => "default",
        }
    }

    /// The file, where there is one.
    pub fn path(&self) -> Option<&std::path::Path> {
        match self {
            Decided::File { path, .. } => Some(path),
            Decided::Default => None,
        }
    }

    /// The scope that decided it, where a file did.
    ///
    /// **What a rewrite of an existing value has to be aimed at.** Writing into a
    /// higher-precedence scope than the one that holds a key shadows it rather
    /// than updating it: the value in force changes, and the file the operator
    /// opens still shows the old one. `None` is io-harness's own default, which no
    /// file holds and which a caller therefore has to choose a scope for.
    pub fn scope(&self) -> Option<Scope> {
        match self {
            Decided::File { scope, .. } => Some(*scope),
            Decided::Default => None,
        }
    }
}

/// One row of the surface.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Setting {
    /// The dotted key, as io-harness spells it.
    pub path: String,
    /// The value as the deciding file writes it, already redacted where it is a
    /// credential. `None` where no file named the key — there is nothing to
    /// quote, and inventing the crate's default here would be this module
    /// claiming to know a value it did not read.
    pub value: Option<String>,
    /// What decided it.
    pub decided: Decided,
}

/// The keys this surface offers even when no file names them.
///
/// io-cli's own list, because io-harness exposes no enumeration of its schema —
/// every section struct is private and `deny_unknown_fields` is the only thing
/// that speaks. A list nothing has to agree with is decoration, so
/// `tests/configure.rs` asserts every key here is one `docs/config.example.toml`
/// documents, and that file is itself gated.
///
/// It is deliberately not exhaustive of the file format: `[[agent]]`, `[[hook]]`,
/// `[[plugin]]` and `[toolchain]` are readable through [`settings`] below but are
/// not editable here, which the release contract excludes by name.
pub const CATALOGUE: &[&str] = &[
    // The boundary, first, because it is the one an operator changes with the
    // most at stake. Four acts and not three: io-harness separates `read` from
    // `write`, and a surface that offered one `fs` row would be inventing a key
    // the schema rejects.
    "policy.defaults.read",
    "policy.defaults.write",
    "policy.defaults.exec",
    "policy.defaults.net",
    "sandbox.mode",
    "sandbox.allow_network",
    "sandbox.force_floor",
    // What one turn may spend.
    "run.max_steps",
    "run.max_tokens",
    "run.max_duration_secs",
    "run.max_retries",
    "run.exec_timeout_secs",
    // What the store keeps, which outlives every run over it.
    "memory.max_entries",
    "memory.max_chars",
    "memory.max_entry_chars",
    // io-cli's own.
    "app.io-cli.theme",
    "app.io-cli.diff",
    "app.io-cli.glyphs",
    "app.io-cli.plain",
    // The three `TaskContract` ceilings io-harness gives no key of its own, so
    // io-cli names them here and `/config` is where an operator meets them.
    "app.io-cli.max_parallel_reads",
    "app.io-cli.spawn_background_after_secs",
    "app.io-cli.detached_spawns",
    // What a turn costs, split across two sections because one of them is not
    // ours. `prices.as_of` is io-harness's and dates the whole table; it is
    // written by a fetch rather than typed, and it is listed here because a date
    // an operator cannot see is a claim with no expiry they cannot check.
    // `[prices.models]` is deliberately **not** a row: it is a list rather than a
    // setting, and it is reached the way `/provider` and `/mcp` reach a list.
    "prices.as_of",
    "app.io-cli.prices.source_url",
    // What "done" means here. Every key of the section and not a chosen few,
    // because the section refuses rather than defaults: exactly one of `command`,
    // `file` and `rubric` may be set, and an operator meeting three of the eight
    // on this surface would be reading a partial list as the whole schema and
    // writing an ambiguous section from it. `expect_exit`, `contains` and
    // `allow_self_review` each qualify exactly one of those three, and a
    // qualifier nobody can see is a qualifier nobody sets.
    //
    // Unlike `prices.source`, none of these is written by machinery — a gate is
    // typed by the operator or it does not exist — so there is no key here that
    // is listed only to be read.
    "app.io-cli.gates.retries",
    "app.io-cli.gates.command",
    "app.io-cli.gates.expect_exit",
    "app.io-cli.gates.file",
    "app.io-cli.gates.contains",
    "app.io-cli.gates.rubric",
    "app.io-cli.gates.reviewer",
    "app.io-cli.gates.allow_self_review",
    // Whether a question that is only a question opens a run. One key rather than
    // a section, and listed because its default is io-harness's rather than
    // io-cli's: an operator who wants every prompt to be a run has no other way to
    // discover that the answer is currently no.
    "app.io-cli.conversational",
    // When a run changes model, and to which. Every key of both rules for the
    // reason the gate's eight are all here: a threshold without its model is half
    // a rule, and an operator meeting one half on this surface would write a
    // section that refuses to parse. `require_primary` is deliberately absent —
    // see `crate::routing` for the provider method that never answers.
    //
    // **This surface is also where the containment disclosure is owed.** The rules
    // reach the contract on every arm and fire only on the flat one, so an
    // operator editing them with `[app.io-cli.containment]` in the same file is
    // told here as well as at startup — see `routing::inert_under_containment`.
    "app.io-cli.routing.escalate_after.failures",
    "app.io-cli.routing.escalate_after.model",
    "app.io-cli.routing.downshift_under.bytes",
    "app.io-cli.routing.downshift_under.model",
];

/// How a value for a key is obtained.
///
/// **The kind says how a value is *obtained*, never what it means.** io-harness
/// owns discovery, layering and validation, and a surface that started deciding
/// what a setting does would be a second configuration parser disagreeing with the
/// first. What this answers is only: can the options be shown, and in what shape.
///
/// Until 0.28.0 every one of the thirty-seven keys was typed blind — `/config`
/// prefilled the key and left the value to an operator whatever its kind, so
/// setting `policy.defaults.write` meant guessing a value out of a set the pinned
/// dependency has made public. Asking someone to type a value you could have
/// offered, and could have proven, is the defect this exists to remove.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Kind {
    /// `true` or `false`.
    Flag,
    /// A closed set, whose members are the dependency's own. See [`effects`] and
    /// [`exec_modes`] for how each set is obtained and what that guarantees.
    Choice(Vec<String>),
    /// A whole number, chosen from the ladder [`ladder`] builds.
    ///
    /// `signed` because `app.io-cli.gates.expect_exit` is an `i32` and a process
    /// may legitimately be expected to exit on a negative status; every other
    /// number key here counts something and cannot go below zero.
    Number { signed: bool },
    /// A model name, chosen from `[prices.models]` already in the file. **Never a
    /// network call**: a settings screen that reached for the network to draw a
    /// list would be spending an operator's money to render a menu.
    Model,
    /// A path, chosen from the workspace.
    File,
    /// A list of strings, written through [`crate::edit::array`].
    ///
    /// Exactly one key — `app.io-cli.gates.command` is `Option<Vec<String>>`
    /// (`src/gates.rs:84`) — and it has its own kind rather than being folded into
    /// [`Kind::Text`] because a scalar written to that key is a value io-harness
    /// cannot read back. The generic "type a value" editor this release replaces
    /// would have written exactly that.
    List,
    /// Text no menu can hold: a substring to look for, a rubric, a URL. Three keys.
    Text,
    /// Written by machinery, not by a person, and so never offered for typing.
    ///
    /// One key, `prices.as_of`. It is in the catalogue because a date an operator
    /// cannot see is a claim with no expiry they cannot check — but it is a fact to
    /// read, and the act beside it is [`REFRESH_PRICES`].
    Machine,
}

/// The `Effect` variants, spelled as the file spells them.
///
/// **This is a build-breaking census and it can be, because `Effect` is not
/// `#[non_exhaustive]`** (`io-harness-0.69.0/src/policy.rs:91-97`). The array
/// names every variant and the `match` covers every variant, so a variant *added*
/// by a later io-harness fails the match and a variant *removed* fails the array.
/// Either way this crate stops compiling rather than shipping a menu that quietly
/// omits an option — and an option that was never offered is one an operator
/// cannot detect, which is why that guarantee is worth a function.
///
/// **The one thing it does not catch** is a rename: the strings are serde's
/// `rename_all = "snake_case"` of the variant names, and io-harness exposes no
/// `as_str` for `Effect` the way it does for [`exec_modes`], so io-cli has to
/// spell them. A dependency that renamed `Allow` to `Permit` would leave this
/// compiling and writing a word the schema rejects. `tests/configure.rs` closes
/// that by round-tripping each string through io-harness's own deserializer.
#[must_use]
pub fn effects() -> Vec<String> {
    [
        io_harness::Effect::Allow,
        io_harness::Effect::Ask,
        io_harness::Effect::Deny,
    ]
    .iter()
    .map(|effect| {
        match effect {
            io_harness::Effect::Allow => "allow",
            io_harness::Effect::Ask => "ask",
            io_harness::Effect::Deny => "deny",
        }
        .to_string()
    })
    .collect()
}

/// The `ExecMode` variants, spelled by io-harness itself.
///
/// **A weaker guarantee than [`effects`]'s, and the difference is the
/// dependency's and not a choice made here.** `ExecMode` *is* `#[non_exhaustive]`
/// (`io-harness-0.69.0/src/sandbox.rs:379`), so rustc requires a wildcard arm on
/// any match over it and a variant added by a later io-harness would fall into
/// that wildcard silently — compiling green while the menu omits the new mode.
/// Only a *removal* breaks the build here.
///
/// So what is asserted is what is obtainable: the spelling of every variant comes
/// from `ExecMode::as_str` (`sandbox.rs:400`) rather than from io-cli, so a rename
/// cannot slip through; and [`exec_mode_label`] reports an unknown mode rather
/// than dropping it. An issue asking io-harness for variant enumeration on both
/// enums is filed with this release — `strum` is not an answer, being forbidden by
/// io-harness's own NF2 and by this crate's no-new-dependency constraint.
#[must_use]
pub fn exec_modes() -> Vec<String> {
    [
        io_harness::ExecMode::ReadOnly,
        io_harness::ExecMode::WorkspaceWrite,
        io_harness::ExecMode::FullAccess,
    ]
    .iter()
    .map(|mode| mode.as_str().to_string())
    .collect()
}

/// How a mode reads on the surface, including one this build has never heard of.
///
/// The mitigation [`exec_modes`] describes. A wildcard that omits is the defect; a
/// wildcard that says so is the most a `#[non_exhaustive]` enum allows, and it
/// turns "the menu is missing an option" — which nobody can see — into a row that
/// names the mode and admits io-cli does not know it.
#[must_use]
pub fn exec_mode_label(mode: io_harness::ExecMode) -> String {
    match mode {
        io_harness::ExecMode::ReadOnly
        | io_harness::ExecMode::WorkspaceWrite
        | io_harness::ExecMode::FullAccess => mode.as_str().to_string(),
        _ => format!(
            "{} — this build of io-cli does not know this mode; it is io-harness's and it is in \
             force",
            mode.as_str()
        ),
    }
}

/// The kind of a catalogue key, or `None` for a key no catalogue entry names.
///
/// `None` is not a gap to fill in: [`settings`] deliberately lists keys an
/// operator wrote which this catalogue does not know about, and a kind guessed for
/// one of those would be io-cli inventing a schema. Such a key stays readable and
/// is edited as text.
#[must_use]
pub fn kind_of(key: &str) -> Option<Kind> {
    Some(match key {
        // The boundary. Four acts, one set of effects.
        "policy.defaults.read"
        | "policy.defaults.write"
        | "policy.defaults.exec"
        | "policy.defaults.net" => Kind::Choice(effects()),
        "sandbox.mode" => Kind::Choice(exec_modes()),
        // io-cli's own closed sets. Unlike the two above these are this crate's to
        // define, so the literal is the schema rather than a copy of one.
        "app.io-cli.theme" => Kind::Choice(vec!["dark".into(), "light".into()]),
        "app.io-cli.diff" => Kind::Choice(vec!["unified".into(), "minimal".into()]),
        "app.io-cli.glyphs" => Kind::Choice(vec!["unicode".into(), "ascii".into()]),
        "sandbox.allow_network"
        | "sandbox.force_floor"
        | "app.io-cli.plain"
        | "app.io-cli.detached_spawns"
        | "app.io-cli.gates.allow_self_review"
        | "app.io-cli.conversational" => Kind::Flag,
        // The one signed number: a process may be expected to exit negative.
        "app.io-cli.gates.expect_exit" => Kind::Number { signed: true },
        "run.max_steps"
        | "run.max_tokens"
        | "run.max_duration_secs"
        | "run.max_retries"
        | "run.exec_timeout_secs"
        | "memory.max_entries"
        | "memory.max_chars"
        | "memory.max_entry_chars"
        | "app.io-cli.max_parallel_reads"
        | "app.io-cli.spawn_background_after_secs"
        | "app.io-cli.gates.retries"
        | "app.io-cli.routing.escalate_after.failures"
        | "app.io-cli.routing.downshift_under.bytes" => Kind::Number { signed: false },
        "app.io-cli.gates.reviewer"
        | "app.io-cli.routing.escalate_after.model"
        | "app.io-cli.routing.downshift_under.model" => Kind::Model,
        "app.io-cli.gates.file" => Kind::File,
        "app.io-cli.gates.command" => Kind::List,
        "app.io-cli.gates.contains"
        | "app.io-cli.gates.rubric"
        | "app.io-cli.prices.source_url" => Kind::Text,
        "prices.as_of" => Kind::Machine,
        _ => return None,
    })
}

/// The one-two-five ladder around `current`, nearest first.
///
/// **Anchored on the value in force, and that is the audit's finding rather than a
/// preference.** The plan for this release said the ladder would be anchored on
/// io-harness's own default for the key. There is no such thing to read:
/// `run.max_tokens` and `run.max_duration_secs` are `None` in both `TaskContract`
/// constructors (`contract.rs:652,730`), `run.max_steps` is 8 in one and 12 in the
/// other (`:650,:728`) so neither is "the default", the `[run]` section is a
/// private struct with no getter, and `io_harness::Defaults` is the policy tier
/// defaults under a colliding name. So the anchor is the value the operator
/// actually has — which is also the value they are reasoning from.
///
/// `None` — a key no file names — ladders from 1. The alternative was an empty
/// picker saying so, and a surface whose whole argument is that a value is chosen
/// rather than typed cannot have a state in which it offers nothing.
///
/// The ladder is 1, 2, 5 at each magnitude, which is why it needs no per-key step
/// table to go stale and invents no bound the dependency does not expose. A signed
/// key ladders through zero into the negatives.
#[must_use]
pub fn ladder(current: Option<i64>, signed: bool) -> Vec<i64> {
    let mut rungs: Vec<i64> = Vec::new();
    let mut magnitude: i64 = 1;
    // Ten magnitudes covers 1 to 5,000,000,000 — past `memory.max_chars` and past
    // any token ceiling a provider sells — and stops well short of `i64`'s edge,
    // so the multiplication below cannot overflow.
    for _ in 0..10 {
        for step in [1_i64, 2, 5] {
            rungs.push(step * magnitude);
        }
        magnitude = magnitude.saturating_mul(10);
    }
    if signed {
        let mut whole: Vec<i64> = rungs.iter().rev().map(|rung| -rung).collect();
        whole.push(0);
        whole.extend(rungs);
        rungs = whole;
    } else {
        rungs.insert(0, 0);
    }
    // The value in force is a rung whether or not it sits on the ladder, because a
    // list that silently omits what the file currently says is a list an operator
    // cannot find their own setting in.
    if let Some(value) = current {
        if !rungs.contains(&value) {
            rungs.push(value);
        }
    }
    rungs.sort_unstable();
    rungs.dedup();
    // Nearest the anchor first: the rung an operator wants is almost always the one
    // either side of where they are, and a list starting at zero buries it.
    //
    // **Distance along the ladder, never numeric distance**, and the first attempt
    // at this got it wrong in a way only a test caught. The rungs are logarithmic,
    // so from 200 the arithmetic neighbours are 100, 50, 20 — every rung *below* —
    // and 500, the one step up an operator is most likely to want, sorts ninth. A
    // ladder ordered by subtraction is a ladder that only goes down.
    let anchor = current.unwrap_or(1);
    let at = rungs
        .iter()
        .position(|rung| *rung == anchor)
        // A value in force is always pushed above, so this only runs for `None`,
        // where the anchor is 1 and always present. Kept total rather than
        // unwrapped: a panic on a settings screen is never the right answer.
        .unwrap_or(0);
    // The list is sorted and deduplicated, so a rung's index *is* its position on
    // the ladder — taken here, before the reorder, because a key that read the list
    // it is reordering would be reading positions that have already moved.
    let mut ordered: Vec<(usize, i64)> = rungs.into_iter().enumerate().collect();
    ordered.sort_by_key(|(position, rung)| (position.abs_diff(at), *rung));
    ordered.into_iter().map(|(_, rung)| rung).collect()
}

/// Whether writing `value` to `key` would be refused in a **project-scoped** file.
///
/// io-harness refuses five (key, value) pairs in a committed `io.toml`
/// (`PROJECT_WIDENING`, `io-harness-0.69.0/src/config.rs:1759-1769`): the two acts
/// defaulted to `allow`, egress re-opened inside the sandbox, the portable floor
/// switched off, and the widest exec mode. The narrowing value of each stays legal,
/// which is what the scope is for.
///
/// **Mirrored here because a menu that offers a value the destination file will
/// refuse is a menu that lies, and the cost is not one key.** `refuse_widening`
/// runs before deserialization, so the refusal takes the *whole file*: an operator
/// who picks `full-access` on a key their project `io.toml` decides does not get a
/// rejected setting, they get a configuration that no longer parses. `write`
/// already re-reads and refuses with io-harness's own sentence — this is what lets
/// the row say so beforehand instead.
///
/// The pairs are io-harness's and are spelled here because it exposes no reader for
/// them; `tests/configure.rs` round-trips each one through `Config::from_toml`, so a
/// pair the dependency adds or drops is caught by the gate rather than by an
/// operator.
#[must_use]
pub fn widens_project(key: &str, value: &str) -> bool {
    matches!(
        (key, value.trim().trim_matches('"')),
        ("policy.defaults.exec", "allow")
            | ("policy.defaults.net", "allow")
            | ("sandbox.allow_network", "true")
            | ("sandbox.force_floor", "false")
            | ("sandbox.mode", "full-access")
    )
}

/// The `/config` row that re-reads the price catalogue.
///
/// **A row rather than a key, because it is an act and not a setting.** Every
/// other row on that surface names something in a file and puts it in the
/// composer for the operator to type a value after; this one does something. It
/// is a sentinel and not a path so that `settings` and `setting` never see it —
/// a key that is not in any file and never will be would show as "not set"
/// forever on a surface whose whole job is saying what is in force.
///
/// It lives here rather than in the driver so a test can reach it: nothing under
/// `tests/` can link `src/main.rs`, and a row spelled in the driver is a row no
/// test can assert on.
pub const REFRESH_PRICES: &str = "!refresh-prices";

/// The label that sentinel wears on the picker.
pub fn refresh_row(setting: &Setting) -> crate::picker::Row {
    let detail = match &setting.value {
        Some(as_of) => format!("last read {as_of}"),
        None => "no prices are configured".to_string(),
    };
    crate::picker::Row::with_detail("prices: re-read the catalogue", detail)
}

/// Whether a key's value is a credential and must never be shown in full.
///
/// N2. The test is the key rather than the value, because a credential that
/// happened to look like a word would otherwise be printed, and a key named
/// `api_key` is a credential whatever it contains.
fn is_credential(path: &str) -> bool {
    let last = path.rsplit('.').next().unwrap_or(path);
    matches!(last, "api_key" | "token" | "secret" | "password")
}

/// A value as it is safe to show.
///
/// A `${env:VAR}` or `${file:PATH}` reference is shown **as written**: the
/// variable's name is the information an operator needs and its contents are not.
///
/// **There are three substitution forms and not two.** io-harness resolves
/// `${env:...}`, `${file:...}` **and** `${cmd:...}`
/// (`io-harness-0.69.0/src/config.rs:1909`); this comment claimed two until
/// 0.21.0, and the sentence it claimed it in was the argument for which forms
/// pass through here. The third is deliberately not one of them: a `${env:}` or
/// `${file:}` reference is a *name*, and the name is the whole of what an
/// operator needs to identify the credential. `${cmd:}` is a program and its
/// arguments — the arguments are values rather than names, and printing them
/// whole under a key this function was called on precisely because it holds a
/// secret is the one thing it exists to avoid. So it is reduced like any other
/// value. Separately, io-harness refuses `${cmd:}` outright in a project-scoped
/// file, so the form reaches this function only from a file the operator owns.
///
/// Anything else in a credential key is reduced to its last four characters,
/// which is enough to tell two keys apart and not enough to use.
pub fn redact(path: &str, value: &str) -> String {
    if !is_credential(path) {
        return value.to_string();
    }
    let bare = value.trim().trim_matches('"');
    if bare.starts_with("${env:") || bare.starts_with("${file:") {
        return value.to_string();
    }
    let tail: String = bare
        .chars()
        .rev()
        .take(4)
        .collect::<Vec<_>>()
        .into_iter()
        .rev()
        .collect();
    if bare.len() <= 4 {
        "\"set\"".to_string()
    } else {
        format!("\"…{tail}\"")
    }
}

/// Every setting this surface shows, in catalogue order then whatever else the
/// files named.
///
/// The second half is the property that matters: a key an operator wrote which
/// io-cli's catalogue does not know about is still shown, with its origin, rather
/// than being invisible. A surface that only listed what it already knew would
/// hide exactly the keys a reader went looking for.
pub fn settings(config: &Config) -> Vec<Setting> {
    let mut seen: BTreeSet<String> = BTreeSet::new();
    let mut rows: Vec<Setting> = Vec::new();

    for key in CATALOGUE {
        seen.insert((*key).to_string());
        rows.push(setting(config, key));
    }

    // Anything a file named that the catalogue does not carry — **except the
    // price table, which is a list and not a set of settings.**
    //
    // io-harness's origin index records *leaf* keys, so a filled `[prices.models]`
    // contributes two to five of them per model: `prices.models.<id>.input`,
    // `.output`, `.cache_read`, and so on. On an OpenRouter install that is well
    // over a thousand rows, and `/config` — a picker an operator opens to change
    // one setting — becomes a list of rates with every real setting buried above
    // them. The `CATALOGUE` comment already says `[prices.models]` is deliberately
    // not a row; this is what makes that true, because the sweep below was putting
    // every one of them back.
    //
    // They would not even render: `record_origins` joins the path with `.` and
    // does not quote, so a model id containing a dot arrives as a key that
    // `edit::value_at` cannot resolve, and the row draws with no value at all.
    let mut extra: Vec<String> = config
        .origins()
        .map(|(key, _)| key.to_string())
        .filter(|key| !seen.contains(key))
        .filter(|key| !key.starts_with("prices.models."))
        .collect();
    extra.sort();
    for key in extra {
        rows.push(setting(config, &key));
    }

    rows
}

/// One key, resolved.
pub fn setting(config: &Config, key: &str) -> Setting {
    let origins = config.origin(key);
    let decided = match origins.last() {
        // The last origin is the winning one: the scopes merge in precedence
        // order, and a key more than one file named lists them in that order.
        Some(origin) => Decided::File {
            scope: origin.scope,
            path: origin.path.clone(),
        },
        None => Decided::Default,
    };

    let value = decided.path().and_then(|path| {
        let text = std::fs::read_to_string(path).ok()?;
        crate::edit::value_at(&text, key)
    });

    Setting {
        path: key.to_string(),
        value: value.map(|v| redact(key, &v)),
        decided,
    }
}

/// The file a scope writes to, whether or not it exists yet.
///
/// [`Config::sources`] answers this for a scope whose file is already there, and
/// says nothing about one that is not — which is the case that matters, because
/// writing a key into a scope for the first time is how an operator uses this.
/// So the path is derived: the user scope from io-harness's own
/// [`io_harness::config::user_path`], and the two workspace scopes from its own
/// `PROJECT_FILE` and `LOCAL_FILE` constants, so io-cli never spells either name
/// as a literal of its own.
pub fn scope_path(root: &std::path::Path, scope: Scope) -> Option<PathBuf> {
    match scope {
        Scope::User => io_harness::config::user_path(),
        Scope::Project => Some(root.join(io_harness::config::PROJECT_FILE)),
        Scope::Local => Some(root.join(io_harness::config::LOCAL_FILE)),
    }
}

/// What to print when the configuration cannot be read at all.
///
/// **The whole of io-harness's sentence, and one line of io-cli's own.** A
/// `Config::discover` that fails is not always a broken file: since io-harness
/// refuses a project-scoped `[[hook]]` outright — a hook runs a command, and
/// `io.toml` is the file a `git clone` delivers — the commonest way for a
/// perfectly well-formed repository to stop io from starting is a table somebody
/// added in good faith to the wrong one of three files.
///
/// io-harness's own message names the key, says why, and names the two files that
/// may carry it. Nothing io-cli could write would be better, so it is passed
/// through whole. What io-cli adds is the one thing the harness cannot know: which
/// directory was being read, because an operator who ran `io` in the wrong place
/// is looking at a message about a file they have never opened.
///
/// **This lives here and not in `main.rs`.** Nothing under `tests/` links the
/// binary, so a sentence composed in that file is one no test drives and no
/// sabotage can make fail — the same reasoning that put plain-mode resolution in
/// the library.
pub fn refusal(root: &std::path::Path, error: &io_harness::Error) -> String {
    format!(
        "the configuration in {} could not be read:\n{error}",
        root.display()
    )
}

/// Write one change into the scope the operator picked, and prove it landed.
///
/// **The write is verified by io-harness reading it back, and rolled back when it
/// refuses.** That is what makes F4 work rather than being a second copy of the
/// harness's rules: `refuse_widening` fires on the *scope of the file*, which only
/// [`Config::discover`] knows — `Config::from_toml` has no path and therefore no
/// scope — so the only honest way to ask "may this file say this" is to write it
/// and re-discover. When the answer is no the original bytes go back and the
/// harness's own sentence comes out, re-worded by nobody.
///
/// A scope with no file yet gets one, created through [`crate::settings::write`]
/// so it lands `0600` like every other file this crate creates with a credential
/// in reach.
pub fn write(
    root: &std::path::Path,
    scope: Scope,
    edits: &[crate::edit::Edit],
) -> Result<(), String> {
    let path = scope_path(root, scope)
        .ok_or_else(|| "there is no path for that scope on this machine".to_string())?;

    let before = std::fs::read_to_string(&path).ok();
    if before.is_none() {
        crate::settings::write(&path, "").map_err(|e| format!("{}: {e}", path.display()))?;
    }

    crate::edit::write(&path, edits)?;

    // The round trip. Anything io-harness refuses is undone before this returns.
    match Config::discover(root) {
        Ok(_) => Ok(()),
        Err(refusal) => {
            match &before {
                Some(text) => {
                    let _ = crate::settings::write(&path, text);
                }
                // The file did not exist before this call, so the state it must
                // go back to is "absent" rather than "empty" — an empty io.toml
                // left behind would be a project file this operator never wrote.
                None => {
                    let _ = std::fs::remove_file(&path);
                }
            }
            Err(refusal.to_string())
        }
    }
}

/// Re-read everything a written change can alter.
///
/// **Both halves, and that is the whole point.** `main` builds its `Config` once
/// and derives io-cli's own `CliSettings` from it once; a reload that refreshed
/// only the `Config` would leave every `[app.io-cli]` answer — the theme, the
/// glyph set, the keys, the capabilities — as it was at session start, so the
/// surface would report a value that no turn was using. Returning the pair is
/// what stops a caller refreshing one and forgetting the other.
pub fn reload(
    root: &std::path::Path,
) -> Result<(Config, Option<crate::settings::CliSettings>), String> {
    let config = Config::discover(root).map_err(|e| e.to_string())?;
    let (stored, _) = crate::settings::stored(&config);
    Ok((config, stored))
}

/// The rows as the picker draws them: the key, then its value and its origin.
///
/// Content before metadata, which is this product's rule everywhere: the key is
/// the label and the value rides the detail column with the origin word after it.
pub fn rows(settings: &[Setting]) -> Vec<crate::picker::Row> {
    settings
        .iter()
        .map(|setting| {
            let value = setting.value.as_deref().unwrap_or("—");
            crate::picker::Row::with_detail(
                setting.path.clone(),
                format!("{value}   {}", setting.decided.word()),
            )
        })
        .collect()
}

/// The `[profile.*]` names a configuration declares.
///
/// **io-harness has no accessor for these.** [`Config::with_profile`] applies one
/// by name and reports its own sentence when the name is wrong, but nothing
/// lists them: the merged table is private, and profile keys do not appear in
/// [`Config::origins`]. So the names come from the file that declared them,
/// through the same scan the writer cuts a document with.
///
/// Sorted and deduplicated, because a profile may declare sub-tables — a file
/// with `[profile.fast]` and `[profile.fast.run]` has one profile, not two.
pub fn profiles(config: &Config) -> Vec<String> {
    let Some(text) = config
        .sources()
        .last()
        .and_then(|(_, path)| std::fs::read_to_string(path).ok())
    else {
        return Vec::new();
    };

    let mut names: Vec<String> = crate::edit::sections(&text)
        .into_iter()
        .filter_map(|path| {
            (path.first().map(String::as_str) == Some("profile"))
                .then(|| path.get(1).cloned())
                .flatten()
        })
        .collect();
    names.sort();
    names.dedup();
    names
}

/// Apply a named profile, or report io-harness's own refusal.
///
/// A thin wrapper on [`Config::with_profile`] so that io-cli holds no second
/// opinion about what a profile overlay means — the harness applies it through
/// the same merge the scopes use, which is why this is a call rather than a
/// reimplementation.
pub fn with_profile(config: &Config, name: &str) -> Result<Config, String> {
    config.with_profile(name).map_err(|error| error.to_string())
}
