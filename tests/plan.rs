//! **F4** — a plan is read, approved, sent back or cancelled before any of it
//! runs.
//!
//! The claim that matters is the negative one: a cancelled plan leaves the
//! workspace untouched. That is not asserted by looking at the workspace — it is
//! guaranteed by *when* the gate is called. io-harness denies every write and
//! every exec under a `plan-gate` layer for as long as the planning phase is on,
//! and the phase ends only on `Approve`. So what these tests assert is the
//! verdict the run received, which is the thing that decides all of it.

mod support;

use std::sync::Arc;

use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use io_cli::app::App;
use io_cli::theme::DARK;
use io_harness::{Plan, PlanGate, PlanStep, PlanVerdict};

fn key(code: KeyCode) -> KeyEvent {
    KeyEvent::new(code, KeyModifiers::NONE)
}

/// **F3 — `/plan` says which it is and never guesses.**
///
/// The same shape `/contain` has, and for the same reason: turning the planning
/// phase on stops every turn until somebody approves a proposal, so a bare
/// `/plan` that toggled would be a coin flip between an agent that works and one
/// that waits.
///
/// Sabotage: make the bare form `Action::Plan(Some(true))` — under which only
/// this test fails, and it fails by switching a mode the operator was asking
/// about.
#[test]
fn f3_plan_parses_as_a_question_or_an_answer() {
    use io_cli::commands::{self, Action};

    let keys = io_cli::keys::Keys::default();
    assert_eq!(commands::parse("plan", &keys, &DARK), Action::Plan(None));
    assert_eq!(
        commands::parse("plan on", &keys, &DARK),
        Action::Plan(Some(true))
    );
    assert_eq!(
        commands::parse("plan off", &keys, &DARK),
        Action::Plan(Some(false))
    );
    // The words `/contain` already accepts, because an operator who learned them
    // one command over should not have to learn them twice.
    assert_eq!(
        commands::parse("plan yes", &keys, &DARK),
        Action::Plan(Some(true))
    );
    assert_eq!(
        commands::parse("plan no", &keys, &DARK),
        Action::Plan(Some(false))
    );
}

/// **F3 — the command is listed where an operator looks for it.**
///
/// A command that works and is not in `COMMANDS` is not reachable from the
/// palette, which is 0.9.0's lesson about `/exit`: advertised and inert is the
/// one failure a listed command must not have, and unlisted and working is the
/// same failure from the other side.
#[test]
fn f3_plan_is_listed() {
    let listed = io_cli::commands::COMMANDS
        .iter()
        .find(|(name, _)| *name == "/plan");
    let (_, blurb) = listed.expect("`/plan` is in the command table");
    assert!(
        blurb.contains("on") && blurb.contains("off"),
        "the blurb says the switch has both directions: {blurb}",
    );
}

/// **F2 — containment no longer decides whether a turn plans.**
///
/// A source gate, for the reason `tests/contract.rs` names: `src/main.rs` is a
/// binary and nothing under `tests/` can link it. The value half of F2 is in
/// `tests/contract.rs`, which reads `plan_gate` off a built contract, and the
/// live half is `tests/live.rs`, which asserts on the events of a real run.
///
/// Sabotage: pass the gate on `containment.is_some()` again — under which only
/// this test fails, and it fails by restoring the coupling that made every
/// contained turn stop for a plan.
#[test]
fn f2_the_gate_follows_the_operator_and_not_the_caps() {
    let driver = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("src/main.rs");
    let text = std::fs::read_to_string(driver).expect("the driver");

    let call = text
        .split_once("let contract = io_cli::contract::session(")
        .expect("one contract is built for every turn")
        .1;
    let args = &call[..call.find(");").expect("the call closes")];

    assert!(
        args.contains("planning"),
        "the gate argument is the operator's switch: {args:?}",
    );
    assert!(
        !args.contains("containment"),
        "and it is not the caps: {args:?}",
    );
}

fn typed(app: &mut App, text: &str) {
    for ch in text.chars() {
        app.key(key(KeyCode::Char(ch)));
    }
}

fn said(app: &mut App) -> String {
    app.take_pending()
        .iter()
        .flat_map(|line| line.spans.iter())
        .map(|span| span.content.to_string())
        .collect::<Vec<_>>()
        .join(" ")
}

fn app() -> App {
    App::new(DARK, "test-model")
}

fn plan() -> Plan {
    Plan::new([
        PlanStep::new("read every caller of the old column"),
        PlanStep::new("write the migration"),
    ])
}

/// The gate, the plan it is asked about, and the join handle the verdict comes
/// back through — the run's side of the seam, stopped until the overlay answers.
async fn proposed(app: &mut App) -> tokio::task::JoinHandle<Option<PlanVerdict>> {
    let (gate, mut plans) = io_cli::plan::channel();
    let gate: Arc<dyn PlanGate> = Arc::new(gate);
    let reviewing = tokio::spawn(async move { gate.review(&plan()).await });
    app.open_plan(plans.recv().await.expect("the plan reaches the ui"));
    reviewing
}

