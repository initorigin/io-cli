//! **F3** — the agent's question about intent is answered in the session it was
//! asked in, and declining it pauses the run rather than answering it wrongly.
//!
//! The seam under test is `Responder`, which io-harness calls from inside the
//! run: the run is stopped at that await until something comes back. So every
//! test here drives both ends — the harness's side through `Responder::answer`
//! and the operator's side through `App::key` — and asserts what the run
//! received, never what the screen looked like while it waited.

mod support;

use std::sync::Arc;

use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use io_cli::app::App;
use io_cli::theme::DARK;
use io_harness::{Question, Responder};

fn key(code: KeyCode) -> KeyEvent {
    KeyEvent::new(code, KeyModifiers::NONE)
}

fn typed(app: &mut App, text: &str) {
    for ch in text.chars() {
        app.key(key(KeyCode::Char(ch)));
    }
}

/// Everything the app has queued for the scrollback, as one string.
fn said(app: &mut App) -> String {
    app.take_pending()
        .iter()
        .flat_map(|line| line.spans.iter())
        .map(|span| span.content.to_string())
        .collect::<Vec<_>>()
        .join(" ")
}

fn app() -> App {
    App::new(DARK, "test-model")
}

/// What the app draws, through a real render buffer rather than a row string —
/// the rule this product has paid for three times: a string assertion cannot see
/// a widget clip a row.
fn drawn(app: &App, width: u16, height: u16) -> String {
    let (mut screen, _recorder) = support::screen_of(width, height, height);
    screen
        .draw(|frame| app.render(frame, frame.area()))
        .expect("a frame");
    screen.viewport_text().to_string()
}

/// **F3 — the answer the operator types is the answer the run receives, and the
/// turn continues from where it stopped.**
///
/// Sabotage: send the answer on a channel the run is not awaiting — return
/// `Some(String::new())` from `Responder::answer` and deliver the text some other
/// way — under which only this test fails, and it fails by handing the agent an
/// empty answer while the operator's words go nowhere.
#[tokio::test]
async fn the_operator_answers_and_the_run_gets_the_words() {
    let (answerer, mut questions) = io_cli::intent::channel();
    let responder: Arc<dyn Responder> = Arc::new(answerer);

    let asking = tokio::spawn(async move {
        responder
            .answer(&Question::new("drop the column or keep it?"))
            .await
    });

    let asked = questions.recv().await.expect("the question reaches the ui");
    let mut app = app();
    app.open_intent(asked);
    assert!(app.asking(), "the overlay owns the keyboard while it is up");

    typed(&mut app, "keep it");
    app.key(key(KeyCode::Enter));

    assert_eq!(
        asking.await.expect("the responder future"),
        Some("keep it".to_string()),
        "the run receives exactly what was typed",
    );
    assert!(!app.asking(), "and the overlay is gone");
    assert!(
        said(&mut app).contains("keep it"),
        "the answer is in the scrollback"
    );
}

/// **F3 — declining is a real answer.** `None` is what io-harness documents as
/// "nobody here can answer this": the question is persisted and the run pauses,
/// resumable, which is exactly what a session with no responder does today.
///
/// Sabotage: answer `Some(String::new())` on `Esc`, under which only this test
/// fails — and it fails by sending the agent back to work with nothing.
#[tokio::test]
async fn esc_leaves_the_question_for_later_rather_than_answering_it() {
    let (answerer, mut questions) = io_cli::intent::channel();
    let responder: Arc<dyn Responder> = Arc::new(answerer);
    let asking = tokio::spawn(async move { responder.answer(&Question::new("which one?")).await });

    let mut app = app();
    app.open_intent(questions.recv().await.expect("asked"));
    app.key(key(KeyCode::Esc));

    assert_eq!(asking.await.expect("the responder future"), None);
    assert!(!app.asking());
    assert!(
        said(&mut app).contains("pauses"),
        "and the operator is told the run kept the question",
    );
}

