//! io-harness events, in the vocabulary an ACP client understands.
//!
//! [`crate::acp`] is the wire. This is the translation: one io-harness
//! [`io_harness::RunEvent`] in, at most one ACP `session/update` notification
//! out, plus the `stopReason` a `session/prompt` answers with.
//!
//! # Why this is a table and not only a match
//!
//! `EventKind` is `#[non_exhaustive]`, so the compiler can never prove a match
//! over it exhaustive and a `_ => {}` arm is mandatory. That arm is the defect
//! this module is most likely to ship: an editor that silently never shows that
//! the agent edited a file, behind a green suite, because a variant the harness
//! added went to the wildcard. It has happened here in a smaller way already —
//! io-harness 0.73.0 gave `read_skill` an optional field and started announcing
//! it in preference to the old one, and io-cli's existing test stayed green over
//! garbage output.
//!
//! So the same answer [`crate::triage`] reached applies, for the same reason:
//! [`MAPPING`] is keyed by the snake-case wire name, `tests/acp.rs` reads the
//! locked harness's own enum and fails **by name** when the two sets differ, and
//! a kind that reaches no `session/update` has to say so in the table with its
//! reason. A no-op that is written down is a decision the next release can check;
//! a no-op that falls through a wildcard is a silence nobody chose.
//!
//! `src/exec.rs` deliberately does the opposite — it forwards every event
//! verbatim rather than matching, and its own comment says why. That is right for
//! a raw NDJSON stream, whose contract is io-harness's shape. It is not available
//! here, because ACP has a fixed vocabulary and a translation is exactly what a
//! client is asking for.
//!
//! # What the client is told, and what it is not
//!
//! ACP's `session/update` carries agent output, agent thinking, tool calls and
//! plans. It has no vocabulary for a sandbox being created, a spend draw, a cache
//! mark or a bundle loading — and inventing one by pushing those through
//! `agent_message_chunk` would put io-cli's internal bookkeeping into the
//! conversation an operator reads, which is the opposite of a translation. Those
//! kinds are no-ops here and reach a machine consumer through `io exec --json`
//! and the durable trace instead.

use io_harness::{EventKind, RunEvent};
// One name per line rather than a brace list. `tests/dependencies.rs` forbids
// `use serde_json::{` everywhere — the permitted modules included — because
// spelling the name around is the same act as writing it, and a parse hidden
// behind an alias appears in no sweep. This module deserializes nothing at all;
// it takes the macro and the type and keeps the import shape the gate requires.
use serde_json::json;
use serde_json::Value;

/// What a kind becomes on the wire.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Update {
    /// `session/update` with `sessionUpdate: "agent_message_chunk"` — the
    /// agent's answer, as the operator reads it.
    MessageChunk,
    /// `session/update` with `sessionUpdate: "agent_thought_chunk"` — reasoning
    /// the provider returned separately. A distinct channel in ACP precisely so
    /// a client can fold or hide it, which is why it is not a message chunk.
    ThoughtChunk,
    /// `session/update` with `sessionUpdate: "tool_call"` — a call beginning.
    ToolCall,
    /// `session/update` with `sessionUpdate: "tool_call_update"` — a call whose
    /// status changed.
    ToolCallUpdate,
    /// `session/update` with `sessionUpdate: "plan"`.
    Plan,
    /// Nothing goes to the client. The reason column says where the fact goes
    /// instead, and a row here without one is a silence nobody chose.
    None,
}

