//! The command line. A handful of flags, and a subcommand for each thing that is
//! not a session, because everything else is a slash command inside one.
//!
//! The count is deliberately not stated. It said "five" from 0.2.0 until 0.6.0
//! and was wrong for most of that time, and it said "two subcommands" until
//! 0.23.0 added a third — which is the argument against writing a number into
//! prose that nothing checks.

use std::path::PathBuf;

use clap::{Parser, Subcommand};

/// `io` — a terminal agent over io-harness.
#[derive(Debug, Parser)]
#[command(name = "io", version, about, long_about = None)]
pub struct Cli {
    /// The workspace to work in. Defaults to the current directory.
    ///
    /// `global` so it is accepted on either side of a subcommand: `io -C dir
    /// exec "…"` and `io exec -C dir "…"` are the same command. Without it clap
    /// takes a flag declared here as belonging strictly before the subcommand,
    /// which is not how anyone types it and not how the README documents it.
    #[arg(short = 'C', long, value_name = "DIR", global = true)]
    pub dir: Option<PathBuf>,

    /// A named profile from the configuration file, for this run only.
    ///
    /// `[profile.<name>]` is io-harness's own — a profile body is the file
    /// format again, applied over the merged scopes through the same merge they
    /// use. It has been in the harness since its 0.27.0 and no io-cli release
    /// selected one until 0.16.0.
    ///
    /// `global` for the reason `-C` and `-m` are: a set of choices you want for
    /// one run is exactly the thing you type on either side of a subcommand, and
    /// `io exec --profile ci "…"` is the shape CI reaches for. Nothing is
    /// written — a profile chosen here lasts the run and no longer.
    #[arg(long, value_name = "NAME", global = true)]
    pub profile: Option<String>,

    /// The model to use for this run, overriding the configured one.
    #[arg(short, long, value_name = "MODEL", global = true)]
    pub model: Option<String>,

    /// Run without animation: nothing turns, nothing moves, and every state the
    /// session enters is written into the terminal's scrollback as one line of
    /// text. Forces the ASCII glyph set.
    ///
    /// For a screen reader, a braille display, a serial console and a captured
    /// log — surfaces on which a spinner is not a quiet decoration but a cell
    /// that changes ten times a second with no new information in it, and on
    /// which a status line that only ever repaints is a state nobody can read.
    ///
    /// `global` for the same reason `-C` and `-m` are: `io --plain exec "…"` and
    /// `io exec --plain "…"` are one command, and 0.5.0 shipped a defect where
    /// `-m` after the subcommand was rejected. A flag whose acceptance depends on
    /// which side of a word it is typed is a flag that works on the author's
    /// machine.
    ///
    /// It reaches an interactive session and stops there. `io exec` constructs no
    /// theme, draws nothing and animates nothing already — see `crate::exec` —
    /// so there is no second thing for the flag to switch off there, and wiring
    /// it in would be a knob with no wire behind it.
    #[arg(long, global = true)]
    pub plain: bool,

    #[command(subcommand)]
    pub command: Option<Command>,
}

#[derive(Debug, Subcommand)]
pub enum Command {
    /// Run the first-run wizard again: provider, credential, model, theme and
    /// permission posture, written to io-harness's configuration file.
    Setup,
    /// Run one goal to completion without a terminal, and exit with a status
    /// that says how the run ended.
    Exec(Exec),
    /// List the runs parked in the store, or carry one of them on: answer its
    /// question, decide its plan, or say what happened to the call it stopped
    /// mid-way through.
    Resume(Resume),
    /// Add, list, inspect, change or remove an MCP server, without opening a
    /// session.
    Mcp(Manage),
    /// Add, list, search for or remove a capability bundle, and manage the
    /// marketplaces bundles come from, without opening a session.
    Plugin(Manage),
    /// Read or write one configuration key, without opening a session.
    Config(Manage),
    /// Install, list or remove a skill, without opening a session.
    ///
    /// **Missing until 0.30.1, and `io skill add` did not exist because of it.**
    /// `manage::parse` accepted the surface, `manage::plan` answered for it and
    /// the session's own arm ran it — but clap knows only the subcommands named
    /// in this enum, so the argv door answered `unrecognized subcommand 'skill'`
    /// for a verb the README and the CHANGELOG both documented. Nothing under
    /// `tests/` links `src/main.rs` and nothing tested clap's routing, so the
    /// whole suite was green over a door that did not open.
    Skill(Manage),
}

