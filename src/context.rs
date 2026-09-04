//! What the request that carried this turn actually contained.
//!
//! **Read from the request, and there is no other honest source.** io-harness
//! enumerates no context window: `run::prompts::compose` is `pub(super)`,
//! `workspace_tools()` is `pub(super)`, `Assembled` is built inside the step loop
//! and never returned, and `EventKind::PromptComposed` carries a *byte count* and
//! deliberately no prompt text. So every accessor that could answer "what is in
//! the window" is behind a wall, and everything io-cli could reach instead is a
//! reconstruction — a second opinion about a prompt this crate did not compose,
//! which would drift from the real one the first time the harness moved a
//! section.
//!
//! What is not behind a wall is the wire. Every completion this session makes
//! passes through the caller's own [`Provider`](io_harness::Provider), and
//! `CompletionRequest` has all-public fields: `system`, `user`, `messages` and
//! `tools`. io-cli already routes every provider through one seam —
//! [`crate::provider::WithProvider`] — so a decorator that implements `Provider`
//! by delegating and keeps a copy of the request on the way past is the whole
//! mechanism. See [`crate::provider::Watched`]; this module is what it fills and
//! what that is drawn as.
//!
//! # Why the sections are a partition and not a list
//!
//! F7 says the sections "sum to a total". A list of overlapping counts sums to
//! nothing an operator can act on: the repository's instructions are *inside* the
//! system block (`compose` frames them as a `repository_guidance` section), the
//! MCP tools are *inside* the tool catalogue (they are the members of
//! `request.tools` carrying [`io_harness::MCP_TOOL_PREFIX`]),
//! and the recalled memory is *inside* the conversation (`assemble` pushes the
//! `[memory]` block onto the front of the assembled text, which the user turn
//! carries). Reporting each whole would count some bytes three times and produce
//! a total larger than the request.
//!
//! So each of the seven sections is a **disjoint region of the request**, located
//! by the text io-harness actually wrote:
//!
//! * the repository's instructions are `contract.instructions` joined the way
//!   `instructions_section` joins them, found as a substring of `request.system`;
//! * the skill catalogue is the block `with_skill_catalog` appends, also inside
//!   `request.system` (0.37.0 — before that it was silently part of the row
//!   below, which is why a session's skills appeared to cost nothing);
//! * the system block is the rest of `request.system`;
//! * the MCP tools are the prefixed entries of `request.tools`, grouped by the
//!   server id the contract configured and named with the bundle that
//!   contributed them;
//! * the tool catalogue is the rest of `request.tools`;
//! * the recalled memory is the `[memory]` block located in the conversation;
//! * the conversation is the rest of it.
//!
//! # What a mask does to this page, and what it does not
//!
//! Withholding a tool changes **nothing** on the rows above. io-harness sends a
//! byte-identical catalogue to a masked and an unmasked turn — the tool array
//! sits ahead of the provider's cache breakpoint, so dropping a definition would
//! save its tokens once and pay a cache *write* on every later turn of the run,
//! and the harness argues that trade at length in `ToolMask`'s own
//! documentation. A mask in fact makes the request marginally **larger**: one
//! sentence naming the withheld tools, appended after the observations where it
//! costs no cache entry.
//!
//! This page must therefore never draw a mask as a saving, and the surface that
//! sets one says so in the other direction. A mask is a scoping and safety lever
//! — it decides what the agent may *do* — and the cost of a turn is not what it
//! is for.
//!
//! Summing them is then a statement about the request rather than an arithmetic
//! coincidence, which is exactly what makes a wrong section detectable: a number
//! that cannot be reconciled with the total beside it is the failure this design
//! exists to make visible.
//!
//! # Why every count is io-harness's estimator
//!
//! [`io_harness::context::estimate_tokens`] is public, and it is the same
//! function `ContextBudget` is spent in and `Assembled::est_tokens` reports. A
//! heuristic of io-cli's own would put a number beside a ceiling that was
//! measured in different units — the page would be arithmetically consistent with
//! itself and wrong about the only comparison it exists to make.
//!
//! The denominator is `ContextBudget::effective_tokens`, read off the
//! *contract* rather than off `ContextBudget::default()`, because the operator's
//! `[context]` configuration is already resolved onto the contract io-cli builds
//! and reading the file a second time here would be a second answer to a settled
//! question.

use std::collections::BTreeMap;
use std::sync::{Arc, Mutex};

use io_harness::context::estimate_tokens;
use io_harness::{
    CompletionRequest, Message, TaskContract, ToolMask, ToolSpec, MCP_TOOL_PREFIX, NAMESPACE,
};
use ratatui::text::{Line, Span};

