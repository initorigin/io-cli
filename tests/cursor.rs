//! **F2 — a frame that accepts input sets a cursor position, on every surface.**
//!
//! ratatui hides the terminal cursor on any frame that does not set one: its
//! `Terminal::draw` ends in `match cursor_position { None => hide_cursor(), Some(p)
//! => { show_cursor(); set_cursor_position(p) } }`. A hidden cursor removes the
//! only focus indicator a screen reader has, and it is removed at exactly the
//! moments the operator is being asked to choose something.
//!
//! So the observable here is the **byte stream**, not the rendered buffer. A cell
//! grid cannot say whether the cursor was hidden — the sequences that hide and
//! show it (`ESC[?25l`, `ESC[?25h`) and the one that places it (`ESC[row;colH`)
//! are the whole subject, and `support::Fixed` is a real `CrosstermBackend`, so
//! what the recorder holds is what a terminal would have been sent. [`cursor_of`]
//! reads the position back out of it.
//!
//! Every assertion names its surface. A frame that forgets has to be findable
//! from the failure alone, which is why there is no aggregate "some surface
//! forgot" assertion anywhere in this file.

mod support;

use std::cell::Cell;

use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use io_harness::{Act, ApprovalContext, Approver, Decision, Request};
use ratatui::layout::{Position, Rect};

use io_cli::app::App;
use io_cli::approval::{self, Answer, Approval, Ask};
use io_cli::composer::PROMPT;
use io_cli::picker::{Picker, Row};
use io_cli::theme::DARK;
use io_cli::wizard::{Step, Wizard};

fn key(code: KeyCode) -> KeyEvent {
    KeyEvent::new(code, KeyModifiers::NONE)
}

/// One frame, and everything it can be asked about afterwards.
struct Drawn {
    /// Where the terminal was told to put the cursor, or `None` if the frame hid
    /// it — which is the whole failure this file exists to catch.
    cursor: Option<Position>,
    /// The rectangle the widget was laid out against. The inline viewport does
    /// not start at row zero, so every expectation is relative to this.
    area: Rect,
    text: String,
}

impl Drawn {
    /// The cursor, or a failure that names the surface that forgot it.
    fn position(&self, surface: &str) -> Position {
        self.cursor.unwrap_or_else(|| {
            panic!(
                "{surface}: the frame set no cursor position, so ratatui hid the \
                 terminal cursor and this surface has no focus indicator at all. \
                 The frame drew:\n{}",
                self.text
            )
        })
    }

    /// What the row under the cursor says, from the cursor rightwards.
    ///
    /// This is what a reader following the caret lands on, which is the only
    /// reason a *position* rather than a *presence* is worth asserting.
    fn row_from_cursor(&self, surface: &str) -> String {
        let at = self.position(surface);
        assert!(
            self.area.contains(at),
            "{surface}: the cursor was placed outside the frame at {at:?}, area {:?}",
            self.area,
        );
        let row = self
            .text
            .lines()
            .nth((at.y - self.area.y) as usize)
            .unwrap_or("");
        row.chars().skip((at.x - self.area.x) as usize).collect()
    }
}

/// Draw one frame on a screen of its own and read the cursor back off the wire.
///
/// A screen of its own per frame on purpose: `Screen::draw` skips a frame that
/// says exactly what the terminal is already showing, so a second identical draw
/// writes no bytes at all and the recorder would still be holding the previous
/// frame's answer.
fn draw(viewport: u16, render: impl FnOnce(&mut ratatui::Frame)) -> Drawn {
    draw_at(80, viewport, render)
}

/// The same, at a width the caller chooses, for the surfaces whose narrow form
/// is the thing under test.
fn draw_at(width: u16, viewport: u16, render: impl FnOnce(&mut ratatui::Frame)) -> Drawn {
    let (mut screen, recorder) = support::screen_of(width, 24, viewport);
    let area = Cell::new(Rect::new(0, 0, 0, 0));
    screen
        .draw(|frame| {
            area.set(frame.area());
            render(frame);
        })
        .expect("frame");
    Drawn {
        cursor: cursor_of(&recorder.text()),
        area: area.get(),
        text: screen.viewport_text().to_string(),
    }
}

