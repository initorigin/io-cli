//! The live rehearsal for F1.
//!
//! Every test here is `#[ignore]`d, so `cargo test` never runs one and CI — which
//! holds no secrets — never tries. Run them by hand with a key in the
//! environment:
//!
//! ```text
//! export OPENROUTER_API_KEY=sk-or-...
//! cargo test --test live -- --ignored --nocapture
//! ```
//!
//! **These do not satisfy F1 and are not meant to.** F1 is a person at a real
//! terminal, taking a first run from an empty configuration through the wizard to
//! a verified edit, interrupting a second turn mid-flight, and then searching and
//! selecting the transcript with the terminal's own facilities afterwards. None
//! of that has a tty here, and the recording of it is the release evidence.
//!
//! What these do is prove the halves of F1 that a terminal is not needed for, so
//! that the manual run is not the first time any of it has been exercised: the
//! credential check against a real endpoint, the catalogue read, and a real turn
//! driven through the real bridge and the real event renderer, editing a real
//! file inside the sandbox.

mod support;

use std::sync::{Arc, Mutex};

use io_cli::events::Events;
use io_cli::settings::Posture;
use io_cli::theme::DARK;
use io_cli::verify;
use io_harness::{
    DenyAll, EventKind, Flow, Observer, Policy, ProviderSpec, RunEvent, Session, Steer, Store,
};

fn key() -> String {
    std::env::var("OPENROUTER_API_KEY")
        .ok()
        .filter(|key| !key.is_empty())
        .expect("set OPENROUTER_API_KEY to run the live rehearsal")
}

fn model() -> String {
    std::env::var("OPENROUTER_MODEL").unwrap_or_else(|_| "anthropic/claude-sonnet-4".into())
}

fn spec(api_key: Option<String>) -> ProviderSpec {
    ProviderSpec::OpenRouter {
        model: model(),
        api_key,
    }
}

fn workspace_policy() -> Policy {
    Policy {
        layers: Policy::default().layers,
        defaults: Posture::Workspace.defaults(),
    }
}

#[tokio::test]
#[ignore = "live: needs OPENROUTER_API_KEY"]
async fn live_a_real_key_is_accepted() {
    verify::credential(&spec(Some(key())))
        .await
        .expect("the wizard's verification call should succeed with a real key");
}

#[tokio::test]
#[ignore = "live: needs OPENROUTER_API_KEY"]
async fn live_f3_a_wrong_key_is_rejected_in_the_providers_own_words() {
    let message = verify::credential(&spec(Some("sk-or-v1-definitely-not-a-key".into())))
        .await
        .expect_err("a bad key must not pass verification");

    // F3's real point: the message is the provider's, not ours. A generic
    // "verification failed" would leave the user with nothing to act on.
    println!("the provider said: {message}");
    assert!(
        !message.is_empty(),
        "a rejection has to carry something to read",
    );
    assert!(
        !message.contains("verification failed"),
        "the message should be the provider's own, not a category of ours: {message}",
    );
}

#[tokio::test]
#[ignore = "live: needs OPENROUTER_API_KEY"]
async fn live_the_catalogue_has_models_in_it() {
    let models = verify::catalogue(&spec(Some(key()))).await;
    println!("{} models in the catalogue", models.len());
    assert!(
        models.len() > 10,
        "the wizard's model step needs a catalogue to offer",
    );
    assert!(
        models.iter().any(|id| id.contains('/')),
        "OpenRouter ids are vendor-prefixed: {:?}",
        &models[..models.len().min(5)],
    );
}

/// Collects events the way the interface does, so what is asserted is what a
/// session would have rendered.
struct Collector {
    events: Arc<Mutex<Vec<RunEvent>>>,
}

impl Observer for Collector {
    fn event(&self, event: &RunEvent) -> Flow {
        self.events
            .lock()
            .expect("not poisoned")
            .push(event.clone());
        Flow::Continue
    }
}

#[tokio::test]
#[ignore = "live: needs OPENROUTER_API_KEY"]
async fn live_f1_a_real_turn_streams_and_edits_a_file() {
    let key = key();
    let dir = tempfile::tempdir().expect("a workspace");
    let root = dir.path();
    std::fs::write(root.join("greeting.txt"), "hello\n").expect("the fixture file");

    let store = Store::open(root.join("runs.db")).expect("a store");
    let mut session = Session::open(&store, root).expect("a session");
    let provider = io_harness::OpenRouter::new(&key, model());
    let policy = workspace_policy();
    let approver = DenyAll;
    let (_steer, inbox) = Steer::channel();

    let collected = Arc::new(Mutex::new(Vec::new()));
    let observer = Collector {
        events: Arc::clone(&collected),
    };

    let result = session
        .turn_steered(
            "Edit greeting.txt so that it contains exactly the word: goodbye. \
             Then tell me in one sentence what you changed.",
            &provider,
            &store,
            &policy,
            &approver,
            &observer,
            &inbox,
        )
        .await
        .expect("the turn runs");

    println!("outcome: {:?}", result.outcome);

    let events = collected.lock().expect("not poisoned").clone();
    println!("{} events", events.len());

    // The renderer sees the same events the interface would, and turns them into
    // lines. This is the path F1 exercises through a terminal.
    let mut renderer = Events::new(DARK);
    let mut committed = Vec::new();
    for event in &events {
        committed.extend(renderer.event(event, std::time::Duration::ZERO));
    }
    committed.extend(renderer.flush());
    let transcript: String = committed
        .iter()
        .map(|line| {
            line.spans
                .iter()
                .map(|span| span.content.as_ref())
                .collect::<String>()
        })
        .collect::<Vec<_>>()
        .join("\n");
    println!("--- as the interface would have rendered it ---\n{transcript}\n---");

    // **Not asserted: that the model streamed any assistant text.** The prompt
    // asks for a sentence, and this test used to require a `Token` event on the
    // strength of that. It is the model's choice whether to answer in prose or to
    // spend the whole turn in tool calls, and on 2026-08-18 the model behind
    // `OPENROUTER_MODEL` chose the latter — so the assertion failed while every
    // durable fact about the run was correct. That is the rule this repository
    // already wrote down once and had to learn twice: a live assertion rests on
    // what reached the store, never on what the model decided to say. What is
    // asserted instead is below — a tool call happened, the renderer produced
    // lines, and the file on disk changed.
    assert!(
        events
            .iter()
            .any(|event| matches!(event.kind, EventKind::ToolCall { .. })),
        "the agent should have used a tool to edit the file",
    );
    assert!(
        !committed.is_empty(),
        "the renderer produced no lines from a real run",
    );

    let after = std::fs::read_to_string(root.join("greeting.txt")).expect("the file survives");
    println!("greeting.txt is now {after:?}");
    assert!(
        after.to_lowercase().contains("goodbye"),
        "the file was not edited: {after:?}",
    );
}

