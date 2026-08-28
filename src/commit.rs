//! What the agent committed, and the prompt that asks it to.
//!
//! io-harness offers `git_commit` on every workspace run and has since long
//! before this interface existed. What it does **not** offer is any record of the
//! commit as a fact: no `EventKind` variant carries a message, a branch or an
//! object id, and no `Store` method returns one. The commit message survives in
//! exactly one place — the tool call the model made — and this module reads it
//! from there.
//!
//! # Read the typed call, never the display string
//!
//! `Store::step_turns` returns `AssistantTurn`s whose `calls` are typed
//! `ToolCall`s: a name and a `serde_json::Value` of arguments. That is the shape
//! this module reads.
//!
//! `StepRecord::tool_call` also contains the message, as
//! `git_commit:{"message":"…"}` joined by `" | "` — and it is **not** used here.
//! That joining is an internal display convention of the dependency rather than a
//! documented format, and splitting it on `:` fails on the first commit message
//! containing a colon, which is most of them.
//!
//! # The block does not re-draw the diff
//!
//! `crate::events` states the rule this module is an exception to: one cell per
//! call, because uncollapsed tool output is what makes a transcript unreadable.
//! The exception is narrow — a commit is the one tool call whose *whole purpose*
//! is to be read later — and it stays narrow by naming what git reported rather
//! than re-drawing hunks. The step's diffs are already on screen immediately
//! above, committed by the driver's edit path, and drawing them twice would cost
//! the reader the thing the exception was made for.

use io_harness::Identity;

/// The argument `git_commit` carries the message in.
///
/// Named rather than spelled inline because it is the one string in this module
/// that comes from the dependency's tool schema, and a typo in it produces a
/// commit block that is silently always empty.
pub const MESSAGE: &str = "message";

/// The tool whose calls this module reads.
pub const TOOL: &str = io_harness::tools::GIT_COMMIT_TOOL;

/// A commit the agent made during a turn.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Made {
    /// The step whose call made it.
    pub step: u32,
    /// The message the agent wrote, exactly as it was passed to `git_commit`.
    pub message: String,
}

impl Made {
    /// The message's first line, which is what a one-line summary shows.
    ///
    /// A commit message is a subject and an optional body separated by a blank
    /// line; showing the whole thing on a status row would push the body through
    /// a surface that has no room for it.
    pub fn subject(&self) -> &str {
        self.message.lines().next().unwrap_or_default().trim()
    }
}

/// Every commit made in the turns given, oldest first.
///
/// Pure over the store's own rows so it can be asserted without a store: the
/// caller passes what `Store::step_turns` returned. A call carrying no `message`
/// argument, or one whose `message` is not a string, is skipped rather than
/// rendered as an empty commit — an absent argument means the model called the
/// tool wrongly and the harness refused it, not that somebody committed nothing.
pub fn made_in(turns: &[io_harness::AssistantTurn]) -> Vec<Made> {
    let mut out = Vec::new();
    for turn in turns {
        for call in &turn.calls {
            if call.name != TOOL {
                continue;
            }
            let Some(message) = call.arguments.get(MESSAGE).and_then(|m| m.as_str()) else {
                continue;
            };
            if message.trim().is_empty() {
                continue;
            }
            out.push(Made {
                step: turn.step,
                message: message.to_string(),
            });
        }
    }
    out
}

/// The lines a commit commits into the scrollback.
///
/// Plain strings rather than styled lines, for the reason every other page in
/// this crate returns text: the tone is the caller's, and a module that builds
/// styled lines cannot be asserted without a theme.
///
/// `branch` is where it landed and `touched` is what git printed — both optional,
/// because both can genuinely be unknown, and a line saying "unknown" is worse
/// than a line that is not there.
pub fn block(made: &Made, branch: Option<&str>, touched: Option<&str>) -> Vec<String> {
    let mut lines = Vec::new();
    lines.push(match branch {
        Some(branch) => format!("committed on {branch}"),
        None => "committed".to_string(),
    });
    for line in made.message.lines() {
        lines.push(format!("  {line}"));
    }
    if let Some(touched) = touched {
        let touched = touched.trim();
        if !touched.is_empty() {
            lines.push(format!("  {touched}"));
        }
    }
    lines
}

/// The prompt `/commit` hands to the agent.
///
/// io-cli does not write the message. It asks the agent to describe the work it
/// just did and to stage it, which is the whole of what the command means — and
/// it is why this is a prompt rather than a git invocation. This crate cannot run
/// git at all: the engine is `pub(crate)` in the dependency and
/// `std::process::Command` is permitted in `crate::shell` alone.
pub fn prompt() -> String {
    "Commit the work from this turn. Review what changed with the git tools, stage \
     what belongs in this commit, and write a message describing what the change \
     does and why. Do not commit files unrelated to this turn's work."
        .to_string()
}

/// The sentence naming who a commit will be authored as.
///
/// Shown before the turn is spent, because the identity is the one thing about a
/// commit that cannot be corrected afterwards without rewriting history.
/// io-cli never sets it: `[run.commit_identity]` reaches the contract through the
/// harness's own `Config::apply_to`, and when the operator has configured nothing
/// this reports the default the *harness* will use rather than one invented here.
pub fn authored_as(identity: &Identity) -> String {
    format!("authored as {} <{}>", identity.name, identity.email)
}
