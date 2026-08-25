//! The contract a session turn carries, and the one place it is built.
//!
//! **A session builds its own contract in io-harness, and that is the whole
//! reason this module exists.** `Session::turn` and `Session::turn_steered` call
//! `default_contract`, which is `TaskContract::workspace(text, root)` and nothing
//! else — so a responder, a plan gate, an MCP server, a language server, a
//! browser and a skills directory are all unreachable from those turns however
//! they are configured. io-cli uses neither: both of its arms take a contract
//! built here.
//!
//! **The coupling this module used to describe is gone, and was gone before it
//! stopped saying so.** Through 0.10.0,
//! `Session::turn_contained_bounded_observed` was the only session entry point
//! that took a caller's contract, so everything below arrived with the fan-out or
//! not at all. 0.11.0 needed a contract on the flat turn for the step cap and
//! moved it to `Session::turn_bounded_observed`, which takes one too — and with
//! that, one contract is built per turn and both arms are handed it. What a turn
//! carries no longer depends on whether it can fan out;
//! `tests/contract.rs`'s F6 is what keeps that true.
//!
//! The two seams that are not capabilities of the operator's configuration are
//! arguments instead: the responder is unconditional, because io-harness resolves
//! it inside the tool dispatch on any run, and the plan gate is present only when
//! the operator asked with `/plan on`, because registering one is the entire
//! condition for io-harness's planning phase.
//!
//! **0.14.0 — the configuration file reaches a session turn, and it reaches it
//! from here.** Until this release `contract::session` assembled a contract by
//! hand and never called `Config::apply_to`, while `io exec` had called it since
//! 0.2.0 — so eleven sections of `io.toml` were read by io-harness, validated by
//! io-harness, documented in this product's own README, and then discarded by
//! every interactive turn. [`configured`] is the half both arms are now built
//! from, and its doc comment carries the order of precedence the whole release
//! turns on.

use std::path::PathBuf;
use std::sync::Arc;

use io_harness::{Config, TaskContract};

use crate::settings::CliSettings;

/// What the operator configured that a turn's contract can carry.
///
/// io-harness's own types in every field. io-cli defines no schema for an MCP
/// server, a language server or a browser: each is `Deserialize` in the harness
/// for exactly this purpose, and a second spelling would be a second thing to
/// keep true — the same reason `[app.io-cli.containment]` is
/// `io_harness::Containment` rather than four fields of io-cli's own.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Capabilities {
    /// `[[app.io-cli.mcp]]` — servers spawned as child processes for the turn.
    pub mcp: Vec<io_harness::McpServer>,
    /// `[[app.io-cli.lsp]]` — language servers brought up for this workspace.
    pub lsp: Vec<io_harness::LspServer>,
    /// `[app.io-cli.browser]` — a browser already installed, never downloaded.
    pub browser: Option<io_harness::BrowserConfig>,
    /// `[app.io-cli] skills` — the directory io-harness discovers skills in.
    pub skills: Option<PathBuf>,
    /// `[app.io-cli] max_steps` — how many steps one turn may take.
    ///
    /// io-harness's `TaskContract::workspace` caps a turn at twelve, which a
    /// turn that reads a repository and writes a file reaches with the work half
    /// done. Raising it is why 0.11.0 gave the flat turn a contract at all, and
    /// is therefore the accidental cause of the whole coupling coming apart.
    pub max_steps: Option<u32>,
}

impl Capabilities {
    /// What `[app.io-cli]` asked for, or nothing at all.
    pub fn stored(stored: Option<&CliSettings>) -> Self {
        let Some(settings) = stored else {
            return Self::default();
        };
        Self {
            mcp: settings.mcp.clone().unwrap_or_default(),
            lsp: settings.lsp.clone().unwrap_or_default(),
            browser: settings.browser.clone(),
            skills: settings.skills.clone(),
            max_steps: settings.max_steps,
        }
    }

    /// Whether anything here would change a turn.
    ///
    /// What the status line asks before it says a session is connected to
    /// something it was never given.
    pub fn any(&self) -> bool {
        !self.mcp.is_empty()
            || !self.lsp.is_empty()
            || self.browser.is_some()
            || self.skills.is_some()
    }
}