/// Where the byte stream left the cursor, or `None` if it left it hidden.
///
/// Read from the end backwards: the last visibility sequence in the stream is the
/// one in force, and when it is a show, the placement ratatui writes immediately
/// after it is the frame's own. Nothing else can be between the two — `draw`
/// writes `show_cursor()` and `set_cursor_position()` back to back.
fn cursor_of(bytes: &str) -> Option<Position> {
    let shown = bytes.rfind("\x1b[?25h")?;
    if bytes
        .rfind("\x1b[?25l")
        .is_some_and(|hidden| hidden > shown)
    {
        return None;
    }
    let after = &bytes[shown + "\x1b[?25h".len()..];
    let start = after.find("\x1b[")? + 2;
    let end = after[start..].find('H')? + start;
    let (row, column) = after[start..end].split_once(';')?;
    Some(Position {
        // Both are one-based on the wire.
        x: column.parse::<u16>().ok()?.saturating_sub(1),
        y: row.parse::<u16>().ok()?.saturating_sub(1),
    })
}

/// A question already delivered to the interface, with the run still waiting on
/// the answer. Dropping the `Ask` unanswered denies, so the waiting task is kept
/// alive for as long as the overlay is.
async fn asked() -> (Ask, tokio::task::JoinHandle<Decision>) {
    let (asker, mut asks) = approval::channel();
    let deciding = tokio::spawn(async move {
        let asker = asker;
        asker
            .decide_in_context(
                &Request::new(Act::Write, "src/main.rs").with_content("fn main() {}\n"),
                &ApprovalContext::new("tidy the parser")
                    .flagged_by(Some("src/*.rs".into()), Some("app".into())),
            )
            .await
    });
    let ask = asks
        .recv()
        .await
        .expect("the question reached the interface");
    (ask, deciding)
}

/// **The regression guard.** The composer has set its own insertion point since
/// 0.1.0 and is the surface the other three are being brought up to.
#[test]
fn f2_the_composer_frame_sets_a_cursor_at_the_insertion_point() {
    let mut app = App::new(DARK, "opus-5");
    for character in "hi".chars() {
        app.key(key(KeyCode::Char(character)));
    }

    let drawn = draw(io_cli::term::VIEWPORT_HEIGHT, |frame| {
        app.render(frame, frame.area())
    });

    let at = drawn.position("the composer");
    assert_eq!(
        at.x,
        drawn.area.x + PROMPT.len() as u16 + 2,
        "the cursor belongs after the prompt marker and the two typed characters: {:?}",
        drawn.text,
    );
    assert!(
        drawn.area.contains(at),
        "the cursor left the viewport: {at:?} in {:?}",
        drawn.area,
    );
}

/// The overlay takes the whole viewport, so nothing else on the frame can set a
/// position on its behalf: `App::render` returns as soon as it has drawn.
#[tokio::test]
async fn f2_the_approval_overlay_frame_sets_a_cursor_on_the_chosen_answer() {
    let (ask, deciding) = asked().await;
    let mut approval = Approval::new(ask, std::path::Path::new(""));

    let drawn = draw(io_cli::term::VIEWPORT_HEIGHT, |frame| {
        approval.render(frame, frame.area(), &DARK)
    });
    assert!(
        drawn
            .row_from_cursor("the approval overlay")
            .starts_with(&format!("{} {}", Answer::Once.key(), Answer::Once.label())),
        "the cursor belongs on the answer Enter would take: {:?}",
        drawn.text,
    );

    // And it follows the selection rather than sitting on a fixed cell — the
    // reason to place it on the answers row at all.
    approval.key(key(KeyCode::Right));
    assert_eq!(approval.chosen(), Answer::Session);
    let moved = draw(io_cli::term::VIEWPORT_HEIGHT, |frame| {
        approval.render(frame, frame.area(), &DARK)
    });
    assert!(
        moved
            .row_from_cursor("the approval overlay")
            .starts_with(&format!(
                "{} {}",
                Answer::Session.key(),
                Answer::Session.label()
            )),
        "the cursor did not follow the selection: {:?}",
        moved.text,
    );

    approval.answer(Answer::Deny);
    deciding.await.expect("the approver did not panic");
}