/// Every kind the locked io-harness declares, in its own declaration order, with
/// what it becomes and why.
///
/// The order is the enum's rather than alphabetical, so this table can be read
/// down the side of `observe.rs` when the pin moves — the same convention
/// [`crate::triage::TRIAGE`] keeps, for the same reason.
pub const MAPPING: &[(&str, Update, &str)] = &[
    (
        "started",
        Update::None,
        "the client already knows: it sent the prompt that started this",
    ),
    (
        "recovery_paused",
        Update::ToolCallUpdate,
        "a call that failed and is being retried is a call whose status changed",
    ),
    (
        "step",
        Update::None,
        "a step boundary is io-harness's unit of work and has no ACP counterpart; \
         what happened in it is already reported by the calls and chunks inside it",
    ),
    (
        "step_attributed",
        Update::None,
        "a per-step timing breakdown, which ACP has no vocabulary for; `io exec \
         --json` and `Store::step_attributions` carry it, as `triage` records",
    ),
    (
        "tool_call",
        Update::ToolCall,
        "the call itself, with io-harness's tool name mapped onto an ACP `ToolKind`",
    ),
    (
        "refused",
        Update::ToolCallUpdate,
        "the policy said no, so the call it names ends failed rather than silently \
         not happening",
    ),
    (
        "approval_requested",
        Update::None,
        "the approver answers this, and in 0.36.0 it refuses — the client learns \
         of it through the `approval_decided` update that follows, not through a \
         second frame here. When the request round trip is wired this becomes a \
         `session/request_permission`, which is a request rather than a \
         notification and so is not a row in this table at all",
    ),
    (
        "approval_decided",
        Update::ToolCallUpdate,
        "the answer ends the call the approval was about, and the decision decides \
         the status — an approval that was refused must not leave a cell spinning",
    ),
    (
        "spend_draw",
        Update::None,
        "money is io-cli's own surface and ACP has no field for it; `/cost` and \
         `io exec --json` carry it",
    ),
    (
        "retry",
        Update::None,
        "a provider retry is below the conversation and a client that drew one per \
         attempt would report a slow network as agent activity",
    ),
    (
        "fell_back_to",
        Update::None,
        "which provider answered is io-cli's routing surface, not the client's",
    ),
    (
        "replan",
        Update::Plan,
        "the plan changed, and the plan is a first-class ACP update",
    ),
    (
        "stalled",
        Update::None,
        "reported through the run's `stopReason`, not as a notification",
    ),
    (
        "spawned",
        Update::None,
        "the agent tree is io-cli's `/fleet` surface; ACP models one session and a \
         child's own events arrive under it",
    ),
    ("child_detached", Update::None, "as `spawned`"),
    ("child_collected", Update::None, "as `spawned`"),
    ("spawn_refused", Update::None, "as `spawned`"),
    ("fleet", Update::None, "as `spawned`"),
    (
        "memory_wrote",
        Update::None,
        "the operator's memory file is io-cli's `/memory` surface",
    ),
    ("memory_forgot", Update::None, "as `memory_wrote`"),
    (
        "todo_wrote",
        Update::Plan,
        "the agent's own task list is what ACP's plan update is for",
    ),
    (
        "question_asked",
        Update::None,
        "an intent question needs an answer, so it is a request rather than a \
         notification; 0.36.0 does not carry one and the run parks, which the \
         `stopReason` reports",
    ),
    ("questions_asked", Update::None, "as `question_asked`"),
    (
        "question_answered",
        Update::None,
        "as `question_asked` — nothing asked it through this door",
    ),
    (
        "plan_proposed",
        Update::Plan,
        "the plan, which is a first-class ACP update",
    ),
    (
        "plan_decided",
        Update::Plan,
        "the plan's state changed and the client redraws it",
    ),
    (
        "reasoning",
        Update::ThoughtChunk,
        "the provider returned thinking separately and ACP has a separate channel \
         for it, so a client can fold it",
    ),
    (
        "server_tool_used",
        Update::ToolCall,
        "a search or fetch the provider ran for the model is still a tool call the \
         operator should see",
    ),
    (
        "token",
        Update::MessageChunk,
        "the answer itself, streamed; concatenation is the property that matters \
         and the chunk is passed through untouched",
    ),
    (
        "sandbox",
        Update::None,
        "the boundary is io-cli's status line and `/status`; ACP has no field for a \
         backend or a probe, and a client cannot act on one",
    ),
    (
        "mcp",
        Update::None,
        "a server reaching the run is configuration, not conversation; an MCP \
         *call* arrives as `tool_call`",
    ),
    (
        "handle_started",
        Update::ToolCall,
        "a background shell is a long-running call and is drawn as one",
    ),
    (
        "handle_polled",
        Update::None,
        "a poll is not an ending and carries no output, as `triage` records",
    ),
    ("handle_killed", Update::ToolCallUpdate, "the handle ended"),
    ("handle_exited", Update::ToolCallUpdate, "the handle ended"),
    (
        "handle_orphaned",
        Update::ToolCallUpdate,
        "the handle ended",
    ),
    (
        "reviewed",
        Update::None,
        "a verification verdict is reported through the run's `stopReason`",
    ),
    (
        "routed",
        Update::None,
        "which model answered is io-cli's routing surface. **Read the note below \
         before drawing this**: 0.75.0 emits it with an empty `from`",
    ),
    (
        "plugin_loaded",
        Update::None,
        "a bundle loading is configuration and fires at step 0 of every turn",
    ),
    ("lsp_started", Update::None, "as `plugin_loaded`"),
    (
        "browser_started",
        Update::None,
        "the browser's own activity arrives as `tool_call`",
    ),
    ("browser_navigated", Update::None, "as `browser_started`"),
    (
        "speculated",
        Update::None,
        "speculation is an optimisation the conversation does not describe",
    ),
    ("plugin_dropped", Update::None, "as `plugin_loaded`"),
    (
        "rewound",
        Update::None,
        "a rewind is io-cli's `/undo` and reaches no ACP session",
    ),
    ("reverted", Update::None, "as `rewound`"),
    (
        "answered",
        Update::None,
        "the answer already streamed as `token` chunks; this names the turn it \
         belongs to and would be a duplicate",
    ),
    (
        "compacted",
        Update::None,
        "folding older context is bookkeeping the conversation does not describe",
    ),
    ("cache_marked", Update::None, "as `compacted`"),
    (
        "prompt_composed",
        Update::None,
        "what was sent is `/context`'s surface, and putting it in the conversation \
         would echo the operator's own prompt back at them",
    ),
    (
        "contained",
        Update::None,
        "as `sandbox` — the boundary is io-cli's status line",
    ),
    (
        "dialed",
        Update::None,
        "reaching the provider is below the conversation",
    ),
    // 0.79.0. Declared unconditionally by `observe.rs` and emitted only behind
    // io-harness's `codeact` feature, which this crate does not enable — so the
    // row is here because the table is total over what the harness declares, and
    // it decides what an editor would be told if the feature were ever on.
    (
        "program",
        Update::None,
        "the acts a program took are not on this event — each re-enters dispatch \
         and arrives as its own `tool_call`, which is already translated; \
         announcing the program itself would put a tool call in the editor that \
         has no `call_id` and never completes",
    ),
    (
        "finished",
        Update::None,
        "the run's end is the `session/prompt` **result** and its `stopReason`, not \
         a notification; sending both would end the turn twice",
    ),
];