/// The three management subcommands' arguments, handed through untouched.
///
/// **Every token, verbatim, straight to `crate::manage::parse`** — which is what
/// makes `io mcp add …` and `/mcp add …` one parse rather than two that agree
/// today. clap is deliberately given no schema for what is inside: a second
/// grammar expressed in `#[arg]` attributes is precisely the second
/// implementation F6 compares bytes to rule out.
///
/// `trailing_var_arg` and `allow_hyphen_values` are both load-bearing and neither
/// is optional. Without them clap consumes `--store` in
/// `io mcp add semlith -- semlith --store <path> mcp` as an unknown flag of io's
/// own and the line dies before `manage` sees it — F7's whole subject. Nothing in
/// `manage.rs` can compensate; the tokens have to arrive intact.
///
/// The flags are not documented here on purpose. `io mcp --help` shows this
/// paragraph and `manage`'s refusals name the accepted shapes at the point of
/// getting one wrong, which is where an operator is actually looking; a flag list
/// repeated in a doc comment is a second place for the grammar to drift.
#[derive(Debug, clap::Args)]
pub struct Manage {
    /// The verb and its arguments — `add semlith -- semlith --store <path> mcp`.
    #[arg(trailing_var_arg = true, allow_hyphen_values = true)]
    pub words: Vec<String>,
}

/// `io exec` — the headless entry point.
///
/// It takes no keyboard and draws nothing. Everything an interactive session
/// decides by keystroke is either a flag here or a line in the configuration
/// file, and the flags are deliberately few: a headless run that needs six of
/// them is a configuration file with worse ergonomics.
#[derive(Debug, clap::Args)]
pub struct Exec {
    /// What the agent should do.
    pub goal: String,

    /// Write the run's events to stdout as newline-delimited JSON, one object
    /// per line, instead of the agent's reply.
    ///
    /// The objects are `io_harness::RunEvent` serialized by io-harness's own
    /// derive — the same shape its `[[hook]]` writer appends and its store
    /// keeps. Nothing else is written to stdout, so the stream pipes straight
    /// into a JSON reader.
    #[arg(long)]
    pub json: bool,

    /// Where a command this run executes may write. Defaults to `[sandbox]`.
    ///
    /// This is not the same axis as `--policy`: `--sandbox` is where the
    /// sandbox lets a command write, `--policy` is what the agent is permitted
    /// to attempt at all. They share the word `read-only` and mean different
    /// things by it.
    #[arg(long, value_enum, value_name = "MODE")]
    pub sandbox: Option<Sandbox>,

    /// The permission posture for this run. Defaults to `[policy]`.
    ///
    /// `ask-writes` is refused here: nothing in an unattended run can answer an
    /// approval, so honouring it would turn *ask* into *deny* without saying so.
    #[arg(long, value_enum, value_name = "POSTURE")]
    pub policy: Option<PolicyFlag>,

    /// Take the provider from the environment instead of a configuration file.
    ///
    /// For CI, where nothing should be written to disk: the credential and the
    /// model come from the pair of variables io-harness's own `from_env`
    /// constructors read, and `-m` overrides the model half.
    #[arg(long, value_enum, value_name = "NAME")]
    pub provider: Option<FromEnv>,
}

