//! F5 — a queued line reaches the turn that is still running, on both arms.
//!
//! **What this file can and cannot prove, stated first, because the gap is the
//! whole reason the file is shaped like this.**
//!
//! A delivered steer emits no observer event. There is no `EventKind::Steered`;
//! io-harness records a `steered` row in the run's own context trace and pushes
//! the operator's words into the ledger as an ordinary `Observation`. So nothing
//! that arrives through `io_cli::bridge` says a message landed, and an
//! implementation that reported success on `Steer::say` returning `Ok` would be
//! indistinguishable, on screen and in this file, from one that works. `Ok` means
//! the channel took the words. It does not mean a step read them.
//!
//! That leaves three things provable here and one that is not:
//!
//! 1. **Both arms are handed an inbox.** Nothing under `tests/` links the binary,
//!    so the driver's decisions are ones no test can drive — this file reads
//!    `src/main.rs` instead, the instrument `tests/contract.rs` and
//!    `tests/structure.rs` already use for exactly that reason. It is also the
//!    assertion that kills the sabotage: keep the two `_observed` entry points
//!    and send into an inbox nothing reads, and only these assertions fail.
//! 2. **The channel's own contract.** Messages arrive in the order they were sent,
//!    an interrupt is carried in the same drain as the words beside it, and a send
//!    into a channel with no reader left is an `Err` rather than a silence.
//! 3. **`/steer` survives the mid-turn refusal.** `App::compose` lets a `/` line
//!    through while a turn holds the session, which is what makes the driver's arm
//!    reachable at all.
//!
//! **Not provable without a network, and owed to the live run (T13):** that the
//! words reach the *model*. A real turn must be steered mid-flight and the run's
//! own trace must then show a `steered` context event, with the message in the
//! run's observations at a step after the one that was in flight. That is the only
//! evidence that closes F5, and it comes from `Store::context_events(run_id)` and
//! the ledger — never from the send.

use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use io_cli::app::{App, Command, Mode};
use io_cli::theme::DARK;
use io_harness::Steer;

fn key(code: KeyCode) -> KeyEvent {
    KeyEvent::new(code, KeyModifiers::NONE)
}

fn type_line(app: &mut App, text: &str) {
    for character in text.chars() {
        app.key(key(KeyCode::Char(character)));
    }
    app.key(key(KeyCode::Enter));
}

fn driver() -> String {
    std::fs::read_to_string(std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("src/main.rs"))
        .expect("the driver")
}

/// Whitespace removed, because rustfmt decides where an eight-argument call
/// breaks and an assertion about where a newline sits is an assertion about
/// formatting — it would go quietly blind the first time one of these grew an
/// argument.
fn squashed(text: &str) -> String {
    text.chars().filter(|c| !c.is_whitespace()).collect()
}

/// F5 — both arms take the caller's contract **and** a steer inbox.
///
/// Through io-harness 0.66 that was a choice: `turn_bounded_observed` and
/// `turn_contained_bounded_observed` had no parameter for an inbox, so a session
/// wanting its own contract gave up steering to get one — which is the trade
/// io-cli made in 0.11.0 and paid for with a turn nobody could correct. 0.67.0
/// opened both, and the `_steered` pair is positionally the same two calls with
/// the inbox appended.
///
/// Sabotage: keep the two `_observed` entry points and send into an inbox nothing
/// reads. Only this test fails — and it is the one failure an operator cannot
/// detect from the screen, because the send still returns `Ok` and no event ever
/// said otherwise.
#[test]
fn f5_both_arms_are_handed_a_steer_inbox() {
    let text = driver();
    let flat = squashed(&text);

    assert!(
        flat.contains(
            "session.turn_bounded_steered(&contract,provider,store,policy,&approver,&observer,\
             &inbox"
        ),
        "the flat arm takes the contract and the inbox",
    );
    assert!(
        flat.contains(
            "session.turn_contained_bounded_steered(&contract,provider,store,policy,&approver,\
             caps,&observer,&inbox"
        ),
        "and so does the contained one, which is the arm that could not have one at all before \
         io-harness 0.67.0",
    );

    // The negative half, and the sabotage's own shape: an entry point that takes
    // no inbox cannot deliver a word, however successful the send looked.
    for deaf in ["turn_bounded_observed(", "turn_contained_bounded_observed("] {
        assert!(
            !text.contains(deaf),
            "{deaf} takes no steer inbox, so a turn driven through it hears nothing",
        );
    }

    // One inbox per turn, because `SteerInbox` is not `Clone` and one turn reads
    // one inbox: two would each get some of the operator's messages and neither
    // would get all of them.
    assert_eq!(
        text.matches("Steer::channel()").count(),
        1,
        "a turn builds exactly one inbox, and both arms are handed that one",
    );
}