/// What `name` becomes, or `None` when the locked harness declares a kind this
/// table has never heard of.
///
/// `None` and [`Update::None`] are different answers and the difference is the
/// whole point: the second is a decision, the first is a gap.
pub fn update_for(name: &str) -> Option<Update> {
    MAPPING
        .iter()
        .find(|(kind, ..)| *kind == name)
        .map(|(_, update, _)| *update)
}

/// Every tool io-harness declares, and the ACP `ToolKind` it is drawn as.
///
/// The nine kinds are the specification's: `read`, `edit`, `delete`, `move`,
/// `search`, `execute`, `think`, `fetch` and `other`.
///
/// **`other` appears in this table, and that is the point of having a table.**
/// It is a correct answer for several harness tools — asking the operator a
/// question is not a read, an edit or an execution in ACP's vocabulary — so a
/// gate asserting "no harness tool is `other`" would be false. What must not
/// happen is a tool reaching `other` because nobody looked at it. Listing every
/// name here makes the two cases different states: an entry saying `other` is a
/// decision, and a name absent from the table is a gap, and `tests/acp.rs` reads
/// the locked harness's own tool names and fails on the second.
///
/// The wildcard in [`tool_kind`] therefore serves only names this crate has never
/// seen — a bundle's own tool, an MCP server's tool — where `other` is the only
/// honest answer and inferring `edit` from a substring would tell a client a read
/// was a write.
pub const TOOL_KINDS: &[(&str, &str)] = &[
    // Reads.
    ("read_file", "read"),
    ("list_dir", "read"),
    ("view_image", "read"),
    ("read_skill", "read"),
    ("read_messages", "read"),
    ("xlsx_read", "read"),
    ("xlsx_sheets", "read"),
    ("docx_read", "read"),
    ("pptx_read", "read"),
    ("pdf_read", "read"),
    ("barcode_decode", "read"),
    ("git_log", "read"),
    ("git_status", "read"),
    ("git_diff", "read"),
    ("browser_read", "read"),
    ("lsp_definition", "read"),
    ("lsp_references", "read"),
    ("lsp_symbols", "read"),
    ("lsp_hover", "read"),
    // Writes. `remember` is an edit because it appends to the operator's memory
    // file, which is a file on their disk.
    ("write_file", "edit"),
    ("edit_file", "edit"),
    ("patch_file", "edit"),
    ("xlsx_write", "edit"),
    ("xlsx_set_cell", "edit"),
    ("docx_write", "edit"),
    ("pdf_write", "edit"),
    ("pdf_fill_form", "edit"),
    ("pdf_watermark", "edit"),
    ("lsp_rename", "edit"),
    ("remember", "edit"),
    // `forget` removes one, which ACP has its own kind for.
    ("forget", "delete"),
    // Searches.
    ("grep", "search"),
    ("find", "search"),
    // Executions. A git write is an execution rather than an edit: it changes the
    // repository's state through a program, and a client offering to preview a
    // diff for `git_commit` would be offering the wrong thing.
    ("exec", "execute"),
    ("shell", "execute"),
    ("shell_start", "execute"),
    ("shell_poll", "execute"),
    ("shell_kill", "execute"),
    ("check", "execute"),
    ("git_add", "execute"),
    ("git_commit", "execute"),
    ("git_branch", "execute"),
    ("git_worktree", "execute"),
    ("spawn_agent", "execute"),
    // 0.79.0, and `execute` is the only honest kind: it runs a program the model
    // wrote, under the run's own exec mode. Declared unconditionally by
    // `tools/mod.rs`, so this row is needed whether or not this crate enables
    // io-harness's `codeact` feature — which today it does not.
    ("run_program", "execute"),
    // Fetches — the browser reaches something outside the workspace.
    ("browser_navigate", "fetch"),
    ("browser_screenshot", "fetch"),
    ("browser_click", "fetch"),
    ("browser_type", "fetch"),
    ("browser_scroll", "fetch"),
    // Thinking. Planning is the closest of the nine and it is what a client folds
    // by default, which is the behaviour these deserve.
    ("propose_plan", "think"),
    ("todo_write", "think"),
    // Genuinely `other`, decided rather than defaulted. Asking the operator a
    // question and messaging a sibling agent are neither reads, edits nor
    // executions in ACP's vocabulary, and forcing one of the eight onto them
    // would mislabel them in the one surface a client renders.
    ("ask_question", "other"),
    ("ask_questions", "other"),
    ("send_message", "other"),
];

