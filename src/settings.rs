//! io-cli's own settings, and writing io-harness's configuration file.
//!
//! **There is no configuration parser in this repository.** io-harness owns
//! discovery, layering and validation; this module hands it a `ProviderSpec` and
//! a `Defaults` — types the harness declares and already derives `Serialize` for —
//! and serializes them. Reading comes back through `Config::discover` and
//! `Config::app`, which is the section the harness deliberately does not validate
//! because it belongs to whoever is building on top of it.

use std::io;
use std::path::{Path, PathBuf};

use io_harness::{Defaults, Effect, ProviderSpec};
use serde::{Deserialize, Serialize};

/// The key io-cli's own section sits under: `[app.io-cli]`.
pub const APP_KEY: &str = "io-cli";

/// Everything io-cli itself remembers.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct CliSettings {
    /// The theme by name. Absent means "detect from the terminal background".
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub theme: Option<String>,
    /// How much of a change to show: `unified` or `minimal`.
    ///
    /// Absent means `unified`, which is what every configuration file written
    /// before 0.3.0 means — so this key needs no migration and an older binary
    /// reading a file that has it ignores it, because `[app.io-cli]` is the one
    /// section io-harness deliberately does not validate.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub diff: Option<String>,
    /// Which glyph set to draw with: `unicode` or `ascii`.
    ///
    /// Absent means "ask the locale", which is what every file written before
    /// 0.6.0 means. It is a separate key from the theme and from `plain` on
    /// purpose: a terminal that cannot draw `›` may still be perfectly happy
    /// with colour, and a reader who wants the animation stilled may be reading
    /// on a terminal that draws every glyph in the set.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub glyphs: Option<String>,
    /// Whether to run in plain mode without being asked each time.
    ///
    /// The same switch as `--plain`, and the flag wins when both are present —
    /// a flag is this run and a file is every run.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub plain: Option<bool>,
    /// The session's keys, by action name: `[app.io-cli.keys]`.
    ///
    /// A map rather than a struct of named fields on purpose. A struct would
    /// make an action nobody has heard of a *deserialization* failure, which
    /// would take the whole section down — theme, diff style, glyphs and plain
    /// mode with it — over a misspelt keybinding. A map lets
    /// [`crate::keys::Keys::resolve`] answer for each line on its own and say
    /// which names it does know, which is the difference between a typo that
    /// costs one key and a typo that costs every setting in the file.
    ///
    /// `BTreeMap` rather than `HashMap` so the notices a bad file produces come
    /// out in the same order every time; a diagnostic that shuffles is one
    /// nobody can compare against the last run.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub keys: Option<std::collections::BTreeMap<String, String>>,
    /// The caps a fan-out runs under: `[app.io-cli.containment]`.
    ///
    /// **This key is what turns the fleet on, and it is not a preference.**
    /// `Session::turn_contained_bounded_steered` is the only session entry point
    /// that passes a containment into the driver, and therefore the only one that
    /// reaches the loop owning the spawn tool — so a session with no caps
    /// configured cannot decompose anything, and one with them runs a materially
    /// different turn.
    ///
    /// **And since 0.12.0 the fan-out is all it decides.** 0.10.0 made this key
    /// carry the four settings below it too, because the contained entry point was
    /// then the only one taking a caller's `TaskContract`. 0.11.0 gave the flat
    /// turn a contract as well, so those four reach every turn now; 0.12.0 moved
    /// the responder to every turn and the plan gate to `/plan`. Configuring caps
    /// buys a fan-out and nothing else. See [`crate::contract`].
    ///
    /// io-harness's own type rather than four fields of io-cli's own, because it
    /// is `Serialize`/`Deserialize` for exactly this purpose and because a
    /// second spelling of the caps would be a second thing to keep true. It
    /// carries the crate's own `#[serde(alias = "max_concurrent")]`, so a file
    /// written against the pre-0.32.0 name still reads.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub containment: Option<io_harness::Containment>,
    /// MCP servers for the turn: `[[app.io-cli.mcp]]`.
    ///
    /// io-harness's own `McpServer`, which is `Deserialize` for exactly this
    /// purpose. **It reaches a turn wherever a contract does**, which since
    /// 0.11.0 is every turn: the interactive session and `io exec` both build one
    /// from the same configuration, so a server declared here is attached to the
    /// contained turn and the flat one alike — see [`crate::contract`] and
    /// [`crate::exec`]'s module note, which says the same of either arm.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub mcp: Option<Vec<io_harness::McpServer>>,
    /// Language servers for this workspace: `[[app.io-cli.lsp]]`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub lsp: Option<Vec<io_harness::LspServer>>,
    /// A browser the agent may drive: `[app.io-cli.browser]`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub browser: Option<io_harness::BrowserConfig>,
    /// The directory io-harness discovers skills in: `skills = "..."`.
    ///
    /// A path and not a list, because discovery is the harness's and io-cli
    /// parses no skill file of its own.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub skills: Option<std::path::PathBuf>,

    /// `max_parallel_reads` — how many read-only tool calls may run at once.
    ///
    /// A `TaskContract` field with **no io-harness configuration key at all**:
    /// `RunSection` carries thirteen and this is not one of them, so a file has
    /// never been able to say it. io-harness's own default is 10, and 0 is
    /// clamped to 1 by the builder rather than meaning "none".
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub max_parallel_reads: Option<usize>,

    /// `spawn_background_after_secs` — when a slow child is backgrounded.
    ///
    /// Absent is io-harness's own default, which is to wait however long the
    /// child takes. Spelled with the `_secs` suffix the harness uses everywhere
    /// a duration is a number in this file.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub spawn_background_after_secs: Option<u64>,

    /// `detached_spawns` — whether a spawn may detach at all.
    ///
    /// Default true. `false` is the embedder asking for a trace with every
    /// child's whole life in it, which is what a detached child gives up.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub detached_spawns: Option<bool>,

    /// What io-cli remembers about prices: `[app.io-cli.prices]`.
    ///
    /// **The table itself is not here, and that is not a filing decision.** The
    /// prices live under `[prices]`, which io-harness owns and reads through
    /// `Config::prices`. That section is `deny_unknown_fields` and carries exactly
    /// `as_of` and `models`, so a key of io-cli's own put beside them would not be
    /// ignored — it would make the operator's whole configuration file fail to
    /// parse, taking the policy, the providers and the run budgets down with it.
    /// `[app.io-cli]` is the one section io-harness deliberately does not
    /// validate, so anything io-cli needs to remember that the harness does not
    /// model belongs here and nowhere else.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub prices: Option<PriceSettings>,

    /// What "done" means for this repository: `[app.io-cli.gates]`.
    ///
    /// **The section as written, and deliberately not the criterion that runs.**
    /// io-harness's `TaskContract` carries one `Verification`, and a section can
    /// say things a `Verification` cannot be built from — no criterion at all, two
    /// kinds at once, a rubric with nobody to answer it, a reviewer that is the
    /// model doing the work. Deserializing straight into the harness's own type
    /// would put that check nowhere, so this field is io-cli's own
    /// [`crate::gates::Settings`] and [`crate::gates::Criterion`] is what survives
    /// being checked. The difference is where the operator hears about a mistake:
    /// io-harness answers the last two with a hard configuration error at run
    /// start, which turns editing a file into a session that will not start.
    ///
    /// It is also **not a preference and not a ceiling**. The three ceilings above
    /// bound a turn that would have run anyway; this decides whether a turn that
    /// finished is finished, and a review criterion spends a real completion on
    /// every gated turn. So an absent section is no gate rather than a default
    /// one — nothing is verified for an operator who never said what verifying
    /// would mean.
    ///
    /// A table rather than an array because the contract's verification is one
    /// value, not a suite; if the dependency ever grows a list, a list here is an
    /// addition rather than a break.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub gates: Option<crate::gates::Settings>,
    /// Whether a question that is only a question may be answered without opening
    /// a run.
    ///
    /// **The behaviour is io-harness's and it has been on all along.**
    /// `session.rs:1125-1127` reads
    /// `contract.conversational.unwrap_or(matches!(contract.verify,
    /// Verification::None))`, and `TaskContract::workspace` starts at
    /// `Verification::None` — so every ungated operator has had greetings answered
    /// in one completion, with no steps row, no gate attempt and no snapshot, since
    /// before this interface existed. 0.24.0 turned it on for gated operators too,
    /// because attaching a criterion would otherwise have switched it off.
    ///
    /// What has never existed is a way to say *no*. This key is that, and it is
    /// `false`-only in spirit: `true` is what an unset file already does in every
    /// case io-cli produces. An operator who wants every prompt to open a run —
    /// because a run is what their hooks, their gate or their trace expect — has
    /// had no way to ask for one.
    ///
    /// Absent leaves the decision exactly where it was, which is what
    /// `tests/contract.rs`'s field-for-field gate requires.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub conversational: Option<bool>,
    /// `[app.io-cli.routing]` — when a run should change models, and to which.
    ///
    /// io-cli's own type rather than `io_harness::Routing`, which is the one place
    /// this section differs from `[app.io-cli.containment]`: `Containment`
    /// deserializes straight into the harness's type, and `Routing` derives no
    /// serde at all and is `#[non_exhaustive]` besides (`contract.rs:1954-1956`). So the
    /// shape an operator writes is [`crate::routing::Settings`] and the conversion
    /// goes through the harness's three builders.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub routing: Option<crate::routing::Settings>,
}

