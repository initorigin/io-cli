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

use std::path::PathBuf;
use std::sync::Arc;

use io_harness::TaskContract;

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

/// The contract a session turn carries — every turn, contained or not.
///
/// **Nothing configured must reproduce `default_contract` exactly**, because a
/// session that asked for none of this must run the turn it ran before this
/// release — the field-for-field assertion is in `tests/contract.rs`. Every
/// builder below is therefore conditional on the operator having asked, and none
/// of them has a default this function supplies.
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

pub fn session(
    text: impl Into<String>,
    root: PathBuf,
    caps: &Capabilities,
    responder: Arc<dyn io_harness::Responder>,
    plan_gate: Option<Arc<dyn io_harness::PlanGate>>,
) -> TaskContract {
    // **Unconditional, and the gate below deliberately is not.** io-harness
    // resolves the responder inside the tool dispatch on any run, so there has
    // never been a reason for a question to reach a person on one kind of turn
    // and pause the run on the other.
    let mut contract = TaskContract::workspace(text, root)
        .with_max_steps(MAX_STEPS)
        .with_responder(responder);
    // **Registering a gate is how the planning phase is turned on**, so this is
    // where the operator's `/plan` becomes a fact about the turn. `None` is not a
    // missing feature: it is a turn that works instead of proposing first, which
    // is what an ordinary prompt asks for.
    if let Some(gate) = plan_gate {
        contract = contract.with_plan_gate(gate);
    }
    if !caps.mcp.is_empty() {
        contract = contract.with_mcp(caps.mcp.clone());
    }
    if !caps.lsp.is_empty() {
        contract = contract.with_lsp(caps.lsp.clone());
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