/// F5 — `Ctrl+C` did not move onto the steer channel.
///
/// Both arms now hold an inbox, so `Steer::interrupt` became a second way to
/// reach `RunOutcome::Cancelled` at the same step boundary. The stop key stays on
/// the observer's flag anyway: the two paths are recorded by different code in
/// io-harness, an operator cannot tell them apart from the screen, and this is the
/// one key this product refuses to let a configuration file rebind. The inbox
/// carries the operator's words; the flag carries their stop.
///
/// Sabotage: route the interrupt through `Steer::interrupt` — under which this
/// test fails, and `tests/interrupt.rs` fails with it for no gain anybody can see.
#[test]
fn f5_the_stop_key_stays_on_the_observers_flag() {
    let text = driver();
    assert!(
        !text.contains("steer.interrupt()"),
        "`Ctrl+C` is the canceller's, and giving it a second mechanism changes which path records \
         the outcome",
    );
    assert!(
        !text.contains("steer.fold()"),
        "a fold is a third thing an operator can send and this release does not offer it; a call \
         with no key behind it is a path nothing can reach",
    );
}

/// F5 — what is sent is what was queued, and it is sent because the operator
/// asked.
///
/// **The open question this release had to answer.** A line typed mid-turn must
/// not reach the agent by itself. A delivered steer emits no event, so an
/// interface cannot show that the agent heard it — a line sent by default would
/// leave the screen with no echo, no cell and no confirmation, which is the same
/// shape as the keystroke 0.16.0 lost and `App::compose` has just been fixed to
/// stop losing. A queue is visible state a surface can draw; a steer is not. And
/// `Steer::say` has no undo, so an operator writing themselves a note while an
/// agent works would be steering it, once per stray sentence.
///
/// So the queue keeps its promise — three lines are three turns — and `/steer` is
/// the one word that spends it differently.
///
/// Sabotage: send every queued prompt as it is typed. Only this test fails, and
/// it fails on the count: a second `steer.say(` is a second path, and a path
/// nobody asked for is the accident.
#[test]
fn f5_the_queue_is_sent_on_the_operators_word_and_not_before() {
    let text = driver();
    assert_eq!(
        text.matches("steer.say(").count(),
        1,
        "one send site. Two means something else in the driver can steer a turn, and the operator \
         did not ask it to",
    );

    // And that one site reads the queue rather than a line of its own: F5 is
    // about a *queued* line reaching the running turn.
    let arm = text
        .split_once("line.trim() == \"steer\"")
        .expect("the driver answers /steer while a turn is running")
        .1;
    let arm = arm
        .split_once("// Refused with a sentence")
        .expect("the arm sits above the refusal it is an exception to")
        .0;
    assert!(
        arm.contains("app.next_queued_prompt()"),
        "what is sent is what was queued: {arm}",
    );
    assert!(
        arm.contains("steer.say("),
        "and it goes into the inbox the two arms were handed: {arm}",
    );
    // Said, never claimed delivered. Nothing on the way in confirms a message
    // landed, so the sentence may not say it did.
    assert!(
        !arm.contains("delivered"),
        "`Ok` from a send means the channel took the words, not that a step read them",
    );
}

/// F5 — the words are not lost when the turn ends before a step reads them.
///
/// The one delivery fact this interface can state is the negative one, and this
/// is where it is stated: `SteerInbox::pending` is public precisely so a caller
/// can drain an inbox it is no longer handing to a turn.
#[test]
fn f5_the_driver_says_what_the_turn_never_read() {
    let text = driver();
    assert!(
        text.contains("inbox.pending()"),
        "a message still in the inbox when the turn returned is a message nobody read, and the \
         operator is told so rather than discovering it later",
    );
    assert!(
        text.contains("not delivered"),
        "and it is said in those words",
    );
}