/// Where prices come from, and what the last read was.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct PriceSettings {
    /// The catalogue to read: `source_url = "..."`.
    ///
    /// Absent means io-harness's own default. This key is the only way an
    /// operator on a self-hosted or `compatible` endpoint gets prices at all: the
    /// reference catalogue cannot speak for a server it has never heard of, and
    /// `Reference::at` takes any URL serving the shape.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub source_url: Option<String>,
    /// What the last read was, in words, for the surfaces that draw money.
    ///
    /// Written by a fetch rather than by hand. It records whether the rates came
    /// from the provider speaking for itself or from a third party's catalogue,
    /// which for two of the three vendors io-cli can connect to is the second —
    /// OpenAI and Anthropic publish no prices on any endpoint. A page that drew a
    /// figure without saying which would be attributing a number to a vendor that
    /// never published one.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub source: Option<String>,
    /// How many models the last read priced.
    ///
    /// **Bookkeeping io-harness does not model, which is what this section is
    /// for.** The count is needed twice: to say on `/cost` how many models the
    /// rates cover, and to refuse a refetch that comes back far shorter than what
    /// it would replace, which is the one failure in this area that loses money
    /// quietly.
    ///
    /// **io-harness 0.71.0 gave `PriceTable` `models()`, `len()` and `is_empty()`
    /// (io-harness#220), and this field survives them.** Those answer what the
    /// merged table holds *now*; this records what one particular read priced *at
    /// the time it was written*, which is the only thing a later refetch can be
    /// compared against. A live count cannot play that role — by the time the
    /// comparison is made it is a count of the very table being replaced.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub models: Option<usize>,
}

