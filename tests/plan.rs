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
