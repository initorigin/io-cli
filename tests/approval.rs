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
use io_cli::theme::{Tone, DARK};

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

    let _ = app.answer_approval(io_cli::approval::Answer::Deny);
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

    let approval = Approval::new(ask, std::path::Path::new(""));
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

    let approval = Approval::new(ask, std::path::Path::new(""));
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

/// What a write would do is what makes an answer an informed one, and it is the
/// only part that can be arbitrarily long. It is fitted into the rows that are
/// left and says how many it did not show.
///
/// **Restated in 0.3.0.** Until then the overlay showed the proposed content as
/// plain lines, so this asserted the first of them was on screen. Now a write
/// whose target can be read is shown as a change, and at a session's few rows
/// the one row available carries `+40 -855` rather than the first line of the
/// file — which is the more useful row, and the one this test now pins. What has
/// not changed is the part that mattered: whatever is not shown is counted out
/// loud.
#[tokio::test]
async fn the_overlay_says_what_it_did_not_show() {
    let long: String = (0..40).map(|n| format!("line {n}\n")).collect();
    let (ask, deciding) = asked(
        Request::new(Act::Write, "src/main.rs").with_content(long),
        ApprovalContext::new("tidy the parser"),
    )
    .await;

    let approval = Approval::new(ask, std::path::Path::new(""));
    let (mut screen, _recorder) = support::screen(80, 24);
    screen
        .draw(|frame| approval.render(frame, frame.area(), &DARK))
        .expect("frame");

    let text = screen.viewport_text();
    assert!(
        text.contains("more lines"),
        "what was cut has to be said, or the reader is approving a file they think \
         they read: {text:?}",
    );
    // The size of the change, which is the decision. `src/main.rs` exists, so
    // this is a real diff against a real file rather than an all-addition wall.
    assert!(
        text.contains('+') && text.contains('-'),
        "the counts are the row that always survives: {text:?}",
    );

    approval.answer(approval::Answer::Deny);
    deciding.await.expect("the approver did not panic");
}

/// **F3.** Each key chooses its own answer, `Enter` gives it, the run is told
/// exactly that, and the transcript gains exactly one line saying so — so a
/// decision is in the terminal's own scrollback as well as in the harness's
/// durable trace.
///
/// **The `Enter` is 0.40.0's and it is the behaviour change.** Through 0.39.0 the
/// letter alone answered, which is what let the `n` in a typed sentence deny a
/// write.
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
        assert!(
            app.asking(),
            "a letter chooses and does not answer, so the overlay is still up: {key:?}",
        );
        app.key(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE));
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
    app.key(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE));
    deciding.await.expect("the approver did not panic");
}

/// **F3 — an approval is not answered by a keystroke.**
///
/// The failure this release exists to remove, asserted as the thing that happened
/// rather than as a rule. In the 2026-09-05 field test an operator typed an
/// instruction while an approval was up; the `n` in "end" denied the write, and
/// the composer was left holding the rest of the sentence, which was then sent as
/// a prompt.
///
/// A sentence containing both `n` and `y` leaves the highlight where the last of
/// them put it and decides nothing — which is the part that could not be asserted
/// before, because the first letter used to end the overlay.
///
/// Sabotage: return the found answer from `Approval::key`'s `Char` arm instead of
/// storing it. The first row goes red on the pending act having been decided.
#[tokio::test]
async fn f3_a_typed_sentence_moves_the_highlight_and_decides_nothing() {
    let (request, context) = flagged();
    let (ask, deciding) = asked(request, context).await;

    let mut approval = Approval::new(ask, std::path::Path::new(""));
    assert_eq!(
        approval.chosen(),
        approval::Answer::Once,
        "the overlay opens on the least committal answer",
    );

    // "never mind" — an `n`, an `e`, a `v`, and so on. Not one of them answers.
    for c in "never mind".chars() {
        assert!(
            approval
                .key(KeyEvent::new(KeyCode::Char(c), KeyModifiers::NONE))
                .is_none(),
            "`{c}` answered an approval on its own",
        );
    }
    // `n` was the first letter and `d` the last; the highlight is on the last of
    // the three answer letters the sentence happened to contain, which is `n`.
    assert_eq!(
        approval.chosen(),
        approval::Answer::Deny,
        "a letter that names an answer moves the highlight to it",
    );

    // And a sentence whose letters name all three ends on the last of them: `a`
    // then `n` then `y`, so the highlight finishes on `y`.
    for c in "any".chars() {
        assert!(approval
            .key(KeyEvent::new(KeyCode::Char(c), KeyModifiers::NONE))
            .is_none());
    }
    assert_eq!(
        approval.chosen(),
        approval::Answer::Once,
        "the last answer letter typed is the one highlighted",
    );

    // Nothing has been decided by any of it. The act is still pending, which is
    // the property the whole change exists for.
    approval.answer(approval::Answer::Once);
    let decision = deciding.await.expect("the approver did not panic");
    assert!(
        matches!(decision, Decision::Approve { .. }),
        "the answer is the one that was given deliberately, not one a letter made: {decision:?}",
    );
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
    app.key(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE));
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

