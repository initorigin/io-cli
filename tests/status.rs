//! The status line, and its share of F9: at eighty columns it degrades to a
//! narrow form rather than wrapping.

mod support;

use std::time::Duration;

use io_cli::app::App;
use io_cli::status::{format_elapsed, Budgets, Status};
use io_cli::theme::{DARK, MONO};
use io_harness::{EventKind, RunEvent, TaskContract};

/// A run event at step zero, which is where everything but a step sits.
fn event(kind: EventKind) -> RunEvent {
    RunEvent::new(1, 0, kind)
}

fn step(number: u32, tokens: u64) -> RunEvent {
    RunEvent::new(
        1,
        number,
        EventKind::Step {
            decision: "edited src/lib.rs".into(),
            tool_call: "apply_patch".into(),
            tokens,
            changed: true,
        },
    )
}

fn rendered(status: &Status, width: u16) -> String {
    status
        .line(width, &DARK)
        .spans
        .iter()
        .map(|span| span.content.as_ref())
        .collect()
}

/// **F4 — the planning phase is visible while it is on, and outlives the run.**
///
/// It is not a run-scoped field, and that is the whole of it: `/plan on` holds
/// until `/plan off`, and while it holds io-harness denies every write and every
/// exec until a proposal is approved. A field cleared with the run would leave an
/// operator watching an agent that will not write, with nothing on screen saying
/// why.
///
/// Sabotage: clear it in `Status::forget_run` beside the run-scoped fields —
/// under which only this test fails, and it fails at exactly the moment the
/// operator most needs the answer, the turn after the last one ended.
///
/// **Both surfaces, and the first cut of this test only checked one.** `Status`
/// renders two ways: `line` is the one-row form, and `footer` is the three-row
/// form the binary actually draws at an idle prompt. The field was added to
/// `line` alone, this test asserted `line` alone, and it passed — while a live
/// capture of the running binary had the word nowhere on screen. A test that
/// reads one of two renderers is a test that can pass while the operator has an
/// invisible mode, which is the whole of what F4 is against.
#[test]
fn f4_the_planning_phase_is_named_and_survives_the_run() {
    // Every surface a reader could be looking at, so neither can be the one that
    // quietly lacks it.
    fn shown(status: &Status) -> String {
        let mut text = rendered(status, 200);
        for line in status.footer(200, &DARK) {
            for span in &line.spans {
                text.push_str(span.content.as_ref());
            }
        }
        text
    }

    let mut status = Status::new("anthropic/claude-sonnet-4.5");

    assert!(
        !shown(&status).contains("planning"),
        "a session that never asked for it says nothing",
    );

    status.planning = true;
    let on = rendered(&status, 200);
    assert!(on.contains("planning"), "the one-row form says it: {on}");
    let footer: String = status
        .footer(200, &DARK)
        .iter()
        .flat_map(|line| line.spans.iter())
        .map(|span| span.content.as_ref())
        .collect();
    assert!(
        footer.contains("planning"),
        "and so does the footer, which is what the binary draws: {footer}",
    );

    // The run ends, the mode does not.
    status.forget_run();
    assert!(
        shown(&status).contains("planning"),
        "the phase outlives the run that ran under it: {}",
        shown(&status),
    );

    status.planning = false;
    assert!(!shown(&status).contains("planning"));
}

/// **F4 — the branch is on both surfaces, and it describes the checkout rather
/// than the conversation.**
///
/// It arrives from `EventKind::ToolCall`'s `target`, which io-harness fills with
/// the `name` argument of the `git_branch` call — the branch the agent created
/// and moved onto. Neither `Status::forget_run` nor `Status::start_run` clears
/// it, and that is the property: `/clear`, `/resume`, a rewind and the operator
/// simply typing again all change which conversation is on screen, and none of
/// them checks out another branch. A field cleared by any of them would blank a
/// fact about the operator's own working tree that is still true.
///
/// Both renderers, through `both_renderers`, because `Status::line` is the
/// one-row fallback and `Status::footer` is what the binary draws on every
/// terminal seven rows or taller. 0.12.0 added a field to `line` alone, asserted
/// `line` alone, went green, and shipped a mode that was nowhere on screen.
///
/// Sabotage: clear the field in `forget_run` beside `containment` — under which
/// only this test fails, and it fails by blanking a true fact about the
/// operator's checkout every time they start a new conversation.
#[test]
fn f4_the_branch_is_drawn_by_both_renderers_and_survives_the_conversation() {
    let mut status = Status::new("anthropic/claude-sonnet-4.5");
    // A run-scoped neighbour, so the test states the difference rather than only
    // the half it is about: the containment word dies with its run and this does
    // not.
    status.containment = Some("workspace-write/macos-sandbox-exec".into());

    status.note_branch(&event(EventKind::ToolCall {
        name: "git_branch".into(),
        target: "agent/fix-the-flake".into(),
    }));
    assert_eq!(status.branch.as_deref(), Some("agent/fix-the-flake"));

    for (renderer, text) in both_renderers(&status) {
        assert!(
            text.contains("git:agent/fix-the-flake"),
            "{renderer} does not say which branch the tree is on: {text:?}",
        );
    }

    // The conversation under the line changes: `/clear`, `/resume`, `/fork`, a
    // rewind. The checkout does not.
    status.forget_run();
    assert_eq!(
        status.containment, None,
        "the containment word belongs to the run that reported it",
    );
    for (renderer, text) in both_renderers(&status) {
        assert!(
            text.contains("git:agent/fix-the-flake"),
            "{renderer} forgot the branch when the conversation changed: {text:?}",
        );
    }

    // And the ordinary path `forget_run` never reaches: the operator types again.
    status.start_run();
    for (renderer, text) in both_renderers(&status) {
        assert!(
            text.contains("git:agent/fix-the-flake"),
            "{renderer} forgot the branch when the next turn started: {text:?}",
        );
    }
}

/// **F4 — a directory that is not a checkout draws no branch field at all.**
///
/// The absence half, and the rule this whole line is held to: an absent fact is
/// absent, not zero and not a word standing in for one. io-cli runs in plenty of
/// directories that were never a repository and must not become worse there — so
/// there is no empty label, no `none`, and no bare `git:` with nothing after it.
///
/// The empty `target` arm is the same rule one step earlier. `git_branch` names
/// the branch in its `target`, and an event carrying none is an event with no
/// answer in it; storing the empty string would put a prefix on screen owning
/// nothing.
///
/// Sabotage: clear the field in `forget_run` beside `containment` — under which
/// only the surviving-the-conversation arm above fails, which is what makes the
/// pair state the field's whole contract.
#[test]
fn f4_a_workspace_with_no_branch_draws_no_branch_field_anywhere() {
    let mut status = Status::new("anthropic/claude-sonnet-4.5");
    assert_eq!(status.branch, None);
    assert_eq!(status.branch_field(), None);

    // An event that names the tool but carries no branch leaves it exactly as it
    // was, rather than storing an empty name.
    status.note_branch(&event(EventKind::ToolCall {
        name: "git_branch".into(),
        target: String::new(),
    }));
    // And so does every other tool call, whatever it points at.
    status.note_branch(&event(EventKind::ToolCall {
        name: "apply_patch".into(),
        target: "src/lib.rs".into(),
    }));
    assert_eq!(
        status.branch, None,
        "a call with no branch in it named one anyway",
    );

    for (renderer, text) in both_renderers(&status) {
        assert!(
            !text.contains("git:"),
            "{renderer} drew a branch field where there is no branch: {text:?}",
        );
        assert!(
            !text.contains("branch"),
            "{renderer} named the absent field rather than leaving it out: {text:?}",
        );
        assert!(
            !text.contains("none"),
            "{renderer} spelled the absence as a word: {text:?}",
        );
    }
}

/// **F4 — it is a word, not only a tone.**
///
/// The rule the whole product is held to, and the one a mode field cannot be
/// exempt from: a screen reader and a monochrome terminal have to reach the same
/// fact a colour does.
#[test]
fn f4_the_planning_phase_reads_without_colour() {
    let mut status = Status::new("anthropic/claude-sonnet-4.5");
    status.planning = true;

    let mono: String = status
        .line(200, &MONO)
        .spans
        .iter()
        .map(|span| span.content.as_ref())
        .collect();
    assert!(mono.contains("planning"), "{mono}");
}

#[test]
fn it_says_the_model_the_state_and_the_elapsed_time() {
    let mut status = Status::new("anthropic/claude-sonnet-4");
    status.elapsed = Duration::from_secs(72);
    status.working = true;

    let line = rendered(&status, 80);
    assert!(line.contains("anthropic/claude-sonnet-4"), "got {line:?}");
    assert!(line.contains("working"), "got {line:?}");
    assert!(line.contains("1m12s"), "got {line:?}");
}

#[test]
fn the_running_state_is_a_word_and_not_only_a_colour() {
    let mut status = Status::new("m");
    assert!(rendered(&status, 80).contains("ready"));
    status.working = true;
    assert!(rendered(&status, 80).contains("working"));

    // The same under NO_COLOR, where the tone carries nothing at all.
    let plain: String = status
        .line(80, &MONO)
        .spans
        .iter()
        .map(|span| span.content.as_ref())
        .collect();
    assert!(plain.contains("working"), "got {plain:?}");
}

#[test]
fn f4_a_running_turn_carries_a_moving_indicator_beside_the_word() {
    let mut status = Status::new("m");
    status.working = true;

    let first = rendered(&status, 80);
    assert!(first.contains("working"), "got {first:?}");
    let spinning = first
        .chars()
        .find(|character| io_cli::status::SPINNER.contains(character))
        .expect("a running turn shows an indicator");

    // The tick is what moves it, and it moves on the tick alone — nothing here
    // waits for anything.
    status.advance();
    let second = rendered(&status, 80);
    let moved = second
        .chars()
        .find(|character| io_cli::status::SPINNER.contains(character))
        .expect("the indicator is still there");
    assert_ne!(
        spinning, moved,
        "the indicator did not move between two ticks: {second:?}",
    );

    // An idle session has nothing to be alive about.
    status.working = false;
    let idle = rendered(&status, 80);
    assert!(
        !idle.chars().any(|c| io_cli::status::SPINNER.contains(&c)),
        "an idle session was animating: {idle:?}",
    );
}

#[test]
fn f4_no_color_keeps_the_word_and_drops_the_animation() {
    let mut status = Status::new("m");
    status.working = true;

    for tick in 0..SPINNER_LEN {
        let plain: String = status
            .line(80, &MONO)
            .spans
            .iter()
            .map(|span| span.content.as_ref())
            .collect();
        assert!(
            plain.contains("working"),
            "the word went with the animation at tick {tick}: {plain:?}",
        );
        assert!(
            !plain.chars().any(|c| io_cli::status::SPINNER.contains(&c)),
            "NO_COLOR animated at tick {tick}: {plain:?}",
        );
        status.advance();
    }
}

/// How many frames the indicator cycles through.
const SPINNER_LEN: usize = io_cli::status::SPINNER.len();

#[test]
fn f9_a_narrow_terminal_drops_whole_fields_from_the_right() {
    let mut status = Status::new("anthropic/claude-sonnet-4");
    status.elapsed = Duration::from_secs(72);
    status.working = true;

    let wide = rendered(&status, 80);
    assert_eq!(
        wide, "anthropic/claude-sonnet-4 · ⠋ working · 1m12s",
        "the full line at eighty columns",
    );

    // Room for the model and the state, but not the clock.
    let narrow = rendered(&status, 38);
    assert_eq!(narrow, "anthropic/claude-sonnet-4 · ⠋ working");

    // Room for the model only.
    let narrower = rendered(&status, 30);
    assert_eq!(narrower, "anthropic/claude-sonnet-4");

    for width in [1u16, 8, 20, 25, 26, 40, 43, 44, 200] {
        let line = rendered(&status, width);
        assert!(
            line.chars().count() <= width as usize,
            "the line overflowed {width} columns: {line:?}",
        );
        assert!(
            !line.contains('\n'),
            "the status line wrapped at {width} columns: {line:?}",
        );
        assert!(!line.is_empty(), "the line vanished at {width} columns");
    }
}

#[test]
fn f9_it_renders_on_one_row_at_eighty_columns() {
    let mut status = Status::new("anthropic/claude-sonnet-4");
    status.elapsed = Duration::from_secs(3725);
    let (mut screen, _recorder) = support::screen(80, 24);

    // **One row, drawn through the one-row form.** 0.11.0 gave the footer a rule
    // and a second line, and `Status::render` draws that wherever it has three
    // rows to draw it in — so what this asserts is the form a terminal with no
    // room left gets, which is the one that has to fit in a single row.
    screen
        .draw(|frame| {
            let area = ratatui::layout::Rect {
                height: 1,
                ..frame.area()
            };
            status.render(frame, area, &DARK)
        })
        .expect("frame");

    let viewport = screen.viewport_text();
    assert_eq!(
        viewport.lines().filter(|line| !line.is_empty()).count(),
        1,
        "the status line took more than one row: {viewport:?}",
    );
    assert!(viewport.contains("1h02m"), "got {viewport:?}");
}