use crate::theme::{Theme, Tone};

/// The head of the memory block, as `io_harness::context::render_notes` writes
/// it.
///
/// Matched on the opening sentence rather than on the whole paragraph: the
/// sentence is the part that identifies the block, and pinning the entire wording
/// would turn a copy-edit in io-harness into a section that silently reports
/// zero. The two forms are separate constants because a run with only
/// every-workspace notes renders the second head and never the first.
const MEMORY_HEAD: &str = "[memory] Notes you recorded on earlier runs";
/// The head of the every-workspace half of the same block (io-harness 0.56.0).
const MEMORY_GLOBAL_HEAD: &str = "[memory: every workspace] Notes kept for every workspace";

/// The head of the skill catalogue, as `run::prompts::with_skill_catalog` writes
/// it (0.37.0).
///
/// **Gated against io-harness's own source, unlike the two heads above.** A
/// constant matched against a dependency's prose has one failure mode — the
/// dependency copy-edits the sentence and this section silently reports zero,
/// which is indistinguishable from a session that configured no skills. The two
/// memory heads still carry that ceiling. This one does not:
/// `tests/context.rs` reads `run/prompts.rs` out of the pinned registry tree
/// through `support::harness_source` and fails if this string is not in it, so a
/// pin that moves the sentence breaks the build rather than the page.
///
/// Matched on the opening clause rather than on the whole paragraph, for the same
/// reason the memory heads are: the clause identifies the block, and the sentence
/// that follows it names a tool whose constant io-harness may re-spell.
pub const SKILLS_HEAD: &str = "Skills available to you";

/// The two openers of io-harness's planning directive, either of which ends the
/// skill catalogue (0.37.0).
///
/// **The directive is glued to the last catalogue line with no newline between
/// them, so a line scan cannot see the boundary.** `compose` builds the prompt as
/// `with_skill_catalog(..)` and then `out.push_str(&directive)`
/// (io-harness-0.76.0/src/run/prompts.rs:73-76); `Skills::catalog()` ends with its
/// last `- name: description` line carrying **no trailing newline**
/// (src/skills.rs:470-476); and `planning_directive` returns a string beginning
/// with a space (src/run/gate.rs:282-286). So with a plan gate registered — which
/// is every contained turn — the final line on the wire reads
/// `- bundle__skill: description Before you do anything else you must call …`,
/// still starts with `- `, and a naive scan swallows the whole directive.
///
/// The cost of missing this is not only a wrong total. `bundle_cost` charges
/// catalogue lines to bundles by prefix, so the directive's tokens land on
/// whichever bundle owns the alphabetically last skill, and that bundle's figure
/// on `/plugin` moves when the operator toggles `/plan` — for a reason that has
/// nothing to do with it.
///
/// Two constants because the directive's first sentence has two forms and they
/// share no usable prefix. Both are gated against the pinned `run/gate.rs` by
/// `tests/context.rs`, for the reason [`SKILLS_HEAD`] is.
pub const PLAN_DIRECTIVE_HEADS: [&str; 2] = [
    "If any part of this needs the repository written to",
    "Before you do anything else you must call",
];

/// One request, as it went out.
///
/// A copy rather than a borrow, because the request is consumed by the provider
/// call that carries it and the page is drawn some keystrokes later. Only the
/// four fields a window is made of are kept: this is not a replay log, and the
/// credential-bearing parts of a provider never come near it.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Request {
    /// `CompletionRequest::system`, whole — instructions still inside it.
    pub system: String,
    /// `CompletionRequest::tools`, exactly as offered.
    ///
    /// **The catalogue, not io-cli's belief about it.** A tool reaches this list
    /// because the harness put it on the wire, so a server that came up late, a
    /// plugin, and a tool this crate has never heard of are all present — which
    /// is the whole reason the page is read from the request.
    pub tools: Vec<ToolSpec>,
    /// The conversation this request carried, flattened to the text it is made
    /// of.
    ///
    /// `messages` when it is non-empty and `user` when it is not: io-harness
    /// fills `user` for every request and fills `messages` too once a run has
    /// driven a step, and counting both would count the same turn twice.
    pub conversation: String,
    /// How many messages that was — `0` for a flat request.
    pub messages: usize,
}

impl Request {
    /// The snapshot of one outgoing request.
    pub fn of(request: &CompletionRequest) -> Self {
        Self {
            system: request.system.clone(),
            tools: request.tools.clone(),
            conversation: if request.messages.is_empty() {
                request.user.clone()
            } else {
                request.messages.iter().map(message_text).collect()
            },
            messages: request.messages.len(),
        }
    }
}