// ---------------------------------------------------------------------------
// F7 — an approval shows a proposed write as a diff against the file on disk,
// and a new file is all addition.
// ---------------------------------------------------------------------------

/// An approval over a plain write, with no file behind it — everything a
/// keyboard arm needs and nothing it does not.
///
/// The arms below never answer: they press a key and read the frame. Dropping
/// the `Ask` without answering is a denial, which is what makes the abandoned
/// task safe.
async fn fresh() -> io_cli::approval::Approval {
    use io_cli::approval::{self, Approval};
    use io_harness::{Act, ApprovalContext, Approver, Request};

    let (asker, mut asks) = approval::channel();
    let request = Request::new(Act::Write, "notes.txt".to_string())
        .with_content("one\ntwo\nthree\n".to_string());
    tokio::spawn(async move {
        let asker = asker;
        asker
            .decide_in_context(&request, &ApprovalContext::new("tidy the parser"))
            .await
    });
    let ask = asks.recv().await.expect("the question arrived");
    Approval::new(ask, std::path::Path::new(""))
}

/// One frame of `approval` after `key`, as text.
fn draw_after(approval: &mut io_cli::approval::Approval, key: KeyCode) -> String {
    use io_cli::theme::DARK;

    approval.key(KeyEvent::new(key, KeyModifiers::NONE));
    let (mut screen, _recorder) = support::screen_of(100, 12, 8);
    screen
        .draw(|frame| approval.render(frame, frame.area(), &DARK))
        .expect("frame");
    screen.viewport_text().to_string()
}

/// A fresh approval struck once with `key`: what it drew, and what it answered.
async fn struck(key: KeyCode) -> (String, Option<io_cli::approval::Answer>) {
    use io_cli::theme::DARK;

    let mut approval = fresh().await;
    let answer = approval.key(KeyEvent::new(key, KeyModifiers::NONE));
    let (mut screen, _recorder) = support::screen_of(100, 12, 8);
    screen
        .draw(|frame| approval.render(frame, frame.area(), &DARK))
        .expect("frame");
    (screen.viewport_text().to_string(), answer)
}

/// [`overlay_for`], with a run of keys pressed before the frame is taken.
async fn overlay_after(
    target: &std::path::Path,
    content: &str,
    height: u16,
    keys: &[KeyCode],
) -> String {
    overlay_with(target, content, height, keys).await
}

async fn overlay_for(target: &std::path::Path, content: &str, height: u16) -> String {
    overlay_with(target, content, height, &[]).await
}