/// The live rehearsal for **F10**, and it does not satisfy it.
///
/// F10 is a person at a real terminal watching a run stop to ask, and saying
/// whether that moment reads as clear or as alarming. What this proves is
/// everything about it a terminal is not needed for, so that the manual run is
/// not the first time any of it has been exercised: that a real model, working on
/// a real file under the *ask before writes* posture, actually reaches the
/// approver at all; that the question carries the act, the target, the rule and
/// the layer the overlay draws from; and that answering it lets the run go on to
/// finish the edit.
///
/// It is the difference between the owner's one run failing because the overlay
/// reads badly — which is the answer F10 is for — and failing because nothing
/// appeared, which would be a waste of their time.
#[tokio::test]
#[ignore = "live: needs OPENROUTER_API_KEY"]
async fn live_f10_a_real_turn_stops_and_asks_before_it_writes() {
    use io_cli::approval::{self, Answer, Approval};

    let key = key();
    let dir = tempfile::tempdir().expect("a workspace");
    let root = dir.path();
    std::fs::write(root.join("greeting.txt"), "hello\n").expect("the fixture file");

    let store = Store::open(root.join("runs.db")).expect("a store");
    let mut session = Session::open(&store, root).expect("a session");
    let provider = io_harness::OpenRouter::new(&key, model());

    // The posture this release exists for, and the one that declined everything
    // through 0.1.0 and 0.1.1.
    let policy = Policy {
        layers: Policy::default().layers,
        defaults: Posture::AskWrites.defaults(),
    };

    let (approver, mut asks) = approval::channel();
    let (_steer, inbox) = Steer::channel();
    let collected = Arc::new(Mutex::new(Vec::new()));
    let observer = Collector {
        events: Arc::clone(&collected),
    };

    // The interface's half, driven by a task rather than by a keyboard. Every
    // question is rendered the way the overlay renders it and then answered
    // `allow once`, which is what the owner will do with `y`.
    let seen = Arc::new(Mutex::new(Vec::new()));
    let answering = tokio::spawn({
        let seen = Arc::clone(&seen);
        // Owned, because the task outlives this borrow of the tempdir.
        let root = root.to_path_buf();
        async move {
            while let Some(ask) = asks.recv().await {
                let approval = Approval::new(ask, &root);
                let question = approval.ask();
                let note = format!(
                    "{} {} — rule {:?}, layer {:?}, {} bytes of content",
                    io_cli::approval::act_word(question.act()),
                    question.target(),
                    question.rule(),
                    question.layer(),
                    question.content().map(str::len).unwrap_or(0),
                );
                println!("the overlay would have asked: {note}");
                seen.lock().expect("not poisoned").push(note);
                approval.answer(Answer::Once);
            }
        }
    });

    let result = session
        .turn_steered(
            "Edit greeting.txt so that it contains exactly the word: goodbye. \
             Then tell me in one sentence what you changed.",
            &provider,
            &store,
            &policy,
            &approver,
            &observer,
            &inbox,
        )
        .await
        .expect("the turn runs");

    println!("outcome: {:?}", result.outcome);
    drop(approver);
    answering.await.expect("the answering task did not panic");

    let asked = seen.lock().expect("not poisoned").clone();
    assert!(
        !asked.is_empty(),
        "under ask-before-writes a run that edits a file must reach the approver; \
         nothing did, which means the overlay would never have opened",
    );

    // And the answer has to have been acted on. A question that is asked and then
    // ignored looks identical on screen to one that worked.
    let after = std::fs::read_to_string(root.join("greeting.txt")).expect("the file survives");
    println!("greeting.txt is now {after:?}");
    assert!(
        after.to_lowercase().contains("goodbye"),
        "the write was approved and did not happen: {after:?}",
    );

    // The harness records the decision as well, which is the half of `one line per
    // decision` that lives outside this process.
    let events = collected.lock().expect("not poisoned").clone();
    assert!(
        events
            .iter()
            .any(|event| matches!(event.kind, EventKind::ApprovalRequested { .. })),
        "the run should have emitted ApprovalRequested for the transcript",
    );
}