/// How many steps a session turn may take when nothing says otherwise.
///
/// **io-harness's own default is twelve, and twelve is not a turn.** A turn that
/// reads a repository, writes a file and checks its work spends that with the
/// job half done, and what the operator sees is `error: step_cap_reached` under
/// an unfinished answer — which is a ceiling reported as a failure.
///
/// A thousand is not a number anybody will reach on purpose. It is the number
/// that stops the cap being the thing that ends a turn, and it is safe to set
/// because it is not the only bound in the system: io-harness stops an agent
/// that stalls, `[run]`'s budgets stop one that spends, and `Ctrl+C` stops one
/// an operator has seen enough of. A step cap was never the right instrument for
/// any of those, and it was standing in for all three.
pub const MAX_STEPS: u32 = 1_000;

/// What io-cli tells the agent about itself, appended to io-harness's own.
///
/// **Every turn before 0.13.0 ran `SystemPrompt::Builtin`**, which the harness
/// documents as naming the tools and saying nothing about how to use them — no
/// tone, no shape, no rule about length. What an operator got for an ordinary
/// question was a model with a tool catalogue and no idea what it was.
///
/// Three properties make this text shippable, and each is a test rather than an
/// intention:
///
/// - **It names no vendor and no model.** io-cli is pointed at a catalogue of
///   four hundred models by a flag, so anything it said about *which* model was
///   reading would be false for almost all of them.
/// - **It claims no capability.** What the agent may reach is composed around
///   this block by the harness, from the contract — a sentence about browsing
///   would be a lie on every session with no `[app.io-cli.browser]`, which is
///   every default session.
/// - **It is bounded**, because it is prepaid on every turn of every session.
///
/// It is `SystemPrompt::Append` and not `Replace`: the harness's framing, its
/// tool catalogue, the repository's own instructions, the boundary section and
/// the ending all still have to reach the model, and this text sits between the
/// catalogue and the boundary rather than in place of any of it.
pub const PROMPT: &str = "\
You are the agent inside io, a terminal interface to one repository. The person \
reading you is at a terminal, in the middle of their own work, in a pane a few \
rows tall.

Answer what was asked, and put the answer first. A question about what something \
is gets prose; a question about what to do gets the smallest step that does it. \
Report work in the past tense once it is done, and never narrate what you are \
about to do. When you do not know, or the answer is in a file you have not \
opened, say so instead of guessing.

Be brief by default: a sentence or two for a small question, a short paragraph \
for a large one, and more only when you were asked for more. Prefer sentences to \
bullets and bullets to tables. Point at code the way a terminal can act on it — \
`src/thing.rs:42`.

You are rendered as monospaced text in a pane the person cannot widen. Code goes \
in a fenced block with its language named. Assume eighty columns: no wide \
tables, no art drawn out of characters, no markup that expects a browser. Do not \
refer to earlier output by where it is on the screen — it has scrolled.";