/// The text one message puts on the wire.
///
/// The arguments and the results are included, not only the prose: a run whose
/// window is filling is filling with tool output, and a count that left it out
/// would report a small conversation on the turn that is about to overflow.
fn message_text(message: &Message) -> String {
    match message {
        Message::User(text) => text.clone(),
        Message::Assistant { text, calls } => {
            let mut out = text.clone().unwrap_or_default();
            for call in calls {
                out.push_str(&call.name);
                out.push_str(&call.arguments.to_string());
            }
            out
        }
        Message::Results(results) => results.iter().map(|r| r.content.as_str()).collect(),
        // Exhaustive, with no wildcard, for the reason `servers::transport` is:
        // `Message` is NOT `#[non_exhaustive]`, so a variant a later io-harness
        // adds breaks this build rather than being silently counted as nothing —
        // which on this page would be a window that under-reports itself.
    }
}

/// The one request the session has most recently made, shared with whatever drew
/// the page.
///
/// A handle rather than a global: `tests/` runs several sessions in one process,
/// and a process-wide slot would have them overwriting each other's window. It is
/// cheap to clone — the decorator holds one and the driver holds the other — and
/// the lock is held for the length of one `clone`, never across an await.
#[derive(Debug, Clone, Default)]
pub struct Seen(Arc<Mutex<Option<Request>>>);

impl Seen {
    /// Keep this request as the newest one.
    ///
    /// **Newest and not first.** A turn makes one completion per step and the
    /// window is what the *next* one would carry, so a page drawn after a turn
    /// should describe the request that ended it. Keeping the first would
    /// describe a window several tool results out of date.
    pub fn record(&self, request: &CompletionRequest) {
        // `unwrap_or_else` on the poison rather than `expect`: a panic in some
        // other thread must not take out the page that would explain it.
        let mut slot = self.0.lock().unwrap_or_else(|e| e.into_inner());
        *slot = Some(Request::of(request));
    }

    /// The newest request, or `None` where no turn has run yet.
    pub fn latest(&self) -> Option<Request> {
        self.0.lock().unwrap_or_else(|e| e.into_inner()).clone()
    }

    /// Forget it, for a `/clear`, a `/resume` or a rewind.
    ///
    /// The same hole `Status::forget_run` and `servers::Observed::forget` close:
    /// per-run state that outlives the run describes a conversation the operator
    /// is no longer in.
    pub fn forget(&self) {
        *self.0.lock().unwrap_or_else(|e| e.into_inner()) = None;
    }
}

/// One region of the request, with what it cost.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Section {
    /// What it is, in the words F7 names it by.
    pub label: String,
    /// What it is made of — a count of files, of tools, of notes, of messages.
    pub detail: String,
    /// Its size, by [`estimate_tokens`] over the region's own text.
    pub tokens: u64,
}

/// The label of the skill-catalogue section.
///
/// A constant because three surfaces now name it — this page, `/plugin`'s cost
/// column and the tests that assert they agree — and a spelling that lives in
/// three string literals is a spelling that drifts in two of them.
pub const SKILL_CATALOGUE: &str = "skill catalogue";

/// What each MCP server cost this request, by the id the surfaces hold.
///
/// Keyed on the **wire** id, not the drawn one, because `/mcp` addresses a server
/// by `Server::id` and a lookup that needed translating first would put the
/// translation in two places. Drawing is the caller's business.
///
/// A server the contract configured but which offered no tool to this request is
/// absent rather than zero: it means "this server has not been on a wire yet",
/// which a zero would misreport as "this server is free".
pub fn server_cost(seen: &Request, contract: &TaskContract) -> BTreeMap<String, u64> {
    let mut out = BTreeMap::new();
    for tool in &seen.tools {
        if !tool.name.starts_with(MCP_TOOL_PREFIX) {
            continue;
        }
        *out.entry(server_of(&tool.name, contract)).or_insert(0) +=
            estimate_tokens(&tool_text(tool));
    }
    out
}