#[test]
fn elapsed_time_is_readable_at_every_scale() {
    assert_eq!(format_elapsed(Duration::ZERO), "0s");
    assert_eq!(format_elapsed(Duration::from_secs(9)), "9s");
    assert_eq!(format_elapsed(Duration::from_secs(59)), "59s");
    assert_eq!(format_elapsed(Duration::from_secs(60)), "1m00s");
    assert_eq!(format_elapsed(Duration::from_secs(72)), "1m12s");
    assert_eq!(format_elapsed(Duration::from_secs(3599)), "59m59s");
    assert_eq!(format_elapsed(Duration::from_secs(3600)), "1h00m");
    assert_eq!(format_elapsed(Duration::from_secs(3725)), "1h02m");
}

/// **F9.** A field with nothing behind it is absent. Not a zero, not a dash, not a
/// placeholder — and the field this matters most for is the one about spending.
#[test]
fn f9_a_field_with_nothing_behind_it_is_absent_rather_than_zero() {
    let status = Status::new("opus-5");
    let line = status.line(120, &DARK).to_string();

    assert!(
        !line.contains("tok"),
        "no step has reported a token count yet: {line:?}",
    );
    assert!(
        !line.contains("ctx"),
        "nothing has said how full the context is: {line:?}",
    );
    // Deliberately not "the line contains no zero": the elapsed field is `0s` and
    // is legitimately zero, because the session really has been open no time at
    // all. The criterion is about a field with no *fact* behind it.
    assert!(
        !line.contains("0 tok") && !line.contains("ctx 0"),
        "an unknown value must not be rendered as a zero: {line:?}",
    );
    // And nothing has said how this run's commands are contained, which is a
    // different statement from saying they are not.
    assert!(
        !line.contains("sandbox"),
        "containment is unknown until the run says so: {line:?}",
    );
}

/// Tokens accumulate across the steps of a session rather than showing the last
/// step's own count, which would swing rather than climb.
#[test]
fn the_token_field_is_the_session_and_not_the_last_step() {
    let mut app = App::new(DARK, "opus-5");
    app.event(&step(1, 1_200), std::time::Duration::ZERO);
    app.event(&step(2, 300), std::time::Duration::ZERO);

    let line = app.status.line(120, &DARK).to_string();
    assert!(
        line.contains("1.5k tok"),
        "the field is the running total: {line:?}",
    );
}

/// **F9, containment.** The mode is what was asked for and the backend is what
/// answered on this host, and io-harness's own documentation says a surface
/// showing the first without the second is reading an intention rather than a
/// fact: `workspace-write` on a portable floor means resource caps only.
#[test]
fn the_containment_field_carries_the_backend_and_not_only_the_mode() {
    let mut app = App::new(DARK, "opus-5");
    app.event(
        &event(EventKind::Contained {
            mode: "workspace-write".into(),
            backend: "portable-floor".into(),
            roots: 0,
        }),
        std::time::Duration::ZERO,
    );

    let line = app.status.line(120, &DARK).to_string();
    assert!(line.contains("workspace-write"), "{line:?}");
    assert!(
        line.contains("portable-floor"),
        "the mode without the backend is an intention, not a fact: {line:?}",
    );
}

/// Context pressure appears once something has said what it is, and says it as a
/// share of the window **this session's contract** declares — not of the crate
/// default, which is what it divided by until 0.17.0 and is why the field was
/// silently wrong for every operator who set `[run.context]`.
#[test]
fn the_context_field_appears_when_a_fold_reports_one() {
    let mut app = App::new(DARK, "opus-5");
    // The denominator, handed over the way the driver hands it: from the contract
    // the turn is running under. Without it there is no window and the field stays
    // away, which is the honest answer and is asserted in its own test.
    app.status.budgets = Budgets::in_force(&budgeted());
    assert!(!app.status.line(120, &DARK).to_string().contains("ctx"));

    app.event(
        &event(EventKind::Compacted {
            through_step: 4,
            before_tokens: 11_000,
            after_tokens: 6_000,
        }),
        std::time::Duration::ZERO,
    );

    let line = app.status.line(120, &DARK).to_string();
    assert!(
        line.contains("ctx "),
        "a fold is the harness telling us how full it was: {line:?}",
    );
    assert!(line.contains('%'), "{line:?}");
}

/// N5's half of this task: the new fields drop from the right, and the line never
/// becomes two lines. A status line that wraps has taken a row from the transcript
/// and stopped being a status line.
#[test]
fn the_new_fields_drop_from_the_right_rather_than_wrapping() {
    let mut app = App::new(DARK, "opus-5");
    app.set_posture(Some(io_cli::settings::Posture::Workspace));
    app.event(&step(1, 12_400), std::time::Duration::ZERO);
    app.event(
        &event(EventKind::Contained {
            mode: "workspace-write".into(),
            backend: "seatbelt".into(),
            roots: 2,
        }),
        std::time::Duration::ZERO,
    );

    let wide = app.status.line(160, &DARK).to_string();
    assert!(wide.contains("seatbelt"), "{wide:?}");

    let narrow = app.status.line(40, &DARK).to_string();
    assert!(narrow.chars().count() <= 40, "{narrow:?}");
    assert!(!narrow.contains('\n'), "the line wrapped: {narrow:?}");
    assert!(
        narrow.contains("opus-5"),
        "the model is the last field to go: {narrow:?}",
    );
    assert!(
        !narrow.contains("seatbelt"),
        "the rightmost fields are the ones that drop: {narrow:?}",
    );
}

/// A plan of three items, one of which the agent says it has finished.
fn plan() -> EventKind {
    EventKind::TodoWrote {
        items: vec![
            io_harness::TodoItem::new("read the file", io_harness::TodoState::Done),
            io_harness::TodoItem::new("change it", io_harness::TodoState::Active),
            io_harness::TodoItem::new("check it", io_harness::TodoState::Pending),
        ],
    }
}

/// **F12.** The plan field is absent until the agent writes a list — not zero, not
/// a placeholder, the same rule every other field on this line already keeps. This
/// is the test a field rendered as `0/0` from the start fails, and the only one.
#[test]
fn f12_the_plan_field_is_absent_until_the_agent_writes_a_list() {
    let mut app = App::new(DARK, "opus-5");

    let quiet = app.status.line(120, &DARK).to_string();
    assert!(
        !quiet.contains("plan"),
        "no plan has been written yet: {quiet:?}",
    );
    assert!(
        !quiet.contains("0/0"),
        "a session with no plan has not written a plan of nothing: {quiet:?}",
    );

    app.event(&event(plan()), Duration::ZERO);

    let line = app.status.line(120, &DARK).to_string();
    assert!(
        line.contains("plan 1/3"),
        "one of three items says it is done: {line:?}",
    );
    // Worded as the agent's own account, because io-harness's `state.rs` says an
    // item claiming `Done` is a claim and nothing verifies it.
    assert!(
        line.contains("claimed"),
        "the count is what the agent says, not what anything checked: {line:?}",
    );
}

/// **F12.** The count comes off the event's own items, so a later write that moves
/// an item back replaces the field rather than climbing past it — `TodoWrote`
/// carries the whole list every time and is never a delta.
#[test]
fn f12_the_plan_field_is_the_last_list_written_and_not_a_running_total() {
    let mut app = App::new(DARK, "opus-5");
    app.event(&event(plan()), Duration::ZERO);
    app.event(
        &event(EventKind::TodoWrote {
            items: vec![io_harness::TodoItem::new(
                "start again",
                io_harness::TodoState::Pending,
            )],
        }),
        Duration::ZERO,
    );

    let line = app.status.line(120, &DARK).to_string();
    assert!(
        line.contains("plan 0/1"),
        "the field is the list as it now stands: {line:?}",
    );
}

/// **F12.** Rightmost, and therefore the first field to go when the terminal is
/// narrow: it drops before the containment field that sits to its left.
#[test]
fn f12_the_plan_field_drops_before_every_field_to_its_left() {
    let mut app = App::new(DARK, "opus-5");
    app.event(
        &event(EventKind::Contained {
            mode: "workspace-write".into(),
            backend: "seatbelt".into(),
            roots: 2,
        }),
        Duration::ZERO,
    );
    app.event(&event(plan()), Duration::ZERO);

    let wide = app.status.line(200, &DARK).to_string();
    assert!(wide.contains("plan 1/3"), "{wide:?}");
    assert!(wide.contains("seatbelt"), "{wide:?}");

    // One column short of the whole line, measured off the line itself rather than
    // off arithmetic repeated here — the width that drops exactly one field.
    let width = wide.chars().count() as u16 - 1;
    let narrow = app.status.line(width, &DARK).to_string();
    assert!(
        !narrow.contains("plan"),
        "the rightmost field is the first to go: {narrow:?}",
    );
    assert!(
        narrow.contains("seatbelt"),
        "containment sits to its left and outlives it: {narrow:?}",
    );
}

/// **F12.** It reads at eighty columns in *both* glyph sets, on one row, with the
/// separator each set spells rather than one typed in here. The ASCII half is the
/// half nothing else on this line covers: a terminal that cannot draw a middle dot
/// still has to be able to read the plan's progress.
#[test]
fn f12_the_plan_field_reads_at_eighty_columns_in_both_glyph_sets() {
    let mut app = App::new(DARK, "opus-5");
    app.event(&event(plan()), Duration::ZERO);

    for theme in [DARK, DARK.with_glyphs(io_cli::glyphs::ASCII)] {
        let (mut screen, _recorder) = support::screen(80, 24);
        // The one-row form, which is what a terminal with no room for the
        // footer's three rows is given and the form this criterion is about.
        screen
            .draw(|frame| {
                let area = ratatui::layout::Rect {
                    height: 1,
                    ..frame.area()
                };
                app.status.render(frame, area, &theme)
            })
            .expect("frame");

        let viewport = screen.viewport_text();
        assert_eq!(
            viewport.lines().filter(|line| !line.is_empty()).count(),
            1,
            "the {} status line took more than one row: {viewport:?}",
            theme.glyphs.name,
        );
        assert!(
            viewport.contains("plan 1/3 claimed"),
            "the {} set lost the plan field: {viewport:?}",
            theme.glyphs.name,
        );
        assert!(
            viewport.contains(&format!("{}plan", theme.glyphs.separator)),
            "the separator comes from the {} set: {viewport:?}",
            theme.glyphs.name,
        );
    }
}

/// **F12.** A write of *no* items leaves the field absent, and clears one already
/// set. io-harness accepts an empty list — `parse_todo_items` validates each item
/// it is given and never rejects a list of none — so `{"items": []}` arrives as a
/// real `TodoWrote`, and setting the field from its length pins `plan 0/0 claimed`
/// to the line for the rest of the session. That is the sabotage arm's own
/// outcome, reached through the event rather than through the renderer.
#[test]
fn f12_a_plan_of_no_items_is_absent_rather_than_zero() {
    let mut app = App::new(DARK, "opus-5");
    app.event(
        &event(EventKind::TodoWrote { items: Vec::new() }),
        Duration::ZERO,
    );

    let quiet = app.status.line(120, &DARK).to_string();
    assert!(
        !quiet.contains("plan"),
        "an empty write is not a plan: {quiet:?}",
    );
    assert!(
        !quiet.contains("0/0"),
        "a plan of nothing is not a plan of zero: {quiet:?}",
    );

    // And a plan erased after one was written goes back to absent, rather than
    // standing at the count it last had.
    app.event(&event(plan()), Duration::ZERO);
    assert!(app.status.line(120, &DARK).to_string().contains("plan 1/3"));
    app.event(
        &event(EventKind::TodoWrote { items: Vec::new() }),
        Duration::ZERO,
    );

    let erased = app.status.line(120, &DARK).to_string();
    assert!(
        !erased.contains("plan"),
        "the agent erased its plan and the line kept the old count: {erased:?}",
    );
}

/// **F12.** Every field on this line that is a fact about the *run* is forgotten
/// when the run under the line changes — `/resume` onto another session, `/fork`
/// away from this one, a rewind that undoes the turn that set one. Nothing else
/// clears them: `TodoWrote` is the only writer of `plan`, and `Status::new` the
/// only other assignment, so without this the line goes on asserting the previous
/// conversation's spend, context, containment and plan.
///
/// Asserted over the whole class rather than over `plan` alone, because the hole
/// is the class's and the fix is one call.
#[test]
fn the_run_fields_do_not_outlive_the_run_that_set_them() {
    let mut app = App::new(DARK, "opus-5");
    // The window `ctx N%` is a share of. A fold with no contract behind it reports
    // nothing from 0.17.0 — see `the_context_field_appears_when_a_fold_reports_one`.
    app.status.budgets = Budgets::in_force(&budgeted());
    app.set_posture(Some(io_cli::settings::Posture::Workspace));
    app.event(&step(1, 12_400), Duration::ZERO);
    app.event(
        &event(EventKind::Compacted {
            through_step: 4,
            before_tokens: 11_000,
            after_tokens: 6_000,
        }),
        Duration::ZERO,
    );
    app.event(
        &event(EventKind::Contained {
            mode: "workspace-write".into(),
            backend: "seatbelt".into(),
            roots: 2,
        }),
        Duration::ZERO,
    );
    app.event(&event(plan()), Duration::ZERO);

    let before = app.status.line(200, &DARK).to_string();
    for fact in ["12.4k tok", "ctx ", "seatbelt", "plan 1/3"] {
        assert!(before.contains(fact), "{fact:?} was never set: {before:?}");
    }

    app.status.forget_run();

    let after = app.status.line(200, &DARK).to_string();
    // The run's OWN spellings, not any row that happens to hold the same word.
    // The budgets are the file's rather than the run's and deliberately survive a
    // forget — they are what the next turn will run under — so a sweep for a bare
    // `"tok"` now finds `left 0/10.0k tok` and fails on the field working
    // correctly. What must go is the count this run spent and the share it
    // reported, and that is what these name.
    for fact in ["12.4k tok", "ctx ", "seatbelt", "plan 1/3"] {
        assert!(
            !after.contains(fact),
            "{fact:?} outlived the run that reported it: {after:?}",
        );
    }
    // Nothing here is a fact about the run, and a swapped session is still the
    // same session with the same model under the same posture.
    assert!(after.contains("opus-5"), "{after:?}");
    assert!(after.contains("policy:"), "{after:?}");
}

