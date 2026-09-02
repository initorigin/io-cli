//! The live rehearsal for F1.
//!
//! Every test that talks to a provider is `#[ignore]`d, so `cargo test` never
//! runs one and CI — which holds no secrets — never tries. Run them by hand with
//! a key in the environment:
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
//!
//! **One test here is not live and runs everywhere**, deliberately:
//! `every_live_arm_that_watches_for_a_question_names_both_ask_variants` reads
//! this file's own source. An arm nobody runs in CI is an arm nothing notices has
//! stopped matching, and 0.33.0 paid for exactly that — see its own comment.

mod support;

use std::sync::{Arc, Mutex};

use io_cli::events::Events;
use io_cli::settings::Posture;
use io_cli::theme::DARK;
use io_cli::verify;
use io_harness::{
    Config, DenyAll, EventKind, Flow, Observer, Policy, ProviderSpec, RunEvent, Session, Steer,
    Store,
};

/// The configuration every arm in this file runs under: none at all.
///
/// From 0.14.0 `contract::session` reads the operator's `io.toml` and applies it,
/// so a session's contract is a function of a file as well as of its arguments.
/// Every arm here was written before that and asserts something else entirely —
/// a real turn against a real endpoint — so each is handed a configuration that
/// names nothing, which is what keeps the contract they were written against
/// exactly what it was. An arm that wants a section applied should say so in its
/// own fixture rather than inherit one from this machine.
fn no_configuration() -> Config {
    Config::from_toml("").expect("an empty configuration file parses")
}

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
    let undone = io_cli::rewind::last_turn(&mut session, &store, &io_harness::Ignore)
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
        &no_configuration(),
        &no_configuration().plugins(),
        &Capabilities::default(),
        Arc::new(answerer),
        Some(Arc::new(gate) as Arc<dyn io_harness::PlanGate>),
    )
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
                Some(delivery) = questions.recv() => {
                    // **One delivery is one overlay, however many questions it
                    // carries** (0.33.0). A batch is answered in place — one
                    // question on screen at a time, deciding one moves to the next
                    // undecided one, deciding the last delivers — so the operator's
                    // side types an answer and presses Enter once per question. A
                    // batch left part-answered is committed by nothing and parks the
                    // whole run, which is why this loops rather than answering the
                    // first and moving on.
                    let count = delivery.len();
                    answered
                        .lock()
                        .expect("not poisoned")
                        .push(format!("questions: {count}"));
                    app.open_intent(delivery);
                    for _ in 0..count {
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
    //
    // **Both ask variants, and that is a gate-integrity fix rather than a
    // feature (0.33.0).** io-harness 0.72.0 emits `QuestionsAsked` for a batched
    // ask and does *not* also emit the singular, so watching only the singular
    // means the moment a model picks `ask_questions` this count is zero, the
    // assertion below is skipped, and a live arm that proves nothing looks
    // exactly like one that passed. `every_live_arm_that_watches_for_a_question`
    // below keeps this pair together.
    let asked = events
        .iter()
        .filter(|event| {
            matches!(
                event.kind,
                EventKind::QuestionAsked { .. } | EventKind::QuestionsAsked { .. }
            )
        })
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

/// **F1 (0.12.0) — a question is answered on a turn that cannot fan out.**
///
/// The uncontained arm, which is the whole point: through 0.11.0 the responder
/// rode `[app.io-cli.containment]`, so a session without caps that reached the
/// ask tool paused with the question persisted and nobody offered it. The goal is
/// written to make asking the sensible move — it names a file with two candidate
/// lines and refuses to say which — but whether a model asks is still the model's
/// choice, so what is asserted is conditional in the same shape 0.10.0's arm uses:
/// if it asked, the answer this crate sent is the answer the run recorded.
///
/// What is asserted unconditionally is the thing that was actually broken: the
/// contract on this arm carries a responder at all.
#[tokio::test]
#[ignore = "live: needs OPENROUTER_API_KEY"]
async fn live_f1_a_question_is_answered_on_an_uncontained_turn() {
    use io_cli::app::App;
    use io_cli::contract::Capabilities;

    let key = key();
    let dir = tempfile::tempdir().expect("a workspace");
    let root = dir.path();
    std::fs::write(
        root.join("notes.md"),
        "# notes\n\nold line\n\n## archive\n\nold line\n",
    )
    .expect("the fixture file");

    let store = Store::open(root.join("runs.db")).expect("a store");
    let mut session = Session::open(&store, root).expect("a session");
    let provider = io_harness::OpenRouter::new(&key, model());
    let policy = workspace_policy();

    let collected = Arc::new(Mutex::new(Vec::new()));
    let observer = Collector {
        events: Arc::clone(&collected),
    };

    let (answerer, mut questions) = io_cli::intent::channel();
    // `None`: no plan gate, which is 0.12.0's default and F2's precondition.
    let contract = io_cli::contract::session(
        "notes.md contains the line `old line` twice. Replace exactly one of them with `new \
         line`. If it is not clear which one is meant, ask before editing.",
        root.to_path_buf(),
        &no_configuration(),
        &no_configuration().plugins(),
        &Capabilities::default(),
        Arc::new(answerer),
        None,
    )
    .with_max_steps(12);

    assert!(
        contract.responder.is_some(),
        "the uncontained arm's contract carries a responder — the defect this release fixes",
    );
    assert!(contract.plan_gate.is_none(), "and no gate nobody asked for");

    let answers = Arc::new(Mutex::new(Vec::<String>::new()));
    let answered = Arc::clone(&answers);
    let operator = tokio::spawn(async move {
        let mut app = App::new(DARK, "live");
        while let Some(delivery) = questions.recv().await {
            // One answer and one Enter per question the delivery carries, for the
            // reason the contained arm above records: a batch is answered in place
            // and a part-answered one parks the run.
            let count = delivery.len();
            answered
                .lock()
                .expect("not poisoned")
                .push(format!("questions: {count}"));
            app.open_intent(delivery);
            for _ in 0..count {
                for character in "the one under archive".chars() {
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
        }
    });

    let result = session
        .turn_bounded_observed(
            &contract,
            &provider,
            &store,
            &policy,
            &io_harness::ApproveAll,
            &observer,
        )
        .await
        .expect("the turn runs");

    // The contract first, for the reason 0.10.0's arm records: the operator's
    // loop ends when the responder inside the contract is dropped, so awaiting it
    // while the contract is still in scope hangs after the run has finished.
    drop(contract);
    drop(session);
    let _ = operator.await;

    let events = collected.lock().expect("not poisoned").clone();
    // Both ask variants, for the reason the contained arm above records: a
    // batched ask emits only the plural, and an arm watching one name would go
    // silent rather than red the first time a model batches.
    let asked = events
        .iter()
        .filter(|event| {
            matches!(
                event.kind,
                EventKind::QuestionAsked { .. } | EventKind::QuestionsAsked { .. }
            )
        })
        .count();
    println!("live 0.12.0 F1: outcome {:?}", result.outcome);
    println!(
        "live 0.12.0 F1: {asked} questions asked, operator answered {:?}",
        answers.lock().expect("not poisoned")
    );

    if asked > 0 {
        assert!(
            events.iter().any(|event| matches!(
                &event.kind,
                EventKind::QuestionAnswered { answer, by }
                    if answer.contains("under archive") && by == "responder"
            )),
            "a question asked on an uncontained turn must be answered by the overlay, not \
             persisted with the run paused: {:?}",
            events.iter().map(|event| &event.kind).collect::<Vec<_>>(),
        );
    }
}

/// **F1 (0.33.0) — a batch asked in one call is answered on one overlay.**
///
/// The arm io-harness 0.72.0's `ask_questions` exists for, driven through `App`
/// so what answers is the type a keystroke reaches. The goal names the tool
/// rather than hoping for it: what is being verified here is that this crate's
/// overlay answers a batch and the run takes the answers, not the model's
/// judgement about when batching is the right move — that judgement is what the
/// two arms above already leave to the model.
///
/// **Every answer is distinct, and that is the assertion.** io-harness emits one
/// `QuestionAnswered` per answer, in the order the questions were asked, so the
/// texts this side typed and the texts the run recorded are comparable as
/// sequences. That is what makes this fail rather than go quiet:
///
/// * an overlay that answered the first question and delivered leaves the batch
///   part-answered, io-harness commits a batch only when every entry is `Some`,
///   the run parks, and **nothing** is recorded — zero answers against the two or
///   three typed here;
/// * an overlay that answered them in the wrong order, or that sent one answer to
///   every question, records a sequence that is the wrong sequence;
/// * a model that declined to ask at all types nothing, which is the model's
///   choice and is printed rather than asserted, exactly as in the arms above.
#[tokio::test]
#[ignore = "live: needs OPENROUTER_API_KEY"]
async fn live_f1_a_batched_ask_is_answered_on_one_overlay() {
    use io_cli::app::App;
    use io_cli::contract::Capabilities;

    let key = key();
    let dir = tempfile::tempdir().expect("a workspace");
    let root = dir.path();
    std::fs::write(
        root.join("notes.md"),
        "# notes\n\nold line\n\n## archive\n\nold line\n",
    )
    .expect("the fixture file");

    let store = Store::open(root.join("runs.db")).expect("a store");
    let mut session = Session::open(&store, root).expect("a session");
    let provider = io_harness::OpenRouter::new(&key, model());
    let policy = workspace_policy();

    let collected = Arc::new(Mutex::new(Vec::new()));
    let observer = Collector {
        events: Arc::clone(&collected),
    };

    let (answerer, mut questions) = io_cli::intent::channel();
    let contract = io_cli::contract::session(
        "Add a short `## setup` section to notes.md. Before you touch the file, use ONE \
         `ask_questions` call to ask me all three of these together: where the section goes, what \
         its one line should say, and whether the `## archive` section stays. Do not edit anything \
         until you have my answers.",
        root.to_path_buf(),
        &no_configuration(),
        &no_configuration().plugins(),
        &Capabilities::default(),
        Arc::new(answerer),
        None,
    )
    .with_max_steps(12);

    // A distinct answer per question, in the order they were asked, kept on this
    // side so the recorded sequence has something to be compared against.
    let spoken = Arc::new(Mutex::new(Vec::<String>::new()));
    let typing = Arc::clone(&spoken);
    let operator = tokio::spawn(async move {
        let mut app = App::new(DARK, "live");
        let mut nth = 0usize;
        while let Some(delivery) = questions.recv().await {
            let count = delivery.len();
            println!("live 0.33.0: a delivery of {count}");
            app.open_intent(delivery);
            // One answer and one Enter per question: the batch is answered in
            // place, deciding one moves to the next undecided one, and deciding
            // the last is what delivers.
            for _ in 0..count {
                nth += 1;
                let text = format!("answer {nth}");
                typing.lock().expect("not poisoned").push(text.clone());
                for character in text.chars() {
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
        }
    });

    let result = session
        .turn_bounded_observed(
            &contract,
            &provider,
            &store,
            &policy,
            &io_harness::ApproveAll,
            &observer,
        )
        .await
        .expect("the turn runs");

    // The contract first, for the reason the arms above record: the operator's
    // loop ends when the responder inside it is dropped.
    drop(contract);
    drop(session);
    let _ = operator.await;

    let events = collected.lock().expect("not poisoned").clone();
    let asked = events
        .iter()
        .filter(|event| {
            matches!(
                event.kind,
                EventKind::QuestionAsked { .. } | EventKind::QuestionsAsked { .. }
            )
        })
        .count();
    let batched: Vec<usize> = events
        .iter()
        .filter_map(|event| match &event.kind {
            EventKind::QuestionsAsked { questions } => Some(questions.len()),
            _ => None,
        })
        .collect();
    let typed = spoken.lock().expect("not poisoned").clone();
    println!("live 0.33.0: outcome {:?}", result.outcome);
    println!("live 0.33.0: {asked} asks, batches of {batched:?}, typed {typed:?}");

    if typed.is_empty() {
        eprintln!("live 0.33.0: the model asked nothing, so there is no batch to answer");
        return;
    }

    let recorded: Vec<String> = events
        .iter()
        .filter_map(|event| match &event.kind {
            EventKind::QuestionAnswered { answer, by } if by == "responder" => {
                Some(answer.trim().to_string())
            }
            _ => None,
        })
        .collect();
    assert_eq!(
        recorded, typed,
        "every answer typed on the overlay must reach the run, in the order it was asked — a \
         batch this side left part-answered is committed by nothing and parks the run with no \
         answer recorded at all",
    );
}

/// **The offline gate that keeps the three arms above from going silent
/// (0.33.0).**
///
/// A live arm runs by hand, never in CI and not every release, so nothing else in
/// this repository would notice the day one of them stopped matching. io-harness
/// 0.72.0 emits the plural for a batched ask and does **not** also emit the
/// singular, so an arm that filters on the singular alone matches nothing the
/// moment a model picks `ask_questions`: the count goes to zero, the assertion
/// behind `if asked > 0` never runs, and the arm passes having proved nothing.
/// Silence is indistinguishable from a green gate, which is the one failure a
/// test must not have.
///
/// **Why this is not a `contains`.** `source.contains("QuestionsAsked")` is
/// satisfied by this very paragraph, by a `use`, by any prose anywhere in a file
/// this size — it would go green on a file whose every arm still watched the
/// singular alone. So the needle is pinned to the arm it has to appear in: every
/// *pattern position* — a variant name followed by a braced rest pattern, which
/// is a `matches!` arm and nothing else — must sit in an alternation with the
/// other variant. Prose never trips this, because prose does not write a rest
/// pattern after a name; and a comment can never satisfy it, because satisfying
/// it means being the other half of an alternation.
///
/// Sabotage: drop the plural from either alternation above. This fails naming the
/// file and the line it was dropped from.
#[test]
fn every_live_arm_that_watches_for_a_question_names_both_ask_variants() {
    // Spelled in halves so this test's own source carries no occurrence of what
    // it hunts for. A gate whose first finding is its own assertion message is a
    // gate that gets deleted.
    let one = concat!("Question", "Asked");
    let many = concat!("Questions", "Asked");
    let arm = format!("{one} {{ .. }}");
    // Either order, and unqualified: the qualification is stripped below so that
    // `EventKind::` being imported or spelled out is not what this gate decides.
    let paired = [
        format!("{arm} | {many} {{ .. }}"),
        format!("{many} {{ .. }} | {arm}"),
    ];

    let lines: Vec<&str> = include_str!("live.rs").lines().collect();
    let mut watching = 0usize;
    let mut lone: Vec<String> = Vec::new();
    for (index, line) in lines.iter().enumerate() {
        if !line.replace("EventKind::", "").contains(&arm) {
            continue;
        }
        watching += 1;
        // Two lines, whitespace flattened: an alternation rustfmt wrapped onto the
        // next line is still one arm, and a gate that failed on formatting is a
        // gate someone turns off inside a release.
        let window = lines[index..(index + 2).min(lines.len())]
            .join(" ")
            .replace("EventKind::", "")
            .split_whitespace()
            .collect::<Vec<_>>()
            .join(" ");
        if !paired.iter().any(|form| window.contains(form.as_str())) {
            lone.push(format!("tests/live.rs:{}: {}", index + 1, line.trim()));
        }
    }

    assert!(
        lone.is_empty(),
        "these live arms watch for one ask variant only, so a batched ask makes them match \
         nothing and their assertions are skipped rather than failed — write `{arm} | {many} {{ \
         .. }}`:\n{}",
        lone.join("\n"),
    );
    // **And the gate is not vacuous.** A renamed variant, or a rewrite that stops
    // matching on the kind at all, would leave the loop above finding nothing and
    // passing — which is the same silence this file exists to close.
    assert!(
        watching >= 2,
        "this gate found {watching} live arms watching for a question; it is watching nothing, \
         so `{one}` is no longer how those arms are written",
    );
}

/// **F2 (0.12.0) — a contained turn proposes no plan unless the operator asked.**
///
/// The absence this release exists for, asserted on the events rather than on an
/// overlay that never opened — an overlay that never opened is also what a broken
/// channel looks like, and 0.9.0 already shipped one control blind to the bound it
/// was checking.
///
/// Contained, deliberately: an uncontained turn never planned, so a flat arm here
/// would pass without touching the thing that changed. Multi-threaded for the
/// reason 0.10.0's arm records — a contained turn deadlocks on a current-thread
/// runtime.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "live: needs OPENROUTER_API_KEY"]
async fn live_f2_a_contained_turn_does_not_plan_unless_asked() {
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

    let (answerer, _questions) = io_cli::intent::channel();
    let contract = io_cli::contract::session(
        "Replace the line `old line` in notes.md with `new line`. Nothing else.",
        root.to_path_buf(),
        &no_configuration(),
        &no_configuration().plugins(),
        &Capabilities::default(),
        Arc::new(answerer),
        None,
    )
    .with_max_steps(12);

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

    drop(contract);
    drop(session);

    let events = collected.lock().expect("not poisoned").clone();
    let proposed = events
        .iter()
        .filter(|event| matches!(event.kind, EventKind::PlanProposed { .. }))
        .count();
    let decided = events
        .iter()
        .filter(|event| matches!(event.kind, EventKind::PlanDecided { .. }))
        .count();
    println!("live 0.12.0 F2: outcome {:?}", result.outcome);
    println!(
        "live 0.12.0 F2: {} events, {proposed} plans proposed, {decided} decided",
        events.len()
    );
    // Printed rather than asserted. Whether a model finishes this goal inside the
    // step cap is the model's business and would make the criterion flaky; whether
    // it was *allowed* to do the work is this release's business, and the file on
    // disk is the cheapest way to see it.
    println!(
        "live 0.12.0 F2: notes.md is {:?}",
        std::fs::read_to_string(root.join("notes.md")).expect("the fixture survives"),
    );

    assert_eq!(
        proposed,
        0,
        "a contained turn with no gate must not enter the planning phase: {:?}",
        events.iter().map(|event| &event.kind).collect::<Vec<_>>(),
    );
    assert_eq!(decided, 0, "and nothing decides a plan that was never made");

    // **And the run was not silently denied instead.** The planning phase denies
    // every write under a `plan-gate` layer, so a turn that both proposed nothing
    // AND wrote nothing would satisfy the assertion above while being exactly the
    // failure it is meant to exclude.
    let denied_by_the_gate = events.iter().any(|event| {
        matches!(
            &event.kind,
            EventKind::Refused { layer, .. } if layer.as_deref() == Some("plan-gate")
        )
    });
    assert!(
        !denied_by_the_gate,
        "nothing may be refused by a layer no gate turned on: {:?}",
        events.iter().map(|event| &event.kind).collect::<Vec<_>>(),
    );
    assert!(
        events.len() > 1,
        "the run has to have actually happened for its absences to mean anything",
    );
}

/// **F2 (0.12.0) — the gate the operator asked for reaches a turn that cannot
/// fan out.**
///
/// The positive half of F2, and the shape that was impossible before this
/// release: through 0.11.0 a plan gate could only be attached where a containment
/// was, so an uncontained turn had no planning phase to enter however much the
/// operator wanted one. Here the caps are absent, the gate is present because
/// `/plan on` would have put it there, and the run must propose.
///
/// Asserted on the events rather than on a capture, because a capture of this is
/// flaky — see the comment in `live_f3_f4_plan_switches_and_the_status_line_says_so`.
/// Deterministic here because the planning phase is entered from
/// `plan_gate.is_some()` in io-harness, not from anything the model decides.
#[tokio::test]
#[ignore = "live: needs OPENROUTER_API_KEY"]
async fn live_f2_a_gate_the_operator_asked_for_reaches_the_run() {
    use io_cli::app::App;
    use io_cli::contract::Capabilities;

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

    let (answerer, _questions) = io_cli::intent::channel();
    let (gate, mut plans) = io_cli::plan::channel();
    let contract = io_cli::contract::session(
        "Replace the line `old line` in notes.md with `new line`. Nothing else.",
        root.to_path_buf(),
        &no_configuration(),
        &no_configuration().plugins(),
        &Capabilities::default(),
        Arc::new(answerer),
        Some(Arc::new(gate) as Arc<dyn io_harness::PlanGate>),
    )
    .with_max_steps(12);

    let seen = Arc::new(Mutex::new(Vec::<String>::new()));
    let proposals = Arc::clone(&seen);
    let operator = tokio::spawn(async move {
        let mut app = App::new(DARK, "live");
        while let Some(proposed) = plans.recv().await {
            proposals
                .lock()
                .expect("not poisoned")
                .push(format!("{} steps", proposed.plan.steps.len()));
            app.open_plan(proposed);
            // Enter on an empty prompt: approve.
            app.key(crossterm::event::KeyEvent::new(
                crossterm::event::KeyCode::Enter,
                crossterm::event::KeyModifiers::NONE,
            ));
        }
    });

    let result = session
        .turn_bounded_observed(
            &contract,
            &provider,
            &store,
            &policy,
            &io_harness::ApproveAll,
            &observer,
        )
        .await
        .expect("the turn runs");

    drop(contract);
    drop(session);
    let _ = operator.await;

    let events = collected.lock().expect("not poisoned").clone();
    let proposed = events
        .iter()
        .filter(|event| matches!(event.kind, EventKind::PlanProposed { .. }))
        .count();
    println!("live 0.12.0 F2+: outcome {:?}", result.outcome);
    println!(
        "live 0.12.0 F2+: {proposed} plans proposed on an UNCONTAINED turn, overlay saw {:?}",
        seen.lock().expect("not poisoned")
    );

    assert!(
        proposed > 0,
        "a gate on an uncontained turn must put the run in its planning phase: {:?}",
        events.iter().map(|event| &event.kind).collect::<Vec<_>>(),
    );
    assert!(
        events.iter().any(|event| matches!(
            &event.kind,
            EventKind::PlanDecided { verdict, by, .. } if verdict == "approve" && by == "gate"
        )),
        "and the verdict the overlay sent is the verdict the run recorded",
    );
}

// ---------------------------------------------------------------------------
// 0.11.0 — the real binary, on a pty, against the real provider.
//
// **Everything above this line asserts on what the library returned.** That is a
// different question from what reached a terminal, and F2 exists because the two
// have disagreed before: 0.9.0 shipped a control that was blind to the bound it
// was meant to check, and four consecutive releases have had the running binary
// find something 500-odd tests could not.
//
// So these drive `target/…/io` itself through a pty that answers `ESC[6n`, and
// assert on the bytes it wrote. The driver is `evidence/0.11.0/drive.py`, kept
// with the captures it produced.
// ---------------------------------------------------------------------------

/// The pty driver, from this release's evidence directory.
fn driver() -> std::path::PathBuf {
    std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join(".ultraship/products/io-cli/evidence/0.13.0/drive.py")
}

/// A workspace, and a configuration that starts a session without the wizard.
///
/// The key is not written to the file. `api_key` absent means the provider's own
/// environment variable, which is the arrangement this product documents and
/// prefers, and a key on disk in a temporary directory is a key on disk.
///
/// **The configuration lives outside the workspace, and it has to.** A file
/// inside the root is project-scoped, and a project-scoped file may narrow the
/// permission boundary and never widen it — a repository you cloned must not be
/// able to grant itself permission. io-cli says exactly that and refuses to
/// start, which is how this was found.
fn configured_workspace() -> (tempfile::TempDir, tempfile::TempDir) {
    let config = tempfile::tempdir().expect("a config directory");
    let workspace = tempfile::tempdir().expect("a workspace");
    std::fs::write(
        config.path().join("io.toml"),
        format!(
            // The sandboxed-workspace posture, so a turn that writes a file
            // finishes rather than stopping on an overlay nothing in a script
            // reliably answers. The approval surface is asserted in
            // `tests/approval.rs`; what these captures are about is the
            // transcript and the two rows above the composer.
            "[[provider]]\nkind = \"openrouter\"\nmodel = {:?}\n\n\
             [policy.defaults]\nread = \"allow\"\nwrite = \"allow\"\n\
             exec = \"allow\"\nnet = \"deny\"\n",
            model()
        ),
    )
    .expect("the configuration is written");
    (config, workspace)
}

/// The capture with its escape sequences removed, leaving what was displayed.
///
/// Enough for the assertions here and no more: CSI and OSC, which is what a
/// viewport draw is made of. A row is written as text, cursor moves and colour
/// changes interleaved, so a claim about what a reader saw has to be made
/// against the text with the machinery taken out of it.
fn strip_escapes(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut chars = text.chars().peekable();
    while let Some(c) = chars.next() {
        if c != '\x1b' {
            out.push(c);
            continue;
        }
        match chars.next() {
            // CSI: parameters, then one final byte in `@`..`~`.
            Some('[') => {
                for c in chars.by_ref() {
                    if ('\x40'..='\x7e').contains(&c) {
                        break;
                    }
                }
            }
            // OSC: runs to a BEL or a string terminator.
            Some(']') => {
                while let Some(c) = chars.next() {
                    if c == '\x07' {
                        break;
                    }
                    if c == '\x1b' && chars.peek() == Some(&'\\') {
                        chars.next();
                        break;
                    }
                }
            }
            // Anything else is a two-byte escape and is already consumed.
            _ => {}
        }
    }
    out
}

/// Run the real binary under the pty driver and return everything it wrote.
///
/// `script` is the driver's own format: `<delay>\t<text>`, where a line starting
/// `raw:` is sent without the `\r` the driver otherwise appends — which is what a
/// key that OPENS something needs, `/` above all.
///
/// **A script does not end with `/quit`.** Typed as a line it opens the palette
/// on its `/`, filters to one row, and `Enter` on a palette row puts the command
/// in the composer rather than running it — which is deliberate and documented,
/// and which cost two inconclusive captures here. `raw:\x04` is `Ctrl+D`, which
/// leaves from an empty prompt.
///
/// **And a script does not key off a clock.** Three runs of the same fixed-delay
/// script produced three different captures — the palette not opening, the
/// palette not opening *or* closing, and the whole thing working — because a
/// keystroke sent at a fixed second lands wherever the machine's load puts it.
/// `wait:<text>` holds until the program has written that text, and with the
/// waits in place three runs are byte-identical.
fn captured(
    name: &str,
    config: &std::path::Path,
    workspace: &std::path::Path,
    script: &str,
) -> String {
    // Every capture before 0.13.0 ran against a terminal that speaks the Kitty
    // keyboard protocol, because until F9 nothing here depended on the
    // difference. That stays the default.
    captured_on(name, config, workspace, script, true)
}

/// [`captured`], on a terminal that does or does not advertise the protocol.
fn captured_on(
    name: &str,
    config: &std::path::Path,
    workspace: &std::path::Path,
    script: &str,
    kitty: bool,
) -> String {
    use std::io::Write;
    use std::process::{Command, Stdio};

    // This release's directory, not the driver's. `drive.py` is kept where it was
    // written and is shared; a capture belongs to the release whose claims it is
    // evidence for.
    let out = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join(".ultraship/products/io-cli/evidence")
        .join(env!("CARGO_PKG_VERSION"))
        .join(name);
    if let Some(parent) = out.parent() {
        std::fs::create_dir_all(parent).expect("the evidence directory");
    }
    let mut child = Command::new("python3")
        .arg(driver())
        .arg(&out)
        .arg(env!("CARGO_BIN_EXE_io"))
        .arg("-C")
        .arg(workspace)
        .env("IO_CONFIG", config.join("io.toml"))
        .env("OPENROUTER_API_KEY", key())
        .env("IO_DRIVE_DEADLINE", "180")
        // Whether the pty answers the Kitty keyboard-enhancement query, which is
        // the whole of the difference F9 is about. Read by the driver itself, not
        // by the child, so it goes on this command rather than into
        // `IO_DRIVE_ENV`.
        .env("IO_DRIVE_KITTY", if kitty { "1" } else { "0" })
        .stdin(Stdio::piped())
        .stdout(Stdio::inherit())
        .stderr(Stdio::inherit())
        .spawn()
        .expect("python3 runs the driver");
    child
        .stdin
        .as_mut()
        .expect("the driver takes a script")
        .write_all(script.as_bytes())
        .expect("the script is sent");
    assert!(child.wait().expect("the driver exits").success());

    let bytes = std::fs::read(&out).expect("the capture was written");
    assert!(
        bytes.len() > 1_000,
        "{name} captured {} bytes, which is a run that never started rather than \
         a run that said nothing",
        bytes.len(),
    );
    String::from_utf8_lossy(&bytes).into_owned()
}

/// **0.12.0 F3 and F4.** `/plan` reports, switches, and says so on the status line.
///
/// The half no unit test can reach. `Status::planning` is asserted as a value in
/// `tests/status.rs`, and the parse is asserted in `tests/plan.rs`, but whether
/// the word actually reaches a terminal — and whether it is still there after a
/// turn has ended — is a question about the binary, and this product has shipped
/// a control blind to exactly that gap before.
///
/// The script asks three times: bare `/plan` before anything, `/plan on`, then a
/// real turn, then bare `/plan` again. What must hold is that the first answer is
/// the working one, the word appears after the switch, and it is still there
/// after the turn — `Status::forget_run` clears every neighbouring field and must
/// not clear this one.
#[tokio::test]
#[ignore]
async fn live_f3_f4_plan_switches_and_the_status_line_says_so() {
    let (config, dir) = configured_workspace();
    std::fs::write(dir.path().join("notes.txt"), "one\ntwo\n").expect("the fixture file");

    // **An argument is typed after the palette, not into it, and two captures
    // paid for that sentence.** `/` at an empty prompt opens the palette and the
    // keystroke never reaches the composer, so a command is chosen from a row —
    // which puts `/plan` in the composer rather than running it — and only then
    // does `raw: on` make it `/plan on` and `raw:\r` submit it. Typing the whole
    // `/plan on` as one line filters the palette by `plan on`, which matches no
    // row: the first capture ended with `> /plan` unsubmitted and the second on
    // `No row matches “plan ”`. Both are the palette working as designed, and the
    // same is true of `/contain on` since 0.8.0.
    let text = captured(
        "live-planning.raw",
        config.path(),
        dir.path(),
        "0\twait:for commands\n\
         0.3\t/plan\n\
         0.3\traw:\\r\n\
         0\twait:working — a turn starts on the job\n\
         0.3\t/plan\n\
         0.3\traw: on\n\
         0.3\traw:\\r\n\
         0\twait:planning from the next turn\n\
         0.3\tRead notes.txt and say what its second line is. Do not write anything.\n\
         0\twait:Enter approves\n\
         0.5\traw:\\r\n\
         0\twait:ready\n0.5\traw:\\x04\n",
    );

    // **Whether the turn actually proposed is not asserted here, and that is a
    // finding rather than a gap.** Two captures of this exact script disagreed:
    // one showed the proposal and the `Enter approves` footer, the next ran the
    // same goal to completion in four steps without proposing. Whether a model
    // reaches for the plan tool is the model's business, and an assertion on it
    // would be a flake with a criterion's name on it. The deterministic version
    // of that claim is `live_f2_a_gate_the_operator_asked_for_reaches_the_run`,
    // which asserts on the events. What is asserted here is what the interface
    // does, which is deterministic: it switched, and it says so.

    // Bare `/plan` first, before anything was switched: it reports the default
    // and it reports it as a phase rather than as a toggle having happened.
    assert!(
        text.contains("working — a turn starts on the job"),
        "bare /plan reports the phase it is in",
    );
    assert!(
        text.contains("planning from the next turn"),
        "/plan on says what it did, and when it takes effect",
    );

    // **F4, and the part that only a capture can show.** The word is on the
    // status line after the turn has finished — the run is over, `forget_run` has
    // been through every field beside it, and the phase is still on.
    let after_the_turn = text
        .rsplit_once("ready")
        .map(|(_, tail)| tail.to_string())
        .unwrap_or_default();
    assert!(
        after_the_turn.contains("planning"),
        "the phase outlives the run it was set on; the tail of the capture said: \
         {after_the_turn:?}",
    );
}

/// **0.11.0 F2.** The six strings the owner named are gone from a real run.
///
/// Asserted against the pty capture and never against the `Vec<Line>` the
/// renderer returns, which is the criterion's own sabotage arm: the renderer can
/// be right while the binary prints all six, and that is exactly the class of
/// control this product has already shipped once.
///
/// The presences are half the test. "Quiet" must not be reached by committing
/// nothing at all, so the same capture has to carry the agent's answer and a tool
/// cell — and, for F10, the two facts the removed rows used to hold.
#[tokio::test]
#[ignore]
async fn live_f2_the_strings_the_owner_named_are_gone_from_a_real_run() {
    let (config, dir) = configured_workspace();
    std::fs::write(dir.path().join("greeting.txt"), "hello\n").expect("the fixture file");

    let text = captured(
        "live-transcript.raw",
        config.path(),
        dir.path(),
        "0\twait:for commands\n0.3\tRead greeting.txt and tell me in one sentence what it says.\n\
         0\twait:working\n0\twait:ready\n0.5\traw:\\x04\n",
    );

    for absent in [
        "via ",
        "prompt_composed",
        "contained",
        "reasoning",
        "answered",
        "finished · ",
    ] {
        assert!(
            !text.contains(absent),
            "{absent:?} reached the terminal in a real run",
        );
    }

    // Committing nothing is not the same as being quiet.
    assert!(
        text.contains("greeting.txt"),
        "the turn's own subject is not in the capture at all",
    );
    // **F10.** The facts the two removed rows carried, where they moved to. The
    // footer drops the `provider:` label — the value names itself, and a label
    // on every field is what made that row one grey run — so what is asserted is
    // the provider's own name.
    assert!(
        text.contains("openrouter"),
        "the provider is on the status line"
    );
    assert!(
        text.contains(" step") || text.contains("steps"),
        "the step count is on the status line",
    );
}

/// **0.13.0 F9.** The key reference names the newline key this terminal can
/// report — in the same binary, twice, against two terminals.
///
/// The unit tests drive the decision both ways by handing it a boolean. What no
/// unit test can reach is whether the boolean the binary uses is the one the
/// terminal actually answered with, and that is the whole defect: a session that
/// negotiated the protocol up and then printed `Alt+Enter`, or the reverse, would
/// pass every test in `tests/keyboard.rs`.
///
/// `IO_DRIVE_KITTY=0` is the second terminal — the pty answers the enhancement
/// query never, the way Apple's Terminal does, and crossterm's wait times out
/// into a "no".
#[tokio::test]
#[ignore]
async fn live_f9_the_key_reference_names_the_key_this_terminal_can_report() {
    let (config, dir) = configured_workspace();

    // `/help` is chosen from the palette rather than typed, for the reason 0.12.0
    // wrote down: `/` opens the palette and the keystroke never reaches the
    // composer, so the name is a filter and `Enter` puts the command in the
    // composer, which a second `Enter` submits.
    let script = "0\twait:for commands\n0.3\traw:/\n0\twait:Which command?\n\
                  0.3\traw:help\n0.3\traw:\\r\n0\twait:/help\n0.3\traw:\\r\n\
                  0\twait:Enter\n0.5\traw:\\x04\n";

    let advertised = captured_on(
        "live-keys-kitty.raw",
        config.path(),
        dir.path(),
        script,
        true,
    );
    let plain = captured_on(
        "live-keys-plain.raw",
        config.path(),
        dir.path(),
        script,
        false,
    );

    // Whitespace dropped: a table row is drawn with cursor moves inside it, so a
    // literal match asks the terminal for spaces it had no reason to write.
    let squash = |text: &str| -> String {
        strip_escapes(text)
            .chars()
            .filter(|c| !c.is_whitespace())
            .collect()
    };
    let advertised = squash(&advertised);
    let plain = squash(&plain);

    assert!(
        advertised.contains("Shift+Enter"),
        "a terminal that speaks the protocol was not told about `Shift+Enter`",
    );
    assert!(
        plain.contains("Alt+Enter"),
        "a terminal that cannot report `Shift+Enter` was not given a key that works",
    );
    assert!(
        plain.contains("cannotreport"),
        "the terminal that cannot report `Shift+Enter` was not told why it is missing",
    );
    // The two captures are the same binary, the same script and the same session,
    // and they differ. That difference is the criterion.
    assert_ne!(
        advertised, plain,
        "both terminals were told the same thing, so the answer was never read",
    );
}

/// **0.13.0 F8.** The operator's second prompt has air above it, in a real run.
///
/// `tests/events.rs` asserts the rule against the renderer; this asserts it
/// against the scrollback a terminal actually received, which is the one place
/// the two could differ — the renderer's blank line is a `Line` and the
/// terminal's is a row that something else may have written over.
///
/// The turn between the two prompts is a real one, so what precedes the second
/// `›` is whatever that turn ended with: an answer, a tool cell, or a thought
/// footer. Every one of those is a committed designed block, which is what the
/// criterion is about.
#[tokio::test]
#[ignore]
async fn live_f8_a_second_prompt_is_not_welded_to_the_block_above_it() {
    let (config, dir) = configured_workspace();
    std::fs::write(dir.path().join("greeting.txt"), "hello\n").expect("the fixture file");

    let text = captured(
        "live-gap.raw",
        config.path(),
        dir.path(),
        "0\twait:for commands\n0.3\tRead greeting.txt and tell me in one sentence what it says.\n\
         0\twait:ready\n0.5\tAnd what is the file called?\n0\twait:ready\n0.5\traw:\\x04\n",
    );

    // The transcript as a person would read it: escapes stripped, rows as rows.
    let rows: Vec<String> = strip_escapes(&text)
        .lines()
        .map(|row| row.trim_end().to_string())
        .collect();
    let marks: Vec<usize> = rows
        .iter()
        .enumerate()
        .filter(|(_, row)| row.trim_start().starts_with('›'))
        .map(|(at, _)| at)
        .collect();
    assert!(
        marks.len() >= 2,
        "the capture holds {} goal lines and this test needs the second: {rows:#?}",
        marks.len(),
    );

    // **Only "at least one", and the reason is what this capture is.** A pty
    // capture is a byte stream, not a screen: a committed blank row and the
    // boundary between two viewport frames look identical in it, and the frames
    // are interleaved with the transcript on the same stream rows. So the half
    // this can honestly answer is that the goal line is not welded to the block
    // above it. "Never two blank rows" is a claim about rows as rows, and it is
    // asserted where rows are rows — `f8_a_prompt_after_an_answer_is_one_blank_row_and_not_two`
    // in `tests/events.rs`, against the `Vec<Line>` the renderer returns.
    for at in marks.iter().skip(1) {
        let above = rows
            .get(at.saturating_sub(1))
            .map(String::as_str)
            .unwrap_or("");
        assert!(
            above.trim().is_empty(),
            "the goal line at row {at} is welded to {above:?}",
        );
    }
}

/// **0.11.0 F5 and F6.** The two rows above the composer, in a real run.
///
/// One capture for both, because they are one arrangement: the word and the clock
/// on the top row, and the literal act under it. What the second row says depends
/// on where the turn is when a frame lands, so it is asserted as "one of the
/// things F6 allows" rather than as a single string.
#[tokio::test]
#[ignore]
async fn live_f5_f6_the_activity_line_and_the_live_row_are_in_a_real_run() {
    let (config, dir) = configured_workspace();
    std::fs::write(dir.path().join("notes.txt"), "one\ntwo\n").expect("the fixture file");

    let text = captured(
        "live-working-view.raw",
        config.path(),
        dir.path(),
        "0\twait:for commands\n\
         0.3\tRead notes.txt, then write a file called out.txt containing its second line.\n\
         0\twait:working\n0\twait:ready\n0.5\traw:\\x04\n",
    );

    let word = io_cli::status::WORDS
        .iter()
        .find(|word| text.contains(**word))
        .unwrap_or_else(|| panic!("no activity word reached the terminal"));
    println!("live 0.11.0: the activity line said {word}");

    assert!(
        ["Read", "Write", "thinking", "waiting for you"]
            .iter()
            .any(|said| text.contains(said)),
        "the live row said none of the things F6 allows it to say",
    );
}

/// **0.13.0 F6 — `/` writes no cursor query.**
///
/// The half no unit test can reach: `Screen::replace` re-attaches to the real
/// terminal, so a `Fixed` backend cannot see it at all, and the decision itself
/// is in `src/main.rs`, which nothing links. What this asserts is the property
/// stated as a fact about the wire — **the session asks the terminal where its
/// cursor is when it attaches and never again** — paired with a positive in the
/// same slice, because an absence assertion passes just as happily against a
/// capture where nothing happened.
///
/// Up to 0.12.0 this test asserted the opposite: `ESC[6n` after the palette
/// closed was how it proved the viewport had been re-placed. That round trip is
/// what 0.13.0 removes, on the fastest thing an operator does.
///
/// No provider is asked for anything here: the palette opens at an empty prompt.
#[tokio::test]
#[ignore]
async fn live_f6_the_palette_opens_without_asking_the_terminal_anything() {
    let (config, dir) = configured_workspace();
    // Closed by **choosing**, which the criterion allows and which a pty can
    // send unambiguously. A bare `Esc` cannot be driven reliably from here: sent
    // alone it sits in crossterm's parser until another byte arrives, and the
    // next keystroke then reads as `Alt+<that key>` — so the palette stayed open
    // in one run out of two. `Esc` is asserted where it can be, in
    // `tests/palette.rs`, against the picker itself.
    //
    // The row chosen is `/exit`, so this also proves F9 in the running binary:
    // the palette puts the command in the composer and the `Enter` after it
    // leaves.
    let text = captured(
        "live-palette.raw",
        config.path(),
        dir.path(),
        "0\twait:for commands\n0.3\traw:/\n0\twait:Which command?\n\
         0.3\traw:exi\n0.3\traw:\\r\n0\twait:/exit\n0.5\traw:\\r\n",
    );

    // Everything the session wrote after its first painted frame: the palette
    // opening, the query typed into it, the choice, the close, and the exit. The
    // attach that happens before this anchor is the one query io is allowed.
    let after = text
        .find("for commands")
        .map(|at| &text[at..])
        .expect("the session drew its status line");

    // **The positive first**, because what follows is an absence and an absence
    // holds trivially against a capture where nothing was drawn.
    assert!(
        after.contains("Which command?"),
        "the palette never painted, so the assertion below is about nothing: {after:?}",
    );
    // Escapes stripped and whitespace dropped before comparing. A row is drawn
    // with cursor moves inside it — `copy`, a jump, then `diff` — so a literal
    // `contains("copy diff")` asks the terminal for a space it had no reason to
    // write, and even the squashed form has an escape sequence in the middle.
    let squashed: String = strip_escapes(after)
        .chars()
        .filter(|c| !c.is_whitespace())
        .collect();
    // The rows the viewport has — not every command any more. What is below the
    // fold is reached by typing, which this capture then does.
    let drawn = io_cli::commands::COMMANDS
        .iter()
        .filter(|(name, _)| {
            let label: String = name
                .strip_prefix('/')
                .expect("a command")
                .chars()
                .filter(|c| !c.is_whitespace())
                .collect();
            squashed.contains(&label)
        })
        .count();
    assert!(
        drawn >= 3,
        "the palette drew {drawn} of its command rows, which is not an open palette",
    );

    // **And it asked the terminal nothing to do it.** One query per process, at
    // the attach before this slice began; a `/` that re-placed the viewport would
    // put another one here, and another on the way out.
    assert!(
        !after.contains("\x1b[6n"),
        "`/` asked the terminal where its cursor is, which is the round trip 0.13.0 removes",
    );
    // The footer's own key hint, which only the session draws: a picker draws no
    // footer at all, so its presence after the palette is the session back.
    let closed = after
        .rfind("Which command?")
        .map(|at| &after[at..])
        .expect("the palette drew its title");
    assert!(
        closed.contains("for commands"),
        "the session's status line did not come back: {closed:?}",
    );
    // And the binary left on its own, rather than being killed holding the
    // terminal: the `Enter` on the `/exit` the palette put in the composer.
    assert!(
        closed.contains("\x1b[?2004l"),
        "the terminal was never handed back: {closed:?}",
    );
}

/// What the system prompt costs, measured on the wire rather than estimated.
///
/// Wraps the real provider, forwards the request untouched, and keeps the size of
/// the system prompt it carried beside the `prompt_tokens` the vendor billed for
/// it. Two runs of one question — one contract with io-cli's prompt, one built
/// the way 0.12.0 built it — is the difference the release record carries.
struct Measured<'a, P> {
    inner: &'a P,
    seen: Arc<Mutex<Vec<(usize, u64)>>>,
}

impl<P: io_harness::Provider + Sync> io_harness::Provider for Measured<'_, P> {
    async fn complete(
        &self,
        request: io_harness::provider::CompletionRequest,
    ) -> io_harness::Result<io_harness::provider::CompletionResponse> {
        let bytes = request.system.len();
        let response = self.inner.complete(request).await?;
        let prompt_tokens = response
            .usage
            .as_ref()
            .map_or(0, |usage| usage.prompt_tokens);
        self.seen
            .lock()
            .expect("not poisoned")
            .push((bytes, prompt_tokens));
        Ok(response)
    }
}

/// **The number the release record quotes.** Not an assertion about a size — a
/// measurement, printed, of one question asked twice against one model.
#[tokio::test]
#[ignore = "live: needs OPENROUTER_API_KEY"]
async fn live_what_the_system_prompt_costs_on_one_model() {
    let key = key();
    let provider = io_harness::OpenRouter::new(&key, model());
    let policy = workspace_policy();

    for (label, prompt) in [("0.12.0 (builtin)", false), ("0.13.0 (appended)", true)] {
        let dir = tempfile::tempdir().expect("a workspace");
        let root = dir.path();
        let store = Store::open(root.join("runs.db")).expect("a store");
        let mut session = Session::open(&store, root).expect("a session");

        let (answerer, _questions) = io_cli::intent::channel();
        let responder: Arc<dyn io_harness::Responder> = Arc::new(answerer);
        let contract = match prompt {
            true => io_cli::contract::session(
                "How are you?",
                root.to_path_buf(),
                &no_configuration(),
                &no_configuration().plugins(),
                &io_cli::contract::Capabilities::default(),
                responder,
                None,
            ),
            // 0.12.0's contract, field for field: the step cap and the responder,
            // and no prompt.
            false => io_harness::TaskContract::workspace("How are you?", root)
                .with_max_steps(io_cli::contract::MAX_STEPS)
                .with_responder(responder),
        };

        let seen = Arc::new(Mutex::new(Vec::new()));
        let measured = Measured {
            inner: &provider,
            seen: Arc::clone(&seen),
        };
        session
            .turn_bounded_observed(
                &contract,
                &measured,
                &store,
                &policy,
                &DenyAll,
                &io_harness::observe::Ignore,
            )
            .await
            .expect("the turn runs");

        for (bytes, tokens) in seen.lock().expect("not poisoned").iter() {
            println!("{label}: system prompt {bytes} bytes, {tokens} prompt tokens billed");
        }
    }
}

/// **0.13.0 F5 — the manner is visible in a real answer.**
///
/// The one thing the suite cannot see. `tests/contract.rs` proves the prompt is
/// attached, bounded and neutral, and that io-harness's own sections survive it;
/// whether it makes an answer better is a person's judgement, taken once, on the
/// reply this prints into `evidence/0.13.0/`.
///
/// **Nothing here asserts on the reply's words.** 0.4.0 paid for that lesson and
/// 0.12.0 paid for it again: identical code, same goal, same model, different
/// prose — and once a different outcome. What is asserted is what reached the
/// store. `TurnKind::Reply` is io-harness's own answer to "was this turn answered
/// rather than run": one completion, no step staged, no tool call, no approver
/// consulted. A question about how the agent is doing that reaches for a tool is
/// the defect this release exists to remove, and it is the ONE fact about the
/// prose that is durable enough to assert.
///
/// The contract is built by `io_cli::contract::session`, not by the harness's own
/// `default_contract` — which is the whole point: the prompt rides the contract,
/// so a live arm that took a session's own turn would be exercising
/// `SystemPrompt::Builtin` and passing.
#[tokio::test]
#[ignore = "live: needs OPENROUTER_API_KEY"]
async fn live_f5_an_ordinary_question_is_answered_rather_than_worked_on() {
    let key = key();
    let dir = tempfile::tempdir().expect("a workspace");
    let root = dir.path();
    std::fs::write(root.join("greeting.txt"), "hello\n").expect("the fixture file");

    let store = Store::open(root.join("runs.db")).expect("a store");
    let mut session = Session::open(&store, root).expect("a session");
    let provider = io_harness::OpenRouter::new(&key, model());
    let policy = workspace_policy();

    let (answerer, _questions) = io_cli::intent::channel();
    let contract = io_cli::contract::session(
        "How are you?",
        root.to_path_buf(),
        &no_configuration(),
        &no_configuration().plugins(),
        &io_cli::contract::Capabilities::default(),
        Arc::new(answerer),
        None,
    );

    let collected = Arc::new(Mutex::new(Vec::new()));
    let observer = Collector {
        events: Arc::clone(&collected),
    };

    let result = session
        .turn_bounded_observed(&contract, &provider, &store, &policy, &DenyAll, &observer)
        .await
        .expect("the turn runs");

    let reply = result.reply.clone().unwrap_or_default();
    println!("outcome: {:?}  kind: {:?}", result.outcome, result.kind);
    println!("--- the reply, for evidence/0.13.0/ ---\n{reply}\n---");

    assert_eq!(
        result.kind,
        io_harness::TurnKind::Reply,
        "an ordinary question reached for a tool: {:?}",
        result.outcome,
    );
    assert!(
        !reply.trim().is_empty(),
        "the turn was answered and the answer was empty",
    );

    // And no tool call happened on the wire either, which is the same claim read
    // off the events rather than off the result — the two disagreeing would mean
    // the interface and the store had different accounts of one turn.
    let events = collected.lock().expect("not poisoned").clone();
    let calls = events
        .iter()
        .filter(|event| matches!(event.kind, EventKind::ToolCall { .. }))
        .count();
    assert_eq!(calls, 0, "the turn made {calls} tool calls");
}

/// 0.23.0's headline, against a real model: a run that stopped to ask is answered
/// afterwards and carries on from the step it stopped at.
///
/// **Nothing in the offline suite can assert this.** `tests/resume.rs` proves the
/// arithmetic against a seeded store, but "the answer reached the agent and the
/// run went on to do the work" needs a provider — and it is the sentence the
/// whole release is written around, so it is asserted here or nowhere.
///
/// The pause is produced honestly rather than by inserting a row: the responder's
/// receiver is dropped, which is how io-harness is told nobody is here to answer,
/// so the harness persists the question and ends the run at `AwaitingAnswer`
/// exactly as it does for a headless `io exec`. Whether the model asks at all is
/// still the model's choice, so the assertions are conditional in the shape
/// `live_f1_a_question_is_answered_on_an_uncontained_turn` already uses — and
/// what is unconditional is that this crate never drives a run it should not.
#[tokio::test]
#[ignore = "live: needs OPENROUTER_API_KEY"]
async fn live_f2_a_parked_question_is_answered_and_the_run_carries_on() {
    use io_cli::contract::Capabilities;
    use io_cli::resume::{self, Pending};

    let key = key();
    let dir = tempfile::tempdir().expect("a workspace");
    let root = dir.path();
    std::fs::write(
        root.join("notes.md"),
        "# notes\n\nold line\n\n## archive\n\nold line\n",
    )
    .expect("the fixture file");

    let store = Store::open(root.join("runs.db")).expect("a store");
    let mut session = Session::open(&store, root).expect("a session");
    let provider = io_harness::OpenRouter::new(&key, model());
    let policy = workspace_policy();
    let collected = Arc::new(Mutex::new(Vec::new()));
    let observer = Collector {
        events: Arc::clone(&collected),
    };

    // **The receiver is dropped on purpose.** `Answerer::answer` awaits a reply
    // that can never come and resolves `None`, which is io-harness's own signal
    // that nobody can answer — so the question is written to `pending_questions`
    // and the run ends parked. This is the `io exec` shape, reproduced here
    // because it is the state a resume has to start from.
    let (answerer, parked) = io_cli::intent::channel();
    drop(parked);
    let goal = "notes.md contains the line `old line` twice. Replace exactly one of them with \
                `new line`. If it is not clear which one is meant, ask before editing.";
    let contract = io_cli::contract::session(
        goal,
        root.to_path_buf(),
        &no_configuration(),
        &no_configuration().plugins(),
        &Capabilities::default(),
        Arc::new(answerer),
        None,
    )
    .with_max_steps(12);

    let result = session
        .turn_bounded_observed(
            &contract,
            &provider,
            &store,
            &policy,
            &io_harness::ApproveAll,
            &observer,
        )
        .await
        .expect("the turn runs");

    let run_id = result.run_id;
    let pending = resume::pending_for(&store, run_id).expect("the store answers");
    let Pending::Question { question_id, .. } = pending else {
        // The model chose to edit rather than ask, which is a legal answer to
        // this goal. Nothing to resume, and the release is not falsified by a
        // model being decisive — but say so, so a run of this suite that never
        // exercised the path cannot read as one that did.
        eprintln!("live_f2: the model did not ask; nothing was parked to resume ({pending:?})");
        return;
    };

    // Read **before** the resume, because that is the only moment it means
    // anything: afterwards it is the step the resumed run reached.
    let before = store.last_step(run_id).expect("the last committed step");
    let expected_head = session.head();

    let resumed = resume::answer_question(
        &contract,
        &provider,
        &store,
        run_id,
        question_id,
        "the one under the archive heading",
        &policy,
        &io_harness::ApproveAll,
        None,
        &observer,
        expected_head,
    )
    .await
    .expect("the parked run is answered and driven");

    assert_eq!(
        resumed.resumed_after, before,
        "the resume must carry on from the step the run stopped at, not from the beginning",
    );

    // The harness's own markers, read back out of the store rather than trusted:
    // one `resume` for the continuation and one `skipped` per step already
    // committed. This is what distinguishes a resume from a re-run.
    let markers = store
        .checkpoint_events(run_id)
        .expect("the checkpoint events");
    let resumes = markers
        .iter()
        .filter(|event| event.kind == "resume")
        .count();
    let skipped = markers
        .iter()
        .filter(|event| event.kind == "skipped")
        .count();
    assert_eq!(resumes, 1, "exactly one resume marker, got {resumes}");
    assert_eq!(
        skipped, before as usize,
        "one skipped marker per committed step: {before} committed, {skipped} skipped",
    );

    // And the session bookkeeping a free resume does not do. A turn left open
    // here is the failure that shows up weeks later as a missing turn.
    let turn_id = resumed.turn_id.expect("a session turn was closed");
    let turn = store
        .session_turn(turn_id)
        .expect("the store answers")
        .expect("the turn is held");
    assert!(
        turn.outcome.is_some(),
        "the resumed turn must be closed, not left reading awaiting_answer",
    );
    // Unchanged, and that is the correct answer rather than a weak one: the turn
    // already existed and the head already pointed at it — `Session::drive` closes
    // both even for a run that paused. What the resume had to do was write the
    // head *conditionally*, and a write that refused would have returned
    // `Failure::HeadMoved` above rather than reaching here.
    assert_eq!(
        session.head(),
        expected_head,
        "the resumed turn is still the head this process believed in",
    );

    // The answer must have reached the model, which is the whole point and the
    // one thing only a live run can say.
    let events = collected.lock().expect("not poisoned");
    let steps = events
        .iter()
        .filter(|event| matches!(event.kind, EventKind::Step { .. }))
        .count();
    assert!(
        steps > 0,
        "the resumed run drove no step, so the answer reached nothing",
    );
}

/// The `[app.io-cli.gates]` section, built through the user scope.
///
/// **`Config::from_toml` stopped being usable here at io-harness 0.74.0**, and the
/// live suite is where that was found — every offline binary had already been
/// migrated, and this file was not among them because `cargo test` never runs it.
/// `from_toml` hard-codes `Scope::Project`, where a `[[provider]]` is now refused
/// outright, and a rubric judged by a second model has to name one.
///
/// So it goes through the user scope like every other fixture in this repository,
/// which does mean an environment variable and therefore a lock — taken and
/// released inside `support::user_scope`, which is why no caller here holds one.
fn gated(section: &str) -> Config {
    support::user_scope(section).config.clone()
}

/// **0.24.0 F2 — a command criterion that passes makes the run `Success`.**
///
/// The whole release in one assertion. Until now every contract this crate built
/// carried `Verification::None`, so `Finished` was the only thing a clean run
/// could return and `Success` was unreachable from this interface. A criterion the
/// operator wrote is what makes the difference, and the run has to come back
/// saying so.
///
/// `true` and `false` are used as the gate programs because they are the only
/// commands whose exit status is their entire contract. The policy is permissive
/// so that `Act::Exec` admits them — a refused program is a different fact, and
/// `live_f2_a_refused_gate_program_is_not_a_failing_gate` is where that belongs.
#[tokio::test]
#[ignore = "live: needs OPENROUTER_API_KEY"]
#[cfg(unix)]
async fn live_f2_a_command_criterion_that_passes_makes_the_run_succeed() {
    let key = key();
    let dir = tempfile::tempdir().expect("a workspace");
    let root = dir.path();
    std::fs::write(root.join("notes.md"), "# notes\n\nold line\n").expect("the fixture file");

    let store = Store::open(root.join("runs.db")).expect("a store");
    let mut session = Session::open(&store, root).expect("a session");
    let provider = io_harness::OpenRouter::new(&key, model());

    let config = gated("[app.io-cli.gates]\ncommand = [\"true\"]\n");
    let contract = io_cli::contract::configured(
        "Replace the line `old line` in notes.md with `new line`. Nothing else.",
        root.to_path_buf(),
        &config,
        &config.plugins(),
    )
    .with_max_steps(12);

    // The criterion actually reached the contract. Asserted before the run,
    // because a run that succeeded with no criterion on it would look identical
    // from the outcome alone — which is exactly the confusion this release exists
    // to end.
    assert!(
        matches!(
            contract.verify,
            io_harness::Verification::Command { ref argv, expect_exit }
                if argv == &vec!["true".to_string()] && expect_exit == 0
        ),
        "the configured criterion did not reach the contract: {:?}",
        contract.verify,
    );

    let result = session
        .turn_bounded_observed(
            &contract,
            &provider,
            &store,
            &Policy::permissive(),
            &io_harness::ApproveAll,
            &io_harness::Ignore,
        )
        .await
        .expect("the turn runs");

    println!("live 0.24.0 F2: outcome {:?}", result.outcome);

    assert!(
        matches!(result.outcome, io_harness::RunOutcome::Success { .. }),
        "a passing criterion should return Success, not {:?} — if this is Finished \
         the criterion never ran",
        result.outcome,
    );

    let attempts = store
        .gate_attempts(result.run_id)
        .expect("the gate attempts");
    println!("live 0.24.0 F2: {} gate attempt(s)", attempts.len());
    assert!(
        io_cli::gates::standing(&attempts)
            .is_some_and(|standing| matches!(standing.outcome, io_harness::GateOutcome::Passed)),
        "the store should record a passing gate attempt, found {attempts:?}",
    );

    // And the exit status a headless run would have reported is unchanged by a
    // gate that passed.
    assert_eq!(
        io_cli::exec::verified_code(&result.outcome, io_cli::gates::standing(&attempts).as_ref()),
        io_cli::exec::OK,
    );
}

/// **0.24.0 F2 + F8 — a failing criterion is recorded, and it is exit `6`.**
///
/// The negative half, and the one that pays for the release: without it an
/// unattended run exits `0` over work nothing checked. `false` cannot be satisfied
/// by any amount of agent effort, so this asserts the gate's verdict rather than
/// the model's competence — the run is allowed to do whatever it likes with the
/// file and still must not be reported as verified.
#[tokio::test]
#[ignore = "live: needs OPENROUTER_API_KEY"]
#[cfg(unix)]
async fn live_f2_f8_a_failing_criterion_is_recorded_and_exits_six() {
    let key = key();
    let dir = tempfile::tempdir().expect("a workspace");
    let root = dir.path();
    std::fs::write(root.join("notes.md"), "# notes\n\nold line\n").expect("the fixture file");

    let store = Store::open(root.join("runs.db")).expect("a store");
    let mut session = Session::open(&store, root).expect("a session");
    let provider = io_harness::OpenRouter::new(&key, model());

    let config = gated("[app.io-cli.gates]\ncommand = [\"false\"]\n");
    let contract = io_cli::contract::configured(
        "Replace the line `old line` in notes.md with `new line`. Nothing else.",
        root.to_path_buf(),
        &config,
        &config.plugins(),
    )
    .with_max_steps(6);

    let result = session
        .turn_bounded_observed(
            &contract,
            &provider,
            &store,
            &Policy::permissive(),
            &io_harness::ApproveAll,
            &io_harness::Ignore,
        )
        .await
        .expect("the turn runs");

    println!("live 0.24.0 F8: outcome {:?}", result.outcome);

    assert!(
        !matches!(result.outcome, io_harness::RunOutcome::Success { .. }),
        "a criterion that cannot pass must never return Success",
    );

    let attempts = store
        .gate_attempts(result.run_id)
        .expect("the gate attempts");
    let standing = io_cli::gates::standing(&attempts).expect("a gate ran and answered");
    println!(
        "live 0.24.0 F8: phase {:?}, outcome {:?}, attempt {}",
        standing.phase, standing.outcome, standing.attempt,
    );
    assert_eq!(standing.phase, "command");
    assert!(matches!(standing.outcome, io_harness::GateOutcome::Failed));

    // The point of the release: whatever the run itself reported, the operator's
    // own criterion decides the status a script branches on.
    assert_eq!(
        io_cli::exec::verified_code(&result.outcome, Some(&standing)),
        io_cli::exec::UNVERIFIED,
        "a run whose gate said no must exit 6, not {}",
        io_cli::exec::code(&result.outcome),
    );

    // And the retry has something real to say. An empty failure text would make
    // the follow-up turn no better informed than the first, which is the whole
    // reason io-cli drives one at all.
    let events = store.sandbox_events(result.run_id).expect("sandbox events");
    let step = attempts.last().map(|last| last.step).unwrap_or_default();
    println!(
        "live 0.24.0 F7: recorded output {:?}",
        io_cli::gates::output(&events, step),
    );
}

/// **0.24.0 F5 — a rubric is judged by a second model, and the verdict arrives.**
///
/// The one criterion that costs a provider call, and the only one whose verdict
/// reaches the event stream at all: `EventKind::Reviewed` is emitted for a review
/// and for nothing else. The rubric is written so that a correct run passes it and
/// the judgement is still a real one — a rubric no model could satisfy would prove
/// only that the reviewer answered.
///
/// The reviewer is named explicitly and is the same model here, with
/// `allow_self_review` set, because this asserts the mechanism rather than the
/// independence. An operator who does not set it is refused, which is asserted in
/// the offline suite.
#[tokio::test]
#[ignore = "live: needs OPENROUTER_API_KEY"]
async fn live_f5_a_rubric_is_judged_by_a_second_model() {
    let key = key();
    let dir = tempfile::tempdir().expect("a workspace");
    let root = dir.path();
    std::fs::write(root.join("notes.md"), "# notes\n\nold line\n").expect("the fixture file");

    let store = Store::open(root.join("runs.db")).expect("a store");
    let mut session = Session::open(&store, root).expect("a session");
    let provider = io_harness::OpenRouter::new(&key, model());

    // The provider block is what `contract::configured` builds the reviewer from,
    // so the section carries both. The key reaches the file only in memory.
    let config = gated(&format!(
        "[[provider]]\nkind = \"openrouter\"\nmodel = \"{model}\"\napi_key = \"{key}\"\n\n\
         [app.io-cli.gates]\nrubric = \"notes.md contains the line `new line`\"\n\
         reviewer = \"{model}\"\nallow_self_review = true\n",
        model = model(),
    ));

    let contract = io_cli::contract::configured(
        "Replace the line `old line` in notes.md with `new line`. Nothing else.",
        root.to_path_buf(),
        &config,
        &config.plugins(),
    )
    .with_max_steps(12);

    assert!(
        matches!(contract.verify, io_harness::Verification::Review { .. }),
        "the rubric did not reach the contract: {:?}",
        contract.verify,
    );
    assert!(
        contract.reviewer.is_some(),
        "a Review criterion with no reviewer is Error::Config at run start",
    );

    let collected = Arc::new(Mutex::new(Vec::new()));
    let observer = Collector {
        events: Arc::clone(&collected),
    };

    let result = session
        .turn_bounded_observed(
            &contract,
            &provider,
            &store,
            &Policy::permissive(),
            &io_harness::ApproveAll,
            &observer,
        )
        .await
        .expect("the turn runs");

    println!("live 0.24.0 F5: outcome {:?}", result.outcome);

    let events = collected.lock().expect("not poisoned").clone();
    let reviewed: Vec<_> = events
        .iter()
        .filter_map(|event| match &event.kind {
            EventKind::Reviewed { passed, reasons } => Some((*passed, reasons.clone())),
            _ => None,
        })
        .collect();
    println!("live 0.24.0 F5: reviewed {reviewed:?}");

    assert!(
        !reviewed.is_empty(),
        "a review criterion must emit EventKind::Reviewed; the second model was \
         never asked",
    );

    // A refusal has to carry its reasons, because the reasons are the only thing
    // the operator can act on and the only thing the retry can carry.
    for (passed, reasons) in &reviewed {
        if !passed {
            assert!(
                !reasons.is_empty(),
                "a failing review with no reasons tells the operator nothing",
            );
        }
    }

    let attempts = store
        .gate_attempts(result.run_id)
        .expect("the gate attempts");
    let standing = io_cli::gates::standing(&attempts).expect("the review answered");
    assert_eq!(standing.phase, "review");
}

// ---------------------------------------------------------------------------
// 0.25.0 — the git surface.
// ---------------------------------------------------------------------------

/// Build a real repository at `root`, on `main`, with one committed file.
///
/// A real `.git`, written by git itself, because [`io_cli::repo`] parses the
/// format git actually writes and a fixture that mimics it would only prove the
/// mimicry. This is the one place in this file that shells out, and it is a test
/// fixture rather than product code — the crate itself starts no process, which
/// `tests/dependencies.rs` asserts by path.
fn a_repository(root: &std::path::Path) {
    let git = |args: &[&str]| {
        let out = std::process::Command::new("git")
            .args(args)
            .current_dir(root)
            .output()
            .expect("git runs");
        assert!(
            out.status.success(),
            "git {args:?} failed: {}",
            String::from_utf8_lossy(&out.stderr),
        );
    };
    git(&["init", "--initial-branch=main"]);
    git(&["config", "user.email", "fixture@io-cli.invalid"]);
    git(&["config", "user.name", "fixture"]);
    std::fs::write(root.join("notes.md"), "# notes\n\nold line\n").expect("the fixture file");
    git(&["add", "notes.md"]);
    git(&["commit", "-m", "fixture: the file the agent will change"]);
}

/// **F8 — a `worktree = true` child works in a checkout of its own.**
///
/// The rooting happens inside io-harness, before the child's first step, off a
/// roster io-cli passes in — and there is no public reader for a child's root, so
/// this asserts on the filesystem the harness created rather than on a store row.
/// That is the honest assertion available: `.worktrees/<slug>` exists and holds a
/// checkout, and the parent is still on `main`.
#[tokio::test]
#[ignore = "live: needs OPENROUTER_API_KEY"]
async fn live_f8_a_worktree_child_works_in_a_checkout_of_its_own() {
    use io_cli::contract::Capabilities;
    use io_harness::{AgentDef, Agents, Containment};

    let key = key();
    let dir = tempfile::tempdir().expect("a workspace");
    let root = dir.path();
    a_repository(root);

    let store = Store::open(root.join("runs.db")).expect("a store");
    let mut session = Session::open(&store, root).expect("a session");
    let provider = io_harness::OpenRouter::new(&key, model());
    let policy = workspace_policy();

    let (answerer, _questions) = io_cli::intent::channel();
    let roster = Agents::new().with(
        AgentDef::new("builder")
            .with_role("Edit the file you are asked to edit.")
            .with_max_steps(6)
            .with_worktree(),
    );
    let contract = io_cli::contract::session(
        "Spawn the `builder` agent and ask it to replace the line `old line` in \
         notes.md with `new line`. Wait for it.",
        root.to_path_buf(),
        &no_configuration(),
        &no_configuration().plugins(),
        &Capabilities::default(),
        Arc::new(answerer),
        None,
    )
    .with_agents(roster)
    .with_max_steps(12);

    let result = session
        .turn_contained_bounded_observed(
            &contract,
            &provider,
            &store,
            &policy,
            &io_harness::ApproveAll,
            &Containment::new(4, 2, 1, 200_000),
            &io_harness::Ignore,
        )
        .await
        .expect("the turn runs");

    let worktrees = root.join(".worktrees");
    let made: Vec<String> = std::fs::read_dir(&worktrees)
        .map(|entries| {
            entries
                .filter_map(|entry| entry.ok())
                .map(|entry| entry.file_name().to_string_lossy().into_owned())
                .collect()
        })
        .unwrap_or_default();
    println!("live 0.25.0 F8: run {} worktrees {made:?}", result.run_id);

    assert!(
        !made.is_empty(),
        "a `worktree = true` roster entry must root its child under `.worktrees/`; \
         nothing was created, so the roster did not reach the spawn",
    );

    // The child's checkout is a real one, and the parent did not move onto it.
    for name in &made {
        let child = worktrees.join(name);
        assert!(
            io_cli::repo::branch(&child).is_some(),
            "the child's checkout at {child:?} has no readable head, so it is a \
             directory rather than a worktree",
        );
    }
    assert_eq!(
        io_cli::repo::branch(root).as_deref(),
        Some("main"),
        "the parent must stay where it was; a child taking the parent's branch \
         with it is the overwriting this switch exists to stop",
    );
}

/// **F5 — a commit the agent made comes back with the message it wrote.**
///
/// The whole chain in one turn: the agent calls `git_commit`, the message
/// survives only in the typed call, and `commit::made_in` reads it back off
/// `Store::step_turns`. Asserted against the repository as well as the store, so
/// a block that rendered without a commit behind it fails here.
#[tokio::test]
#[ignore = "live: needs OPENROUTER_API_KEY"]
async fn live_f5_a_commit_the_agent_made_reads_back_with_its_message() {
    use io_cli::contract::Capabilities;

    let key = key();
    let dir = tempfile::tempdir().expect("a workspace");
    let root = dir.path();
    a_repository(root);

    let store = Store::open(root.join("runs.db")).expect("a store");
    let mut session = Session::open(&store, root).expect("a session");
    let provider = io_harness::OpenRouter::new(&key, model());
    let policy = workspace_policy();

    let (answerer, _questions) = io_cli::intent::channel();
    let contract = io_cli::contract::session(
        "Replace the line `old line` in notes.md with `new line`, then stage and \
         commit that change with a message describing it.",
        root.to_path_buf(),
        &no_configuration(),
        &no_configuration().plugins(),
        &Capabilities::default(),
        Arc::new(answerer),
        None,
    )
    .with_max_steps(12);

    let result = session
        .turn_bounded_observed(
            &contract,
            &provider,
            &store,
            &policy,
            &io_harness::ApproveAll,
            &io_harness::Ignore,
        )
        .await
        .expect("the turn runs");

    let turns = store
        .step_turns(result.run_id)
        .expect("the assistant turns");
    let made = io_cli::commit::made_in(&turns);
    println!("live 0.25.0 F5: commits {made:?}");

    assert!(
        !made.is_empty(),
        "the agent was asked to commit and no `git_commit` call came back off \
         `Store::step_turns`; the message survives nowhere else",
    );

    for commit in &made {
        assert!(
            !commit.subject().is_empty(),
            "a commit block with an empty subject says nothing: {commit:?}",
        );
    }

    // And the repository agrees. A block drawn from a call the policy refused
    // would pass everything above and fail here, which is why this assertion is
    // against git rather than against the store.
    let log = std::process::Command::new("git")
        .args(["log", "--oneline", "--no-decorate"])
        .current_dir(root)
        .output()
        .expect("git log runs");
    let log = String::from_utf8_lossy(&log.stdout);
    println!("live 0.25.0 F5: log\n{log}");
    assert!(
        log.lines().count() >= 2,
        "the fixture commit is one; the agent's is the second, and the log has \
         only: {log}",
    );
}

/// **F1/F2 — an asking posture ASKS about git and the approver's answer stands.**
///
/// **This arm asserted the opposite until 0.29.0, and the pin is what changed it.**
/// It was written for 0.25.0, whose premise was that io-harness refused git under
/// an asking posture *before any approver existed to be consulted* — reported as
/// io-harness#214. **0.70.0 closed that issue**, at all four sites carrying the
/// comparison, and this arm came back with `refusals []` on the first live run
/// after the pin: the approval is raised, `ApproveAll` answers it, and the spawn
/// happens.
///
/// So what it proves now is the fix rather than the defect, and it is still worth
/// a real run for the reason it always was: **only a live turn has an approver in
/// it.** `tests/policy.rs` can assert that the posture's effect is `Ask`; nothing
/// but a run can show that `Ask` is now routed to somebody and honoured.
///
/// The allowance is still asserted, because it is still what a `read only`
/// operator needs — a deny that came from a tier default, which is the one shape
/// `crate::commit::asked` will offer to lift.
#[tokio::test]
#[ignore = "live: needs OPENROUTER_API_KEY"]
async fn live_f1_f2_an_asking_posture_asks_about_git_and_the_answer_is_honoured() {
    use io_cli::contract::Capabilities;
    use io_harness::{Act, Effect};

    let key = key();
    let dir = tempfile::tempdir().expect("a workspace");
    let root = dir.path();
    a_repository(root);

    let store = Store::open(root.join("runs.db")).expect("a store");
    let mut session = Session::open(&store, root).expect("a session");
    let provider = io_harness::OpenRouter::new(&key, model());

    // The posture io-cli's own wizard recommends, and the one most operators run.
    let asking = Policy {
        layers: Policy::default().layers,
        defaults: Posture::AskWrites.defaults(),
    };
    assert_eq!(
        asking.check(Act::Exec, io_cli::approval::GIT).effect,
        Effect::Ask,
        "the recommended posture asks about git — since io-harness 0.70.0 that \
         means an approval is raised rather than a refusal returned",
    );

    // And a `read only` operator is the one the allowance is still for: a deny
    // that came from a tier default rather than from a rule, which is the only
    // shape `commit::asked` offers to lift.
    let denying = Policy {
        layers: Policy::default().layers,
        defaults: Posture::ReadOnly.defaults(),
    };
    let verdict = denying.check(Act::Exec, io_cli::approval::GIT);
    assert_eq!(verdict.effect, Effect::Deny);
    assert!(
        verdict.rule.is_none(),
        "the deny has to come from the tier default for the allowance to be \
         offerable; a deny rule can never be widened by a later layer",
    );
    let lifted = io_cli::approval::effective_policy(&denying, &[io_cli::approval::git_allowance()]);
    assert_eq!(
        lifted.check(Act::Exec, io_cli::approval::GIT).effect,
        Effect::Allow,
        "and the one rule lifts it",
    );

    let collected = Arc::new(Mutex::new(Vec::new()));
    let observer = Collector {
        events: Arc::clone(&collected),
    };

    let (answerer, _questions) = io_cli::intent::channel();
    let contract = io_cli::contract::session(
        "Run `git status` to see what has changed in this repository, then say what it said.",
        root.to_path_buf(),
        &no_configuration(),
        &no_configuration().plugins(),
        &Capabilities::default(),
        Arc::new(answerer),
        None,
    )
    .with_max_steps(6);

    let result = session
        .turn_bounded_observed(
            &contract,
            &provider,
            &store,
            &asking,
            &io_harness::ApproveAll,
            &observer,
        )
        .await
        .expect("the turn runs");

    let events = collected.lock().expect("not poisoned").clone();
    let refusals: Vec<(String, String)> = events
        .iter()
        .filter_map(|event: &RunEvent| match &event.kind {
            EventKind::Refused { act, target, .. } => Some((act.clone(), target.clone())),
            _ => None,
        })
        .collect();
    println!(
        "live 0.25.0 F1/F2: run {} refusals {refusals:?}",
        result.run_id
    );

    // **The fix, observed rather than predicted.** `ApproveAll` is handed in
    // deliberately: it answers both the `.git` write gate and — since io-harness
    // 0.70.0 — the exec approval the spawn now raises. So an `exec`/`git` refusal
    // appearing here would mean the asking posture was still being treated as a
    // hard refusal with nobody consulted, which is exactly the defect #214 closed.
    //
    // Asserted as an absence, which is weaker than asserting the spawn happened
    // and is deliberate: whether the model chooses to call a git tool at all is
    // the model's decision, and an arm that required the call would be a live
    // test that fails on a differently-worded reply. What is not the model's
    // decision is what the harness does when it *is* called, and a refusal is the
    // only way that shows up here.
    assert!(
        !refusals
            .iter()
            .any(|(act, target)| act == "exec" && target == io_cli::approval::GIT),
        "an `exec`/`git` refusal reached the observer while an approver was in \
         force. io-harness 0.70.0 routes `Effect::Ask` on `Act::Exec` to the \
         approver instead of refusing outright (#214); a refusal here means that \
         is not happening, and `App::note_git` will be explaining a wall the \
         operator was never actually shown. Got {refusals:?}",
    );
}

/// **F1 live — a reasoning level goes out on a real wire and the endpoint takes
/// it.**
///
/// The half of F1 no fixture can reach. `contract::buying` puts an `Effort` on the
/// contract and `tests/contract.rs` asserts that; what only a real request can
/// settle is that io-harness translates it into a field the vendor accepts, and
/// that a turn carrying one still finishes. A wrong spelling would be a 400 from
/// the endpoint rather than a failing assertion here, which is exactly why this arm
/// exists.
///
/// **The assertion is that the turn finishes, and that is the whole of it.** A
/// reasoning field the endpoint does not recognise is an HTTP 400 from the vendor,
/// not a failing comparison — so a turn that completes against a real endpoint
/// with `contract.effort` set is the evidence, and there is nothing further to
/// read back. io-cli's own `context::Request` deliberately does not carry the
/// field: `/context` reports what is in the window, and a reasoning level is not
/// context.
///
/// Nothing is asserted about the answer's words, for the reason every arm in this
/// file gives: identical code and the same model produce different prose.
#[tokio::test]
#[ignore = "live: needs OPENROUTER_API_KEY"]
async fn live_f1_a_reasoning_level_reaches_the_wire_and_the_turn_still_finishes() {
    let key = key();
    let dir = tempfile::tempdir().expect("a workspace");
    let root = dir.path();
    std::fs::write(root.join("notes.md"), "one line\n").expect("the fixture file");

    let store = Store::open(root.join("runs.db")).expect("a store");
    let mut session = Session::open(&store, root).expect("a session");
    let provider = io_harness::OpenRouter::new(&key, model());
    let policy = workspace_policy();

    let (answerer, _questions) = io_cli::intent::channel();
    let contract = io_cli::contract::buying(
        io_cli::contract::session(
            "What is in notes.md? Answer in one sentence.",
            root.to_path_buf(),
            &no_configuration(),
            &no_configuration().plugins(),
            &io_cli::contract::Capabilities::default(),
            Arc::new(answerer),
            None,
        ),
        Some(io_harness::Effort::Low),
    );

    assert_eq!(
        contract.effort,
        Some(io_harness::Effort::Low),
        "the contract has to carry the level before the wire can",
    );

    let result = session
        .turn_bounded_observed(
            &contract,
            &provider,
            &store,
            &policy,
            &DenyAll,
            &io_harness::Ignore,
        )
        .await
        .expect("a turn carrying a reasoning level still finishes");

    println!(
        "live 0.26.0 F1: outcome {:?} kind {:?}",
        result.outcome, result.kind
    );

    assert!(
        result
            .reply
            .as_deref()
            .is_some_and(|text| !text.trim().is_empty()),
        "a turn carrying a reasoning level produced no answer, which is what a \
         field the endpoint refuses looks like from here: {:?}",
        result.outcome,
    );
}

/// **F5 live — a primary that cannot be reached is fallen through, against a real
/// endpoint underneath.**
///
/// The other half no fixture settles. `tests/provider.rs` proves the chain's logic
/// with a fake link that fails on demand; what it cannot prove is that a *real*
/// transport failure is classified as retryable by io-harness and therefore
/// reaches the next link at all. The head here is a `Compatible` pointed at a port
/// nothing is listening on, which is the cheapest genuine `ProviderErrorKind::
/// Transport` available — no credential is spent on it, because the connection
/// never opens.
///
/// The tail is the real provider, so a pass means an answer that actually came
/// from the second link, and `last_served` names it. That name is also what
/// io-harness turns into `EventKind::FellBackTo` (`run/step.rs:503`), which is the
/// scrollback arm this release makes reachable for the first time.
#[tokio::test]
#[ignore = "live: needs OPENROUTER_API_KEY"]
async fn live_f5_an_unreachable_primary_falls_through_to_the_provider_underneath() {
    let key = key();
    let dir = tempfile::tempdir().expect("a workspace");
    let root = dir.path();

    let store = Store::open(root.join("runs.db")).expect("a store");
    let mut session = Session::open(&store, root).expect("a session");
    let policy = workspace_policy();

    // **This test needed io-harness 0.74.0's lift to keep being about its own
    // subject, and the live suite is what found that.** The floor refuses a
    // loopback endpoint before any connection is attempted, whatever the policy
    // says, so the primary failed with `Error::Refused` — which is **not**
    // retryable, so the chain stopped instead of falling through. That is correct
    // behaviour and worth stating: a boundary refusal that quietly tried the next
    // vendor would be the fall-through working around the thing that said no.
    //
    // There is no way to route around it either. The floor also refuses a name it
    // cannot resolve — "a name that will not resolve is not checkable" — so an
    // `.invalid` host is a `Refused` too, not a transport failure. To reach a
    // *transport* failure at all the address has to be one the floor permits, and
    // the only fast, deterministic one is a loopback port with nothing behind it.
    //
    // So the variable is set here, which is exactly the choice the harness means an
    // operator to make deliberately. Set and never unset: these arms share one
    // process, and taking it away mid-run would be taking it from whichever
    // sibling was mid-request. It changes nothing for them — every other arm talks
    // to a public endpoint, which the floor never touched.
    //
    // `src/` still sets it nowhere, which `f11_io_cli_never_sets_the_local_address_variable`
    // asserts: a test choosing it for itself is the operator's choice, and a
    // product choosing it for the operator is not.
    std::env::set_var("IO_HARNESS_ALLOW_LOCAL_ADDRESSES", "1");

    // A base URL on the loopback with nothing behind it. The failure is a refused
    // connection, which is `Transport` — retryable, and therefore worth another
    // vendor by io-harness's own predicate.
    let specs = vec![
        ProviderSpec::Compatible {
            model: model(),
            preset: None,
            base_url: Some("http://127.0.0.1:9/v1".into()),
            api_key: Some("unused".into()),
            auth: None,
            name: None,
            reference_prices: false,
        },
        ProviderSpec::OpenRouter {
            model: model(),
            api_key: Some(key),
        },
    ];

    struct Falling<'a> {
        session: &'a mut Session,
        store: &'a Store,
        policy: &'a Policy,
        root: std::path::PathBuf,
    }

    impl io_cli::provider::WithProvider for Falling<'_> {
        type Out = (String, Option<String>);

        async fn call<P: io_harness::Provider>(
            self,
            make: impl Fn(&str) -> Result<P, String>,
            model: String,
        ) -> Self::Out {
            let provider = make(&model).expect("the chain builds");
            let (answerer, _questions) = io_cli::intent::channel();
            let contract = io_cli::contract::session(
                "Reply with the single word: ready.",
                self.root.clone(),
                &no_configuration(),
                &no_configuration().plugins(),
                &io_cli::contract::Capabilities::default(),
                Arc::new(answerer),
                None,
            );
            let result = self
                .session
                .turn_bounded_observed(
                    &contract,
                    &provider,
                    self.store,
                    self.policy,
                    &DenyAll,
                    &io_harness::Ignore,
                )
                .await
                .expect("the second link answers");
            (
                result.reply.clone().unwrap_or_default(),
                provider.last_served(),
            )
        }
    }

    let (reply, served) = io_cli::provider::build(
        specs,
        None,
        Falling {
            session: &mut session,
            store: &store,
            policy: &policy,
            root: root.to_path_buf(),
        },
    )
    .await
    .expect("a chain of two builds");

    println!("live 0.26.0 F5: served {served:?} reply {reply:?}");

    assert!(
        !reply.trim().is_empty(),
        "the turn produced no answer, so nothing fell through to anything",
    );
    assert!(
        served.is_some(),
        "a fall-through happened and `last_served` must name the link that \
         answered — it is what io-harness turns into `EventKind::FellBackTo`",
    );
}

/// **O3 / F6 / F7 — an undo of a file a real agent actually wrote.**
///
/// The fixture tests build their restore points through io-harness's own run
/// loop with a scripted provider, which is the right instrument for asserting
/// the four answers. What no fixture can settle is whether a *real* model's
/// `write_file` produces a snapshot this crate can rewind — that depends on the
/// tool the model chooses, and a model may edit, patch or rewrite.
///
/// So this drives a real turn, asserts the file changed, then puts it back and
/// asserts the bytes on disk are the ones that were there before the agent ran.
#[tokio::test]
#[ignore = "live: needs OPENROUTER_API_KEY"]
async fn live_f6_f7_a_real_agents_write_is_put_back() {
    let key = key();
    let dir = tempfile::tempdir().expect("a workspace");
    let root = dir.path();
    let before = "the original line\n";
    std::fs::write(root.join("subject.txt"), before).expect("the fixture file");

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
            "Replace the entire contents of subject.txt with exactly the word: replaced. \
             Then say what you did in one sentence.",
            &provider,
            &store,
            &policy,
            &io_harness::ApproveAll,
            &observer,
            &inbox,
        )
        .await
        .expect("the turn runs");
    println!("outcome: {:?}", result.outcome);

    let after = std::fs::read_to_string(root.join("subject.txt")).expect("the file is there");
    println!("after the turn: {after:?}");
    assert_ne!(after, before, "the agent has to have changed something");

    let head = session.head().expect("the turn is on the head");
    let run_id = store
        .session_turn(head)
        .expect("readable")
        .expect("there")
        .run_id;

    // The undo, through the same call `/undo <path>` makes.
    let workspace = io_harness::tools::Workspace::new(root);
    let answer =
        io_cli::undo::one_file(&workspace, &store, run_id, "subject.txt").expect("the undo runs");
    println!(
        "the undo said: {}",
        io_cli::undo::said("subject.txt", &answer)
    );

    assert!(
        matches!(answer, io_harness::Rewind::Restored(_)),
        "a real agent's write leaves a restore point this crate can use: {answer:?}",
    );
    assert_eq!(
        std::fs::read_to_string(root.join("subject.txt")).expect("still there"),
        before,
        "the bytes on disk are the ones that were there before the agent ran",
    );

    // And the whole-turn form emits the event that had never fired before
    // 0.27.0 — through `rewind::last_turn`, which is the production path.
    let watcher = Collector {
        events: Arc::new(Mutex::new(Vec::new())),
    };
    let undone = io_cli::rewind::last_turn(&mut session, &store, &watcher)
        .expect("the whole-turn undo runs");
    println!("undone: {undone:?}");
    let saw = watcher
        .events
        .lock()
        .expect("not poisoned")
        .iter()
        .any(|event| matches!(event.kind, io_harness::EventKind::Rewound { .. }));
    assert!(saw, "EventKind::Rewound reached the observer on a real run");
}

/// **O3 / F8 — an export of a conversation that actually happened.**
///
/// A fixture's trace is assembled from rows a test wrote. This asserts the
/// export against a run a real model drove: the markdown carries the prompt and
/// the reply that were really exchanged, and the trace file is byte-identical to
/// what io-harness produced for that run.
#[tokio::test]
#[ignore = "live: needs OPENROUTER_API_KEY"]
async fn live_f8_a_real_conversation_exports() {
    let key = key();
    let dir = tempfile::tempdir().expect("a workspace");
    let root = dir.path();

    let store = Store::open(root.join("runs.db")).expect("a store");
    let mut session = Session::open(&store, root).expect("a session");
    let provider = io_harness::OpenRouter::new(&key, model());
    let policy = workspace_policy();
    let (_steer, inbox) = Steer::channel();
    let collected = Arc::new(Mutex::new(Vec::new()));
    let observer = Collector {
        events: Arc::clone(&collected),
    };

    let prompt = "In one short sentence, say what a canonical trace is for.";
    session
        .turn_steered(
            prompt, &provider, &store, &policy, &DenyAll, &observer, &inbox,
        )
        .await
        .expect("the turn runs");

    let markdown = io_cli::export::conversation(&store, &session)
        .expect("readable")
        .expect("a conversation that happened");
    println!("--- exported markdown ---\n{markdown}");
    assert!(
        markdown.contains(prompt),
        "the operator's own words are in the export",
    );

    let head = session.head().expect("a turn");
    let run_id = store
        .session_turn(head)
        .expect("readable")
        .expect("there")
        .run_id;

    let trace = io_cli::export::trace(&store, run_id).expect("a trace");
    assert!(
        !trace.is_empty(),
        "a run that really happened has a trace; an empty one would make the \
         byte comparison below vacuous, which is the defect this release found \
         in its own fixture",
    );

    let workspace = io_harness::tools::Workspace::new(root);
    let written = io_cli::export::write(&workspace, &io_cli::export::trace_path(run_id), &trace)
        .expect("the trace is written");
    let back = std::fs::read_to_string(root.join(&written.path)).expect("the file");
    assert_eq!(
        back,
        store.canonical_trace(run_id).expect("readable"),
        "the file on disk is io-harness's own string, byte for byte",
    );
}

/// **O3 — whichever of the two conditional events a real run emits.**
///
/// `Speculated` fires only when a provider reports finished calls and something
/// was actually started; `CacheMarked` only when a marked prefix advances.
/// Neither can be provoked, so this **reports** rather than asserts: it prints
/// what a real run emitted so the release record can say what was observed
/// rather than what was hoped for.
///
/// The one thing it does assert is that if `Speculated` arrives with something
/// discarded, the renderer draws a line for it — which is the arm 0.27.0 added.
#[tokio::test]
#[ignore = "live: needs OPENROUTER_API_KEY"]
async fn live_f9_whichever_conditional_events_a_real_run_emits() {
    let key = key();
    let dir = tempfile::tempdir().expect("a workspace");
    let root = dir.path();
    for name in ["one.txt", "two.txt", "three.txt"] {
        std::fs::write(root.join(name), format!("contents of {name}\n")).expect("a fixture file");
    }

    let store = Store::open(root.join("runs.db")).expect("a store");
    let mut session = Session::open(&store, root).expect("a session");
    let provider = io_harness::OpenRouter::new(&key, model());
    let policy = workspace_policy();
    let (_steer, inbox) = Steer::channel();
    let collected = Arc::new(Mutex::new(Vec::new()));
    let observer = Collector {
        events: Arc::clone(&collected),
    };

    session
        .turn_steered(
            "Read one.txt, two.txt and three.txt, then tell me in one sentence what \
             they have in common.",
            &provider,
            &store,
            &policy,
            &DenyAll,
            &observer,
            &inbox,
        )
        .await
        .expect("the turn runs");

    let events = collected.lock().expect("not poisoned").clone();
    let mut drew = 0usize;
    for event in &events {
        match &event.kind {
            io_harness::EventKind::Speculated {
                started,
                used,
                discarded,
            } => {
                println!("live: Speculated started={started} used={used} discarded={discarded}");
                if *discarded > 0 {
                    let mut renderer = io_cli::events::Events::new(io_cli::theme::DARK);
                    let lines = renderer.event(event, std::time::Duration::ZERO);
                    assert!(
                        !lines.is_empty(),
                        "a real discarded read must draw the line 0.27.0 added",
                    );
                    drew += 1;
                }
            }
            io_harness::EventKind::CacheMarked {
                through_step,
                prefix_bytes,
            } => println!("live: CacheMarked through_step={through_step} bytes={prefix_bytes}"),
            _ => {}
        }
    }
    println!(
        "live: drew {drew} speculation line(s) from {} events",
        events.len()
    );
}
