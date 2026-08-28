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
//! **0.17.0 — the arms moved again, and nothing here moved with them.** Both are
//! now `Session::turn_bounded_steered` and
//! `Session::turn_contained_bounded_steered`, which are the same two calls with a
//! `SteerInbox` appended, so a contained turn can be steered and containment
//! decides fan-out and nothing else. The inbox is a parameter of the drive call
//! rather than a field of [`io_harness::TaskContract`] — it carries the
//! operator's words for the duration of one turn, and a contract is what that
//! turn is *for* — which is the whole reason this module needed no change to get
//! it. `tests/steer.rs`'s F5 asserts both call sites.
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
    // Kept before `root` is moved into the contract: both the plugin hook merge
    // and nothing else needs it, and cloning a `PathBuf` once per turn is not a
    // cost worth threading an argument to avoid.
    let dir = root.clone();
    let contract = config.apply_to(TaskContract::workspace(text, root).with_max_steps(MAX_STEPS));
    let contract = match config.sandbox() {
        Some(sandbox) => contract.with_contained_exec(sandbox),
        None => contract,
    };
    // **The three ceilings, here rather than beside the other `[app.io-cli]`
    // keys, and the placement is the criterion.** `max_parallel_reads`,
    // `spawn_background_after` and `detached_spawns` are `TaskContract` fields
    // with no io-harness configuration key of their own, so io-cli names them —
    // and they belong in the half BOTH arms share. [`session`] is the session's
    // alone; `io exec` calls only this function, so a ceiling applied there
    // would bound a terminal and leave CI running with the defaults, which is
    // the 0.14.0 asymmetry this product deleted once already.
    let contract = crate::settings::ceilings(config).apply(contract);
    // **The bundles, applied here and in no other function, which is what makes
    // one call cover both arms.** `Config::plugins()` re-reads every directory a
    // `[[plugin]]` entry names; `Plugins::apply_to` merges their `[[agent]]`
    // definitions into the roster, extends `[[mcp]]` rather than assigning over
    // it, and sets `contract.plugins` — which is the field
    // `TaskContract::discover_skills` reads at run start to fold a bundle's
    // skills in, and the field `emit_plugins` iterates at step 0 to report what
    // loaded. Applied *after* `config.apply_to` on purpose: the merge has to see
    // the operator's own roster to merge into it.
    //
    // Nothing here can fail. A bundle that will not load is not an error, it is
    // a row in `Plugins::dropped()` — which is what stops one bad directory in a
    // shared configuration file from costing a whole team their session, and
    // which is why `/plugin` is the surface that reads it rather than this
    // function returning a `Result`.
    let plugins = config.plugins();
    let contract = plugins.apply_to(contract);
    // **The hooks, and the `is_empty` guard is not an optimisation.** io-harness
    // disables read speculation on any run carrying a `Hooks` at all — even one
    // holding no hooks — so attaching it unconditionally would quietly make every
    // session slower for the overwhelming majority of operators who have never
    // written a hook, with nothing failing and nothing on screen to connect the
    // loss to this release.
    //
    // `with_tool_hooks` is the *lifecycle* half — the `at = "before_tool"` tables
    // that can turn a tool call back. The `on = [...]` event tables arrive by a
    // different road entirely: the `Hooks` value is also an `Observer`, and the
    // driver puts it in the fan-out beside the interface's own. Both installs, or
    // one half of the file is accepted and silently never runs.
    //
    // **And not at all where the caller has no root, which is not a nicety.**
    // `io_harness::Hooks::new` **creates every `append` path it is given, empty,
    // as it is built** — so building one against `PathBuf::new()` resolves a
    // relative `append = "audit.jsonl"` against the *process working directory*
    // and leaves a stray empty file wherever `io` happened to be launched from.
    // [`server_notices`] calls this function with exactly that empty root, at
    // startup, purely to read the merged `[[mcp]]` and `[[lsp]]` lists back off a
    // throwaway contract — so before 0.20.0 the empty root cost nothing and now it
    // would cost a file in the operator's home. A contract with no root cannot run
    // a turn, so it has no use for hooks either.
    let hooks = if dir.as_os_str().is_empty() {
        None
    } else {
        let hooks = plugins.apply_to_hooks(config.hooks(), &dir);
        (!hooks.is_empty()).then_some(hooks)
    };
    let contract = match hooks {
        Some(hooks) => contract.with_tool_hooks(std::sync::Arc::new(hooks)),
        None => contract,
    };
    // **The operator's own criterion, here rather than in [`session`], and the
    // placement is the whole of F1.** A gate applied in the session's half would
    // hold a terminal to a standard and let CI run to green with nothing checked
    // — the 0.14.0 asymmetry again, and the one this release is least able to
    // afford, because the headless arm is where an unverified success does the
    // most damage.
    //
    // A refusal leaves the contract ungated on purpose. `/gates` refuses both
    // mistakes at the moment the file is written (F5), so a section that still
    // resolves to one here was hand-edited afterwards — and the honest response
    // to an unusable criterion is a run with no gate plus a notice, never a run
    // io-harness kills at start with `Error::Config` before the first billed
    // call. [`gate_notice`] is what puts that on screen.
    let contract = match criterion_for(config) {
        Some((criterion, reviewer)) => {
            let contract = contract.with_verification(criterion.verification());
            match reviewer {
                Some(reviewer) => contract.with_reviewer(reviewer),
                None => contract,
            }
        }
        None => contract,
    };
    // `[run] skills` has had its say, and `io exec` reads no other key that can
    // name one — so for the headless arm this is already the point after every
    // key. [`session`] calls this again once `[app.io-cli]` has had its own.
    resolve_skills(contract)
}