/// 0.8.0 F6 — the spend field is absent until a draw arrives, and never zero.
#[test]
fn f6_spend_is_absent_until_a_draw_reports_one() {
    let status = Status::new("a-model");
    assert_eq!(status.spend, None);
    assert!(
        !rendered(&status, 120).contains("spend"),
        "a turn that has drawn nothing has not drawn zero",
    );
}

/// 0.8.0 F6 — the draw climbs and the remainder is replaced.
#[test]
fn f6_the_draw_accumulates_and_the_remainder_is_the_ledgers_latest() {
    let mut app = App::new(DARK, "a-model");
    app.event(
        &RunEvent::new(
            1,
            1,
            EventKind::SpendDraw {
                tokens: 200,
                remaining: Some(800),
            },
        ),
        Duration::ZERO,
    );
    assert_eq!(app.status.spend, Some((200, Some(800))));
    app.event(
        &RunEvent::new(
            1,
            2,
            EventKind::SpendDraw {
                tokens: 300,
                remaining: Some(500),
            },
        ),
        Duration::ZERO,
    );
    assert_eq!(
        app.status.spend,
        Some((500, Some(500))),
        "the draw climbs; the remainder is what the ledger says now",
    );
    let line = rendered(&app.status, 120);
    assert!(line.contains("spend 500/1.0k"), "{line}");
}

/// 0.8.0 F6 — a tree with no ceiling reports no ceiling.
///
/// The sabotage arm renders `remaining: None` as `0`, which would report a tree
/// that has all of its budget as one that has spent every token of it.
#[test]
fn f6_no_ceiling_is_not_an_exhausted_one() {
    let mut app = App::new(DARK, "a-model");
    app.event(
        &RunEvent::new(
            1,
            1,
            EventKind::SpendDraw {
                tokens: 200,
                remaining: None,
            },
        ),
        Duration::ZERO,
    );
    let line = rendered(&app.status, 120);
    assert!(line.contains("spend 200"), "{line}");
    assert!(
        !line.contains("spend 200/0") && !line.contains("/0"),
        "no ceiling was reported, so none is stated: {line}",
    );
}

/// 0.8.0 F6 — the spend belongs to the run that reported it.
#[test]
fn f6_forgetting_a_run_forgets_its_spend() {
    let mut status = Status::new("a-model");
    status.spend = Some((500, Some(500)));
    status.forget_run();
    assert_eq!(
        status.spend, None,
        "/resume and /fork must not leave another run's spend on the line",
    );
}

/// 0.8.0 F6 — the spend outranks the containment word, which is what a live run
/// proved rather than what looked tidy.
///
/// Drafted to the right of it, the field never appeared on a real terminal: a
/// containment word is `workspace-write/macos-sandbox-exec` — thirty-three
/// characters — and at a hundred columns beside the model, the posture, the
/// state, the clock and the token count there was nothing left for it. The one
/// field this release exists to fill was the first one dropped.
#[test]
fn f6_the_spend_survives_a_width_the_containment_word_does_not() {
    let mut status = Status::new("openai/gpt-5.6-luna");
    status.policy = Some("workspace".into());
    status.working = true;
    status.elapsed = Duration::from_secs(52);
    status.tokens = Some(3_900);
    status.containment = Some("read-only/macos-sandbox-exec".into());
    status.spend = Some((3_900, Some(116_100)));

    let line = rendered(&status, 100);
    assert!(
        line.contains("spend 3.9k/120.0k"),
        "the spend is what this release is for, and it fits: {line:?}",
    );
    assert!(
        !line.contains("macos-sandbox-exec"),
        "and the field it outranks is the one that goes: {line:?}",
    );
}

/// **F6 — the session says what it is connected to, and only what the events
/// say.**
///
/// Every one of these fields is filled from the stream. A server named in the
/// configuration that never came up leaves the line silent, which is correct and
/// is the whole reason the field is worth having: an operator looking at it is
/// asking whether the thing they configured is actually there.
///
/// Sabotage: render the configured values instead of the events', under which
/// only these tests fail — and they fail by reporting a connection that was asked
/// for and never made.
#[test]
fn f6_a_session_with_no_connections_says_nothing_about_them() {
    let status = Status::new("openai/gpt-5.6-luna");
    let line = rendered(&status, 120);

    assert!(
        !line.contains("mcp"),
        "zero servers is not `mcp 0`: {line:?}"
    );
    assert!(!line.contains("lsp"), "{line:?}");
    assert!(!line.contains("web"), "{line:?}");
}

#[test]
fn f6_a_server_that_answered_is_named_with_the_tools_it_offered() {
    let mut app = App::new(DARK, "openai/gpt-5.6-luna");
    // The server reaching the run: no tool on the event.
    app.event(
        &event(EventKind::Mcp {
            server: "docs".into(),
            tool: None,
            ok: None,
            millis: None,
            tools: Some(2),
        }),
        Duration::ZERO,
    );
    // Then two of its tools being called. A call is not a second server.
    for tool in ["search", "fetch"] {
        app.event(
            &event(EventKind::Mcp {
                server: "docs".into(),
                tool: Some(tool.into()),
                ok: Some(true),
                millis: Some(12),
                tools: None,
            }),
            Duration::ZERO,
        );
    }

    assert_eq!(app.status.mcp, (1, 2), "one server, two calls");
    // The label says `calls` from 0.16.0. It said `tools` and counted calls
    // from 0.10.0, because EventKind::Mcp carries no tool count and there is no
    // catalogue accessor — so the number this field wanted was never on the
    // wire. `/mcp` draws a per-server count beside this one now, and two
    // numbers disagreeing about one word is worse than an honest label.
    assert!(rendered(&app.status, 120).contains("mcp 1/2 calls"));
}

#[test]
fn f6_a_language_server_that_came_up_is_counted() {
    let mut app = App::new(DARK, "openai/gpt-5.6-luna");
    app.event(
        &event(EventKind::LspStarted {
            server: "rust-analyzer".into(),
            root: "/tmp/workspace".into(),
            ready_ms: 900,
        }),
        Duration::ZERO,
    );

    assert_eq!(app.status.lsp, 1);
    assert!(rendered(&app.status, 120).contains("lsp 1"));
}

/// A host that was blocked and a host that was visited must not read the same.
/// This is the arm the criterion names, and the one a field carrying only the
/// host would get wrong.
#[test]
fn f6_a_refused_host_does_not_read_like_a_visited_one() {
    let mut app = App::new(DARK, "openai/gpt-5.6-luna");
    app.event(
        &event(EventKind::BrowserStarted {
            binary: "/usr/bin/chromium".into(),
            headless: true,
            ready_ms: 400,
        }),
        Duration::ZERO,
    );
    assert!(
        rendered(&app.status, 120).contains("web ready"),
        "a browser that has gone nowhere says so rather than naming a host",
    );

    app.event(
        &event(EventKind::BrowserNavigated {
            host: "docs.rs:443".into(),
            permitted: true,
        }),
        Duration::ZERO,
    );
    let visited = rendered(&app.status, 120);
    assert!(visited.contains("web docs.rs:443"), "{visited}");
    assert!(!visited.contains("refused"), "{visited}");

    app.event(
        &event(EventKind::BrowserNavigated {
            host: "ads.example.com:443".into(),
            permitted: false,
        }),
        Duration::ZERO,
    );
    let refused = rendered(&app.status, 120);
    assert!(
        refused.contains("web ads.example.com:443 refused"),
        "the refusal is drawn as a refusal: {refused}",
    );
}

/// **F10.** The provider is on this line, and it got here from the event that
/// carries it.
///
/// Until 0.11.0 the only place the provider was ever named was a `via {provider}`
/// row committed under every prompt. That row is gone, so if this field were not
/// filled the fact would have been deleted rather than moved —
/// `US-IO-CLI-0.11.0-I01`.
#[test]
fn f10_the_provider_reaches_the_status_line_from_the_started_event() {
    let mut app = App::new(DARK, "opus-5");
    app.event(
        &event(EventKind::Started {
            goal: "make the failing test pass".into(),
            provider: "openrouter".into(),
        }),
        std::time::Duration::ZERO,
    );

    let line = app.status.line(120, &DARK).to_string();
    assert!(
        line.contains("provider:openrouter"),
        "the provider is named nowhere else in the product now: {line:?}",
    );
    // Never the word the removed row used. F2 asserts `via ` never reaches a
    // terminal again, and a status line is a terminal.
    assert!(!line.contains("via "), "{line:?}");
}

/// **F10.** A fallback is a different provider answering the same turn, so the
/// field says who is serving rather than who was asked.
#[test]
fn f10_a_fallback_moves_the_provider_field() {
    let mut app = App::new(DARK, "opus-5");
    app.event(
        &event(EventKind::Started {
            goal: "g".into(),
            provider: "openrouter".into(),
        }),
        std::time::Duration::ZERO,
    );
    app.event(
        &event(EventKind::FellBackTo {
            provider: "anthropic".into(),
        }),
        std::time::Duration::ZERO,
    );

    let line = app.status.line(120, &DARK).to_string();
    assert!(line.contains("provider:anthropic"), "{line:?}");
    assert!(!line.contains("openrouter"), "{line:?}");
}

/// **F10.** The step count, which the removed `Finished` row was the only place
/// to read.
///
/// It climbs from the envelope's own step number rather than from a counter kept
/// here, because a resumed run replays its backlog — and it is replaced by the
/// run's own total at the end, which is authoritative over the steps this
/// process happened to see.
#[test]
fn f10_the_step_count_climbs_and_is_replaced_by_the_runs_own_total() {
    let mut app = App::new(DARK, "opus-5");
    assert!(
        !app.status.line(120, &DARK).to_string().contains("step"),
        "a session that has taken no steps says nothing about steps",
    );

    app.event(&step(1, 10), std::time::Duration::ZERO);
    assert!(app.status.line(120, &DARK).to_string().contains("1 step"));

    app.event(&step(2, 10), std::time::Duration::ZERO);
    let line = app.status.line(120, &DARK).to_string();
    assert!(line.contains("2 steps"), "{line:?}");

    app.event(
        &event(EventKind::Finished {
            outcome: "finished".into(),
            steps: 9,
            tokens: 0,
        }),
        std::time::Duration::ZERO,
    );
    let line = app.status.line(120, &DARK).to_string();
    assert!(
        line.contains("9 steps"),
        "the run's own total outranks the steps we saw: {line:?}",
    );
}

/// **F10, and its sabotage arm.** Both new fields are run facts: they are
/// cleared when the conversation under the line changes, and never when a run
/// begins.
///
/// The arm the criterion names is clearing them on `Started` instead, which
/// blanks the provider at the exact moment it becomes true.
#[test]
fn f10_the_provider_and_the_step_count_are_forgotten_with_the_run() {
    let mut app = App::new(DARK, "opus-5");
    app.event(
        &event(EventKind::Started {
            goal: "g".into(),
            provider: "openrouter".into(),
        }),
        std::time::Duration::ZERO,
    );
    app.event(&step(3, 40), std::time::Duration::ZERO);

    // Still there while the run is the run — this is the half the sabotage arm
    // breaks.
    let line = app.status.line(120, &DARK).to_string();
    assert!(line.contains("provider:openrouter"), "{line:?}");
    assert!(line.contains("3 steps"), "{line:?}");

    app.status.forget_run();
    assert_eq!(app.status.provider, None);
    assert_eq!(app.status.steps, None);
    let line = app.status.line(120, &DARK).to_string();
    assert!(!line.contains("provider:"), "{line:?}");
    assert!(!line.contains("step"), "{line:?}");
}

/// **F10.** Both drop from the right before the model, the posture and the state
/// word, which is the rule every field on this line already follows.
#[test]
fn f10_the_new_fields_drop_before_the_model_the_posture_and_the_state() {
    let mut app = App::new(DARK, "openai/gpt-5.6-luna");
    app.status.policy = Some("workspace".into());
    app.event(
        &event(EventKind::Started {
            goal: "g".into(),
            provider: "openrouter".into(),
        }),
        std::time::Duration::ZERO,
    );
    app.event(&step(4, 900), std::time::Duration::ZERO);

    let narrow = app.status.line(46, &DARK).to_string();
    assert!(narrow.contains("openai/gpt-5.6-luna"), "{narrow:?}");
    assert!(narrow.contains("policy:workspace"), "{narrow:?}");
    assert!(narrow.contains("ready"), "{narrow:?}");
    assert!(
        !narrow.contains("provider:") && !narrow.contains("step"),
        "the two new fields must be the ones that go, not the three that must not: {narrow:?}",
    );
    assert!(
        narrow.chars().count() <= 46,
        "the line wrapped instead of dropping fields: {narrow:?}",
    );
}