/// **The surface the sabotage drops.** `paint_picker` draws the picker *instead
/// of* the app, so an open picker is a frame with no composer on it.
#[test]
fn f2_the_picker_frame_sets_a_cursor_on_the_selected_row() {
    let mut picker = Picker::new(
        "Which provider?",
        vec![
            Row::new("OpenRouter"),
            Row::new("Anthropic"),
            Row::new("OpenAI"),
        ],
    );

    let drawn = draw(6, |frame| picker.render(frame, frame.area(), &DARK));
    assert!(
        drawn
            .row_from_cursor("the picker")
            .starts_with("OpenRouter"),
        "the cursor belongs at the start of the selected row's label: {:?}",
        drawn.text,
    );

    picker.key(key(KeyCode::Down));
    picker.key(key(KeyCode::Down));
    let moved = draw(6, |frame| picker.render(frame, frame.area(), &DARK));
    assert_eq!(
        moved.position("the picker").y,
        drawn.position("the picker").y + 2,
        "the cursor did not follow the selection down the list: {:?}",
        moved.text,
    );
    assert!(
        moved.row_from_cursor("the picker").starts_with("OpenAI"),
        "the cursor is not on the row that is selected: {:?}",
        moved.text,
    );
}

/// A picker with more rows than it has room for scrolls, and the cursor is a
/// *screen* position — it has to follow the row to where it was actually drawn,
/// not to where it would be in an unscrolled list.
#[test]
fn f2_the_picker_cursor_follows_a_scrolled_row_to_where_it_was_drawn() {
    let rows: Vec<Row> = (0..40).map(|n| Row::new(format!("model-{n}"))).collect();
    let mut picker = Picker::new("Which model?", rows);
    for _ in 0..30 {
        picker.key(key(KeyCode::Down));
    }

    let drawn = draw(8, |frame| picker.render(frame, frame.area(), &DARK));
    assert!(
        drawn.row_from_cursor("the picker").starts_with("model-30"),
        "the cursor is not on the selected row of a scrolled list: {:?}",
        drawn.text,
    );
}

/// Every step of the wizard, enumerated rather than sampled.
///
/// [`name`] is an exhaustive match on purpose: a step added to `Step` stops this
/// file compiling until somebody says which frame it is and draws it here.
fn name(step: Step) -> &'static str {
    match step {
        Step::Welcome => "the wizard's welcome step",
        Step::Provider => "the wizard's provider step",
        Step::BaseUrl => "the wizard's base URL step",
        Step::Credential => "the wizard's credential step",
        Step::Verifying => "the wizard's verifying step",
        Step::Model => "the wizard's model step",
        Step::ModelText => "the wizard's typed-model step",
        Step::Theme => "the wizard's theme step",
        Step::Posture => "the wizard's posture step",
        Step::Confirm => "the wizard's confirmation step",
        Step::Done => "the wizard's done step",
        Step::Cancelled => "the wizard's cancelled step",
    }
}

const EVERY_STEP: [Step; 12] = [
    Step::Welcome,
    Step::Provider,
    Step::BaseUrl,
    Step::Credential,
    Step::Verifying,
    Step::Model,
    Step::ModelText,
    Step::Theme,
    Step::Posture,
    Step::Confirm,
    Step::Done,
    Step::Cancelled,
];

/// Draw whatever step the wizard is on, and assert it set a cursor. Returns the
/// step so the caller can record that it was covered.
fn step_sets_a_cursor(wizard: &mut Wizard) -> Step {
    let step = wizard.step();
    let drawn = draw(io_cli::term::WIZARD_VIEWPORT_HEIGHT, |frame| {
        wizard.render(frame, frame.area())
    });
    drawn.position(name(step));
    step
}