/// Is `name` a tool this adapter has classified deliberately?
///
/// The distinction [`TOOL_KINDS`] exists to make: a name in the table answering
/// `other` is a decision, a name absent from it is a gap.
pub fn classified(name: &str) -> bool {
    TOOL_KINDS.iter().any(|(tool, _)| *tool == name)
}

/// io-harness's tool name as one of ACP's nine `ToolKind`s.
///
/// A name this crate has never seen — a bundle's own tool, an MCP server's tool —
/// is `other`, which is the only honest answer for it. See [`TOOL_KINDS`].
pub fn tool_kind(name: &str) -> &'static str {
    TOOL_KINDS
        .iter()
        .find(|(tool, _)| *tool == name)
        .map(|(_, kind)| *kind)
        .unwrap_or("other")
}

/// The `stopReason` for a run that ended this way.
///
/// ACP names five: `end_turn`, `max_tokens`, `max_turn_requests`, `refusal` and
/// `cancelled`. io-harness names eighteen outcomes, so this is a narrowing and
/// several outcomes share an answer — which is correct, because a client acts on
/// the reason and only these five distinctions change what it does.
///
/// `SchemaUnsatisfied` joins the attempts bucket rather than `refusal`: a run
/// that could not produce the requested shape has exhausted the attempts it was
/// allowed, and telling a client it was refused would misreport the agent's
/// willingness the same way a pause would.
///
/// **A pause is `end_turn` and not `refusal`**, and that is the one mapping worth
/// arguing. A run waiting on a question or a plan has not refused anything; it
/// has stopped with work outstanding, and telling a client it was refused would
/// be a lie about the agent's willingness. 0.36.0 carries no way to answer those
/// through ACP — `session/prompt` returns and the run is resumable with
/// `io resume`, which the guide says.
pub fn stop_reason(outcome: &io_harness::RunOutcome) -> &'static str {
    use io_harness::RunOutcome as O;
    match outcome {
        O::Success { .. } | O::Finished { .. } => "end_turn",
        O::Cancelled { .. } => "cancelled",
        O::Denied { .. } | O::Refused { .. } | O::PlanRejected { .. } => "refusal",
        O::StepCapReached { .. }
        | O::VerificationFailed { .. }
        | O::SchemaUnsatisfied { .. }
        | O::Stalled { .. }
        | O::Escalated { .. } => "max_turn_requests",
        O::TimeBudgetExceeded { .. }
        | O::CostBudgetExceeded { .. }
        | O::BudgetCeilingReached { .. } => "max_tokens",
        O::AwaitingRecovery { .. }
        | O::AwaitingApproval { .. }
        | O::AwaitingAnswer { .. }
        | O::AwaitingPlan { .. } => "end_turn",
        // `RunOutcome` is `#[non_exhaustive]`. An outcome this build has not seen
        // ends the turn rather than claiming a reason it cannot know — and
        // `tests/acp.rs` reads the locked enum and fails by name, so this arm is
        // the behaviour between a harness release and the release that maps it,
        // never the permanent answer.
        _ => "end_turn",
    }
}