/// The caps this session runs its turns under, if any.
///
/// A function rather than a field read at the call site so that the decision has
/// somewhere a test can reach: `src/main.rs` cannot be linked by anything under
/// `tests/`, which is the same reason [`plain`] lives here.
pub fn containment(stored: Option<&CliSettings>) -> Option<&io_harness::Containment> {
    stored.and_then(|settings| settings.containment.as_ref())
}

/// What a contained turn decides, in the words the session says it in.
///
/// **Disclosure rather than decoration**, and through 0.11.0 the disclosure was
/// wrong. It offered a responder, a plan gate, MCP servers, language servers, a
/// browser and skills as things this mode grants, and named a lost mid-turn steer
/// as the price. Both stopped being true: 0.11.0 gave the flat turn a contract
/// too, so every one of those capabilities is on both turns, and since 0.17.0
/// both turns take a `SteerInbox` as well — a contained turn can be steered, so
/// there is no price to name. `Ctrl+C` is the observer's cancel on both, which
/// is the half of that old sentence that was right all along.
///
/// What is left is one difference, and it is the one the caps are for:
/// `turn_contained_bounded_steered` is the only session entry point that reaches
/// io-harness's spawn loop, so this is the only turn that can fan out. A notice
/// that sold the mode on anything else was talking an operator into a fan-out to
/// get capabilities their session already had.
pub fn contained_notice(caps: &io_harness::Containment, dash: &str) -> String {
    format!(
        "contained {dash} up to {} agents, {} at once per tier, {} deep, {} tokens for the \
         tree. That is the whole of what this mode changes: it is the only turn that can fan \
         out. Skills, mcp, lsp, browser and answering a question are the same on every turn, \
         and Ctrl+C ends either one.",
        caps.max_total_agents, caps.max_concurrent_agents, caps.max_depth, caps.max_total_tokens,
    )
}