#[test]
fn f2_every_wizard_step_sets_a_cursor() {
    // `Step::Done` is only reachable through the confirmation screen, which asks
    // `settings::user_path` where the file would go. Pointed at a temporary
    // directory so the answer is a path rather than `None` — nothing is written
    // here, because writing is the driver's half of `Progress::Write`.
    let home = tempfile::tempdir().expect("a temporary directory");
    std::env::set_var("IO_CONFIG_HOME", home.path());
    std::env::remove_var("IO_CONFIG");

    let mut seen = Vec::new();

    // The vendor path: a provider with a catalogue behind it.
    let mut wizard = Wizard::new(DARK);
    seen.push(step_sets_a_cursor(&mut wizard)); // Welcome
    wizard.key(key(KeyCode::Enter));
    seen.push(step_sets_a_cursor(&mut wizard)); // Provider
    wizard.key(key(KeyCode::Enter));
    seen.push(step_sets_a_cursor(&mut wizard)); // Credential
    wizard.paste("sk-not-a-real-key");
    wizard.key(key(KeyCode::Enter));
    seen.push(step_sets_a_cursor(&mut wizard)); // Verifying
    wizard.verified();
    wizard.catalogue(vec!["anthropic/claude-sonnet-4".to_string()]);
    seen.push(step_sets_a_cursor(&mut wizard)); // Model
    wizard.key(key(KeyCode::Enter));
    seen.push(step_sets_a_cursor(&mut wizard)); // Theme
    wizard.key(key(KeyCode::Enter));
    seen.push(step_sets_a_cursor(&mut wizard)); // Posture
    wizard.key(key(KeyCode::Enter));
    seen.push(step_sets_a_cursor(&mut wizard)); // Confirm
    wizard.key(key(KeyCode::Enter));
    assert_eq!(wizard.step(), Step::Done, "the walk did not reach the end");
    seen.push(step_sets_a_cursor(&mut wizard)); // Done

    // The compatible path: the two steps the vendor path never renders.
    let mut wizard = Wizard::new(DARK);
    wizard.key(key(KeyCode::Enter));
    for _ in 0..3 {
        wizard.key(key(KeyCode::Down));
    }
    wizard.key(key(KeyCode::Enter));
    assert_eq!(wizard.step(), Step::BaseUrl);
    seen.push(step_sets_a_cursor(&mut wizard)); // BaseUrl
    wizard.paste("http://localhost:11434/v1");
    wizard.key(key(KeyCode::Enter));
    wizard.paste("sk-not-a-real-key");
    wizard.key(key(KeyCode::Enter));
    wizard.verified();
    // No catalogue and no default model to fall back on, which is the one route
    // to the typed-model step.
    wizard.catalogue(Vec::new());
    assert_eq!(wizard.step(), Step::ModelText);
    seen.push(step_sets_a_cursor(&mut wizard)); // ModelText

    let mut wizard = Wizard::new(DARK);
    wizard.key(key(KeyCode::Esc));
    assert_eq!(wizard.step(), Step::Cancelled);
    seen.push(step_sets_a_cursor(&mut wizard)); // Cancelled

    for step in EVERY_STEP {
        assert!(
            seen.contains(&step),
            "{} was never drawn, so F2 says nothing about it",
            name(step),
        );
    }
}

/// A viewport too short for the field is exactly when a reader most needs the
/// caret, and it is the case the old `if area.height > used` guard dropped: the
/// prompt takes the only row, the input is not drawn, and the frame ends with no
/// position on it at all.
#[test]
fn f2_a_wizard_step_too_short_for_its_field_still_sets_a_cursor() {
    let mut wizard = Wizard::new(DARK);
    wizard.key(key(KeyCode::Enter));
    wizard.key(key(KeyCode::Enter));
    assert_eq!(wizard.step(), Step::Credential);

    let drawn = draw(1, |frame| wizard.render(frame, frame.area()));
    drawn.position("the wizard's credential step in a one-row viewport");
}

