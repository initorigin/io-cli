//! F8 — every event kind is accounted for.
//!
//! `io_harness::EventKind` is `#[non_exhaustive]`, so a wildcard arm is required
//! by the type rather than chosen. Through 0.10.0 the criterion was asserted as
//! the two things that mattered then: no kind rendered to nothing, and no kind
//! existed that this repository had never seen.
//!
//! **0.11.0's F1 replaced the first half.** "No kind renders to nothing" was
//! true because the wildcard printed the variant's own name, which is the defect
//! this release removed — so what is asserted now is that every kind has a
//! *disposition*, and `tests/triage.rs` owns that. What stays here is what each
//! designed line actually says.

mod support;

use std::time::Duration;

use io_cli::events::{kind_name, Events};
use io_cli::theme::{Tone, DARK};
use io_harness::{EventKind, RunEvent, TodoItem, TodoState, TODO_MAX_ITEMS};

// The `STYLED` and `FALLS_THROUGH` lists stood here until 0.11.0. They were a
// pair because the second one was a real destination — a muted line naming the
// kind — and a name could be moved between them without any arm changing, which
// this file's own comment said out loud. `triage::TRIAGE` replaced both with one
// list that says what each kind does and where its fact goes, and
// `tests/triage.rs` is what holds it to the locked harness.

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

/// The same lines kept apart, one string per committed row.
///
/// [`flatten`] glues every line into one string, which is enough for a claim
/// about order and blind to a claim about a *row*. A plan is a list, so the facts
/// that matter — this item carries this state, and no row is wider than the
/// terminal it was written for — are per-row facts and are asserted as such.
fn rows(lines: Vec<ratatui::text::Line<'static>>) -> Vec<String> {
    lines
        .iter()
        .map(|line| {
            line.spans
                .iter()
                .map(|span| span.content.as_ref())
                .collect()
        })
        .collect()
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
    // **0.11.0 — the goal, and nothing else.** The provider was a second row
    // under every prompt in a session; it is a status-line field now, and
    // `tests/status.rs` is where it is asserted. A default session must not
    // print it here at all: F2 asserts the removal against a real run, and this
    // is the same claim at the unit level.
    assert!(
        !started.contains("openrouter"),
        "the provider is a status-line field and not a row under the goal: {started:?}",
    );

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
    // `Run`, the verb F4 maps `exec` to, and not `exec` itself.
    assert!(tool.contains("Run"), "{tool:?}");
    assert!(!tool.contains("exec"), "{tool:?}");
    assert!(tool.contains("cargo test"), "{tool:?}");
    // The harness's sentence is `ran cargo test`; the cell has already said
    // `Run` and `cargo test`, so what it adds is `ran` and that is what it
    // carries. `f4_the_result_says_what_it_adds_and_not_what_the_cell_already_said`
    // is where that rule is asserted in full.
    assert!(tool.contains("ran"), "{tool:?}");
    assert!(!tool.contains("ran cargo test"), "{tool:?}");

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

    // The request commits nothing in a session — the overlay is on screen
    // saying it — and commits the line in plain mode, which has no overlay.
    let quiet = rendered(
        &mut events,
        EventKind::ApprovalRequested {
            act: "exec".into(),
            target: "rm -rf build".into(),
        },
    );
    assert!(quiet.is_empty(), "the overlay says this one: {quiet:?}");

    let mut plain = Events::new(DARK);
    plain.set_plain(true);
    let approval = rendered(
        &mut plain,
        EventKind::ApprovalRequested {
            act: "exec".into(),
            target: "rm -rf build".into(),
        },
    );
    assert!(approval.contains("warning"), "{approval:?}");
    assert!(approval.contains("rm -rf build"), "{approval:?}");

    // **0.11.0 — a turn ends on its answer.** The row of arithmetic under it is
    // gone: the step count and the token total are status-line fields, and a
    // plain finish has nothing left to say that the answer above it did not.
    let finished = rendered(
        &mut events,
        EventKind::Finished {
            outcome: "success".into(),
            steps: 7,
            tokens: 9876,
        },
    );
    assert!(
        finished.trim().is_empty(),
        "a turn that finished should end on its answer, not on a row about itself: {finished:?}",
    );
}

