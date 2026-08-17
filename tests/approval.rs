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

use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

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

/// **F3.** Each key is its own answer, the run is told exactly that, and the
/// transcript gains exactly one line saying so — so a decision is in the terminal's
/// own scrollback as well as in the harness's durable trace.
#[tokio::test]
async fn f3_each_answer_reaches_the_run_as_itself_and_commits_one_line() {
    for (key, expected) in [
        ('y', approval::Answer::Once),
        ('a', approval::Answer::Session),
        ('n', approval::Answer::Deny),
    ] {
        let (request, context) = flagged();
        let (ask, deciding) = asked(request, context).await;

        let mut app = App::new(DARK, "opus-5");
        app.open_approval(ask);
        assert!(app.asking(), "the overlay is up before the key");

        app.key(KeyEvent::new(KeyCode::Char(key), KeyModifiers::NONE));
        assert!(!app.asking(), "answering closes the overlay: {key:?}");

        let decision = deciding.await.expect("the approver did not panic");
        match (expected, &decision) {
            (approval::Answer::Once, Decision::Approve { remember, .. }) => {
                assert!(
                    remember.is_empty(),
                    "allow ONCE must remember nothing: {remember:?}",
                );
            }
            (approval::Answer::Session, Decision::Approve { remember, .. }) => {
                assert_eq!(
                    remember.len(),
                    1,
                    "allow this session must hand the run one rule: {remember:?}",
                );
                assert_eq!(remember[0].act, Act::Write);
                assert_eq!(remember[0].pattern, "src/main.rs");
            }
            (approval::Answer::Deny, Decision::Deny { .. }) => {}
            (expected, got) => panic!("{key:?} should be {expected:?}, got {got:?}"),
        }

        let committed = app.take_pending();
        assert_eq!(
            committed.len(),
            1,
            "exactly one line per decision, not none and not a paragraph: {committed:?}",
        );
        let text: String = committed[0]
            .spans
            .iter()
            .map(|span| span.content.as_ref())
            .collect();
        assert!(
            text.contains("write") && text.contains("src/main.rs"),
            "the committed line names the act and the target: {text:?}",
        );
    }
}

/// The other way in. A key that only works for a reader who already knows it is
/// not an interface, so the same three answers are reachable by moving and
/// pressing `Enter` — and the overlay opens on the least committal one, so
/// `Enter` by reflex gives away the least rather than the most.
#[tokio::test]
async fn f3_the_answers_are_reachable_without_knowing_the_keys() {
    let (request, context) = flagged();
    let (ask, deciding) = asked(request, context).await;

    let mut app = App::new(DARK, "opus-5");
    app.open_approval(ask);

    app.key(KeyEvent::new(KeyCode::Right, KeyModifiers::NONE));
    app.key(KeyEvent::new(KeyCode::Right, KeyModifiers::NONE));
    // Clamped at the end rather than wrapping, like every other list in the
    // product: a surface that jumps from deny back to allow on one keypress is
    // one where holding an arrow key approves a write.
    app.key(KeyEvent::new(KeyCode::Right, KeyModifiers::NONE));
    app.key(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE));

    let decision = deciding.await.expect("the approver did not panic");
    assert!(
        matches!(decision, Decision::Deny { .. }),
        "three rights and Enter is the third answer: {decision:?}",
    );
}

/// A question takes the keyboard while it is up. The run is stopped, so there is
/// nothing to type at, and a keystroke that reached the composer would be one the
/// operator never sees the effect of.
#[tokio::test]
async fn an_open_question_takes_the_keyboard() {
    let (request, context) = flagged();
    let (ask, deciding) = asked(request, context).await;

    let mut app = App::new(DARK, "opus-5");
    app.open_approval(ask);
    app.key(KeyEvent::new(KeyCode::Char('h'), KeyModifiers::NONE));
    app.key(KeyEvent::new(KeyCode::Char('i'), KeyModifiers::NONE));

    assert!(
        app.composer.is_empty(),
        "a keystroke reached the composer while a run was waiting on an answer",
    );

    app.key(KeyEvent::new(KeyCode::Char('n'), KeyModifiers::NONE));
    deciding.await.expect("the approver did not panic");
}

/// **F5.** The harness's own `remember` is run-scoped: it applies for the rest of
/// the turn and dies with it. So the answer is asserted where it has to survive —
/// on the policy the *next* turn is handed — and asserted as a verdict rather than
/// as a label, because a rule that is carried but never consulted is not an
/// answer that was remembered.
#[tokio::test]
async fn f5_allowing_for_the_session_survives_into_the_next_turn() {
    use io_harness::{Effect, Policy};

    let base = Policy::default();
    assert_eq!(
        base.check(Act::Write, "src/main.rs").effect,
        Effect::Ask,
        "the base policy is what makes this question happen at all",
    );

    let (request, context) = flagged();
    let (ask, deciding) = asked(request, context).await;

    let mut app = App::new(DARK, "opus-5");
    app.open_approval(ask);
    app.key(KeyEvent::new(KeyCode::Char('a'), KeyModifiers::NONE));
    deciding.await.expect("the approver did not panic");

    let next = approval::effective_policy(&base, app.remembered());
    assert_eq!(
        next.check(Act::Write, "src/main.rs").effect,
        Effect::Allow,
        "the next turn must not ask again about what was already allowed",
    );
    // Narrow, not blanket. Saying yes to one file is not saying yes to writing.
    assert_eq!(
        next.check(Act::Write, "src/other.rs").effect,
        Effect::Ask,
        "a remembered allow must not widen past the target it was given for",
    );
}

/// The other half, and the reason the merge is io-harness's recipe rather than a
/// second one: a remembered allow may widen an *asking* default and must never
/// defeat a deny beneath it. The secrets layer is the case that matters — an
/// agent that gets `.env` approved once must not have it approved for the session.
#[tokio::test]
async fn a_remembered_allow_cannot_defeat_a_deny() {
    use io_harness::{Effect, Policy, Rule};

    let base = Policy::default();
    assert_eq!(
        base.check(Act::Write, ".env").effect,
        Effect::Deny,
        "the harness's builtin-secrets layer is what this is asserting against",
    );

    let next = approval::effective_policy(
        &base,
        &[Rule {
            act: Act::Write,
            effect: Effect::Allow,
            pattern: ".env".into(),
        }],
    );
    assert_eq!(
        next.check(Act::Write, ".env").effect,
        Effect::Deny,
        "a session allow must not be able to unlock a denied target",
    );
}

/// With nothing remembered the next turn runs under exactly the policy it would
/// have anyway. A release that quietly rebuilt the policy every turn would be one
/// where a `merge` bug is invisible until somebody answers a question.
#[tokio::test]
async fn nothing_remembered_changes_nothing() {
    use io_harness::Policy;

    let base = Policy::default();
    assert_eq!(approval::effective_policy(&base, &[]), base);
}
