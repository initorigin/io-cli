//! F6 — an interrupt stops the turn and keeps the session.
//!
//! Asserted at the seam rather than against a live provider: `Ctrl+C` during a
//! running turn must stop it, the partial output must be committed rather than
//! lost with the turn, and the composer must take the next prompt. A real
//! interrupted turn against a real model is F1's job.
//!
//! **The mechanism is half of F6 and it is not the steer inbox.** Since 0.17.0
//! both turn arms hold a `SteerInbox`, so `Steer::interrupt` exists and would
//! reach the same `RunOutcome::Cancelled` at the same step boundary. The stop key
//! stays on the observer's flag — `Flow::Cancel` out of `io_cli::bridge` — and
//! that is asserted here rather than remembered, because it is a decision
//! `src/main.rs` makes and nothing under `tests/` links the binary. This sentence
//! used to read "must reach `Steer::interrupt`", which was the opposite of what
//! the driver has ever done.

mod support;

use std::collections::BTreeMap;

use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use io_cli::app::{App, Command, Mode};
use io_cli::keys::{Action, Chord, Hit, Keys};
use io_cli::theme::DARK;
use io_harness::{EventKind, Plan, PlanStep, Question, RunEvent, Steer};

fn key(code: KeyCode) -> KeyEvent {
    KeyEvent::new(code, KeyModifiers::NONE)
}

fn control(character: char) -> KeyEvent {
    KeyEvent::new(KeyCode::Char(character), KeyModifiers::CONTROL)
}

fn type_text(app: &mut App, text: &str) {
    for character in text.chars() {
        app.key(key(KeyCode::Char(character)));
    }
}