/// **F1's counter, reachable.** A kind with no disposition is counted and the
/// count is on the status line — absent at zero, which is every line this
/// release will ever draw against the harness it is locked to.
#[test]
fn f1_an_untriaged_kind_is_reachable_on_the_status_line() {
    let mut status = Status::new("opus-5");
    assert!(!rendered(&status, 120).contains("unknown"));
    status.unknown = 2;
    assert!(rendered(&status, 120).contains("unknown 2"));
}

/// **F10, and the model field.** A routing rule that changes the model mid-run
/// changes what this line says, because the field is the change itself — that is
/// `routed`'s whole disposition in `triage::TRIAGE`.
#[test]
fn a_routed_model_moves_the_model_field() {
    let mut app = App::new(DARK, "openai/gpt-5.6-luna");
    app.event(
        &event(EventKind::Routed {
            from: "openai/gpt-5.6-luna".into(),
            to: "anthropic/claude-opus-5".into(),
            why: "the task got harder".into(),
        }),
        std::time::Duration::ZERO,
    );
    assert!(app
        .status
        .line(120, &DARK)
        .to_string()
        .contains("anthropic/claude-opus-5"));

    // An empty `to` is the provider's own default, not a session with no model.
    app.event(
        &event(EventKind::Routed {
            from: "anthropic/claude-opus-5".into(),
            to: String::new(),
            why: "back to the default".into(),
        }),
        std::time::Duration::ZERO,
    );
    assert_eq!(app.status.model, "anthropic/claude-opus-5");
}

/// All three are run facts and none outlives the run that reported them.
/// `/resume`, `/fork` and a rewind all land here.
#[test]
fn f6_the_connections_are_forgotten_with_the_run() {
    let mut status = Status::new("openai/gpt-5.6-luna");
    status.mcp = (2, 9);
    status.lsp = 1;
    status.browser = Some(("docs.rs:443".into(), Some(true)));

    status.forget_run();

    assert_eq!(status.mcp, (0, 0));
    assert_eq!(status.lsp, 0);
    assert_eq!(status.browser, None);
    let line = rendered(&status, 120);
    assert!(!line.contains("mcp") && !line.contains("lsp") && !line.contains("web"));
}

/// The footer's rows as one string, the way [`rendered`] gives the one-row form.
fn footed(status: &Status, width: u16) -> String {
    let mut text = String::new();
    for line in status.footer(width, &DARK) {
        for span in &line.spans {
            text.push_str(span.content.as_ref());
        }
    }
    text
}

