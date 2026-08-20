//! The contract a session turn carries, and the one place it is built.
//!
//! **A session builds its own contract in io-harness, and that is the whole
//! reason this module exists.** `Session::turn` and `Session::turn_steered` call
//! `default_contract`, which is `TaskContract::workspace(text, root)` and nothing
//! else — so a responder, a plan gate, an MCP server, a language server, a
//! browser and a skills directory are all unreachable from those turns however
//! they are configured. `Session::turn_contained_bounded_observed` (io-harness
//! 0.66.0) takes the caller's contract beside the caps, and it is the only
//! session entry point that takes one at all.
//!
//! The consequence is a coupling worth stating rather than hiding: **the
//! capabilities below and the fan-out are one switch.** A session with no
//! `[app.io-cli.containment]` runs the steered turn it has always run, keeps
//! `Ctrl+C` mid-turn, and carries none of this. That is io-harness's shape, not a
//! preference expressed here.

use std::path::PathBuf;

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
    /// done. It is raisable here and nowhere else: the steered turn builds its
    /// own contract and takes none from a caller.
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

/// The contract one contained session turn carries.
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

pub fn session(text: impl Into<String>, root: PathBuf, caps: &Capabilities) -> TaskContract {
    let mut contract = TaskContract::workspace(text, root).with_max_steps(MAX_STEPS);
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