/// The criterion this configuration resolves to, with its reviewer already built.
///
/// `None` covers every case in which the run must not be gated: no section, a
/// section that names no criterion, a section that names two, and — the one that
/// matters most — a review criterion whose reviewer cannot be constructed. **A
/// `Verification::Review` reaching a contract with no reviewer beside it is
/// `Error::Config` at run start, on every turn**, so the criterion and its
/// reviewer are decided together here and either both go on or neither does.
fn criterion_for(
    config: &Config,
) -> Option<(
    crate::gates::Criterion,
    Option<std::sync::Arc<dyn io_harness::Reviewer>>,
)> {
    let gates = crate::settings::stored(config).0?.gates?;
    // The model the work is done by, which is what the self-review refusal is
    // decided against. An empty string means it is not knowable from here, and
    // `Settings::criterion` treats that as "cannot clash" rather than guessing.
    let working = config
        .provider_spec()
        .map(crate::provider::model_of)
        .unwrap_or_default();
    let criterion = gates.criterion(working).ok()??;
    match &criterion {
        crate::gates::Criterion::Review { reviewer, .. } => {
            let spec = config.provider_spec()?;
            let built = crate::reviewer::build(spec, reviewer).ok()?;
            Some((criterion.clone(), Some(built)))
        }
        _ => Some((criterion, None)),
    }
}

/// Why the configured criterion is not gating this run, if it is not.
///
/// Separate from [`configured`] because that function returns a contract and a
/// refusal is not a contract. A surface calls this to say the one sentence an
/// operator needs: the section is there, and it is not doing anything.
pub fn gate_notice(config: &Config) -> Option<String> {
    let gates = crate::settings::stored(config).0?.gates?;
    let working = config
        .provider_spec()
        .map(crate::provider::model_of)
        .unwrap_or_default();
    match gates.criterion(working) {
        Err(refusal) => Some(refusal.to_string()),
        Ok(Some(crate::gates::Criterion::Review { reviewer, .. })) => {
            match config.provider_spec() {
                None => Some(format!(
                "the gate asks {reviewer} to review the work, but no provider is configured to \
                 reach it — this turn is not gated"
            )),
                Some(spec) => crate::reviewer::build(spec, &reviewer)
                    .err()
                    .map(|why| format!("the gate's reviewer could not be built: {why}")),
            }
        }
        Ok(_) => None,
    }
}