/// An empty prompt is a mis-key, not an answer. The overlay stays up.
#[tokio::test]
async fn enter_on_an_empty_prompt_answers_nothing() {
    let (answerer, mut questions) = io_cli::intent::channel();
    let responder: Arc<dyn Responder> = Arc::new(answerer);
    let asking = tokio::spawn(async move { responder.answer(&Question::new("which one?")).await });

    let mut app = app();
    app.open_intent(questions.recv().await.expect("asked"));
    app.key(key(KeyCode::Enter));
    app.key(key(KeyCode::Enter));

    assert!(app.asking(), "an empty answer does not close the overlay");

    typed(&mut app, "the second one");
    app.key(key(KeyCode::Enter));
    assert_eq!(
        asking.await.expect("the responder future"),
        Some("the second one".to_string()),
    );
}

/// A question is modal on the same terms an approval is: nothing typed while it
/// is up may land in a composer sitting behind it, seen by nobody and sent with
/// the next prompt.
#[tokio::test]
async fn the_overlay_refuses_a_paste_and_keeps_the_composer_clear() {
    let (answerer, mut questions) = io_cli::intent::channel();
    let responder: Arc<dyn Responder> = Arc::new(answerer);
    let asking = tokio::spawn(async move { responder.answer(&Question::new("which one?")).await });

    let mut app = app();
    app.open_intent(questions.recv().await.expect("asked"));

    assert!(
        app.paste("a paragraph from somewhere else", false) == io_cli::app::Pasted::Refused,
        "a modal surface refuses a paste",
    );

    app.key(key(KeyCode::Esc));
    let _ = asking.await;
    assert!(
        app.composer.is_empty(),
        "and nothing typed at the question reached the prompt behind it",
    );
}

/// The question, what the agent already knows, and the options it offered are all
/// on screen — the choices as offers, because io-harness's own documentation says
/// an answer is not obliged to be one of them.
#[test]
fn the_question_its_context_and_its_choices_are_all_drawn() {
    let (answerer, mut questions) = io_cli::intent::channel();
    let responder: Arc<dyn Responder> = Arc::new(answerer);
    let question = Question {
        question: "drop the column or keep it?".to_string(),
        context: Some("it has 40 rows and one caller".to_string()),
        choices: vec!["drop".to_string(), "keep".to_string()],
    };

    let runtime = tokio::runtime::Builder::new_current_thread()
        .build()
        .expect("a runtime");
    let asked = runtime.block_on(async {
        let asking = tokio::spawn(async move { responder.answer(&question).await });
        let asked = questions.recv().await.expect("asked");
        (asked, asking)
    });

    let mut app = app();
    app.open_intent(asked.0);
    let screen = drawn(&app, 80, 12);

    assert!(screen.contains("drop the column or keep it?"), "{screen}");
    assert!(
        screen.contains("40 rows and one caller"),
        "the context is shown"
    );
    assert!(
        screen.contains("drop") && screen.contains("keep"),
        "so are the choices",
    );
    assert!(
        screen.contains("Esc"),
        "and the way out is named rather than folklore: {screen}",
    );
}

/// **Non-functional — the question is readable with no colour at all.** Under
/// `NO_COLOR` a tone carries nothing, so the question, what the agent already
/// knows, the options and the way out all have to be words. The answer path is
/// exercised too, because an overlay that draws under `MONO` and cannot be
/// answered there is half a surface.
#[tokio::test]
async fn the_question_is_readable_and_answerable_with_no_colour() {
    let (answerer, mut questions) = io_cli::intent::channel();
    let responder: Arc<dyn Responder> = Arc::new(answerer);
    let asking = tokio::spawn(async move {
        responder
            .answer(&Question {
                question: "drop the column or keep it?".to_string(),
                context: Some("it has 40 rows".to_string()),
                choices: vec!["drop".to_string()],
            })
            .await
    });

    let mut app = App::new(io_cli::theme::MONO, "test-model");
    app.open_intent(questions.recv().await.expect("asked"));
    let screen = drawn(&app, 80, 12);

    assert!(screen.contains("drop the column or keep it?"), "{screen}");
    assert!(screen.contains("40 rows"), "{screen}");
    assert!(screen.contains("Esc"), "{screen}");

    typed(&mut app, "keep it");
    app.key(key(KeyCode::Enter));
    assert_eq!(
        asking.await.expect("the responder future"),
        Some("keep it".to_string()),
    );
}
