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
//! # A call is a decision, not an outcome
//!
//! io-harness stages an `AssistantTurn` from the provider's response, with the
//! calls exactly as the model produced them, *before* any of them is dispatched.
//! So a `git_commit` the policy refused, or one git rejected, is in `calls` beside
//! one that landed and reads identically there. Nothing this module can see
//! separates them, and no `EventKind` carries a tool's result either — which is
//! why [`made_in`] answers "what the model asked for" and the block is committed
//! by the caller, which watched the refusal and the failure go past. That
//! boundary is stated here rather than guessed at, because the wrong half of it
//! is a scrollback claiming a commit that does not exist.
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
    /// The message's first written line, which is what a one-line summary shows.
    ///
    /// A commit message is a subject and an optional body separated by a blank
    /// line; showing the whole thing on a status row would push the body through
    /// a surface that has no room for it.
    ///
    /// **First *written* line, not first line.** A model that opens its message
    /// with a newline has still written a subject, and taking `lines().next()`
    /// there answers `""` — a status row that has gone blank for a commit that
    /// has a perfectly good subject one line below. The scan stops at the first
    /// line with something on it, so nothing further down can be promoted past a
    /// real subject.
    pub fn subject(&self) -> &str {
        self.message
            .lines()
            .map(str::trim)
            .find(|line| !line.is_empty())
            .unwrap_or_default()
    }
}

/// Every commit made in the turns given, oldest first.
///
/// Pure over the store's own rows so it can be asserted without a store: the
/// caller passes what `Store::step_turns` returned. A call carrying no `message`
/// argument, one whose `message` is not a string, and one whose message is only
/// whitespace are all skipped rather than rendered as an empty commit — each of
/// those means the model called the tool wrongly and the harness refused it, not
/// that somebody committed nothing.
///
/// Order is the order the model worked in: turns arrive from `step_turns`
/// oldest step first, and a turn's `calls` are in the order it made them, so
/// walking both in sequence is already chronological. Nothing sorts afterwards,
/// because a sort on `step` alone would shuffle two commits made in one step
/// into whichever order the sort happened to be stable about.
///
/// A turn holding one `git_commit` among a batch of reads and writes yields that
/// one call and nothing else; the name check is per call, not per turn.
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
/// `branch` is where it landed, optional because it can genuinely be unknown — a
/// contained child commits in a checkout this process cannot name — and a line
/// saying "unknown" is worse than a line that is not there. A branch that is
/// present but blank is treated as absent for the same reason: it is an unknown
/// wearing a value's clothes.
///
/// **There is deliberately no "what git printed" parameter, and one was removed
/// to make that true.** It had a single production call site, passing `None`
/// forever, because no `EventKind` carries a tool's result — the module header
/// says so. Only the tests ever supplied it, which made it dead flexibility, and
/// the README carried a sentence describing a line no operator could ever see.
///
/// Only a commit that landed reaches here. See the module note on why a refused
/// or failed `git_commit` is the caller's to filter and not this function's to
/// detect.
pub fn block(made: &Made, branch: Option<&str>) -> Vec<String> {
    let mut lines = Vec::new();
    lines.push(match branch.map(str::trim).filter(|b| !b.is_empty()) {
        Some(branch) => format!("committed on {branch}"),
        None => "committed".to_string(),
    });
    for line in made.message.lines() {
        // A blank line in the body stays blank rather than becoming two spaces.
        // Indenting nothing is invisible on screen and load-bearing everywhere
        // else — it is what a copied block pastes back, and what a test that
        // compares committed lines has to spell.
        lines.push(if line.trim().is_empty() {
            String::new()
        } else {
            format!("  {line}")
        });
    }
    lines
}

/// The prompt `/commit` hands to the agent.
///
/// io-cli does not write the message. It asks the agent to describe the work it
/// just did and to stage it, which is the whole of what the command means — and
/// it is why this is a prompt rather than a git invocation. This crate cannot run
/// git at all: the engine is `pub(crate)` in the dependency, and
/// `tests/dependencies.rs` permits a process spawn in `src/shell.rs` alone,
/// identified by path. That gate matches raw text, comments included, so the
/// forbidden name is not spelled out here — not even to say this module does not
/// use it, which is a sentence that reddens the gate it is describing.
pub fn prompt() -> String {
    "Commit the work from this turn. Review what changed with the git tools, stage \
     what belongs in this commit, and write a message describing what the change \
     does and why. Do not commit files unrelated to this turn's work."
        .to_string()
}