/// `io resume` — the parked runs, and the way to carry one on.
///
/// The register is [`Exec`]'s: a positional for the thing being acted on, then
/// flags, then the same `--json` / `--policy` / `--provider` trio a headless run
/// takes, meaning the same three things. What is deliberately absent is
/// `--sandbox`: a resumed run is one that already started under a boundary, and
/// the confinement it should carry on under is the project's `[sandbox]`, which
/// [`crate::contract::configured`] already puts on the contract. A flag that
/// widened it halfway through a run would be a widening nobody asked for at the
/// point nobody is watching.
///
/// **Each pause takes its own input, and exactly one.** A question wants free
/// text, a plan wants a verdict and — for `revise` — a correction, an
/// interrupted call wants a decision and — for `completed` — the operator's
/// account of what it returned, and a run whose process merely died wants
/// nothing at all. clap cannot see which pause a run is on, so which flag was
/// the right one is settled against the store by [`crate::exec::decision_for`].
#[derive(Debug, clap::Args)]
pub struct Resume {
    /// The run to carry on. Omitted with `--list`.
    #[arg(value_name = "RUN_ID", required_unless_present = "list")]
    pub run: Option<i64>,

    /// List the runs waiting for a person and carry none of them on.
    ///
    /// Reads the store and calls no provider, so it costs nothing and takes no
    /// lease on anything it lists.
    #[arg(
        long,
        conflicts_with_all = ["run", "answer", "plan", "correction", "recovery", "account", "goal"]
    )]
    pub list: bool,

    /// The answer to the question the run stopped on.
    #[arg(long, value_name = "TEXT")]
    pub answer: Option<String>,

    /// What to do with the plan the run proposed.
    #[arg(long, value_enum, value_name = "VERDICT")]
    pub plan: Option<PlanFlag>,

    /// What the plan should do differently. Required by `--plan revise` and
    /// meaningless without it.
    #[arg(long, value_name = "TEXT", requires = "plan")]
    pub correction: Option<String>,

    /// What happened to the call the run was interrupted in the middle of.
    #[arg(long, value_enum, value_name = "DECISION")]
    pub recovery: Option<RecoveryFlag>,

    /// What the call returned. Required by `--recovery completed` and
    /// meaningless without it.
    ///
    /// Nothing validates it: the operator is asserting a fact about the outside
    /// world that no code here can check.
    #[arg(long, value_name = "TEXT", requires = "recovery")]
    pub account: Option<String>,

    /// What the run was asked to do.
    ///
    /// `runs.goal` has no public reader, so a contract cannot be rebuilt from a
    /// run alone. For a run that served a session turn the operator's own words
    /// are recoverable from the turn; for a *bare* run — one `io exec` or any
    /// other non-session caller started — they are not, and this is the only way
    /// to supply them.
    #[arg(long, value_name = "TEXT")]
    pub goal: Option<String>,

    /// Write the resumed run's events to stdout as newline-delimited JSON, and
    /// `--list`'s rows as one object per line, instead of prose.
    #[arg(long)]
    pub json: bool,

    /// The permission posture for the rest of this run. Defaults to what the run
    /// itself recorded, and then to `[policy]`.
    ///
    /// `ask-writes` is refused for the reason `io exec` refuses it.
    #[arg(long, value_enum, value_name = "POSTURE")]
    pub policy: Option<PolicyFlag>,

    /// Take the provider from the environment instead of a configuration file.
    #[arg(long, value_enum, value_name = "NAME")]
    pub provider: Option<FromEnv>,
}

/// `--plan`, in `io_harness::PlanVerdict`'s own words.
///
/// `cancel` is a decision and not a refusal to decide: it ends the run as
/// `PlanRejected`, which is what "do not do this at all" means and is why it
/// goes through the plan entry point rather than being turned into a plain
/// resume that would spend the rest of the budget on the approach just refused.
#[derive(Debug, Clone, Copy, PartialEq, Eq, clap::ValueEnum)]
pub enum PlanFlag {
    /// Carry it out.
    #[value(name = "approve")]
    Approve,
    /// Not this one. `--correction` says what to change.
    #[value(name = "revise")]
    Revise,
    /// Do not do this at all.
    #[value(name = "cancel")]
    Cancel,
}

