//! F5 — a queued line reaches the turn that is still running, on both arms.
//!
//! **And F7 since 0.24.0, at the bottom of the file, because it is the same
//! queue.** A failing verification gate buys another turn by putting a prompt at
//! the FRONT of the queue this file is about — not by opening a second driver
//! loop beside the one that already drains it. So the retry's mechanism belongs
//! next to the mechanism it borrows, and the same instrument proves it: the
//! library halves are driven directly, and the driver's wiring is read as text.
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
///
/// **The observer argument is spelled `watcher` from 0.20.0, and the change is
/// the point rather than a rename.** Through 0.19.0 the driver passed `&observer`
/// — the `Bridge` itself, one value, the only observer there was. It now passes
/// `Broadcast` over a `Fanout` over the bridge and the operator's `Hooks`, and
/// the local holding that composition is named for what it is. What this test
/// asserts is unchanged: both arms take the caller's contract **and** the inbox,
/// in that positional order.
#[test]
fn f5_both_arms_are_handed_a_steer_inbox() {
    let text = driver();
    let flat = squashed(&text);

    assert!(
        flat.contains(
            "session.turn_bounded_steered(&contract,provider,store,policy,&approver,watcher,\
             &inbox"
        ),
        "the flat arm takes the contract and the inbox",
    );
    assert!(
        flat.contains(
            "session.turn_contained_bounded_steered(&contract,provider,store,policy,&approver,\
             caps,watcher,&inbox"
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
    // A fold IS offered, by `/compact`, and it is a third thing the operator sends
    // rather than a second thing the stop key does. What this pins is that the two
    // stay separate: the fold rides the inbox because it is a message, the stop
    // rides the flag because it is not, and neither is reachable from the other's
    // key. An earlier draft of this test asserted the fold was absent — written
    // before `/compact` landed in the same release, which is what a gate written
    // against a half-built tree pins.
    assert!(
        text.contains("steer.fold()"),
        "`/compact` sends the fold through the inbox, beside the words `/steer` sends",
    );
    assert!(
        !text.contains("canceller") || !text.contains("steer.fold();\n"),
        "the fold is the operator's third message and never the stop key's second meaning",
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
    // The FIRST WORD, not the whole trimmed line. `/steer do the thing` used to
    // miss this guard and fall to the mid-turn refusal below it, which told the
    // operator to interrupt the turn first — the opposite of what the command
    // does.
    let arm = text
        .split_once("line.split_whitespace().next() == Some(\"steer\")")
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

// ---------------------------------------------------------------------------
// F7 — a failing gate buys another turn, through this same queue
// ---------------------------------------------------------------------------
//
// The retry is not a second driver loop. It is a prompt at the FRONT of the queue
// this file is otherwise about, so a turn earned by a gate and a turn typed by an
// operator go through one `turn` call, get one echo, one clock and one `Ctrl+C`.
// What is provable here is the mechanism — the queue's order, the prompt's
// contents, and the fact that the wiring in the driver exists at all. What is not
// is that a real model reads it, which is the live run's to close.

/// One recorded gate evaluation. `at` is a stored string this crate never parses.
fn attempt(step: u32, phase: &str, outcome: io_harness::GateOutcome) -> io_harness::GateAttempt {
    io_harness::GateAttempt {
        id: 1,
        step,
        phase: phase.into(),
        outcome,
        detail: String::new(),
        at: String::new(),
    }
}

/// F7 — the retry prompt carries the criterion and the failure, and not the goal.
///
/// **A retry that re-sent the original prompt is the sabotage this criterion
/// names**, and it is the one that looks like it works: a second turn runs, the
/// model does more work, and the gate fails again for exactly the reason nobody
/// told it about. `app::gate_retry` has no parameter a goal could arrive through,
/// so that implementation cannot be written — and this asserts the two things it
/// must carry instead.
#[test]
fn f7_the_retry_prompt_carries_the_criterion_and_what_failed() {
    let criterion = io_cli::gates::Criterion::Command {
        argv: vec!["cargo".into(), "test".into()],
        expect_exit: 0,
    };
    let attempts = [attempt(4, "command", io_harness::GateOutcome::Failed)];
    let events = [
        io_harness::SandboxEvent::gate_output(9, 3, "an older failure, two turns ago"),
        io_harness::SandboxEvent::gate_output(9, 4, "error[E0433]: failed to resolve `frobnicate`"),
    ];

    let prompt = io_cli::app::gate_retry(&criterion, &attempts, &events);
    assert!(
        prompt.contains("cargo test"),
        "the retried turn is told what it is judged by, in the words it was judged \
         by: {prompt}",
    );
    assert!(
        prompt.contains("E0433") && prompt.contains("frobnicate"),
        "a retry that does not carry the failure is a retry that repeats it: {prompt}",
    );
    assert!(
        !prompt.contains("two turns ago"),
        "the output is the one for the step this attempt ran after, not the first \
         failure of the run: {prompt}",
    );
    assert!(
        prompt.contains("Do not change the gate"),
        "an agent handed a failing check and no instruction about it will sometimes \
         edit the check",
    );

    // A review says its reasons in the ROW and prints nothing, so a reader that
    // only looked at sandbox events would retry with no explanation at all.
    let reviewed = io_cli::gates::Criterion::Review {
        rubric: "every public item has a doc comment".into(),
        reviewer: "vendor/judge".into(),
        allow_self_review: false,
    };
    let mut refused = attempt(2, "review", io_harness::GateOutcome::Failed);
    refused.detail = "three functions carry no documentation".into();
    let prompt = io_cli::app::gate_retry(&reviewed, &[refused], &[]);
    assert!(
        prompt.contains("three functions carry no documentation"),
        "a review's reasons are in its row, not in a sandbox event: {prompt}",
    );

    // And a gate that failed silently still states the criterion, which is more
    // than the turn had before.
    let quiet = io_cli::app::gate_retry(&criterion, &attempts, &[]);
    assert!(
        quiet.contains("cargo test") && !quiet.is_empty(),
        "a command that printed nothing still has a criterion to state: {quiet}",
    );
}

/// F7 — an existence criterion is judged here, or it is not judged at all.
///
/// **This is the single easiest defect in the release to ship.**
/// `Criterion::verification` maps a bare `file` criterion to `Verification::None`,
/// because there is no honest counterpart for it in io-harness's enum — and a run
/// carrying `Verification::None` never reaches the gate: the step loop returns
/// `Finished` the moment the agent stops calling tools, so the store holds no row
/// at all. A caller that read `Store::gate_attempts` and stopped would draw
/// nothing, retry nothing, and report an ungated run.
///
/// Sabotage: return the recorded rows unchanged. Every command and review gate in
/// the product still works, the suite stays green, and an operator who asked for a
/// file to exist gets a session that never once looks for it.
#[test]
fn f7_an_existence_criterion_is_evaluated_where_io_harness_did_not() {
    let dir = tempfile::tempdir().expect("a temporary directory");
    let criterion = io_cli::gates::Criterion::File {
        file: std::path::PathBuf::from("REPORT.md"),
        contains: None,
    };

    // Nothing recorded, because nothing was asked of io-harness.
    let missing = io_cli::gates::gate_attempts(Vec::new(), Some(&criterion), dir.path());
    let standing = io_cli::gates::standing(&missing)
        .expect("an existence criterion that reports nothing is an ungated run");
    assert_eq!(standing.outcome, io_harness::GateOutcome::Failed);
    assert!(
        io_cli::gates::may_retry(&missing, 1),
        "and a file that is not there earns the turn that writes it",
    );

    std::fs::write(dir.path().join("REPORT.md"), "done").expect("the report");
    let written = io_cli::gates::gate_attempts(Vec::new(), Some(&criterion), dir.path());
    assert_eq!(
        io_cli::gates::standing(&written)
            .expect("a standing")
            .outcome,
        io_harness::GateOutcome::Passed,
    );

    // **A `none` row is dropped, not kept — and the row shape here is the whole
    // point.** io-harness evaluates the contract's criterion after every step on
    // which the agent called a tool, and for `Verification::None` that evaluation
    // is `Ok(false)` — so it records `phase = "none", **Failed**`, once per such
    // step, on every ungated run this product has ever driven. An earlier draft of
    // this test used `Passed`, a row production never emits, and so proved nothing
    // about the case that actually exists: read naively those rows say the gate
    // failed on a session nobody gated.
    std::fs::remove_file(dir.path().join("REPORT.md")).expect("take it away again");
    let folded = io_cli::gates::gate_attempts(
        vec![
            attempt(5, "none", io_harness::GateOutcome::Failed),
            attempt(6, "none", io_harness::GateOutcome::Failed),
            attempt(7, "none", io_harness::GateOutcome::Failed),
        ],
        Some(&criterion),
        dir.path(),
    );
    assert_eq!(
        io_cli::gates::standing(&folded)
            .expect("a standing")
            .outcome,
        io_harness::GateOutcome::Failed,
        "a row for a criterion that checked nothing must not stand as the verdict",
    );
    assert!(
        folded.iter().all(|attempt| attempt.phase != "none"),
        "the row io-harness wrote about checking nothing is not part of the count",
    );

    // Every other criterion is io-harness's to judge, and the rows come back
    // untouched — including the empty list, which is an ungated run.
    let command = io_cli::gates::Criterion::Command {
        argv: vec!["make".into()],
        expect_exit: 0,
    };
    let rows = vec![attempt(1, "command", io_harness::GateOutcome::Passed)];
    assert_eq!(
        io_cli::gates::gate_attempts(rows.clone(), Some(&command), dir.path()),
        rows,
        "a command gate is not re-judged here",
    );
    assert!(io_cli::gates::gate_attempts(Vec::new(), None, dir.path()).is_empty());

    // **The case that would have shipped exit 6 to every operator alive.** A
    // session with no `[app.io-cli.gates]` at all still leaves `none`/`Failed`
    // rows behind for any turn in which the model called a tool. Folded with no
    // criterion they must come to nothing — no standing, no status word, no
    // scrollback line, and in `io exec` no exit 6.
    let ungated = vec![
        attempt(1, "none", io_harness::GateOutcome::Failed),
        attempt(2, "none", io_harness::GateOutcome::Failed),
    ];
    let folded = io_cli::gates::gate_attempts(ungated, None, dir.path());
    assert!(
        folded.is_empty(),
        "an ungated run's bookkeeping rows are not a verdict: {folded:?}",
    );
    assert!(
        io_cli::gates::standing(&folded).is_none(),
        "a run nobody gated has no standing",
    );
    assert_eq!(
        io_cli::exec::verified_code(
            &io_harness::RunOutcome::Finished { steps: 2 },
            io_cli::gates::standing(&folded).as_ref(),
        ),
        io_cli::exec::OK,
        "an ungated headless run exits exactly what it exited before this release",
    );
}

/// F6 — the scrollback line spells the verdict the way everything else does.
#[test]
fn f6_the_gate_line_names_the_phase_the_verdict_and_what_it_printed() {
    let attempts = [
        attempt(3, "command", io_harness::GateOutcome::Failed),
        attempt(4, "command", io_harness::GateOutcome::Failed),
    ];
    let events = [io_harness::SandboxEvent::gate_output(
        9,
        4,
        "2 tests failed",
    )];
    let (tone, line) =
        io_cli::app::gate_report(&attempts, &events).expect("a gated turn has a line");
    // **`Warning`, and `Refused` would be a defect rather than a preference.**
    // `Tone::Refused` renders the literal word `refused`, which is the permission
    // boundary's word — this release moved the failing review off that tone in
    // `src/events.rs` for exactly that reason, and a scrollback line one row below
    // spelling `refused: gate failed` would put the collision straight back.
    // "The policy would not run my gate" and "your work did not meet the bar"
    // need opposite responses from the operator.
    assert_eq!(
        tone,
        io_cli::theme::Tone::Warning,
        "a gate that answered no did not refuse anything: `refused` is the boundary's word",
    );
    assert_ne!(
        tone,
        io_cli::theme::Tone::Refused,
        "the gate's verdict must not borrow the permission boundary's vocabulary",
    );
    assert!(
        line.contains("failed") && line.contains("command") && line.contains("2 tests failed"),
        "the phase, the verdict and what it printed: {line}",
    );
    assert!(
        line.contains("attempt 2"),
        "the attempt is on the line once there has been more than one: {line}",
    );

    // `GateOutcome::as_str` verbatim, because the status line, the exit code and
    // this sentence all have to spell one verdict the same way.
    let (_, passed) = io_cli::app::gate_report(
        &[attempt(1, "contains", io_harness::GateOutcome::Passed)],
        &[],
    )
    .expect("a line");
    assert!(
        passed.contains(io_harness::GateOutcome::Passed.as_str()),
        "the word is the harness's own: {passed}",
    );
    assert!(
        !passed.contains("attempt"),
        "every gate that ran at all ran once, so `attempt 1` tells nobody anything",
    );

    assert!(
        io_cli::app::gate_report(&[], &[]).is_none(),
        "a turn nothing gated earns no line; a session with no gates section would \
         otherwise carry one under every turn",
    );
}

/// F7 — the driver drives the retry through the queue it already drains.
///
/// Nothing under `tests/` links `src/main.rs`, so the wiring is read as text, the
/// way `tests/contract.rs` and `tests/context_share.rs` read it. Four properties,
/// each of which is a way the retry can be wrong while every test above still
/// passes:
///
/// 1. **One loop.** The retry goes back through `next`, so it gets the
///    configuration refresh, the picture drain and the stop key that every other
///    turn gets. A second loop beside this one is where those drift apart.
/// 2. **The budget is asked of the library.** `gates::may_retry` deliberately
///    differs from `GateOutcome::is_retryable` — both `Failed` and `Errored` earn
///    another *turn*, because the tree changes between turns — and a driver that
///    reached for the method would silently stop retrying the commonest failure
///    there is.
/// 3. **A stopped turn is never retried.** One press of the stop key must not
///    start another turn.
/// 4. **The prompt is composed by the library.** A sentence built in this file
///    would be one no test can read.
#[test]
fn f7_the_retry_rides_the_drivers_own_queue() {
    let text = driver();
    let flat = squashed(&text);

    assert!(
        flat.contains("next=app.next_queued_prompt();"),
        "the retry is drained by the loop that was already there",
    );
    assert!(
        flat.contains("app.requeue_prompts(vec![prompt])"),
        "and it goes to the FRONT of that queue, so it is the next turn and the \
         prompts typed during this one keep their order behind it",
    );
    assert!(
        flat.contains("io_cli::gates::may_retry(&gated,section.retries())"),
        "the budget is the library's answer, asked with the operator's own number",
    );
    assert!(
        !text.contains("is_retryable"),
        "`GateOutcome::is_retryable` answers a different question — whether the \
         SAME criterion over the SAME tree could say something else — and using it \
         would stop retrying every gate that merely failed",
    );
    assert!(
        flat.contains("!turned.stopped&&io_cli::gates::may_retry("),
        "a turn the operator interrupted is not retried",
    );
    assert!(
        flat.contains("io_cli::app::gate_retry(criterion,&gated,&events)"),
        "the prompt is composed in the library, where a test can read it",
    );

    // The attempts are accumulated across the chain. Every turn is its own run, so
    // `Store::gate_attempts` restarts at one on each retry — a driver that passed
    // only the current run's rows to `may_retry` would buy another turn for every
    // one of them, forever, against a real model.
    assert!(
        flat.contains("gated.extend(io_cli::gates::gate_attempts("),
        "the chain's attempts accumulate; `retries = 1` means one further turn, \
         not an unbounded loop",
    );
    assert!(
        flat.contains("gated.clear();"),
        "and a chain that ends releases them, or the next prompt is charged for a \
         gate that failed two prompts ago",
    );

    // The standing comes off the STORE. `EventKind::Sandbox` carries no detail
    // payload, so nothing in the event stream can say which phase failed or what
    // it printed — and reading rows is also what makes this right after a
    // `/resume`, where this process never watched the run.
    assert!(
        flat.contains("store.gate_attempts(run_id)")
            && flat.contains("store.sandbox_events(run_id)"),
        "the phase, the verdict and the output are the store's",
    );
    assert!(
        flat.contains("app.status.gate=Some(standing.outcome.as_str().to_string());"),
        "the status word is the harness's own spelling, passed through",
    );
}
