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
    /// read, and the act beside it is [`REFRESH_PRICES`], one descent below the row
    /// through [`descent`]. A `Machine` key still has no value to type, which is
    /// why `value_rows` answers `None` for it and `manage::config_value` refuses it
    /// by name; the descent offers the act, never the key.
    Machine,
}

/// The `Effect` variants, spelled by io-harness itself.
///
/// **Both halves are the dependency's since io-harness 0.71.0, and neither is
/// written here any more**: the list is `Effect::ALL`
/// (`io-harness-0.73.0/src/policy.rs:127`) and each spelling is `Effect::as_str`
/// (`:143`), which is the word io-harness's own deserializer reads.
///
/// Until this release io-cli held a copy of both — an array naming three variants
/// and a `match` mapping them to three string literals — and the copy was the
/// defect. The array was a build-breaking census, so a variant *added* or
/// *removed* upstream stopped this crate compiling; but the literals were io-cli's
/// transcription of serde's `rename_all = "snake_case"`, so a *rename* upstream
/// left it compiling and writing a word the schema rejects. Taking the string from
/// the variant closes the one hole the census could not.
///
/// `Effect` is not `#[non_exhaustive]` (`policy.rs:90-92`), so `ALL` is a census
/// io-harness itself keeps complete rather than a list that can quietly fall
/// behind, and there is no wildcard on this side to swallow a fourth effect.
/// `tests/configure.rs` still round-trips every string through io-harness's own
/// deserializer, because `as_str` and `Deserialize` are two impls and only the
/// round trip proves they agree.
#[must_use]
pub fn effects() -> Vec<String> {
    io_harness::Effect::ALL
        .iter()
        .map(|effect| effect.as_str().to_string())
        .collect()
}

/// The `ExecMode` variants, spelled by io-harness itself.
///
/// **The list is `ExecMode::ALL` (`io-harness-0.73.0/src/sandbox.rs:424`) and the
/// spellings are `ExecMode::as_str` (`:431`).** io-cli wrote the variant list out
/// by hand until this release for a reason that was the dependency's and not a
/// choice made here: `ExecMode` is `#[non_exhaustive]` (`sandbox.rs:378-381`), and
/// a caller outside the defining crate cannot enumerate such an enum without a
/// wildcard arm that silently swallows the next variant. The issue this crate
/// filed asking for the enumeration is io-harness#218, and 0.71.0 answers it —
/// `ALL` is kept complete by an in-crate exhaustive `match` that stops io-harness
/// compiling when a mode is added, which is a guarantee nothing on this side could
/// have provided. `strum` was never an answer, being forbidden by io-harness's own
/// NF2 and by this crate's no-new-dependency constraint.
///
/// `ExecMode` is still `#[non_exhaustive]`, so a *match* over it here still needs
/// a wildcard — see [`exec_mode_label`], which reports an unknown mode rather than
/// dropping it. What changed is that the menu no longer depends on that: a mode
/// io-harness adds is offered because it is in `ALL`, not because io-cli noticed.
#[must_use]
pub fn exec_modes() -> Vec<String> {
    io_harness::ExecMode::ALL
        .iter()
        .map(|mode| mode.as_str().to_string())
        .collect()
}

/// How a mode reads on the surface, including one this build has never heard of.
///
/// The wildcard [`exec_modes`] names. A wildcard that omits is the defect; a
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
/// **Anchored on the value in force, and that is still the finding rather than a
/// preference — but half of the old reason is now false and the correction is
/// worth writing down.** io-harness 0.71.0 names its own defaults:
/// `DEFAULT_MAX_STEPS` = 8, `DEFAULT_WORKSPACE_MAX_STEPS` = 12 and
/// `DEFAULT_MAX_RETRIES` = 2 (`io-harness-0.73.0/src/contract.rs:652,670,686`),
/// re-exported at the crate root. "There is nothing to read" was true when this
/// was written and is not true now. What is still true is that none of it anchors
/// *this* ladder:
///
/// * `run.max_steps` — **io-cli does not run on either harness default.**
///   [`crate::contract::configured`] builds every session turn and every
///   `io exec` from `TaskContract::workspace(..)` and immediately replaces its
///   `DEFAULT_WORKSPACE_MAX_STEPS` with [`crate::contract::MAX_STEPS`]
///   (`src/contract.rs:210`), *before* the configuration is applied over it. The
///   number in force when no file names the key is a thousand, and it is io-cli's
///   own. Anchoring the picker on 12 would show a figure no turn has ever run
///   under.
/// * `run.max_tokens` and `run.max_duration_secs` — `None` in **both**
///   `TaskContract` constructors, so there is no default to show at all.
/// * The rest — the `[run]` section is a private struct with no getter, and
///   `io_harness::Defaults` is the policy tier defaults under a colliding name.
///
/// And a per-key anchor is not this function's to apply in any case: it is handed
/// a number and a sign, never a key ([`Kind::Number`] carries only `signed`), so
/// the choice would belong to the caller. So the anchor is the value the operator
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