/// What each bundle cost this request, by plugin id.
///
/// **Two of the three costing kinds, and the third is named rather than guessed
/// at.** Of the seven things a bundle contributes, only skills, agents and MCP
/// servers reach the wire — hooks, templates, a declared binary and a policy layer
/// cost nothing at all, which is the fact an operator least expects and most needs.
/// This sums a bundle's MCP tools and its catalogue lines. An agent definition is
/// **not** included: io-harness composes a roster into the system block with no
/// marker this module can locate, so a number for it would be invented. That is
/// stated as a limitation rather than approximated.
///
/// A bundle's skills are namespaced `<plugin>__<skill>` before they reach the
/// catalogue, so a catalogue line attributes itself and no second lookup is needed.
pub fn bundle_cost(seen: &Request, contract: &TaskContract) -> BTreeMap<String, u64> {
    let per_server = server_cost(seen, contract);
    let mut out = BTreeMap::new();
    for plugin in contract.plugins.iter() {
        let tools: u64 = plugin
            .mcp_servers()
            .iter()
            .filter_map(|s| per_server.get(&s.id))
            .sum();
        let skills: u64 = skills_in(&seen.system)
            .into_iter()
            .flat_map(str::lines)
            .filter(|line| {
                line.strip_prefix("- ")
                    .and_then(|rest| rest.strip_prefix(plugin.id()))
                    .is_some_and(|rest| rest.starts_with(NAMESPACE))
            })
            .map(estimate_tokens)
            .sum();
        // Present at zero, unlike a server: a bundle contributing only hooks is
        // genuinely free, and that is the row worth drawing rather than hiding.
        out.insert(plugin.id().to_string(), tools + skills);
    }
    out
}

/// What `/context withhold` and `/context allow` do to the mask, and what is said
/// about it (0.37.0).
///
/// **A function of the mask rather than a method on the driver**, for the reason
/// [`crate::contract::buying`] gives about itself: nothing under `tests/` links
/// `src/main.rs`, so a transition written there could be neither asserted nor
/// sabotaged. The driver takes the new mask and prints the line.
///
/// The returned sentence is where this release is most able to lie, so it is
/// written once, here, and said in the direction that is true: withholding costs a
/// little more rather than saving anything. A reader who is told a tool was
/// withheld will assume the request got smaller unless told otherwise, and the
/// assumption is the one the roadmap this release was planned from also made.
/// `offered` is the catalogue the last request actually carried, and it is a
/// parameter because a name that answers to nothing is a mask that does nothing.
/// io-harness **keeps** an unknown name rather than rejecting it, deliberately, so
/// that a mask written against one build stays portable to another with different
/// cargo features — which means the harness cannot warn and this is the only place
/// that can. Withholding a misspelling therefore succeeds, silently, and leaves the
/// tool fully callable: `mask_gate` matches on the exact name.
///
/// So the name is still added — refusing it here would break the portability the
/// harness designed for — and the operator is told it matches nothing on the wire.
/// The `Allow` arm has always refused to be a silent no-op; this is the same
/// courtesy on the direction where the cost of not having it is a safety lever the
/// operator believes is on.
pub fn withhold(
    mask: &io_harness::ToolMask,
    said: &crate::commands::Masked,
    offered: &[String],
) -> (ToolMask, String) {
    use crate::commands::Masked;

    let mut names: Vec<String> = mask.names().map(str::to_string).collect();
    match said {
        Masked::Withhold(tool) => {
            if names.iter().any(|n| n == tool) {
                let line = format!("{tool} is already withheld until you allow it again");
                return (ToolMask::withholding(names), line);
            }
            names.push(tool.clone());
            // **Only when a catalogue is known.** Before the first turn `offered`
            // is empty, and warning that every name is unrecognised when nothing
            // has been offered yet would be noise that trains the reader to ignore
            // the warning that matters.
            let unknown = !offered.is_empty() && !offered.iter().any(|name| name == tool);
            let line = if unknown {
                format!(
                    "{tool} is withheld, but no tool of that name was in the last request — \
                     check the spelling against /context, which lists what was offered. \
                     A mask entry that matches nothing withholds nothing. {} withheld now.",
                    names.len()
                )
            } else {
                // Said in full every time rather than only on the first withhold:
                // the cost direction is the thing an operator is most likely to
                // have wrong, and a note that appears once is a note most people
                // never see.
                format!(
                    "{tool} is withheld until you allow it again — the model is still offered \
                     it and it still costs its definition, and calling it will be refused \
                     before anything starts. {} withheld now.",
                    names.len()
                )
            };
            (ToolMask::withholding(names), line)
        }
        Masked::Allow(tool) => {
            let before = names.len();
            names.retain(|n| n != tool);
            let line = if names.len() == before {
                // Never silently a no-op: an operator who mistyped a name and got
                // nothing back would believe the tool was allowed again.
                format!("{tool} was not withheld, so nothing changed")
            } else {
                format!("{tool} may be called again")
            };
            (ToolMask::withholding(names), line)
        }
        // Every tool named, because this is the one transition that changes
        // several things at once and an operator who typed it one word early is
        // owed the list of what came back.
        Masked::Clear => {
            let line = if names.is_empty() {
                "nothing was withheld".to_string()
            } else {
                format!("no tool is withheld now — {} may be called again", {
                    names.sort();
                    names.join(", ")
                })
            };
            (ToolMask::none(), line)
        }
        Masked::NoTool => (
            ToolMask::withholding(names),
            "withhold needs a tool to withhold — /context withhold docx_write. \
             /context lists what the model was offered."
                .to_string(),
        ),
        Masked::Unknown(word) => (
            ToolMask::withholding(names),
            format!("{word} is not a verb here — /context takes withhold or allow"),
        ),
    }
}