/// The half of a turn's contract that comes out of the configuration file — the
/// half both arms share, built once.
///
/// **The two builders were never assemblies of the same fields**, which is why
/// this is a new function rather than a rename of either. `io exec` set
/// `TaskContract::workspace`, `Config::apply_to`, `[sandbox]` and the
/// `--sandbox` flag and nothing else; `contract::session` set a step cap, a
/// responder and a system prompt and read no configuration at all. What is
/// unified here is the config-derived half. The three fields that stay
/// arm-specific are arguments of [`session`] and are not config-derived at all:
/// a session has a person behind it and `io exec` has nobody who could answer a
/// question, `/plan on` is a session keystroke, and [`PROMPT`] tells the model it
/// is rendered in an eighty-column pane whose earlier output has scrolled, which
/// is false of `io exec --json`.
///
/// **The step cap is no longer one of them.** A headless run took io-harness's
/// own twelve, so `io exec` ended `error: step_cap_reached` under half-finished
/// work with nobody watching — the same defect [`MAX_STEPS`] exists to fix in a
/// session, made worse rather than better by the run being unattended.
///
/// The order of the calls below is the order of precedence, weakest to
/// strongest:
///
/// 1. io-harness's own defaults, from `TaskContract::workspace`.
/// 2. io-cli's step floor. **Applied before `apply_to` and not after**, so a
///    `[run] max_steps` the operator actually wrote beats it. A floor applied
///    last would ignore a file that lowers the cap while honouring one that
///    raises it, which is the ordering defect that would otherwise ship looking
///    like a working feature.
/// 3. `Config::apply_to`, which is the twelve applicable `[run]` keys plus
///    `[[mcp]]`, `[[lsp]]`, `[browser]`, `[[agent]]`, `[web]`, `[instructions]`
///    and `[memory]`. It applies neither `[policy]` nor `[sandbox]`, and it
///    returns early on a file with no `[run]` table — which is why
///    `[run.commit_identity]` is behind that table rather than beside it.
/// 4. `[sandbox]`, attached here by hand because `apply_to` does not carry it:
///    the policy travels as its own argument to the turn, and the sandbox has
///    nowhere else to arrive from.
///
/// **`[sandbox]` is attached only where the file actually has one.** A default
/// `SandboxConfig` carries real ceilings — sixty CPU seconds, a hundred and
/// twenty wall seconds, two gibibytes, five hundred and twelve descriptors —
/// while `TaskContract::workspace` deliberately starts from
/// `SandboxLimits::none()`. Attaching one unconditionally would impose caps on
/// every session whose operator never asked for any, which is the failure the
/// field-for-field assertion in `tests/contract.rs` exists to catch.
///
/// `[app.io-cli]` is the fifth and strongest layer, and it belongs to [`session`]
/// alone: it is io-cli's own table and `io exec` does not read it.
pub fn configured(text: impl Into<String>, root: PathBuf, config: &Config) -> TaskContract {
    let contract = config.apply_to(TaskContract::workspace(text, root).with_max_steps(MAX_STEPS));
    match config.sandbox() {
        Some(sandbox) => contract.with_contained_exec(sandbox),
        None => contract,
    }
}

/// One collection out of the two scopes a server may be named in, and the ids a
/// collision cost.
///
/// **`with_mcp` and `with_lsp` assign the whole collection rather than extending
/// it**, so applying `[[mcp]]` and then `[[app.io-cli.mcp]]` in sequence leaves a
/// contract holding only the second list — an operator with servers in both
/// scopes would silently lose one set. They are concatenated here instead and
/// deduplicated by id, with the `[app.io-cli]` entry winning a collision because
/// it is the more specific scope.
///
/// The wider list keeps its positions, including the ones a collision replaced,
/// so what reaches the turn is the file's own order with an entry swapped rather
/// than two lists stapled together. Linear scans and an owned id on both sides:
/// these are lists an operator typed by hand, and an index over four elements is
/// a data structure nobody would be able to justify at the next release.
fn merged<T: Clone>(wide: &[T], narrow: &[T], id: impl Fn(&T) -> String) -> (Vec<T>, Vec<String>) {
    let mut out = Vec::with_capacity(wide.len() + narrow.len());
    let mut dropped = Vec::new();
    for entry in wide {
        // The `[app.io-cli]` entry takes the wider entry's place rather than
        // being appended, so the collection reaches the turn in the file's own
        // order with one element swapped.
        match narrow.iter().find(|other| id(*other) == id(entry)) {
            Some(winner) => {
                dropped.push(id(entry));
                out.push(winner.clone());
            }
            None => out.push(entry.clone()),
        }
    }
    for other in narrow {
        if !wide.iter().any(|entry| id(entry) == id(other)) {
            out.push(other.clone());
        }
    }
    (out, dropped)
}