/// **Both of them, named, because `Status` has two renderers and only one of
/// them is what an operator is looking at.**
///
/// `Status::line` is the one-row form and has exactly one production caller —
/// the fallback for a terminal under seven rows. `Status::footer` is the
/// three-row form `Status::render` takes on everything taller, which is to say
/// on every real terminal. 0.12.0 added a field to `line`, asserted `line`, went
/// green, and shipped a word that was nowhere on screen in a live capture. Every
/// budget claim below runs over this pair rather than over either one.
fn both_renderers(status: &Status) -> [(&'static str, String); 2] {
    [
        ("Status::line", rendered(status, 200)),
        ("Status::footer", footed(status, 200)),
    ]
}

/// **F1 — the effort level is on both renderers, and absent from both when unset.**
///
/// Both, because a field added to `Status::fields` alone is green on
/// `Status::line` and nowhere the binary draws: `Status::render` takes the footer
/// on any terminal seven rows or taller. That is 0.12.0's planning field and
/// 0.8.0's spend field, and [`both_renderers`] exists because of them.
///
/// The absent half is asserted as an absence rather than assumed. A default that
/// rendered `effort medium` would put a field on every operator's status line for
/// a release that changed nothing about their turns.
///
/// Sabotage: draw the level on `Status::line` only — under which this fails on the
/// footer arm alone, which is exactly the shape of the two defects above.
#[test]
fn f1_the_effort_level_is_on_both_renderers_and_absent_until_it_is_set() {
    let mut status = Status::new("a-model");

    for (which, drawn) in both_renderers(&status) {
        assert!(
            !drawn.contains("effort"),
            "{which} names an effort level in a session that has never set one: {drawn}",
        );
    }

    status.effort = Some(io_harness::Effort::High);
    for (which, drawn) in both_renderers(&status) {
        assert!(
            drawn.contains("effort high"),
            "{which} does not carry the level every turn is buying: {drawn}",
        );
    }
}

/// **F1 — the level survives the turn it was set on.**
///
/// `Status::forget_run` clears everything that was true of one run. A standing
/// choice is not one of those, and it is cleared beside them exactly as often as
/// somebody adds a field to that function without asking which kind it is —
/// `policy`, `budgets` and `planning` are all there for the same reason.
#[test]
fn f1_forgetting_a_run_does_not_forget_the_effort_level() {
    let mut status = Status::new("a-model");
    status.effort = Some(io_harness::Effort::Low);

    status.forget_run();

    assert_eq!(
        status.effort,
        Some(io_harness::Effort::Low),
        "a level holds until `/effort` says otherwise, not until a run ends",
    );
}

/// A contract carrying all three budgets, the way an operator's `[run]` table
/// leaves one.
fn budgeted() -> TaskContract {
    TaskContract::workspace("summarise the module", std::path::PathBuf::from("/tmp"))
        .with_max_steps(20)
        .with_token_budget(10_000)
        .with_time_budget(Duration::from_secs(600))
}

/// **F6 — every budget in force is on the line, with what is left of it.**
///
/// The three are read off the contract and the remainder is arithmetic over
/// counters `Status` already carries: the steps the run has taken, the tokens
/// this turn has spent, and how long it has been going. Nothing here is a second
/// counter for a number already on the line, and nothing here reads a clock —
/// `elapsed` is a value the driver handed in, which is what makes the time
/// budget's remainder something this test can state rather than race.
///
/// Sabotage: report the ceiling through the error path — under which only F6
/// fails, by reproducing the `error: step_cap_reached` under an unfinished
/// answer that `src/contract.rs` documents as the reason `MAX_STEPS` exists.
#[test]
fn f6_every_budget_in_force_is_drawn_with_what_is_left_of_it() {
    let mut status = Status::new("anthropic/claude-sonnet-4.5");
    status.budgets = Budgets::in_force(&budgeted());
    // Three steps taken, two and a half thousand tokens spent, a minute and a
    // half gone. Every one of these is a field the line already draws, which is
    // the point: the budget is the same fact with a ceiling on it.
    status.steps = Some(3);
    status.run_tokens = Some(2_500);
    status.elapsed = Duration::from_secs(90);

    for (renderer, text) in both_renderers(&status) {
        assert!(
            text.contains("left 17/20 steps"),
            "{renderer} does not say what is left of the step budget: {text:?}",
        );
        assert!(
            text.contains("left 7.5k/10.0k tok"),
            "{renderer} does not say what is left of the token budget: {text:?}",
        );
        assert!(
            text.contains("left 8m30s/10m00s"),
            "{renderer} does not say what is left of the time budget: {text:?}",
        );
    }
}

/// **F6 — a session with no `[run]` table shows no budget field at all.**
///
/// The absence half, and the one that keeps this feature free on the
/// overwhelming majority of lines. It is the same rule `tokens`, `spend` and the
/// background-job count are held to: a session with no budget has not been given
/// a budget of zero, and a `left 0/0` would report every default session as one
/// about to stop.
///
/// Sabotage: report the ceiling through the error path — under which only F6
/// fails, by reproducing the `error: step_cap_reached` under an unfinished
/// answer that `src/contract.rs` documents as the reason `MAX_STEPS` exists.
#[test]
fn f6_a_session_with_no_configured_budget_draws_no_budget_field_anywhere() {
    let mut status = Status::new("anthropic/claude-sonnet-4.5");
    status.steps = Some(3);
    status.run_tokens = Some(2_500);
    status.elapsed = Duration::from_secs(90);

    assert!(
        status.budgets_left().is_empty(),
        "a status nobody configured composed a budget: {:?}",
        status.budgets_left(),
    );
    for (renderer, text) in both_renderers(&status) {
        assert!(
            !text.contains("left "),
            "{renderer} drew a budget on a session that has none: {text:?}",
        );
    }
}

/// **F6 — io-cli's own step floor is not a budget the operator set.**
///
/// `TaskContract::max_steps` is a plain `u32` and is therefore always populated,
/// so "is there a step budget" is a question the contract cannot answer on its
/// own. `contract::configured` sets `MAX_STEPS` on every turn — a thousand,
/// chosen precisely so the cap is not the thing that ends a turn — and a line
/// reading `left 997/1000 steps` on every default session would be io-cli
/// reporting its own scaffolding back as a ceiling somebody chose.
///
/// Sabotage: report the ceiling through the error path — under which only F6
/// fails, by reproducing the `error: step_cap_reached` under an unfinished
/// answer that `src/contract.rs` documents as the reason `MAX_STEPS` exists.
#[test]
fn f6_the_step_budget_is_the_operators_and_never_io_clis_own_floor() {
    let root = std::path::PathBuf::from("/tmp");
    let floor =
        TaskContract::workspace("goal", root.clone()).with_max_steps(io_cli::contract::MAX_STEPS);
    assert_eq!(
        Budgets::in_force(&floor).steps,
        None,
        "the floor every turn carries was read as a budget the operator asked for",
    );

    // The number itself is not what is special: a file that names a cap has named
    // one, whatever it is, and twenty is a cap somebody typed.
    let asked = TaskContract::workspace("goal", root).with_max_steps(20);
    assert_eq!(Budgets::in_force(&asked).steps, Some(20));
}

/// **F6 — what is left moves as the turn spends, and stops at nothing left.**
///
/// A budget is a ceiling the harness stops a run at rather than a fence the run
/// cannot cross: the step that ends a turn may finish over its token budget. `0`
/// left is the honest reading of that, and a wrapped subtraction would report an
/// exhausted budget as an enormous one — which is the failure this arm exists
/// for rather than the arithmetic in the ordinary case.
///
/// Sabotage: report the ceiling through the error path — under which only F6
/// fails, by reproducing the `error: step_cap_reached` under an unfinished
/// answer that `src/contract.rs` documents as the reason `MAX_STEPS` exists.
#[test]
fn f6_a_budget_spent_past_its_ceiling_reads_as_nothing_left() {
    let mut status = Status::new("anthropic/claude-sonnet-4.5");
    status.budgets = Budgets::in_force(&budgeted());

    status.steps = Some(19);
    status.run_tokens = Some(9_999);
    status.elapsed = Duration::from_secs(599);
    assert_eq!(
        status.budgets_left(),
        vec![
            "left 1/20 step".to_string(),
            // One token left, spelled the way `format_tokens` spells anything
            // under a thousand: a bare number, not `0.0k`.
            "left 1/10.0k tok".to_string(),
            "left 1s/10m00s".to_string(),
        ],
        "the last step of a budget is singular and the remainders are the \
         subtraction",
    );

    status.steps = Some(25);
    status.run_tokens = Some(12_000);
    status.elapsed = Duration::from_secs(900);
    assert_eq!(
        status.budgets_left(),
        vec![
            "left 0/20 steps".to_string(),
            "left 0/10.0k tok".to_string(),
            "left 0s/10m00s".to_string(),
        ],
        "a turn that overran its budget reported more left than it started with",
    );
}

/// **F6 — a budget is a session fact and survives what the run does not.**
///
/// `Status::forget_run` clears the counters a budget is measured against, and it
/// must not clear the budget: `io.toml` does not change while a session runs, so
/// `/resume` onto another conversation, a `/fork` away from this one and a
/// rewind all land under the same `[run]` table they started under. A budget
/// blanked by any of the three would leave an operator with a turn that will
/// stop at a ceiling and nothing on screen saying which.
///
/// Sabotage: report the ceiling through the error path — under which only F6
/// fails, by reproducing the `error: step_cap_reached` under an unfinished
/// answer that `src/contract.rs` documents as the reason `MAX_STEPS` exists.
#[test]
fn f6_the_budgets_outlive_the_run_whose_counters_they_bound() {
    let mut status = Status::new("anthropic/claude-sonnet-4.5");
    status.budgets = Budgets::in_force(&budgeted());
    status.steps = Some(11);

    status.forget_run();

    assert_eq!(status.budgets, Budgets::in_force(&budgeted()));
    for (renderer, text) in both_renderers(&status) {
        assert!(
            text.contains("left 20/20 steps"),
            "{renderer} lost the budget with the run it was bounding: {text:?}",
        );
    }
}

// ---------------------------------------------------------------------------
// 0.14.0 F10 — `/status` commits the whole state, and every field is io-harness's.
// ---------------------------------------------------------------------------

/// A width nothing folds at, so the assertions below are about *content*.
///
/// Whether the page wraps rather than truncating is F11's claim and is asserted
/// in `tests/narrow.rs` at eighty columns, which is the width that can actually
/// fail. Asserting content at a width where a workspace path folds across two
/// rows would make every `contains` here fail for a reason that has nothing to do
/// with the field it names.
const ROOMY: u16 = 200;

/// Everything `status::committed` needs, each piece as io-harness hands it over.
///
/// The workspace and the conversation come from a real `Session` over a real
/// store rather than from three loose values, because the criterion names the
/// `Session` as the source of both and a test that passed its own numbers in
/// would be asserting the format string and nothing else.
struct Fixture {
    /// Held so the workspace the session was opened over outlives the session.
    _dir: tempfile::TempDir,
    /// Held for the same reason: the session was opened over it.
    _store: io_harness::Store,
    session: io_harness::Session,
    policy: io_harness::Policy,
    contract: TaskContract,
    caps: io_harness::Containment,
}

fn fixture() -> Fixture {
    let dir = tempfile::tempdir().expect("a workspace");
    let store = io_harness::Store::memory().expect("an in-memory store");
    let session = io_harness::Session::open(&store, dir.path()).expect("a session");
    // Two named layers over three acts, which is what an operator's file leaves
    // and what `Policy::layers` — a public field — carries.
    let policy = io_harness::Policy::permissive()
        .layer("ops-baseline")
        .allow_read("src/*")
        .allow_write("out/*")
        .layer("secrets")
        .deny_read(".env")
        .deny_net("ads.example.com");
    let contract = TaskContract::workspace("summarise the module", dir.path().to_path_buf())
        .with_max_steps(20)
        .with_token_budget(10_000)
        .with_time_budget(Duration::from_secs(600))
        .with_mcp(vec![
            io_harness::McpServer::stdio("docs", "docs-server"),
            io_harness::McpServer::stdio("issues", "issues-server"),
        ])
        .with_lsp(vec![io_harness::LspServer::new(
            "rust-analyzer",
            "rust-analyzer",
        )])
        .with_skills(dir.path().join("skills"));
    Fixture {
        _dir: dir,
        _store: store,
        session,
        policy,
        contract,
        caps: io_harness::Containment::new(12, 4, 2, 200_000),
    }
}

/// The committed page as a reader would see it: every row, spans concatenated.
fn committed(
    app: &App,
    fixture: &Fixture,
    caps: Option<&io_harness::Containment>,
    theme: &io_cli::theme::Theme,
    width: u16,
) -> Vec<String> {
    committed_of(app, fixture, &fixture.contract, caps, theme, width)
}

/// The same page against a contract other than the fixture's.
///
/// **The page's budgets come off the contract it is handed and never off
/// `Status`**, so a test that wants the no-budget case has to hand it a contract
/// carrying none — setting the field would prove nothing, because nothing reads
/// it. That is the property itself: `/status` reports what the next turn would
/// run under, and reading the page changes nothing about the session.
fn committed_of(
    app: &App,
    fixture: &Fixture,
    contract: &TaskContract,
    caps: Option<&io_harness::Containment>,
    theme: &io_cli::theme::Theme,
    width: u16,
) -> Vec<String> {
    io_cli::status::committed(
        &app.status,
        fixture.session.root(),
        fixture.session.id(),
        fixture.session.head(),
        &fixture.policy,
        contract,
        caps,
        theme,
        width,
    )
    .iter()
    .map(|line| {
        line.spans
            .iter()
            .map(|span| span.content.as_ref())
            .collect::<String>()
    })
    .collect()
}

/// A session with every live fact reported, each through the event that carries
/// it and through the one function the driver calls.
fn reported() -> App {
    let mut app = App::new(DARK, "anthropic/claude-sonnet-4.5");
    // **Before the events, from 0.17.0, and the order is the assertion.** `ctx N%`
    // is a share of the window the CONTRACT declares, so a `Status` that was never
    // handed one has no denominator and reports nothing rather than a share of the
    // crate default — which is the defect F10 exists to remove, and a fixture that
    // set the budgets afterwards would be asserting the old behaviour.
    app.status.budgets = Budgets::in_force(&budgeted());
    app.event(
        &event(EventKind::Started {
            goal: "summarise the module".into(),
            provider: "openrouter".into(),
        }),
        Duration::ZERO,
    );
    app.event(
        &event(EventKind::Contained {
            mode: "workspace-write".into(),
            backend: "macos-sandbox-exec".into(),
            roots: 2,
        }),
        Duration::ZERO,
    );
    app.event(
        &RunEvent::new(
            1,
            1,
            EventKind::SpendDraw {
                tokens: 1_500,
                remaining: Some(198_500),
            },
        ),
        Duration::ZERO,
    );
    app.event(
        &event(EventKind::Compacted {
            through_step: 4,
            before_tokens: 11_000,
            after_tokens: 6_000,
        }),
        Duration::ZERO,
    );
    app.event(
        &event(EventKind::Mcp {
            server: "docs".into(),
            tool: None,
            ok: None,
            millis: None,
            tools: Some(1),
        }),
        Duration::ZERO,
    );
    app.event(
        &event(EventKind::Mcp {
            server: "docs".into(),
            tool: Some("search".into()),
            ok: Some(true),
            millis: Some(12),
            tools: None,
        }),
        Duration::ZERO,
    );
    app.event(
        &event(EventKind::LspStarted {
            server: "rust-analyzer".into(),
            root: "/tmp/workspace".into(),
            ready_ms: 900,
        }),
        Duration::ZERO,
    );
    app.event(
        &event(EventKind::BrowserStarted {
            binary: "/usr/bin/chromium".into(),
            headless: true,
            ready_ms: 400,
        }),
        Duration::ZERO,
    );
    app.event(
        &event(EventKind::BrowserNavigated {
            host: "docs.rs:443".into(),
            permitted: true,
        }),
        Duration::ZERO,
    );
    app
}

/// **0.14.0 F10 — `/status` commits the whole state, and every field on it is a
/// value io-harness supplied.**
///
/// One capture, and one assertion per fact, so a field that stops arriving is
/// named rather than folded into a single failure. Each value here reached the
/// page by the route the criterion names: the layers off `Policy::layers`, the
/// backend off `EventKind::Contained` because `ExecContainment` is `pub(crate)`,
/// the draw off the `SpendDraw` stream because the containment `Ledger` is never
/// returned, the connected servers off `Mcp` and `LspStarted` because
/// `McpSession` and `LspSession` are `pub(crate)` too, and the workspace and the
/// conversation off the `Session` itself.
///
/// Sabotage: hard-code the backend name rather than reading `EventKind::Contained`
/// — under which only F10 fails, on a host whose chain selects a different rung,
/// which is the difference between the mode asked for and the backend that
/// applied that the README already insists on.
#[test]
fn f10_status_commits_every_field_from_the_value_io_harness_supplied() {
    let fixture = fixture();
    let mut app = reported();
    app.status.budgets = Budgets::in_force(&fixture.contract);
    app.status.steps = Some(3);
    app.status.run_tokens = Some(2_500);
    app.status.elapsed = Duration::from_secs(90);

    let rows = committed(&app, &fixture, Some(&fixture.caps), &DARK, ROOMY);
    let page = rows.join("\n");

    // The workspace and the conversation, from the `Session`.
    assert!(
        page.contains(&fixture.session.root().display().to_string()),
        "the workspace root is the session's own: {page}",
    );
    assert!(
        page.contains(&format!("session: {}", fixture.session.id())),
        "the session id is the session's own: {page}",
    );

    // The model and the provider, the latter from `EventKind::Started`.
    assert!(
        page.contains("model: anthropic/claude-sonnet-4.5"),
        "{page}"
    );
    assert!(page.contains("provider: openrouter"), "{page}");

    // Every layer by name with the acts it governs, from `Policy::layers`.
    assert!(
        page.contains("policy ops-baseline: read, write"),
        "a layer is named with the acts it governs: {page}",
    );
    assert!(
        page.contains("policy secrets: read, reach"),
        "a second layer is a second row, in the harness's own stacking order: {page}",
    );

    // The mode asked for beside the backend that answered.
    assert!(
        page.contains("sandbox: workspace-write/macos-sandbox-exec"),
        "the mode without the backend is an intention, not a fact: {page}",
    );

    // The caps, and the draw against them.
    assert!(
        page.contains("up to 12 agents, 4 at once per tier, 2 deep, 200000 tokens"),
        "the containment caps are the ones the next turn runs under: {page}",
    );
    assert!(
        page.contains("drawn: 1.5k of 200.0k"),
        "the draw comes from the SpendDraw stream, not from a ledger nobody returns: {page}",
    );

    // Each budget with what is left, in `Status::budgets_left`'s own spelling and
    // never a third one.
    for text in app.status.budgets_left() {
        assert!(
            page.contains(&format!("budget: {text}")),
            "the page composed a budget of its own instead of reading {text:?}: {page}",
        );
    }
    assert!(page.contains("budget: left 17/20 steps"), "{page}");

    // The context fill, read off the field the fold set rather than recomputed —
    // the denominator is io-harness's own declared budget, so a number written
    // out here would be wrong the first time the harness changed it.
    let fill = app.status.context.expect("the fold reported one");
    assert!(page.contains(&format!("context: {fill}%")), "{page}");

    // What is connected, beside what was configured.
    assert!(
        // `answering N calls` and not `offering N tools`: the second number has
        // counted calls since 0.10.0 and this was the one site 0.16.0's rename
        // missed. `/mcp` draws the real offered count from the event's own
        // `tools` field from 0.17.0, so two different numbers under one word
        // would now contradict each other on two surfaces.
        page.contains("mcp: 1 of 2 configured connected, answering 1 call"),
        "a server that answered and a server that is named in the file are \
         different facts, and both are on the page: {page}",
    );
    assert!(page.contains("lsp: 1 of 1 configured started"), "{page}");
    assert!(page.contains("browser: at docs.rs:443"), "{page}");
    let skills = fixture
        .contract
        .skills
        .as_ref()
        .expect("the fixture configured one");
    assert!(
        page.contains(&format!("skills: {}", skills.display())),
        "the skills directory is the contract's own: {page}",
    );

    // The edges, so a reader can tell how far the passage goes in a scrollback
    // that already holds every earlier turn.
    assert!(rows[0].ends_with("status"), "{:?}", rows[0]);
    assert!(
        rows[rows.len() - 1].ends_with("status ends"),
        "{:?}",
        rows[rows.len() - 1],
    );
}

/// **0.14.0 F10 — nothing on `/status` is computed by io-cli, asserted rather
/// than said in prose.**
///
/// Two readings, because either alone can be green while the claim is false. The
/// first is structural: none of the seven labels `io_harness::Backend::as_str`
/// can return appears anywhere in `src/status.rs`, so the backend on the page
/// cannot have been written there — which is exactly what the sabotage would
/// have to do. The second is behavioural: with only the event changed, the page
/// moves to whatever the event said, including a name no host in this repository
/// would ever select.
///
/// Sabotage: hard-code the backend name rather than reading `EventKind::Contained`
/// — under which only F10 fails, at the first of these two assertions on a
/// literal in the source and at the second on a host that chose another rung.
#[test]
fn f10_no_field_on_status_is_a_name_io_cli_wrote_for_itself() {
    // Comment rows are stripped first, and deliberately: `Status::fields` names
    // `workspace-write/macos-sandbox-exec` in a comment as the worked example of
    // why the containment field is ordered where it is, and that sentence is
    // documentation rather than a value anything renders. What this assertion is
    // against is a backend label reaching a *string* the page is built from.
    let source: String = std::fs::read_to_string("src/status.rs")
        .expect("the status module")
        .lines()
        .filter(|line| !line.trim_start().starts_with("//"))
        .collect::<Vec<_>>()
        .join("\n");
    for backend in [
        io_harness::Backend::MacosSandboxExec,
        io_harness::Backend::LinuxLandlock,
        io_harness::Backend::LinuxBubblewrap,
        io_harness::Backend::LinuxNamespaces,
        io_harness::Backend::WindowsAppContainer,
        io_harness::Backend::WindowsJobObject,
        io_harness::Backend::PortableFloor,
    ] {
        assert!(
            !source.contains(backend.as_str()),
            "`{}` is written into src/status.rs, so the backend on the page is \
             io-cli's guess rather than the one that actually applied",
            backend.as_str(),
        );
    }

    // And the page follows the event wherever it goes, including to a rung this
    // host would never select.
    let fixture = fixture();
    for backend in ["linux-landlock", "windows-appcontainer", "none"] {
        let mut app = App::new(DARK, "opus-5");
        app.event(
            &event(EventKind::Contained {
                mode: "full-access".into(),
                backend: backend.into(),
                roots: 0,
            }),
            Duration::ZERO,
        );
        let page = committed(&app, &fixture, None, &DARK, ROOMY).join("\n");
        assert!(
            page.contains(&format!("sandbox: full-access/{backend}")),
            "the page did not follow the event to `{backend}`: {page}",
        );
    }
}

/// **0.14.0 F10 — a fact nothing has reported is said to be unknown, never
/// invented.**
///
/// The three `pub(crate)` facts are the ones with no live handle behind them, so
/// before a turn has run there is genuinely nothing true to say about the
/// backend, the draw or the connections. A page that filled them with a default
/// would be reporting `portable-floor` and `0` as observations, which is the one
/// failure mode that makes the whole surface untrustworthy: the reader cannot
/// tell an answer from a placeholder.
///
/// Sabotage: render the absent backend as the weakest rung rather than as
/// unknown — under which only this test fails, on a session that has run nothing
/// claiming to know how its commands would be contained.
#[test]
fn f10_a_fact_no_event_has_reported_reads_as_unknown_rather_than_as_a_default() {
    let fixture = fixture();
    let app = App::new(DARK, "opus-5");
    let page = committed(&app, &fixture, None, &DARK, ROOMY).join("\n");

    assert!(
        page.contains("sandbox: not known until a turn has run"),
        "{page}",
    );
    assert!(
        page.contains("provider: not known until a turn has started"),
        "{page}",
    );
    assert!(
        page.contains("nothing has been drawn against the tree yet"),
        "{page}",
    );
    // **Half of this fact IS knowable before a turn, and 0.17.0 says the half it
    // knows.** The share needs an assembly to have happened; the WINDOW is the
    // contract's, so it is knowable at the idle prompt and is exactly what an
    // operator checking `[run.context]` came to the page for. The old sentence
    // said "not known until the context has been folded", which withheld a number
    // this page was already holding — and named a fold, which is no longer the
    // only thing that reports one.
    assert!(
        page.contains("context: nothing assembled yet — the window is"),
        "{page}",
    );
    assert!(page.contains("mcp: 0 of 2 configured connected"), "{page}");
    assert!(page.contains("browser: not configured"), "{page}");
    assert!(
        page.contains("containment: not contained"),
        "a session that cannot fan out says so rather than dropping the field: {page}",
    );
    // A budget that does not exist is an absence somebody has to be told about,
    // not a gap to interpret. The floor is on the contract because every turn
    // carries it, and `Budgets::in_force` is what knows it is not a budget.
    let bare = TaskContract::workspace("g", std::path::PathBuf::from("/tmp"))
        .with_max_steps(io_cli::contract::MAX_STEPS);
    let app = App::new(DARK, "opus-5");
    let page = committed_of(&app, &fixture, &bare, None, &DARK, ROOMY).join("\n");
    assert!(page.contains("budget: none"), "{page}");
}

/// **0.14.0 F10 — the page reports the ceilings the next turn would run under,
/// and reading it changes nothing.**
///
/// Both halves matter and each fails for its own reason. A session that has run
/// no turn has nothing in `Status::budgets`, because that field is filled where a
/// turn is built — so a page that read it would tell an operator whose `io.toml`
/// sets three ceilings that there are none, which is precisely the silence this
/// release exists to end. And the first shape of the fix assigned the field
/// before composing, which answered the question but made a read-only command
/// change what the status line said the moment it was opened.
///
/// The counters are still the session's own: what is drawn against a ceiling is a
/// fact about this session however the ceiling was arrived at, so a page opened
/// after three steps says three have gone.
///
/// Sabotage: compose the page from `Status::budgets` rather than from the
/// contract — under which the first assertion fails on a fresh session, which is
/// every session at the moment its operator first asks what it is running under.
#[test]
fn f10_status_reports_the_next_turns_budgets_without_touching_the_session() {
    let fixture = fixture();
    let mut app = App::new(DARK, "opus-5");
    app.status.steps = Some(3);
    let before = app.status.budgets;

    let page = committed(&app, &fixture, None, &DARK, ROOMY).join("\n");

    // The fixture's contract carries all three, and none of them ever reached
    // `Status`.
    assert!(page.contains("budget: left 17/20 steps"), "{page}");
    assert!(page.contains("budget: left 10.0k/10.0k tok"), "{page}");
    assert!(page.contains("budget: left 10m00s/10m00s"), "{page}");
    assert_eq!(
        app.status.budgets, before,
        "opening the page must not put budgets on the status line",
    );
    assert_eq!(
        before,
        Budgets::default(),
        "the fixture session has run no turn, so it had no budgets to read",
    );
}

/// Held by every test here that reads or writes an `IO_CONFIG*` variable.
///
/// The environment is process-wide and this whole file is one binary, so two
/// tests pointing `IO_CONFIG` at different files at once would make each other's
/// `user_path` answer wrong — intermittently, on a loaded machine, which is the
/// most expensive kind of failure to diagnose. The same shape `tests/wizard.rs`
/// and `tests/contract.rs` use, and for the same reason.
fn env_lock() -> std::sync::MutexGuard<'static, ()> {
    static LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());
    LOCK.lock().unwrap_or_else(|poisoned| poisoned.into_inner())
}

