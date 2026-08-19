//! The status line, and its share of F9: at eighty columns it degrades to a
//! narrow form rather than wrapping.

mod support;

use std::time::Duration;

use io_cli::app::App;
use io_cli::status::{format_elapsed, Status};
use io_cli::theme::{DARK, MONO};
use io_harness::{EventKind, RunEvent};

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

    screen
        .draw(|frame| status.render(frame, frame.area(), &DARK))
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
/// share of the budget io-harness itself declares rather than of a number copied
/// into this repository.
#[test]
fn the_context_field_appears_when_a_fold_reports_one() {
    let mut app = App::new(DARK, "opus-5");
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
        screen
            .draw(|frame| app.status.render(frame, frame.area(), &theme))
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
    for fact in ["tok", "ctx", "seatbelt", "plan"] {
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

    assert!(!line.contains("mcp"), "zero servers is not `mcp 0`: {line:?}");
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
            }),
            Duration::ZERO,
        );
    }

    assert_eq!(app.status.mcp, (1, 2), "one server, two tools");
    assert!(rendered(&app.status, 120).contains("mcp 1/2 tools"));
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