/// What naming one server in both scopes cost, said in the operator's own
/// spellings.
///
/// Said once at session start rather than once per turn, which is why it is a
/// function of its own rather than a second return value from [`session`]: the
/// file does not change while a session runs, so a duplicate dropped on the first
/// turn is the same duplicate on the fiftieth, and a warning that repeats is one
/// an operator learns to read past.
///
/// The goal and the root handed to [`configured`] here are never read. Only
/// `Config::apply_to` can say what `[[mcp]]` and `[[lsp]]` hold — io-harness
/// keeps both lists private and exposes no accessor for either — so the shortest
/// way to ask the question is to apply the configuration to a contract nothing
/// runs.
pub fn server_notices(config: &Config, caps: &Capabilities) -> Vec<String> {
    let applied = configured(String::new(), PathBuf::new(), config);
    let (_, mcp) = merged(&applied.mcp, &caps.mcp, |server| server.id.clone());
    let (_, lsp) = merged(&applied.lsp, &caps.lsp, |server| server.id.clone());
    let mut notices = Vec::with_capacity(mcp.len() + lsp.len());
    for id in mcp {
        notices.push(format!(
            "the MCP server `{id}` is named in both `[[mcp]]` and `[[app.io-cli.mcp]]`; this \
             session runs the `[app.io-cli]` one and drops the other",
        ));
    }
    for id in lsp {
        notices.push(format!(
            "the language server `{id}` is named in both `[[lsp]]` and `[[app.io-cli.lsp]]`; this \
             session runs the `[app.io-cli]` one and drops the other",
        ));
    }
    notices
}

/// The contract a session turn carries — every turn, contained or not.
///
/// **Nothing configured must reproduce `default_contract` exactly**, because a
/// session that asked for none of this must run the turn it ran before this
/// release — the field-for-field assertion is in `tests/contract.rs`. Every
/// builder below is therefore conditional on the operator having asked, and none
/// of them has a default this function supplies. That is what makes
/// [`configured`] safe to call unconditionally: `Config::apply_to` carries only
/// what the file named, and `Config::sandbox` is `None` where the file has no
/// `[sandbox]`.
pub fn session(
    text: impl Into<String>,
    root: PathBuf,
    config: &Config,
    caps: &Capabilities,
    responder: Arc<dyn io_harness::Responder>,
    plan_gate: Option<Arc<dyn io_harness::PlanGate>>,
) -> TaskContract {
    // **Unconditional, and the gate below deliberately is not.** io-harness
    // resolves the responder inside the tool dispatch on any run, so there has
    // never been a reason for a question to reach a person on one kind of turn
    // and pause the run on the other.
    // **Unconditional for the same reason, and in the one place a turn's contract
    // is built.** Both arms are handed this value, so the manner cannot depend on
    // whether a turn can fan out — which is the drift `tests/contract.rs`'s F6
    // exists to make unrepresentable.
    let mut contract = configured(text, root, config)
        .with_responder(responder)
        .with_system_prompt(io_harness::SystemPrompt::Append(PROMPT.to_string()));
    // **Registering a gate is how the planning phase is turned on**, so this is
    // where the operator's `/plan` becomes a fact about the turn. `None` is not a
    // missing feature: it is a turn that works instead of proposing first, which
    // is what an ordinary prompt asks for.
    if let Some(gate) = plan_gate {
        contract = contract.with_plan_gate(gate);
    }
    // **The two scopes are merged here, after `apply_to` and never before it.**
    // `configured` has already put `[[mcp]]` and `[[lsp]]` on the contract, and
    // `with_mcp` assigns rather than extends — so reading the applied lists back
    // off the contract and writing the union is the only order in which an
    // operator who named servers in both scopes keeps both. What was dropped is
    // discarded here and reported once at session start by [`server_notices`],
    // because this runs on every turn and that sentence should not.
    let (mcp, _) = merged(&contract.mcp, &caps.mcp, |server| server.id.clone());
    if !mcp.is_empty() {
        contract = contract.with_mcp(mcp);
    }
    let (lsp, _) = merged(&contract.lsp, &caps.lsp, |server| server.id.clone());
    if !lsp.is_empty() {
        contract = contract.with_lsp(lsp);
    }
    if let Some(browser) = &caps.browser {
        contract = contract.with_browser(browser.clone());
    }
    if let Some(skills) = &caps.skills {
        contract = contract.with_skills(skills.clone());
    }
    if let Some(max_steps) = caps.max_steps {
        contract = contract.with_max_steps(max_steps);
    }
    contract
}