/// Open an overlay for a write of `content` to `target`, and return what it
/// draws at `width` columns in `height` rows.
///
/// Rendered through a real `Screen` rather than by calling the private layout,
/// because what matters is what reaches the terminal — and because the overlay
/// is height-constrained, which is the whole reason its content flexes.
///
/// `keys` are pressed before the frame is taken, which is how the scrolling arms
/// reach a row that is not on the first screen.
async fn overlay_with(
    target: &std::path::Path,
    content: &str,
    height: u16,
    keys: &[KeyCode],
) -> String {
    use io_cli::approval::{self, Approval};
    use io_cli::theme::DARK;
    use io_harness::{Act, ApprovalContext, Approver, Decision, Request};

    let (asker, mut asks) = approval::channel();
    let request = Request::new(Act::Write, target.to_string_lossy().to_string())
        .with_content(content.to_string());
    let deciding = tokio::spawn(async move {
        let asker = asker;
        asker
            .decide_in_context(&request, &ApprovalContext::new("tidy the parser"))
            .await
    });
    let ask = asks.recv().await.expect("the question arrived");

    // An empty root leaves an absolute target resolving to itself, which is what
    // these fixtures use; the relative case is covered by the live run, which is
    // where it was found.
    let mut approval = Approval::new(ask, std::path::Path::new(""));
    for key in keys {
        approval.key(KeyEvent::new(*key, KeyModifiers::NONE));
    }
    // `height` is the VIEWPORT's height, which is what the overlay is drawn
    // into — not the terminal's. A session's is four; a taller one is what a
    // reader gets on a terminal with room, and both are worth asserting.
    let (mut screen, _recorder) = support::screen_of(100, height + 4, height);
    screen
        .draw(|frame| approval.render(frame, frame.area(), &DARK))
        .expect("frame");
    let drawn = screen.viewport_text().to_string();

    approval.answer(approval::Answer::Deny);
    let decision = deciding.await.expect("the approver did not panic");
    assert!(matches!(decision, Decision::Deny { .. }));
    drawn
}

#[tokio::test]
async fn f7_a_write_over_an_existing_file_is_shown_as_what_changes() {
    let directory = tempfile::tempdir().expect("a temporary directory");
    let target = directory.path().join("parse.rs");
    let before = "fn one() {}\nfn two() {}\nfn three() {}\n";
    let after = "fn one() {}\nfn two(s: &str) {}\nfn three() {}\n";
    std::fs::write(&target, before).expect("the file exists first");

    let drawn = overlay_for(&target, after, 8).await;

    // The counts are the single most useful row: this write touches one line.
    assert!(drawn.contains("+1"), "{drawn}");
    assert!(drawn.contains("-1"), "{drawn}");
    // And the change itself, from the harness's own renderer.
    assert!(drawn.contains("-fn two() {}"), "{drawn}");
    assert!(drawn.contains("+fn two(s: &str) {}"), "{drawn}");
    // The version that would ship by accident: every existing line shown as new.
    assert!(
        !drawn.contains("+fn one() {}"),
        "an unchanged line was drawn as an addition, which means the old side was \
         empty — the write reads as four hundred lines when it is one: {drawn}",
    );
}

/// **F2 — the overlay's counts agree with the body it drew them over.**
///
/// This overlay computes its own `Edit` from the file on disk and the content the
/// harness handed it, so `Edit::measure` and `Edit::with_hunk` are given the same
/// two texts here and have always agreed. That is *not* true of an edit read back
/// from the store — see `tests/diff.rs`, where `measure` was handed the fragment
/// an `edit_file` replaced and the hunk is the whole file's diff — and it is that
/// pair the 2026-09-05 field test met as `+3 -9` on an approval above an edit the
/// transcript recorded as `-9 +10`.
///
/// So what this arm holds is the property that makes the two surfaces one answer:
/// both count the hunk. Here that is a consistency check rather than a fix, and
/// it is worth having because the alternative — going back to the recorded
/// fields on this one surface — is exactly how the two drifted apart before.
#[tokio::test]
async fn f2_the_counts_agree_with_the_diff_drawn_beneath_them() {
    let directory = tempfile::tempdir().expect("a temporary directory");
    let target = directory.path().join("two-places.rs");
    let before = "fn first() {}\nfn b() {}\nfn c() {}\nfn d() {}\nfn last() {}\n";
    let after = "fn FIRST() {}\nfn b() {}\nfn c() {}\nfn d() {}\nfn LAST() {}\n";
    std::fs::write(&target, before).expect("the file exists first");

    // Tall enough that the whole hunk is drawn, so the header is compared against
    // the body rather than against an elision.
    let drawn = overlay_for(&target, after, 16).await;

    // A drawn diff row is `  1 -fn first() {}` — the line number gutter comes
    // first, so the marker is the first character that is neither a space nor a
    // digit. The header itself is excluded by that same rule: it carries the
    // separator, and no body row does.
    let separator = io_cli::theme::DARK.glyphs.separator;
    let marker = |line: &str| {
        (!line.contains(separator))
            .then(|| line.trim_start_matches(|c: char| c.is_ascii_digit() || c == ' '))
            .and_then(|rest| rest.chars().next())
    };
    let added = drawn
        .lines()
        .filter(|line| marker(line) == Some('+'))
        .count();
    let removed = drawn
        .lines()
        .filter(|line| marker(line) == Some('-'))
        .count();
    assert!(
        drawn.contains(&format!("+{added} -{removed}")),
        "the header does not count the rows drawn under it: {added} additions and \
         {removed} removals are on screen. {drawn}",
    );
}