/// The same for the overlay: at two rows there is no room for the answers line,
/// and the frame still has to say where the caret is.
#[tokio::test]
async fn f2_an_approval_too_short_for_its_answers_still_sets_a_cursor() {
    let (ask, deciding) = asked().await;
    let approval = Approval::new(ask, std::path::Path::new(""));

    let drawn = draw(2, |frame| approval.render(frame, frame.area(), &DARK));
    let at = drawn.position("the approval overlay in a two-row viewport");
    assert!(
        drawn.area.contains(at),
        "the cursor left the overlay: {at:?} in {:?}",
        drawn.area,
    );

    approval.answer(Answer::Deny);
    deciding.await.expect("the approver did not panic");
}

/// The composer's own narrow form, which was the last frame in the product that
/// hid the caret.
///
/// `Composer::render` returned early when the area was too narrow to hold the
/// prompt marker and any text beside it, and returning meant setting no cursor
/// position at all — so the one surface that already knew why the caret matters
/// had a shape where it dropped it anyway. A terminal two columns wide is a
/// degenerate case, and it is also exactly the case where a reader has least
/// else to go on.
#[test]
fn f2_the_composer_frame_sets_a_cursor_even_when_it_is_too_narrow_to_draw() {
    let mut app = App::new(DARK, "opus-5");
    app.key(key(KeyCode::Char('x')));

    let drawn = draw_at(2, io_cli::term::VIEWPORT_HEIGHT, |frame| {
        app.render(frame, frame.area())
    });

    let at = drawn.position("the composer at two columns");
    assert!(
        drawn.area.contains(at),
        "the cursor left the viewport: {at:?} in {:?}",
        drawn.area,
    );
}

// ---------------------------------------------------------------------------
// F12 — the frames 0.7.0 added.
//
// The palette, the completion list and the filtered picker are all `Picker`,
// which is the claim rather than a redundancy: every product that grows a
// completion list grows a second overlay for it, and a second overlay is a second
// frame that can forget the caret. There is one widget here, so there is one
// place the caret is set — and the two states below are the ones that widget had
// never been in before this release.
//
// The plan block and the `!` shell block are deliberately absent. Both are
// committed into the terminal's own scrollback rather than drawn, so neither is a
// frame and there is nothing for ratatui to hide a cursor on; their eighty-column
// audit is in `tests/narrow.rs`.
// ---------------------------------------------------------------------------

/// The palette as the driver opens it, over the command inventory alone.
fn palette() -> Picker {
    Picker::new(
        "Which command?",
        io_cli::commands::palette(&io_harness::Templates::none(), &io_harness::Skills::none()),
    )
}

fn type_at(picker: &mut Picker, text: &str) {
    for character in text.chars() {
        picker.key(key(KeyCode::Char(character)));
    }
}

/// **F12.** The palette sets a caret, and a query moves it to the row the ranking
/// left under the marker rather than to the row that was there before.
///
/// `Picker` draws the match set in ranked order and holds its intended row as an
/// index into the caller's own list, so a caret placed from an unfiltered index
/// would sit on whichever row happened to fall under it — pointing a reader at
/// one command while `Enter` took another.
#[test]
fn f2_the_slash_palette_sets_a_cursor_on_the_row_the_query_left_under_the_marker() {
    let mut picker = palette();

    let opened = draw(io_cli::term::VIEWPORT_HEIGHT, |frame| {
        picker.render(frame, frame.area(), &DARK)
    });
    assert!(
        opened
            .row_from_cursor("the slash palette")
            .starts_with("help"),
        "the cursor belongs at the start of the row Enter would take: {:?}",
        opened.text,
    );

    type_at(&mut picker, "fork");
    let filtered = draw(io_cli::term::VIEWPORT_HEIGHT, |frame| {
        picker.render(frame, frame.area(), &DARK)
    });
    assert!(
        filtered
            .row_from_cursor("the filtered slash palette")
            .starts_with("fork"),
        "the cursor stayed at a position instead of following the row the query \
         left under the marker: {:?}",
        filtered.text,
    );
}