fn text_of(lines: &[ratatui::text::Line<'_>]) -> String {
    lines
        .iter()
        .flat_map(|line| line.spans.iter())
        .map(|span| span.content.as_ref())
        .collect::<Vec<_>>()
        .join("")
}

#[test]
fn f6_ctrl_c_during_a_turn_interrupts_it_and_keeps_the_partial_output() {
    let (steer, inbox) = Steer::channel();
    let mut app = App::new(DARK, "anthropic/claude-sonnet-4");

    // A prompt starts a turn.
    type_text(&mut app, "refactor the parser");
    assert_eq!(
        app.key(key(KeyCode::Enter)),
        Command::Submit("refactor the parser".into()),
    );
    app.started();
    assert_eq!(app.mode(), Mode::Running);

    // The model streams some of an answer.
    for word in ["Looking ", "at ", "the ", "parser"] {
        app.event(
            &RunEvent::new(1, 1, EventKind::Token { text: word.into() }),
            std::time::Duration::ZERO,
        );
    }
    assert_eq!(app.events.live(), "Looking at the parser");

    // Ctrl+C.
    assert_eq!(app.key(control('c')), Command::Interrupt);

    // **Sent by hand, and that is the point.** `Command::Interrupt` is the whole
    // of what the key produces; the driver turns it into the observer's flag, and
    // `f6_the_stop_key_reaches_the_run_as_flow_cancel_and_not_the_steer_inbox`
    // below is what asserts that. What this half shows is the other path — the one
    // that exists, ends in the same outcome, and is deliberately not wired to any
    // key: nothing but this test line can put an interrupt in the inbox.
    steer.interrupt().expect("the turn is still listening");

    // io-harness 0.69.0 replaced the `(Vec<String>, bool)` tuple with `Steering`,
    // which is `#[non_exhaustive]` so the fourth thing an operator can send costs
    // this caller nothing.
    let steering = inbox.pending();
    assert!(
        steering.interrupted,
        "Steer::interrupt never reached the inbox"
    );
    assert!(
        steering.messages.is_empty(),
        "an interrupt is not a message"
    );
    assert!(!steering.fold, "an interrupt is not a fold");

    // The turn ends. The partial output is committed rather than lost with it.
    app.finished();
    let committed = text_of(&app.take_pending());
    assert!(
        committed.contains("Looking at the parser"),
        "the partial answer was lost when the turn was interrupted: {committed:?}",
    );
    assert_eq!(app.mode(), Mode::Idle);
    assert_eq!(app.events.live(), "");

    // And the composer takes the next prompt.
    type_text(&mut app, "try again");
    assert_eq!(
        app.key(key(KeyCode::Enter)),
        Command::Submit("try again".into()),
        "the session did not survive the interrupt",
    );
}

#[test]
fn f6_ctrl_c_twice_at_an_idle_composer_exits() {
    let mut app = App::new(DARK, "m");
    assert_eq!(app.key(control('c')), Command::None, "the first one warns");
    // In the footer since 0.13.1: it answers the key just pressed and is gone at
    // the next one, rather than living in the scrollback for the session's life.
    let warning = app
        .status
        .notice
        .as_ref()
        .map(|(_, text)| text.clone())
        .unwrap_or_default();
    assert!(
        warning.contains("again"),
        "the first Ctrl+C should say what the second one does: {warning:?}",
    );
    assert_eq!(app.key(control('c')), Command::Exit);
}

#[test]
fn ctrl_c_with_something_typed_clears_it_rather_than_exiting() {
    let mut app = App::new(DARK, "m");
    type_text(&mut app, "half a thought");

    assert_eq!(app.key(control('c')), Command::None);
    assert!(
        app.composer.is_empty(),
        "the composer should have been cleared"
    );
    // ...and that did not count towards the exit, so a stray Ctrl+C cannot end
    // the session by surprise.
    assert_eq!(app.key(control('c')), Command::None);
    assert_eq!(app.key(control('c')), Command::Exit);
}

#[test]
fn ctrl_d_exits_only_on_an_empty_composer() {
    let mut app = App::new(DARK, "m");
    type_text(&mut app, "not finished yet");
    assert_eq!(
        app.key(control('d')),
        Command::None,
        "Ctrl+D must not discard what was typed",
    );

    app.composer.clear();
    assert_eq!(app.key(control('d')), Command::Exit);
}

#[test]
fn ctrl_l_clears_the_viewport_and_says_so() {
    let mut app = App::new(DARK, "m");
    assert_eq!(app.key(control('l')), Command::ClearViewport);
}

#[test]
fn a_prompt_beginning_with_a_slash_is_a_command() {
    let mut app = App::new(DARK, "m");
    type_text(&mut app, "/theme");
    assert_eq!(app.key(key(KeyCode::Enter)), Command::Slash("theme".into()));

    type_text(&mut app, "/model gpt-5");
    assert_eq!(
        app.key(key(KeyCode::Enter)),
        Command::Slash("model gpt-5".into()),
    );
}

#[test]
fn the_observer_hands_events_on_without_blocking_the_run() {
    use io_harness::Observer;

    let (bridge, mut receiver) = io_cli::bridge::channel();
    for step in 0..1000 {
        let flow = bridge.event(&RunEvent::new(1, step, EventKind::Stalled));
        assert_eq!(
            flow,
            io_harness::Flow::Continue,
            "the observer must not cancel the run it is watching",
        );
    }

    let mut seen = 0;
    while receiver.try_recv().is_ok() {
        seen += 1;
    }
    assert_eq!(
        seen, 1000,
        "an event was dropped between the run and the screen"
    );
}

#[test]
fn a_dropped_interface_does_not_cancel_the_run() {
    use io_harness::Observer;

    let (bridge, receiver) = io_cli::bridge::channel();
    drop(receiver);
    assert_eq!(
        bridge.event(&RunEvent::new(1, 1, EventKind::Stalled)),
        io_harness::Flow::Continue,
        "a send failure must not stop the agent loop",
    );
}

#[test]
fn f9_a_streaming_answer_of_any_length_leaves_the_viewport_the_same_size() {
    // The viewport is a fixed few rows and stays that way however much streams,
    // because each line commits to the terminal's own scrollback as it finishes.
    // Only the unfinished tail is ever live, so there is nothing for the viewport
    // to grow to hold.
    // Asked through `viewport_wanted`, which is the demand the driver acts on.
    // Since 0.32.0 the viewport does grow — for a question, a plan, a queue or a
    // picker — so "it does not move" has to be asserted against the number that
    // can now move, rather than against a constant that never could.
    let mut app = App::new(DARK, "m");
    let quiet = app.viewport_wanted(80, 40);
    assert!(quiet >= 3, "streaming tail, composer and status line");

    let mut committed = 0;
    for index in 0..200 {
        committed += app_lines(&mut app, &format!("line {index} of an answer\n"));
    }
    assert_eq!(
        committed, 200,
        "every finished line should have been committed as it arrived",
    );
    assert_eq!(
        app.viewport_wanted(80, 40),
        quiet,
        "the viewport did not move"
    );

    // A partial line stays live until something finishes it.
    app_lines(&mut app, "the tail with no newline yet");
    assert_eq!(app.events.live(), "the tail with no newline yet");
    assert_eq!(
        app.viewport_wanted(80, 40),
        quiet,
        "a live tail is not a surface asking for rows",
    );

    app.finished();
    assert_eq!(app.events.live(), "");
    let tail = text_of(&app.take_pending());
    assert!(tail.contains("the tail with no newline yet"), "{tail:?}");
}

/// A turn that has done a step's worth of work, so it is past [`App::undoable`]
/// and takes the ordinary two-press stop.
///
/// `status.steps` rather than a streamed token, because what makes a turn
/// undoable is the step count and the rows on screen — see `App::undoable` — and
/// a test that streamed text to move one of them would be asserting the other by
/// accident.
fn working() -> App {
    let mut app = App::new(DARK, "m");
    app.started();
    app.status.steps = Some(1);
    assert!(
        !app.undoable(),
        "a turn with a step behind it is not undone"
    );
    app
}

/// **F6 — the first press on a turn that has done nothing takes it back whole.**
///
/// The branch above the two-press stop, and the only one that answers with
/// `Command::Abandon` on the first key: no boundary to wait for and nothing
/// streamed to keep, so the driver puts the prompt back in the composer.
///
/// Sabotage: drop the `undoable` arm from `App::interrupt_or_quit` — under which
/// this fails and the operator who pressed the key a moment after Enter waits for
/// a step boundary that has nothing in it.
#[test]
fn f6_the_first_press_takes_back_a_turn_that_has_done_nothing() {
    let mut app = App::new(DARK, "m");
    type_text(&mut app, "sent by accident");
    assert_eq!(
        app.key(key(KeyCode::Enter)),
        Command::Submit("sent by accident".into()),
    );
    app.started();

    assert!(
        app.undoable(),
        "nothing has streamed and no step has finished"
    );
    assert_eq!(
        app.key(control('c')),
        Command::Abandon,
        "a turn with nothing in it is undone rather than stopped at a boundary",
    );
}

/// **F6 — pressed again while it is stopping, it abandons; and `Esc` is the same
/// key.**
///
/// Once asks, twice takes. The first press says where the turn will stop and
/// cancels through the observer, which io-harness honours at the next step
/// boundary; the second does not wait for one. `Esc` is a second interrupt while
/// a turn runs, so the two presses need not be the same chord — an operator who
/// read the sentence the first press printed presses the key it names.
///
/// Sabotage: make the second press return `Command::Interrupt` again — under
/// which this fails, and it fails as a key that reads as ignored for as long as a
/// slow tool call takes.
#[test]
fn f6_a_second_press_while_stopping_abandons_and_esc_is_that_second_press() {
    let mut app = working();
    assert_eq!(app.key(control('c')), Command::Interrupt);
    let said = app
        .status
        .notice
        .as_ref()
        .map(|(_, text)| text.clone())
        .unwrap_or_default();
    assert!(
        said.contains("stopping") && said.contains("again"),
        "the first press says what is happening and what a second one does: {said:?}",
    );
    assert_eq!(
        app.key(control('c')),
        Command::Abandon,
        "the second press does not wait for a step boundary",
    );

    // And the same two presses spelled the other way, which is how the sentence
    // above tells an operator to spell them.
    let mut app = working();
    assert_eq!(app.key(key(KeyCode::Esc)), Command::Interrupt);
    assert_eq!(app.key(key(KeyCode::Esc)), Command::Abandon);

    // Mixed, because neither key is a mode: the state that decides the second
    // press is the turn's, not the chord's.
    let mut app = working();
    assert_eq!(app.key(control('c')), Command::Interrupt);
    assert_eq!(app.key(key(KeyCode::Esc)), Command::Abandon);
}

/// **F6 — the stop key pre-empts an approval.**
///
/// An open question owns the keyboard: the run is stopped waiting on an answer
/// and no other key reaches the composer. `Ctrl+C` is the exception, and it has
/// to be — an approval is exactly where an operator decides they want out, and a
/// surface that answered the stop key with "that is not one of the three answers"
/// would be the lock this product refuses to build.
///
/// Sabotage: drop `.filter(|_| !interrupting)` from the approval guard in
/// `App::key` — under which this fails, and it fails with the key swallowed by an
/// overlay that returns `Command::None`.
#[tokio::test]
async fn f6_the_stop_key_pre_empts_an_approval() {
    use io_harness::{Act, ApprovalContext, Approver, Request};

    let (asker, mut asks) = io_cli::approval::channel();
    let deciding = tokio::spawn(async move {
        let asker = asker;
        asker
            .decide_in_context(
                &Request::new(Act::Write, "src/main.rs").with_content("fn main() {}\n"),
                &ApprovalContext::new("tidy the parser"),
            )
            .await
    });
    let ask = asks
        .recv()
        .await
        .expect("the question reached the interface");

    let mut app = working();
    app.open_approval(ask);
    assert!(app.modal(), "an approval owns the keyboard while it is up");

    assert_eq!(
        app.key(control('c')),
        Command::Interrupt,
        "the stop key interrupts rather than answering the question",
    );

    // The question is denied as a consequence of the turn ending, which is the
    // driver's doing — dropping the overlay drops the `Ask`, and a dropped `Ask`
    // is a denial by construction. Asserted here only so far as this file can:
    // the key produced the interrupt and not an answer.
    drop(app);
    let decision = deciding.await.expect("the approver did not panic");
    assert!(
        matches!(decision, io_harness::Decision::Deny { .. }),
        "an abandoned question denies rather than approving on the way out: {decision:?}",
    );
}

/// **F6 — the stop key pre-empts a question about intent.**
///
/// Same terms as the approval, one surface down: every other key is the operator
/// typing prose, which is why an unanswered question must not be able to hold
/// the one key that ends the turn it belongs to.
///
/// Sabotage: drop `.filter(|_| !interrupting)` from the intent guard — under
/// which this fails, and the operator is left typing an answer to a turn they
/// asked to stop.
#[test]
fn f6_the_stop_key_pre_empts_a_question_about_intent() {
    let (answer, _reply) = tokio::sync::oneshot::channel();
    let mut app = working();
    app.open_intent(io_cli::intent::Asked {
        question: Question::new("drop the column or keep it?"),
        answer,
    });
    assert!(app.asking(), "the overlay owns the keyboard while it is up");

    assert_eq!(
        app.key(control('c')),
        Command::Interrupt,
        "the stop key interrupts rather than becoming the first letter of an answer",
    );
}

/// **F6 — the stop key pre-empts a plan gate.**
///
/// The plan gate is the surface with the strongest claim on the keyboard: the
/// planning phase denies every write and every exec until somebody decides, so a
/// session waiting at it is a session that can do nothing else. That is precisely
/// why the stop key has to reach through it.
///
/// Sabotage: drop `.filter(|_| !interrupting)` from the plan guard — under which
/// this fails, and the one surface that stops the whole agent becomes the one
/// that can also refuse to let go of it.
#[test]
fn f6_the_stop_key_pre_empts_a_plan_gate() {
    let (verdict, _reply) = tokio::sync::oneshot::channel();
    let mut app = working();
    app.open_plan(io_cli::plan::Proposed {
        plan: Plan::new([PlanStep::new("write the migration")]),
        verdict,
    });
    assert!(app.modal(), "a plan owns the keyboard while it is up");

    assert_eq!(
        app.key(control('c')),
        Command::Interrupt,
        "the stop key interrupts rather than approving, correcting or cancelling the plan",
    );
}

/// **F6 — the queue surface does not take the stop key, on either press.**
///
/// `tests/queue_surface.rs` asserts the first press reaches the turn and that the
/// surface is not modal. This is the half it does not cover: the surface stays up
/// through the stop — nothing about it is a turn-scoped acknowledgement — the
/// *second* press still abandons, and neither press is a queue command. A stop
/// that silently emptied the queue would be one key doing two jobs, which is the
/// thing 0.17.0 exists to avoid.
///
/// Sabotage: give the queue surface the keyboard the way the fleet view has it,
/// or close it on `Command::Interrupt` — under which this fails on the press the
/// existing test does not make.
#[test]
fn f6_the_queue_surface_takes_neither_press() {
    let mut app = working();
    app.queue_prompt("run the tests after this");
    assert!(
        app.queue_open(),
        "a prompt queued mid-turn opens the surface"
    );

    assert_eq!(app.key(control('c')), Command::Interrupt);
    assert!(
        app.queue_open(),
        "the stop key is not the surface's dismissal — `Esc` is, and it is the key \
         the surface answers first",
    );
    assert_eq!(
        app.key(control('c')),
        Command::Abandon,
        "the second press reaches the turn with the surface still drawn over the composer",
    );
    assert_eq!(
        app.queued_prompts().len(),
        1,
        "stopping a turn is not forgetting what was queued behind it; `/queue clear` is \
         the key that does that",
    );
}

/// **F6 — the chord cannot be rebound in either spelling, and a file that tried
/// still has a working stop key.**
///
/// Both spellings: naming `interrupt` in `[app.io-cli.keys]`, and putting some
/// *other* action on `ctrl+c`. `tests/keys.rs` asserts the refusal and that the
/// key still exits an idle prompt. What is asserted here is the half F6 is about
/// — that a session started from such a file can still stop a *running turn* —
/// because a refusal that left the binding half-applied would look identical at
/// an idle composer.
///
/// Sabotage: honour the second spelling, `clear = "ctrl+c"`, on the grounds that
/// it does not name the immovable action — under which this fails on the arm the
/// first spelling never reaches.
#[test]
fn f6_the_stop_key_cannot_be_rebound_in_either_spelling() {
    // The invariant `App::key` computes `interrupting` from, stated once: the
    // default binding is the control chord and it fires rather than arming.
    assert_eq!(Action::Interrupt.default_binding(), "ctrl+c");
    assert!(!Action::Interrupt.rebindable());
    assert_eq!(
        Keys::default().hit(Chord::of(control('c')), None),
        Some(Hit::Fire(Action::Interrupt)),
    );

    for asking in [("interrupt", "ctrl+x"), ("clear", "ctrl+c")] {
        let configured: BTreeMap<String, String> =
            [(asking.0.to_string(), asking.1.to_string())].into();
        let (keys, notices) = Keys::resolve(Some(&configured));
        assert!(
            notices.join("\n").contains("Ctrl+C is not rebindable"),
            "`{} = \"{}\"` must be refused out loud",
            asking.0,
            asking.1,
        );

        let mut app = working();
        app.set_keys(keys);
        assert_eq!(
            app.key(control('c')),
            Command::Interrupt,
            "`{} = \"{}\"` took the stop key away from a running turn",
            asking.0,
            asking.1,
        );
        assert_eq!(app.key(control('c')), Command::Abandon);
    }
}

/// `src/main.rs`, with every comment taken off before anything is matched, and
/// then every space.
///
/// The stripping is the difference between a gate and a green light: `src/main.rs`
/// names `Steer::interrupt` in a comment explaining why it is *not* used, so a
/// gate that read the raw file would fail on the prose that documents the
/// decision it is asserting. `//` appears in no string literal in that file and it
/// has no block comments — `tests/structure.rs` rests on the same reading. The
/// whitespace goes because rustfmt decides where a line breaks.
fn driver_squashed() -> String {
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("src/main.rs");
    let text = std::fs::read_to_string(path).expect("the driver is readable");
    text.lines()
        .map(|line| match line.find("//") {
            Some(at) => &line[..at],
            None => line,
        })
        .collect::<Vec<_>>()
        .join("\n")
        .chars()
        .filter(|c| !c.is_whitespace())
        .collect()
}

/// **F6, the mechanism.** The stop key reaches the run as `Flow::Cancel`, and not
/// through the steer inbox.
///
/// This is a property of `src/main.rs`, which nothing under `tests/` links, so it
/// is read out of the source the way `tests/contract.rs` and `tests/structure.rs`
/// read the driver's other decisions. It is asserted in two halves, because
/// either one alone is satisfiable by the sabotage:
///
/// 1. **The positive.** Both `Command::Interrupt` and `Command::Abandon` set the
///    canceller taken from the observer, and that flag is what `io_cli::bridge`
///    turns into `Flow::Cancel` — asserted here against the real bridge rather
///    than assumed from the driver's variable name.
/// 2. **The negative.** The driver calls nothing's `interrupt`. `tests/steer.rs`
///    greps for the absence of `steer.interrupt()`; the absence on its own would
///    still pass with the arms rewritten to send a `Steering` some other way, so
///    the positive above is what makes this a gate.
///
/// Sabotage: route the interrupt through `Steer::interrupt`. Only F6 fails —
/// the key still stops the turn, at the same boundary, with the same outcome, so
/// nothing on screen and no other test moves; what changes is which code in
/// io-harness records the outcome, and that every sentence this product has
/// written about its stop key becomes wrong.
#[test]
fn f6_the_stop_key_reaches_the_run_as_flow_cancel_and_not_the_steer_inbox() {
    use io_harness::{Flow, Observer};

    let driver = driver_squashed();

    // (1) The flag exists, and it is the observer's rather than a bool of the
    // driver's own — a local `stopped` proves nothing about the run.
    assert!(
        driver.contains("letcanceller=observer.canceller();"),
        "the turn's stop is the observer's flag, taken from the bridge that watches it",
    );

    // Both arms set it. The second one is the one a refactor loses: it looks like
    // the drop below it does the work, and dropping the future leaves io-harness
    // with a run it never closed.
    for arm in [
        "Command::Interrupt=>{canceller.store(true,std::sync::atomic::Ordering::Relaxed);\
         stopped=true;}",
        "Command::Abandon=>{canceller.store(true,std::sync::atomic::Ordering::Relaxed);\
         stopped=true;",
    ] {
        assert!(
            driver.contains(arm),
            "the stop key sets the canceller and nothing else: {arm}",
        );
    }

    // (2) The negative: no `interrupt` is called on anything, which is the one
    // shape the sabotage takes. The comments are already gone, and the comment
    // that explains this decision is the reason they had to be.
    assert!(
        !driver.contains(".interrupt("),
        "`Steer::interrupt` reaches the same outcome by different code in io-harness; the \
         stop key is the canceller's and moving it buys an operator nothing they can see",
    );
    assert!(
        !driver.contains("Steer::interrupt"),
        "not through a path built by hand either",
    );

    // And the flag is `Flow::Cancel`, against the real bridge: the two halves of
    // the mechanism, joined, so the driver's variable name is not what this rests
    // on.
    let (observer, _events) = io_cli::bridge::channel();
    let canceller = observer.canceller();
    let event = RunEvent::new(1, 1, EventKind::Stalled);
    assert_eq!(observer.event(&event), Flow::Continue);
    canceller.store(true, std::sync::atomic::Ordering::Relaxed);
    assert_eq!(
        observer.event(&event),
        Flow::Cancel,
        "the flag the stop key sets is the one io-harness honours at the next step boundary",
    );
}

/// Feed one token and report how many lines it committed.
fn app_lines(app: &mut App, text: &str) -> usize {
    let before = app.take_pending().len();
    assert_eq!(before, 0, "the caller should have drained first");
    app.event(
        &RunEvent::new(1, 1, EventKind::Token { text: text.into() }),
        std::time::Duration::ZERO,
    );
    app.take_pending().len()
}