/// io-cli's own section, and what was wrong with it.
///
/// **This is F10, and it exists because `.unwrap_or_default()` on the `Result`
/// was the whole of the old behaviour.** io-harness answers `Config::app` with
/// three distinct outcomes — the section is there and parsed, the section is not
/// there at all, or the section is there and could not be read — and collapsing
/// the third into the second meant that one mistyped value silently reverted the
/// theme, the diff style, the glyph set, plain mode and every keybinding at
/// once, with nothing said about any of it. A setting that quietly goes back to
/// its default is worse than one that fails loudly: the operator sees a session
/// that looks almost right and has no thread to pull.
///
/// The notice carries **the harness's own message**, which already names the
/// section and the key that broke — rewording it here would drop the only part
/// that says where to look.
///
/// It lives in the library rather than at the two call sites in `src/main.rs`
/// because nothing under `tests/` can link the binary: a decision written there
/// is one no test drives and no sabotage can make fail.
pub fn stored(config: &io_harness::Config) -> (Option<CliSettings>, Option<String>) {
    match config.app(APP_KEY) {
        Ok(stored) => (stored, None),
        Err(error) => (
            None,
            Some(format!(
                "{error}; this session is running on the default settings until that is fixed"
            )),
        ),
    }
}

/// The one line a file still carrying `[app.io-cli] max_steps` earns.
///
/// **The key was removed in 0.16.0 and the notice was not, and that pairing is
/// the whole point.** `CliSettings` carries no `#[serde(deny_unknown_fields)]`,
/// so deleting the field alone would make a leftover key *silently ignored*: no
/// error anywhere, and an operator's step cap quietly back to
/// [`crate::contract::MAX_STEPS`] or to whatever `[run] max_steps` says. A
/// removal nobody is told about is indistinguishable from a bug.
///
/// So this reads the **raw** section rather than the typed struct — the field it
/// used to read does not exist any more — through `Config::app`, with a shape
/// that names only the dead key and ignores everything else in the table.
pub fn deprecated_max_steps(config: &io_harness::Config) -> Option<String> {
    /// Only the key that is gone. No `deny_unknown_fields`: every other
    /// `[app.io-cli]` key is live and none of them is this function's business.
    #[derive(Deserialize)]
    struct Removed {
        #[serde(default)]
        max_steps: Option<u32>,
    }

    let steps = config.app::<Removed>(APP_KEY).ok()??.max_steps?;
    Some(format!(
        "`[app.io-cli] max_steps` was removed in 0.16.0 and is no longer read, so the {steps} \
         steps it asks for are not in force. Use `[run] max_steps` instead, which bounds a \
         session turn and an `io exec` run alike."
    ))
}