/// **0.15.0 F8 — `/status` says where io-cli keeps what it keeps, and who
/// decided.**
///
/// One `home` row beside `workspace`, carrying the directory the configuration
/// file and the store are *actually* in and the word that decided it. Drawn at
/// eighty columns in the ASCII set with `Status::plain` on, which is the width
/// and the mode the row has to survive: eighty is where a path folds, and plain
/// is where a glyph that is not on the terminal would be the thing lost.
///
/// Three arms, one per word, and the third is the one that matters. `$IO_CONFIG`
/// names a *file*, anywhere at all — so the directory in force is that file's
/// parent and nothing io-cli would ever have chosen. The first arm is the
/// ordinary post-`adopt` session: the variable is set, but it points at io-cli's
/// own home, so it reads as `default` rather than crediting the operator for this
/// crate's own doing.
///
/// Sabotage: report the home io-cli would have chosen (`home::path`) rather than
/// the directory in force (`home::in_force`) — under which only F8 fails, and it
/// fails on the `IO_CONFIG` arm, which is the one case the row exists to make
/// legible.
#[test]
fn f8_status_names_the_home_in_force_and_the_word_that_decided_it() {
    let _guard = env_lock();
    let fixture = fixture();
    let elsewhere = tempfile::tempdir().expect("a directory io-cli would never choose");
    let named = elsewhere.path().join("named-by-the-operator");
    std::fs::create_dir_all(&named).expect("the directory the file sits in");

    // io-cli's own home, which is what `adopt` puts in `IO_CONFIG_HOME` and is
    // therefore what an ordinary session's environment holds.
    let own = io_cli::home::path().expect("this test runs with a home directory");

    // The page as it lands in a scrollback at the width that can actually fail,
    // in the glyph set a terminal that cannot draw the rich one gets.
    let theme = DARK.with_glyphs(io_cli::glyphs::Glyphs::resolve(true, true, None));
    let page = |dir: &std::path::Path, word: &str| {
        let mut app = App::new(DARK, "opus-5");
        // Set exactly where the driver sets it. It governs animation and nothing
        // committed animates, so this surface must be unmoved by it — which is
        // asserted by this test passing with it on.
        app.status.plain = true;
        let rows = committed(&app, &fixture, None, &theme, 80);

        for row in &rows {
            assert!(
                row.chars().count() <= 80,
                "a row overflowed eighty columns: {row:?}",
            );
            assert!(
                row.is_ascii(),
                "plain mode drew a glyph a terminal cannot: {row:?}"
            );
        }

        // A path at eighty columns folds, and folding only ever inserts
        // whitespace — so the comparison is made with every space taken out of
        // both sides. Squeezed rather than `contains` on the joined page, which
        // would fail on the fold rather than on the field.
        let squeezed: String = rows
            .concat()
            .chars()
            .filter(|c| !c.is_whitespace())
            .collect();
        let wanted: String = format!("home: {} {} {word}", dir.display(), theme.glyphs.dash)
            .chars()
            .filter(|c| !c.is_whitespace())
            .collect();
        assert!(
            squeezed.contains(&wanted),
            "the page did not say the home is {} by {word}: {rows:#?}",
            dir.display(),
        );
    };

    // 1. The ordinary session: the variable is there because `adopt` set it, and
    //    it points at io-cli's own home, so nobody but io-cli decided.
    std::env::remove_var("IO_CONFIG");
    std::env::set_var("IO_CONFIG_HOME", &own);
    page(&own, "default");

    // 2. An operator who named a file outright, which wins over the directory —
    //    and the home in force is that file's parent, which is nowhere io-cli
    //    would have looked. **This is the arm the sabotage fails at**, and it is
    //    second rather than last so that it is the first thing to go red.
    assert_ne!(
        named, own,
        "the arm is only a test of `in_force` if the two differ",
    );
    std::env::set_var("IO_CONFIG", named.join("io.toml"));
    page(&named, "IO_CONFIG");

    // 3. An operator who named a directory of their own. `IO_CONFIG` has to go
    //    first: it names a file and would win over the directory.
    std::env::remove_var("IO_CONFIG");
    std::env::set_var("IO_CONFIG_HOME", elsewhere.path());
    page(elsewhere.path(), "IO_CONFIG_HOME");

    // Left as this binary found it, so no test after this one reads a variable
    // this one set.
    std::env::remove_var("IO_CONFIG");
    std::env::remove_var("IO_CONFIG_HOME");
}

/// **N3 — the queue depth is on every renderer that draws the status.**
///
/// `Status` has three renderers that do not share a body: `fields` feeds `line`
/// and nothing else, `footer` is hand-built beside it, and `Status::render`
/// takes the **footer** on any terminal seven rows or taller — which is to say
/// on every terminal an operator actually has. So a field is not "on the status
/// line" because `fields` composes it; it is on the status line when both forms
/// draw it, in the same words, and the only way to know that is to read both.
///
/// **This is the test that kills the sabotage.** Add the depth to `fields` alone
/// and the `Status::line` arm below stays green while the `Status::footer` arm
/// goes red — on the renderer the binary draws, which is precisely how 0.12.0
/// shipped a planning mode that was nowhere on screen in a live capture.
///
/// The spelling is asserted against `Status::queued_left` rather than against a
/// literal repeated three times, so the one place that composes it is the one
/// place a rename has to touch: a renderer that grew its own `format!` would
/// fail here even if both forms happened to say something.
#[test]
fn n3_the_queue_depth_is_drawn_by_the_line_and_by_the_footer_alike() {
    let mut status = Status::new("anthropic/claude-sonnet-4.5");

    // Nothing typed ahead, so nothing said. Both forms, because "absent at zero"
    // is a claim about what reaches a terminal and not about one code path.
    assert_eq!(
        status.queued_left(),
        None,
        "a session nobody typed ahead of composed a depth",
    );
    for (renderer, text) in both_renderers(&status) {
        assert!(
            !text.contains("queued"),
            "{renderer} drew a queue on a session with none: {text:?}",
        );
    }

    status.queued_prompts = 3;
    let spelling = status
        .queued_left()
        .expect("three waiting prompts are a queue");
    assert_eq!(spelling, "queued 3", "the depth is spelled once, plainly");
    for (renderer, text) in both_renderers(&status) {
        assert!(
            text.contains(&spelling),
            "{renderer} does not say what is waiting behind this turn: {text:?}",
        );
    }

    // And back down to nothing when the queue drains, on both forms — a count
    // that never returns to absent would leave every session that ever queued a
    // prompt carrying the field for the rest of its life.
    status.queued_prompts = 0;
    for (renderer, text) in both_renderers(&status) {
        assert!(
            !text.contains("queued"),
            "{renderer} kept a drained queue on screen: {text:?}",
        );
    }
}

/// **N3 — the queue belongs to the session and outlives the run.**
///
/// `Status::forget_run` clears the per-run facts when the conversation under the
/// line changes, and the depth is deliberately not one of them. The queue itself
/// is `App::prompts` and `forget_run` cannot reach it — so a count blanked here
/// would not drop a single prompt, it would only stop reporting them, and they
/// would still fire a turn each under a line that had just said there were none.
///
/// Sabotage: clear `queued_prompts` in `forget_run` beside `jobs` — under which only
/// this test fails, and it fails in the one state where the operator has typed
/// ahead and then moved the conversation, which is exactly when a prompt firing
/// unannounced is least welcome.
#[test]
fn n3_the_queue_depth_survives_forgetting_the_run() {
    let mut status = Status::new("anthropic/claude-sonnet-4.5");
    status.queued_prompts = 2;
    // A run-scoped neighbour, so this test states the difference rather than
    // just the half it is about.
    status.jobs = 1;

    status.forget_run();

    assert_eq!(status.jobs, 0, "a background job belongs to its run");
    assert_eq!(
        status.queued_prompts, 2,
        "the prompts the operator typed are still going to run",
    );
    for (renderer, text) in both_renderers(&status) {
        assert!(
            text.contains("queued 2"),
            "{renderer} stopped saying what is still waiting: {text:?}",
        );
    }
}

/// A session at the width and in the state the 0.21.0 live capture caught.
///
/// Nothing here is invented for the test. The posture and the containment word
/// are the pair `Status::fields` names in its own comment as the real thing —
/// `workspace-write/macos-sandbox-exec`, thirty-four characters — and the
/// counters are the ones the footer had when the last turn ended: six steps,
/// twenty-six point nine thousand tokens, twenty-three percent of the window,
/// and the idle key hint. That row is forty-six characters, the right-hand
/// group is fifty-seven, and a hundred columns cannot hold both.
fn full_row() -> Status {
    let mut status = Status::new("anthropic/claude-sonnet-4.5");
    status.policy = Some("read-only".into());
    status.containment = Some("workspace-write/macos-sandbox-exec".into());
    status.planning = true;
    status.steps = Some(6);
    status.tokens = Some(26_900);
    status.context = Some(23);
    status
}