/// The names the last request offered, for [`withhold`] to check a spelling
/// against.
///
/// Empty before any turn has run, which [`withhold`] reads as "no catalogue is
/// known" rather than as "nothing is offered" — the two must not collapse, or
/// every name typed at a fresh prompt would be reported as a misspelling.
pub fn offered(seen: Option<&Request>) -> Vec<String> {
    seen.map(|request| request.tools.iter().map(|t| t.name.clone()).collect())
        .unwrap_or_default()
}

/// What the mask is, drawn for the page.
///
/// Absent entirely when nothing is withheld, because a row saying "nothing is
/// withheld" on every `/context` is a row that trains a reader to skip the place
/// the answer appears.
pub fn withheld_line(mask: &io_harness::ToolMask, dash: &str) -> Option<String> {
    if mask.is_empty() {
        return None;
    }
    let names: Vec<&str> = mask.names().collect();
    // **"until you allow it again", never "for the next turn".** io-harness
    // documents `ToolMask` as a request about one turn, and it is — but io-cli
    // re-applies this one to every turn until `/context allow`, so the harness's
    // wording describes the contract's lifetime and would describe the operator's
    // posture wrongly. A reader told "the next turn" reasonably stops thinking
    // about it, which is the 0.26.0 `/effort` defect in the opposite direction.
    Some(format!(
        "withheld: {} {dash} refused until you allow it again. The catalogue above is \
         unchanged: withholding costs one extra sentence and saves nothing.",
        names.join(", ")
    ))
}

/// The text one tool occupies on the wire.
///
/// Name, description and schema, because all three are sent. A catalogue counted
/// by name alone reports a few dozen tokens for a toolbox that costs thousands,
/// and the schema is where nearly all of it is.
fn tool_text(tool: &ToolSpec) -> String {
    format!("{}\n{}\n{}", tool.name, tool.description, tool.parameters)
}

/// The server a namespaced tool came from.
///
/// Resolved against the ids the contract configured rather than by splitting on
/// `__`, because a server id may itself contain the separator and a split would
/// then name a server nobody configured. A prefixed tool matching no configured
/// id keeps its full name as the group: it is still an MCP tool, and inventing an
/// id for it would be worse than saying the one on the wire.
fn server_of(tool: &str, contract: &TaskContract) -> String {
    let bare = match tool.strip_prefix(MCP_TOOL_PREFIX) {
        Some(bare) => bare,
        None => return String::new(),
    };
    // **The LONGEST matching id, not the first.** `contract.mcp` lists the
    // operator's own `[[mcp]]` entries before the plugin servers `Plugins::apply_to`
    // appends, and a bare id is validated by nobody — so `github` and
    // `github__enterprise` can both be configured, and a first-match loop charges
    // every enterprise tool to `github` while `github__enterprise` reads
    // "not yet on a request" for the life of the session. The same shape fires when
    // an operator's bare `docs` collides with a bundle `docs` contributing `search`.
    if let Some(server) = contract
        .mcp
        .iter()
        .filter(|server| {
            bare.strip_prefix(&server.id)
                .is_some_and(|r| r.starts_with("__"))
        })
        .max_by_key(|server| server.id.len())
    {
        return server.id.clone();
    }
    bare.split_once("__")
        .map(|(server, _)| server.to_string())
        .unwrap_or_else(|| bare.to_string())
}

/// The region of `haystack` occupied by the repository's instructions.
///
/// `instructions_section` joins them with a blank line and drops them verbatim
/// into the system block, so the join *is* the needle. `None` means this request
/// did not carry them — a contract with no instructions, or a system prompt the
/// operator replaced — and the page says so rather than reporting a zero that
/// looks like an empty file.
fn instructions_in<'a>(haystack: &'a str, instructions: &[String]) -> Option<&'a str> {
    if instructions.is_empty() {
        return None;
    }
    let needle = instructions.join("\n\n");
    let at = haystack.find(&needle)?;
    Some(&haystack[at..at + needle.len()])
}