/// F5 — `/steer` reaches the driver while a turn holds the session.
///
/// `App::compose` refuses a slash command mid-turn with a sentence *in the
/// driver*, not in the library — the `/` arm deliberately keeps falling through.
/// That fall-through is what makes the driver's arm reachable, and it is the only
/// part of this key path a test can link against.
#[test]
fn f5_a_slash_command_still_reaches_the_driver_mid_turn() {
    let mut app = App::new(DARK, "anthropic/claude-sonnet-4");

    // A prompt starts the turn.
    type_line(&mut app, "refactor the parser");
    app.started();
    assert_eq!(app.mode(), Mode::Running);

    // A plain line typed while it runs is held, which is the thing `/steer`
    // spends. (`tests/queue.rs` owns the queue's own behaviour; this is only the
    // premise — without something waiting there is nothing to send.)
    type_line(&mut app, "actually, prefer the smaller diff");
    assert_eq!(app.queued_prompts().len(), 1);
    assert_eq!(
        app.queued_prompts()[0],
        "actually, prefer the smaller diff",
        "the line the operator wants steered is waiting",
    );

    // And the word that spends it is not swallowed by the library: the driver's
    // arm is the only place that answers it, and it can only answer what reaches
    // it.
    for character in "/steer".chars() {
        app.key(key(KeyCode::Char(character)));
    }
    assert_eq!(
        app.key(key(KeyCode::Enter)),
        Command::Slash("steer".into()),
        "a slash command still reaches the driver while a turn holds the session",
    );
    // And it did not run itself on the way past: the queue is spent in the
    // driver, where the inbox is.
    assert_eq!(app.queued_prompts().len(), 1);
}

/// F5 — the channel delivers the operator's words in the order they were said.
///
/// One `Observation` per message, in order, because that is what the model reads.
/// Joined into one they would be a paragraph the operator never wrote.
#[test]
fn f5_messages_arrive_in_the_order_they_were_sent() {
    let (steer, inbox) = Steer::channel();
    for said in ["first", "second", "third"] {
        steer.say(said).expect("the inbox is alive");
    }

    let steering = inbox.pending();
    assert_eq!(steering.messages, ["first", "second", "third"]);
    assert!(!steering.interrupted, "saying something is not stopping");
    assert!(!steering.fold, "and it is not a fold");

    // Drained is drained: what a turn has read, a later read does not see again.
    assert!(inbox.pending().messages.is_empty());
}

/// F5 — an interrupt is carried alongside the words sent before it, and the drain
/// answers it first.
///
/// The precedence is io-harness's — `drain_steer` returns `RunOutcome::Cancelled`
/// before it looks at the fold or pushes a message — and this asserts the half a
/// caller can observe: both reach the same drain, so the trace holds both. An
/// operator who typed a correction and then hit stop sent both, and neither is
/// swallowed on the way.
#[test]
fn f5_an_interrupt_rides_the_same_drain_as_the_words_before_it() {
    let (steer, inbox) = Steer::channel();
    steer.say("try the other approach").expect("alive");
    steer.fold().expect("alive");
    steer.interrupt().expect("alive");

    let steering = inbox.pending();
    assert!(steering.interrupted, "the stop reached the same boundary");
    assert_eq!(
        steering.messages,
        ["try the other approach"],
        "and it did not swallow what was said before it",
    );
    assert!(steering.fold, "nor the fold, which the interrupt outranks");
}

/// F5 — a send with nobody left to read it is an error, not a silence.
///
/// The failure this whole file is arranged around is a send that reports success
/// while the agent never sees a word of it. io-harness refuses to be that: when
/// the inbox is gone, `say` says so.
#[test]
fn f5_a_send_after_the_turn_has_ended_is_refused() {
    let (steer, inbox) = Steer::channel();
    steer.say("in time").expect("the turn is still listening");
    drop(inbox);

    let error = steer
        .say("too late")
        .expect_err("a message nobody will read must not report success");
    let said = error.to_string();
    assert!(
        !said.is_empty() && said.contains("ended"),
        "the operator is told their correction went nowhere: {said}",
    );
    assert!(
        steer.interrupt().is_err() && steer.fold().is_err(),
        "and the other two go the same way",
    );
}
