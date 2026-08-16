//! F8 — every event kind is accounted for.
//!
//! `io_harness::EventKind` is `#[non_exhaustive]`, so a wildcard arm is required
//! by the type rather than chosen. The criterion is therefore asserted as the two
//! things that actually matter: no kind renders to nothing, and no kind exists
//! that this repository has never seen.

mod support;

use io_cli::events::{kind_name, Events};
use io_cli::theme::DARK;
use io_harness::{EventKind, RunEvent};

/// The kinds this release styles, in the contract's own words: `started`,
/// `token`, `step`, `tool_call` and `finished` rendered fully, `refused` and
/// `approval_requested` as a plain one-line notice.
const STYLED: &[&str] = &[
    "started",
    "token",
    "step",
    "tool_call",
    "finished",
    "refused",
    "approval_requested",
];

/// Every other kind io-harness 0.60.1 emits. Each renders as a muted single line
/// naming itself. This list is not decoration: the drift test below fails when
/// io-harness grows a kind that is in neither list, which is the moment somebody
/// has to decide what it should look like.
const FALLS_THROUGH: &[&str] = &[
    "approval_decided",
    "spend_draw",
    "retry",
    "fell_back_to",
    "replan",
    "stalled",
    "spawned",
    "spawn_refused",
    "child_detached",
    "child_collected",
    "fleet",
    "memory_wrote",
    "memory_forgot",
    "todo_wrote",
    "question_asked",
    "question_answered",
    "plan_proposed",
    "plan_decided",
    "reasoning",
    "server_tool_used",
    "sandbox",
    "mcp",
    "handle_started",
    "handle_polled",
    "handle_killed",
    "handle_exited",
    "handle_orphaned",
    "reviewed",
    "routed",
    "plugin_loaded",
    "plugin_dropped",
    "lsp_started",
    "browser_started",
    "browser_navigated",
    "speculated",
    "dialed",
    "rewound",
    "reverted",
    "answered",
    "compacted",
    "cache_marked",
    "prompt_composed",
    "contained",
];

fn event(kind: EventKind) -> RunEvent {
    RunEvent::new(1, 1, kind)
}

fn rendered(events: &mut Events, kind: EventKind) -> String {
    events
        .event(&event(kind))
        .iter()
        .flat_map(|line| line.spans.iter())
        .map(|span| span.content.as_ref())
        .collect()
}

#[test]
fn f8_every_styled_kind_renders_its_own_facts() {
    let mut events = Events::new(DARK);

    let started = rendered(
        &mut events,
        EventKind::Started {
            goal: "make the failing test pass".into(),
            provider: "openrouter".into(),
        },
    );
    assert!(
        started.contains("make the failing test pass"),
        "{started:?}"
    );
    assert!(started.contains("openrouter"), "{started:?}");

    let tool = rendered(
        &mut events,
        EventKind::ToolCall {
            name: "exec".into(),
            target: "cargo test".into(),
        },
    );
    assert!(tool.contains("exec"), "{tool:?}");
    assert!(tool.contains("cargo test"), "{tool:?}");

    let step = rendered(
        &mut events,
        EventKind::Step {
            decision: "edited src/lib.rs".into(),
            tool_call: "apply_patch".into(),
            tokens: 1234,
            changed: true,
        },
    );
    assert!(step.contains("edited src/lib.rs"), "{step:?}");
    assert!(step.contains("1234"), "{step:?}");
    assert!(step.contains("changed files"), "{step:?}");

    // The two facts no other core records must survive even the plain notice this
    // release renders a refusal as.
    let refused = rendered(
        &mut events,
        EventKind::Refused {
            act: "write".into(),
            target: "/etc/hosts".into(),
            rule: Some("fs.deny".into()),
            layer: Some("workspace".into()),
        },
    );
    assert!(refused.contains("refused"), "{refused:?}");
    assert!(refused.contains("/etc/hosts"), "{refused:?}");
    assert!(refused.contains("fs.deny"), "{refused:?}");
    assert!(refused.contains("workspace"), "{refused:?}");

    let approval = rendered(
        &mut events,
        EventKind::ApprovalRequested {
            act: "exec".into(),
            target: "rm -rf build".into(),
        },
    );
    assert!(approval.contains("warning"), "{approval:?}");
    assert!(approval.contains("rm -rf build"), "{approval:?}");

    let finished = rendered(
        &mut events,
        EventKind::Finished {
            outcome: "success".into(),
            steps: 7,
            tokens: 9876,
        },
    );
    assert!(finished.contains("ok"), "{finished:?}");
    assert!(finished.contains("success"), "{finished:?}");
    assert!(finished.contains('7'), "{finished:?}");
}

