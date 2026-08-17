//! F8 — every event kind is accounted for.
//!
//! `io_harness::EventKind` is `#[non_exhaustive]`, so a wildcard arm is required
//! by the type rather than chosen. The criterion is therefore asserted as the two
//! things that actually matter: no kind renders to nothing, and no kind exists
//! that this repository has never seen.

mod support;

use std::time::Duration;

use io_cli::events::{kind_name, Events};
use io_cli::theme::DARK;
use io_harness::{EventKind, RunEvent};

/// The kinds this release handles, in the contract's own words: `started`,
/// `token`, `step`, `tool_call` and `finished` rendered fully, `refused` and
/// `approval_requested` as a plain one-line notice.
///
/// `mcp` is here from 0.3.0 for a reason that is not visible in its own line: it
/// is the only event io-harness emits carrying how long a tool actually ran, and
/// that number is harvested onto the open tool cell. Its own line is still the
/// muted one naming the kind — an event is never consumed silently — but it is no
/// longer a kind this release merely passes through.
const STYLED: &[&str] = &[
    "started",
    "token",
    "step",
    "tool_call",
    "finished",
    "refused",
    "approval_requested",
    "mcp",
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

/// What one event commits, at a session age nothing measured.
///
/// `Duration::ZERO` is the right default for every test that is not about a tool
/// cell's duration: the age is an argument handed in by the driver, so a test
/// that does not care about it states zero rather than arranging for a clock.
fn rendered(events: &mut Events, kind: EventKind) -> String {
    rendered_at(events, kind, Duration::ZERO)
}

/// The same, at a stated session age. Stated, never measured — N1.
fn rendered_at(events: &mut Events, kind: EventKind, at: Duration) -> String {
    flatten(events.event(&event(kind), at))
}

fn flatten(lines: Vec<ratatui::text::Line<'static>>) -> String {
    lines
        .iter()
        .flat_map(|line| line.spans.iter())
        .map(|span| span.content.as_ref())
        .collect()
}

#[test]
fn f5_a_step_reads_decision_then_tool_then_result_then_its_metadata() {
    let mut events = Events::new(DARK);
    let line: String = flatten(events.event(
        &RunEvent::new(
            1,
            7,
            EventKind::Step {
                decision: "edited the parser".into(),
                tool_call: "apply_patch src/lib.rs".into(),
                tokens: 1234,
                changed: true,
            },
        ),
        Duration::ZERO,
    ));

    // The order is the assertion, not the contents. A line carrying all five
    // facts in the wrong order is the line 0.1.0 shipped.
    let at = |needle: &str| {
        line.find(needle)
            .unwrap_or_else(|| panic!("{needle:?} is missing from {line:?}"))
    };
    let decision = at("edited the parser");
    let tool = at("apply_patch src/lib.rs");
    let result = at("changed files");
    let tokens = at("1234 tok");
    let step = at("step 7");

    assert!(
        decision < tool,
        "the tool came before the decision: {line:?}"
    );
    assert!(tool < result, "the result came before the tool: {line:?}");
    assert!(
        result < tokens,
        "the token count came before the result: {line:?}",
    );
    assert!(
        tokens < step,
        "the step number came before the token count: {line:?}",
    );
}

#[test]
fn f5_a_step_that_changed_nothing_still_says_so() {
    let mut events = Events::new(DARK);
    let line: String = flatten(events.event(
        &RunEvent::new(
            1,
            2,
            EventKind::Step {
                decision: "read the failing test".into(),
                tool_call: String::new(),
                tokens: 88,
                changed: false,
            },
        ),
        Duration::ZERO,
    ));

    // The result is always present, so a skim down the transcript reads the same
    // column of answers whether or not a step touched anything.
    assert!(line.contains("no change"), "{line:?}");
    assert!(
        line.find("read the failing test") < line.find("no change"),
        "{line:?}",
    );
    assert!(line.find("no change") < line.find("88 tok"), "{line:?}");
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

    // A tool call's facts are asserted where they are committed, which is the step
    // that finished it. The announcement itself commits nothing, because when it
    // arrives io-harness does not yet know what came back.
    events.event(
        &event(EventKind::ToolCall {
            name: "exec".into(),
            target: "cargo test".into(),
        }),
        Duration::ZERO,
    );
    let tool = rendered_at(
        &mut events,
        EventKind::Step {
            decision: "ran cargo test".into(),
            tool_call: "exec".into(),
            tokens: 12,
            changed: false,
        },
        Duration::from_millis(900),
    );
    assert!(tool.contains("exec"), "{tool:?}");
    assert!(tool.contains("cargo test"), "{tool:?}");
    assert!(tool.contains("ran cargo test"), "{tool:?}");

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
        let committed = events.event(
            &event(EventKind::Token { text: word.into() }),
            Duration::ZERO,
        );
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

/// **F8.** No event is dropped.
///
/// The invariant is not "every event commits a line" — a token does not, and from
/// 0.3.0 neither does a tool call. Both are *deferred* rather than discarded: the
/// token into the live buffer, the call into an open cell that `live()` shows
/// immediately and that the step commits complete. So each kind must leave a mark
/// somewhere a reader can see it, and the two that defer are named here rather
/// than exempted, so that a third one cannot be added silently.
#[test]
fn f8_no_event_is_dropped_though_a_token_and_a_tool_call_are_deferred() {
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
            name: "read_file".into(),
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
        let lines = events.event(&event(kind), Duration::ZERO);
        if name == "tool_call" {
            // Deferred, and visible while it is deferred. An open call that
            // committed nothing AND showed nothing would be the discarded event
            // this test exists to catch.
            assert!(
                lines.is_empty(),
                "a tool call committed a line before its result was known",
            );
            assert!(
                events.live().contains("read_file"),
                "a deferred tool call must be visible in the viewport: {:?}",
                events.live(),
            );
            continue;
        }
        assert!(
            !lines.is_empty(),
            "{name} rendered to nothing, which is an event silently discarded",
        );
    }

    // The turn ends, and the call that nothing ever reported on is still accounted
    // for rather than lost with the turn.
    let closed = flatten(events.flush());
    assert!(closed.contains("read_file"), "{closed:?}");
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
    //
    // 0.2.0 changes what the way out *is* — an approval is now answered on screen,
    // so the help no longer sends everyone to `io setup` — but not that there has
    // to be one. The assertion is therefore that each help names something the
    // operator can actually do next, rather than one particular sentence.
    for outcome in ["awaiting_answer", "awaiting_approval", "awaiting_plan"] {
        assert_eq!(outcome_tone(outcome), Tone::Warning, "{outcome}");
        let help = outcome_help(outcome)
            .unwrap_or_else(|| panic!("{outcome} leaves the operator with no next action"));
        assert!(
            ["io setup", "Shift+Tab", "next prompt"]
                .iter()
                .any(|way| help.contains(way)),
            "{outcome} should name something the operator can do: {help}",
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
        line.contains("next prompt"),
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

/// **F8.** The two facts no other core records, in the order a reader needs them,
/// asserted by position. A `contains` assertion is green when the sentence is
/// inside out — which 0.1.1 paid to learn.
#[test]
fn f8_a_refusal_names_act_then_target_then_rule_then_layer() {
    let mut events = Events::new(DARK);
    let refused = rendered(
        &mut events,
        EventKind::Refused {
            act: "write".into(),
            target: "/etc/hosts".into(),
            rule: Some("fs.deny".into()),
            layer: Some("ops-baseline".into()),
        },
    );

    let at = |needle: &str| {
        refused
            .find(needle)
            .unwrap_or_else(|| panic!("{needle:?} is not in the line: {refused:?}"))
    };
    assert!(at("write") < at("/etc/hosts"), "act first: {refused:?}");
    assert!(
        at("/etc/hosts") < at("fs.deny"),
        "the target before the rule: {refused:?}",
    );
    assert!(
        at("fs.deny") < at("ops-baseline"),
        "the rule before the layer it came from: {refused:?}",
    );
    assert!(
        refused.contains("refused"),
        "colour is never the only carrier of a meaning: {refused:?}",
    );
}

/// **F8, the other half.** In io-harness a missing rule means the *tier default*
/// decided — the least vouched-for kind of action, not the most. A line that
/// renders nothing there tells the reader the opposite of what happened.
#[test]
fn f8_a_refusal_with_no_rule_says_the_tier_default_decided() {
    let mut events = Events::new(DARK);
    let refused = rendered(
        &mut events,
        EventKind::Refused {
            act: "net".into(),
            target: "api.example.com:443".into(),
            rule: None,
            layer: None,
        },
    );

    assert!(refused.contains("api.example.com:443"), "{refused:?}");
    assert!(
        refused.contains("tier default"),
        "an unnamed refusal must say what refused it: {refused:?}",
    );
}

/// **F2.** A call announced is not a call finished.
///
/// `EventKind::ToolCall` is emitted before the tool runs, so anything committed
/// here could only say what the agent was about to do — which is the line 0.2.0
/// shipped and the reason a transcript never said what came back. It is held
/// open instead, and shown live, which is the half of the design that keeps the
/// deferral from being a disappearance.
#[test]
fn f2_a_tool_call_commits_nothing_until_its_step_lands() {
    let mut events = Events::new(DARK);
    let lines = events.event(
        &event(EventKind::ToolCall {
            name: "read_file".into(),
            target: "src/lib.rs".into(),
        }),
        Duration::ZERO,
    );

    assert!(
        lines.is_empty(),
        "a call committed a line before anything knew its result: {lines:?}",
    );
    let live = events.live();
    assert!(live.contains("read_file"), "{live:?}");
    assert!(live.contains("src/lib.rs"), "{live:?}");
}

/// **F2.** The whole cell, in one line, when the step that ran it commits.
///
/// Asserted by position rather than by presence. A line carrying all four facts
/// inside out passes `contains` just as green, which this repository has now paid
/// to learn twice.
#[test]
fn f2_a_tool_cell_commits_its_result_and_its_observed_duration() {
    let mut events = Events::new(DARK);
    events.event(
        &event(EventKind::ToolCall {
            name: "read_file".into(),
            target: "src/lib.rs".into(),
        }),
        Duration::ZERO,
    );
    let line = rendered_at(
        &mut events,
        EventKind::Step {
            // io-harness's own sentence about what the call did, written after it
            // ran. The cell says this and never a word io-cli made up.
            decision: "read src/lib.rs".into(),
            tool_call: "read_file".into(),
            tokens: 5,
            changed: false,
        },
        Duration::from_millis(250),
    );

    let at = |needle: &str| {
        line.find(needle)
            .unwrap_or_else(|| panic!("{needle:?} is missing from {line:?}"))
    };
    assert!(
        at("read_file") < at("src/lib.rs"),
        "the tool before its target: {line:?}",
    );
    assert!(
        at("src/lib.rs") < at("read src/lib.rs"),
        "the target before what came back: {line:?}",
    );
    assert!(
        at("read src/lib.rs") < at("~250ms"),
        "content before metadata: {line:?}",
    );

    // The `~` is load-bearing. io-cli did not time the tool; it observed the
    // interval between two events, which also contains the model's turnaround and
    // whatever queued in front of the call. A bare `250ms` here would be io-cli
    // claiming a measurement it never made.
    assert!(
        !line.contains(" 250ms"),
        "an observed interval must not read as a measured duration: {line:?}",
    );
}

/// **F2.** A refused call never closes as a completed one.
///
/// A refusal does not end the step — io-harness feeds it back to the model as an
/// observation and the step commits anyway — so the cell is still closed by that
/// step. What it must not do is close wearing the step's own verdict, which would
/// report a call that was stopped as a call that ran.
#[test]
fn f2_a_refused_call_does_not_close_as_a_completed_one() {
    let mut events = Events::new(DARK);
    events.event(
        &event(EventKind::ToolCall {
            name: "write_file".into(),
            target: "src/main.rs".into(),
        }),
        Duration::ZERO,
    );
    rendered(
        &mut events,
        EventKind::Refused {
            act: "write".into(),
            target: "/repo/src/main.rs".into(),
            rule: Some("fs.deny".into()),
            layer: Some("workspace".into()),
        },
    );

    // Two decisions against one open call: the count does not pair, which is the
    // ordinary shape of a step that also waited on an approval. The refusal is
    // then the only true thing left to say about the call.
    let line = rendered_at(
        &mut events,
        EventKind::Step {
            decision: "write refused; awaiting approval (write)".into(),
            tool_call: "write_file".into(),
            tokens: 40,
            changed: false,
        },
        Duration::from_millis(300),
    );

    assert!(line.contains("refused"), "{line:?}");
    assert!(
        !line.contains("changed files"),
        "a refused call must not close as one that changed something: {line:?}",
    );
    assert!(
        line.find("refused") < line.find("no change"),
        "the cell closes before the step it belonged to: {line:?}",
    );
}

/// **F2.** A call nothing ever reported on closes as unfinished, with no number.
///
/// `Step` may never arrive: io-harness skips `commit_step` when a sub-agent's
/// child deferred, and a turn can be interrupted mid-call. The cell is still
/// accounted for — and carries no duration, because io-cli knows when the call
/// was announced and nothing whatever about when it stopped.
#[test]
fn f2_an_unfinished_call_closes_without_inventing_a_duration() {
    let mut events = Events::new(DARK);
    events.event(
        &event(EventKind::ToolCall {
            name: "read_file".into(),
            target: "src/lib.rs".into(),
        }),
        Duration::ZERO,
    );

    let line = flatten(events.flush());
    assert!(line.contains("read_file"), "{line:?}");
    assert!(line.contains("unfinished"), "{line:?}");
    assert!(
        !line.contains('~') && !line.contains("ms"),
        "a duration here would be a guess wearing a measurement's clothes: {line:?}",
    );

    // Closed once. A cell that survived its own closing would arrive again in the
    // next turn's scrollback.
    assert_eq!(events.live(), "");
}

/// **F2.** A parallel batch is the ordinary case, not the edge one.
///
/// `read_batch` announces every call in a batch up front and only then runs any of
/// them, so two calls before one step is what a normal parallel read looks like. A
/// single-slot design is wrong on the first one, and a single shared duration
/// would hide which of the two was slow.
#[test]
fn f2_two_calls_before_one_step_each_close_with_their_own_duration() {
    let mut events = Events::new(DARK);
    events.event(
        &event(EventKind::ToolCall {
            name: "read_file".into(),
            target: "src/a.rs".into(),
        }),
        Duration::ZERO,
    );
    events.event(
        &event(EventKind::ToolCall {
            name: "read_file".into(),
            target: "src/b.rs".into(),
        }),
        Duration::from_millis(100),
    );

    let line = rendered_at(
        &mut events,
        EventKind::Step {
            decision: "read src/a.rs; read src/b.rs".into(),
            tool_call: "read_file".into(),
            tokens: 60,
            changed: false,
        },
        Duration::from_millis(300),
    );

    let at = |needle: &str| {
        line.find(needle)
            .unwrap_or_else(|| panic!("{needle:?} is missing from {line:?}"))
    };
    // In the order they were announced, each paired with the decision written for
    // it — and each measured from its own announcement, so the second call is not
    // charged for the time the first one spent.
    assert!(at("src/a.rs") < at("src/b.rs"), "{line:?}");
    assert!(at("~300ms") < at("~200ms"), "{line:?}");
}

/// A rule with no layer is possible and is not the same as no rule at all. It is
/// rendered as what it is rather than being rounded to either neighbour.
#[test]
fn f8_a_rule_without_a_layer_is_neither_of_the_other_two_cases() {
    let mut events = Events::new(DARK);
    let refused = rendered(
        &mut events,
        EventKind::Refused {
            act: "exec".into(),
            target: "curl".into(),
            rule: Some("*.sh".into()),
            layer: None,
        },
    );

    assert!(refused.contains("*.sh"), "{refused:?}");
    assert!(
        !refused.contains("tier default"),
        "a named rule did decide this one: {refused:?}",
    );
}