/// A value as TOML spells it for that kind.
///
/// **The serialized value, never the label**, which is the difference F3's own
/// sabotage turns on: writing `dark` unquoted where a string belongs produces a
/// file io-harness refuses, and a check that quoted back the bytes just written
/// would pass anyway. A boolean and a number are bare; everything else is a basic
/// string; a list goes through [`crate::edit::array`], which is the one renderer
/// that already knows how.
#[must_use]
pub fn spell_value(kind: &Kind, value: &str) -> String {
    let bare = value.trim().trim_matches('"');
    match kind {
        Kind::Flag | Kind::Number { .. } => bare.to_string(),
        Kind::List => {
            let words: Vec<&str> = bare.split_whitespace().collect();
            crate::edit::array(&words)
        }
        _ => format!("\"{bare}\""),
    }
}

/// The shape a typed value must take, and a worked example of one.
///
/// **What remains typed is only what no menu can hold**, and each of those has to
/// say what it wants before it asks. A composer opened with a bare key and no
/// candidates is the state this release exists to remove; where the value genuinely
/// cannot be offered, the next best thing is a sentence naming the shape and one
/// line showing it.
///
/// `None` for a kind that is chosen rather than typed — those descend into their
/// values and never reach a composer.
#[must_use]
pub fn shape_of(key: &str, config: &Config) -> Option<String> {
    let said = match key {
        "app.io-cli.gates.command" => {
            "a command line, split on spaces into a list — for example: cargo test --all"
        }
        "app.io-cli.gates.contains" => {
            "a substring to look for in what the command printed — for example: test result: ok"
        }
        "app.io-cli.gates.rubric" => {
            "a sentence a reviewer model judges the turn against — for example: every public \
             item changed in this turn carries a doc comment"
        }
        "app.io-cli.prices.source_url" => {
            "a URL returning a model catalogue — for example: https://openrouter.ai/api/v1/models"
        }
        "prices.as_of" => {
            "written by the price refresh rather than typed; choose it on `/config` and the \
             refresh that re-reads the catalogue is the row after `leave it`"
        }
        // A key the catalogue does not name. Its shape is the operator's own
        // business — io-cli has no schema for it and inventing one here would be
        // this module claiming to know a key it does not.
        _ => return None,
    };
    let setting = setting(config, key);
    Some(match setting.value {
        Some(value) => format!("{key} is {value} ({}); {said}", setting.decided.word()),
        None => format!("{key} is not set; {said}"),
    })
}