/// The region of `haystack` occupied by the recalled memory block.
///
/// Located by io-harness's own head sentence and ended at the first line that is
/// neither a head nor a note. `render_notes` writes exactly three shapes of line
/// — the two heads and `- key: value  (step n)`, with an elision line that is
/// also a `- ` — and the observations that follow it are emitted whole or as
/// bracketed stubs, so the first line that is not one of those is where the block
/// stops.
///
// ponytail: a line-shape scan, not a parser. Its ceiling is an observation whose
// first line begins with "- ", which would be counted as memory rather than as
// conversation; both are inside the same request, so the total is unaffected and
// only the split between two rows moves. The upgrade path is an io-harness
// accessor for `Assembled`, which is what would remove the scan entirely.
fn memory_in(haystack: &str) -> Option<&str> {
    let at = [MEMORY_HEAD, MEMORY_GLOBAL_HEAD]
        .iter()
        .filter_map(|head| haystack.find(head))
        .min()?;
    let mut end = at;
    for line in haystack[at..].split_inclusive('\n') {
        let trimmed = line.trim_end_matches('\n');
        let is_block = trimmed.starts_with(MEMORY_HEAD)
            || trimmed.starts_with(MEMORY_GLOBAL_HEAD)
            || trimmed.starts_with("- ");
        if !is_block {
            break;
        }
        end += line.len();
    }
    Some(&haystack[at..end])
}

/// The region of `haystack` occupied by the skill catalogue (0.37.0).
///
/// `with_skill_catalog` appends one paragraph naming what a skill is and how to
/// read one, then a newline, then `Skills::catalog()` — which is one `- name:
/// description` line per skill and nothing else. So the block is the head
/// paragraph plus the `- ` lines that follow it, and it ends at the first line
/// that is neither. That is the shape [`memory_in`] already scans for, and it is
/// deliberately the same shape rather than a second technique.
///
/// `None` means this request carried no catalogue — a session with no skills
/// configured — and the page says so rather than drawing a zero, because a zero
/// here is exactly what a drifted needle produces and the two must stay
/// distinguishable.
///
/// **Why this is not `Skills::catalog()` used as the needle.** That was the plan
/// this release was specified with. It needs a `Skills` value, and
/// `TaskContract::skills` is an `Option<PathBuf>` — a directory — so io-cli would
/// have to run `Skills::discover` a second time to build one. That is a second
/// walk of the operator's disk on every draw, and worse, a second opinion about a
/// set the run already resolved: the two could disagree, and the page would report
/// the one that is not on the wire. Locating io-harness's own output is the read
/// this module is built on everywhere else.
fn skills_in(haystack: &str) -> Option<&str> {
    let at = haystack.find(SKILLS_HEAD)?;
    let mut end = at;
    for (n, line) in haystack[at..].split_inclusive('\n').enumerate() {
        let trimmed = line.trim_end_matches('\n');
        // The head paragraph is one line; every line after it is a catalogue
        // entry. A catalogue with no entries cannot occur — `with_skill_catalog`
        // returns `base` untouched when the set is empty.
        if n > 0 && !trimmed.starts_with("- ") {
            break;
        }
        end += line.len();
    }
    let block = &haystack[at..end];
    // And cut the planning directive back off the last line, where io-harness
    // appended it with no newline. See [`PLAN_DIRECTIVE_HEADS`] — without this the
    // row over-reports by the directive's length and one bundle is charged for it.
    let cut = PLAN_DIRECTIVE_HEADS
        .iter()
        .filter_map(|head| block.find(head))
        .min()
        .map(|at| block[..at].trim_end().len())
        .unwrap_or(block.len());
    Some(&block[..cut])
}

/// The bundle that contributed `server`, or `None` for one the operator declared.
///
/// `Plugins::apply_to` extends `contract.mcp` with every loaded plugin's servers
/// and stores itself on `contract.plugins`, so the contract already knows this and
/// nothing needs resolving. Matched on the id rather than on the separator: a
/// plugin id cannot contain `__` (io-harness validates it to
/// `[a-z0-9][a-z0-9-]{0,31}`) but a bare `[[mcp]]` id from `io.toml` is validated
/// by nobody and may.
///
/// This reads the `plugins` **field**, never `Config::plugins()`, which is a full
/// parse of every declared manifest and is confined to `crate::resolved` by an
/// exact-path gate in `tests/dependencies.rs`.
fn bundle_of<'a>(server: &str, contract: &'a TaskContract) -> Option<&'a str> {
    contract
        .plugins
        .iter()
        .find(|plugin| plugin.mcp_servers().iter().any(|s| s.id == server))
        .map(|plugin| plugin.id())
}