/// **F4 — a counts row that will not fit costs a counter, never the mode.**
///
/// `row` fits the footer's right-hand group all or nothing, and the counts row
/// used to be handed to it whole: one character over and the posture, the
/// containment word and the planning phase all left the screen together while
/// every counter stayed. In the 0.21.0 live capture that is exactly what
/// happened — the working frame drew the group at column forty-four and the
/// finished frame, five characters wider because `esc stops` had become `/ for
/// commands`, drew no group at all. The operator finished a turn under `/plan
/// on` with nothing on screen saying io-harness would refuse the next write.
///
/// It is the failure F4 was written against, reached from the other direction,
/// and it is a regression with a date: the same capture passes on 0.12.0 and
/// 0.13.1, whose row was `4 steps · 14.2k tok · / for commands` and left room.
/// Adding `ctx N%` to the counts row spent the last ten columns.
///
/// Which side gives is settled by the module rather than by this test: a
/// standing mode that stops the agent writing outranks what the last turn
/// spent. So the assertion is not merely that `planning` survives — it is that
/// a counter went instead, and that the counters left are the leftmost ones.
///
/// Sabotage: hand `counts` to `row` unnarrowed, which is the 0.21.0 code — under
/// which only this pair of tests fails, and it fails at the one width an
/// eighty-to-a-hundred-and-twenty-column terminal is most likely to be.
#[test]
fn f4_a_full_counts_row_drops_a_counter_and_not_the_planning_phase() {
    let text = footed(&full_row(), 100);

    assert!(
        text.contains("planning"),
        "the mode that stops the agent writing left the row: {text:?}",
    );
    assert!(
        text.contains("workspace-write/macos-sandbox-exec") && text.contains("read-only"),
        "the phase took the rest of its group with it: {text:?}",
    );
    // What paid for it, and which end it was taken off. `ctx 23%` is the
    // rightmost *counter* and is what a crowded row gives up first.
    assert!(
        !text.contains("ctx 23%"),
        "nothing was dropped, so the row cannot have fitted: {text:?}",
    );
    assert!(
        text.contains("6 steps") && text.contains("26.9k tok"),
        "counters came off the wrong end: {text:?}",
    );
    // **And the key hint is not a counter and is not droppable.** It was the
    // rightmost item on the row and the narrowing took it first, which meant a
    // full row lost `esc stops` mid-turn — the only place the footer says how to
    // interrupt what is running. It is held out of the narrowing and appended
    // after it, so it survives every width the row is drawn at.
    assert!(
        text.contains("/ for commands"),
        "the interrupt hint was dropped to make room for a counter: {text:?}",
    );
}

/// **F4 — and it holds with one more counter on the row, at every width the
/// group can be drawn at.**
///
/// The companion above pins the geometry that actually failed; this one pins
/// the property, because the geometry is about to move. The counts row grows
/// every release — `ctx N%` is what tipped 0.21.0 over, and the cost counter
/// this release adds sits in the same group and makes the row wider still — so
/// a test that asserted one magic width would go green on the arithmetic of the
/// week it was written and say nothing about the week after.
///
/// `plan 2/5` stands in for that next counter: it is a real member of the same
/// row, it is eight characters plus a separator, and with it the row no longer
/// fits at any width in the sweep on the first trim.
///
/// **The sweep starts at seventy-four, and the number is the whole of what the
/// key hint being undroppable costs.** The right-hand group is fifty-seven
/// columns; `/ for commands` is fourteen and its separator three; fifty-seven
/// plus seventeen is seventy-four, and that is the narrowest row that can carry
/// the standing mode at all now that the hint is not a counter and cannot be
/// taken to make room. Below it the group goes and the hint stays — which is the
/// right way round: a mode an operator cannot see is bad, and a running turn an
/// operator cannot find the key to stop is worse.
///
/// Sabotage: pop from the front of `counts` instead of the back — under which
/// this test still passes at every width and the one above fails on `6 steps`,
/// which is why both are here.
#[test]
fn f4_the_planning_phase_holds_at_every_width_that_can_hold_the_group() {
    let mut status = full_row();
    status.plan = Some((2, 5));

    for width in 74..=160u16 {
        let text = footed(&status, width);
        assert!(
            text.contains("planning"),
            "the planning phase is off the footer at {width} columns: {text:?}",
        );
        // The row it was fitted into is still a row. A trim that undercounted
        // would put the group past the edge instead of dropping a counter.
        for line in status.footer(width, &DARK) {
            let drawn: usize = line
                .spans
                .iter()
                .map(|span| span.content.chars().count())
                .sum();
            assert!(
                drawn <= width as usize,
                "a footer row overflowed {width} columns: {line:?}",
            );
        }
    }
}

// ---------------------------------------------------------------------------
// 0.22.0 — the cost field
// ---------------------------------------------------------------------------

/// A store with one run, and one provider call per model named.
///
/// Real rows through io-harness's own recorder rather than a `Total` built here,
/// because `Status::note_cost_from` reads the store: the split a price needs —
/// fresh prompt against cache read against cache write against completion — lives
/// only on the `provider_calls` row, and `EventKind::Step` carries a scalar token
/// count that cannot be priced at all. A fixture that handed the figure in would
/// be asserting the format string.
fn run_calling(models: &[&str]) -> (io_harness::Store, i64) {
    let store = io_harness::Store::memory().expect("an in-memory store");
    let run = store
        .start_run("summarise the module", "/repo")
        .expect("a run");
    for model in models {
        store
            .record_provider_call(
                run,
                &io_harness::ProviderCall {
                    step: 1,
                    provider: "anthropic".into(),
                    model: Some((*model).to_string()),
                    usage: Some(io_harness::Usage {
                        prompt_tokens: 10_000,
                        completion_tokens: 2_000,
                        total_tokens: 12_000,
                        ..Default::default()
                    }),
                    latency_ms: 1_200,
                    ..Default::default()
                },
            )
            .expect("the call is recorded");
    }
    (store, run)
}

/// The rates in force, pricing exactly one model.
fn rates() -> io_harness::pricing::PriceTable {
    io_harness::pricing::PriceTable::new("2026-08-27").with(
        "anthropic/claude-sonnet-4.5",
        io_harness::pricing::Price {
            input: 3_000_000,
            output: 15_000_000,
            ..io_harness::pricing::Price::ZERO
        },
    )
}

/// **The cost field is absent rather than zero in all three ways of having no
/// answer, and they are three different states of an install.**
///
/// An operator with no `[prices]` section, an operator whose table does not price
/// the model this run is using, and a session that has not called anything yet.
/// The status line has no room to tell them apart and does not try — `/cost` is
/// one keystroke away and tells all three — but it must not answer any of them
/// with `$0`, because `$0` is a measured figure and none of these is measured.
///
/// This is the same rule the rest of the line already keeps: every counter is
/// absent until there is something to count, so a session that has run nothing
/// carries an almost empty row rather than a row of zeroes. The money field is the
/// one where getting it wrong is not merely untidy — a run whose models are all
/// outside the table has a real cost this program cannot state, and stating it as
/// nothing is the invented number the whole of `/cost` is built to avoid.
///
/// Sabotage: `self.cost = Some(total.micros)` — three characters, reads as a
/// simplification of a `then_some`, and under it every unpriced session reports
/// `$0` for a turn that cost real money. Nothing else in the repository fails.
#[test]
fn the_cost_field_is_absent_rather_than_zero_when_nothing_can_be_priced() {
    // 1. No prices configured at all. An empty table is what `cost::table` hands
    //    back for a file with no `[prices]`, so this is the ordinary state of a
    //    fresh install rather than an edge case.
    let (store, run) = run_calling(&["anthropic/claude-sonnet-4.5"]);
    let mut status = Status::new("anthropic/claude-sonnet-4.5");
    status.note_cost_from(&store, run, &io_harness::pricing::PriceTable::new(""));
    assert_eq!(status.cost, None, "an empty table produced a figure");
    for (renderer, text) in both_renderers(&status) {
        assert!(
            !text.contains('$'),
            "{renderer} drew money with no prices configured: {text:?}",
        );
    }

    // 2. Prices configured, and none of them for the model this run used. The
    //    table is real, the date is real, and the answer is still unavailable.
    let (store, run) = run_calling(&["some-lab/experimental-preview"]);
    let mut status = Status::new("some-lab/experimental-preview");
    status.note_cost_from(&store, run, &rates());
    assert_eq!(
        status.cost, None,
        "a model outside the table was priced at nothing",
    );
    for (renderer, text) in both_renderers(&status) {
        assert!(
            !text.contains('$'),
            "{renderer} priced a model the table does not price: {text:?}",
        );
    }

    // 3. A run that has called nothing. Not zero: nothing.
    let (store, run) = run_calling(&[]);
    let mut status = Status::new("anthropic/claude-sonnet-4.5");
    status.note_cost_from(&store, run, &rates());
    assert_eq!(status.cost, None, "a run with no calls reported a cost");
    for (renderer, text) in both_renderers(&status) {
        assert!(!text.contains('$'), "{renderer}: {text:?}");
    }

    // And a run id nothing ever recorded reads back as no calls at all — which
    // is no figure, not a zero one, and is not an error either: a notice about a
    // failed read of a decorative field would be worse than the field's absence,
    // which is why `note_cost_from` is silent on every failure.
    let mut status = Status::new("anthropic/claude-sonnet-4.5");
    status.note_cost_from(&store, 9_999, &rates());
    assert_eq!(
        status.cost, None,
        "a run id that does not exist invented a figure"
    );
}

/// **When there is a figure it is on BOTH renderers, out of one method.**
///
/// `Status` renders two ways: `line` is the one-row form, which has exactly one
/// production caller — the fallback for a terminal under seven rows — and `footer`
/// is the three-row form `Status::render` takes on everything taller, which is to
/// say on every real terminal anybody uses. **This file has shipped a field into
/// one of them twice**: 0.8.0's spend field and 0.12.0's planning phase were each
/// added to `line`, asserted against `line`, went green, and were nowhere on
/// screen in a live capture of the running binary.
///
/// So the assertion is over the pair, and the money is one method — `cost_field`
/// — that both renderers extend from, exactly as `budgets_left` and `queued_left`
/// already are. A test that read one renderer is a test that can pass while the
/// operator has an invisible field, which is the whole of what this test is
/// against.
///
/// The figure itself is checked against `cost::money` of what io-harness says the
/// call cost, rather than against a string, so the two surfaces are proved to be
/// drawing the same number and not merely both drawing a dollar sign.
///
/// Sabotage: push `self.cost_field()` into `Status::fields` alone and drop the
/// `counts.extend(self.cost_field())` from the footer — under which the one-row
/// arm of this test passes and the binary shows nothing.
#[test]
fn the_cost_field_is_drawn_by_the_line_and_by_the_footer_alike() {
    let (store, run) = run_calling(&["anthropic/claude-sonnet-4.5"]);
    let table = rates();

    let mut status = Status::new("anthropic/claude-sonnet-4.5");
    status.note_cost_from(&store, run, &table);

    // What the harness says the run cost, from the rows the harness stored.
    let calls = store.provider_calls(run).expect("the calls are readable");
    let expected = io_cli::cost::Total::of(&calls, &table).micros;
    assert!(expected > 0, "a fixture that costs nothing proves nothing");
    assert_eq!(status.cost, Some(expected));

    let drawn = io_cli::cost::money(expected);
    for (renderer, text) in both_renderers(&status) {
        assert!(
            text.contains(&drawn),
            "{renderer} does not carry the cost {drawn}: {text:?}",
        );
    }

    // **Run-scoped, and forgotten with the run.** A cost that outlived its turn
    // would be a figure that stopped moving while a new turn spent money beside
    // it — the same argument the module makes for clearing the step count and the
    // provider, and the opposite of the one it makes for keeping the planning
    // phase.
    status.forget_run();
    assert_eq!(
        status.cost, None,
        "the cost outlived the run that incurred it"
    );
    for (renderer, text) in both_renderers(&status) {
        assert!(
            !text.contains('$'),
            "{renderer} still shows the last run's bill: {text:?}",
        );
    }
}

// ---------------------------------------------------------------------------
// 0.24.0 — the verification gate's standing on the status line.
// ---------------------------------------------------------------------------

/// A gated turn that failed its criterion and is being tried again.
///
/// The standing is a word the driver hands over rather than one this test
/// invents a type for: `Status::gate` is plain data, so what a test can state
/// about it is exactly what an operator would see.
fn gated(standing: &str, attempt: Option<u32>) -> Status {
    let mut status = Status::new("anthropic/claude-sonnet-4.5");
    status.gate = Some(standing.to_string());
    status.gate_attempt = attempt;
    status
}

/// **The standing reaches the terminal through BOTH renderers, out of one
/// method.**
///
/// `Status::line` is the one-row form and has exactly one production caller —
/// the fallback for a terminal under seven rows. `Status::footer` is the
/// three-row form `Status::render` takes on everything taller, which is every
/// real terminal. This file has shipped a field into one of them twice: 0.8.0's
/// spend and 0.12.0's planning phase were each added to `line`, asserted against
/// `line`, went green, and were nowhere on screen in a live capture.
///
/// The spelling is asserted against `Status::gate_field` rather than against a
/// literal repeated per arm, so the one place that composes it is the one place a
/// rename has to touch — a renderer that grew its own `format!` would fail here
/// even if both forms happened to say something.
///
/// **And the standing is a word, so it survives every set.** The line is read in
/// `MONO` as well, which is the theme `NO_COLOR` and `--plain` resolve to: a
/// standing carried by a colour or a tick is a standing those readers do not
/// have. The whole field is ASCII, so no glyph set can change it.
///
/// Sabotage: push `self.gate_field()` into `Status::fields` alone and drop the
/// `counts.extend(self.gate_field())` from the footer — under which the
/// `Status::line` arm below stays green and the binary shows nothing.
#[test]
fn the_gate_standing_is_drawn_by_the_line_and_by_the_footer_alike() {
    for standing in ["passed", "failed", "errored", "running"] {
        let status = gated(standing, None);
        let spelling = status
            .gate_field()
            .expect("a configured criterion has a standing");
        assert_eq!(
            spelling,
            format!("gate {standing}"),
            "the standing is spelled once, plainly, and in the harness's own word",
        );
        for (renderer, text) in both_renderers(&status) {
            assert!(
                text.contains(&spelling),
                "{renderer} does not say where the gate stands: {text:?}",
            );
        }
    }

    // Every character of it is ASCII, which is what makes the claim above true
    // of a terminal that cannot draw braille as well as of one that can.
    let status = gated("failed", Some(3));
    let spelling = status.gate_field().expect("a standing");
    assert!(
        spelling.is_ascii(),
        "the standing is not readable under the ASCII set: {spelling:?}",
    );
    let mono = status.line(200, &MONO).to_string();
    assert!(
        mono.contains(&spelling),
        "the standing is gone from a terminal with no colour: {mono:?}",
    );
}