/// What `/commit` may do, given the policy in force.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Asked {
    /// Nothing refuses git. Buy the turn.
    Ready,
    /// An **asking default** refuses it, and [`crate::approval::git_allowance`]
    /// would lift it. The sentence names the rule; `/commit allow` takes it.
    Offer(String),
    /// Something refuses it that the allowance cannot lift, or must not.
    Refuse(String),
}

/// Whether `/commit` may spend a turn, and what to say when it may not.
///
/// **The check happens before the turn, not after it.** A commit the policy was
/// always going to refuse still costs a real completion against a real model to
/// discover, so the one question worth asking first is whether git can run at all.
///
/// **The three answers turn on the whole verdict and not on the tier default, and
/// this release shipped the shallower version first.** `Verdict` carries `rule`,
/// which is `None` exactly when the tier default decided, so:
///
/// - `Allow` — [`Asked::Ready`].
/// - `Ask` **with no rule** — an asking *default*. The operator chose a posture
///   that promises to ask and the harness's git spawn does not ask, so naming the
///   one rule that lifts it tells them what their own posture meant to do. This
///   is the only case the allowance is offered in, because it is the only case
///   where the allowance both helps and is honest.
/// - **Anything else** — [`Asked::Refuse`], and the allowance is neither offered
///   nor applied. Two distinct situations arrive here and both are traps:
///
///   A **denying default** is `read only`. A rule is matched *before* a default,
///   so the allowance would in fact work there — which is exactly why it must not
///   be offered. A keystroke that defeats the one posture whose name is a promise
///   is not a convenience, and worse, the `.git` write gate would refuse the
///   commit anyway, so the turn would be bought for nothing.
///
///   A **rule** — a `deny_exec` in the operator's own configuration, or a deny
///   with an allowlist — cannot be lifted by a later allow at all, because deny
///   wins across layers. Offering the allowance there would print advice that can
///   never be taken, on every attempt, forever.
pub fn asked(policy: &io_harness::Policy) -> Asked {
    let verdict = policy.check(io_harness::Act::Exec, crate::approval::GIT);
    match (verdict.effect, verdict.rule.as_deref()) {
        (io_harness::Effect::Allow, _) => Asked::Ready,
        (io_harness::Effect::Ask, None) => Asked::Offer(
            "this posture asks before running a command, and the harness's git tools are refused \
             rather than asked — so a commit would be refused after the turn was paid for. Run \
             `/commit allow` to permit `git` for this session and commit."
                .to_string(),
        ),
        // A rule decided, whatever it said. Name the layer, because a refusal an
        // operator cannot locate in their own files is one they cannot act on.
        (_, Some(rule)) => Asked::Refuse(format!(
            "a rule in the policy refuses git ({rule}{}), so the agent cannot commit. Allowing \
             `git` for the session cannot lift it — a deny wins over any later allow — so this is \
             a change to make in the file the rule came from.",
            verdict
                .layer
                .as_deref()
                .map(|layer| format!(" in {layer}"))
                .unwrap_or_default(),
        )),
        // A denying default: `read only`, and it is a posture rather than a rule.
        (_, None) => Asked::Refuse(
            "this posture does not let the agent run a command, so it cannot commit. Change the \
             posture if you want it to — `/commit allow` deliberately will not, because a rule \
             beats a default and this one would quietly undo the posture you chose."
                .to_string(),
        ),
    }
}

/// The sentence naming who a commit will be authored as.
///
/// Shown before the turn is spent, because the identity is the one thing about a
/// commit that cannot be corrected afterwards without rewriting history.
///
/// **This function has no default of its own, and that is the whole of it.**
/// `[run.commit_identity]` reaches `TaskContract::commit_identity` through the
/// harness's own `Config::apply_to`, and io-harness passes that value to git as
/// `-c user.name=… -c user.email=…` on the commit invocation itself. The field is
/// an `Identity` rather than an `Option<Identity>`, already carrying
/// `Identity::default()` when the operator configured no section — so a
/// repository with no identity of its own is told which default *io-harness* will
/// use, by reading the same value git will be handed.
///
/// A name chosen here instead would be a string this crate invented, written into
/// the operator's history by a tool that was only asked to describe it. There is
/// nowhere else to correct that afterwards, which is why the branch that would
/// pick one does not exist rather than being merely unused: an identity is read
/// and printed, never substituted, not even for a value this crate recognises as
/// the default.
pub fn authored_as(identity: &Identity) -> String {
    format!("authored as {} <{}>", identity.name, identity.email)
}