/// The other direction, and the one that is easy to get wrong: a denial must stop
/// the write and leave the file alone. An approver that is consulted but whose
/// `no` is not enforced is worse than no approver at all.
#[tokio::test]
#[ignore = "live: needs OPENROUTER_API_KEY"]
async fn live_f10_a_denial_leaves_the_file_alone() {
    use io_cli::approval::{self, Answer, Approval};

    let key = key();
    let dir = tempfile::tempdir().expect("a workspace");
    let root = dir.path();
    std::fs::write(root.join("greeting.txt"), "hello\n").expect("the fixture file");

    let store = Store::open(root.join("runs.db")).expect("a store");
    let mut session = Session::open(&store, root).expect("a session");
    let provider = io_harness::OpenRouter::new(&key, model());
    let policy = Policy {
        layers: Policy::default().layers,
        defaults: Posture::AskWrites.defaults(),
    };

    let (approver, mut asks) = approval::channel();
    let (_steer, inbox) = Steer::channel();
    let collected = Arc::new(Mutex::new(Vec::new()));
    let observer = Collector {
        events: Arc::clone(&collected),
    };

    let answering = tokio::spawn({
        let root = root.to_path_buf();
        async move {
            let mut denied = 0usize;
            while let Some(ask) = asks.recv().await {
                Approval::new(ask, &root).answer(Answer::Deny);
                denied += 1;
            }
            denied
        }
    });

    let result = session
        .turn_steered(
            "Edit greeting.txt so that it contains exactly the word: goodbye. \
             Then tell me in one sentence what you changed.",
            &provider,
            &store,
            &policy,
            &approver,
            &observer,
            &inbox,
        )
        .await
        .expect("the turn runs");

    println!("outcome: {:?}", result.outcome);
    drop(approver);
    let denied = answering.await.expect("the answering task did not panic");
    println!("{denied} question(s) denied");

    let after = std::fs::read_to_string(root.join("greeting.txt")).expect("the file survives");
    println!("greeting.txt is still {after:?}");
    assert_eq!(
        after, "hello\n",
        "a denied write happened anyway, which is the worst failure this surface has",
    );

    let events = collected.lock().expect("not poisoned").clone();
    let mut renderer = Events::new(DARK);
    let transcript: String = events
        .iter()
        .flat_map(|event| renderer.event(event, std::time::Duration::ZERO))
        .map(|line| {
            line.spans
                .iter()
                .map(|span| span.content.as_ref())
                .collect::<String>()
        })
        .collect::<Vec<_>>()
        .join("\n");
    println!("--- as the interface would have rendered it ---\n{transcript}\n---");
}

/// What *allow for the rest of this session* does inside the turn it is given in.
///
/// F5 asserts the half that outlives the turn — the policy the next turn is handed
/// — because that is the half io-cli owns. This asserts the other half, which
/// io-harness owns: `Decision::Approve { remember }` installs a run-scoped layer,
/// so a second attempt at the same target should not ask again.
///
/// It matters because an agent does not write a file once. It tries `write_file`,
/// then `edit_file`, then `patch_file`, and under *allow once* each of those is
/// its own question. If `a` did not collapse them, the answer would be a button
/// that does nothing visible and the operator would learn to stop pressing it.
#[tokio::test]
#[ignore = "live: needs OPENROUTER_API_KEY"]
async fn live_allowing_for_the_session_stops_the_reasking_inside_one_turn() {
    use io_cli::approval::{self, Answer, Approval};

    let key = key();
    let dir = tempfile::tempdir().expect("a workspace");
    let root = dir.path();
    std::fs::write(root.join("greeting.txt"), "hello\n").expect("the fixture file");

    let store = Store::open(root.join("runs.db")).expect("a store");
    let mut session = Session::open(&store, root).expect("a session");
    let provider = io_harness::OpenRouter::new(&key, model());
    let policy = Policy {
        layers: Policy::default().layers,
        defaults: Posture::AskWrites.defaults(),
    };

    let (approver, mut asks) = approval::channel();
    let (_steer, inbox) = Steer::channel();
    let collected = Arc::new(Mutex::new(Vec::new()));
    let observer = Collector {
        events: Arc::clone(&collected),
    };

    let answering = tokio::spawn({
        let root = root.to_path_buf();
        async move {
            let mut targets = Vec::new();
            while let Some(ask) = asks.recv().await {
                let approval = Approval::new(ask, &root);
                targets.push(approval.ask().target().to_string());
                println!("asked about {}", approval.ask().target());
                approval.answer(Answer::Session);
            }
            targets
        }
    });

    let result = session
        .turn_steered(
            "Edit greeting.txt so that it contains exactly the word: goodbye. \
             Then tell me in one sentence what you changed.",
            &provider,
            &store,
            &policy,
            &approver,
            &observer,
            &inbox,
        )
        .await
        .expect("the turn runs");

    println!("outcome: {:?}", result.outcome);
    drop(approver);
    let targets = answering.await.expect("the answering task did not panic");
    println!("{} question(s): {targets:?}", targets.len());

    let after = std::fs::read_to_string(root.join("greeting.txt")).expect("the file survives");
    assert!(
        after.to_lowercase().contains("goodbye"),
        "the write was allowed and did not happen: {after:?}",
    );

    let repeats = targets
        .iter()
        .filter(|target| *target == &targets[0])
        .count();
    assert_eq!(
        repeats, 1,
        "answering `allow this session` did not stop the re-asking for {:?} — it was \
         asked {repeats} times in one turn, so the answer is a key that does nothing \
         a reader can see: {targets:?}",
        targets[0],
    );
}

// ===========================================================================
// F12 — a real turn against a live provider edits a file, and every surface
// this release ships is exercised on it.
//
// This replaced the human gate the first three releases each ended at. The
// owner withdrew manual release testing on 2026-08-17; see
// `.ultraship/iterations/US-IO-CLI-0.3.0-I01.yaml`. What a gate proved and this
// cannot is whether a diff is *pleasant* to read, and the release record says so
// rather than implying otherwise.
//
// What it does prove is the thing no assertion over a hand-written hunk can:
// that the hunk io-harness ACTUALLY stores for a real edit, by a real model,
// through the real tool layer, is the one this renderer draws.
// ===========================================================================