/// The refusal a headless run earns when `[app.io-cli]` will not parse **and it
/// names a gate**.
///
/// **A gate that cannot be read is a gate that is not in force, and headless is
/// the one surface with nobody to notice (0.38.1).** `[app.io-cli]` has no
/// `deny_unknown_fields`, but it is still one section: a single mistyped value
/// anywhere in it — `max_total_agents = "4"`, which the 2026-09-05 field test
/// produced with `io config set` — fails the whole `Config::app` call, and
/// [`stored`] then answers `None` for every key including the gate. A session
/// prints the notice to a person who is looking at it. `io exec` did not call
/// [`stored`] at all, so the run proceeded with no criterion, no gate row and
/// exit `0`: a verification the operator configured, silently not applied, on the
/// surface whose whole purpose is to be trusted unattended.
///
/// So the raw section is read for a gate key, by exactly the shape
/// [`deprecated_max_steps`] uses and for the same reason — the typed struct is
/// unavailable precisely when this question needs answering.
///
/// **It refuses rather than warns**, and only here. A warning is the behaviour
/// that produced the finding; and the same broken section in a session is a
/// person reading a line, which is why this is `io exec`'s alone.
pub fn ungated_by_a_broken_section(config: &io_harness::Config) -> Option<String> {
    /// The gate table alone, and deliberately never constructed.
    ///
    /// `IgnoredAny` accepts any shape and keeps none of it, so this parse
    /// succeeds on exactly the values that fail `CliSettings` — which is the
    /// whole point: it answers "was a gate asked for", never "is the gate
    /// valid". It is also the reason this file still names no configuration
    /// type and parses no TOML, which `tests/dependencies.rs` holds it to;
    /// `src/import.rs` reads a credential-shaped field the same way and for the
    /// same reason.
    #[derive(Deserialize)]
    struct Gated {
        #[serde(default)]
        gates: Option<serde::de::IgnoredAny>,
    }

    // Nothing to say when the section parses: `stored` returns the real settings
    // and the gate is in force or genuinely absent.
    if config.app::<CliSettings>(APP_KEY).is_ok() {
        return None;
    }
    config.app::<Gated>(APP_KEY).ok()??.gates?;
    Some(
        "`[app.io-cli]` names a gate but the section could not be read, so no criterion is in \
         force and this run would report success without verifying anything. `io config get \
         app.io-cli` shows the section; a value written as a string where a number is expected \
         is the usual cause. Nothing was run."
            .to_string(),
    )
}

/// Whether this session runs in plain mode: `--plain`, or `[app.io-cli] plain`.
///
/// **A pure function, and it lives here rather than in `src/main.rs` on purpose.**
/// The binary has no automated coverage by construction — an integration test
/// cannot link it — so a decision written inline there is one no test can drive
/// and no sabotage can be made to fail. Two earlier releases had to move a
/// decision out of `main.rs` for exactly that reason, and this is the third.
///
/// **The flag wins over the file**, because a flag is this run and a file is
/// every run. That has teeth in one direction only, and saying so is more honest
/// than implying a precedence there is no way to exercise: there is no
/// `--no-plain`, so a file that says `plain = true` cannot be turned off from the
/// command line for one session. The asymmetry is the right way round —
/// accessibility is a thing somebody switched on deliberately, and a mode that
/// can be lost to a stray flag is not one you can rely on.
///
/// An absent key is `false`, which is what every configuration file written
/// before 0.6.0 means. `Some(false)` and `None` therefore answer the same, and
/// the distinction is kept in the type only so that a file can state the default
/// without the wizard ever writing it — plain mode is asked for, never inferred.
pub fn plain(flag: bool, stored: Option<&CliSettings>) -> bool {
    flag || stored.is_some_and(|settings| settings.plain.unwrap_or(false))
}