/// The models `[prices.models]` names, across every scope, sorted and deduplicated.
///
/// **Read from the dependency's own table since io-harness 0.71.0, not scraped
/// out of the files.** `PriceTable::models` (`io-harness-0.73.0/src/pricing.rs:268`)
/// lists every model the table can actually price, and [`Config::prices`] has
/// always built that table out of the three scopes — so the merged question this
/// used to hand-roll is precisely the one the accessor answers, and the gap filed
/// upstream as io-harness#220 has landed. The models come back sorted and unique
/// because the table keys them in a `BTreeMap`.
///
/// **The scrape was also wrong, and replacing it fixes a menu an operator could
/// not see past.** It matched a literal `[prices.models]` header, so a file
/// spelling its table as a sub-table per model — `[prices.models."gpt-4.1"]`,
/// which is [`crate::prices::Shape::SubTables`], legal TOML io-harness reads
/// perfectly well, and the shape an operator writing rates by hand is most likely
/// to reach for — listed *no* models at all, and the picker for every
/// [`Kind::Model`] key came up empty on a perfectly good price table.
///
/// **A model priced by tiers alone is not offered**, which is `models`'s own
/// contract rather than a rule invented here: `PriceTier`s are keyed separately
/// from base prices and `cost_micros` answers `None` for a model that has only
/// tiers, so listing it would be promising a cost the table cannot produce.
///
/// **No network call, ever.** A settings screen that reached for a catalogue would
/// be spending an operator's money to draw a menu; where no priced section exists
/// the caller says so and offers the refresh row that already acts.
///
/// **This takes the `Config` the caller already holds, and must never re-discover
/// one.** `Config::discover` resolves every `${env:}`, `${file:}` and `${cmd:}` as
/// it reads (`io-harness-0.73.0/src/config.rs:517`), so a second discovery re-runs
/// an operator's credential commands — which for a `${cmd:}` fetching a key out of
/// a keychain means a Touch-ID prompt raised in order to draw a menu, every time
/// the picker opens. Taking a `&Config` is not an optimisation; it is the
/// difference between reading a value and executing somebody's program.
///
/// A configuration with no priced section lists nothing, and the caller says so and
/// offers the refresh row that already acts.
#[must_use]
pub fn priced_models(config: &Config) -> Vec<String> {
    let Some(prices) = config.prices() else {
        return Vec::new();
    };
    prices.models().into_iter().map(str::to_string).collect()
}

/// Which file a change to `key` should be written into, and whether that was
/// inherited from the file already deciding it.
///
/// **A write goes where the key already lives.** Asking every time costs more than
/// the change did — the value was chosen in one keystroke — and answering "the
/// user scope" every time is worse than asking: it silently shadows a committed
/// project setting with a personal one, which is the change an operator is least
/// able to see afterwards. A key no file names has nothing to inherit and goes to
/// the operator's own file.
///
/// The boolean is what lets the confirmation say *which* file and *why*, so the
/// scope is stated either way and never assumed silently. An explicit choice
/// overrides it and moves the key between files.
#[must_use]
pub fn destination(config: &Config, key: &str) -> (Scope, bool) {
    match setting(config, key).decided {
        Decided::File { scope, .. } => (scope, true),
        Decided::Default => (Scope::User, false),
    }
}

/// Whether writing `value` to `key` would be refused in a **project-scoped** file.
///
/// io-harness refuses five (key, value) pairs in a committed `io.toml`
/// (`PROJECT_WIDENING`, `io-harness-0.73.0/src/config.rs:1998-2008`): the two acts
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
/// **It is not a row of the bare list, since 0.33.0.** Through 0.32.0 it was
/// appended after the settings, and the bare `/config` therefore *was* a write and
/// a reassignment of the running configuration — which is the whole of why the
/// command was refused mid-turn (`crate::commands::MID_TURN`, and
/// `US-IO-CLI-0.32.0-I11` with it). Moving it one descent below `prices.as_of`,
/// where [`descent`] hands it out, is what makes the bare list a list of facts and
/// nothing else; the act is still one keystroke further on, beside the very date it
/// writes.
///
/// It lives here rather than in the driver so a test can reach it: nothing under
/// `tests/` can link `src/main.rs`, and a row spelled in the driver is a row no
/// test can assert on.
pub const REFRESH_PRICES: &str = "!refresh-prices";

/// The key the decline row of a `/config` descent carries.
///
/// **A sentinel and not the label, for exactly the reason [`REFRESH_PRICES`] is
/// one.** The rows of [`descent`] travel in a `Vec<String>` the driver matches on,
/// and until 0.33.0 the decline row's entry in it was the bare string
/// [`crate::store::LEAVE_IT`] — `leave it`, with no `!` in front of it. TOML
/// accepts `"leave it" = true` as a quoted key, [`settings`] sweeps every key out
/// of `Config::origins()` onto the bare `/config` list, and the driver matches
/// these keys by value: an operator with that key in a file would have got a real
/// row whose Enter hit the do-nothing arm and reported nothing at all. `!` is the
/// one character no key in this product's catalogue starts with, and it is why the
/// act beside this row is spelled the way it is.
///
/// The *label* stays [`crate::store::LEAVE_IT`] — that is the word the operator
/// reads, and every confirmation in this product opens on it. What changes is only
/// the key underneath it, which nobody reads.
pub const DECLINE: &str = "!leave-it";