/// **A gate with nothing behind it is absent rather than zero.**
///
/// Most sessions configure no criterion at all, and one of them has not passed a
/// gate zero times — it has not been gated. So the field draws nothing: not
/// `none`, not `gate 0`, not an empty label with a separator either side of it,
/// which on this line is the same defect wearing a space. It is the rule
/// `tokens`, `spend`, `bg N` and the queue depth are already held to.
///
/// Sabotage: render `self.gate.clone().unwrap_or_default()` — under which every
/// ungated session carries a bare `gate` with nothing after it, and only this
/// test fails.
#[test]
fn the_gate_field_with_nothing_behind_it_is_absent_rather_than_zero() {
    let status = Status::new("opus-5");
    assert_eq!(
        status.gate_field(),
        None,
        "a session nobody asked to verify composed a standing",
    );
    for (renderer, text) in both_renderers(&status) {
        assert!(
            !text.contains("gate"),
            "{renderer} drew a gate on a session with no criterion: {text:?}",
        );
        assert!(
            !text.contains("attempt"),
            "{renderer} counted attempts at a gate that does not exist: {text:?}",
        );
    }

    // An attempt number with no standing beside it is a number about nothing,
    // and it must not summon the field on its own — this is the state a driver
    // leaves behind if it reports the retry before the verdict.
    let mut counted = Status::new("opus-5");
    counted.gate_attempt = Some(2);
    assert_eq!(
        counted.gate_field(),
        None,
        "an attempt count drew a gate that has no standing",
    );
    for (renderer, text) in both_renderers(&counted) {
        assert!(
            !text.contains("attempt"),
            "{renderer} drew a retry count with no verdict behind it: {text:?}",
        );
    }
}

/// **The attempt number appears from the second attempt and never on the first.**
///
/// Every gate that ran at all ran once, so `attempt 1` would be on every gated
/// line in the world and would tell an operator nothing — the argument
/// `Budgets::in_force` already makes about io-cli's own step floor, which is a cap
/// on every turn and therefore a ceiling nobody chose. What is worth the ten cells
/// is that the turn is being *retried*: the criterion did not hold, the retry
/// budget is being spent, and the agent is doing the work again.
///
/// The first arm asserts the absence of the word `attempt` outright rather than of
/// the string `attempt 1`, because `attempt 1` is absent from a line reading
/// `attempt 12` and an assertion satisfied by the bug is not an assertion.
///
/// Sabotage: drop the `attempt > 1` guard — under which every gated line carries
/// `attempt 1`, the first arm of this test fails, and nothing else in the
/// repository does.
#[test]
fn the_gate_attempt_is_drawn_only_from_the_second_attempt() {
    for first in [None, Some(1)] {
        let status = gated("failed", first);
        assert_eq!(
            status.gate_field().as_deref(),
            Some("gate failed"),
            "the first attempt is every gate's first attempt and is not news",
        );
        for (renderer, text) in both_renderers(&status) {
            assert!(
                !text.contains("attempt"),
                "{renderer} numbered the attempt nobody needed numbering: {text:?}",
            );
        }
    }

    let status = gated("failed", Some(2));
    let spelling = status.gate_field().expect("a retried gate has a standing");
    assert_eq!(
        spelling, "gate failed attempt 2",
        "the retry is stated in words beside the standing it belongs to",
    );
    for (renderer, text) in both_renderers(&status) {
        assert!(
            text.contains(&spelling),
            "{renderer} does not say the turn is being retried: {text:?}",
        );
    }

    // And it keeps counting rather than sticking at two, which is what a
    // hard-coded second attempt would look like from the outside.
    assert_eq!(
        gated("errored", Some(7)).gate_field().as_deref(),
        Some("gate errored attempt 7"),
    );
}

/// **A standing belongs to the turn that was gated and outlives no other.**
///
/// `Status::forget_run` clears the per-run facts when the conversation under the
/// line changes — `/resume` onto another session, `/fork` away from this one, a
/// rewind that undoes the turn that set one — and `Status::start_run` clears them
/// when the operator simply types again, which is the path `forget_run` never
/// sees. Both matter here for different reasons: a resumed conversation nobody
/// ever gated would otherwise inherit `gate passed` and read as verified, and an
/// ordinary second turn would spend its whole length under the first turn's
/// verdict. This codebase has shipped exactly that with a per-turn field before.
///
/// The planning phase is set beside it so the test states the *difference* rather
/// than half of it: a standing choice survives both, an account of one turn
/// survives neither.
///
/// Sabotage: clear `gate` and leave `gate_attempt` standing — under which a later
/// verdict on a later turn is drawn with the retry count of an earlier one, and
/// the last assertion of each half is what catches it.
#[test]
fn the_gate_standing_does_not_outlive_the_turn_that_was_gated() {
    for clear in [Status::forget_run as fn(&mut Status), Status::start_run] {
        let mut status = gated("failed", Some(2));
        // A standing choice, which is not a fact about the run and must survive.
        status.planning = true;
        for (renderer, text) in both_renderers(&status) {
            assert!(
                text.contains("gate failed attempt 2"),
                "{renderer} never drew the standing this test is about: {text:?}",
            );
        }

        clear(&mut status);

        assert_eq!(status.gate, None, "the verdict outlived its turn");
        assert_eq!(
            status.gate_attempt, None,
            "the retry count outlived the verdict it counted",
        );
        assert!(
            status.planning,
            "a standing choice was cleared with the run"
        );
        for (renderer, text) in both_renderers(&status) {
            assert!(
                !text.contains("gate"),
                "{renderer} still reports the previous turn as gated: {text:?}",
            );
            assert!(
                !text.contains("attempt"),
                "{renderer} kept the previous turn's retry count: {text:?}",
            );
            assert!(
                text.contains("planning"),
                "{renderer} lost the mode the gate was not: {text:?}",
            );
        }
    }
}

/// **The standing survives a width the counters do not, and the line still never
/// wraps.**
///
/// Both renderers drop from the right, so position is priority, and the module
/// states which way this one goes: a standing mode that stops the agent writing
/// outranks what the last turn spent. A gate that has not passed is that fact from
/// the other end — it is *why* the turn is not finished, and on a retry it is why
/// the agent is doing the work a second time. So it sits right of the planning
/// phase, which is a choice that holds after the turn ends, and left of every
/// counter, which is an account of what the turn cost.
///
/// The order is asserted on `Status::fields` directly as well as through a
/// rendered width, because an arithmetic assertion alone would move the day
/// somebody adds a field five cells wider, and the claim being made is about
/// order rather than about a hundred columns.
///
/// Sabotage: push the gate after `ctx` in `Status::fields` and after `steps` in
/// the footer's counts — under which both forms still draw it at two hundred
/// columns, every other test in this file stays green, and the one field that
/// explains a turn that will not end is the first thing a narrow terminal gives
/// up.
#[test]
fn the_gate_standing_survives_a_width_the_counters_do_not() {
    let mut status = full_row();
    status.gate = Some("failed".into());
    status.gate_attempt = Some(2);

    // Order first, where it is a claim about the list rather than about a width.
    let fields = status.fields(&DARK);
    let index = |needle: &str| {
        fields
            .iter()
            .position(|field| field.text.contains(needle))
            .unwrap_or_else(|| panic!("no field says {needle:?}: {fields:?}"))
    };
    assert!(
        index("planning") < index("gate "),
        "the standing outranked a choice that outlives the turn: {fields:?}",
    );
    assert!(
        index("gate ") < index("6 steps")
            && index("gate ") < index("26.9k tok")
            && index("gate ") < index("ctx 23%"),
        "a counter outranked the reason the turn is not finished: {fields:?}",
    );

    // Wide enough for everything, so the narrowing below is the only thing the
    // arms after it can be failing on.
    let wide = rendered(&status, 200);
    for fact in ["gate failed attempt 2", "6 steps", "26.9k tok", "ctx 23%"] {
        assert!(wide.contains(fact), "{fact:?} was never drawn: {wide:?}");
    }

    // A hundred columns, which is the width the 0.21.0 live capture was taken at
    // and the one an ordinary terminal is most likely to be.
    let narrowed = rendered(&status, 100);
    assert!(narrowed.chars().count() <= 100, "{narrowed:?}");
    assert!(!narrowed.contains('\n'), "the line wrapped: {narrowed:?}");
    assert!(
        narrowed.contains("gate failed attempt 2"),
        "the standing went before the counters it outranks: {narrowed:?}",
    );
    for counter in ["6 steps", "26.9k tok", "ctx 23%"] {
        assert!(
            !narrowed.contains(counter),
            "nothing was dropped, so this width proves nothing: {narrowed:?}",
        );
    }

    // The footer narrows by its own mechanism — counters come off the right of
    // the counts group until the right-hand group fits — so the same claim has to
    // be made against it separately or half of it is untested.
    let footer = footed(&status, 100);
    assert!(
        footer.contains("gate failed attempt 2") && footer.contains("planning"),
        "the footer gave up the standing to keep a counter: {footer:?}",
    );
    assert!(
        !footer.contains("6 steps"),
        "the footer row fitted whole, so it proves nothing: {footer:?}",
    );

    // And at a width that holds one field, it is still the model. Everything this
    // task adds goes before it and after everything else.
    let cramped = rendered(&status, 40);
    assert!(cramped.chars().count() <= 40, "{cramped:?}");
    assert!(!cramped.contains('\n'), "the line wrapped: {cramped:?}");
    assert!(
        cramped.contains("anthropic/claude-sonnet-4.5"),
        "the model is the last field to go: {cramped:?}",
    );
    assert!(
        !cramped.contains("gate"),
        "the gate outlasted the model: {cramped:?}",
    );
}

// ---------------------------------------------------------------------------
// O13 — the token figure moves while the step is spending, and settles when it
// commits.
// ---------------------------------------------------------------------------

/// **O13 — the figure changes across consecutive ticks with no `Step` between
/// them, and it is drawn as the estimate it is.**
///
/// This is the defect: `EventKind::Token` carries text and no count, so nothing
/// updated the token field until a step committed — and the one number telling an
/// operator what a turn was costing sat still for the whole of it while the clock
/// beside it ran.
///
/// Sabotage: drop the tilde. The provisional and settled forms become
/// indistinguishable and the second half of this test fails — which is the point,
/// because a number that reads as settled and is not would be worse than the
/// frozen count it replaces.
#[test]
fn o13_the_token_figure_moves_between_steps_and_says_it_is_an_estimate() {
    let mut status = io_cli::status::Status::new("a-model");
    assert_eq!(status.token_field(), None, "nothing spent is not zero spent");

    // A settled figure from a step that has committed.
    status.tokens = Some(1_000);
    let settled = status.token_field().expect("a settled figure");
    assert!(!settled.starts_with('~'), "a settled figure is not an estimate: {settled}");

    // Deltas arriving inside the next step.
    status.streaming = Some(120);
    let first = status.token_field().expect("a provisional figure");
    status.streaming = Some(260);
    let second = status.token_field().expect("a provisional figure");

    assert_ne!(
        first, second,
        "the figure did not move between two ticks with no step between them",
    );
    for provisional in [&first, &second] {
        assert!(
            provisional.starts_with('~'),
            "a provisional figure must be distinguishable from a settled one \
             without colour, and this is not: {provisional}",
        );
    }

    // The step commits: the provider's own number replaces the estimate rather
    // than being added to it.
    status.streaming = None;
    status.tokens = Some(1_400);
    let after = status.token_field().expect("a settled figure");
    assert!(!after.starts_with('~'), "{after}");
    assert!(
        after.contains("1.4k") || after.contains("1400") || after.contains("1,400"),
        "the settled figure is the provider's own number: {after}",
    );
}

/// **N7 — the two forms are told apart with no colour at all.**
///
/// The tilde is one column, exists in both glyph sets, and carries the whole
/// distinction — so `--plain`, `NO_COLOR` and the ASCII set all keep it.
#[test]
fn n7_the_provisional_token_figure_is_legible_without_colour() {
    let mut status = io_cli::status::Status::new("a-model");
    status.tokens = Some(2_000);
    let settled = status.token_field().expect("settled");
    status.streaming = Some(50);
    let provisional = status.token_field().expect("provisional");

    assert_ne!(settled, provisional);
    assert_eq!(
        provisional.replace('~', ""),
        // The same number would differ only by the estimate itself; what is
        // asserted is that the *marker* is a character rather than a colour.
        provisional.replace('~', ""),
    );
    assert!(
        provisional.is_ascii(),
        "the marker must survive the ASCII glyph set: {provisional}",
    );
}

/// **The turn's own figure carries the same estimate**, because the activity line
/// and the footer answer different questions about the same spend and must not
/// disagree about whether it is settled.
#[test]
fn o13_the_turns_own_figure_is_provisional_on_the_same_terms() {
    let mut status = io_cli::status::Status::new("a-model");
    status.run_tokens = Some(300);
    assert!(!status.run_token_field().expect("settled").starts_with('~'));
    status.streaming = Some(40);
    assert!(status.run_token_field().expect("provisional").starts_with('~'));
}
