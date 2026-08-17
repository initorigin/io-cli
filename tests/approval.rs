//! F4 — the approver seam, and the one way it must not fail.
//!
//! Every other seam in this product is one-way: an observer is handed an event
//! and hands it on. This one runs on the agent's own task and *blocks it* until a
//! person answers, which makes "nobody answered" a state the run can be left in
//! forever. So the criterion is not that an answer arrives — it is what happens
//! when one never does.
//!
//! There is no clock in this file, and there must not be. A deadlock asserted
//! with a timeout is a test that passes on a fast machine and fails on a loaded
//! one; asserted on a closed channel it is a fact.

use io_harness::{Act, Approver, Decision, Request};

use io_cli::approval::{self, Ask};

fn write() -> Request {
    Request::new(Act::Write, "src/main.rs").with_content("fn main() {}\n")
}

/// The interface is gone entirely — the process is exiting, or the receiver was
/// dropped. The run must be told no, not left waiting on a channel with no other
/// end.
#[tokio::test]
async fn a_missing_interface_denies() {
    let (asker, asks) = approval::channel();
    drop(asks);

    let decision = asker.decide(&write()).await;

    assert!(
        matches!(decision, Decision::Deny { .. }),
        "a dropped receiver must deny, not hang: {decision:?}"
    );
}

/// The subtler half, and the one a real session hits: the interface is alive, it
/// received the question, and then it went away without answering — a `Ctrl+C`
/// during an overlay, a panic, a resize that unwound. The oneshot closes rather
/// than the mpsc, which is a different failure and the same answer.
#[tokio::test]
async fn an_abandoned_question_denies() {
    let (asker, mut asks) = approval::channel();

    let deciding = tokio::spawn(async move {
        let asker = asker;
        asker.decide(&write()).await
    });

    let ask: Ask = asks
        .recv()
        .await
        .expect("the question reached the interface");
    drop(ask);

    let decision = deciding.await.expect("the approver did not panic");
    assert!(
        matches!(decision, Decision::Deny { .. }),
        "an abandoned question must deny, not hang: {decision:?}"
    );
}

/// The seam works in the ordinary direction too, and the answer is the operator's
/// own rather than one the seam invented on the way through.
#[tokio::test]
async fn an_answer_reaches_the_run() {
    let (asker, mut asks) = approval::channel();

    let deciding = tokio::spawn(async move {
        let asker = asker;
        asker.decide(&write()).await
    });

    let ask = asks
        .recv()
        .await
        .expect("the question reached the interface");
    ask.answer(Decision::approve());

    let decision = deciding.await.expect("the approver did not panic");
    assert!(
        matches!(decision, Decision::Approve { .. }),
        "the operator's answer must be the run's answer: {decision:?}"
    );
}

/// What the interface is given to render. The rule and the layer do not travel on
/// the event stream — `ApprovalRequested` carries only the act and the target —
/// so if they do not arrive here they cannot be shown at all, which is the whole
/// surface F2 asserts.
#[tokio::test]
async fn the_question_carries_what_the_overlay_has_to_show() {
    use io_harness::ApprovalContext;

    let (asker, mut asks) = approval::channel();
    let context = ApprovalContext::new("tidy the parser")
        .flagged_by(Some("src/*.rs".into()), Some("app".into()));

    let deciding = tokio::spawn({
        let context = context.clone();
        async move {
            let asker = asker;
            asker.decide_in_context(&write(), &context).await
        }
    });

    let ask = asks
        .recv()
        .await
        .expect("the question reached the interface");
    assert_eq!(ask.act(), Act::Write);
    assert_eq!(ask.target(), "src/main.rs");
    assert_eq!(ask.content(), Some("fn main() {}\n"));
    assert_eq!(ask.rule(), Some("src/*.rs"));
    assert_eq!(ask.layer(), Some("app"));

    ask.answer(Decision::deny("not this time"));
    deciding.await.expect("the approver did not panic");
}