/// **F1.** The rule this test asserted is the one 0.11.0 reversed.
///
/// It required an unstyled kind to name itself, which is how `replan` — the
/// example it used — reached an operator as a Rust variant. `Replan` now has a
/// designed line, and the property worth keeping from the old test is that the
/// line says what happened in words rather than in the enum's vocabulary.
#[test]
fn f1_a_kind_with_a_designed_line_says_what_happened_and_never_its_own_name() {
    let mut events = Events::new(DARK);
    let line = rendered(&mut events, EventKind::Replan { window: 3 });
    assert!(
        line.contains("3 steps"),
        "the window is the fact an operator can act on: {line:?}",
    );
    assert!(
        !line.contains("replan"),
        "the variant's own name must not reach the transcript: {line:?}",
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

/// **F8, as 0.11.0 leaves it.** Every kind with a designed line still commits
/// one, and the two that defer still defer.
///
/// The old invariant — *no* kind renders to nothing — died with the wildcard that
/// made it true, and `Stalled` is where that shows: it is silent here now,
/// because the run's own outcome carries the word and a line beside it would say
/// the same thing twice. What this test still owns is that a `Line` kind is not
/// quietly emptied, and that a token and a tool call are deferred rather than
/// discarded.
#[test]
fn f8_a_line_kind_commits_one_though_a_token_and_a_tool_call_are_deferred() {
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
        EventKind::Replan { window: 8 },
        EventKind::Retry {
            kind: "timeout".into(),
            attempt: 2,
            delay_ms: 400,
        },
        EventKind::FellBackTo {
            provider: "anthropic".into(),
        },
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
                events.live().contains("Read"),
                "a deferred tool call must be visible in the viewport: {:?}",
                events.live(),
            );
            continue;
        }
        // **0.11.0 — the approval overlay says this one, larger.** A committed
        // line as well put `warning: write SUMMARY.md — waiting for you` directly
        // above the overlay's own `warning: write SUMMARY.md`, which is the same
        // sentence twice in two sizes. In plain mode, which draws no overlay, the
        // line is still committed — `tests/plain.rs` is where that is asserted.
        if name == "approval_requested" {
            assert!(
                lines.is_empty(),
                "the overlay says this one; a line here says it twice",
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
    assert!(closed.contains("Read"), "{closed:?}");
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

    // **0.11.0 — the outcomes an operator meets most now say what they mean.**
    // A real run ended with `error: step_cap_reached` over a prompt and nothing
    // else, which says whether the run stopped but not whether that was a crash,
    // a refusal or a ceiling. The harness's own word still leads the line; the
    // sentence under it is this crate's.
    for outcome in [
        "step_cap_reached",
        "stalled",
        "time_budget_exceeded",
        "cost_budget_exceeded",
        "budget_ceiling_reached",
        "plan_rejected",
        "cancelled",
        "awaiting_recovery",
        "escalated",
    ] {
        assert!(outcome_help(outcome).is_some(), "{outcome}");
    }

    // A turn that ended well needs no explanation and does not get one.
    for outcome in ["finished", "success"] {
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

/// **0.11.0.** A finished turn says nothing about itself in a session, and says
/// everything in a plain one.
///
/// The two halves are one test because the property is the trade: the row was
/// removed for a reader who can see the status line, and kept for the reader who
/// cannot. Splitting them would let either half pass while the other silently
/// did the opposite.
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
    assert!(
        line.trim().is_empty(),
        "a plain finish commits nothing but the blank line: {line:?}",
    );

    let mut plain = Events::new(DARK);
    plain.set_plain(true);
    let line = rendered(
        &mut plain,
        EventKind::Finished {
            outcome: "finished".into(),
            steps: 8,
            tokens: 32624,
        },
    );
    assert!(line.contains("ok"), "{line:?}");
    assert!(line.contains("8 steps"), "{line:?}");
    // The status line's own spelling, which is the point of committing this row
    // in plain mode at all: a plain session met `32624 tok` here and `32.6k tok`
    // on the line, and that is one fact with two spellings.
    assert!(line.contains("32.6k tok"), "{line:?}");
    assert!(!line.contains("32624"), "{line:?}");
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
    // The drift check itself moved to `tests/triage.rs`, which compares the
    // locked harness's declared kinds against the disposition table rather than
    // against a pair of lists nothing behind them had to agree with. What is
    // left here is the count, because this file's own fixtures are written
    // against it.
    let declared = support::harness_event_kinds();
    assert_eq!(
        declared.len(),
        51,
        "the locked io-harness declares fifty-one event kinds; found {}",
        declared.len(),
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
    // The verb, in the live row as in the committed cell: 0.11.0's F4 maps the
    // name once, where the call is opened, so both say the same word.
    assert!(live.contains("Read"), "{live:?}");
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
    // `Read`, not `read_file`: 0.11.0's F4 put the operator's verb in this
    // column.
    assert!(
        at("Read") < at("src/lib.rs"),
        "the tool before its target: {line:?}",
    );
    assert!(
        at("src/lib.rs") < at("~250ms"),
        "content before metadata: {line:?}",
    );
    // **And the result column says what it ADDS.** io-harness's sentence here is
    // `read src/lib.rs`, which is the tool and the target in the harness's own
    // words — the two things this cell has already said in the operator's. A
    // real run committed `⋅ Read io.toml · read io.toml · ~0ms`, and reading it
    // back is what found this.
    assert!(
        !line.contains("read src/lib.rs"),
        "the cell said the same thing twice in two vocabularies: {line:?}",
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
    assert!(line.contains("Read"), "{line:?}");
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

/// **F11.** The plan is the assertion: every item's own words and its own state,
/// in the order the agent wrote them.
///
/// Asserted per row rather than over the whole passage, because a state word
/// sitting somewhere in the transcript is not the same claim as a state word
/// sitting on the item it belongs to — a list whose states had all slid down by
/// one would pass a `contains` sweep unchanged. The three words are read off
/// `TodoState::as_str` rather than typed here, so a release that renames one
/// fails this test instead of quietly inventing a label of io-cli's own.
///
/// This is the test the sabotage kills. Deleting the match arm sends the kind
/// back through the wildcard, which commits the single word `todo_wrote` and not
/// one item of the plan.
#[test]
fn f11_a_plan_commits_every_item_with_its_own_state_word_in_order() {
    let mut events = Events::new(DARK);
    let plan = rows(events.event(
        &event(EventKind::TodoWrote {
            items: vec![
                TodoItem::new("read the current parser", TodoState::Done),
                TodoItem::new("port the tokenizer", TodoState::Active),
                TodoItem::new("port the error paths", TodoState::Pending),
            ],
        }),
        Duration::ZERO,
    ));

    let row_of = |text: &str, state: TodoState| {
        let (index, row) = plan
            .iter()
            .enumerate()
            .find(|(_, row)| row.contains(text))
            .unwrap_or_else(|| panic!("{text:?} never reached the transcript: {plan:?}"));
        let word = state.as_str();
        assert!(
            row.contains(word),
            "the state is carried as a word and not only as a colour: {row:?}",
        );
        assert!(
            row.find(text) < row.find(word),
            "content before its state, like every other line here: {row:?}",
        );
        index
    };

    let read = row_of("read the current parser", TodoState::Done);
    let tokenizer = row_of("port the tokenizer", TodoState::Active);
    let errors = row_of("port the error paths", TodoState::Pending);
    assert!(
        read < tokenizer && tokenizer < errors,
        "the plan is committed in the order the agent wrote it: {plan:?}",
    );
}

/// **F11.** An item is fitted to the row, and what it was cut with says so.
///
/// `TODO_TEXT_CAP` is two hundred characters and the terminal this product is
/// audited at is eighty columns, so an unfitted plan of any length is a wall of
/// wrapped rows rather than a list. The elision mark is the half that keeps the
/// shortening honest: a row cut with nothing to show for it reads as an item the
/// agent wrote that way.
#[test]
fn f11_an_item_longer_than_the_row_is_fitted_and_says_it_was_cut() {
    let mut events = Events::new(DARK);
    // Two hundred and ten characters — longer than the store's own text cap, which
    // the event is not subject to either.
    let long = "port the error paths ".repeat(10);
    let plan = rows(events.event(
        &event(EventKind::TodoWrote {
            items: vec![TodoItem::new(long, TodoState::Pending)],
        }),
        Duration::ZERO,
    ));

    let row = plan
        .iter()
        .find(|row| row.contains("port the error paths"))
        .unwrap_or_else(|| panic!("the item never reached the transcript: {plan:?}"));
    assert!(
        row.contains(DARK.glyphs.ellipsis),
        "a shortened item has to say it was shortened: {row:?}",
    );
    assert!(
        row.contains(TodoState::Pending.as_str()),
        "the state survives the fitting; it is not what gets cut: {row:?}",
    );
    for row in &plan {
        // Counted in characters, never in bytes: this row is not ASCII.
        assert!(
            row.chars().count() <= 80,
            "a plan row overran the eighty columns this product is audited at: {row:?}",
        );
    }
}

/// **F11.** A plan longer than the store keeps is disclosed, not silently cut.
///
/// The event carries the model's list *before* the cap is applied — the
/// dispatcher clones `items` and only then does `Store::write_todos` keep
/// `TODO_MAX_ITEMS` of them, and the dropped count never reaches any event. So
/// this line is the only place the whole length is knowable, and an operator who
/// is not told here will read a plan of sixty-four and never learn the agent
/// wrote more.
#[test]
fn f11_a_plan_longer_than_the_store_keeps_says_how_much_longer() {
    let mut events = Events::new(DARK);
    // Deliberately wordless: an item named "item 70" would make the assertions
    // below green on its own text rather than on the disclosure.
    let total = TODO_MAX_ITEMS + 6;
    let items: Vec<TodoItem> = (0..total)
        .map(|_| TodoItem::new("a step in the plan", TodoState::Pending))
        .collect();
    let plan = rows(events.event(&event(EventKind::TodoWrote { items }), Duration::ZERO));

    assert_eq!(
        plan.iter()
            .filter(|row| row.contains("a step in the plan"))
            .count(),
        total,
        "the whole list is committed, since the event is what carries it: {plan:?}",
    );

    let notice = plan
        .iter()
        .find(|row| row.contains("warning"))
        .unwrap_or_else(|| panic!("a plan over the cap was committed in silence: {plan:?}"));
    assert!(
        notice.contains(&total.to_string()),
        "the disclosure states what the agent wrote: {notice:?}",
    );
    assert!(
        notice.contains(&TODO_MAX_ITEMS.to_string()),
        "and what the store keeps of it: {notice:?}",
    );
}

/// **F11.** A write of *no* items is not a plan, and does not commit one.
///
/// io-harness accepts it: `parse_todo_items` validates each item it is handed and
/// never rejects an empty list, so `{"items": []}` reaches this module as a real
/// `TodoWrote`. Before the guard, that committed a header reading `0 of 0 done, by
/// the agent's own account` with not one row under it — the placeholder F12's
/// sabotage arm names, written into the transcript instead of the status line.
///
/// Through 0.10.0 the event was still committed, as the muted word naming
/// itself. **0.11.0's F1 is why that stops:** `todo_wrote` is triaged as a line,
/// and a `Line` kind whose arm declines this particular payload commits nothing
/// rather than falling through to its own variant name. The empty write is not
/// counted as an unknown kind either — it is a kind with a disposition, and the
/// arm made a judgement about the payload.
#[test]
fn f11_a_plan_of_no_items_is_not_committed_as_a_plan() {
    let mut events = Events::new(DARK);
    let committed = rows(events.event(
        &event(EventKind::TodoWrote { items: Vec::new() }),
        Duration::ZERO,
    ));

    for row in &committed {
        assert!(
            !row.contains("plan"),
            "an empty write was committed as a plan: {committed:?}",
        );
        assert!(
            !row.contains("0 of 0"),
            "a plan of nothing is not a plan of zero: {committed:?}",
        );
    }
    assert!(
        committed.is_empty(),
        "an empty write has nothing to say and must commit nothing: {committed:?}",
    );
    assert_eq!(
        events.unknown(),
        0,
        "`todo_wrote` has a disposition; declining a payload is not an unheard-of kind",
    );
}

/// 0.8.0 F2 — a spawn commits where it happens, indented by the event's own depth.
///
/// The event is attributed to the PARENT: `run_id` is the parent's and `depth` is
/// the parent's, while `child_run_id` is on the event. Indenting by the child's
/// level instead would be invisible at depth one and a level too far in past it,
/// which is the sabotage arm this asserts against.
#[test]
fn f2_a_spawn_names_the_child_and_indents_by_the_parents_depth() {
    let mut events = Events::new(DARK);
    let root = flatten(events.event(
        &RunEvent::at_depth(
            1,
            2,
            0,
            EventKind::Spawned {
                child_run_id: 7,
                goal: "read every file under src/".into(),
            },
        ),
        Duration::ZERO,
    ));
    assert!(root.contains("run 7"), "the child is named: {root:?}");
    assert!(
        root.contains("read every file under src/"),
        "and what it was asked to do: {root:?}",
    );
    // **Absolute, not only relative.** The leader itself begins with two spaces,
    // so a spawn by the root starts exactly there — and this line is the whole
    // point of the pair: a sabotage that indents every row by one level too many
    // is invisible to a test that only compares two rows with each other, which
    // is what this test did until the arm for it killed nothing.
    let root_indent = root.len() - root.trim_start().len();
    assert_eq!(
        root_indent, 2,
        "a spawn by the root sits at the leader and not a level in front of it: \
         {root:?}",
    );

    let deeper = events.event(
        &RunEvent::at_depth(
            7,
            1,
            1,
            EventKind::Spawned {
                child_run_id: 9,
                goal: "and the tests".into(),
            },
        ),
        Duration::ZERO,
    );
    let deeper = rows(deeper);
    let row = deeper.first().expect("one row");
    let deep_indent = row.len() - row.trim_start().len();
    assert_eq!(
        deep_indent,
        root_indent + 2,
        "a spawn by a depth-1 agent sits exactly one level in from the root's: \
         {root:?} then {row:?}",
    );
}

/// 0.8.0 F3 — a refused spawn names the cap in words, and says the run continues.
#[test]
fn f3_a_refused_spawn_names_which_cap_refused_it() {
    for (cap, expected) in [
        ("agents", "as many agents as it may"),
        ("depth", "nest deeper"),
        ("budget", "token ceiling"),
    ] {
        let mut events = Events::new(DARK);
        let line = rendered(
            &mut events,
            EventKind::SpawnRefused {
                cap: cap.to_string(),
            },
        );
        assert!(
            line.contains(expected),
            "{cap:?} should render as {expected:?}: {line:?}",
        );
        assert!(
            line.contains("goes on with what it has"),
            "a refusal is not an error; the parent adapts: {line:?}",
        );
    }
}

/// 0.8.0 F3 — a cap this release has never heard of is printed as it came.
///
/// The sabotage arm folds an unknown cap into `agents`, which would put a
/// sentence on screen asserting a cap that refused nothing. Concurrency is the
/// live example: crossing `max_concurrent_agents` queues a child and reports
/// `Fleet`, so it must never arrive here — and if io-harness ever did send a
/// fourth word, saying it beats guessing.
#[test]
fn f3_an_unknown_cap_is_not_folded_into_a_known_one() {
    let mut events = Events::new(DARK);
    let line = rendered(
        &mut events,
        EventKind::SpawnRefused {
            cap: "concurrency".to_string(),
        },
    );
    assert!(
        line.contains("concurrency"),
        "an unknown cap is named as it came: {line:?}",
    );
    assert!(
        !line.contains("as many agents as it may"),
        "and never rendered as a cap that did not refuse it: {line:?}",
    );
}

/// 0.8.0 F8 — a collected report belongs to the tree, not to a child.
///
/// `ChildCollected` carries `text` and no run id. With two children in flight
/// their reports arrive in whatever order they finish, so any attribution here
/// would be arrival order rendered in the words of a fact.
#[test]
fn f8_a_collected_report_names_no_child() {
    let mut events = Events::new(DARK);
    let line = rendered(
        &mut events,
        EventKind::Spawned {
            child_run_id: 4,
            goal: "one".into(),
        },
    );
    assert!(line.contains("run 4"));
    let line = rendered(
        &mut events,
        EventKind::Spawned {
            child_run_id: 5,
            goal: "two".into(),
        },
    );
    assert!(line.contains("run 5"));

    let collected = rendered(
        &mut events,
        EventKind::ChildCollected {
            text: "found three call sites".into(),
        },
    );
    assert!(
        collected.contains("a child reported back"),
        "the report is committed where it arrives: {collected:?}",
    );
    assert!(
        collected.contains("found three call sites"),
        "with what it said: {collected:?}",
    );
    // No run id AT ALL, rather than "not one of the two we spawned". The
    // plausible mistake is reaching for the id that is on the event — which is
    // the PARENT's — and a test naming only the children would pass while the
    // screen said the root had reported to itself.
    assert!(
        !collected.contains("run "),
        "and with no run named, because the event names none: {collected:?}",
    );
}

/// 0.8.0 F8 — a detached child is named, and said to be still running.
#[test]
fn f8_a_detached_child_is_named_and_still_running() {
    let mut events = Events::new(DARK);
    let waited = rendered(
        &mut events,
        EventKind::ChildDetached {
            child_run_id: 11,
            after: Some(Duration::from_secs(30)),
        },
    );
    assert!(waited.contains("run 11"), "{waited:?}");
    assert!(waited.contains("30 seconds"), "{waited:?}");
    assert!(
        waited.contains("still running"),
        "detaching is not stopping: {waited:?}",
    );

    let never = rendered(
        &mut events,
        EventKind::ChildDetached {
            child_run_id: 12,
            after: None,
        },
    );
    assert!(
        never.contains("without waiting"),
        "a spawn that never waited is not one that waited zero seconds: {never:?}",
    );
    assert!(never.contains("still running"), "{never:?}");
}

/// 0.8.0 F6 — a draw commits no line, and that is not the same as being dropped.
///
/// One `SpendDraw` per step of a contained turn: a line each would double the
/// transcript and say in prose what one status field says. The fact reaches
/// `App::status_from` instead, which `tests/status.rs` asserts — the same shape
/// as `ToolCall`, whose fact is committed by the `Step` that follows it.
#[test]
fn f6_a_spend_draw_commits_nothing_to_the_scrollback() {
    let mut events = Events::new(DARK);
    let committed = events.event(
        &event(EventKind::SpendDraw {
            tokens: 21,
            remaining: Some(500),
        }),
        Duration::ZERO,
    );
    assert!(
        committed.is_empty(),
        "a per-step draw is a status field, not a transcript row: {committed:?}",
    );
}

/// The events that open a step, so a thought has something to be measured from.
///
/// A thought's duration is the interval since the step it belongs to opened, and
/// a step opens on `Started` for the first one and on the previous `Step` for
/// every one after it. Stated ages, never measured ones — N1.
fn started_at(events: &mut Events, at: Duration) {
    events.event(
        &event(EventKind::Started {
            goal: "read the parser".into(),
            provider: "openrouter".into(),
        }),
        at,
    );
}

fn thought(text: &str, tokens: u64) -> EventKind {
    EventKind::Reasoning {
        text: text.into(),
        tokens,
    }
}

/// 0.11.0 F3 — one row: that it thought, how long for, what it cost.
///
/// The heading says `thought`, not `reasoning`: the variant's own name is one of
/// the six strings F2 asserts never reaches a terminal again, and this row is the
/// last place that name could survive.
///
/// **The text itself is not committed, and that is deliberate.** A thought is the
/// model talking to itself and is routinely longer than the answer it precedes; a
/// transcript carrying every one of them buries the work in the deliberation,
/// which is what a real session showed. The text is kept for `/expand`, because
/// this event is the only place it ever exists.
#[test]
fn f3_a_thought_is_one_row_and_the_text_is_kept_rather_than_committed() {
    let mut events = Events::new(DARK);
    started_at(&mut events, Duration::from_secs(1));
    let committed = rows(events.event(
        &event(thought("the parser is the only caller", 120)),
        Duration::from_millis(3_500),
    ));

    assert_eq!(committed.len(), 1, "a thought is one row: {committed:?}");
    let heading = &committed[0];
    assert!(heading.contains("thought"), "{heading:?}");
    assert!(
        !heading.contains("reasoning"),
        "the variant's own name is one of the strings F2 asserts absent: {heading:?}",
    );
    // 3.5s − 1s. The interval since the step opened, not the session's age.
    assert!(heading.contains("2.5s"), "{heading:?}");
    assert!(heading.contains("120 tok"), "{heading:?}");
    assert!(
        !heading.contains("the parser is the only caller"),
        "the thought's text belongs to `/expand`, not to the transcript: {heading:?}",
    );
    assert_eq!(events.thought(), Some("the parser is the only caller"));
}

/// 0.11.0 F3 — the block is muted throughout, and carries nothing in colour.
///
/// Asserted against the styles rather than the strings, because "no colour
/// carries meaning on its own" is a claim about the spans and is invisible to a
/// test that only reads their text.
#[test]
fn f3_a_thought_is_muted_throughout_and_says_nothing_in_colour_alone() {
    let mut events = Events::new(DARK);
    started_at(&mut events, Duration::ZERO);
    let lines = events.event(
        &event(thought("the parser is the only caller", 120)),
        Duration::from_millis(400),
    );
    // The word itself is italic — a thought is the model's own voice, set apart
    // from the tool cells around it without spending a colour on it — so the
    // claim is about the *colour* being one tone throughout, not the weight.
    let muted = DARK.style(Tone::Muted);
    for line in &lines {
        for span in &line.spans {
            assert!(
                span.style.fg == muted.fg || span.content.trim().is_empty(),
                "a thought is one tone: {:?} carries {:?}",
                span.content,
                span.style,
            );
        }
    }
}

/// 0.11.0 F3 — a long thought is fitted, and the whole of it is kept for
/// `/expand`.
///
/// Every thought is kept whole, however short, because `/expand` is the only
/// place it can be read and this event is the only place it ever exists.
#[test]
fn f3_every_thought_is_kept_whole_for_expand() {
    let mut events = Events::new(DARK);
    started_at(&mut events, Duration::ZERO);

    let long = "the parser is the only caller of this function and every other \
                path reaches it through the same entry point "
        .repeat(12);
    events.event(&event(thought(&long, 900)), Duration::from_secs(2));
    assert_eq!(events.thought(), Some(long.as_str()));

    // A short one too. The row on screen is the same row either way, so there is
    // no length at which the text stops being worth keeping.
    events.event(&event(thought("one short thought", 12)), Duration::ZERO);
    assert_eq!(events.thought(), Some("one short thought"));
}

/// 0.11.0 F3 — only a `Reasoning` event commits a thought.
///
/// The sabotage arm this criterion names is emitting the block on every event
/// that carries text, which turns the agent's own answer into a thought. The
/// answer streams as tokens and commits through the flush, so that is what this
/// drives.
///
/// **The first version of this test was blind to its own sabotage.** It streamed
/// a token ending in a newline, and the `Token` arm commits a complete line the
/// moment it arrives and drains it — so `flush_text`, which is where an answer
/// that has not ended in a newline is committed and where the sabotage lived,
/// was never reached. Both paths are driven here: a finished line and an
/// unterminated tail are two different commits of the same answer.
#[test]
fn f3_only_a_reasoning_event_commits_a_thought() {
    let mut events = Events::new(DARK);
    started_at(&mut events, Duration::ZERO);
    let mut committed = flatten(events.event(
        &event(EventKind::Token {
            text: "the file is read first.\nand the parser".into(),
        }),
        Duration::ZERO,
    ));
    committed.push_str(&flatten(events.flush()));

    assert!(
        committed.contains("the file is read first."),
        "the answer still reaches the scrollback: {committed:?}",
    );
    assert!(
        committed.contains("and the parser"),
        "so does the tail that never ended in a newline: {committed:?}",
    );
    assert!(
        !committed.contains("thought"),
        "an answer is not a thought: {committed:?}",
    );
    assert_eq!(events.thought(), None);
}

/// 0.11.0 F3 — a run that produced no reasoning commits nothing.
///
/// An empty `Reasoning` is the shape that fails this quietly: the provider
/// billed for a thought and returned no text, and a heading over an empty block
/// tells a reader the model thought nothing rather than that it did not say.
#[test]
fn f3_an_empty_thought_commits_no_block() {
    let mut events = Events::new(DARK);
    started_at(&mut events, Duration::ZERO);
    let committed = events.event(&event(thought("   \n  ", 4)), Duration::from_secs(1));
    assert!(committed.is_empty(), "{committed:?}");
    assert_eq!(events.thought(), None);
}

/// 0.11.0 F6 — the live row names the act and the target of an open call.
#[test]
fn f6_an_open_call_names_that_act_and_its_target() {
    let mut events = Events::new(DARK);
    events.set_root("/work/io-cli");
    events.event(
        &event(EventKind::ToolCall {
            name: "write_file".into(),
            target: "/work/io-cli/src/lib.rs".into(),
        }),
        Duration::ZERO,
    );
    let row = events.live();
    assert!(
        row.contains("Write") && row.contains("src/lib.rs"),
        "{row:?}"
    );
}

/// 0.11.0 F6 — with nothing open and a thought most recent, the row says so.
#[test]
fn f6_a_thought_most_recent_and_nothing_open_says_the_agent_is_thinking() {
    let mut events = Events::new(DARK);
    events.event(
        &event(thought("which caller reaches it", 20)),
        Duration::ZERO,
    );
    assert!(events.live().contains("thinking"), "{:?}", events.live());

    // And it stops being the most recent thing as soon as something else
    // happens. The answer streaming is the ordinary case.
    events.event(
        &event(EventKind::Token {
            text: "the parser".into(),
        }),
        Duration::ZERO,
    );
    let row = events.live();
    assert!(!row.contains("thinking"), "{row:?}");
    assert!(row.contains("the parser"), "{row:?}");
}

/// 0.11.0 F6 — a pending approval outranks both, and this is the sabotage arm.
///
/// Ordering thinking above waiting-on-a-person tells an operator the agent is
/// busy at the exact moment it is blocked on them — so this test arranges for all
/// three to be true at once and asserts which one the row says.
///
/// **The order the events arrive in is the whole test.** Written the other way
/// round — the thought before the call — the call clears `thinking` and the
/// sabotage has nothing to beat: the first version of this test passed with
/// thinking ranked above waiting, because by the time it looked, nothing was
/// thinking. The call is announced first here so that all three are true at the
/// moment the row is read.
#[test]
fn f6_a_pending_approval_outranks_an_open_call_and_a_thought() {
    let mut events = Events::new(DARK);
    events.event(
        &event(EventKind::ToolCall {
            name: "write_file".into(),
            target: "src/lib.rs".into(),
        }),
        Duration::ZERO,
    );
    events.event(
        &event(thought("this write needs asking about", 20)),
        Duration::ZERO,
    );
    events.event(
        &event(EventKind::ApprovalRequested {
            act: "write".into(),
            target: "src/lib.rs".into(),
        }),
        Duration::ZERO,
    );

    let row = events.live();
    assert!(
        row.contains("waiting for you"),
        "the run is blocked on a person and the row said otherwise: {row:?}",
    );

    // Decided, and the run is moving again: the open call takes the row back.
    events.event(
        &event(EventKind::ApprovalDecided {
            act: "write".into(),
            target: "src/lib.rs".into(),
            decision: "allow".into(),
        }),
        Duration::ZERO,
    );
    let row = events.live();
    assert!(!row.contains("waiting for you"), "{row:?}");
    assert!(row.contains("Write"), "{row:?}");
}

/// 0.11.0 F6 — a turn that ended while an approval was outstanding stops asking.
///
/// An interrupt between the request and the decision is the case: io-harness
/// never sends the decision that would clear it, and a row still asking for an
/// answer to a question that died with the run is a session that looks hung.
#[test]
fn f6_an_interrupted_turn_stops_waiting_on_a_person() {
    let mut events = Events::new(DARK);
    events.event(
        &event(EventKind::ApprovalRequested {
            act: "write".into(),
            target: "src/lib.rs".into(),
        }),
        Duration::ZERO,
    );
    events.flush();
    assert!(
        !events.live().contains("waiting for you"),
        "{:?}",
        events.live()
    );
}

/// One call, announced and then closed by the step that ran it.
fn cell(events: &mut Events, name: &str, target: &str) -> String {
    events.event(
        &event(EventKind::ToolCall {
            name: name.into(),
            target: target.into(),
        }),
        Duration::ZERO,
    );
    rendered_at(
        events,
        EventKind::Step {
            decision: "done".into(),
            tool_call: name.into(),
            tokens: 5,
            changed: false,
        },
        Duration::from_millis(250),
    )
}

/// 0.11.0 F4 — a mapped tool reads as a verb, and its target as a path in the
/// workspace.
#[test]
fn f4_a_mapped_tool_reads_as_a_verb_and_a_workspace_relative_path() {
    let mut events = Events::new(DARK);
    events.set_root("/work/io-cli");
    let line = cell(&mut events, "read_file", "/work/io-cli/src/lib.rs");

    assert!(line.contains("Read"), "{line:?}");
    assert!(
        !line.contains("read_file"),
        "io-harness's wire name is what this criterion removes: {line:?}",
    );
    assert!(line.contains("src/lib.rs"), "{line:?}");
    assert!(
        !line.contains("/work/io-cli/src/lib.rs"),
        "a target inside the workspace is shown relative to it: {line:?}",
    );
}

/// 0.11.0 F4 — an unmapped tool is printed exactly as io-harness sent it.
///
/// This is the criterion's own sabotage arm: a title-cased fallback would invent
/// a verb for a tool this release has never seen, which is a word in front of an
/// operator that nothing in the system means. An MCP tool and an embedder's
/// custom tool both arrive this way.
#[test]
fn f4_an_unmapped_tool_is_printed_exactly_as_it_arrived() {
    let mut events = Events::new(DARK);
    events.set_root("/work/io-cli");
    let line = cell(&mut events, "customer_lookup", "/work/io-cli/rows.csv");

    assert!(
        line.contains("customer_lookup"),
        "an unmapped name passes through whole: {line:?}",
    );
    for invented in ["Customer Lookup", "Customer_lookup", "Customer lookup"] {
        assert!(
            !line.contains(invented),
            "no verb is invented for an unknown tool: {invented:?} in {line:?}",
        );
    }
    // The target is still shortened. Which tool ran and where it ran are two
    // separate facts, and not knowing the first says nothing about the second.
    assert!(line.contains("rows.csv") && !line.contains("/work/io-cli/rows.csv"));
}

/// 0.11.0 F4 — the result column says what it adds, not what the cell said.
///
/// Every one of these came off a real run. io-harness writes the step's decision
/// in its own words, and printed whole beside a cell that has already named the
/// tool and the target it read `⋅ Read io.toml · read io.toml · ~0ms` and
/// `⋅ Search model = · "model =" (1 hits)`. What the harness ADDS — how many
/// entries, how many hits — is the part worth a column and is kept in full.
#[test]
fn f4_the_result_says_what_it_adds_and_not_what_the_cell_already_said() {
    let cases = [
        // (tool, target, io-harness's decision, what the cell should carry)
        ("read_file", "io.toml", "read io.toml", None),
        (
            "list_dir",
            "list_dir",
            "list_dir  (4 entries)",
            Some("(4 entries)"),
        ),
        (
            "grep",
            "model =",
            "grep \"model =\" (1 hits)",
            Some("(1 hits)"),
        ),
        (
            "find",
            "io.toml",
            "find io.toml (1 paths)",
            Some("(1 paths)"),
        ),
        // Nothing in common: kept exactly as it arrived.
        (
            "write_file",
            "notes.txt",
            "the file did not exist",
            Some("the file did not exist"),
        ),
    ];

    for (tool, target, decision, expected) in cases {
        let mut events = Events::new(DARK);
        events.event(
            &event(EventKind::ToolCall {
                name: tool.into(),
                target: target.into(),
            }),
            Duration::ZERO,
        );
        let line = rendered_at(
            &mut events,
            EventKind::Step {
                decision: decision.into(),
                tool_call: tool.into(),
                tokens: 5,
                changed: false,
            },
            Duration::from_millis(10),
        );

        if let Some(kept) = expected {
            assert!(
                line.contains(kept),
                "{tool}: {kept:?} missing from {line:?}"
            );
        }
        // Printed whole only when the harness said something the cell did not.
        // `expected == Some(decision)` is that case, and it is in the table on
        // purpose: a trim that ate an unrelated sentence would be worse than the
        // duplication it was written to remove.
        if expected != Some(decision) {
            assert!(
                !line.contains(decision),
                "{tool}: the harness's sentence was printed whole beside the cell \
                 that already said it: {line:?}",
            );
        }
    }
}

/// 0.11.0 F4 — a target outside the workspace is shown whole.
///
/// The fact worth seeing about a file outside the workspace is precisely that it
/// is outside one, and a `../../..` chain is less readable than the path it was
/// computed from.
#[test]
fn f4_a_target_outside_the_workspace_is_shown_whole() {
    let mut events = Events::new(DARK);
    events.set_root("/work/io-cli");
    let line = cell(&mut events, "read_file", "/etc/hosts");
    assert!(line.contains("/etc/hosts"), "{line:?}");
}

/// 0.11.0 F4 — the result and the duration columns are untouched, `~` included.
///
/// The verb changed the first column and nothing else. The `~` is the load-
/// bearing part: io-cli observed an interval between two events, it did not time
/// the tool, and a bare `250ms` would be a claim it cannot make.
#[test]
fn f4_the_result_and_the_duration_columns_are_unchanged() {
    let mut events = Events::new(DARK);
    let line = cell(&mut events, "shell", "cargo test");
    assert!(line.contains("Run"), "{line:?}");
    assert!(
        line.contains("done"),
        "the result column is still there: {line:?}"
    );
    assert!(line.contains("~250ms"), "{line:?}");
    assert!(
        !line.contains(" 250ms"),
        "an observed interval must not read as a measured one: {line:?}",
    );
}

/// 0.11.0 F4 — the table is a table: no name twice, and no empty side.
#[test]
fn f4_no_tool_is_mapped_twice_and_no_row_is_empty() {
    let mut seen = std::collections::BTreeSet::new();
    for (tool, word) in io_cli::events::VERBS {
        assert!(!tool.is_empty() && !word.is_empty(), "{tool:?} {word:?}");
        assert!(seen.insert(*tool), "{tool:?} is mapped twice");
        assert_eq!(io_cli::events::verb(tool), *word);
    }
    assert_eq!(
        io_cli::events::verb("customer_lookup"),
        "customer_lookup",
        "a name that is not in the table is not in the table",
    );
}