/// **F1 — a key this modal does not take says so, rather than doing nothing.**
///
/// Every key that was not `y`, `a`, `n`, an arrow or `Enter` returned `None` and
/// changed nothing on screen. An operator who has begun typing a sentence sees
/// their words go nowhere and no sign that the interface is waiting for one of
/// three keys — which is how the 2026-09-05 field test lost one.
///
/// The whole printable ASCII range is swept rather than a chosen letter, because
/// the interesting keys are the ones nobody thought of. The three that answer are
/// excluded in both cases, and so are the four that scroll and the two that move
/// the selection: those do something visible already.
///
/// Sabotage: drop the `self.nudged = answer.is_none()` assignment. This fails and
/// names the character.
#[tokio::test]
async fn f1_a_key_the_modal_does_not_take_is_refused_out_loud() {
    use io_cli::approval::ONLY_THREE_KEYS;

    // **Four keys as of 0.41.0, not three.** `w` chooses "allow and write it down",
    // the answer that appends a `[[policy.layers]]` rule to the operator's own
    // configuration. The set is read from `Answer::ALL` rather than written out, so
    // a fifth answer added later cannot leave this loop asserting that its key is
    // refused — which would fail, and would be fixed by someone adding a letter to
    // a list rather than by anyone deciding a keystroke may grant a permission.
    let answers: Vec<char> = io_cli::approval::Answer::ALL
        .iter()
        .map(|answer| answer.key())
        .collect();
    for byte in b' '..=b'~' {
        let c = byte as char;
        if answers.contains(&c.to_ascii_lowercase()) {
            continue;
        }
        let (drawn, answer) = struck(KeyCode::Char(c)).await;
        assert!(
            answer.is_none(),
            "{c:?} answered the approval. Three keys decide this, and a fourth \
             one that does is a permission given by a keystroke nobody meant as \
             one",
        );
        assert!(
            drawn.contains(ONLY_THREE_KEYS),
            "{c:?} was ignored in silence. The operator is typing into a modal \
             and nothing on screen tells them so: {drawn}",
        );
    }
}

/// **F1 — and the mark describes the last key, not any key ever pressed.**
///
/// A refusal that stayed on screen would be as bad as one that never appeared:
/// it would sit above the answers row for the rest of the overlay's life, saying
/// something untrue about the key that has just arrived.
#[tokio::test]
async fn f1_the_refusal_clears_on_the_next_key_the_modal_does_take() {
    use io_cli::approval::ONLY_THREE_KEYS;

    let mut approval = fresh().await;
    let refused = draw_after(&mut approval, KeyCode::Char('q'));
    assert!(refused.contains(ONLY_THREE_KEYS), "{refused}");

    // An arrow is taken, so the refusal is over.
    let moved = draw_after(&mut approval, KeyCode::Right);
    assert!(
        !moved.contains(ONLY_THREE_KEYS),
        "the refusal outlived the key it was about: {moved}",
    );
}