/// How much of a change a diff shows.
///
/// Two, not a number of context lines. The counter-pressure this answers is
/// approval fatigue: someone reviewing by file rather than by hunk wants the
/// changed lines and nothing else, and a dial from 0 to 3 is a dial nobody sets.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum DiffStyle {
    /// The hunk as the harness stored it, context and all.
    #[default]
    Unified,
    /// Changed lines only, with the `@@` header kept so the change still says
    /// where in the file it is.
    Minimal,
}

impl DiffStyle {
    /// What a configured value means. An unrecognised one is `Unified` rather
    /// than an error: `[app.io-cli]` is unvalidated by design, and refusing to
    /// start a session over a typo in a cosmetic key would be the wrong trade.
    pub fn from_setting(value: Option<&str>) -> Self {
        match value {
            Some("minimal") => Self::Minimal,
            _ => Self::Unified,
        }
    }
}

/// A default permission posture, in the words the wizard offers it in.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Posture {
    /// Read, write and run inside the workspace; no outbound network.
    Workspace,
    /// Read freely; writes and commands ask first.
    AskWrites,
    /// Read only.
    ReadOnly,
}

impl Posture {
    pub const ALL: &'static [Posture] =
        &[Posture::Workspace, Posture::AskWrites, Posture::ReadOnly];

    pub fn label(self) -> &'static str {
        match self {
            Self::Workspace => "Sandboxed workspace",
            Self::AskWrites => "Ask before writes",
            Self::ReadOnly => "Read only",
        }
    }

    /// The short name the status line uses. Hyphenated rather than spaced, so the
    /// field is one token a reader's eye can skip over or stop on.
    pub fn short(self) -> &'static str {
        match self {
            Self::Workspace => "workspace",
            Self::AskWrites => "ask-writes",
            Self::ReadOnly => "read-only",
        }
    }

    /// The next posture in the cycle. It wraps, because one key that only ever
    /// moves one way is a key you press three times to undo.
    pub fn next(self) -> Self {
        match self {
            Self::Workspace => Self::AskWrites,
            Self::AskWrites => Self::ReadOnly,
            Self::ReadOnly => Self::Workspace,
        }
    }

    /// Which posture a set of defaults *is*, if it is one of them.
    ///
    /// `None` for a configuration file holding a policy nobody offered, which is
    /// allowed — io-harness's own file can express far more than three postures.
    /// Reporting such a policy as one of the three would put a true-looking word
    /// beside a boundary it does not describe.
    pub fn of(defaults: &Defaults) -> Option<Self> {
        Self::ALL
            .iter()
            .copied()
            .find(|posture| &posture.defaults() == defaults)
    }

    pub fn detail(self) -> &'static str {
        match self {
            Self::Workspace => "read, write and run inside this repository; no outbound network",
            // True as of 0.2.0. Through 0.1.0 and 0.1.1 this line had to say that
            // a write was *declined* rather than asked about, because the approver
            // handed to the harness was `DenyAll` — a posture whose behaviour is
            // not what its name suggests has to say so at the moment it is chosen.
            Self::AskWrites => "read freely; a write or a command stops and asks you first",
            Self::ReadOnly => "read only; nothing is written and nothing is run",
        }
    }

    /// The policy defaults this posture is.
    ///
    /// A posture is an `io_harness::Policy`, not a flag of io-cli's own. That is
    /// what makes the status line able to name the layer in force, and what will
    /// make a refusal able to name the rule that produced it.
    pub fn defaults(self) -> Defaults {
        match self {
            Self::Workspace => Defaults {
                read: Effect::Allow,
                write: Effect::Allow,
                exec: Effect::Allow,
                net: Effect::Deny,
            },
            Self::AskWrites => Defaults {
                read: Effect::Allow,
                write: Effect::Ask,
                exec: Effect::Ask,
                net: Effect::Deny,
            },
            Self::ReadOnly => Defaults {
                read: Effect::Allow,
                write: Effect::Deny,
                exec: Effect::Deny,
                net: Effect::Deny,
            },
        }
    }
}