/// `--recovery`, for a call whose outcome only a person can establish.
///
/// The names are the operator's rather than the harness's on one of the three:
/// io-harness spells the middle one `Abort`, which reads as *stop the program*
/// on a command line, and what it actually means is *do not make that call, and
/// stop*.
#[derive(Debug, Clone, Copy, PartialEq, Eq, clap::ValueEnum)]
pub enum RecoveryFlag {
    /// Make the call again. For a call established not to have landed, or one
    /// that is harmless to repeat.
    #[value(name = "retry")]
    Retry,
    /// Do not make the call, and do not carry on.
    #[value(name = "abandon")]
    Abandon,
    /// The call landed. `--account` is what the agent is told it returned.
    #[value(name = "completed")]
    Completed,
}

/// `--provider`, for a run with no configuration file.
///
/// `compatible` is deliberately absent. io-harness gives it no `from_env` of its
/// own, for the reason its source states: a base URL or a preset has to come
/// from somewhere, and a base URL on a command line is a configuration file with
/// worse ergonomics. A `compatible` endpoint reaches `io exec` through
/// `io.toml`, which already works.
#[derive(Debug, Clone, Copy, PartialEq, Eq, clap::ValueEnum)]
pub enum FromEnv {
    /// Named as io-harness's own `ProviderSpec` tag spells it, not as clap
    /// would derive it from the variant — `openrouter`, never `open-router`.
    #[value(name = "openrouter")]
    OpenRouter,
    #[value(name = "anthropic")]
    Anthropic,
    #[value(name = "openai")]
    OpenAi,
}

impl FromEnv {
    /// The credential and model variables, which are io-harness's own names —
    /// so a shell that already works with the harness works here unchanged.
    pub fn vars(self) -> (&'static str, &'static str) {
        match self {
            Self::OpenRouter => ("OPENROUTER_API_KEY", "OPENROUTER_MODEL"),
            Self::Anthropic => ("ANTHROPIC_API_KEY", "ANTHROPIC_MODEL"),
            Self::OpenAi => ("OPENAI_API_KEY", "OPENAI_MODEL"),
        }
    }
}

/// `--sandbox`, in `io_harness::ExecMode`'s own kebab-case names.
#[derive(Debug, Clone, Copy, PartialEq, Eq, clap::ValueEnum)]
pub enum Sandbox {
    /// Commands may read the workspace and write nothing.
    #[value(name = "read-only")]
    ReadOnly,
    /// Commands may write inside the workspace.
    #[value(name = "workspace-write")]
    WorkspaceWrite,
    /// No confinement. A widening a checked-in configuration file is not
    /// allowed to express, which is why using it prints a line on stderr.
    #[value(name = "full-access")]
    FullAccess,
}

impl Sandbox {
    pub fn mode(self) -> io_harness::ExecMode {
        match self {
            Self::ReadOnly => io_harness::ExecMode::ReadOnly,
            Self::WorkspaceWrite => io_harness::ExecMode::WorkspaceWrite,
            Self::FullAccess => io_harness::ExecMode::FullAccess,
        }
    }
}

/// `--policy`, in the words the status line and the wizard already use.
///
/// The value names are `settings::Posture::short()`, reused rather than
/// re-spelled; `tests/exec.rs` asserts the two lists are the same, because a
/// flag that disagrees with the status line is a flag that teaches the wrong
/// vocabulary.
#[derive(Debug, Clone, Copy, PartialEq, Eq, clap::ValueEnum)]
pub enum PolicyFlag {
    /// Read, write and run inside the workspace; no outbound network.
    #[value(name = "workspace")]
    Workspace,
    /// Read freely; writes and commands ask first. Refused for a headless run.
    #[value(name = "ask-writes")]
    AskWrites,
    /// Read only.
    #[value(name = "read-only")]
    ReadOnly,
}