/// The hooks a run should be observed by, for the caller that installs the
/// fan-out.
///
/// **The same value [`configured`] puts on the contract, built the same way and
/// deliberately built twice rather than threaded.** A `Hooks` is cheap — it is a
/// parsed `Vec<Hook>` and a directory — and the two installs happen at different
/// points in different functions with different lifetimes: the lifecycle half
/// goes onto the contract behind an `Arc` before the turn, the event half is
/// borrowed by the fan-out for exactly the turn's duration. Returning one value
/// for both would mean handing an `Arc` to a `&dyn Observer` site and keeping the
/// contract's copy alive across a `/config` reload that rebuilt the `Config` it
/// came from.
///
/// `None` where the file declares no hook, so the caller can leave the fan-out at
/// one observer and keep read speculation — the same guard [`configured`] makes,
/// made once here so the two cannot drift apart.
pub fn hooks(config: &Config, root: &std::path::Path) -> Option<io_harness::Hooks> {
    // The same empty-root guard [`configured`] makes, and for the same reason:
    // building a `Hooks` creates its `append` files, so a rootless caller would
    // leave them in the process working directory.
    if root.as_os_str().is_empty() {
        return None;
    }
    let hooks = config.plugins().apply_to_hooks(config.hooks(), root);
    (!hooks.is_empty()).then_some(hooks)
}

/// The skills directory this session will really hand the agent, for the one
/// surface that has to know it before a turn exists.
///
/// The palette is walked once at startup, so it cannot read the answer off a
/// contract — and it has always walked `[app.io-cli] skills` alone, which meant a
/// `[run] skills` reached the model while `/` listed nothing from it. 0.15.0 would
/// have widened that hole rather than left it, because the home default reaches
/// the model too. So the palette asks the same resolution the contract uses,
/// through a throwaway contract, rather than a second copy of the precedence that
/// could disagree with the first.
pub fn skills_dir(config: &Config, capabilities: &Capabilities, root: PathBuf) -> Option<PathBuf> {
    let contract = config.apply_to(TaskContract::workspace(String::new(), root));
    let contract = match &capabilities.skills {
        Some(dir) => contract.with_skills(dir.clone()),
        None => contract,
    };
    resolve_skills(contract).skills
}

/// `~/.io-cli/skills`, when there is something there to discover.
///
/// **The existence test is not caution, it is the whole of what makes this
/// default safe.** `Skills::discover` does not return early on a directory that
/// is not there — it returns `Error::Config("skills directory … does not exist")`
/// (`io-harness-0.66.0/src/skills.rs`), and `TaskContract::discover_skills`
/// propagates it from `run.rs` at run start, before the first completion. A
/// contract that named this directory unconditionally would therefore fail every
/// turn of every operator who has never made one, which is almost all of them.
/// Filtered, an operator with no `~/.io-cli/skills` gets a contract with no
/// skills directory, which is exactly the contract they got before this release.
fn default_skills() -> Option<PathBuf> {
    let dir = crate::home::path()?.join("skills");
    dir.is_dir().then_some(dir)
}

/// The skills directory a contract actually carries: whatever named it, with a
/// leading `~` expanded, or io-cli's own home where nothing named one.
///
/// **One expansion for two keys, applied after both have had their say.**
/// io-harness substitutes `${env:…}` and `${file:…}` and nothing else
/// (`io-harness-0.66.0/src/config.rs:1965` — there is no tilde branch anywhere in
/// it), so a `~` an operator wrote in `[run] skills` or `[app.io-cli] skills`
/// reaches `Skills::discover` as a directory whose name is one character long.
/// Expanding at each key instead would be two places to keep true and two places
/// for the next key to be forgotten in.
fn resolve_skills(contract: TaskContract) -> TaskContract {
    let dir = contract
        .skills
        .as_deref()
        .map(crate::home::expand)
        .or_else(default_skills);
    match dir {
        Some(dir) => contract.with_skills(dir),
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
///
/// **0.15.0 adds one thing an operator did not ask for, and it is conditional on
/// the operator's own disk rather than on their file.** A contract that no key
/// named a skills directory for carries `~/.io-cli/skills` — but only where that
/// directory exists, because `Skills::discover` fails the run on one that does
/// not — `Error::Config("skills directory … does not exist")`, propagated by
/// `TaskContract::discover_skills` before the first completion. An operator who
/// has never made the directory gets the
/// contract they got before, field for field.
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
    // **The one point after both keys have had their say.** `[run] skills` was
    // applied by `Config::apply_to` inside [`configured`] and `[app.io-cli]
    // skills` two lines up, so this is where a `~` either of them carries becomes
    // a home directory and where a session that named neither picks up
    // `~/.io-cli/skills`.
    resolve_skills(contract)
}