/// The whole file, as it will be written.
#[derive(Debug, Serialize)]
struct File<'a> {
    provider: Vec<&'a ProviderSpec>,
    policy: PolicySection,
    app: AppSection,
}

#[derive(Debug, Serialize)]
struct PolicySection {
    defaults: Defaults,
}

#[derive(Debug, Serialize)]
struct AppSection {
    #[serde(rename = "io-cli")]
    io_cli: CliSettings,
}

/// Render the configuration file's text.
///
/// Separate from writing it so the confirmation screen can show exactly what is
/// about to land, and so a test can read it without a filesystem.
pub fn render(
    spec: &ProviderSpec,
    posture: Posture,
    theme: &str,
) -> Result<String, toml::ser::Error> {
    let file = File {
        provider: vec![spec],
        policy: PolicySection {
            defaults: posture.defaults(),
        },
        app: AppSection {
            io_cli: CliSettings {
                theme: Some(theme.to_string()),
                // Left out of the file the wizard writes. Its absence is
                // `unified`, and a key written with its own default is a key a
                // reader has to wonder about — and one that would have to be
                // rewritten if the default ever changed.
                diff: None,
                // Left out for the same reason, and with more force. The glyph
                // set the wizard ran under was chosen from the locale of the
                // machine it ran on; writing it down would freeze that answer
                // into a file that may later be read on another terminal, and
                // turn a detected default into a stated preference nobody
                // stated. Plain mode likewise: it is asked for, never inferred.
                glyphs: None,
                plain: None,
                // Left out for the strongest reason of the four: writing the
                // defaults down would make every later change to a default a
                // change that only reaches new installations, and would put a
                // table of five bindings in a file the wizard's user never
                // asked to edit. The keys are documented; they are not written.
                keys: None,
                // Left out with the most force of all: this key changes what a
                // turn *is*, not how it looks. The wizard asks nothing about
                // fan-out, and a file that arrived with caps already in it would
                // have put every turn through io-harness's spawn loop for
                // somebody who never chose to.
                containment: None,
                // The capability keys are left out because the wizard asks about
                // none of them and a file that arrived with an MCP server, a
                // language server or a browser in it would have configured
                // something nobody chose.
                mcp: None,
                lsp: None,
                browser: None,
                skills: None,
                // And the three contract ceilings for the same reason as the
                // diff style: each one's absence is io-harness's own default,
                // and a key written with its own default is a key a reader has
                // to wonder about. The wizard asks about none of them.
                max_parallel_reads: None,
                spawn_background_after_secs: None,
                detached_spawns: None,
                // Left out because the wizard has not read a catalogue yet when
                // it renders this. Prices arrive from the read that follows the
                // credential check, as their own edits against the file this
                // wrote — which is also what keeps the shape identical for an
                // operator who runs the wizard and one who adds a provider from
                // `/provider` later.
                prices: None,
                // Left out for the same reason as the caps, and with the same
                // force: a gate decides whether a finished turn counts as
                // finished, and a file that arrived with one already in it would
                // hold back every turn of somebody who never said what "done"
                // means here — a review criterion by spending a second model's
                // completion to do it. The wizard asks about none of that, and
                // there is no default worth writing: "no gate" is the honest
                // answer for an operator who has not answered yet.
                gates: None,
                // Left out for the strongest version of the same reason: absent
                // is what io-harness already does, and it is what almost every
                // operator wants. Writing `conversational = true` into a fresh
                // file would state a default as though it were a choice, and
                // writing `false` would make a new install open a run to answer
                // "hello".
                conversational: None,
                // Left out for the caps' reason: routing names models, and the
                // wizard has asked about exactly one. A rule pointing at a model
                // the operator never chose is worse than no rule.
                routing: None,
            },
        },
    };
    toml::to_string_pretty(&file)
}

