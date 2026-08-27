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
use io_cli::intent::{Asked, Intent};
use io_cli::theme::DARK;
use io_harness::{PendingQuestion, Question, Responder};

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

// ---- 0.23.0: the same overlay, opened on a run that already stopped ----

/// The question every test below is asked, live or stored — one fixture, so the
/// two construction paths are compared on identical material.
fn question() -> Question {
    Question {
        question: "drop the column or keep it?".to_string(),
        context: Some("it has 40 rows and one caller".to_string()),
        choices: vec!["drop".to_string(), "keep".to_string()],
    }
}

/// A `PendingQuestion` the store itself wrote and read back.
///
/// **The fixture is authentic because io-harness built it.** `PendingQuestion`
/// has no public constructor, and a struct literal assembled here would skip the
/// JSON round-trip `choices` actually goes through — which is the one field a
/// resumed overlay could plausibly lose. So the row is written and read exactly
/// as a resume reads it. `Store::memory()` rather than a temporary directory:
/// same schema, same SQL, same encode and decode, and no directory any assertion
/// here looks at.
fn stored(question: &Question) -> PendingQuestion {
    let store = io_harness::Store::memory().expect("an in-memory store");
    let run = store
        .start_run("drop the column", "openrouter")
        .expect("a run to hang the question off");
    let id = store
        .put_question(run, 1, question)
        .expect("the row is written");
    store
        .question(id)
        .expect("the row reads back")
        .expect("the row is there")
}

/// One overlay, opened on a live turn. The receiver comes back with it because a
/// `oneshot` whose receiver has been dropped is what the live path already reads
/// as "nobody can answer" — a fixture that dropped it would be testing the
/// stored path through the live constructor.
fn live(question: Question) -> (Intent, tokio::sync::oneshot::Receiver<Option<String>>) {
    let (answer, reply) = tokio::sync::oneshot::channel();
    (Intent::new(Asked { question, answer }), reply)
}

/// What the overlay draws, on its own rather than through the app: `App` can only
/// hold a live question, so a stored one has to be rendered directly, and holding
/// the live one to the same helper is what makes the comparison a comparison.
fn drawn_overlay(overlay: &Intent, theme: &io_cli::theme::Theme) -> String {
    let (mut screen, _recorder) = support::screen_of(80, 12, 12);
    screen
        .draw(|frame| overlay.render(frame, frame.area(), theme))
        .expect("a frame");
    screen.viewport_text().to_string()
}

/// Everything about a question overlay that does **not** depend on which way in
/// was taken. Run against both constructors, because a property asserted for a
/// live question and not a stored one is how the two paths drift apart.
fn the_properties_both_paths_owe(open: impl Fn() -> Intent) {
    let screen = drawn_overlay(&open(), &DARK);
    assert!(screen.contains("drop the column or keep it?"), "{screen}");
    assert!(
        screen.contains("40 rows and one caller"),
        "the context: {screen}",
    );
    assert!(
        screen.contains("drop") && screen.contains("keep"),
        "the choices, as offers: {screen}",
    );
    assert!(screen.contains("Esc"), "the way out is a word: {screen}");

    let mut answering = open();
    assert_eq!(
        answering.key(key(KeyCode::Enter)),
        None,
        "an empty prompt is a mis-key, not an answer",
    );
    for ch in "keep it".chars() {
        assert_eq!(
            answering.key(key(KeyCode::Char(ch))),
            None,
            "typing does not close the overlay",
        );
    }
    assert_eq!(
        answering.key(key(KeyCode::Enter)),
        Some(Some("keep it".to_string())),
        "and the answer is exactly what was typed",
    );

    let mut declining = open();
    assert_eq!(
        declining.key(key(KeyCode::Esc)),
        Some(None),
        "Esc declines, and declining carries no answer",
    );
}

