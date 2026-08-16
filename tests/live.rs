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
        committed.extend(renderer.event(event));
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

    // The prompt asks for a sentence on purpose. A turn that is nothing but tool
    // calls emits no assistant text and therefore no tokens, which is the model's
    // choice rather than a property of this interface — the first version of this
    // test asserted on a purely mechanical edit and failed for that reason.
    assert!(
        events
            .iter()
            .any(|event| matches!(event.kind, EventKind::Token { .. })),
        "the turn should have streamed tokens; without them nothing appears live",
    );
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