/// The label that sentinel wears on the picker.
pub fn refresh_row(setting: &Setting) -> crate::picker::Row {
    let detail = match &setting.value {
        Some(as_of) => format!("last read {as_of}"),
        None => "no prices are configured".to_string(),
    };
    crate::picker::Row::with_detail("prices: re-read the catalogue", detail)
}

/// The acts one key descends into, as `(title, rows, keys)`, or `None` where the
/// key descends into values alone.
///
/// **The parallel `keys` vector is what the caller decides on, never the row's
/// position or its label** — the same shape `/gates` already uses for
/// [`crate::app::PROPOSED_GATE`]: a sentinel sits in the list where a real key
/// would, so one `match` covers both kinds of row. Row 0 is labelled
/// [`crate::store::LEAVE_IT`] and declines, which is this product's rule for every
/// confirmation, and `crate::store::acts` is what reads that position. Its *key* is
/// [`DECLINE`] rather than that label, because a key a file could also name is a
/// key two different rows answer to.
///
/// **Exactly one key answers `Some`, and it is named rather than derived from its
/// [`Kind`].** `prices.as_of` is `Kind::Machine` and so is offered no value to
/// type — but "machine-written" is not "has an act": a second `Machine` key added
/// later would inherit a price refresh that has nothing to do with it. The act
/// belongs to the price table, so it is the price table's key that opens it, and
/// every other key — machine-written or not — descends exactly where it did.
///
/// This offers no way to *type* `prices.as_of`. `manage::config_value` still
/// refuses that key by name and this changes nothing about it: a date typed by hand
/// is a claim about a fetch that never happened. What the descent offers is the
/// fetch.
#[must_use]
pub fn descent(
    config: &Config,
    key: &str,
) -> Option<(String, Vec<crate::picker::Row>, Vec<String>)> {
    if key != "prices.as_of" {
        return None;
    }
    let setting = setting(config, key);
    let title = match &setting.value {
        Some(as_of) => format!(
            "{key} is {as_of} ({}), and is written by the refresh rather than typed",
            setting.decided.word()
        ),
        None => format!("{key} is not set, and is written by the refresh rather than typed"),
    };
    let rows = vec![
        crate::picker::Row::with_detail(
            crate::store::LEAVE_IT,
            "nothing is read and nothing is written",
        ),
        refresh_row(&setting),
    ];
    // **The label is `store::LEAVE_IT`; the key beside it is [`DECLINE`].** A row's
    // label is what the operator reads and its key is what the driver matches on,
    // and only the second one has to be a sentinel — see `DECLINE` for the
    // collision that made it one.
    let keys = vec![DECLINE.to_string(), REFRESH_PRICES.to_string()];
    Some((title, rows, keys))
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
/// (`substitute`, `io-harness-0.73.0/src/config.rs:2150`, the `cmd` arm at
/// `:2241`); this comment claimed two until
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

/// What one setting is, and which file decided it, as one sentence.
///
/// **One speller for two doors.** `/config <key>` in the idle loop and a row
/// chosen on the bare `/config` list while a turn is in flight answer the same
/// question, and since 0.33.0 both can be reached in one session. Two `format!`
/// calls agreeing today is the shape this product has repeatedly found disagreeing
/// later — most recently a guided browser that built a command string by hand and
/// was a second implementation of the parse.
///
/// A key no file names reads `not set` rather than an empty value, because an
/// empty string is a value an operator can actually write.
#[must_use]
pub fn said(setting: &Setting) -> String {
    let what = setting.value.as_deref().unwrap_or("not set");
    format!("{} is {what} ({})", setting.path, setting.decided.word())
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
///
/// **Every source, not just the last one.** Until 0.30.0 this read
/// `sources().last()` alone, so a profile declared in a lower-precedence scope was
/// invisible to the one surface whose whole job is to list them — and
/// [`with_profile`] would have applied it perfectly well, because io-harness
/// merges the overlay across the scopes it merged the file from. A list that
/// cannot see what the switch can reach is a list that is wrong.
pub fn profiles(config: &Config) -> Vec<String> {
    let mut names: Vec<String> = Vec::new();
    for (_, path) in config.sources() {
        let Ok(text) = std::fs::read_to_string(path) else {
            continue;
        };
        names.extend(declared_profiles(&text));
    }
    names.sort();
    names.dedup();
    names
}

/// The `[profile.<name>]` names one file's own text declares.
///
/// Split out because [`remove_profile`] needs the *sections*, not the names: a
/// profile is however many headers share its first two segments, and removing it
/// means removing all of them.
fn declared_profiles(text: &str) -> Vec<String> {
    crate::edit::sections(text)
        .into_iter()
        .filter_map(|path| {
            (path.first().map(String::as_str) == Some("profile"))
                .then(|| path.get(1).cloned())
                .flatten()
        })
        .collect()
}

/// The edit that creates `[profile.<name>]`, or the reason it cannot.
///
/// **[`crate::edit::Edit::section`] and deliberately not
/// [`crate::edit::Edit::set`].** `section` refuses a section that already exists,
/// which is exactly the "that name is taken" answer this verb owes an operator —
/// so the refusal is the write primitive's own and there is no second opinion
/// about what "already there" means. `set` would have appended a second
/// `[profile.<name>]` header to the file, which is the shape `src/edit.rs:120-126`
/// records as the reason `section` exists at all.
///
/// The body is a comment rather than nothing, because a bare header an operator
/// then cannot find in their own file is a worse first experience than a line
/// saying what it is for. `/config --scope` writes the keys.
pub fn create_profile(name: &str) -> Result<crate::edit::Edit, String> {
    let trimmed = name.trim();
    if trimmed.is_empty() {
        return Err("a profile needs a name".to_string());
    }
    // `spell` quotes a segment that is not a bare key, so a name with a dot or a
    // space addresses one profile rather than silently nesting two tables.
    let path = format!("profile.{}", crate::edit::spell(trimmed));
    Ok(crate::edit::Edit::section(
        path,
        format!("# `{trimmed}`, applied with `/profile` or `--profile {trimmed}`\n"),
    ))
}

/// The edits that remove `[profile.<name>]` **and every sub-table under it**.
///
/// **A profile is not one section.** `[profile.fast]` and `[profile.fast.run]` are
/// two headers and one profile — which [`profiles`] already knew, because it
/// deduplicates on exactly that. So the removal is one
/// [`crate::edit::Edit::remove`] per header, applied together:
/// [`crate::edit::apply`] splices in reverse start order and is all-or-nothing, so
/// either the whole profile goes or the file is untouched.
///
/// **`remove` and never [`crate::edit::Edit::unset`]**, and getting that backwards
/// is the confusion 0.28.0 paid for: `remove` takes a whole `[section]` region,
/// header and body; `unset` deletes a single `key = value` line and errors on a
/// section path. This verb wants the region.
///
/// `Err` when the file declares no such profile, so the surface can say which
/// names it does have rather than reporting a successful write that removed
/// nothing.
pub fn remove_profile(text: &str, name: &str) -> Result<Vec<crate::edit::Edit>, String> {
    let headers: Vec<Vec<String>> = crate::edit::sections(text)
        .into_iter()
        .filter(|path| {
            path.first().map(String::as_str) == Some("profile")
                && path.get(1).map(String::as_str) == Some(name)
        })
        .collect();
    if headers.is_empty() {
        return Err(format!(
            "this file declares no `[profile.{name}]`; it declares {}",
            match declared_profiles(text).as_slice() {
                [] => "no profiles at all".to_string(),
                found => {
                    let mut names = found.to_vec();
                    names.sort();
                    names.dedup();
                    names
                        .iter()
                        .map(|found| format!("`{found}`"))
                        .collect::<Vec<_>>()
                        .join(", ")
                }
            }
        ));
    }
    Ok(headers
        .into_iter()
        .map(|path| {
            crate::edit::Edit::remove(
                path.iter()
                    .map(|segment| crate::edit::spell(segment))
                    .collect::<Vec<_>>()
                    .join("."),
            )
        })
        .collect())
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
