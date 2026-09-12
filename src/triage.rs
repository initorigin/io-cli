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

/// Every kind io-harness 0.79 declares, in its own declaration order.
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
    // **0.36.0 — a breakdown emitted beside every step, and the one place a line
    // would cost more than it says.** io-harness 0.75.0 emits this from the same
    // place as `step` above, for every committed step, carrying the span the step
    // line already draws plus the phases that span divides into. Drawn as a line
    // it would put a second row under every step row, restating a number the row
    // above it carries — a transcript twice as long to say the same thing once
    // more.
    //
    // **The route is the machine surfaces, and it says so rather than naming an
    // operator surface that does not draw these numbers.** `/store` and
    // `/context` are both registered commands, so a route naming either would
    // have satisfied `f9_every_silent_route_names_a_surface_that_exists` — which
    // checks that a named command exists, not that it renders the fact — and been
    // a claim this build does not honour. io-harness's own declaration points an
    // observer at `Store::step_attributions`, which is the reader that has the
    // phases and the time to first token this event deliberately leaves off.
    (
        "step_attributed",
        Disposition::Silent,
        "`io exec --json`, which forwards it verbatim, and the durable trace; the harness's own \
         declaration points an observer at `Store::step_attributions` for the same figures. \
         **Silent is a close call and it is recorded as one.** Where a slow step spent its wall \
         clock is exactly the kind of fact this interface argues should be visible — but it \
         arrives beside every committed step, so a line is a second row under every step row \
         restating that step's span, and no surface in this release renders the phase breakdown \
         that would earn the room",
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
    (
        "question_asked",
        Disposition::Silent,
        "the question overlay, which is on screen at the moment it is asked — and a committed line \
         wherever nothing draws one: plain mode, and a resumed run whose overlay this process is \
         not holding",
    ),
    // **0.33.0 — the one kind in this table whose fact no overlay can carry.**
    // The obvious row here would have copied `question_asked` above and been
    // `Silent` with the overlay as its route, and it would have been wrong.
    // `crate::intent::Answerer` implements `Responder::answer` and nothing else,
    // so io-harness's own `answer_all` hands the overlay the batch one question at
    // a time; the overlay draws a question, never an ask. "These three arrived
    // together" is the entire content of this variant — it is why io-harness
    // added it — and there is no surface in this product that would have said it.
    // A `Silent` row would have routed the only fact it carries to nothing.
    (
        "questions_asked",
        Disposition::Line,
        "the batch line, saying how many the agent asked at once and numbering each one in the \
         order it asked them. The questions themselves keep `question_asked`'s rule and are drawn \
         under it only where no overlay will draw them — the count is committed either way, \
         because that is the part nothing else says",
    ),
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
         the kind and the backend alone. Until io-harness 0.86.0 the `detail` holding both stayed \
         in the run's own `sandbox_events` rows and that was the only place a diagnosis could read \
         it; the quoting is now `gate_output`'s, the row below",
    ),
    // **`gate_output` is a top-level kind as of io-harness 0.86.0, and it is NOT
    // the `gate_output` named in the row above.** That one is a *value of the
    // `kind` field* on `EventKind::Sandbox`, which carries no payload at all —
    // the field report drove a gate to failure fourteen times and read back
    // `{"event":"sandbox","kind":"gate_output","backend":null}`, which says a
    // gate produced output and does not say what it was. This is a separate
    // variant of the same wire name carrying `output` and `exit_code`, and the
    // two must not be folded together: a table keyed on wire names has one row
    // per variant, and the sandbox sub-kind is not a row here at all.
    //
    // **A line, because a gate that failed with nothing said is the thing this
    // release exists to stop.** An operator whose first gate fails today learns
    // only that it failed; the cause sat in a store row they had no reason to
    // open. The line quotes the head of what the command printed and its exit
    // code, which is the whole of the diagnosis for a mis-set gate.
    //
    // **io-cli re-bounds nothing.** io-harness bounds `output` at 4,000
    // characters kept from the head *and* the tail, with its reason written into
    // the declaration — a test runner puts the invocation at one end and the
    // failure at the other — so a renderer that trimmed it further would be
    // holding a second opinion about somebody else's bounded text. The
    // transcript shows the first lines because a transcript is narrow; the
    // string forwarded to `io exec --json` is the event's own, byte for byte.
    (
        "gate_output",
        Disposition::Line,
        "the gate line, quoting the first lines of what the command printed and its exit code; \
         `io exec --json` forwards the whole bounded string verbatim",
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
    // **0.27.0 — the one silence in this table with no route to any surface an
    // operator uses.** It read "`io exec --json` and the durable trace", and both
    // of those are places a person goes deliberately, afterwards, having already
    // suspected something. A route is supposed to name where the fact reaches
    // somebody who was not looking for it, and this one never did.
    //
    // What it says is worth a line, too: reads started before the model had
    // finished asking, and how many were thrown away. Started-and-discarded is
    // work that was paid for, and it is the only figure in this product an
    // operator can act on by turning speculation off.
    //
    // The other eight silences reviewed in the same pass keep their routes,
    // which are better arguments than drawing them would be — see
    // `US-IO-CLI-0.27.0-I04`.
    ("speculated", Disposition::Line, "the speculation line"),
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
    // **0.79.0 declared it, 0.39.0 turned it on.** The note here said "revisit
    // when this crate enables `codeact`", and this is that revisit. Placed
    // between `contained` and `dialed` because that is where `EventKind` declares
    // it, and this table's whole ordering rule is that it can be read down the
    // side of `observe.rs` when the pin moves.
    //
    // **A line, and the old note's objection is answered rather than overruled.**
    // It refused one because "a line here would announce that a program ran and
    // then say nothing about what it did", which is true of the event and false
    // of the transcript: every act the program took re-enters dispatch and
    // arrives as its own `tool_call`, so the rows underneath this one are exactly
    // what it did. What was missing was the row that says they belong to a
    // program rather than to the model calling tools one at a time — which is a
    // materially different thing for a reader to know, because a program's acts
    // are not separately approved.
    //
    // The availability half is drawn too, and only when a capability was
    // **withheld**. That is a fact about the run's configuration rather than
    // about the conversation, and would be furniture on every contained turn —
    // but an operator who configured `[codeact]` and is watching the agent make
    // twelve round trips instead of writing one program needs to know the host
    // had no interpreter.
    (
        "program",
        Disposition::Line,
        "a row naming the interpreter, how many calls the program made and how it ended, with the \
         acts it took drawn beneath it as their own tool cells; a withheld capability says so \
         once, and `io exec --json` forwards the event verbatim",
    ),
    (
        "dialed",
        Disposition::Line,
        "the dial line, carrying the host as the command asked for it, the port, and whether the \
         policy permitted it",
    ),
    // **0.38.2 — the three kinds io-harness 0.80.0 and 0.81.0 added, arriving
    // together on one pin bump.** None of them fails to compile: `EventKind` is
    // `#[non_exhaustive]`, so a kind with no row falls through the wildcard and is
    // drawn by nothing, silently. This table is the only thing that notices, and
    // it noticed all three the moment the pin moved.
    //
    // **`context_ceiling` is a status field and not a line**, because the number
    // it carries is a denominator rather than an event: `ctx N%` is the assembled
    // request over exactly this ceiling, and until io-harness 0.81.0 the contract
    // was the only thing that knew it. `Status::note_ceiling` takes it and
    // `context::window` divides by it, so the share and the `/context` page move
    // together — they disagreed once, on one screen, and the fix then was to make
    // them one expression. A line would be a row per run restating a number
    // already on the line, and the one case worth acting on — `source:
    // "fallback"`, the harness guessing — is worth a release that renders it
    // rather than a row that mentions it.
    (
        "context_ceiling",
        Disposition::Status,
        "the `ctx` field's denominator, and the `/context` page's total, which are one expression \
         so that they cannot disagree",
    ),
    // **`step_usage` was the close call 0.38.2 recorded, and 0.39.0 is the release
    // it said would answer it.** The note here read "the fresh-versus-cached split
    // it carries is what the footer's `tok` figure does not yet separate, and
    // separating it is a release rather than a row". This is that release: the
    // cache read accumulates onto `Status::cached` and the footer draws
    // `52k tok · 44k cached`.
    //
    // A field and not a line, because it arrives beside **every** committed step —
    // a row per step would be a breakdown nobody asked for scrolling past the work
    // it describes. The other three numbers it carries stay unrendered and stay
    // available on `io exec --json`.
    (
        "step_usage",
        Disposition::Status,
        "the footer's `tok` field, which draws how much of the running total the provider read \
         from its cache; `io exec --json` forwards all four numbers verbatim, and the durable \
         trace keeps them",
    ),
    // **Three kinds io-harness 0.84.0 and 0.85.0 declared and this release does
    // not draw, recorded as deliberate silences rather than left to the
    // wildcard.** The pin moved for reasons of its own and brought them with it;
    // a table whose whole job is totality must answer for a kind the moment it is
    // declared, and "we have not built the surface yet" is an answer as long as
    // it is written down and the route is honest.
    //
    // **The surface they belong on is 0.43.0's and is named in the roadmap**: the
    // status line and `/usage` showing each platform window as a percentage used
    // with its reset time, `/context` showing where the cacheable prefix ends,
    // and a `prefix_broke` or `cache_miss` rendering as a transcript notice
    // naming the step and the reason. Drawing them here would be that release
    // built early and badly — `rate_limit`'s numbers mean nothing without the
    // window accounting around them, and a cache notice with no `/usage` to
    // explain it is a line an operator can do nothing with.
    //
    // **So the route is the two machine surfaces, which genuinely carry them
    // today.** `io exec --json` forwards every field of every kind, and the
    // durable trace keeps them, so nothing here is lost — it is unrendered, which
    // is a different claim and the one these rows make.
    (
        "prefix_broke",
        Disposition::Silent,
        "`io exec --json`, which forwards the step, the byte the prefix broke at and the reason, \
         and the durable trace, which keeps them. The transcript notice is 0.43.0's",
    ),
    (
        "cache_miss",
        Disposition::Silent,
        "`io exec --json`, which forwards the step, the reprocessed tokens and what was expected, \
         and the durable trace, which keeps them. The transcript notice is 0.43.0's",
    ),
    (
        "rate_limit",
        Disposition::Silent,
        "`io exec --json`, which forwards the remaining requests and tokens with both reset \
         windows, and the durable trace, which keeps them. The window accounting these numbers \
         need around them is 0.43.0's",
    ),
    // **`image_attached` is a line as of 0.39.0, and the objection the old note
    // raised is answered rather than overruled.** That note said "a line naming a
    // picture without showing it would be the class of sentence this release
    // exists to delete", and it was right about the sentence it was refusing. What
    // it did not weigh is that the alternative was **silence about a thing that
    // entered the model's context** — an operator whose window filled up with a
    // browser screenshot they never asked for could read `/context`, see the
    // conversation swollen, and find nothing anywhere saying a picture had
    // arrived.
    //
    // So the row names where it came from, what it is and how big — `mcp`,
    // `browser`, `view_image` or `caller`, from io-harness's own `source` field —
    // and does not pretend to show it. The picture itself is still only drawn for
    // an image the operator handed over, through `src/picture.rs`, on a terminal
    // that can draw one.
    (
        "image_attached",
        Disposition::Line,
        "a row naming where the picture came from, its media type and its size; the image itself \
         is drawn only for one the operator attached, and `io exec --json` forwards the event \
         verbatim either way",
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