#[test]
fn f8_a_kind_this_release_does_not_style_still_renders_and_names_itself() {
    let mut events = Events::new(DARK);
    let line = rendered(
        &mut events,
        EventKind::SpendDraw {
            tokens: 21,
            remaining: Some(500),
        },
    );
    assert!(
        line.contains("spend_draw"),
        "an unstyled kind must name itself rather than vanish: {line:?}",
    );
}

#[test]
fn f8_tokens_are_coalesced_rather_than_committed_one_at_a_time() {
    let mut events = Events::new(DARK);
    for word in ["Here ", "is ", "the ", "answer."] {
        let committed = events.event(&event(EventKind::Token { text: word.into() }));
        assert!(
            committed.is_empty(),
            "a token committed a line to scrollback on its own",
        );
    }
    assert_eq!(events.live(), "Here is the answer.");

    // ...and the whole passage commits once, when something finishes it.
    let finished = rendered(
        &mut events,
        EventKind::Finished {
            outcome: "success".into(),
            steps: 1,
            tokens: 4,
        },
    );
    assert!(finished.contains("Here is the answer."), "{finished:?}");
    assert_eq!(events.live(), "", "the live buffer should be emptied");
}

#[test]
fn f8_no_event_renders_to_nothing_except_a_token() {
    let mut events = Events::new(DARK);
    let kinds = vec![
        EventKind::Started {
            goal: "g".into(),
            provider: "p".into(),
        },
        EventKind::Step {
            decision: "d".into(),
            tool_call: String::new(),
            tokens: 0,
            changed: false,
        },
        EventKind::ToolCall {
            name: "n".into(),
            target: String::new(),
        },
        EventKind::Refused {
            act: "a".into(),
            target: "t".into(),
            rule: None,
            layer: None,
        },
        EventKind::ApprovalRequested {
            act: "a".into(),
            target: "t".into(),
        },
        EventKind::Finished {
            outcome: "cancelled".into(),
            steps: 0,
            tokens: 0,
        },
        EventKind::Stalled,
        EventKind::Replan { window: 8 },
    ];

    for kind in kinds {
        let name = kind_name(&kind);
        let lines = events.event(&event(kind));
        assert!(
            !lines.is_empty(),
            "{name} rendered to nothing, which is an event silently discarded",
        );
    }
}

#[test]
fn a_turn_that_ended_well_does_not_end_the_transcript_with_a_warning() {
    use io_cli::events::outcome_tone;
    use io_cli::theme::Tone;

    // `finished` is what EVERY io-cli turn returns: a steerable turn is built on
    // TaskContract::workspace, which carries Verification::None, so there is no
    // criterion to pass and `success` is unreachable from this interface. Reading
    // it as a warning was a real defect and a live run is what found it.
    assert_eq!(outcome_tone("finished"), Tone::Success);
    assert_eq!(outcome_tone("success"), Tone::Success);

    // Stopped deliberately: not a failure, and not silence either.
    for outcome in [
        "cancelled",
        "denied",
        "refused",
        "plan_rejected",
        "stalled",
        "budget_ceiling_reached",
    ] {
        assert_eq!(outcome_tone(outcome), Tone::Warning, "{outcome}");
    }

    // Gave up, or a word this release has never seen.
    for outcome in ["escalated_terminal", "escalated_retryable", "something_new"] {
        assert_eq!(outcome_tone(outcome), Tone::Error, "{outcome}");
    }
}