/// The seven sections of one request, in the order the page draws them.
///
/// Every one is a region of `seen` and no two overlap, so
/// [`total`] is a statement about the request rather than a sum of unrelated
/// numbers. The MCP half is one row per server the request actually offered tools
/// from — plus one row saying so when it offered none, because a section that
/// disappears when it is empty is a section a reader cannot tell from a section
/// that was never drawn.
pub fn sections(seen: &Request, contract: &TaskContract) -> Vec<Section> {
    let mut out = Vec::new();

    // 1, 2 and 3 — the system block, split at the guidance io-cli put in it and
    // at the skill catalogue io-harness appended to it. Both are subtracted from
    // the block rather than reported beside it, or the same bytes would be
    // counted twice and the total would exceed the request.
    let guidance = instructions_in(&seen.system, &contract.instructions);
    let guidance_tokens = guidance.map(estimate_tokens).unwrap_or(0);
    let skills = skills_in(&seen.system);
    let skill_tokens = skills.map(estimate_tokens).unwrap_or(0);
    out.push(Section {
        label: "system block".into(),
        detail: "what io-harness composed for this turn".into(),
        tokens: estimate_tokens(&seen.system)
            .saturating_sub(guidance_tokens)
            .saturating_sub(skill_tokens),
    });
    out.push(Section {
        label: "repository instructions".into(),
        detail: match guidance {
            Some(_) => format!(
                "{} file(s), carried inside the system block",
                contract.instructions.len()
            ),
            None => "none in this request".into(),
        },
        tokens: guidance_tokens,
    });
    out.push(Section {
        label: SKILL_CATALOGUE.into(),
        detail: match skills {
            // Counted off the block on the wire, not off the skills io-cli can
            // see on disk: a skill added since the last turn is not in this
            // request, and the page is about this request.
            //
            // One line per skill, name and description, never a body — a body is
            // loaded on demand by `read_skill`, which is why a session with
            // twenty skills pays for twenty *lines* here and not twenty files.
            Some(block) => format!(
                "{} skill(s) named, bodies read on demand",
                block.lines().filter(|l| l.starts_with("- ")).count()
            ),
            None => "no skill was named to this request".into(),
        },
        tokens: skill_tokens,
    });

    // 3 and 4 — the catalogue, split at the MCP prefix.
    let (from_mcp, workspace): (Vec<&ToolSpec>, Vec<&ToolSpec>) = seen
        .tools
        .iter()
        .partition(|tool| tool.name.starts_with(MCP_TOOL_PREFIX));
    out.push(Section {
        label: "tool catalogue".into(),
        detail: format!("{} tool(s) offered", workspace.len()),
        tokens: workspace
            .iter()
            .map(|tool| estimate_tokens(&tool_text(tool)))
            .sum(),
    });
    // Grouped in the order the request listed them, so the page reads in the
    // order the model does. A `BTreeMap` would sort them, which is a second
    // ordering of a list that already has one.
    let mut servers: Vec<(String, Vec<&ToolSpec>)> = Vec::new();
    for tool in from_mcp.iter().copied() {
        let id = server_of(&tool.name, contract);
        match servers.iter_mut().find(|(name, _)| *name == id) {
            Some((_, list)) => list.push(tool),
            None => servers.push((id, vec![tool])),
        }
    }
    if servers.is_empty() {
        out.push(Section {
            label: "mcp tools".into(),
            detail: "no server offered a tool to this request".into(),
            tokens: 0,
        });
    }
    for (id, tools) in &servers {
        // A bundle's server is drawn in the spelling an operator reads and named
        // with the bundle that brought it, because "where did this come from" is
        // the question a cost row exists to answer. `naming::display` is applied
        // only to a server a bundle contributed: a plugin id cannot carry the
        // separator, so the translation is unambiguous there, while a bare
        // `[[mcp]]` id is validated by nobody and may carry one legitimately.
        let bundle = bundle_of(id, contract);
        let shown = match bundle {
            Some(_) => crate::naming::display(id),
            None => id.clone(),
        };
        out.push(Section {
            label: format!("mcp {shown}"),
            detail: match bundle {
                Some(plugin) => format!(
                    "{} tool(s) offered, from the {} bundle",
                    tools.len(),
                    crate::naming::display(plugin)
                ),
                None => format!("{} tool(s) offered", tools.len()),
            },
            tokens: tools
                .iter()
                .map(|tool| estimate_tokens(&tool_text(tool)))
                .sum(),
        });
    }

    // 5 and 6 — the conversation, split at the memory block.
    let memory = memory_in(&seen.conversation);
    let memory_tokens = memory.map(estimate_tokens).unwrap_or(0);
    out.push(Section {
        label: "recalled memory".into(),
        detail: match memory {
            // Counted off the block rather than off `Store::memory_recalls`: the
            // page is about what the request carried, and the store also holds
            // the notes a fit elided.
            Some(block) => format!(
                "{} note(s) carried",
                block.lines().filter(|l| l.starts_with("- ")).count()
            ),
            None => "no note was recalled into this request".into(),
        },
        tokens: memory_tokens,
    });
    out.push(Section {
        label: "conversation".into(),
        detail: match seen.messages {
            0 => "one flat turn".into(),
            n => format!("{n} message(s)"),
        },
        tokens: estimate_tokens(&seen.conversation).saturating_sub(memory_tokens),
    });

    out
}