/// **F12.** A query that admits nothing still sets a caret.
///
/// The frame this release added and the one most easily left without one: there
/// is no selected row for a cursor to sit on, so the obvious implementation sets
/// none — and a caret that disappears the moment the list goes empty removes the
/// only focus indicator at exactly the moment the operator is trying to work out
/// whether the thing has broken.
///
/// Asserted on the caret's **row** rather than its column on purpose. The column
/// is the marker's width, which is right for a row with a marker in front of it
/// and puts the caret two characters into the sentence here; that is recorded as
/// a finding rather than pinned as the contract.
#[test]
fn f2_a_picker_whose_query_matches_nothing_still_sets_a_cursor() {
    let mut picker = palette();
    type_at(&mut picker, "zzzz");
    assert_eq!(
        picker.matching(),
        0,
        "the fixture must really admit nothing, or the line under audit is never \
         drawn",
    );

    let drawn = draw(io_cli::term::VIEWPORT_HEIGHT, |frame| {
        picker.render(frame, frame.area(), &DARK)
    });
    let at = drawn.position("a picker whose query matches nothing");
    assert!(
        drawn.area.contains(at),
        "the cursor left the frame: {at:?} in {:?}",
        drawn.area,
    );

    let row = drawn
        .text
        .lines()
        .nth((at.y - drawn.area.y) as usize)
        .unwrap_or("");
    assert!(
        row.starts_with("No row matches"),
        "the caret belongs on the only line there is, which is the one saying why \
         the list is empty: {:?}",
        drawn.text,
    );
}

/// **F12.** The `@` completion picker sets a caret on the path `Enter` would take.
#[test]
fn f2_the_completion_picker_frame_sets_a_cursor_on_the_selected_path() {
    let dir = tempfile::tempdir().expect("a temporary directory");
    std::fs::write(dir.path().join("notes.md"), "what to do\n").expect("a file");
    std::fs::create_dir(dir.path().join("src")).expect("a directory");

    let (found, _cut) =
        io_cli::complete::entries(dir.path(), &io_harness::Policy::permissive(), "")
            .expect("a listing");
    let rows = io_cli::complete::rows(&found);
    let first = rows
        .first()
        .expect("the fixture is not empty")
        .label
        .clone();
    let mut picker = Picker::new(io_cli::complete::title("", &DARK.glyphs), rows);

    let drawn = draw(io_cli::term::VIEWPORT_HEIGHT, |frame| {
        picker.render(frame, frame.area(), &DARK)
    });
    assert!(
        drawn
            .row_from_cursor("the completion picker")
            .starts_with(&first),
        "the cursor belongs at the start of the path Enter would take: {:?}",
        drawn.text,
    );
}

/// **F12.** The composer sets its caret past the placeholder *as drawn*, not past
/// the block it stands for.
///
/// A paste over the threshold is one line on screen and its whole text inside the
/// composer, so the two disagree by however large the paste was. A caret computed
/// from `Composer::text` would land hundreds of columns off the right-hand edge of
/// an eighty-column terminal.
#[test]
fn f2_the_composer_sets_a_cursor_past_a_collapsed_paste() {
    let pasted = "x".repeat(io_cli::composer::PASTE_THRESHOLD + 1);
    let placeholder = format!("[pasted text #1, {} characters]", pasted.chars().count());

    let mut app = App::new(DARK, "opus-5");
    assert!(
        app.paste(&pasted, false),
        "nothing was open, so the paste had nowhere to go but the composer",
    );

    let drawn = draw(io_cli::term::VIEWPORT_HEIGHT, |frame| {
        app.render(frame, frame.area())
    });
    let at = drawn.position("the composer holding a collapsed paste");
    assert_eq!(
        at.x,
        drawn.area.x + PROMPT.len() as u16 + placeholder.chars().count() as u16,
        "the caret belongs after the placeholder as it is drawn: {:?}",
        drawn.text,
    );
    assert!(
        drawn.area.contains(at),
        "the cursor left the viewport: {at:?} in {:?}",
        drawn.area,
    );
}