/// **A question resumed off the store is answered by the same widget on the same
/// keys as a live one.** The whole point of the second constructor: an operator
/// answering yesterday's pause must not be looking at a different surface.
///
/// Sabotage: give the stored path its own `render` and drop the choices from it —
/// under which only this test fails, and it fails on the stored arm alone while
/// every live-path test above still passes, which is precisely the drift this
/// shared block exists to catch.
#[test]
fn a_stored_question_answers_and_declines_and_draws_as_a_live_one_does() {
    the_properties_both_paths_owe(|| live(question()).0);
    the_properties_both_paths_owe(|| Intent::resumed(&stored(&question())));
}

/// **A live question resolves by sending; the caller is handed nothing back**,
/// because the turn parked on the channel already has it.
#[tokio::test]
async fn a_live_answer_goes_down_the_channel_and_leaves_the_caller_nothing_to_do() {
    let (overlay, reply) = live(question());

    assert_eq!(
        overlay.resolve(Some("keep it".to_string())),
        None,
        "nothing comes back: the turn took it",
    );
    assert_eq!(
        reply.await.expect("the turn's end"),
        Some("keep it".to_string()),
    );
}

/// **A stored question resolves by returning**, because the run it belongs to has
/// already ended and there is no channel to send down — the answer has to reach
/// `resume_with_answer_observed`, and it does that as a value.
///
/// Sabotage: build the stored path a `oneshot` of its own and send into it — under
/// which only this test fails, and it fails by dropping the operator's answer into
/// a channel with no receiver, which is the shape the live path reads as "nobody
/// answered, stay paused".
#[test]
fn a_stored_answer_comes_back_to_the_caller_that_has_to_deliver_it() {
    let overlay = Intent::resumed(&stored(&question()));

    assert_eq!(
        overlay.resolve(Some("keep it".to_string())),
        Some(Some("keep it".to_string())),
        "the caller resumes the run with this",
    );
}

/// **Esc on a stored question declines without producing an answer**: the run was
/// found parked and is left parked, and what comes back says "no answer" rather
/// than an empty one, which the agent would read as information.
#[test]
fn esc_on_a_stored_question_leaves_the_run_parked_with_no_answer() {
    let mut overlay = Intent::resumed(&stored(&question()));

    assert_eq!(overlay.key(key(KeyCode::Esc)), Some(None));
    assert_eq!(
        overlay.resolve(None),
        Some(None),
        "no answer is delivered, and none is invented",
    );
}

/// The one line that is allowed to differ, and the reason it differs: declining a
/// live question defers it inside a turn that is still running, while declining a
/// resumed one leaves a run parked exactly where it was found. Both name `Esc`;
/// neither promises something behind the screen is not doing.
#[test]
fn the_footer_says_what_esc_actually_leaves_behind_on_each_path() {
    let live_screen = drawn_overlay(&live(question()).0, &DARK);
    let stored_screen = drawn_overlay(&Intent::resumed(&stored(&question())), &DARK);

    assert!(
        live_screen.contains("Esc leaves it for later"),
        "{live_screen}",
    );
    assert!(
        stored_screen.contains("Esc leaves the run parked"),
        "{stored_screen}",
    );
    assert!(
        !stored_screen.contains("for later"),
        "a resumed question does not promise a turn will pick it up: {stored_screen}",
    );
}

/// **Non-functional — a resumed question is readable with no colour at all.** The
/// stored path draws under `MONO` for the same reason the live one must: a tone
/// carries nothing there, so the question, the context, the choices and the way
/// out all have to be words.
#[test]
fn a_stored_question_is_readable_with_no_colour_at_all() {
    let overlay = Intent::resumed(&stored(&question()));
    let screen = drawn_overlay(&overlay, &io_cli::theme::MONO);

    assert!(screen.contains("drop the column or keep it?"), "{screen}");
    assert!(screen.contains("40 rows"), "{screen}");
    assert!(screen.contains("Esc"), "{screen}");
}
