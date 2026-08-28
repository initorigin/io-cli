//! Where every io-harness event goes, decided once and by hand.
//!
//! Until 0.11.0 this module did not exist and its job was done by a wildcard arm
//! in [`crate::events`] that committed the event's own Rust variant name in a
//! muted line. That was 0.1.0's honest placeholder — a release that starts
//! emitting something new is visible rather than silent — and it outlived its
//! usefulness by ten releases and thirty-seven kinds, which is how an operator
//! came to read `prompt_composed`, `contained`, `reasoning` and `answered` in a
//! transcript that is otherwise written in English.
//!
//! So every kind gets a [`Disposition`] here instead, and the table says where
//! the fact goes when it is not a line. That third column is the load-bearing
//! one: [`Disposition::Silent`] is only correct when the fact reaches the
//! operator by another route, and a route written down is a claim the next
//! release can check.
//!
//! **The table is keyed by the snake-case name and not by the variant**, because
//! `EventKind` is `#[non_exhaustive]`: a Rust match over it can never be proven
//! exhaustive by the compiler, so a table the compiler cannot check is what this
//! has to be. What replaces the compiler is `tests/triage.rs`, which reads
//! `pub enum EventKind` out of the io-harness source this crate is locked to and
//! fails by name when the two sets differ.
//!
//! A kind that is not in the table at all commits nothing and is counted by
//! [`crate::events::Events::unknown`], so a harness that starts emitting
//! something new is quiet in the transcript and reachable on the status line
//! rather than shouting a variant name at whoever is reading.

/// What a kind does when it arrives.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Disposition {
    /// A line an operator was meant to read, designed in [`crate::events`].
    ///
    /// A `Line` kind may still commit nothing on a particular event — an empty
    /// plan, a sandbox notice that is not a cap being hit — and that is a
    /// judgement inside the arm rather than a different disposition.
    Line,
    /// A status-line field, and no line.
    Status,
    /// Nothing. The route column says how the fact reaches the operator instead.
    Silent,
}