/// **F2 — the proposed change can be read to its end.**
///
/// The diff was cut at whatever the viewport had, with `⋯ N more lines` on the
/// last row. That is a reasonable summary of a change and no way to read one: an
/// operator approving a write whose end they cannot see is approving the elision.
///
/// The fixture is forty distinct lines into a short viewport, so the last line
/// cannot be on the first screen by accident. `Home` is asserted as well, because
/// a scroll with no way back to the top strands a reader who has paged past the
/// counts they were deciding on.
///
/// Sabotage: ignore `self.scroll` in `as_diff`. This fails; the header and
/// counting arms stay green.
#[tokio::test]
async fn f2_the_whole_change_is_reachable_rather_than_elided() {
    let directory = tempfile::tempdir().expect("a temporary directory");
    let target = directory.path().join("long.rs");
    let after: String = (0..40).map(|n| format!("fn line{n}() {{}}\n")).collect();

    let first = overlay_for(&target, &after, 8).await;
    assert!(
        first.contains("fn line0() {}"),
        "the change opens at its beginning: {first}",
    );
    assert!(
        !first.contains("fn line39() {}"),
        "the fixture is meant to be taller than the viewport, or this arm proves \
         nothing: {first}",
    );
    assert!(
        first.contains("more lines"),
        "a change that does not fit has to say so: {first}",
    );

    let paged = overlay_after(&target, &after, 8, &[KeyCode::PageDown; 8]).await;
    assert!(
        paged.contains("fn line39() {}"),
        "the end of the change is unreachable, so the operator is approving the \
         elision: {paged}",
    );

    let home = overlay_after(
        &target,
        &after,
        8,
        &[KeyCode::PageDown, KeyCode::PageDown, KeyCode::Home],
    )
    .await;
    assert!(
        home.contains("fn line0() {}"),
        "there is no way back to the top of the change: {home}",
    );
}

/// **F2 — scrolling reads and never answers.**
///
/// The four keys that move the proposal are added to a surface whose entire job
/// is taking a permission. None of them may decide anything, and the counts row
/// stays on screen at every offset because it is what the decision is about.
#[tokio::test]
async fn f2_scrolling_the_change_decides_nothing_and_keeps_the_counts() {
    // `End` is in this list because it was not in the code: every other
    // navigation key was taken silently while `End` — what a reader presses to
    // reach the bottom of a diff — flashed `press y, a or n`. Found by the
    // adversarial review, which noticed that the refusal sweep only walks
    // printable ASCII and so never presses it.
    for key in [
        KeyCode::Up,
        KeyCode::Down,
        KeyCode::PageUp,
        KeyCode::PageDown,
        KeyCode::Home,
        KeyCode::End,
    ] {
        let (drawn, answer) = struck(key).await;
        assert!(
            !drawn.contains(io_cli::approval::ONLY_THREE_KEYS),
            "{key:?} was refused. A key that moves the proposal is a key this \
             surface takes, and being told to answer for pressing it is the \
             confusion this overlay exists to remove: {drawn}",
        );
        assert!(
            answer.is_none(),
            "{key:?} answered an approval. Reading a change is not agreeing to it",
        );
    }

    let directory = tempfile::tempdir().expect("a temporary directory");
    let target = directory.path().join("long.rs");
    let after: String = (0..40).map(|n| format!("fn line{n}() {{}}\n")).collect();
    let paged = overlay_after(&target, &after, 8, &[KeyCode::PageDown; 4]).await;
    assert!(
        paged.contains("+40"),
        "the counts scrolled away with the diff. They are what is being decided \
         on, and every other row is context for them: {paged}",
    );
}

#[tokio::test]
async fn f7_a_write_of_a_file_that_does_not_exist_yet_is_all_addition() {
    let directory = tempfile::tempdir().expect("a temporary directory");
    let target = directory.path().join("brand-new.rs");
    let after = "fn hello() {}\nfn goodbye() {}\n";

    let drawn = overlay_for(&target, after, 8).await;

    assert!(drawn.contains("+2"), "two lines arrive: {drawn}");
    assert!(drawn.contains("-0"), "and none leave: {drawn}");
    assert!(drawn.contains("+fn hello() {}"), "{drawn}");
}

#[tokio::test]
async fn f7_no_file_other_than_the_request_s_own_target_is_read() {
    let directory = tempfile::tempdir().expect("a temporary directory");
    let target = directory.path().join("parse.rs");
    std::fs::write(&target, "fn one() {}\n").expect("the target");
    // A neighbour with a distinctive string in it. io-cli is not a file browser:
    // the one workspace read this interface performs is of the file the operator
    // is being asked about, and nothing else in the directory is its business.
    std::fs::write(
        directory.path().join("secrets.env"),
        "TOKEN=NEVERREADTHISFILE\n",
    )
    .expect("the neighbour");

    let drawn = overlay_for(&target, "fn one() {}\nfn two() {}\n", 8).await;

    assert!(
        !drawn.contains("NEVERREADTHISFILE"),
        "a file the request never named reached the screen: {drawn}",
    );
}

