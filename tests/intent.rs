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
use io_harness::{Choice, PendingQuestion, Question, Responder};

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
fn drawn(app: &mut App, width: u16, height: u16) -> String {
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
    // **The offers are spelled with words the question does not contain**, and
    // that is the whole assertion below. Through 0.32.0 this fixture offered
    // `drop` and `keep` against a question reading "drop the column or keep it?",
    // so the line checking that the choices are drawn passed on the question text
    // alone — it could not fail while the question rendered at all, which is a
    // gate asserting nothing.
    let question = Question::new("drop the column or keep it?")
        .with_context("it has 40 rows and one caller")
        .with_choices(["created_at", "updated_at"]);

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
    let screen = drawn(&mut app, 80, 12);

    assert!(screen.contains("drop the column or keep it?"), "{screen}");
    assert!(
        screen.contains("40 rows and one caller"),
        "the context is shown"
    );
    assert!(
        screen.contains("created_at") && screen.contains("updated_at"),
        "so are the choices, and neither word is in the question line above: {screen}",
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
            .answer(
                &Question::new("drop the column or keep it?")
                    .with_context("it has 40 rows")
                    .with_choices(["drop"]),
            )
            .await
    });

    let mut app = App::new(io_cli::theme::MONO, "test-model");
    app.open_intent(questions.recv().await.expect("asked"));
    let screen = drawn(&mut app, 80, 12);

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
///
/// The offers are words the question does not contain, for the reason
/// [`the_question_its_context_and_its_choices_are_all_drawn`] gives: an offer
/// spelled with a word already in the question line makes "the choices are drawn"
/// a claim the question's own rendering satisfies.
fn question() -> Question {
    Question::new("drop the column or keep it?")
        .with_context("it has 40 rows and one caller")
        .with_choices(["created_at", "updated_at"])
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
fn drawn_overlay(overlay: &mut Intent, theme: &io_cli::theme::Theme) -> String {
    drawn_overlay_at(overlay, 80, 12, theme)
}

/// The same, at a stated size. A block whose height depends on the width it wraps
/// at cannot be asserted through a helper that hard-codes one.
fn drawn_overlay_at(
    overlay: &mut Intent,
    width: u16,
    height: u16,
    theme: &io_cli::theme::Theme,
) -> String {
    let (mut screen, _recorder) = support::screen_of(width, height, height);
    screen
        .draw(|frame| overlay.render(frame, frame.area(), theme))
        .expect("a frame");
    screen.viewport_text().to_string()
}

/// Everything about a question overlay that does **not** depend on which way in
/// was taken. Run against both constructors, because a property asserted for a
/// live question and not a stored one is how the two paths drift apart.
fn the_properties_both_paths_owe(open: impl Fn() -> Intent) {
    let screen = drawn_overlay(&mut open(), &DARK);
    assert!(screen.contains("drop the column or keep it?"), "{screen}");
    assert!(
        screen.contains("40 rows and one caller"),
        "the context: {screen}",
    );
    assert!(
        screen.contains("created_at") && screen.contains("updated_at"),
        "the choices, as offers, in words the question line does not carry: {screen}",
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
        Some(vec![Some("keep it".to_string())]),
        "and the answer is exactly what was typed",
    );

    let mut declining = open();
    assert_eq!(
        declining.key(key(KeyCode::Esc)),
        Some(vec![None]),
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
        overlay.resolve(vec![Some("keep it".to_string())]),
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
        overlay.resolve(vec![Some("keep it".to_string())]),
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

    assert_eq!(overlay.key(key(KeyCode::Esc)), Some(vec![None]));
    assert_eq!(
        overlay.resolve(vec![None]),
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
    let live_screen = drawn_overlay(&mut live(question()).0, &DARK);
    let stored_screen = drawn_overlay(&mut Intent::resumed(&stored(&question())), &DARK);

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
    let mut overlay = Intent::resumed(&stored(&question()));
    let screen = drawn_overlay(&mut overlay, &io_cli::theme::MONO);

    assert!(screen.contains("drop the column or keep it?"), "{screen}");
    assert!(screen.contains("40 rows"), "{screen}");
    assert!(screen.contains("Esc"), "{screen}");
}

// ---------------------------------------------------------------------------
// 0.32.0 — the question becomes answerable, as one list with no modes.
// ---------------------------------------------------------------------------

/// A question with a context line and five choices: the shape that filled all
/// eight rows of the viewport through 0.31.0 and left the operator with nothing
/// to type into.
fn five_choices() -> Question {
    Question::new("which column should the migration drop?")
        .with_context("the table has 40 rows and one caller")
        .with_choices([
            "created_at",
            "updated_at",
            "deleted_at",
            "archived_at",
            "expired_at",
        ])
}

/// **O1 — the answer composer is rendered and focusable, at 80x24 and at 80x8.**
///
/// This is the release's own defect. `Intent::render` drew the composer only
/// `if area.height > head`, and `head` was `lines.len()` over a paragraph that
/// wraps — so a question with a context line and five choices consumed the whole
/// viewport and the text area was never rendered at all. The operator saw a
/// question, some inert bullets, and no way to answer.
///
/// Asserted by finding the composer's own prompt marker in a rendered frame, not
/// by the absence of a panic: the old code did not panic, it silently drew
/// nothing.
///
/// Sabotage: restore `if area.height > head` around the composer render, under
/// which the 80x8 arm goes red and the 80x24 arm does not — which is also the
/// evidence that the small terminal is the case that mattered.
#[test]
fn o1_the_composer_is_drawn_however_long_the_question_is() {
    for (width, height) in [(80u16, 24u16), (80, 8)] {
        let (mut overlay, _reply) = live(five_choices());
        let (mut screen, _recorder) = support::screen_of(width, height, height);
        screen
            .draw(|frame| overlay.render(frame, frame.area(), &DARK))
            .expect("a frame");
        let screen = screen.viewport_text().to_string();

        assert!(
            screen.contains(io_cli::composer::PROMPT.trim_end()),
            "no composer at {width}x{height}, so the question cannot be answered: {screen}",
        );
    }
}

/// **O1/O2 — the offers and the free-text row are one list, and the marker opens
/// on the row that takes prose.**
///
/// The second half is a safety property rather than a preference: an overlay that
/// opened with an offer marked would turn a reflexive `Enter` — the key the
/// operator has just pressed to submit the prompt that started this turn — into
/// silent agreement with a suggestion they have not read.
#[test]
fn o2_the_marker_opens_on_the_row_that_takes_prose() {
    let (mut overlay, _reply) = live(five_choices());
    assert_eq!(
        overlay.key(key(KeyCode::Enter)),
        None,
        "Enter on an untouched overlay must be a mis-key, never agreement with \
         the agent's first offer",
    );
}

/// **O2 — `Enter` on a choice sends that string verbatim.**
///
/// Verbatim matters: the string the agent sent is the string it gets back, never
/// the row's label re-read off the screen and never a fitted copy of it. That is
/// why `Outcome::Chosen` indexes the caller's own unfiltered rows.
#[test]
fn o2_enter_on_an_offer_sends_that_offer_verbatim() {
    let (mut overlay, _reply) = live(five_choices());

    // Up from the free-text row is the last offer.
    assert_eq!(
        overlay.key(key(KeyCode::Up)),
        None,
        "moving does not answer"
    );
    assert_eq!(
        overlay.key(key(KeyCode::Enter)),
        Some(vec![Some("expired_at".to_string())]),
        "the offer the marker was on, exactly as the agent spelled it",
    );
}

/// **O2 — typing answers, wherever the marker was, and the marker follows.**
///
/// The one place this surface differs from every other `Picker` in the product,
/// and it is deliberate: a picker's printable keys filter its rows, which is right
/// for four hundred models and wrong for five offers. Here the expensive act is
/// answering, not finding.
#[test]
fn o2_typing_from_an_offer_answers_rather_than_filtering() {
    let (mut overlay, _reply) = live(five_choices());

    // Put the marker on an offer first, so the typing has somewhere wrong to go.
    overlay.key(key(KeyCode::Up));
    for ch in "none of those".chars() {
        assert_eq!(
            overlay.key(key(KeyCode::Char(ch))),
            None,
            "typing does not close the overlay",
        );
    }
    assert_eq!(
        overlay.key(key(KeyCode::Enter)),
        Some(vec![Some("none of those".to_string())]),
        "the words the operator typed, not the offer the marker started on",
    );
}

/// **O2 — moving the marker off the free-text row folds the composer shut without
/// losing what was typed.**
#[test]
fn o2_folding_the_composer_keeps_what_was_typed() {
    let (mut overlay, _reply) = live(five_choices());
    for ch in "half written".chars() {
        overlay.key(key(KeyCode::Char(ch)));
    }
    // Up onto an offer, then back down to the free-text row.
    overlay.key(key(KeyCode::Up));
    overlay.key(key(KeyCode::Down));

    assert_eq!(
        overlay.key(key(KeyCode::Enter)),
        Some(vec![Some("half written".to_string())]),
        "the composer lost its contents when the marker left it",
    );
}

/// **O2 — a question with no choices is the free-text row alone, already
/// unfolded**, which is the surface this overlay had before 0.32.0. Asserted
/// because a redesign that changes the simplest case has changed something it was
/// not asked to.
#[test]
fn o2_a_question_with_no_choices_is_the_surface_it_always_was() {
    let (mut overlay, _reply) = live(Question::new("what did you mean?"));
    let (mut screen, _recorder) = support::screen_of(80, 12, 12);
    screen
        .draw(|frame| overlay.render(frame, frame.area(), &DARK))
        .expect("a frame");
    let drawn = screen.viewport_text().to_string();
    assert!(
        drawn.contains(io_cli::composer::PROMPT.trim_end()),
        "the composer is not drawn: {drawn}",
    );

    for ch in "i meant the second one".chars() {
        overlay.key(key(KeyCode::Char(ch)));
    }
    assert_eq!(
        overlay.key(key(KeyCode::Enter)),
        Some(vec![Some("i meant the second one".to_string())]),
    );
}

/// **O4 — `Esc` still sends `None`, from either row.**
///
/// Asserted against the existing behaviour rather than re-derived: `None` is what
/// io-harness documents as "nobody here can answer this", so the run parks with
/// the question persisted rather than being denied. A redesign that turned a
/// decline into an empty answer would send the agent back to work knowing nothing
/// more, and would look identical on screen.
#[test]
fn o4_esc_declines_from_the_offers_as_well_as_from_the_composer() {
    let (mut from_composer, _a) = live(five_choices());
    assert_eq!(from_composer.key(key(KeyCode::Esc)), Some(vec![None]));

    let (mut from_offers, _b) = live(five_choices());
    from_offers.key(key(KeyCode::Up));
    assert_eq!(
        from_offers.key(key(KeyCode::Esc)),
        Some(vec![None]),
        "Esc on an offer declines too, rather than choosing it",
    );
}

/// **O3 — the question is not drawn as a warning.**
///
/// `Tone::Warning`'s word is literally `warning`, so every question this agent
/// ever asked arrived prefixed with it. Asserted under `MONO`, where a tone is
/// nothing but its word — which is the only place the claim is checkable at all.
#[test]
fn o3_a_question_is_not_a_warning() {
    let (mut overlay, _reply) = live(five_choices());
    let (mut screen, _recorder) = support::screen_of(80, 24, 24);
    screen
        .draw(|frame| overlay.render(frame, frame.area(), &io_cli::theme::MONO))
        .expect("a frame");
    let drawn = screen.viewport_text().to_string();

    assert!(
        drawn.contains("which column should the migration drop?"),
        "{drawn}",
    );
    assert!(
        !drawn.contains("warning"),
        "the question is drawn as a warning: {drawn}",
    );
}

/// **O16 — the overlay asks the viewport for the rows it needs, and the request
/// grows with the question.**
///
/// A request rather than a demand: `App::viewport_wanted` clamps it to what the
/// terminal can spare. What is asserted here is that the number is derived from
/// the content and is measured through the wrapper, not counted as lines — the
/// defect `rows::wrapped` exists for.
#[test]
fn o16_the_overlay_asks_for_rows_that_grow_with_the_question() {
    let (small, _a) = live(Question::new("what did you mean?"));
    let (large, _b) = live(five_choices());

    let small_rows = small.rows_wanted(80, &DARK);
    let large_rows = large.rows_wanted(80, &DARK);
    assert!(
        large_rows > small_rows,
        "five offers and a context line asked for {large_rows} rows, no more than \
         the bare question's {small_rows}",
    );

    // The measurement is of wrapped rows, so a question too long for the width
    // asks for more than the same question at a width that fits it.
    let (long, _c) = live(Question::new(
        "which of the columns in this table should the migration drop, given that \
         it has forty rows and exactly one caller anywhere in the workspace?",
    ));
    assert!(
        long.rows_wanted(40, &DARK) > long.rows_wanted(200, &DARK),
        "the row demand ignores wrapping, which is the `lines.len()` measurement \
         this release replaced",
    );
}

// ---------------------------------------------------------------------------
// 0.33.0 — a batch of questions is one overlay, and a second question is held
// rather than dropped.
// ---------------------------------------------------------------------------

/// Five independent questions, the shape io-harness 0.72.0's `answer_all` exists
/// for and the shape that arrived as five consecutive overlays before it.
fn five_questions() -> Vec<Question> {
    (1..=5)
        .map(|at| Question::new(format!("question {at} of the migration?")))
        .collect()
}

/// **F2 — a batch crosses the channel as ONE delivery and comes back in the order
/// it was asked.**
///
/// The count is what catches the regression. io-harness's default `answer_all`
/// body loops `answer` once per question, so without the override the first
/// `recv` yields a delivery of **one** and the second question is not even sent
/// until the first has been answered — five overlays, each hiding the next.
///
/// Sabotage: delete `Answerer::answer_all` and fall back to the trait default,
/// under which the length assertion fails at 1. Sabotage the order by collecting
/// the answers into a set or by delivering them as they are decided, under which
/// the final comparison fails while every other assertion here still passes.
#[tokio::test]
async fn t07_a_batch_crosses_as_one_delivery_and_comes_back_in_order() {
    let (answerer, mut questions) = io_cli::intent::channel();
    let responder: Arc<dyn Responder> = Arc::new(answerer);
    let asking = tokio::spawn(async move {
        let batch = five_questions();
        responder.answer_all(&batch).await
    });

    let batch = questions.recv().await.expect("the batch reaches the ui");
    assert_eq!(
        batch.len(),
        5,
        "five questions did not cross as one unit, so the interface is still \
         answering them one at a time",
    );

    let mut app = app();
    app.open_intent(batch);
    for at in 1..=5 {
        assert!(
            app.asking(),
            "the overlay closed after {} answers, and a batch that is not whole \
             parks the run",
            at - 1,
        );
        typed(&mut app, &format!("answer {at}"));
        app.key(key(KeyCode::Enter));
    }
    assert!(
        !app.asking(),
        "the overlay is gone once every question is decided"
    );

    assert_eq!(
        asking.await.expect("the responder future"),
        (1..=5)
            .map(|at| Some(format!("answer {at}")))
            .collect::<Vec<_>>(),
        "the run received the answers out of the order it asked the questions",
    );
}

/// **F2 — declining one question of a batch lands `None` in exactly that position,
/// and the surface says the run parks rather than reporting success.**
///
/// io-harness commits a batch only when every entry is `Some`: four answers out of
/// five park the whole batch for a human. So the one thing this must not do is
/// look like an answered turn.
///
/// Sabotage: make `Esc` decline the whole batch, under which the vector comes back
/// all-`None` and the position assertions fail. Sabotage the notice by recording
/// only the per-question lines, under which the scrollback claims four answers and
/// never says the run stopped.
#[tokio::test]
async fn t07_declining_one_question_parks_the_batch_and_says_so() {
    let (answerer, mut questions) = io_cli::intent::channel();
    let responder: Arc<dyn Responder> = Arc::new(answerer);
    let asking = tokio::spawn(async move {
        let batch = five_questions();
        responder.answer_all(&batch).await
    });

    let mut app = app();
    app.open_intent(questions.recv().await.expect("the batch"));
    for at in 1..=5 {
        if at == 3 {
            // The one nobody here can answer. It decides *this* question and
            // moves on rather than closing the overlay.
            app.key(key(KeyCode::Esc));
            assert!(
                app.asking(),
                "Esc closed the whole batch, so four questions nobody declined \
                 were declined for them",
            );
            continue;
        }
        typed(&mut app, &format!("answer {at}"));
        app.key(key(KeyCode::Enter));
    }

    let answers = asking.await.expect("the responder future");
    assert_eq!(
        answers.len(),
        5,
        "one entry per question, whatever was decided"
    );
    assert_eq!(answers[2], None, "the declined question is the third one");
    for (at, answer) in answers.iter().enumerate() {
        if at == 2 {
            continue;
        }
        assert_eq!(
            answer.as_deref(),
            Some(format!("answer {}", at + 1).as_str()),
            "question {} lost its answer to the decline beside it",
            at + 1,
        );
    }

    let said = said(&mut app);
    // **Needled on words only the batch summary says.** `pauses` was the needle
    // until the sabotage pass proved it vacuous: the per-question decline line
    // committed for the third question is "left unanswered — the run pauses and
    // keeps the question", so the whole summary could be deleted and this stayed
    // green. What the summary alone carries is the count and the rule — that four
    // answers buy nothing because a batch is committed whole — and those are what
    // a transcript showing four `answered` lines is missing.
    assert!(
        said.contains("keeps all 5"),
        "the scrollback never says the other four answers are parked with the \
         decline, so it reads as a turn that carried on: {said}",
    );
    assert!(
        said.contains("a batch is committed only when every question is answered"),
        "the scrollback says the run stopped but not why four answers were not \
         enough: {said}",
    );
}

/// **F3 — a second question arriving while one is open is HELD, and both reply
/// channels survive.**
///
/// The defect this closes: `App::open_intent` assigned the overlay
/// unconditionally, so the second question replaced the first and dropped its
/// reply channel — and a dropped channel is `None`, which io-harness reads as
/// *pause the run and keep the question*. The first question became a silent
/// pause that nobody was ever shown. The mpsc has always carried N; only the
/// interface assumed one.
///
/// **This test fails before the fix**, on the first assertion: the first future
/// resolves `None` while the operator's words go to the second question.
///
/// Sabotage: restore the unconditional assignment, or hold the second question and
/// never open it — the first fails the answer comparison, the second fails
/// `asking()` after the first answer.
#[tokio::test]
async fn f3_a_second_question_is_held_rather_than_replacing_the_first() {
    let (answerer, mut questions) = io_cli::intent::channel();
    let responder: Arc<dyn Responder> = Arc::new(answerer);

    // Asked and received one at a time, so which question is which is a fact
    // rather than a race.
    let first = {
        let responder = Arc::clone(&responder);
        tokio::spawn(async move { responder.answer(&Question::new("the first?")).await })
    };
    let one = questions.recv().await.expect("the first question");
    let second = {
        let responder = Arc::clone(&responder);
        tokio::spawn(async move { responder.answer(&Question::new("the second?")).await })
    };
    let two = questions.recv().await.expect("the second question");

    let mut app = app();
    app.open_intent(one);
    app.open_intent(two);

    typed(&mut app, "answering the first");
    app.key(key(KeyCode::Enter));
    assert!(
        app.asking(),
        "the held question did not open, so a run is still waiting on an overlay \
         that will never be drawn",
    );

    typed(&mut app, "answering the second");
    app.key(key(KeyCode::Enter));
    assert!(!app.asking(), "and nothing is left holding the keyboard");

    assert_eq!(
        first.await.expect("the first responder future"),
        Some("answering the first".to_string()),
        "the first question's channel was dropped by the second question arriving",
    );
    assert_eq!(
        second.await.expect("the second responder future"),
        Some("answering the second".to_string()),
    );
}

/// **A `multiple` question answers with the marked offers, in the caller's row
/// order, spelled by whatever io-harness spells a several-part answer with.**
///
/// **What this gates, stated honestly.** `Question::answer_of` is `join(", ")`, so
/// the equality below is satisfied by *any* io-cli that joins with `", "` —
/// including one that writes the separator out itself. What it does gate is the
/// selection and the order: which offers came back, and that they came back in the
/// order the agent listed them rather than the order they were marked. The joiner
/// half is a **drift** gate rather than a today gate — it goes red the release
/// io-harness changes `answer_of`'s spelling and io-cli is not calling it, which is
/// the only moment a forked joiner is detectable at all, and is why the right-hand
/// side is a call and never a literal.
///
/// Sabotage: mark the offers and answer with `chosen()`'s row order reversed, or
/// with the row under the marker alone — under which the first arm fails on the
/// pair and the second still passes, which is the half a one-label answer cannot
/// see.
#[test]
fn t07_a_multiple_question_answers_in_the_harness_own_spelling() {
    let plural = || {
        Question::new("which platforms should the build target?")
            .with_choices(["linux", "windows", "macos"])
            .multiple()
    };

    let (mut overlay, _reply) = live(plural());
    // The marker opens on the free-text row, as it does on every question. Up is
    // the last offer.
    overlay.key(key(KeyCode::Up));
    overlay.key(key(KeyCode::Char(' ')));
    overlay.key(key(KeyCode::Up));
    overlay.key(key(KeyCode::Char(' ')));
    assert_eq!(
        overlay.key(key(KeyCode::Enter)),
        Some(vec![Some(Question::answer_of(["windows", "macos"]))]),
        "the marked offers, in the caller's row order and in the harness's spelling",
    );

    // **Nothing marked is still an answer**: `Picker::chosen` falls back to the
    // row under the marker, so `Enter` on an offer of a plural question sends that
    // offer rather than nothing at all.
    let (mut unmarked, _reply) = live(plural());
    unmarked.key(key(KeyCode::Up));
    assert_eq!(
        unmarked.key(key(KeyCode::Enter)),
        Some(vec![Some(Question::answer_of(["macos"]))]),
        "a plural question with nothing marked answered with nothing",
    );
}

/// **The spacebar on a single-answer question is still a space — including the
/// first one, pressed while the marker is still on an offer.**
///
/// `Picker::accepting_several` costs the spacebar, so a question that does not take
/// several must never opt in — otherwise every two-word answer in the product loses
/// its spaces to a mark nobody asked for.
///
/// **The space is pressed BEFORE anything is typed, and that ordering is the whole
/// test.** Until 0.33.0 this typed `"two words"` from an offer, which gated nothing:
/// the printable arm moves the marker to the free-text row on the *first* character,
/// so by the time the space arrived `writing()` was already true and the space
/// reached the composer whatever `Intent::list` had opted into. Deleting the
/// `multiple` routing condition outright left it green. Pressing space first is the
/// only sequence in which a mark and a character are still distinguishable.
///
/// Two assertions, and each names its own failure. The marker must have *moved* to
/// the row that takes prose — a space swallowed as a mark leaves it on the offer —
/// and the answer must still carry that leading space, because a mark produces no
/// character at all.
///
/// Sabotage: call `accepting_several` unconditionally in `Intent::list`, or drop
/// `self.current().multiple` from the space arm in `Intent::decision` — under both
/// the marker stays on the offer and the answer comes back `"two words"` with its
/// first space missing.
#[test]
fn t07_the_spacebar_on_a_single_answer_question_is_still_a_space() {
    let (mut overlay, _reply) = live(five_choices());
    // Five offers and the row that takes prose, which is the last one.
    let free = overlay.offers().rows().len() - 1;
    // From an offer, which is the only place a mark could be made at all.
    overlay.key(key(KeyCode::Up));
    assert_ne!(
        overlay.offers().selected(),
        free,
        "the marker is on an offer"
    );

    assert_eq!(
        overlay.key(key(KeyCode::Char(' '))),
        None,
        "a space does not close the overlay",
    );
    assert_eq!(
        overlay.offers().selected(),
        free,
        "the space was swallowed as a mark: it never reached the composer, so the \
         marker never moved to the row that takes prose",
    );

    for ch in "two words".chars() {
        assert_eq!(
            overlay.key(key(KeyCode::Char(ch))),
            None,
            "typing does not close the overlay",
        );
    }
    assert_eq!(
        overlay.key(key(KeyCode::Enter)),
        Some(vec![Some(" two words".to_string())]),
        "the spacebar was taken from a question that takes one answer",
    );
}

/// **F9 — the free-text row is last and holds the marker, on every question of a
/// batch as on a single one.** Asserted by index rather than off the screen: a
/// row string can be produced by a question line that happens to contain the same
/// words, and 0.32.0's decision is about *which row is focused*, which no screen
/// assertion can see.
///
/// Sabotage: focus row 0 when the overlay moves to the next question of a batch —
/// under which the second arm fails while every single-question test passes, and
/// a reflexive `Enter` becomes silent agreement with an offer nobody read.
#[tokio::test]
async fn f9_the_free_text_row_is_last_and_focused_on_every_question_of_a_batch() {
    let (answerer, mut questions) = io_cli::intent::channel();
    let responder: Arc<dyn Responder> = Arc::new(answerer);
    let _asking = tokio::spawn(async move {
        let batch = vec![five_choices(), Question::new("and then what?")];
        responder.answer_all(&batch).await
    });

    let mut overlay = Intent::new(questions.recv().await.expect("the batch"));
    // Five offers on the first question, none on the second: the free-text row is
    // `choices.len()` in both, which is the index the unfold is keyed on.
    for offers in [5usize, 0] {
        let rows = overlay.offers().rows();
        assert_eq!(
            rows.len(),
            offers + 1,
            "the free-text row is missing, or something else is in the list",
        );
        assert_eq!(
            rows[offers].label,
            io_cli::intent::OWN_WORDS,
            "the free-text row is not the last one",
        );
        assert_eq!(
            overlay.offers().selected(),
            offers,
            "the marker did not open on the row that takes prose",
        );
        // Decide this one, which moves the overlay to the next question.
        overlay.key(key(KeyCode::Char('x')));
        overlay.key(key(KeyCode::Enter));
    }
}

// ---------------------------------------------------------------------------
// 0.33.0 — an offer can say more than its label: F4 the description, F5 the
// preview, F6 the room the preview is given.
// ---------------------------------------------------------------------------

/// Offers that say more than their labels.
///
/// **Every word asserted below appears exactly once in this fixture**, and never
/// in the question, the context, a label or the key line. That is not tidiness:
/// two gates in this file were vacuous because `contains` found their needle in
/// the question text, so a description asserted with words the question already
/// says proves that the question was drawn.
fn explained() -> Question {
    Question::new("which column should the migration drop?")
        .with_context("the table has 40 rows and one caller")
        .with_choices([
            Choice::new("created_at").describe("stamped once and never read since"),
            Choice::new("updated_at"),
            Choice::new("archived_at").preview("ALTER TABLE ledger\n  DROP COLUMN archived_at;"),
        ])
}

/// **F4 — a description takes a row under its label, and an offer without one
/// takes no row at all.**
///
/// The count is what catches it. A description drawn into the row's `detail`
/// instead — the same words, on the same line — leaves four rows where this wants
/// five, and looks correct in any screen assertion that only asks whether the
/// sentence is present.
///
/// Sabotage: push a description row for every offer rather than for the described
/// ones, under which the count is seven; drop the row entirely and it is four;
/// build it with `Row::new` rather than `Row::heading` and the `heading`
/// assertion fails, which is the one that keeps `Enter` from answering the agent
/// with its own explanation.
#[test]
fn f4_a_description_takes_a_row_and_an_offer_without_one_takes_none() {
    let (mut overlay, _reply) = live(explained());

    let rows = overlay.offers().rows().to_vec();
    assert_eq!(
        rows.len(),
        5,
        "three offers, one description and the free-text row is five rows: {:?}",
        rows.iter().map(|row| &row.label).collect::<Vec<_>>(),
    );
    assert_eq!(rows[0].label, "created_at");
    assert_eq!(
        rows[1].label.trim(),
        "stamped once and never read since",
        "the description is not directly under the label it explains",
    );
    assert!(
        rows[1].heading,
        "the description is a selectable row, so Enter can answer the agent with \
         its own explanation of an offer",
    );
    assert_eq!(
        rows[2].label, "updated_at",
        "the offer with no description grew a row anyway",
    );
    assert_eq!(rows[3].label, "archived_at");
    assert_eq!(rows[4].label, io_cli::intent::OWN_WORDS);

    // And it reaches the screen — always, rather than under the marker. The
    // marker opens on the free-text row, so the preview on `archived_at` is
    // **not** open, which is the whole difference between the two fields.
    let screen = drawn_overlay(&mut overlay, &DARK);
    assert!(
        screen.contains("stamped once and never read since"),
        "the description reached the row list and not the screen: {screen}",
    );
    assert!(
        !screen.contains("ALTER TABLE"),
        "a preview is drawn without its offer holding the marker, which puts \
         every offer's block on the screen at once: {screen}",
    );
}

/// Two offers, each with a preview nothing else on the screen says.
fn two_previews() -> Question {
    Question::new("which migration should run first?").with_choices([
        Choice::new("widen the ledger")
            .preview("ALTER TABLE ledger\n  ALTER COLUMN amount TYPE numeric;"),
        Choice::new("backfill the ledger")
            .preview("UPDATE ledger\n  SET amount = 0 WHERE amount IS NULL;"),
    ])
}

/// **F5 — a preview unfolds under the offer that holds the marker, moving the
/// marker folds it and opens the next, and only one is ever open.**
///
/// Asserted three ways round, because each catches a different mistake: nothing
/// is open before an offer is marked, the marked offer's own block is the one
/// drawn, and the block belonging to the other offer is absent every time.
///
/// Sabotage: draw every configured preview rather than the open one, under which
/// the "two at once" assertions fail; key the unfold on the row that *was* marked
/// rather than the row that is, under which the second and third frames show the
/// wrong block.
#[test]
fn f5_a_preview_unfolds_under_the_marked_offer_and_only_one_is_ever_open() {
    let (mut overlay, _reply) = live(two_previews());

    let closed = drawn_overlay(&mut overlay, &DARK);
    assert!(
        !closed.contains("TYPE numeric") && !closed.contains("IS NULL"),
        "a preview is unfolded while the marker is still on the free-text row: \
         {closed}",
    );

    // Up from the free-text row lands on the second offer.
    overlay.key(key(KeyCode::Up));
    assert_eq!(overlay.offers().selected(), 1, "the marker did not move");
    let second = drawn_overlay(&mut overlay, &DARK);
    assert!(
        second.contains("IS NULL"),
        "the marked offer's preview is not drawn: {second}",
    );
    assert!(
        !second.contains("TYPE numeric"),
        "the other offer's preview is open at the same time: {second}",
    );

    overlay.key(key(KeyCode::Up));
    assert_eq!(overlay.offers().selected(), 0);
    let first = drawn_overlay(&mut overlay, &DARK);
    assert!(
        first.contains("TYPE numeric"),
        "moving the marker did not open the next preview: {first}",
    );
    assert!(
        !first.contains("IS NULL"),
        "the preview the marker left did not fold: {first}",
    );
}

/// **F5/O4 — `Enter` on an offer whose preview is open answers with the offer.**
///
/// The defect this closes is the one the release nearly shipped: through 0.32.0
/// the free-text row was the only row that unfolded anything, so "something is
/// unfolded" and "the operator is writing" were the same question and `Enter` was
/// routed by the first. Give an offer a preview and that test starts answering
/// `true` on the offers as well — `Enter` would go to the composer, which is
/// empty, and the keypress would do nothing at all.
///
/// Sabotage: route `Enter` on `Picker::unfolded_now` again, under which this
/// fails with `None` — the overlay still open and the run still stopped.
#[test]
fn f5_enter_on_an_offer_with_an_open_preview_answers_with_the_offer() {
    let (mut overlay, _reply) = live(two_previews());
    overlay.key(key(KeyCode::Up));
    assert_eq!(
        overlay.key(key(KeyCode::Enter)),
        Some(vec![Some("backfill the ledger".to_string())]),
        "Enter over an open preview did not answer with the offer under it",
    );
}

/// One offer, one preview, so the block's height is the only thing that moves.
fn one_preview(preview: &str) -> Question {
    Question::new("which migration?")
        .with_choices([Choice::new("drop three columns").preview(preview)])
}

/// A preview of **one logical line** that wraps to several rows at a narrow
/// width. The fixture is the test: a gate written against `lines.len()` sees one
/// row here and one row in [`SHORT`], and cannot tell them apart.
const LONG: &str =
    "ALTER TABLE ledger DROP COLUMN archived_at, DROP COLUMN expired_at, DROP COLUMN deleted_at;";

/// The control: one logical line that also *occupies* one row at that width.
const SHORT: &str = "DROP COLUMN archived_at;";

/// **F6 — the unfold is the WRAPPED height at the rendered width, not the count
/// of lines in the preview.**
///
/// This is the defect 0.32.0 paid for in two overlays: a `Paragraph` with
/// wrapping on occupies more rows than it has lines, and a surface that reserved
/// `lines.len()` drew its block over rows the list had been promised.
///
/// The two fixtures have the **same logical line count** and different wrapped
/// heights, so `lines.len()` cannot distinguish them and `rows::wrapped` must.
/// Both halves are asserted: the room asked for, and the last word of the preview
/// actually reaching the screen — a block reserved one row deep clips the rest
/// silently, which is exactly how this failure hides.
///
/// Sabotage: measure with `preview.lines().count()` instead of `rows::wrapped`,
/// under which the difference below is 0 and `deleted_at;` is off the screen.
#[test]
fn f6_the_unfold_is_the_wrapped_height_and_not_the_line_count() {
    assert_eq!(
        LONG.lines().count(),
        SHORT.lines().count(),
        "the fixtures differ in line count, so this proves nothing about wrapping",
    );

    let (long, _a) = live(one_preview(LONG));
    let (short, _b) = live(one_preview(SHORT));
    // Identical questions, identical labels, identical heads: the only thing that
    // can differ between these two numbers is the block.
    assert_eq!(
        long.rows_wanted(200, &DARK),
        short.rows_wanted(200, &DARK),
        "at a width that fits both previews on one row they must ask for the same",
    );
    let extra = long
        .rows_wanted(40, &DARK)
        .saturating_sub(short.rows_wanted(40, &DARK));
    assert!(
        extra >= 2,
        "a one-line preview that wraps asked for {extra} rows more than one that \
         does not, which is the `lines.len()` measurement this release replaced",
    );

    // And the whole block is on the screen, not just the room for it.
    let (mut overlay, _c) = live(one_preview(LONG));
    overlay.key(key(KeyCode::Up));
    let screen = drawn_overlay_at(&mut overlay, 40, 16, &DARK);
    assert!(
        screen.contains("deleted_at;"),
        "the tail of a wrapped preview was clipped, so the block was reserved \
         shorter than it draws: {screen}",
    );
}

/// **F6 — the room a preview asks for is the same number before and after the
/// frame that measures it.**
///
/// This is the ordering the criterion is about. A preview's height is a function
/// of the width, the width is not known until something draws, and the driver
/// reads the demand **before** it draws — so a measurement that only reaches
/// `Picker::set_unfold` from inside `render` arrives a frame late: the overlay
/// opens a block too short and grows under the operator's hands on their first
/// keystroke.
///
/// Sabotage, in both directions, and each fails only this test. Measure the
/// previews solely in `render` and the first number is short of the second.
/// Measure them in `rows_wanted` *and* leave the block the picker is now
/// reserving in the sum, and the second is larger than the first by the whole
/// block — a viewport that grows every frame until it hits the ceiling.
#[test]
fn f6_the_room_asked_for_is_the_same_before_and_after_the_first_frame() {
    let (mut overlay, _reply) = live(one_preview(LONG));

    let before = overlay.rows_wanted(40, &DARK);
    let _ = drawn_overlay_at(&mut overlay, 40, 16, &DARK);
    assert_eq!(
        overlay.rows_wanted(40, &DARK),
        before,
        "the room asked for changed once a frame had measured the preview",
    );

    // And it does not follow the marker either: the reservation is the largest
    // configured block, not the open one.
    overlay.key(key(KeyCode::Up));
    assert_eq!(
        overlay.rows_wanted(40, &DARK),
        before,
        "the demand moved when the marker did, which re-places the viewport on \
         every arrow key",
    );
}

/// **F5 — the quoted block is drawn in both glyph sets, and in each set's own
/// character.**
///
/// The vocabulary is the markdown blockquote's — `theme.glyphs.rule` and a space
/// — so it is `─ ` under Unicode and `- ` under ASCII, one cell plus a space
/// either way. Asserted with the preview's own first words attached, because the
/// ASCII `rule` is also the ASCII `dash` and a bare `contains("- ")` is satisfied
/// by the key line above the list.
///
/// Sabotage: write the prefix as a literal `| ` or `> ` rather than taking it off
/// the glyph set, under which both arms fail; take it off `glyphs.dash` instead
/// and the Unicode arm fails, because an em dash is not the rule.
#[test]
fn f5_the_quoted_block_is_drawn_in_both_glyph_sets() {
    let ascii = DARK.with_glyphs(io_cli::glyphs::ASCII);

    let (mut unicode_overlay, _a) = live(one_preview(SHORT));
    unicode_overlay.key(key(KeyCode::Up));
    let unicode = drawn_overlay(&mut unicode_overlay, &DARK);
    assert!(
        unicode.contains("\u{2500} DROP COLUMN archived_at;"),
        "the Unicode set did not quote the preview with its own rule: {unicode}",
    );

    let (mut ascii_overlay, _b) = live(one_preview(SHORT));
    ascii_overlay.key(key(KeyCode::Up));
    let drawn = drawn_overlay(&mut ascii_overlay, &ascii);
    assert!(
        drawn.contains("- DROP COLUMN archived_at;"),
        "the ASCII set did not quote the preview at all: {drawn}",
    );
    assert!(
        !drawn.contains('\u{2500}'),
        "the ASCII set drew a box-drawing character, which is a replacement box \
         on the terminal that asked for ASCII: {drawn}",
    );
}

/// **F9 — the free-text row is still last and still holds the marker once the
/// offers carry more than labels.**
///
/// The existing F9 test uses bare offers, where the free-text row's index is
/// `choices.len()` and a marker aimed at either number lands in the same place.
/// A described offer takes a row for its description, so the two numbers part
/// company — and aiming at `choices.len()` now puts the opening marker on a real
/// offer, which is exactly the reflexive-`Enter` failure F9 exists to prevent.
///
/// Sabotage: focus `choices.len()` rather than the last row, under which the
/// first arm's marker sits on `archived_at` — index 3 of 5 — and `Enter` agrees
/// with an offer nobody read.
#[tokio::test]
async fn f9_the_free_text_row_is_last_when_the_offers_carry_descriptions() {
    let (answerer, mut questions) = io_cli::intent::channel();
    let responder: Arc<dyn Responder> = Arc::new(answerer);
    let _asking = tokio::spawn(async move {
        let batch = vec![explained(), Question::new("and then what?")];
        responder.answer_all(&batch).await
    });

    let mut overlay = Intent::new(questions.recv().await.expect("the batch"));
    // Five rows on the first question — three offers, one description and the
    // free-text row — and one on the second.
    for expected in [5usize, 1] {
        let rows = overlay.offers().rows();
        assert_eq!(rows.len(), expected, "the list is not the shape F4 builds");
        assert_eq!(
            rows[expected - 1].label,
            io_cli::intent::OWN_WORDS,
            "the free-text row is not the last one",
        );
        assert_eq!(
            overlay.offers().selected(),
            expected - 1,
            "the marker opened on an offer rather than on the row that takes \
             prose, which turns a reflexive Enter into agreement",
        );
        overlay.key(key(KeyCode::Char('x')));
        overlay.key(key(KeyCode::Enter));
    }
}

/// **F4/F5 — an offer's own label is what the agent gets back, whichever row the
/// list drew it on.**
///
/// A description takes a row, so from 0.33.0 the row index the picker hands back
/// is no longer the choice index io-harness expects. Indexing `choices` with a
/// row answers the agent with a **different offer of the same question** —
/// plausible, wrong, and invisible on screen.
///
/// Sabotage: index `question.choices` with the row directly, under which `Enter`
/// on `updated_at` — row 2, choice 1 — answers `archived_at`.
#[test]
fn f4_choosing_an_offer_below_a_description_answers_with_that_offer() {
    let (mut overlay, _reply) = live(explained());
    // Home puts the marker on the first row, then two Downs step past the
    // description onto `updated_at`, which the picker refuses to stop on.
    overlay.key(key(KeyCode::Home));
    assert_eq!(overlay.offers().selected(), 0);
    overlay.key(key(KeyCode::Down));
    assert_eq!(
        overlay.offers().selected(),
        2,
        "the marker rested on a description row, which is not a choice",
    );
    assert_eq!(
        overlay.key(key(KeyCode::Enter)),
        Some(vec![Some("updated_at".to_string())]),
        "the answer is the offer under the marker, not the one at that row index",
    );
}

// ---------------------------------------------------------------------------
// 0.33.0 — a batch that PARKED resumes as the batch the agent asked, not as the
// row's rendering of it.
// ---------------------------------------------------------------------------

/// A batch the store itself parked as one row, written and read exactly as a
/// resume reads it.
///
/// **`Store::put_questions`, and that is the point of the fixture.** Every stored
/// test above goes through `put_question` — the *singular* writer — which leaves
/// the row's `questions` empty, so nothing before this ever handed
/// `Intent::resumed` a batch-shaped row. That gap is why a resumed batch shipped as
/// one accent line holding embedded newlines, question one's context presented as
/// everyone's, a picker with zero offers and `multiple` defaulted to false: the
/// code path was never given the shape it fails on.
fn stored_batch(questions: &[Question]) -> PendingQuestion {
    let store = io_harness::Store::memory().expect("an in-memory store");
    let run = store
        .start_run("port the migration", "openrouter")
        .expect("a run to hang the batch off");
    let id = store
        .put_questions(run, 1, questions)
        .expect("the row is written");
    store
        .question(id)
        .expect("the row reads back")
        .expect("the row is there")
}

/// The batch every test below resumes.
///
/// **The second question carries everything**, deliberately: its own context, its
/// own offers, a described offer and `multiple`. A fix that read `questions[0]` and
/// built one question out of it would satisfy every assertion about the first one
/// and fail every assertion about this one, which is the only reason the tests
/// below page across before they assert.
fn parked_batch() -> Vec<Question> {
    vec![
        Question::new("which database should the port target?")
            .with_context("both are already in the lockfile")
            .with_choices(["postgres", "sqlite"]),
        Question::new("which platforms should the build cover?")
            .with_context("the runner offers three and CI uses one")
            .with_choices([
                Choice::new("linux").describe("the only one CI has today"),
                Choice::new("windows"),
                Choice::new("macos"),
            ])
            .multiple(),
    ]
}

/// **A parked batch resumes as the batch, with every question's own offers,
/// context and `multiple` — reached through the live construction path.**
///
/// The defect: `Intent::resumed` built one `Question` out of the row's `question`,
/// `context` and `choices` columns and never read `questions`. For a batch those
/// three columns are a *rendering* — the whole ask as `"1. …\n2. …"`, the **first**
/// question's context, and no choices at all — so a resumed batch was one
/// `Tone::Accent` line with embedded newlines that ratatui does not break a `Line`
/// on, one question's context presented as everyone's, an empty picker, and
/// `multiple` false: no marks and no `Question::answer_of`. It is reachable through
/// this release's own flow, since `Esc` on a batch parks the run and `/resume`
/// opens it here, and `io exec` reaches it on the first ask.
///
/// Sabotage: build the batch from `pending.question`/`context`/`choices` as before,
/// under which the very first assertion fails on a question line that is the whole
/// numbered block. Sabotage by reading `pending.questions[0]` alone, under which
/// `PageDown` moves nothing and every assertion about the second question fails
/// while the first question's still pass.
#[test]
fn f2_a_parked_batch_resumes_as_the_batch_the_agent_asked() {
    let row = stored_batch(&parked_batch());
    // What the store actually holds, so the assertions below are known to be about
    // the shape the defect was reachable through rather than a fixture that
    // flattered the fix.
    assert!(
        row.choices.is_empty(),
        "put_questions writes no choices column, so a surface reading it draws a \
         picker with nothing to pick",
    );
    assert!(
        row.question.contains('\n'),
        "the row's question is the whole batch as numbered prose: {}",
        row.question,
    );
    assert_eq!(
        row.questions.len(),
        2,
        "and the ask itself is in `questions`"
    );

    let mut overlay = Intent::resumed(&row);
    assert_eq!(
        overlay.question().question,
        "which database should the port target?",
        "the overlay opened on the row's rendering rather than on the first \
         question the agent asked",
    );

    // Tall enough that nothing asserted here is a row the picker elided: a batch's
    // head is five lines before the offers start.
    let first = drawn_overlay_at(&mut overlay, 80, 20, &DARK);
    assert!(
        first.contains("question 1 of 2"),
        "a resumed batch has no batch chrome, so nothing on screen says another \
         question is waiting behind this one: {first}",
    );

    overlay.key(key(KeyCode::PageDown));
    assert_eq!(
        overlay.question().question,
        "which platforms should the build cover?",
        "PgDn moved nothing, so the resumed overlay is holding one question",
    );
    assert!(
        overlay.question().multiple,
        "the second question's `multiple` did not survive the store, so the \
         spacebar marks nothing and `Question::answer_of` never spells the answer",
    );

    let labels: Vec<&str> = overlay
        .offers()
        .rows()
        .iter()
        .map(|offer| offer.label.as_str())
        .collect();
    assert_eq!(
        labels,
        [
            "linux",
            "  the only one CI has today",
            "windows",
            "macos",
            io_cli::intent::OWN_WORDS,
        ],
        "the second question's offers, its described offer's own row, and the row \
         that takes prose",
    );

    let second = drawn_overlay_at(&mut overlay, 80, 20, &DARK);
    assert!(
        second.contains("the runner offers three and CI uses one"),
        "the second question is drawn under the FIRST question's context: {second}",
    );
    assert!(
        !second.contains("both are already in the lockfile"),
        "question one's context is presented as question two's: {second}",
    );
    assert!(
        second.contains("space marks several"),
        "the list did not opt into marks, so a `multiple` question cannot be \
         answered with more than one offer: {second}",
    );
}

/// **A resumed batch is delivered as ONE text, because it parked as one row.**
///
/// io-harness parks a whole `ask_questions` under a single `question_id` and
/// `answer_question` is a single compare-and-swap, so there is one text to hand
/// `resume_with_answer_observed` however many questions the operator worked
/// through. Every answer is paired with the question it answers, because a bare
/// list of sentences makes the model re-derive the pairing from position.
///
/// Sabotage: return the last answer alone — the shape a `Destination::Stored` per
/// question falls into when `resolve` just zips and keeps the last delivery — under
/// which the first question's words vanish and the run is resumed with half the ask
/// answered. Sabotage the pairing by joining the answers with no question text
/// between them, under which the equality fails on a block the model would have to
/// re-derive the pairing of from position.
#[test]
fn f2_a_resumed_batch_comes_back_as_one_text_for_the_one_row_it_parked_as() {
    let mut overlay = Intent::resumed(&stored_batch(&parked_batch()));

    for ch in "postgres, and drop the old one".chars() {
        overlay.key(key(KeyCode::Char(ch)));
    }
    assert_eq!(
        overlay.key(key(KeyCode::Enter)),
        None,
        "one question of two decided, and something was delivered anyway",
    );

    // On the second question, which takes several. Up from the row that takes
    // prose is `macos`, and one more is `windows`.
    overlay.key(key(KeyCode::Up));
    overlay.key(key(KeyCode::Char(' ')));
    overlay.key(key(KeyCode::Up));
    overlay.key(key(KeyCode::Char(' ')));
    let answers = overlay
        .key(key(KeyCode::Enter))
        .expect("both questions are decided, so the batch is delivered");
    assert_eq!(
        answers,
        vec![
            Some("postgres, and drop the old one".to_string()),
            Some(Question::answer_of(["windows", "macos"])),
        ],
    );

    let delivered = overlay
        .resolve(answers)
        .expect("a resumed row has no turn awaiting it: the caller resumes with this")
        .expect("every question was answered, so there is an answer");
    assert_eq!(
        delivered,
        format!(
            "1. which database should the port target?\n   postgres, and drop the old one\n\
             2. which platforms should the build cover?\n   {}",
            Question::answer_of(["windows", "macos"]),
        ),
        "every answer beside the question it answers, once, in the order asked",
    );
}

/// **One decline anywhere in a resumed batch delivers no answer at all**, and the
/// run stays parked on the row it was found on.
///
/// A batch is answered wholly or not at all — io-harness's own rule for the live
/// path, and it binds harder here: resolving the row is one compare-and-swap with
/// no second chance, so a text assembled around a hole would answer an ask the
/// operator did not finish. `Some(None)` is what the driver reads as *the operator
/// left it*, which is exactly what `Esc` has always promised.
///
/// Sabotage: assemble the text anyway with a placeholder for the missing answer,
/// under which this returns `Some(Some(..))` and the run is resumed with an answer
/// nobody gave.
#[test]
fn f2_one_decline_in_a_resumed_batch_leaves_the_run_parked() {
    let mut overlay = Intent::resumed(&stored_batch(&parked_batch()));

    for ch in "postgres".chars() {
        overlay.key(key(KeyCode::Char(ch)));
    }
    assert_eq!(overlay.key(key(KeyCode::Enter)), None);
    let answers = overlay
        .key(key(KeyCode::Esc))
        .expect("declining the last undecided question decides the batch");
    assert_eq!(answers, vec![Some("postgres".to_string()), None]);

    assert_eq!(
        overlay.resolve(answers),
        Some(None),
        "an answer was assembled around a question nobody answered",
    );
}

/// **A singular row is untouched by the batch path, including one written before
/// `questions` existed.**
///
/// `Store::put_question` writes no `questions` value at all, and a row written by
/// io-harness 0.71.0 has no such column — so the row's own `question`, `context` and
/// `choices` are the ask, exactly as they were, and reading `questions` first must
/// not change that. Its choices are bare labels, which io-harness serializes as a
/// plain JSON array of strings byte for byte as 0.71.0 wrote it (its own `Choice`
/// doctests gate that encoding); this asserts the consequence, which is that they
/// come back as selectable offers answering with their own labels.
///
/// Sabotage: read `pending.questions` unconditionally, under which a singular row
/// hands the constructor an empty batch and this panics on the first index before
/// it asserts anything. Sabotage the fallback's `with_choices`, under which the
/// offers are gone, `Home` rests on the row that takes prose, and the `Enter`
/// assertion answers nothing at all.
#[test]
fn f2_a_singular_row_still_opens_on_its_own_columns() {
    let row = stored(&question());
    assert!(
        row.questions.is_empty(),
        "the singular writer wrote a batch, so this fixture is not the shape it \
         claims to be",
    );
    assert!(
        row.choices
            .iter()
            .all(|choice| choice.description.is_none() && choice.preview.is_none()),
        "the fixture's offers are bare labels, which is the 0.71.0 spelling",
    );

    let mut overlay = Intent::resumed(&row);
    let screen = drawn_overlay(&mut overlay, &DARK);
    assert!(
        !screen.contains("question 1 of"),
        "a single question grew batch chrome: {screen}",
    );
    assert!(
        screen.contains("drop the column or keep it?") && screen.contains("created_at"),
        "the row's own columns are the ask: {screen}",
    );

    overlay.key(key(KeyCode::Home));
    assert_eq!(
        overlay.key(key(KeyCode::Enter)),
        Some(vec![Some("created_at".to_string())]),
        "a bare label round-tripped through the store is still an offer that \
         answers with itself",
    );
}