/// **F4 — an empty prompt approves.** There is nothing to say about a plan you
/// agree with, and a keystroke meaning "yes, and I have nothing to add" is one
/// nobody would find.
#[tokio::test]
async fn enter_on_an_empty_prompt_approves_the_plan() {
    let mut app = app();
    let reviewing = proposed(&mut app).await;

    assert!(app.asking(), "the overlay owns the keyboard");
    app.key(key(KeyCode::Enter));

    assert_eq!(
        reviewing.await.expect("the gate"),
        Some(PlanVerdict::Approve)
    );
    assert!(said(&mut app).contains("approved"));
}

/// **F4 — a correction goes back as the operator's own words**, and the run stays
/// in its planning phase, still writing nothing.
///
/// Sabotage: send `PlanVerdict::Approve` whenever the prompt has text, under
/// which only this test fails — and it fails by running the plan the operator
/// was in the middle of objecting to.
#[tokio::test]
async fn text_and_enter_sends_the_plan_back_with_that_correction() {
    let mut app = app();
    let reviewing = proposed(&mut app).await;

    typed(&mut app, "start with the tests");
    app.key(key(KeyCode::Enter));

    assert_eq!(
        reviewing.await.expect("the gate"),
        Some(PlanVerdict::revise("start with the tests")),
    );
    assert!(said(&mut app).contains("start with the tests"));
}

/// **F4 — cancel is one key that means nothing else**, and it ends the turn with
/// no step executed. `RunOutcome::PlanRejected` is io-harness's own consequence
/// of this verdict; what io-cli must get right is sending it.
///
/// Sabotage: treat cancel as an approval with an empty correction, under which
/// only this test fails — and it fails by running a plan a person just refused.
#[tokio::test]
async fn esc_cancels_and_nothing_runs() {
    let mut app = app();
    let reviewing = proposed(&mut app).await;

    app.key(key(KeyCode::Esc));

    assert_eq!(
        reviewing.await.expect("the gate"),
        Some(PlanVerdict::Cancel)
    );
    let said = said(&mut app);
    assert!(said.contains("cancelled"), "{said}");
    assert!(said.contains("nothing ran"), "and it says so: {said}");
    assert!(!app.asking(), "the overlay is gone");
}

/// A turn that ended while a plan was up leaves no decision behind. Dropping the
/// sender is `None`, which pauses rather than approves — the safe direction
/// twice over, since the run this belonged to has already stopped.
#[tokio::test]
async fn a_turn_that_ends_first_decides_nothing() {
    let mut app = app();
    let reviewing = proposed(&mut app).await;

    app.finished();

    assert_eq!(reviewing.await.expect("the gate"), None);
    assert!(!app.asking());
}

/// The steps are drawn as steps, numbered and in order, with the three ways out
/// named rather than left to folklore.
#[tokio::test]
async fn the_steps_are_shown_as_steps_and_the_keys_are_named() {
    let mut app = app();
    let reviewing = proposed(&mut app).await;

    let (mut screen, _recorder) = support::screen_of(80, 14, 14);
    screen
        .draw(|frame| app.render(frame, frame.area()))
        .expect("a frame");
    let drawn = screen.viewport_text().to_string();

    assert!(drawn.contains("1. read every caller"), "{drawn}");
    assert!(
        drawn.contains("2. write the migration"),
        "in order: {drawn}"
    );
    assert!(drawn.contains("Enter approves"), "{drawn}");
    assert!(drawn.contains("Esc cancels"), "{drawn}");

    app.key(key(KeyCode::Esc));
    let _ = reviewing.await;
}

/// **Non-functional — no new state is signalled by colour alone.** Under
/// `NO_COLOR` the tones carry nothing at all, so a plan overlay that distinguished
/// approve from cancel by tone would be a decision surface a reader could not use.
/// Every one of the three ways out is a word.
#[tokio::test]
async fn the_plan_is_readable_with_no_colour_at_all() {
    let mut app = App::new(io_cli::theme::MONO, "test-model");
    let (gate, mut plans) = io_cli::plan::channel();
    let gate: Arc<dyn PlanGate> = Arc::new(gate);
    let reviewing = tokio::spawn(async move { gate.review(&plan()).await });
    app.open_plan(plans.recv().await.expect("the plan reaches the ui"));

    let (mut screen, _recorder) = support::screen_of(80, 14, 14);
    screen
        .draw(|frame| app.render(frame, frame.area()))
        .expect("a frame");
    let drawn = screen.viewport_text().to_string();

    assert!(drawn.contains("read every caller"), "{drawn}");
    assert!(drawn.contains("Enter approves"), "{drawn}");
    assert!(drawn.contains("Esc cancels"), "{drawn}");

    app.key(key(KeyCode::Esc));
    assert_eq!(
        reviewing.await.expect("the gate"),
        Some(PlanVerdict::Cancel)
    );
    assert!(
        said(&mut app).contains("cancelled"),
        "and the outcome is a word, not a colour",
    );
}