// ---------------------------------------------------------------------------
// F4 — every tone that means something renders its word, asserted over the whole
// `Tone` enum rather than over the site that was wrong.
// ---------------------------------------------------------------------------

/// Where a tone's meaning is said out loud.
///
/// The point of naming this per variant is that F4 is a property of the *enum*,
/// not of the approval overlay. `carrier` below is an exhaustive `match`, so a
/// variant added to `Tone` stops this file compiling until somebody says which
/// surface carries its word — and a tone that has a word but is called
/// decoration, or the reverse, fails the test rather than the compiler. Which is
/// the failure that was actually shipping: `Tone::Warning` had a word all along,
/// and the overlay's act was drawn with a bare `Span::styled` that never used it.
enum Carrier {
    /// `word()` is `None`. Presentation only — a diff line's `+`, the accent on a
    /// selection — and forcing a word onto one would put "accent" in front of
    /// every prompt.
    Decoration,
    /// The approval overlay's act row. The one surface F4 exists for.
    Overlay,
    /// A line committed to the terminal's scrollback, which is where the other
    /// three states reach a reader.
    Transcript,
}

fn carrier(tone: Tone) -> Carrier {
    match tone {
        Tone::Normal | Tone::Muted | Tone::Accent => Carrier::Decoration,
        // A diff line already carries its meaning in the `+` or `-` the harness
        // put on it, and a syntax colour means nothing on its own to carry.
        Tone::Added | Tone::Removed | Tone::Keyword | Tone::StringLiteral | Tone::Literal => {
            Carrier::Decoration
        }
        Tone::Warning => Carrier::Overlay,
        Tone::Success | Tone::Error | Tone::Refused => Carrier::Transcript,
    }
}

/// Every variant, written out because Rust cannot enumerate an enum from
/// outside it. The exhaustive `match` above is what stops a new tone arriving
/// unanswered; this list is what stops one arriving unasserted, and the two are
/// kept adjacent on purpose so a variant added to one is added to the other.
const EVERY_TONE: &[Tone] = &[
    Tone::Normal,
    Tone::Muted,
    Tone::Accent,
    Tone::Success,
    Tone::Warning,
    Tone::Error,
    Tone::Refused,
    Tone::Added,
    Tone::Removed,
    Tone::Keyword,
    Tone::StringLiteral,
    Tone::Literal,
];

/// The overlay's act row as a reader sees it: drawn through a real screen at
/// eighty columns, with a path long enough that the row has to be fitted.
///
/// Fitted on purpose. A word that is only present on a wide terminal is not a
/// carrier — the question is whether it survives the cut, and the only way to
/// ask that is to make the cut happen.
async fn act_row_at_eighty() -> String {
    let (ask, deciding) = asked(
        Request::new(
            Act::Write,
            "crates/some-rather-long-crate-name/src/subsystem/module/implementation.rs",
        )
        .with_content("fn main() {}\n"),
        ApprovalContext::new("tidy the parser")
            .flagged_by(Some("crates/**/*.rs".into()), Some("app".into())),
    )
    .await;

    let approval = Approval::new(ask, std::path::Path::new(""));
    let (mut screen, _recorder) = support::screen(80, 24);
    screen
        .draw(|frame| approval.render(frame, frame.area(), &DARK))
        .expect("frame");
    // The first row the overlay draws is the act row; F2 pins that order.
    let row = screen
        .viewport_text()
        .lines()
        .next()
        .expect("the overlay drew something")
        .to_string();

    approval.answer(approval::Answer::Deny);
    deciding.await.expect("the approver did not panic");
    row
}