/// What the sections come to.
pub fn total(sections: &[Section]) -> u64 {
    sections.iter().map(|section| section.tokens).sum()
}

/// The window this contract declares, in tokens.
///
/// `remaining` is what is left of `[run] max_tokens` for this turn, which is what
/// makes the ceiling move: with a run budget the assembled section takes a share
/// of what is *unspent*, so a run running low reports a smaller window rather
/// than the flat maximum it can no longer afford.
pub fn window(contract: &TaskContract, remaining: Option<u64>) -> u64 {
    contract.context.effective_tokens(remaining)
}

/// The page, committed into the scrollback.
///
/// A page and never a modal: the viewport does not grow, and the
/// terminal's own search, selection and copy already work on everything above it
/// — the same answer `/status` and `/expand` give. `seen` is `None` before the
/// first turn, and that is said in a sentence rather than drawn as an empty
/// table: a window nothing has been sent through has no contents, and zeroes
/// would read as an empty one.
pub fn committed(
    seen: Option<&Request>,
    contract: &TaskContract,
    remaining: Option<u64>,
    // What the next turn may not call. Drawn on this page because this is where
    // the tools are named, and an operator who withheld one an hour ago should
    // not have to remember it — but drawn only when there is one, so the row is
    // information rather than furniture.
    mask: &io_harness::ToolMask,
    theme: &Theme,
    width: u16,
) -> Vec<Line<'static>> {
    let rule = theme.glyphs.rule;
    let dash = theme.glyphs.dash;
    let mut lines = vec![Line::from(Span::styled(
        format!("{rule}{rule}{rule} context"),
        theme.style(Tone::Accent),
    ))];

    let room = width as usize;
    // Every row folded rather than fitted, for the reason `crate::status`'s page
    // is: this owns as many rows as it needs, and the characters a fitter would
    // drop are the tool names and the numbers.
    let mut push = |text: String, tone: Tone| {
        for row in wrapped(&text, room) {
            lines.push(Line::from(Span::styled(row, theme.style(tone))));
        }
    };

    match seen {
        None => push(
            format!(
                "nothing has been sent yet {dash} the window is read from the request \
                 that carries a turn, so /context answers once one has run"
            ),
            Tone::Muted,
        ),
        Some(seen) => {
            let sections = sections(seen, contract);
            for section in &sections {
                push(
                    format!(
                        "{}: {} tokens {dash} {}",
                        section.label, section.tokens, section.detail
                    ),
                    Tone::Normal,
                );
            }
            let total = total(&sections);
            let window = window(contract, remaining);
            push(
                format!(
                    "total: {total} tokens of {window} {dash} the window this contract declares"
                ),
                Tone::Normal,
            );

            // The catalogue itself, named. The counts above say how much it
            // costs; this says what the model was actually handed, which is the
            // half an operator cannot get from anywhere else — a tool here that
            // io-cli never registered is a tool that arrived from a server or a
            // plugin, and it is on the wire either way.
            if seen.tools.is_empty() {
                push("offered: no tool at all".into(), Tone::Muted);
            } else {
                let names: Vec<&str> = seen.tools.iter().map(|t| t.name.as_str()).collect();
                push(format!("offered: {}", names.join(", ")), Tone::Muted);
            }
        }
    }

    // Below the catalogue and not above it, because it is a statement *about* the
    // list of tools directly above — and drawn whether or not a turn has run, so
    // that a mask set at the prompt before the first message is visible rather
    // than silently in force.
    if let Some(line) = withheld_line(mask, dash) {
        push(line, Tone::Refused);
    }

    lines.push(Line::from(Span::styled(
        format!("{rule}{rule}{rule} context ends"),
        theme.style(Tone::Accent),
    )));
    lines
}

/// `text` as rows no wider than `width`, indented two then four.
///
/// **The body moved to [`crate::page::folded`] in 0.22.0.** This was the second of
/// two copies of the same twenty lines — `crate::status` had the other, under the
/// name `folded`, taking as arguments the two indents this hard-coded — and
/// `/cost` and `/stats` would have made four. The two numbers stay here because
/// they are this surface's decision; the folding is not.
fn wrapped(text: &str, width: usize) -> Vec<String> {
    crate::page::folded(text, width, 2, 4)
}
