//! F1, F2 and F4 — the approver seam, and the overlay drawn on it.
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

mod support;

use io_harness::{Act, ApprovalContext, Approver, Decision, Request};

use io_cli::app::App;
use io_cli::approval::{self, Approval, Ask};
use io_cli::theme::DARK;

fn write() -> Request {
    Request::new(Act::Write, "src/main.rs").with_content("fn main() {}\n")
}

/// A question already delivered to the interface, and the task still waiting on
/// the answer. The waiting task is handed back rather than detached so a test can
/// assert what the run was finally told.
async fn asked(
    request: Request,
    context: ApprovalContext,
) -> (Ask, tokio::task::JoinHandle<Decision>) {
    let (asker, mut asks) = approval::channel();
    let deciding = tokio::spawn(async move {
        let asker = asker;
        asker.decide_in_context(&request, &context).await
    });
    let ask = asks
        .recv()
        .await
        .expect("the question reached the interface");
    (ask, deciding)
}

/// The write the overlay tests are drawn from: an act, a target, the rule that
/// flagged it and the layer that rule came from.
fn flagged() -> (Request, ApprovalContext) {
    (
        write(),
        ApprovalContext::new("tidy the parser")
            .flagged_by(Some("src/*.rs".into()), Some("app".into())),
    )
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

/// **F1.** A question that has been committed to scrollback is a question that
/// can be scrolled away from a run which is blocked on it. So the overlay lives
/// in the viewport, and the transcript gains nothing at all while it is open.
#[tokio::test]
async fn f1_an_open_approval_commits_nothing_to_scrollback() {
    let (request, context) = flagged();
    let (ask, deciding) = asked(request, context).await;

    let mut app = App::new(DARK, "opus-5");
    app.open_approval(ask);

    assert!(
        app.take_pending().is_empty(),
        "an open approval must not commit a line to the terminal's scrollback",
    );

    let (mut screen, recorder) = support::screen(80, 24);
    screen
        .draw(|frame| app.render(frame, frame.area()))
        .expect("frame");
    assert!(
        screen.viewport_text().contains("src/main.rs"),
        "the question must be in the viewport: {:?}",
        screen.viewport_text(),
    );

    // `insert_before` is how a line reaches scrollback, and it moves the cursor up
    // before writing. Nothing of the sort should have been written for a question.
    let bytes = recorder.text();
    assert!(
        !bytes.contains("src/main.rs\r\n"),
        "the question was committed to scrollback: {bytes:?}",
    );

    app.answer_approval(io_cli::approval::Answer::Deny);
    deciding.await.expect("the approver did not panic");
}

/// **F2.** Asserted on positions rather than on presence. A `contains` assertion
/// passes just as happily when the sentence is inside out, which 0.1.1 paid to
/// learn: a sabotage that moved a token count in front of a decision left the
/// membership assertion green.
#[tokio::test]
async fn f2_the_overlay_states_act_then_target_then_rule_then_layer() {
    let (request, context) = flagged();
    let (ask, deciding) = asked(request, context).await;

    let approval = Approval::new(ask);
    let (mut screen, _recorder) = support::screen(80, 24);
    screen
        .draw(|frame| approval.render(frame, frame.area(), &DARK))
        .expect("frame");

    let text = screen.viewport_text();
    let at = |needle: &str| {
        text.find(needle)
            .unwrap_or_else(|| panic!("{needle:?} is not on screen: {text:?}"))
    };

    assert!(
        at("write") < at("src/main.rs"),
        "act before target: {text:?}"
    );
    assert!(
        at("src/main.rs") < at("src/*.rs"),
        "target before the rule: {text:?}",
    );
    assert!(
        at("src/*.rs") < at("app"),
        "rule before the layer: {text:?}"
    );

    approval.answer(approval::Answer::Deny);
    deciding.await.expect("the approver did not panic");
}

/// The other half of F2, and the one io-harness's own documentation warns about:
/// `rule` and `layer` are `None` when the *tier default* decided, which is the
/// least vouched-for kind of action rather than the most. A surface that renders
/// nothing there tells the reader the opposite of what happened.
#[tokio::test]
async fn f2_an_unnamed_action_says_the_tier_default_decided() {
    let (ask, deciding) = asked(write(), ApprovalContext::new("tidy the parser")).await;

    let approval = Approval::new(ask);
    let (mut screen, _recorder) = support::screen(80, 24);
    screen
        .draw(|frame| approval.render(frame, frame.area(), &DARK))
        .expect("frame");

    let text = screen.viewport_text();
    assert!(
        text.contains("tier default"),
        "an unnamed action must say so rather than showing a blank: {text:?}",
    );

    approval.answer(approval::Answer::Deny);
    deciding.await.expect("the approver did not panic");
}

/// The proposed content is what makes an answer an informed one, and it is the
/// only part that can be arbitrarily long. It is fitted into the rows that are
/// left and says how many it did not show.
#[tokio::test]
async fn the_overlay_shows_the_proposed_content_and_says_what_it_cut() {
    let long: String = (0..40).map(|n| format!("line {n}\n")).collect();
    let (ask, deciding) = asked(
        Request::new(Act::Write, "src/main.rs").with_content(long),
        ApprovalContext::new("tidy the parser"),
    )
    .await;

    let approval = Approval::new(ask);
    let (mut screen, _recorder) = support::screen(80, 24);
    screen
        .draw(|frame| approval.render(frame, frame.area(), &DARK))
        .expect("frame");

    let text = screen.viewport_text();
    assert!(text.contains("line 0"), "the content is shown: {text:?}");
    assert!(
        text.contains("more lines"),
        "what was cut has to be said, or the reader is approving a file they think they read: {text:?}",
    );

    approval.answer(approval::Answer::Deny);
    deciding.await.expect("the approver did not panic");
}
