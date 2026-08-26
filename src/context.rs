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
//! `request.tools` carrying [`MCP_TOOL_PREFIX`](io_harness::MCP_TOOL_PREFIX)),
//! and the recalled memory is *inside* the conversation (`assemble` pushes the
//! `[memory]` block onto the front of the assembled text, which the user turn
//! carries). Reporting each whole would count some bytes three times and produce
//! a total larger than the request.
//!
//! So each of the six sections is a **disjoint region of the request**, located
//! by the text io-harness actually wrote:
//!
//! * the repository's instructions are `contract.instructions` joined the way
//!   `instructions_section` joins them, found as a substring of `request.system`;
//! * the system block is the rest of `request.system`;
//! * the MCP tools are the prefixed entries of `request.tools`, grouped by the
//!   server id the contract configured;
//! * the tool catalogue is the rest of `request.tools`;
//! * the recalled memory is the `[memory]` block located in the conversation;
//! * the conversation is the rest of it.
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
//! The denominator is [`ContextBudget::effective_tokens`], read off the
//! *contract* rather than off `ContextBudget::default()`, because the operator's
//! `[context]` configuration is already resolved onto the contract io-cli builds
//! and reading the file a second time here would be a second answer to a settled
//! question.

use std::sync::{Arc, Mutex};

use io_harness::context::estimate_tokens;
use io_harness::{CompletionRequest, Message, TaskContract, ToolSpec, MCP_TOOL_PREFIX};
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
    for server in &contract.mcp {
        if bare
            .strip_prefix(&server.id)
            .is_some_and(|r| r.starts_with("__"))
        {
            return server.id.clone();
        }
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

/// The six sections of one request, in the order the page draws them.
///
/// Every one is a region of `seen` and no two overlap, so
/// [`total`] is a statement about the request rather than a sum of unrelated
/// numbers. The MCP half is one row per server the request actually offered tools
/// from — plus one row saying so when it offered none, because a section that
/// disappears when it is empty is a section a reader cannot tell from a section
/// that was never drawn.
pub fn sections(seen: &Request, contract: &TaskContract) -> Vec<Section> {
    let mut out = Vec::new();

    // 1 and 2 — the system block, split at the guidance io-cli put in it.
    let guidance = instructions_in(&seen.system, &contract.instructions);
    let guidance_tokens = guidance.map(estimate_tokens).unwrap_or(0);
    out.push(Section {
        label: "system block".into(),
        detail: "what io-harness composed for this turn".into(),
        tokens: estimate_tokens(&seen.system).saturating_sub(guidance_tokens),
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
        out.push(Section {
            label: format!("mcp {id}"),
            detail: format!("{} tool(s) offered", tools.len()),
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
/// A page and never a modal: the viewport is four rows and cannot grow, and the
/// terminal's own search, selection and copy already work on everything above it
/// — the same answer `/status` and `/expand` give. `seen` is `None` before the
/// first turn, and that is said in a sentence rather than drawn as an empty
/// table: a window nothing has been sent through has no contents, and zeroes
/// would read as an empty one.
pub fn committed(
    seen: Option<&Request>,
    contract: &TaskContract,
    remaining: Option<u64>,
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

    lines.push(Line::from(Span::styled(
        format!("{rule}{rule}{rule} context ends"),
        theme.style(Tone::Accent),
    )));
    lines
}

/// `text` as rows no wider than `width`, indented two then four.
///
/// **Folded and never fitted**, for the reason `crate::status`'s own helper is: a
/// committed surface owns as many rows as it needs, and the characters a fitter
/// would drop here are tool names — which is to say the answer. A word longer
/// than its row is split rather than allowed to overflow, so eighty columns holds.
fn wrapped(text: &str, width: usize) -> Vec<String> {
    let mut rows: Vec<String> = Vec::new();
    let mut indent = 2usize;
    let mut row = " ".repeat(indent);
    let mut used = 0usize;
    for word in text.split_whitespace() {
        let mut word = word;
        loop {
            // At least one cell, so a terminal narrower than the indent still
            // makes progress instead of looping forever.
            let room = width.saturating_sub(indent).max(1);
            let space = usize::from(used > 0);
            let length = word.chars().count();
            if used + space + length <= room {
                if space == 1 {
                    row.push(' ');
                }
                row.push_str(word);
                used += space + length;
                break;
            }
            if used > 0 {
                rows.push(std::mem::take(&mut row));
                indent = 4;
                row = " ".repeat(indent);
                used = 0;
                continue;
            }
            let head: String = word.chars().take(room).collect();
            word = &word[head.len()..];
            row.push_str(&head);
            rows.push(std::mem::take(&mut row));
            indent = 4;
            row = " ".repeat(indent);
        }
    }
    if used > 0 || rows.is_empty() {
        rows.push(row);
    }
    rows
}