/// Where io-harness looks for the user-scope file.
///
/// The harness's own function, not a copy of its rules: `$IO_CONFIG`, else
/// `$IO_CONFIG_HOME/io.toml`, else the platform's own place. Duplicating that
/// here would be a second answer to a question the harness already answers, and
/// the two would drift.
pub fn user_path() -> Option<PathBuf> {
    io_harness::config::user_path()
}

/// The run store, beside the configuration file.
///
/// That is the directory this product already owns, and asking for a second one
/// buys nothing. It lives here rather than in the binary because both entry
/// points need it: an interactive session and a headless `io exec` write to the
/// same store, which is what lets `/resume` list a run that CI started.
pub fn store_path() -> Option<PathBuf> {
    Some(user_path()?.parent()?.join("runs.db"))
}

/// Write the file, creating its directory, with mode `0600` on unix.
///
/// The mode is set on the file that is created rather than afterwards, so there
/// is no window in which a key sits on disk world-readable. This is what `gh`,
/// `aws` and `npm` do; an OS keychain is a later question and not obviously a
/// better answer, since one that fails silently on a headless Linux box is worse
/// than a file that never does.
pub fn write(path: &Path, contents: &str) -> io::Result<()> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }

    #[cfg(unix)]
    {
        use std::io::Write as _;
        use std::os::unix::fs::OpenOptionsExt;

        let mut file = std::fs::OpenOptions::new()
            .write(true)
            .create(true)
            .truncate(true)
            .mode(0o600)
            .open(path)?;
        file.write_all(contents.as_bytes())?;
        file.sync_all()
    }
    #[cfg(not(unix))]
    {
        // Windows has no mode bits. The file lands in the user's own roaming
        // profile, which is already per-user, and pretending otherwise by
        // reporting a mode we did not set would be worse than saying nothing.
        std::fs::write(path, contents)
    }
}

/// The three `TaskContract` ceilings io-harness gives no configuration key.
///
/// Read from `[app.io-cli]` because that is io-cli's own section and there is
/// nowhere else: `RunSection` has thirteen fields and none of these is one, so a
/// file has never been able to say them. Applied in
/// [`crate::contract::configured`], which is the half a session turn and an
/// `io exec` run share.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Ceilings {
    max_parallel_reads: Option<usize>,
    spawn_background_after_secs: Option<u64>,
    detached_spawns: Option<bool>,
}

/// What `[app.io-cli]` asks of a contract's ceilings, or nothing.
pub fn ceilings(config: &io_harness::Config) -> Ceilings {
    let Some(stored) = stored(config).0 else {
        return Ceilings::default();
    };
    Ceilings {
        max_parallel_reads: stored.max_parallel_reads,
        spawn_background_after_secs: stored.spawn_background_after_secs,
        detached_spawns: stored.detached_spawns,
    }
}

impl Ceilings {
    /// Put them on a contract, each only where the file asked.
    ///
    /// A key the file does not name leaves io-harness's own default alone —
    /// which for these three is 10, never, and permitted. Writing a default back
    /// explicitly would turn an absence into a statement.
    pub fn apply(self, mut contract: io_harness::TaskContract) -> io_harness::TaskContract {
        if let Some(reads) = self.max_parallel_reads {
            contract = contract.with_max_parallel_reads(reads);
        }
        if let Some(secs) = self.spawn_background_after_secs {
            contract = contract.with_spawn_background_after(std::time::Duration::from_secs(secs));
        }
        // Only the `false` arm does anything: `without_detached_spawns` is the
        // only lever io-harness offers and the default is already true, so
        // `detached_spawns = true` in a file is agreement rather than a change.
        if self.detached_spawns == Some(false) {
            contract = contract.without_detached_spawns();
        }
        contract
    }
}