#[test]
fn a_turn_that_ends_waiting_for_a_human_says_what_to_do_about_it() {
    use io_cli::events::{outcome_help, outcome_tone};
    use io_cli::theme::Tone;

    // A live first run walked straight into this: the ask-before-writes posture
    // denied three actions, the agent asked for permission, and the turn ended
    // `awaiting_answer` — a state this release has nothing on screen to resolve.
    // Nothing went wrong, so it is a warning rather than an error; but an outcome
    // the operator cannot act on has to come with a next action.
    for outcome in ["awaiting_answer", "awaiting_approval", "awaiting_plan"] {
        assert_eq!(outcome_tone(outcome), Tone::Warning, "{outcome}");
        let help = outcome_help(outcome)
            .unwrap_or_else(|| panic!("{outcome} leaves the operator with no next action"));
        assert!(
            help.contains("io setup"),
            "{outcome} should name the way out: {help}",
        );
    }

    // A refusal is actionable too, for the same reason.
    assert!(outcome_help("denied").is_some());
    assert!(outcome_help("refused").is_some());

    // An outcome that needs no explanation does not get one.
    for outcome in ["finished", "success", "cancelled", "stalled"] {
        assert_eq!(outcome_help(outcome), None, "{outcome}");
    }
}

#[test]
fn the_awaiting_help_reaches_the_transcript() {
    let mut events = Events::new(DARK);
    let line = rendered(
        &mut events,
        EventKind::Finished {
            outcome: "awaiting_answer".into(),
            steps: 5,
            tokens: 4210,
        },
    );
    assert!(line.contains("awaiting_answer"), "{line:?}");
    assert!(line.contains("warning"), "{line:?}");
    assert!(
        line.contains("io setup"),
        "the way out should be in the transcript, not only in the docs: {line:?}",
    );
    assert!(!line.contains("error"), "nothing went wrong: {line:?}");
}

#[test]
fn a_finished_turn_reads_as_finished_end_to_end() {
    let mut events = Events::new(DARK);
    let line = rendered(
        &mut events,
        EventKind::Finished {
            outcome: "finished".into(),
            steps: 8,
            tokens: 32624,
        },
    );
    assert!(line.contains("ok"), "{line:?}");
    assert!(!line.contains("warning"), "{line:?}");
}

#[test]
fn the_kind_name_is_the_serde_tag() {
    assert_eq!(
        kind_name(&EventKind::ToolCall {
            name: String::new(),
            target: String::new()
        }),
        "tool_call",
    );
    assert_eq!(kind_name(&EventKind::Stalled), "stalled");
    assert_eq!(
        kind_name(&EventKind::Token {
            text: String::new()
        }),
        "token",
    );
}

#[test]
fn f8_this_release_has_seen_every_kind_io_harness_emits() {
    let declared = support::harness_event_kinds();
    assert_eq!(
        declared.len(),
        50,
        "io-harness 0.60.1 declares fifty event kinds; found {}",
        declared.len(),
    );

    let mut known: Vec<&str> = STYLED.to_vec();
    known.extend_from_slice(FALLS_THROUGH);

    let unseen: Vec<&String> = declared
        .iter()
        .filter(|name| !known.contains(&name.as_str()))
        .collect();
    assert!(
        unseen.is_empty(),
        "io-harness emits kinds this repository has never seen: {unseen:?}. \
         Decide whether each is styled or falls through, and add it to the list.",
    );

    let gone: Vec<&&str> = known
        .iter()
        .filter(|name| !declared.contains(&(**name).to_string()))
        .collect();
    assert!(
        gone.is_empty(),
        "these names are no longer io-harness event kinds: {gone:?}",
    );
}