/// Every kind io-harness 0.69 declares, in its own declaration order.
///
/// The order is the enum's rather than alphabetical so that this table can be
/// read down the side of `observe.rs` when the pin moves.
pub const TRIAGE: &[(&str, Disposition, &str)] = &[
    ("started", Disposition::Line, "the goal line"),
    (
        "recovery_paused",
        Disposition::Line,
        "the paused line naming the tool and the attempt",
    ),
    (
        "step",
        Disposition::Line,
        "the step line and its tool cells",
    ),
    (
        "tool_call",
        Disposition::Silent,
        "held open, shown by the live row while it runs, and committed as a cell by the step that \
         closes it",
    ),
    ("refused", Disposition::Line, "the refusal line"),
    (
        "approval_requested",
        Disposition::Silent,
        "the approval overlay, which is on screen at the moment it is asked — and a committed \
         line in plain mode, which draws no overlay",
    ),
    ("approval_decided", Disposition::Line, "the decision line"),
    (
        "spend_draw",
        Disposition::Status,
        "the spend field, drawn against the tree's ceiling",
    ),
    ("retry", Disposition::Line, "the retry line"),
    (
        "fell_back_to",
        Disposition::Line,
        "the fallback line, and the provider field it moves",
    ),
    ("replan", Disposition::Line, "the replan line"),
    (
        "stalled",
        Disposition::Line,
        "the stall line, naming the step it stopped on and how long it has been there — the run's \
         own outcome says the same word much later, and only once nobody is still waiting for it \
         to say something",
    ),
    ("spawned", Disposition::Line, "the spawn line"),
    ("child_detached", Disposition::Line, "the detach line"),
    ("child_collected", Disposition::Line, "the report line"),
    ("spawn_refused", Disposition::Line, "the refusal line"),
    (
        "fleet",
        Disposition::Status,
        "the fleet view, which is what a per-tier count is for",
    ),
    ("memory_wrote", Disposition::Line, "the remembered line"),
    ("memory_forgot", Disposition::Line, "the forgotten line"),
    (
        "todo_wrote",
        Disposition::Line,
        "the plan block, and the plan field",
    ),
    ("question_asked", Disposition::Line, "the question line"),
    ("question_answered", Disposition::Line, "the answer line"),
    (
        "plan_proposed",
        Disposition::Silent,
        "the plan overlay, which is on screen at the moment it is proposed",
    ),
    ("plan_decided", Disposition::Line, "the verdict line"),
    ("reasoning", Disposition::Line, "the thought block"),
    (
        "server_tool_used",
        Disposition::Line,
        "the line naming which of the provider's own tools ran",
    ),
    (
        "token",
        Disposition::Line,
        "the streaming tail, committed a finished line at a time",
    ),
    (
        "sandbox",
        Disposition::Line,
        "the sandbox line, for six of the seven kinds. `create`, `exec`, `cap_hit` and `destroy` \
         say what isolated the work; `gate_phase_failed` and `gate_output` say that the criterion \
         the operator configured under `[app.io-cli.gates]` ran and did not hold, which from \
         0.24.0 is a thing a session can see. `dial` is the one kind that reaches no line: it is \
         drawn by `dialed`, which carries the host, the port and the verdict this kind does not. \
         Neither gate line names the phase or quotes the output — `EventKind::Sandbox` carries \
         the kind and the backend alone, and the `detail` holding both stays in the run's own \
         `sandbox_events` rows, which is where a diagnosis reads it",
    ),
    (
        "mcp",
        Disposition::Status,
        "the mcp field, and the measured duration harvested onto the open cell",
    ),
    (
        "handle_started",
        Disposition::Line,
        "the job line, and the bg count",
    ),
    (
        "handle_polled",
        Disposition::Silent,
        "the handle's own start and end lines; a poll is not an ending and carries no output",
    ),
    ("handle_killed", Disposition::Line, "the job line"),
    ("handle_exited", Disposition::Line, "the job line"),
    ("handle_orphaned", Disposition::Line, "the job line"),
    (
        "reviewed",
        Disposition::Line,
        "the verdict line and its reasons, in the reviewer's own words. Emitted only for a review \
         that happened: one that could not run emits nothing here at all and is recorded as \
         `GateOutcome::Errored`, so an absent verdict line is a review that never answered rather \
         than one that said yes",
    ),
    (
        "routed",
        Disposition::Status,
        "the model field, which is the change itself",
    ),
    (
        "plugin_loaded",
        Disposition::Silent,
        "`/plugin`, which lists it and what it contributed; also `io exec --json` and the durable \
         trace. Silent on the stream because it fires at step 0 of every turn — a bundle that \
         loaded changed nothing an operator asked about mid-session, and a line each time would be \
         a line about a directory that has not moved since the session started",
    ),
    ("lsp_started", Disposition::Status, "the lsp field"),
    ("browser_started", Disposition::Status, "the web field"),
    (
        "browser_navigated",
        Disposition::Status,
        "the web field, with its verdict",
    ),
    (
        "speculated",
        Disposition::Silent,
        "`io exec --json` and the durable trace; nothing about the step's own events moves",
    ),
    (
        "plugin_dropped",
        Disposition::Silent,
        "`/plugin`, which lists it with io-harness's own sentence for why; also `io exec --json` \
         and the durable trace. **Silent is a close call and it is recorded as one.** A bundle \
         that failed to load is exactly the kind of fact this interface argues should be visible — \
         but it is a standing misconfiguration rather than an event, it fires at step 0 of every \
         turn for as long as it goes unfixed, and a refusal repeated once a turn teaches an \
         operator to stop reading refusals. So it goes to the surface that holds standing facts, \
         and `/plugin` exists from this release for that",
    ),
    (
        "rewound",
        Disposition::Silent,
        "io-cli's own rewind summary, written from the `Rewound` value the call returned",
    ),
    (
        "reverted",
        Disposition::Silent,
        "io-cli's own rewind summary, written from the value the call returned",
    ),
    (
        "answered",
        Disposition::Silent,
        "the answer itself, which is the whole of what a conversational turn produced",
    ),
    (
        "compacted",
        Disposition::Status,
        "the context field, which is what a fold changes",
    ),
    (
        "cache_marked",
        Disposition::Silent,
        "`io exec --json` and the durable trace; what a marker bought is in the run's usage rows",
    ),
    (
        "prompt_composed",
        Disposition::Silent,
        "`io exec --json` and the durable trace; it carries no prompt text and is emitted before \
         the first step",
    ),
    ("contained", Disposition::Status, "the containment field"),
    (
        "dialed",
        Disposition::Line,
        "the dial line, carrying the host as the command asked for it, the port, and whether the \
         policy permitted it",
    ),
    (
        "finished",
        Disposition::Line,
        "the outcome line, when the outcome needs one",
    ),
];

/// What this kind does, or `None` for a kind this release has never heard of.
pub fn disposition(name: &str) -> Option<Disposition> {
    TRIAGE
        .iter()
        .find(|(kind, ..)| *kind == name)
        .map(|(_, disposition, _)| *disposition)
}

/// How the fact reaches the operator, for a kind that commits no line.
///
/// Not read by the product. It is the column that makes a `Silent` reviewable,
/// and `tests/triage.rs` requires one on every row.
pub fn route(name: &str) -> Option<&'static str> {
    TRIAGE
        .iter()
        .find(|(kind, ..)| *kind == name)
        .map(|(.., route)| *route)
}