/// **F4.** Every tone whose `word()` is `Some` reaches a reader with that word
/// beside it, on a surface that was actually rendered.
///
/// Asserted over the enum rather than over the overlay, because the defect this
/// replaces was not that one call site was wrong — it was that nothing stopped a
/// call site from being wrong. A tone added tomorrow either names a carrier here
/// or does not compile.
#[tokio::test]
async fn f4_every_tone_that_means_something_renders_its_word() {
    for &tone in EVERY_TONE {
        match (tone.word(), carrier(tone)) {
            // Nothing to say, and nothing must be made up to say.
            (None, Carrier::Decoration) => {}

            (Some(word), Carrier::Overlay) => {
                let row = act_row_at_eighty().await;
                assert!(
                    row.contains(word),
                    "the overlay's act is {tone:?} and the row it draws never says \
                     {word:?}, so with colour off nothing on it says a decision is \
                     being asked for: {row:?}",
                );
                let at = |needle: &str| {
                    row.find(needle)
                        .unwrap_or_else(|| panic!("{needle:?} is not on the row: {row:?}"))
                };
                assert!(
                    at(word) < at("write"),
                    "the word has to lead the act: trailing it puts it where the fit \
                     cuts, which is a carrier that disappears exactly when the row is \
                     hardest to read: {row:?}",
                );
                // The fit really did bite, so the assertion above is about a cut
                // row rather than about a row that happened to fit whole.
                assert!(
                    row.contains('…'),
                    "the fixture path is meant to be too long for eighty columns: {row:?}",
                );
                assert!(
                    row.chars().count() <= 80,
                    "the word must not push the row past the supported width: {row:?}",
                );
            }

            (Some(word), Carrier::Transcript) => {
                let mut app = App::new(DARK, "opus-5");
                app.record(tone, "write to /etc/hosts");
                let text: String = app
                    .take_pending()
                    .iter()
                    .flat_map(|line| line.spans.iter())
                    .map(|span| span.content.as_ref())
                    .collect();
                assert!(
                    text.contains(word),
                    "a committed {tone:?} line never says {word:?}: {text:?}",
                );
                assert!(
                    text.contains("write to /etc/hosts"),
                    "the word replaced the line instead of accompanying it: {text:?}",
                );
            }

            (Some(_), Carrier::Decoration) => panic!(
                "{tone:?} carries a word and no surface renders it — a state told \
                 apart by colour alone",
            ),
            (None, _) => panic!(
                "{tone:?} is decoration and has no word to carry; a surface that \
                 prefixes one is inventing a meaning",
            ),
        }
    }
}

#[tokio::test]
async fn f7_at_the_tightest_size_the_counts_are_what_survives() {
    let directory = tempfile::tempdir().expect("a temporary directory");
    let target = directory.path().join("big.rs");
    std::fs::write(&target, "a\n".repeat(400)).expect("the target");

    // Four rows: the question, its rule row, one row of content, the answers.
    let drawn = overlay_for(&target, &"b\n".repeat(400), 4).await;

    // One row for the change, and it is spent on the size of the change rather
    // than on the first line of it. Four hundred lines out and four hundred in is
    // a different decision from one line out and one in.
    assert!(drawn.contains("+400"), "{drawn}");
    assert!(drawn.contains("-400"), "{drawn}");
    // And the overlay still says how to answer.
    assert!(
        drawn.contains("allow once") && drawn.contains("deny"),
        "{drawn}"
    );
    // Nothing was silently dropped.
    assert!(drawn.contains('⋯'), "the cut has to be said: {drawn}");
}

/// F7: a paste does not land behind an open approval.
///
/// The overlay answers `y`, `a` and `n`, and it takes the whole viewport while
/// it is up. A paste that slipped past it would go into a composer nobody can
/// see, and ride out with the next prompt after the decision.
#[tokio::test]
async fn f7_a_paste_does_not_land_behind_an_open_approval() {
    let (request, context) = flagged();
    let (ask, deciding) = asked(request, context).await;
    let mut app = App::new(DARK, "opus-5");
    app.open_approval(ask);

    assert!(
        app.paste("from the clipboard", false) == io_cli::app::Pasted::Refused,
        "a question is on screen, and it takes the keyboard",
    );
    assert!(
        app.composer.is_empty(),
        "the paste landed in the composer behind the overlay: {:?}",
        app.composer.text(),
    );

    let _ = app.answer_approval(io_cli::approval::Answer::Deny);
    deciding.await.expect("the run was told");
}