/// The whole cell, as a reader would see it.
fn as_text(lines: &[ratatui::text::Line<'static>]) -> String {
    lines
        .iter()
        .map(|line| {
            line.spans
                .iter()
                .map(|span| span.content.as_ref())
                .collect::<String>()
        })
        .collect::<Vec<_>>()
        .join("\n")
}

#[tokio::test]
#[ignore = "live: needs OPENROUTER_API_KEY"]
async fn live_f12_a_real_edit_renders_as_the_hunk_the_harness_stored() {
    let key = key();
    let dir = tempfile::tempdir().expect("a workspace");
    let root = dir.path();
    // Several lines, so a one-line edit produces a hunk with real context and
    // real `@@` numbers rather than a whole-file replacement that would look the
    // same however it was computed.
    std::fs::write(
        root.join("greeting.rs"),
        "fn one() {}\nfn two() {}\nfn three() {}\nfn four() {}\nfn five() {}\n\
         fn six() {}\nfn seven() {}\nfn eight() {}\n",
    )
    .expect("the fixture file");

    let store = Store::open(root.join("runs.db")).expect("a store");
    let mut session = Session::open(&store, root).expect("a session");
    let provider = io_harness::OpenRouter::new(&key, model());
    let policy = workspace_policy();
    let (_steer, inbox) = Steer::channel();
    let collected = Arc::new(Mutex::new(Vec::new()));
    let observer = Collector {
        events: Arc::clone(&collected),
    };

    let result = session
        .turn_steered(
            "In greeting.rs, change only the body of `fn five` so that it reads \
             `fn five() { println!(\"five\"); }`. Change nothing else. Then say done.",
            &provider,
            &store,
            &policy,
            &DenyAll,
            &observer,
            &inbox,
        )
        .await
        .expect("the turn runs");

    println!("outcome: {:?}  run {}", result.outcome, result.run_id);

    // ---- F1/F2/F3/F4: the diff, from the store, for a real edit -------------
    let edits = store.edits(result.run_id).expect("the edits are readable");
    println!("{} edit(s) recorded", edits.len());
    assert!(
        !edits.is_empty(),
        "the model did not edit the file, so this proves nothing about the diff. \
         outcome was {:?}",
        result.outcome,
    );

    for edit in &edits {
        let text = as_text(&io_cli::diff::cell(edit, &DARK, 120));
        println!("--- step {} · {} ---\n{text}\n---", edit.step, edit.tool);

        assert!(
            text.contains(&edit.path),
            "the path is on the header: {text}"
        );
        match &edit.hunk {
            Some(hunk) => {
                // The load-bearing assertion of the whole release: what is on
                // screen is the harness's stored text, not something io-cli
                // recomputed. Every body line survives, in order.
                for line in hunk.lines() {
                    assert!(
                        text.contains(line),
                        "the rendered cell dropped a line the store holds: {line:?}",
                    );
                }
                assert!(
                    text.contains("@@"),
                    "a stored hunk carries the file's own line numbers: {text}",
                );
            }
            // Honest about the other case rather than failing on it: an absent
            // hunk has three causes and none of them is "nothing changed".
            None => assert!(
                text.contains("no diff stored"),
                "an absent hunk has to say so: {text}",
            ),
        }
    }

    // ---- F11: the whole run as one patch, over OSC 52 -----------------------
    let patch = store.patch(result.run_id).expect("the patch is readable");
    assert!(
        !patch.trim().is_empty(),
        "a run that edited a file has a patch"
    );
    let sequence = io_cli::clipboard::sequence(&patch);
    assert!(sequence.starts_with("\x1b]52;c;"), "OSC 52 is malformed");
    assert!(sequence.ends_with('\x07'), "OSC 52 is unterminated");
    let described = io_cli::clipboard::describe(&patch);
    println!("clipboard: {described}");
    for claim in ["copied", "success", "done"] {
        assert!(
            !described.to_lowercase().contains(claim),
            "nothing acknowledges an OSC 52 write, so {claim:?} is a claim this \
             product cannot support: {described}",
        );
    }

    // ---- F10: the conversation, back into the scrollback --------------------
    let transcript = session.transcript(&store).expect("the transcript reads");
    let rendered = as_text(&io_cli::transcript::lines(&transcript, &DARK));
    println!("--- Ctrl+T ---\n{rendered}\n---");
    assert!(
        rendered.contains("greeting.rs") || rendered.contains("five"),
        "the transcript should carry what was asked: {rendered}",
    );

    // ---- F5: the expander reads the detail back out of the trace ------------
    let steps = store.steps(result.run_id).expect("the steps read");
    println!("{} step(s) in the trace", steps.len());
    assert!(
        !steps.is_empty(),
        "a run that edited a file recorded steps, and /expand reads them back",
    );

    // ---- the tool cells, through the real renderer --------------------------
    let events = collected.lock().expect("not poisoned").clone();
    let mut renderer = Events::new(DARK);
    let mut committed = Vec::new();
    for (nth, event) in events.iter().enumerate() {
        // A distinct age per event, stated by the test rather than measured, so
        // each cell's duration is arithmetic this file chose. No clock is read
        // here or anywhere under `tests/`.
        committed.extend(renderer.event(event, std::time::Duration::from_millis(nth as u64 * 100)));
    }
    committed.extend(renderer.flush());
    let text = as_text(&committed);
    println!("--- as the interface renders it ---\n{text}\n---");

    let announced = events
        .iter()
        .filter(|event| matches!(event.kind, EventKind::ToolCall { .. }))
        .count();
    println!("{announced} tool call(s) announced");
    if announced > 0 {
        assert!(
            text.contains("~"),
            "a committed tool cell carries the interface's observed duration, \
             marked as an observation: {text}",
        );
    }
}

#[tokio::test]
#[ignore = "live: needs OPENROUTER_API_KEY"]
async fn live_f12_an_approval_shows_the_write_as_a_diff_against_the_real_file() {
    use io_cli::approval::{self, Approval};

    let key = key();
    let dir = tempfile::tempdir().expect("a workspace");
    let root = dir.path();
    let target = root.join("notes.txt");
    std::fs::write(&target, "one\ntwo\nthree\nfour\nfive\n").expect("the fixture");

    let store = Store::open(root.join("runs.db")).expect("a store");
    let mut session = Session::open(&store, root).expect("a session");
    let provider = io_harness::OpenRouter::new(&key, model());
    // The posture whose whole point is that it asks.
    let policy = Policy {
        layers: Policy::default().layers,
        defaults: Posture::AskWrites.defaults(),
    };
    let (_steer, inbox) = Steer::channel();
    let (asker, mut asks) = approval::channel();

    // Answer whatever is asked, and render the overlay for the first write on the
    // way past. The run stays paused for exactly as long as the `Ask` is held.
    let drawn = Arc::new(Mutex::new(Vec::<String>::new()));
    let seen = Arc::clone(&drawn);
    let answering = tokio::spawn({
        let root = root.to_path_buf();
        async move {
            while let Some(ask) = asks.recv().await {
                let approval = Approval::new(ask, &root);
                // A viewport with room, so the hunk itself is exercised and not only
                // the counts row a four-row session would leave.
                let (mut screen, _recorder) = support::screen_of(100, 20, 12);
                screen
                    .draw(|frame| approval.render(frame, frame.area(), &DARK))
                    .expect("frame");
                seen.lock()
                    .expect("not poisoned")
                    .push(screen.viewport_text().to_string());
                approval.answer(approval::Answer::Once);
            }
        }
    });

    let result = session
        .turn_steered(
            "Change the third line of notes.txt from `three` to `THREE`. \
             Change nothing else. Then say done.",
            &provider,
            &store,
            &policy,
            &asker,
            &io_harness::Ignore,
            &inbox,
        )
        .await
        .expect("the turn runs");
    drop(asker);
    answering.await.expect("the answering task did not panic");

    println!("outcome: {:?}", result.outcome);
    let overlays = drawn.lock().expect("not poisoned").clone();
    for overlay in &overlays {
        println!("--- approval overlay ---\n{overlay}\n---");
    }
    assert!(
        !overlays.is_empty(),
        "the ask-before-writes posture did not ask, so the approval diff was \
         never exercised. outcome was {:?}",
        result.outcome,
    );

    // The counts are the row that always survives, and they are the decision:
    // a write that touches one line is not the same decision as one that
    // rewrites the file.
    let any = overlays.join("\n");
    assert!(
        any.contains('+') && any.contains('-'),
        "the overlay showed no change at all: {any}",
    );
    assert!(
        !any.contains("+one") || any.contains("-one"),
        "an unchanged line was drawn as an addition, which means the old side \
         was empty and the write reads as a whole-file rewrite: {any}",
    );
}

#[tokio::test]
#[ignore = "live: needs OPENROUTER_API_KEY"]
async fn live_f6_a_switched_model_inherits_the_conversation() {
    // F6's real content, isolated from the fork. `/model` claims the context is
    // not lost, and the only honest way to check that is against a real request.
    //
    // **Asserted on what the model was SHOWN, not on what it chose to say.** The
    // first version of this test asserted the switched model's reply contained
    // `alpha`, and it failed against two different alternates — one answered
    // "What would you like to do next?", another returned nothing at all — then
    // passed against the first of them on a later run, which answered
    // "[agent] alpha". That looked exactly like the switch dropping the context.
    //
    // It was not. Two separate provider instances of the SAME model pass the same
    // check, so a fresh provider carries the conversation; the variable was the
    // alternate model's willingness to answer a question ABOUT the conversation
    // rather than about the workspace. Isolate before believing a failure.
    //
    // So the assertion is on `Store::observations` — the durable record of what
    // actually went to the provider, since io-harness seeds each prior turn as an
    // `[operator]`/`[agent]` observation. That is the fact F6 claims, and it does
    // not become false when a vendor retunes a small model. The reply is printed
    // and deliberately not asserted.
    let key = key();
    let dir = tempfile::tempdir().expect("a workspace");
    let root = dir.path();
    let store = Store::open(root.join("runs.db")).expect("a store");
    let mut session = Session::open(&store, root).expect("a session");
    let policy = workspace_policy();
    let (_steer, inbox) = Steer::channel();

    let first = io_harness::OpenRouter::new(&key, model());
    let said = session
        .turn_steered(
            "Reply with exactly the word alpha and nothing else.",
            &first,
            &store,
            &policy,
            &DenyAll,
            &io_harness::Ignore,
            &inbox,
        )
        .await
        .expect("the alpha turn runs");
    println!("turn 1 ({}) outcome: {:?}", model(), said.outcome);

    let alt =
        std::env::var("OPENROUTER_MODEL_ALT").unwrap_or_else(|_| "openai/gpt-4o-mini".to_string());
    let switched = io_harness::OpenRouter::new(&key, &alt);
    let asked = session
        .turn_steered(
            "Which single word did you reply with a moment ago? Answer with that word only.",
            &switched,
            &store,
            &policy,
            &DenyAll,
            &io_harness::Ignore,
            &inbox,
        )
        .await
        .expect("the switched turn runs");
    println!("turn 2 ({alt}) outcome: {:?}", asked.outcome);
    for turn in session.history(&store).expect("history") {
        println!(
            "  {:?} -> {:?}",
            turn.prompt,
            turn.reply.as_deref().unwrap_or("<no reply>")
        );
    }

    let shown = shown_to(&store, asked.run_id);
    println!("what the switched model was shown:\n{shown}");
    assert!(
        shown.contains("alpha"),
        "the conversation did not reach the switched provider, so the context did \
         not survive the switch. it was shown: {shown:?}",
    );
    assert!(
        shown.contains("Reply with exactly the word alpha"),
        "the operator's own earlier prompt is part of the conversation and has to \
         reach the new model too: {shown:?}",
    );
}

/// Everything a run's model was shown, from the durable trace.
///
/// io-harness seeds each prior turn of the conversation into the run's ledger as
/// an `[operator]`/`[agent]` observation, so this is the conversation as the model
/// received it — which is the only model-independent way to assert that context
/// reached a provider.
fn shown_to(store: &Store, run_id: i64) -> String {
    store
        .observations(run_id)
        .expect("the run's observations")
        .iter()
        .map(|obs| obs.text.as_str())
        .collect::<Vec<_>>()
        .join("\n")
}

#[tokio::test]
#[ignore = "live: needs OPENROUTER_API_KEY"]
async fn live_f13_work_survives_the_session() {
    // F13. All four of this release's surfaces, driven off real turns against a
    // real provider, in the order an operator meets them: an edit, an undo, a
    // fork, a model switch, and a session left and reopened.
    //
    // Every offline fixture in this release builds its own store. 0.3.0 shipped a
    // feature that passed four hand-written tests and was dead in every real
    // session, because the fixtures all shared a shape the real input did not
    // have. This is the arm that would have caught it.
    let key = key();
    let dir = tempfile::tempdir().expect("a workspace");
    let root = dir.path();
    let target = root.join("notes.txt");
    std::fs::write(&target, "one\ntwo\nthree\n").expect("the fixture");
    let before = std::fs::read_to_string(&target).expect("read the fixture");

    let store = Store::open(root.join("runs.db")).expect("a store");
    let mut session = Session::open(&store, root).expect("a session");
    let session_id = session.id();
    let provider = io_harness::OpenRouter::new(&key, model());
    let policy = workspace_policy();
    let (_steer, inbox) = Steer::channel();

    // ---- a real edit ---------------------------------------------------------
    let first = session
        .turn_steered(
            "Change the third line of notes.txt from `three` to `THREE`. \
             Change nothing else. Then say done.",
            &provider,
            &store,
            &policy,
            &DenyAll,
            &io_harness::Ignore,
            &inbox,
        )
        .await
        .expect("the first turn runs");
    println!("turn 1 outcome: {:?}", first.outcome);
    assert_ne!(
        before,
        std::fs::read_to_string(&target).expect("read after the edit"),
        "the turn did not edit the file, so nothing below is exercised. \
         outcome was {:?}",
        first.outcome,
    );

    // ---- Esc Esc: the undo puts the workspace back ---------------------------
    let about = io_cli::rewind::preview(&session, &store).expect("a turn to undo");
    println!(
        "armed: {}",
        io_cli::rewind::armed_line(&about, &DARK.glyphs)
    );
    let undone = io_cli::rewind::last_turn(&mut session, &store)
        .expect("the rewind runs")
        .expect("there was a turn to undo");
    for (tone, line) in io_cli::rewind::undone_lines(&undone, &DARK.glyphs) {
        println!("  [{tone:?}] {line}");
    }
    assert_eq!(
        std::fs::read_to_string(&target).expect("read after the undo"),
        before,
        "the rewind did not put the file back to what it was before the turn",
    );
    // That was the session's only turn, which is the case `branch_from` cannot
    // express and the one nobody tries by hand.
    assert_eq!(
        session.head(),
        None,
        "undoing the only turn must leave the conversation with no head at all",
    );
    assert!(
        session.history(&store).expect("history").is_empty(),
        "the conversation should be back to having said nothing",
    );
    assert_eq!(
        store.rewinds(about.run_id).expect("rewinds").len(),
        1,
        "the undo must be in the durable trace, not merely performed",
    );

    // ---- two turns, so there is a conversation to fork -----------------------
    for prompt in [
        "Reply with exactly the word alpha and nothing else.",
        "Reply with exactly the word beta and nothing else.",
    ] {
        session
            .turn_steered(
                prompt,
                &provider,
                &store,
                &policy,
                &DenyAll,
                &io_harness::Ignore,
                &inbox,
            )
            .await
            .expect("the turn runs");
        if prompt.contains("alpha") {
            // Captured before the beta turn moves the head.
            println!("alpha turn is {:?}", session.head());
        }
    }
    let path = session.history(&store).expect("history");
    assert_eq!(path.len(), 2, "two turns on the path before the fork");
    let alpha = path[0].id;
    // The picker's own rows, so the live arm exercises what an operator reads and
    // not only the call underneath it.
    for row in io_cli::sessions::turn_rows(&path, 80, &DARK.glyphs) {
        println!("  fork row: {} — {:?}", row.label, row.detail);
    }

    // ---- /fork: back to alpha, with beta left readable but off the path ------
    session
        .branch_from(&store, alpha)
        .expect("branching from the alpha turn");
    assert_eq!(
        session.history(&store).expect("history").len(),
        1,
        "the fork must move the head back to alpha",
    );
    // Three, not two: the rewound edit turn is here as well. `rewind_run` puts
    // files, memory and the head back and deletes NOTHING from the trace — the
    // harness says so, and this is where it becomes visible. A test expecting two
    // would be asserting that an undo erases history, which is the opposite of
    // what this product records.
    assert_eq!(
        store.session_turns(session_id).expect("every turn").len(),
        3,
        "the branched-away turn and the undone turn must both still be in the \
         store, because a rewind restores state without editing the trace",
    );

    // ---- /model: a different model, and the context is the FORKED one --------
    let alt =
        std::env::var("OPENROUTER_MODEL_ALT").unwrap_or_else(|_| "openai/gpt-4o-mini".to_string());
    println!("switching from {} to {alt}", model());
    let switched = io_harness::OpenRouter::new(&key, &alt);
    let third = session
        .turn_steered(
            "Which single word did you reply with a moment ago? Answer with that word only.",
            &switched,
            &store,
            &policy,
            &DenyAll,
            &io_harness::Ignore,
            &inbox,
        )
        .await
        .expect("the switched turn runs");
    println!("turn 3 outcome: {:?}", third.outcome);

    // The one assertion that checks the fork and the switch together, and it is on
    // the durable record of what the provider was shown rather than on the reply —
    // see `live_f6_a_switched_model_inherits_the_conversation` for why a reply is
    // the wrong thing to assert here.
    let shown = shown_to(&store, third.run_id);
    println!("what the switched model was shown:\n{shown}");
    assert!(
        shown.contains("alpha"),
        "the forked conversation did not reach the switched provider: {shown:?}",
    );
    assert!(
        !shown.contains("beta"),
        "the branched-away turn was sent to the switched model, so the fork did \
         not take: {shown:?}",
    );

    // ---- leave, and /resume -------------------------------------------------
    let head_on_leaving = session.head();
    drop(session);

    let (found, cut) = io_cli::sessions::recent(&store).expect("the session list");
    println!("resume list ({} rows, cut={cut}):", found.len());
    for row in io_cli::sessions::rows(&found, 80, &DARK.glyphs) {
        println!("  {} — {:?}", row.label, row.detail);
    }
    let listed = found
        .iter()
        .find(|session| session.id == session_id)
        .expect("the session that just ran must be in the resume list");
    assert_eq!(
        listed.turns, 4,
        "the row counts every turn the session holds — the branched-away one and \
         the undone one included, because both are still in the store",
    );
    // The session's first prompt ever, not the first on the path it currently
    // holds. Deliberate, and it is the row's identity: a session that is forked or
    // rewound keeps the same label, so a list somebody is scanning does not
    // reshuffle its own descriptions underneath them.
    assert!(
        listed.prompt.contains("Change the third line"),
        "the row carries the session's first prompt as its stable identity: {:?}",
        listed.prompt,
    );

    let reopened = Session::reopen(&store, session_id).expect("reopening the session");
    assert_eq!(
        reopened.head(),
        head_on_leaving,
        "a resumed session must come back at the turn it stopped on",
    );
    assert_eq!(
        reopened.root(),
        root,
        "and pointed at the same workspace it was about",
    );
    println!("resumed session {session_id} at head {:?}", reopened.head());
}

/// F4 against a real provider: the stream a real run produces deserializes back
/// into the harness's own type.
///
/// Every offline arm uses a scripted provider, and this product has twice
/// shipped a feature that passed every offline test and failed on the first real
/// run — an approver reading absolute paths in 0.3.0, and a `/model` assertion
/// that read a model's reply in 0.4.0. The round trip is what a scripted
/// provider cannot prove: those events came from a real agent loop.
///
/// Nothing here asserts on what the model SAID. The assertions are the shape of
/// the stream, the presence of the harness's own step and finished events, and
/// the exit status — all of which survive a vendor retune.
#[tokio::test]
#[ignore = "live: needs OPENROUTER_API_KEY"]
async fn f4_live_a_real_run_streams_events_that_round_trip() {
    let dir = tempfile::tempdir().expect("a workspace");
    let store = Store::memory().expect("an in-memory store");
    let mut session = Session::open(&store, dir.path()).expect("a session");
    let config = io_harness::Config::from_toml("").expect("an empty configuration");
    let provider = io_harness::OpenRouter::new(key(), model());

    let json = io_cli::exec::Ndjson::new(Vec::new());
    let result = io_cli::exec::turn(
        &provider,
        &store,
        &mut session,
        &config,
        &workspace_policy(),
        "Create a file called live.txt containing exactly the word ok, then stop.".into(),
        None,
        &json,
    )
    .await
    .expect("a live headless turn runs");

    let written = String::from_utf8(json.into_inner()).expect("the stream is UTF-8");
    let lines: Vec<&str> = written.lines().collect();
    assert!(!lines.is_empty(), "a real run should emit events");

    // Every line is the harness's own type, read back with the harness's own
    // derive. A shape io-cli invented would fail here and nowhere else.
    let events: Vec<RunEvent> = lines
        .iter()
        .map(|line| serde_json::from_str(line).unwrap_or_else(|e| panic!("{line}\n{e}")))
        .collect();

    let kinds: Vec<String> = lines
        .iter()
        .map(|line| {
            let value: serde_json::Value = serde_json::from_str(line).expect("JSON");
            value["event"].as_str().expect("a tagged event").to_string()
        })
        .collect();

    assert!(kinds.iter().any(|kind| kind == "started"), "{kinds:?}");
    assert!(kinds.iter().any(|kind| kind == "finished"), "{kinds:?}");
    assert!(
        events
            .iter()
            .any(|event| matches!(event.kind, EventKind::Step { .. })),
        "a run that edits a file takes at least one step: {kinds:?}",
    );

    println!("live: outcome {:?}", result.outcome);
    println!("live: kinds {kinds:?}");
    println!("live: file exists {}", dir.path().join("live.txt").exists());
    // **Not asserted: that the run ended cleanly.** This used to require exit 0,
    // which is an assertion about how the agent chose to stop rather than about
    // the stream this test exists to check. 0.5.0's own record says exit 5 is
    // common even when the work completed, because the agent keeps going after
    // the useful part — and on 2026-08-18 it does, reaching `Stalled` on every
    // run of this goal while writing the file correctly each time. The exit code
    // is a total function of the outcome and `tests/exec.rs` covers the mapping
    // over all fifteen variants offline, which is where that belongs.
    //
    // What is asserted is that the outcome maps to a code the published table
    // defines at all, so a harness that grew a variant this product does not
    // handle still fails here.
    assert!(
        io_cli::exec::code(&result.outcome) <= io_cli::exec::UNFINISHED,
        "the outcome {:?} maps outside the published exit-code table",
        result.outcome,
    );
    assert!(
        dir.path().join("live.txt").exists(),
        "the agent was asked to write a file inside the sandbox and should have",
    );

    println!("live: {} events, outcome {:?}", lines.len(), result.outcome);
}

/// **The live verification for F1, F3 and F4 — 0.10.0.**
///
/// There is no human gate on this release, so this is the verification rather
/// than a rehearsal for one. What it drives is the whole seam: io-cli's own
/// `TaskContract`, carrying io-cli's own responder and plan gate, through the one
/// session entry point that takes a contract, against a real model — and the
/// operator's side answered through `App`, the same type a keystroke reaches.
///
/// The plan gate is the deterministic half: registering one turns io-harness's
/// planning phase on, so the run must propose before it acts. The responder is
/// the opportunistic half — whether a model asks about intent is the model's
/// choice — so what happens either way is printed, and what is asserted is that
/// if it did ask, the answer this crate sent is the answer the run recorded.
/// **Multi-threaded, and that is the second finding this test paid for.** Every
/// other test in this file runs on `#[tokio::test]`'s default current-thread
/// runtime, which is fine for a flat turn. A *contained* turn is not: the agent
/// had already made the edit — `notes.md` on disk said `new line` — and the turn
/// never returned, with no socket open and the thread parked, which is a
/// deadlock and not a slow model. A fan-out needs somewhere for its children to
/// run.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "live: needs OPENROUTER_API_KEY"]
async fn live_f1_f3_f4_a_contained_turn_carries_this_crates_contract() {
    use io_cli::app::App;
    use io_cli::contract::Capabilities;
    use io_harness::Containment;

    let key = key();
    let dir = tempfile::tempdir().expect("a workspace");
    let root = dir.path();
    std::fs::write(root.join("notes.md"), "# notes\n\nold line\n").expect("the fixture file");

    let store = Store::open(root.join("runs.db")).expect("a store");
    let mut session = Session::open(&store, root).expect("a session");
    let provider = io_harness::OpenRouter::new(&key, model());
    let policy = workspace_policy();

    let collected = Arc::new(Mutex::new(Vec::new()));
    let observer = Collector {
        events: Arc::clone(&collected),
    };

    let (answerer, mut questions) = io_cli::intent::channel();
    let (gate, mut plans) = io_cli::plan::channel();
    let contract = io_cli::contract::session(
        "Replace the line `old line` in notes.md with `new line`. Nothing else.",
        root.to_path_buf(),
        &Capabilities::default(),
    )
    .with_responder(Arc::new(answerer))
    .with_plan_gate(Arc::new(gate))
    // **Bounded, and that is a finding rather than tidiness.** The first run of
    // this test used `DenyAll` — the approver the other live tests use, whose
    // goals only read — and a goal that writes under it spends the whole run
    // being refused and trying again; it was still going after ten minutes. A
    // live verification of the contract seam must not be a live verification of
    // what an agent does when it is refused forever, so the approver approves and
    // the contract carries the step cap that only a contract can carry.
    .with_max_steps(12);

    // The operator's side, driven through `App` so what answers is the type a
    // keystroke reaches rather than a second implementation written for a test.
    let decided = Arc::new(Mutex::new(Vec::<String>::new()));
    let answered = Arc::clone(&decided);
    let operator = tokio::spawn(async move {
        let mut app = App::new(DARK, "live");
        loop {
            tokio::select! {
                Some(proposed) = plans.recv() => {
                    answered.lock().expect("not poisoned").push(format!(
                        "plan: {} steps",
                        proposed.plan.steps.len()
                    ));
                    app.open_plan(proposed);
                    // Enter on an empty prompt: approve.
                    app.key(crossterm::event::KeyEvent::new(
                        crossterm::event::KeyCode::Enter,
                        crossterm::event::KeyModifiers::NONE,
                    ));
                }
                Some(asked) = questions.recv() => {
                    answered
                        .lock()
                        .expect("not poisoned")
                        .push(format!("question: {}", asked.question.question));
                    app.open_intent(asked);
                    for character in "the second one".chars() {
                        app.key(crossterm::event::KeyEvent::new(
                            crossterm::event::KeyCode::Char(character),
                            crossterm::event::KeyModifiers::NONE,
                        ));
                    }
                    app.key(crossterm::event::KeyEvent::new(
                        crossterm::event::KeyCode::Enter,
                        crossterm::event::KeyModifiers::NONE,
                    ));
                }
                else => break,
            }
        }
    });

    let result = session
        .turn_contained_bounded_observed(
            &contract,
            &provider,
            &store,
            &policy,
            &io_harness::ApproveAll,
            &Containment::new(4, 2, 1, 200_000),
            &observer,
        )
        .await
        .expect("the turn runs");

    // **The contract goes first, and that is the third thing this test paid for.**
    // The operator's loop ends when both of its receivers close, and they close
    // when the responder and the gate inside the contract are dropped — so
    // awaiting the task while the contract was still in scope hung the test
    // AFTER the run had finished: the work was done, no socket was open, and the
    // thread was parked on a task that could never end.
    drop(contract);
    drop(session);
    let _ = operator.await;

    let events = collected.lock().expect("not poisoned").clone();
    let kinds: Vec<String> = events
        .iter()
        .map(|event| {
            format!("{:?}", event.kind)
                .split_whitespace()
                .next()
                .unwrap()
                .to_string()
        })
        .collect();

    println!("live 0.10.0: outcome {:?}", result.outcome);
    println!("live 0.10.0: {} events", events.len());
    println!(
        "live 0.10.0: operator answered {:?}",
        decided.lock().expect("not poisoned")
    );

    // **F4, and the deterministic half.** A registered gate turns the planning
    // phase on, so the run proposes before it acts and the decision is recorded.
    let proposed = events
        .iter()
        .filter(|event| matches!(event.kind, EventKind::PlanProposed { .. }))
        .count();
    let decided_events: Vec<&RunEvent> = events
        .iter()
        .filter(|event| matches!(event.kind, EventKind::PlanDecided { .. }))
        .collect();
    println!(
        "live 0.10.0: {proposed} plans proposed, {} decided",
        decided_events.len()
    );
    assert!(
        proposed > 0,
        "registering a plan gate must put the run in its planning phase: {kinds:?}",
    );
    assert!(
        decided_events.iter().any(|event| matches!(
            &event.kind,
            EventKind::PlanDecided { verdict, by, .. } if verdict == "approve" && by == "gate"
        )),
        "the verdict this crate sent is the verdict the run recorded: {:?}",
        decided_events
            .iter()
            .map(|event| &event.kind)
            .collect::<Vec<_>>(),
    );

    // **F3, and the opportunistic half.** Whether a model asks about intent is
    // its choice; what must hold is that an answer given here is the answer
    // recorded there.
    let asked = events
        .iter()
        .filter(|event| matches!(event.kind, EventKind::QuestionAsked { .. }))
        .count();
    println!("live 0.10.0: {asked} questions asked");
    if asked > 0 {
        assert!(
            events.iter().any(|event| matches!(
                &event.kind,
                EventKind::QuestionAnswered { answer, by }
                    if answer.contains("the second one") && by == "responder"
            )),
            "a question answered through the overlay must reach the run: {kinds:?}",
        );
    }

    // **F1.** The contract reached the run at all, which is what every criterion
    // above rests on: a `default_contract` turn has no planning phase to enter.
    assert!(
        io_cli::exec::code(&result.outcome) <= io_cli::exec::UNFINISHED,
        "the outcome {:?} maps outside the published exit-code table",
        result.outcome,
    );
}