/// The `session/update` params for one event, or `None` when this kind is
/// deliberately not sent.
///
/// The `sessionId` is the caller's — this function does not know it — so the
/// returned object carries the `update` member alone and the handler wraps it.
pub fn translate(event: &RunEvent) -> Option<Value> {
    let update = match &event.kind {
        EventKind::Token { text } => json!({
            "sessionUpdate": "agent_message_chunk",
            "content": { "type": "text", "text": text },
        }),
        EventKind::Reasoning { text, .. } => json!({
            "sessionUpdate": "agent_thought_chunk",
            "content": { "type": "text", "text": text },
        }),
        EventKind::ToolCall { name, target, .. } => json!({
            "sessionUpdate": "tool_call",
            // The id is the step it happened on. A client needs a handle to
            // correlate a later `tool_call_update` with, and io-harness gives a
            // call no id of its own — the step is the finest grain available and
            // it is what `src/events.rs` already uses to close an open cell.
            "toolCallId": call_id(event),
            "title": if target.is_empty() { name.clone() } else { format!("{name} {target}") },
            "kind": tool_kind(name),
            "status": "in_progress",
        }),
        EventKind::ServerToolUsed { .. } => json!({
            "sessionUpdate": "tool_call",
            "toolCallId": call_id(event),
            "title": "provider tool",
            "kind": "fetch",
            "status": "in_progress",
        }),
        EventKind::Refused { act, target, .. } => json!({
            "sessionUpdate": "tool_call_update",
            "toolCallId": call_id(event),
            "status": "failed",
            "title": format!("{act} {target} refused"),
        }),
        // **The decision decides the status, and reporting `in_progress` for all
        // three was a defect the adversarial review found.** io-harness carries
        // `"approve"`, `"deny"` or `"defer"` on this variant. Because 0.36.0's
        // approver always denies, an unconditional `in_progress` meant *every*
        // grey-tier action in an editor session drew a cell that spins for ever
        // on a call that will never complete — the opposite of the refusal the
        // operator needs to see.
        EventKind::ApprovalDecided { decision, .. } => json!({
            "sessionUpdate": "tool_call_update",
            "toolCallId": call_id(event),
            "status": match decision.as_str() {
                "approve" => "in_progress",
                // A defer is not a completion and not a failure; the run stops
                // and the call never happened, which for a client's purposes is
                // the same visible end as a refusal.
                _ => "failed",
            },
        }),
        EventKind::RecoveryPaused { .. } => json!({
            "sessionUpdate": "tool_call_update",
            "toolCallId": call_id(event),
            "status": "pending",
        }),
        EventKind::HandleStarted { .. } => json!({
            "sessionUpdate": "tool_call",
            "toolCallId": call_id(event),
            "title": "background shell",
            "kind": "execute",
            "status": "in_progress",
        }),
        EventKind::HandleExited { .. }
        | EventKind::HandleKilled { .. }
        | EventKind::HandleOrphaned { .. } => json!({
            "sessionUpdate": "tool_call_update",
            "toolCallId": call_id(event),
            "status": "completed",
        }),
        EventKind::PlanProposed { .. }
        | EventKind::PlanDecided { .. }
        | EventKind::Replan { .. }
        | EventKind::TodoWrote { .. } => json!({
            "sessionUpdate": "plan",
            // The entries are deliberately empty in 0.36.0: io-harness's plan
            // shapes are four different types and rendering them into ACP's
            // `PlanEntry` is a translation of its own. A client is told the plan
            // moved, which is true, rather than a fabricated list.
            "entries": [],
        }),
        // Everything else is a deliberate no-op, and `MAPPING` above says which
        // and why. The wildcard is mandatory because `EventKind` is
        // `#[non_exhaustive]`; what stops it becoming a silent drop is that
        // `tests/acp.rs` walks the locked harness's own enum against `MAPPING`
        // and fails by name on a kind neither of them has heard of.
        _ => return None,
    };
    Some(update)
}

/// The handle a client correlates a call and its later update by.
///
/// io-harness gives a tool call no id of its own, so the step is the finest grain
/// available — which is also what `src/events.rs` already uses to close an open
/// cell, so the two surfaces agree by construction rather than by coincidence.
/// The run id is in it because a contained turn's children have their own.
fn call_id(event: &RunEvent) -> String {
    format!("{}-{}", event.run_id, event.step)
}
