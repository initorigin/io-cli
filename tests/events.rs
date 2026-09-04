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
use io_harness::{EventKind, Question, RunEvent, TodoItem, TodoState, TODO_MAX_ITEMS};

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
/// made it true, and `ApprovalRequested` is where that shows: it is silent here,
/// because the overlay is on screen at the moment the question is asked and a
/// line beside it would say the same thing twice. What this test still owns is
/// that a `Line` kind is not quietly emptied, and that a token and a tool call
/// are deferred rather than discarded.
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

    // Two words for a turn that ended well, and both are reachable from 0.24.0.
    // `finished` is a turn with no criterion ending on its own terms, which was
    // every io-cli turn until this release: a contract built here carried
    // Verification::None and `success` could not be produced from this interface
    // at all. A turn whose operator configured a gate — `io_cli::gates` — ends
    // `success` when it holds. Reading either as a warning was a real defect and
    // a live run is what found it.
    assert_eq!(outcome_tone("finished"), Tone::Success);
    assert_eq!(outcome_tone("success"), Tone::Success);

    // Stopped deliberately: not a failure, and not silence either.
    //
    // The three ceilings joined `budget_ceiling_reached` here in 0.14.0, which is
    // the release that gave an interactive session budgets to reach in the first
    // place. A ceiling is the operator's own instruction being carried out, so
    // reporting one through the error path tells them their run broke at the
    // moment their limit held — and `src/contract.rs` documents `error:
    // step_cap_reached` under an unfinished answer as the exact reason io-cli
    // raised the step floor at all. Reaching a bound you set is not a failure.
    for outcome in [
        "cancelled",
        "denied",
        "refused",
        "plan_rejected",
        "stalled",
        "budget_ceiling_reached",
        "step_cap_reached",
        "time_budget_exceeded",
        "cost_budget_exceeded",
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
        // **`/resume` since 0.32.0, and "next prompt" is gone on purpose.** The
        // two sentences that told an operator a parked question or plan could not
        // be answered by this release had been false since 0.23.0 — `/resume`
        // reopens both. An approval keeps `Shift+Tab`, because an approval belongs
        // to the turn that asked for it and there is nothing left to authorize.
        assert!(
            ["io setup", "Shift+Tab", "/resume"]
                .iter()
                .any(|way| help.contains(way)),
            "{outcome} should name something the operator can do: {help}",
        );
        assert!(
            !help.contains("no way to answer") && !help.contains("this release has no"),
            "{outcome} still claims a capability this product shipped in 0.23.0: {help}",
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
        // All three spellings io-harness writes. Only the bare one was listed
        // here, and `escalated_terminal` — the one an operator actually meets,
        // because it is what a provider refusing the request outright ends a turn
        // as — printed as a bare token with nothing under it.
        "escalated",
        "escalated_terminal",
        "escalated_retryable",
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
        line.contains("/resume"),
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
        53,
        "the locked io-harness declares fifty-three event kinds; found {}",
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

/// The goal line the operator's next words commit, at an age nothing measured.
fn goal(events: &mut Events) -> Vec<String> {
    rows(events.event(
        &event(EventKind::Started {
            goal: "read the parser".into(),
            provider: "openrouter".into(),
        }),
        Duration::ZERO,
    ))
}

/// **0.13.0 F8 — a designed block and the operator's next words are not one
/// block.**
///
/// The `›` line is the only row in a transcript that its reader wrote, and it
/// opens the block below it rather than closing the one above. Through 0.12.0 the
/// gap rule in `Events::event` (src/events.rs:451) only knew about a block that
/// ended in a tool cell, so a turn that ended on a thought footer or on a harness
/// warning put the next goal on the row directly under it — two voices, one
/// block, and nothing in the transcript saying where the operator came back in.
///
/// Each arrangement below leaves the transcript ending on a designed row that is
/// not blank, which is the state the rule has to notice. Asserted per rendered
/// row, not on the joined text: "one blank row and not two" is a claim about
/// rows, and a `str::lines()` of the flattened string cannot see it.
/// One arrangement: a name for the failure message, and the events that leave the
/// transcript ending on a designed row. Named rather than written inline because
/// the tuple is what `clippy::type_complexity` objects to, and the objection is
/// fair — the shape says nothing about what these are.
type Arrangement = (&'static str, fn(&mut Events));

#[test]
fn f8_a_designed_block_and_the_next_prompt_are_not_one_block() {
    let blocks: [Arrangement; 3] = [
        ("a thought footer", |events| {
            started_at(events, Duration::ZERO);
            events.event(
                &event(thought("the parser is the only caller", 120)),
                Duration::from_millis(400),
            );
        }),
        ("a tool cell", |events| {
            events.event(
                &event(EventKind::ToolCall {
                    name: "read_file".into(),
                    target: "src/lib.rs".into(),
                }),
                Duration::ZERO,
            );
            events.event(
                &event(EventKind::Step {
                    decision: "read src/lib.rs".into(),
                    tool_call: "read_file".into(),
                    tokens: 5,
                    changed: false,
                }),
                Duration::from_millis(250),
            );
        }),
        ("a harness warning", |events| {
            events.event(
                &event(EventKind::Retry {
                    kind: "timeout".into(),
                    attempt: 2,
                    delay_ms: 400,
                }),
                Duration::ZERO,
            );
        }),
    ];

    for (what, arrange) in blocks {
        let mut events = Events::new(DARK);
        arrange(&mut events);
        let committed = goal(&mut events);

        assert!(
            committed[0].trim().is_empty(),
            "the goal line arrived welded to {what}: {committed:?}",
        );
        // The second row is the goal itself. A second blank here is the gap an
        // operator reads as something having been left out.
        assert!(
            committed[1].contains("read the parser"),
            "two blank rows between {what} and the goal it introduces: {committed:?}",
        );
    }
}

/// **0.13.0 F8, the half that says where the rule lives.** The first prompt of a
/// session opens the transcript, so nothing goes above it.
///
/// This is the control that separates the rule taken in `Events::event` from the
/// same blank pushed inside the `Started` arm (src/events.rs:540). That arm
/// cannot see `last_blank`, so a blank pushed there is pushed unconditionally and
/// the session opens on an empty row — which is the exact case `last_blank` was
/// introduced for, and why it starts `true` (src/events.rs:270). Remove this test
/// and the sabotage passes the suite.
#[test]
fn f8_the_first_prompt_of_a_session_opens_the_transcript_rather_than_a_gap() {
    let mut events = Events::new(DARK);
    let committed = goal(&mut events);

    assert!(
        committed[0].contains("read the parser"),
        "nothing was committed before this line, so there is nothing to separate \
         it from: {committed:?}",
    );
}

/// **0.13.0 F8 — one blank between turns, still never two.**
///
/// The ordinary turn is the case that has to look right: an answer, then the next
/// prompt. `flush_text` ends the answer with a blank row (src/events.rs:438), so
/// the gap rule must stand down here — a rule that pushed a blank for every goal
/// line would put two rows of air under every answer in the session, which is the
/// same defect this criterion is about, in the other direction.
#[test]
fn f8_a_prompt_after_an_answer_is_one_blank_row_and_not_two() {
    let mut events = Events::new(DARK);
    started_at(&mut events, Duration::ZERO);
    // No trailing newline: the answer is still live, and the turn ending is what
    // commits it — together with the one blank row that ends its block.
    events.event(
        &event(EventKind::Token {
            text: "Here is the answer.".into(),
        }),
        Duration::ZERO,
    );
    let answer = rows(events.event(
        &event(EventKind::Finished {
            outcome: "finished".into(),
            steps: 1,
            tokens: 4,
        }),
        Duration::ZERO,
    ));
    assert!(
        answer.last().is_some_and(|row| row.trim().is_empty()),
        "the answer's own block should end with a blank row: {answer:?}",
    );

    let committed = goal(&mut events);
    assert!(
        committed[0].contains("read the parser"),
        "the answer already ended its block with a blank, so a second one here is \
         a gap that reads as something left out: {committed:?}",
    );
}

/// F5 — a prompt written on more than one line is committed as more than one
/// row.
///
/// A `Line` is one row and a newline inside a span is not a break: ratatui draws
/// the cells and a `\n` is not one, so through 0.13.0 a prompt of `abc` and `def`
/// was echoed as `abcdef` and the operator could not read back what they had
/// sent. The defect was reported from a capture of exactly that.
#[test]
fn f5_a_multi_line_goal_is_committed_as_its_lines() {
    let mut events = Events::new(DARK);
    let committed = rows(events.event(
        &event(EventKind::Started {
            goal: "abc\ndef\nghi".into(),
            provider: "openrouter".into(),
        }),
        Duration::ZERO,
    ));

    let said: Vec<&String> = committed
        .iter()
        .filter(|row| !row.trim().is_empty())
        .collect();
    assert_eq!(
        said.len(),
        3,
        "three lines were typed, so three rows are committed: {committed:?}",
    );
    assert!(said[0].contains("abc"), "{committed:?}");
    assert!(said[1].contains("def"), "{committed:?}");
    assert!(said[2].contains("ghi"), "{committed:?}");
    assert!(
        committed.iter().all(|row| !row.contains('\n')),
        "a committed row carries a newline, which draws as nothing: {committed:?}",
    );

    // The mark opens the block and the rest is indented under the first
    // character, so the three rows read as one thing said once.
    let marker = DARK.glyphs.marker;
    assert!(said[0].starts_with(marker), "{committed:?}");
    assert!(
        said[1].starts_with(&" ".repeat(marker.chars().count())),
        "a continuation row belongs under the first character, not under the \
         mark: {committed:?}",
    );
    assert!(!said[1].contains(marker.trim()), "{committed:?}");
}

/// The row a one-line prompt has always committed, unchanged.
#[test]
fn f5_a_one_line_goal_is_still_one_row() {
    let mut events = Events::new(DARK);
    let committed = rows(events.event(
        &event(EventKind::Started {
            goal: "count the tests".into(),
            provider: "openrouter".into(),
        }),
        Duration::ZERO,
    ));

    let said: Vec<&String> = committed
        .iter()
        .filter(|row| !row.trim().is_empty())
        .collect();
    assert_eq!(said.len(), 1, "{committed:?}");
    assert!(said[0].contains("count the tests"), "{committed:?}");
}

/// 0.14.0 F7 — a dial is drawn, permitted or refused.
///
/// Sabotage: restore `Disposition::Silent` on the `dialed` row in
/// `src/triage.rs`, under which only F7 fails — on both lines being gone from
/// the scrollback, with `Status::unknown` unmoved, because a triaged-silent kind
/// was never counted there and cannot start being.
#[test]
fn f7_a_dial_carries_the_host_as_asked_the_port_and_the_verdict() {
    let mut events = Events::new(DARK);

    let permitted = rendered(
        &mut events,
        EventKind::Dialed {
            host: "api.github.com".into(),
            port: 443,
            allowed: true,
        },
    );
    // **The name the command asked for, beside the port, exactly as the event
    // carried them.** Nothing in this process resolves anything, so the only way
    // an address could appear on this row is if the arm composed a target of its
    // own — and an address would not match the policy pattern that decided the
    // dial, which is written against names.
    assert!(permitted.contains("api.github.com:443"), "{permitted:?}");
    // The verdict in a word and not in a colour alone: `NO_COLOR`, a monochrome
    // terminal and a screen reader all read the words and none of them read the
    // tone.
    assert!(permitted.contains("permitted"), "{permitted:?}");
    assert!(!permitted.contains("refused"), "{permitted:?}");

    let lines = events.event(
        &event(EventKind::Dialed {
            host: "api.github.com".into(),
            port: 443,
            allowed: false,
        }),
        Duration::ZERO,
    );
    let refused = flatten(lines.clone());
    assert!(refused.contains("refused"), "{refused:?}");
    assert!(refused.contains("api.github.com:443"), "{refused:?}");
    // Its own tone and not the error one, because nothing broke: the boundary
    // worked. Asserted against the span's style rather than its text, since that
    // is the half a `contains` cannot see.
    let refusal = DARK.style(Tone::Refused);
    assert!(
        lines
            .iter()
            .flat_map(|line| line.spans.iter())
            .any(|span| span.content.contains("refused") && span.style.fg == refusal.fg),
        "a refused dial is drawn in the refusal tone: {lines:?}",
    );
}

/// 0.14.0 F8 — a sandbox says what happened and what isolated it.
///
/// Sabotage: draw `cap_hit` through the error path — `Tone::Error` in place of
/// the warning — under which only F8 fails, on a run whose cap held exactly as
/// its operator configured it being reported to them as a run that broke.
#[test]
fn f8_a_sandbox_draws_its_four_kinds_and_carries_a_backend_only_where_one_exists() {
    let mut events = Events::new(DARK);

    // `create` and `exec` are the two io-harness sets a backend on, so the line
    // carries what isolated the work.
    for (kind, expected) in [("create", "created"), ("exec", "ran")] {
        let line = rendered(
            &mut events,
            EventKind::Sandbox {
                kind: kind.into(),
                backend: Some("macos-sandbox-exec".into()),
            },
        );
        assert!(line.contains(expected), "{kind}: {line:?}");
        assert!(line.contains("macos-sandbox-exec"), "{kind}: {line:?}");
    }

    // `cap_hit` and `destroy` carry `None` always, so there is no backend to
    // draw — and none is worked out here and printed as though the event had
    // said it.
    for kind in ["cap_hit", "destroy"] {
        let line = rendered(
            &mut events,
            EventKind::Sandbox {
                kind: kind.into(),
                backend: None,
            },
        );
        assert!(!line.trim().is_empty(), "{kind} drew nothing: {line:?}");
        assert!(
            !line.contains("sandbox-exec") && !line.contains("none"),
            "{kind} has no backend and this line invented one: {line:?}",
        );
    }

    // **A limit reached, and not a failure.** The sandbox did what it was
    // configured to do, and the error path would say the opposite of that to the
    // one person who chose the number.
    let cap = rendered(
        &mut events,
        EventKind::Sandbox {
            kind: "cap_hit".into(),
            backend: None,
        },
    );
    assert!(cap.contains("limit"), "{cap:?}");
    assert!(
        !cap.contains("error"),
        "a cap that held is not an error: {cap:?}",
    );
    assert!(
        !cap.contains("failed") && !cap.contains("broke"),
        "a cap that held did not fail: {cap:?}",
    );
}

/// 0.14.0 F8 — the `dial` kind draws nothing, because F7 already drew that dial.
///
/// io-harness builds it as a `destroy` event with the kind overwritten and emits
/// it immediately beside `EventKind::Dialed` for the same connection, so a
/// session drawing both would put every dial in the transcript twice — and the
/// copy here is the poorer one, carrying the word and neither the host, the port
/// nor the verdict. It is now the **only** kind of the seven that draws nothing:
/// `gate_phase_failed` and `gate_output` stood beside it here until 0.24.0 gave a
/// session a criterion to fail, and the test below is where they went.
///
/// Sabotage: give `"dial"` a sentence of its own in the `Sandbox` arm, under
/// which only this fails, on one dial arriving as two rows that disagree about
/// how much they know.
#[test]
fn f8_the_dial_kind_of_a_sandbox_event_is_left_to_the_dial_itself() {
    let mut events = Events::new(DARK);
    let lines = events.event(
        &event(EventKind::Sandbox {
            kind: "dial".into(),
            backend: None,
        }),
        Duration::ZERO,
    );
    assert!(lines.is_empty(), "dial drew a line: {lines:?}");
    // Drawn nowhere is not the same as undecided. `sandbox` has a disposition, so
    // this does not reach the counter that exists for a kind nobody has decided
    // about.
    assert_eq!(
        events.unknown(),
        0,
        "a kind the table holds was counted as one this release has never seen",
    );
}

/// **0.24.0 — a gate that ran and said no reaches the screen.**
///
/// Until this release both kinds returned an empty `Vec` from the `Sandbox` arm,
/// which was correct while no contract this crate built carried a criterion: an
/// event that cannot arrive needs no sentence. It can arrive now, and a verdict
/// that lives only in the store is a verdict nobody reads.
///
/// **Neither line can carry the phase or the output**, and that is asserted here
/// rather than merely commented: `EventKind::Sandbox` has `kind` and `backend`
/// and nothing else, while `SandboxEvent::detail` — which holds the failing phase
/// for one and the command's bounded output for the other — never reaches this
/// channel. A line that named a phase would have invented it.
///
/// Sabotage: put either kind back on the `_ => return Vec::new()` arm. Only this
/// fails, and it fails on the line's absence with `Events::unknown` still at
/// zero — a triaged kind is never counted there — which is a gate that judged the
/// work and told nobody.
#[test]
fn a_gate_that_ran_and_did_not_pass_says_so_on_the_channel_a_session_is_watching() {
    let mut events = Events::new(DARK);

    let phase = rendered(
        &mut events,
        EventKind::Sandbox {
            kind: "gate_phase_failed".into(),
            backend: None,
        },
    );
    // **That it ran is the load-bearing half.** "did not pass" alone is the same
    // sentence a criterion that never executed would deserve, and those two need
    // opposite responses from whoever reads the row.
    assert!(phase.contains("ran and"), "{phase:?}");
    assert!(phase.contains("did not pass"), "{phase:?}");

    let output = rendered(
        &mut events,
        EventKind::Sandbox {
            kind: "gate_output".into(),
            backend: None,
        },
    );
    assert!(!output.trim().is_empty(), "gate_output drew nothing");
    assert!(output.contains("printed output"), "{output:?}");

    // Neither event carries a backend and neither line may name one — the same
    // rule `cap_hit` and `destroy` are already held to.
    for line in [&phase, &output] {
        assert!(
            !line.contains("sandbox-exec") && !line.contains("none"),
            "a gate event carries no backend and this line invented one: {line:?}",
        );
    }

    assert_eq!(
        events.unknown(),
        0,
        "a kind the table holds was counted as one this release has never seen",
    );
}

/// **0.24.0 — a criterion that answered and one that never ran do not read
/// alike.**
///
/// `GateOutcome::Failed` and `GateOutcome::Errored` are the two verdicts io-cli
/// has to keep apart on screen, and only one of them has an event: a gate that
/// ran and said no emits `gate_phase_failed`, while a gate whose program the
/// policy would not run emits `EventKind::Refused` with act `exec` and no gate
/// event at all — nothing executed, so nothing judged anything. io-harness will
/// retry the second and never the first.
///
/// So the words have to be disjoint, which is what is asserted: the refusal keeps
/// `refused` and never claims a criterion ran, and the gate line never borrows the
/// word the permission boundary owns. A reader who cannot tell them apart fixes
/// the wrong thing — the policy, or the work.
///
/// Sabotage: draw `gate_phase_failed` through `Tone::Refused`, which is the tone
/// the failing review carried until this release. The line reads `refused: …`,
/// only this test fails, and it fails on the two facts having become one word.
#[test]
fn a_gate_that_could_not_run_does_not_read_like_a_gate_that_ran_and_failed() {
    let mut events = Events::new(DARK);

    // Errored: the policy refused the criterion's program. This is the whole of
    // what the stream says about it — there is no gate event behind it.
    let errored = rendered(
        &mut events,
        EventKind::Refused {
            act: "exec".into(),
            target: "cargo".into(),
            rule: Some("exec.deny".into()),
            layer: Some("workspace".into()),
        },
    );
    assert!(errored.contains("refused"), "{errored:?}");
    assert!(
        !errored.contains("did not pass"),
        "a criterion that never ran did not fail: {errored:?}",
    );

    // Failed: it ran, and it said no.
    let failed = rendered(
        &mut events,
        EventKind::Sandbox {
            kind: "gate_phase_failed".into(),
            backend: None,
        },
    );
    assert!(
        !failed.contains("refused"),
        "the permission boundary owns that word and nothing stopped this gate: {failed:?}",
    );
    // Nor the error path. Nothing broke: a criterion that holds the work to a
    // standard and reports that it was not met did its job.
    assert!(!failed.contains("error"), "{failed:?}");
    assert_ne!(errored, failed);
}

/// **0.24.0 — a review's reasons reach the scrollback, in the reviewer's words.**
///
/// The reasons are the operator's only account of why the work was judged
/// insufficient. The store keeps the same sentences joined by semicolons, so a
/// session that dropped them would be sending somebody to a database to read a
/// paragraph the run had already said out loud.
///
/// **The failing verdict is a warning and not a refusal**, which is the half that
/// changed this release. `Tone::Refused` carries the literal word `refused`, and
/// everywhere else in this transcript that word means the permission boundary
/// stopped an act. A reviewer that read the work and judged it stopped nothing.
///
/// Sabotage: restore `Tone::Refused` on the failing arm. Only this fails, on a
/// judged verdict and a blocked act arriving under one word.
#[test]
fn a_review_carries_its_reasons_and_a_failing_verdict_is_not_a_refusal() {
    let mut events = Events::new(DARK);

    let failed = rows(events.event(
        &event(EventKind::Reviewed {
            passed: false,
            reasons: vec![
                "the new branch has no test".into(),
                "README still documents the old flag".into(),
            ],
        }),
        Duration::ZERO,
    ));
    let said = failed.join("\n");
    assert!(said.contains("the review ran and did not pass"), "{said:?}");
    // Every reason, whole. Asserted per row rather than over the join, so a
    // second reason silently swallowed by the first cannot pass.
    assert!(
        failed
            .iter()
            .any(|row| row.contains("the new branch has no test")),
        "{failed:?}",
    );
    assert!(
        failed
            .iter()
            .any(|row| row.contains("README still documents the old flag")),
        "{failed:?}",
    );
    assert!(
        !said.contains("refused"),
        "a review that read the work and judged it stopped nothing: {said:?}",
    );

    let passed = flatten(events.event(
        &event(EventKind::Reviewed {
            passed: true,
            reasons: Vec::new(),
        }),
        Duration::ZERO,
    ));
    assert!(passed.contains("the review passed"), "{passed:?}");
    assert!(
        !passed.contains("did not"),
        "the two verdicts must not be one sentence apart from a negation this reader \
         can miss: {passed:?}",
    );
}

/// **F5 for the lines this release added.** Nothing outside ASCII reaches the
/// terminal under the ASCII set, and the meaning survives the substitution.
///
/// Swept by `char::is_ascii` over the whole rendered output rather than by
/// looking for the marks this release happens to know about — `tests/glyphs.rs`
/// says why: a `contains` over the code points somebody remembered is green the
/// day a twelfth one is typed into a new line. The review's reasons are the rows
/// that carry a glyph here, through `Glyphs::bullet`.
///
/// Plain mode is the other axis and is asserted beside it, because a plain
/// session is the one that has no status line to fall back on: if a gate verdict
/// were dropped there it would reach nobody at all.
#[test]
fn the_gate_and_review_lines_survive_the_ascii_set_and_plain_mode() {
    use io_cli::glyphs::ASCII;

    for plain in [false, true] {
        let mut events = Events::new(DARK.with_glyphs(ASCII));
        events.set_plain(plain);

        let mut said = String::new();
        for kind in [
            EventKind::Sandbox {
                kind: "gate_phase_failed".into(),
                backend: None,
            },
            EventKind::Sandbox {
                kind: "gate_output".into(),
                backend: None,
            },
            EventKind::Reviewed {
                passed: false,
                reasons: vec!["the diff does not build".into()],
            },
        ] {
            said.push_str(&rendered(&mut events, kind));
            said.push('\n');
        }

        assert!(
            said.is_ascii(),
            "a character the ASCII set cannot draw reached the terminal in \
             plain={plain}: {said:?}",
        );
        // And the meaning survived it. A set that mapped every mark to a space
        // would pass the sweep above and destroy the product.
        assert!(
            said.contains("ran and did not pass"),
            "plain={plain}: {said:?}"
        );
        assert!(said.contains("printed output"), "plain={plain}: {said:?}");
        assert!(
            said.contains("the diff does not build"),
            "plain={plain}: {said:?}",
        );
    }
}

/// 0.14.0 F9 — a stalled agent is on screen before anybody interrupts it.
///
/// `EventKind::Stalled` is a unit variant carrying nothing at all, so the line is
/// composed from the run state around it: `RunEvent::step` for the step it
/// stopped on, and the session age the driver handed in against the age the last
/// step opened at for how long it has been there. **Both are stated by this
/// test and neither is measured** — N1 forbids the renderer a clock, and a line
/// composed only of state handed to it is what that rule buys.
///
/// Sabotage: restore `Disposition::Silent` on the `stalled` row in
/// `src/triage.rs`, under which only F9 fails, on the line's absence rather than
/// on `Status::unknown`, and a stall goes back to being visible as a session
/// that has simply stopped saying anything.
#[test]
fn f9_a_stall_names_the_step_it_stopped_on_and_how_long_it_has_been_there() {
    let mut events = Events::new(DARK);
    started_at(&mut events, Duration::ZERO);
    // The step that goes on to stall opens four seconds into the session …
    events.event(
        &RunEvent::new(
            1,
            6,
            EventKind::Step {
                decision: "read the same file again".into(),
                tool_call: "read_file".into(),
                tokens: 40,
                changed: false,
            },
        ),
        Duration::from_secs(4),
    );
    // … and the stall arrives on step seven, two and a half seconds after it.
    let stalled = flatten(events.event(
        &RunEvent::new(1, 7, EventKind::Stalled),
        Duration::from_millis(6_500),
    ));

    assert!(stalled.contains("step 7"), "{stalled:?}");
    assert!(stalled.contains("2.5s"), "{stalled:?}");
    // And what it means, which is the half a reader of a quiet session would
    // otherwise assume the opposite of: the run is over rather than working.
    assert!(stalled.contains("circles"), "{stalled:?}");
    assert!(stalled.contains("stops here"), "{stalled:?}");
}

// ---------------------------------------------------------------------------
// O3 — a question is committed exactly where nothing else will draw it
// ---------------------------------------------------------------------------

/// A question the agent asked, as io-harness emits it.
fn a_question() -> EventKind {
    EventKind::QuestionAsked {
        question: "drop the column or keep it?".to_string(),
        choices: vec!["drop".to_string(), "keep".to_string()],
    }
}

/// **O3 — with an overlay holding the question, the transcript stays quiet.**
///
/// Through 0.31.0 this line was committed unconditionally and the overlay redrew
/// the same question through `Tone::Warning`, so the operator was asked twice and
/// told the second time was a warning. Two renderers, neither aware of the other.
///
/// **The sabotage pass is why this test exists.** The condition was written, the
/// suite was green, and replacing it with `if true` — the exact defect it guards —
/// failed nothing at all. The fix had no gate until the arm was run.
#[test]
fn o3_a_question_is_not_committed_when_an_overlay_will_draw_it() {
    let mut events = Events::new(DARK);
    events.set_answering(true);
    let line = rendered(&mut events, a_question());
    assert!(
        line.is_empty(),
        "the overlay is holding this question, so committing it asks the operator \
         twice: {line:?}",
    );
}

/// **O3's other direction, and it is the one that matters more.** Suppressing a
/// question everywhere is a worse defect than printing it twice, so every path
/// that has no overlay must still commit the line.
#[test]
fn o3_a_question_is_committed_wherever_nothing_will_draw_it() {
    // A resumed run: this process attached a responder but dropped the receiver,
    // so no overlay exists here. `answering` is false and the line is the only
    // thing that renders the question at all.
    let mut resumed = Events::new(DARK);
    resumed.set_answering(false);
    let line = rendered(&mut resumed, a_question());
    assert!(
        line.contains("drop the column or keep it?"),
        "a resumed run's question reached no surface at all: {line:?}",
    );
    assert!(
        line.contains("drop") && line.contains("keep"),
        "the offers go with it, or the operator is asked a question whose choices \
         they cannot see: {line:?}",
    );

    // Plain mode draws no overlay, whatever this process is holding.
    let mut plain = Events::new(DARK);
    plain.set_answering(true);
    plain.set_plain(true);
    let line = rendered(&mut plain, a_question());
    assert!(
        line.contains("drop the column or keep it?"),
        "plain mode draws no overlay, so the transcript is the only renderer: {line:?}",
    );
}

/// **O3 — and it is not drawn as a warning.** `Tone::Warning`'s word is literally
/// `warning`, so under `MONO` every question the agent ever asked announced itself
/// as one.
#[test]
fn o3_the_committed_question_is_not_a_warning() {
    let mut events = Events::new(io_cli::theme::MONO);
    events.set_answering(false);
    let line = rendered(&mut events, a_question());
    assert!(line.contains("the agent asks:"), "{line:?}");
    assert!(
        !line.contains("warning"),
        "a question is not a warning: {line:?}",
    );
}

// ---------------------------------------------------------------------------
// 0.33.0 — a batch is drawn as a batch
// ---------------------------------------------------------------------------

/// Three questions the agent asked in one call, as io-harness 0.72.0 emits them.
///
/// The three texts share no word with each other and none of them is a substring
/// of the heading, so a `contains` below fails when the arm drops a question
/// rather than being answered by something already on the frame for another
/// reason.
fn a_batch() -> EventKind {
    EventKind::QuestionsAsked {
        questions: vec![
            Question::new("drop the column or keep it?").with_choices(["drop", "keep"]),
            Question::new("which index goes with it?"),
            Question::new("rename the table too?"),
        ],
    }
}

/// **A batched ask names every question it carried, numbered, in the order asked.**
///
/// io-harness emits no `QuestionAsked` for a batch, so before this release the
/// whole ask put *nothing at all* in a transcript — in the release whose subject
/// is batched asks.
///
/// Asserted per row and by pairing rather than over the flattened string: the
/// ordinal and the question it belongs to have to be on the same row, which is a
/// claim a `contains` over one glued string cannot make and which fails when the
/// arm numbers the batch off its own loop counter.
#[test]
fn a_batched_ask_names_every_question_and_says_they_arrived_together() {
    let mut events = Events::new(DARK);
    // No overlay is holding these, so the transcript draws the questions too.
    events.set_answering(false);
    let drawn = rows(events.event(&event(a_batch()), Duration::ZERO));

    let heading = drawn
        .first()
        .unwrap_or_else(|| panic!("a batched ask committed nothing at all: {drawn:?}"));
    assert!(
        heading.contains("3 questions together"),
        "the heading does not say the ask was a batch, which is the only thing \
         distinguishing this event from three singular ones: {heading:?}",
    );

    for (index, asked) in [
        "drop the column or keep it?",
        "which index goes with it?",
        "rename the table too?",
    ]
    .into_iter()
    .enumerate()
    {
        let ordinal = format!("{} of 3", index + 1);
        let row = drawn
            .iter()
            .find(|row| row.contains(&ordinal))
            .unwrap_or_else(|| panic!("no row is numbered {ordinal:?}: {drawn:?}"));
        assert!(
            row.contains(asked),
            "the row numbered {ordinal:?} does not carry the question asked in that \
             position: {row:?}",
        );
    }

    // The offers belong to the question above them and are marked as offers. The
    // needle carries the bullet, because both labels are also words inside the
    // first question's own text — `contains("drop")` would be green with the
    // choice rows deleted.
    let first = drawn
        .iter()
        .position(|row| row.contains("1 of 3"))
        .expect("the first question is numbered");
    assert_eq!(
        drawn[first + 1].trim(),
        format!("{} drop", DARK.glyphs.bullet),
        "the first offer is not the row under the question it belongs to: {drawn:?}",
    );
    assert_eq!(
        drawn[first + 2].trim(),
        format!("{} keep", DARK.glyphs.bullet),
        "the second offer is not the row under the question it belongs to: {drawn:?}",
    );
    // And the question with no offers has none invented for it.
    let last = drawn
        .iter()
        .position(|row| row.contains("3 of 3"))
        .expect("the third question is numbered");
    assert_eq!(
        last,
        drawn.len() - 1,
        "a question that offered nothing grew a choice row: {drawn:?}",
    );
}

/// **The count is committed even when the overlay is holding the questions, and
/// the questions are not.**
///
/// The two halves are one claim. `intent::Answerer` implements `Responder::answer`
/// alone, so io-harness's `answer_all` hands the overlay one question at a time
/// and nothing there says the three arrived together — that fact reaches nobody
/// unless this line commits unconditionally. The questions themselves keep
/// `question_asked`'s 0.32.0 rule, or a batch asks the operator each of its
/// questions twice.
///
/// Sabotage, either direction: put the heading inside the `plain || !answering`
/// guard, or move the question rows outside it. One assertion here fails for each.
#[test]
fn a_batched_ask_commits_its_count_even_when_an_overlay_draws_the_questions() {
    let mut events = Events::new(DARK);
    events.set_answering(true);
    let drawn = rows(events.event(&event(a_batch()), Duration::ZERO));

    assert_eq!(
        drawn.len(),
        1,
        "the overlay asks these one at a time, so the transcript commits the count \
         and nothing else: {drawn:?}",
    );
    assert!(
        drawn[0].contains("3 questions together"),
        "the batch reached no surface at all: {drawn:?}",
    );
    assert!(
        !drawn[0].contains("drop the column"),
        "the question is on the overlay and in the transcript, which is the defect \
         0.32.0 removed from the singular arm: {drawn:?}",
    );
}

/// A batch may carry one question — io-harness rejects an empty list and accepts a
/// single-element one — and the sentence is written for it.
///
/// Sabotage: delete the `asked == 1` branch. The heading reads `1 questions
/// together`, which the second assertion names.
#[test]
fn a_batch_of_one_is_not_announced_as_1_questions() {
    let mut events = Events::new(DARK);
    events.set_answering(true);
    let line = rendered(
        &mut events,
        EventKind::QuestionsAsked {
            questions: vec![Question::new("which index goes with it?")],
        },
    );
    assert!(line.contains("one question"), "{line:?}");
    assert!(!line.contains("1 questions"), "{line:?}");
}

/// **The singular arm is untouched, and this is what says so.**
///
/// The cheap way to have handled 0.72.0 would have been to fold `QuestionAsked`
/// into the batch renderer as a batch of one. It compiles, it is shorter, and it
/// silently rewords every question this product has drawn since 0.7.0 — `the agent
/// asks: <question>` becomes a heading and a `1 of 1` row.
#[test]
fn a_single_question_is_still_drawn_as_a_question_rather_than_a_batch_of_one() {
    let mut events = Events::new(DARK);
    events.set_answering(false);
    let single = rendered(&mut events, a_question());

    assert!(
        single.contains("the agent asks: drop the column or keep it?"),
        "the singular question lost its own sentence: {single:?}",
    );
    for batched in ["together", " of 1", "one question"] {
        assert!(
            !single.contains(batched),
            "a single question was drawn through the batch arm ({batched:?}): {single:?}",
        );
    }
}

/// **A path that contains io-harness's separator is drawn as itself.**
///
/// The display translation belongs to `read_skill`, whose target is a *name*. The
/// first draft applied it to every tool call, on the reasoning that no path
/// contains `__` — which is false and commonly so. `read src/__init__.py` was
/// drawn `read src/:init__.py`, a path that does not exist, in the one place an
/// operator checks what the agent touched.
#[test]
fn a_tool_targets_path_is_never_translated_however_it_is_spelled() {
    for path in [
        "src/__init__.py",
        "src/__pycache__/thing.pyc",
        "app/__tests__/login.test.ts",
        "src/__mocks__/fs.js",
        "snapshots/__snapshots__/App.snap",
        "src/snake__case.rs",
    ] {
        let mut events = Events::new(DARK);
        let opened = rendered(
            &mut events,
            EventKind::ToolCall {
                name: "read_file".to_string(),
                target: path.to_string(),
            },
        );
        // The call is held open and committed by the `Step` that follows it, so
        // the name is read off the live row rather than the scrollback.
        assert!(opened.is_empty(), "a tool call commits nothing on its own");
        let live = events.live().to_string();
        assert!(
            !live.contains(':'),
            "the separator was translated inside a path, so the operator is shown \
             a file that does not exist: {live:?} for {path}",
        );
        assert!(
            live.contains("__"),
            "the path lost its own characters: {live:?} for {path}",
        );
    }
}

/// And the one tool whose target really is a name still reads as a name.
#[test]
fn a_skills_target_is_still_drawn_the_way_the_operator_reads_it() {
    let mut events = Events::new(DARK);
    events.event(
        &event(EventKind::ToolCall {
            name: io_cli::events::READ_SKILL.to_string(),
            target: "ultraship__brainstorm".to_string(),
        }),
        Duration::ZERO,
    );
    let live = events.live().to_string();
    assert!(
        live.contains("ultraship:brainstorm"),
        "a skill is addressed by a name, and the name is the one drawn everywhere \
         else: {live:?}",
    );
}

/// A `read_skill` call announced, then closed by the step that ran it.
///
/// The pair is the unit under test throughout this section: the announcement
/// decides what the live row says and the step's own sentence decides what the
/// committed cell says, and the defects this release fixes are all in the
/// relationship between the two.
fn skill_cell(target: &str, decision: &str) -> String {
    let mut events = Events::new(DARK);
    events.event(
        &event(EventKind::ToolCall {
            name: io_cli::events::READ_SKILL.to_string(),
            target: target.to_string(),
        }),
        Duration::ZERO,
    );
    rendered(
        &mut events,
        EventKind::Step {
            decision: decision.to_string(),
            tool_call: String::new(),
            tokens: 12,
            changed: false,
        },
    )
}

/// **F1 — a loaded skill says which skill loaded, and says nothing in the
/// machine's spelling.**
///
/// This is the line that opened the release. `Read skill ultraship:using-ultraship
/// · ultraship__using-ultraship` was drawn: the target translated, io-harness's
/// sentence beside it untouched, and the one string the translation existed to
/// hide printed in full next to the translation of it.
#[test]
fn f1_a_loaded_skill_is_drawn_as_a_loaded_skill_row() {
    let cell = skill_cell(
        "ultraship__using-ultraship",
        "read skill ultraship__using-ultraship",
    );
    assert!(
        cell.contains("ultraship:using-ultraship"),
        "the row must name the skill the operator reads: {cell:?}",
    );
    assert!(
        cell.contains("loaded"),
        "the row must say what became of it: {cell:?}",
    );
    assert!(
        !cell.contains("__"),
        "io-harness's separator reached the transcript: {cell:?}",
    );
    assert!(
        !cell.contains("read skill"),
        "the harness's own sentence is not drawn for this tool, which is what \
         removes the separator at its source: {cell:?}",
    );
}

/// **F2 — and a skill no bundle contributed gets the same row.**
///
/// An unqualified skill carries no separator, so nothing here is about
/// translation. The row must not be a bundle-only surface: a skill an operator
/// wrote themselves loads exactly as visibly.
#[test]
fn f2_an_unqualified_skill_gets_the_same_row_with_its_bare_name() {
    let cell = skill_cell("brainstorm", "read skill brainstorm");
    assert!(cell.contains("brainstorm"), "{cell:?}");
    assert!(cell.contains("loaded"), "{cell:?}");
    assert!(!cell.contains(':'), "there is no bundle to name: {cell:?}");
}

/// **The row is an assertion, so it is not made when the read did not happen.**
///
/// Drawing `loaded` from io-cli's own words rather than from the harness's
/// sentence is what removes the separator, and it is also what makes this row
/// able to be wrong in a way the old one could not. A failed read still says the
/// read failed.
#[test]
fn a_failed_skill_read_says_so_and_never_says_loaded() {
    let cell = skill_cell("ultraship__plan", "skill ultraship__plan read error");
    assert!(
        cell.contains("read error"),
        "a failed read must say it failed: {cell:?}",
    );
    assert!(
        !cell.contains("loaded"),
        "the row claimed a skill loaded when the read failed: {cell:?}",
    );
    assert!(!cell.contains("__"), "{cell:?}");
}

/// **F11 — a companion path is drawn as itself.**
///
/// io-harness 0.73.0 gave `read_skill` an optional `path` and announces it in
/// preference to the skill's name, so this tool's target is now sometimes a file.
/// The translation rewrites the first separator to a colon, so a companion file
/// called `__init__.py` would be drawn as `:init__.py` — a path that does not
/// exist, in the one place an operator checks what the agent touched. That is
/// verbatim the failure 0.32.0 found; the pin moved the ground under the gate.
#[test]
fn f11_a_companion_path_is_drawn_intact_and_the_skill_is_still_named() {
    for path in ["references/__init__.py", "shared/principles.md"] {
        let mut events = Events::new(DARK);
        events.event(
            &event(EventKind::ToolCall {
                name: io_cli::events::READ_SKILL.to_string(),
                target: path.to_string(),
            }),
            Duration::ZERO,
        );
        let live = events.live().to_string();
        assert!(
            live.contains(path),
            "the companion path lost its own characters on the live row: \
             {live:?} for {path}",
        );
        let cell = skill_cell(path, &format!("read skill ultraship__plan {path}"));
        assert!(
            cell.contains(path),
            "the committed cell must show the file that was read: {cell:?}",
        );
        assert!(
            cell.contains("ultraship:plan"),
            "and must still say which skill it belonged to: {cell:?}",
        );
        // The mangled spelling, not "no separator anywhere" — a companion file
        // called `__init__.py` carries one legitimately, and a gate that forbade
        // it would force the very translation this asserts against.
        assert!(
            !cell.contains(":init__"),
            "the path was translated and now names a file that does not exist: \
             {cell:?}",
        );
    }
}

/// And an empty `path`, which asks for the bundle itself, still names the skill.
///
/// The announced target is empty for this call, so a cell drawn from the target
/// alone would be blank. It is drawn from the harness's sentence, which is not.
#[test]
fn f11_an_empty_path_names_the_skill_rather_than_drawing_a_blank() {
    let cell = skill_cell("", "read skill ultraship__plan .");
    assert!(cell.contains("ultraship:plan"), "{cell:?}");
    assert!(
        cell.contains("listed"),
        "an empty path lists the bundle, and `.` is a spelling for a trace \
         rather than for a person: {cell:?}",
    );
}

/// **F6 — an MCP tool says its server and its tool.**
///
/// `verb` fell an unknown name through unchanged, so `mcp__github__create_issue`
/// reached the transcript whole. The prefix is stripped before translating: it
/// ends with the separator itself, so translating the whole name splits at the
/// prefix's own join and yields `mcp:github__create_issue`, which is both wrong
/// and still carrying the thing being removed.
#[test]
fn f6_an_mcp_tool_cell_names_the_server_and_the_tool() {
    let mut events = Events::new(DARK);
    events.event(
        &event(EventKind::ToolCall {
            name: "mcp__github__create_issue".to_string(),
            target: "mcp__github__create_issue".to_string(),
        }),
        Duration::ZERO,
    );
    let live = events.live().to_string();
    assert!(
        live.contains("Call") && live.contains("github:create_issue"),
        "the cell must keep a verb and name both halves: {live:?}",
    );
    assert!(!live.contains("__"), "{live:?}");
    assert!(
        !live.contains("mcp:"),
        "the prefix was translated instead of stripped: {live:?}",
    );
}

/// A server id carrying the separator splits on the prefix and the first join
/// only, and the tool's own name is left exactly as it was found.
#[test]
fn f6_a_server_id_containing_the_separator_still_splits_at_the_first_join() {
    let mut events = Events::new(DARK);
    events.event(
        &event(EventKind::ToolCall {
            name: "mcp__deep__nested__tool".to_string(),
            target: "mcp__deep__nested__tool".to_string(),
        }),
        Duration::ZERO,
    );
    let live = events.live().to_string();
    assert!(
        live.contains("deep:nested__tool"),
        "only the first join after the prefix is the server boundary: {live:?}",
    );
}

/// **The measured duration still reaches an MCP cell.**
///
/// `EventKind::Mcp` is the only event in the whole enum carrying a per-tool
/// duration, and it is matched to its open cell by rebuilding io-harness's own
/// namespaced name. Translating the field that match compares against loses the
/// measurement silently — nothing fails, the cell simply falls back to io-cli's
/// own observation and wears a `~` that says it is one.
#[test]
fn an_mcp_calls_measured_duration_survives_the_translation() {
    let mut events = Events::new(DARK);
    events.event(
        &event(EventKind::ToolCall {
            name: "mcp__github__create_issue".to_string(),
            target: "mcp__github__create_issue".to_string(),
        }),
        Duration::ZERO,
    );
    events.event(
        &event(EventKind::Mcp {
            server: "github".to_string(),
            tool: Some("create_issue".to_string()),
            ok: Some(true),
            millis: Some(1234),
            tools: None,
        }),
        Duration::ZERO,
    );
    let cell = rendered(
        &mut events,
        EventKind::Step {
            decision: "called create_issue".to_string(),
            tool_call: String::new(),
            tokens: 12,
            changed: false,
        },
    );
    assert!(
        cell.contains("1.2s"),
        "io-harness's own measurement must reach the cell: {cell:?}",
    );
    assert!(
        !cell.contains("~1.2s"),
        "a measurement was drawn as an observation: {cell:?}",
    );
}

/// **F5 — the operator's own command comes back, and the model's does not.**
///
/// The echo row is drawn from io-harness's own `Started` event, whose `goal` is
/// the text that was *sent* — and for a slash-invoked skill that is deliberately
/// the catalogue name, because it is the only string `read_skill` resolves. Up to
/// 0.33.0 the machine's spelling was what the operator read back.
#[test]
fn f5_a_slash_invoked_skill_echoes_what_was_typed_not_what_was_sent() {
    let mut events = Events::new(DARK);
    events.set_echo("/ultraship:brainstorm make me a portfolio");
    let echoed = rendered(
        &mut events,
        EventKind::Started {
            goal: "ultraship__brainstorm\n\nmake me a portfolio".to_string(),
            provider: "openrouter".to_string(),
        },
    );
    assert!(
        echoed.contains("/ultraship:brainstorm"),
        "the operator's own command must come back: {echoed:?}",
    );
    assert!(
        echoed.contains("make me a portfolio"),
        "and so must the argument: {echoed:?}",
    );
    assert!(
        !echoed.contains("__"),
        "the machine's spelling reached the row the reader wrote: {echoed:?}",
    );
}

/// And it is taken, not held — the next turn shows the next prompt.
///
/// A carried string that outlives its turn is the second-use defect this
/// product's adversarial review has found in fifteen consecutive releases: right
/// on a fresh process, wrong from the second turn onward.
#[test]
fn f5_an_echo_does_not_outlive_the_turn_it_belongs_to() {
    let mut events = Events::new(DARK);
    events.set_echo("/ultraship:brainstorm");
    let _ = rendered(
        &mut events,
        EventKind::Started {
            goal: "ultraship__brainstorm".to_string(),
            provider: "openrouter".to_string(),
        },
    );
    let second = rendered(
        &mut events,
        EventKind::Started {
            goal: "now write the tests".to_string(),
            provider: "openrouter".to_string(),
        },
    );
    assert!(
        second.contains("now write the tests"),
        "the second turn drew the first turn's typing: {second:?}",
    );
    assert!(
        !second.contains("brainstorm"),
        "the echo outlived its turn: {second:?}",
    );
}

/// **And a turn that ends without ever starting does not leave it behind.**
///
/// Found by the adversarial review. `Started`'s `take()` is not on its own a
/// guarantee: io-harness validates the contract, discovers skills, opens the run,
/// takes the store lease and sets the provider all *before* it emits `Started`,
/// and every one of those can fail — as can an interrupt in that window. The turn
/// then ends through `flush`, and an echo left there is drawn over the next
/// prompt: the one row in the transcript the reader wrote, showing something they
/// did not type.
#[test]
fn f5_an_echo_dies_with_a_turn_that_never_started() {
    let mut events = Events::new(DARK);
    events.set_echo("/ultraship:plan");
    // The turn ends the way an early failure ends it — no `Started` ever arrived.
    let _ = events.flush();
    let next = rendered(
        &mut events,
        EventKind::Started {
            goal: "now write the tests".to_string(),
            provider: "openrouter".to_string(),
        },
    );
    assert!(
        next.contains("now write the tests"),
        "a turn that never started left its echo behind: {next:?}",
    );
    assert!(!next.contains("plan"), "{next:?}");
}

/// **The echo carries the slash, and the driver is what puts it back.**
///
/// `Command::Slash` arrives with the slash already stripped — it has to be, since
/// the driver matches the first word against the skills catalogue — so an echo
/// built from that text alone comes back as an ordinary prompt that happens to
/// contain a colon. The first version of the test above hand-fed the slash and so
/// could never have seen it; the driver's own re-adding is gated in
/// `tests/dependencies.rs`, because nothing here links `src/main.rs`.
#[test]
fn f5_the_echoed_line_reads_as_a_command_and_not_as_a_prompt() {
    let mut events = Events::new(DARK);
    events.set_echo("/ultraship:brainstorm make me a portfolio");
    let echoed = rendered(
        &mut events,
        EventKind::Started {
            goal: "ultraship__brainstorm\n\nmake me a portfolio".to_string(),
            provider: "openrouter".to_string(),
        },
    );
    assert!(
        echoed.contains("/ultraship:brainstorm"),
        "the row must read as the command that was typed: {echoed:?}",
    );
}

/// **F7.** A withheld call renders as one refused cell naming the mask, and
/// nothing on screen says the tool ran.
///
/// **The sequence is the one io-harness actually emits, and it is the point of
/// this test.** `announce()` runs at `run/dispatch.rs:182` and `mask_gate` at
/// `:185`, so a withheld call opens a `ToolCall` *before* it is refused — the call
/// is announced, then refused, then the step commits. A test built from the
/// `Refused` event alone would pass over a transcript showing an open call that
/// never resolves, which is what an operator would actually see.
///
/// io-harness feeds its own sentence back as the step's decision
/// (`"{tool} refused: withheld from this turn"`, `run/read.rs:1230`), so the cell
/// pairs to that rather than to a word io-cli invented. What this asserts is the
/// pairing survives: one call, one decision, so `paired` holds and the cell
/// carries the harness's sentence instead of falling through to the step verdict.
///
/// Sabotage: drop `layer` from the `Refused` arm's rendered text, or make the
/// refusal close the open call so the cell never commits. The first loses the
/// only words that say *why*; the second loses the cell.
#[test]
fn f7_a_withheld_call_is_one_refused_cell_that_names_the_mask() {
    let mut events = Events::new(DARK);

    // 1. The call is announced — before the mask is consulted, upstream.
    let opened = events.event(
        &event(EventKind::ToolCall {
            name: "docx_write".into(),
            target: "report.docx".into(),
        }),
        Duration::ZERO,
    );
    assert!(
        opened.is_empty(),
        "a call still commits nothing when it is announced: {opened:?}"
    );

    // 2. The mask refuses it. io-harness's own act, rule and layer.
    let refusal = rendered(
        &mut events,
        EventKind::Refused {
            act: "tool".into(),
            target: "docx_write".into(),
            rule: Some("docx_write".into()),
            layer: Some("turn tool mask".into()),
        },
    );
    assert!(
        refusal.contains("docx_write"),
        "the refusal names the tool: {refusal:?}"
    );
    assert!(
        refusal.contains("turn tool mask"),
        "and names what refused it, which is the fact no other agent prints: {refusal:?}"
    );

    // 3. The step commits anyway — a refusal is an observation, not an ending.
    let cell = rendered(
        &mut events,
        EventKind::Step {
            decision: "docx_write refused: withheld from this turn".into(),
            tool_call: "docx_write".into(),
            tokens: 12,
            changed: false,
        },
    );
    assert!(
        cell.contains("withheld from this turn"),
        "the cell carries io-harness's own sentence rather than a bare verdict: {cell:?}"
    );

    // Nothing anywhere in the three may read as the tool having run. `no change`
    // is the step verdict the cell falls back to when the decision cannot be
    // paired, and it is exactly the wrong thing to say about a call that was
    // never started.
    let whole = format!("{refusal}\n{cell}");
    assert!(
        !whole.contains("no change"),
        "a withheld call must not fall through to the step's verdict, which \
         describes a call that ran and changed nothing: {whole:?}"
    );
}
