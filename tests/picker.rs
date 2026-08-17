//! The `Picker`. Every selection surface in the product is this widget, so its
//! behaviour is asserted once here rather than in each caller.

mod support;

use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use io_cli::picker::{Outcome, Picker, Row};
use io_cli::theme::DARK;

fn key(code: KeyCode) -> KeyEvent {
    KeyEvent::new(code, KeyModifiers::NONE)
}

fn providers() -> Picker {
    Picker::new(
        "Which provider?",
        vec![
            Row::with_detail("OpenRouter", "one key, most models"),
            Row::with_detail("Anthropic", "Claude, direct"),
            Row::with_detail("OpenAI", "GPT, direct"),
            Row::with_detail("Any OpenAI-compatible endpoint", "a base URL of your own"),
        ],
    )
}

#[test]
fn the_arrows_move_and_stop_at_both_ends() {
    let mut picker = providers();
    assert_eq!(picker.selected(), 0);

    picker.key(key(KeyCode::Up));
    assert_eq!(picker.selected(), 0, "the first row is the top of the list");

    picker.key(key(KeyCode::Down));
    picker.key(key(KeyCode::Down));
    assert_eq!(picker.selected(), 2);

    picker.key(key(KeyCode::Down));
    picker.key(key(KeyCode::Down));
    assert_eq!(
        picker.selected(),
        3,
        "the last row is the bottom of the list"
    );
}

#[test]
fn enter_chooses_and_escape_backs_out() {
    let mut picker = providers();
    picker.key(key(KeyCode::Down));
    assert_eq!(picker.key(key(KeyCode::Enter)), Outcome::Chosen(1));
    assert_eq!(picker.key(key(KeyCode::Esc)), Outcome::Cancelled);
}

#[test]
fn control_c_backs_out_exactly_as_escape_does() {
    // The keybinding table promises `Ctrl+C` interrupts, and exits from an empty
    // prompt. A picker owns the keyboard while it is open, so a `Ctrl+C` it
    // swallows makes that promise false and leaves the only way out of the
    // overlay a key the table never mentions.
    let mut picker = providers();
    picker.key(key(KeyCode::Down));
    assert_eq!(
        picker.key(KeyEvent::new(KeyCode::Char('c'), KeyModifiers::CONTROL)),
        Outcome::Cancelled,
    );
    // The same outcome as `Esc`, deliberately: the picker closes and the next
    // press reaches the app, where `Ctrl+C` means what the table says.
    assert_eq!(picker.key(key(KeyCode::Esc)), Outcome::Cancelled);
    // Unmodified `c` is not a way out — it is a letter, and one day a filter.
    let mut picker = providers();
    assert_eq!(picker.key(key(KeyCode::Char('c'))), Outcome::Idle);
    assert_eq!(picker.selected(), 0);
}

#[test]
fn an_empty_picker_cannot_be_chosen_from() {
    let mut picker = Picker::new("Nothing here", vec![]);
    assert_eq!(picker.key(key(KeyCode::Enter)), Outcome::Idle);
    assert_eq!(picker.key(key(KeyCode::Down)), Outcome::Idle);
    assert_eq!(picker.selected(), 0);
}

#[test]
fn it_opens_on_the_row_the_caller_names() {
    let picker = providers().selecting(2);
    assert_eq!(picker.selected(), 2);
    // Out of range is clamped rather than panicking: the caller is passing an
    // index derived from configuration, which can name a row that no longer
    // exists.
    assert_eq!(providers().selecting(99).selected(), 3);
}

#[test]
fn a_long_list_scrolls_so_the_selection_stays_visible() {
    let rows: Vec<Row> = (0..40).map(|n| Row::new(format!("model-{n}"))).collect();
    let mut picker = Picker::new("Which model?", rows);
    let (mut screen, _recorder) = support::screen_of(80, 24, 8);

    for _ in 0..30 {
        picker.key(key(KeyCode::Down));
    }
    assert_eq!(picker.selected(), 30);

    screen
        .draw(|frame| picker.render(frame, frame.area(), &DARK))
        .expect("frame");

    let viewport = screen.viewport_text();
    assert!(
        viewport.contains("model-30"),
        "the selected row scrolled out of the picker: {viewport:?}",
    );
    assert!(
        !viewport.contains("model-0\n"),
        "the list did not scroll at all: {viewport:?}",
    );
    assert!(
        viewport.contains("Which model?"),
        "the title is what says what is being chosen",
    );
}

#[test]
fn f9_a_row_too_wide_for_eighty_columns_loses_its_detail_not_its_label() {
    let mut picker = Picker::new(
        "Which provider?",
        vec![Row::with_detail(
            "Any OpenAI-compatible endpoint",
            "a base URL of your own, which is how a proxy, a gateway or a local runtime is reached",
        )],
    );
    let (mut screen, _recorder) = support::screen_of(80, 24, 6);

    screen
        .draw(|frame| picker.render(frame, frame.area(), &DARK))
        .expect("frame");

    let viewport = screen.viewport_text();
    assert!(
        viewport.contains("Any OpenAI-compatible endpoint"),
        "the label was cut: {viewport:?}",
    );
    assert!(
        viewport.contains('…'),
        "the detail should be shortened with a marker: {viewport:?}",
    );
    for line in viewport.lines() {
        assert!(
            line.chars().count() <= 80,
            "a row overflowed eighty columns: {line:?}",
        );
    }
    // Counting the drawn rows, not the viewport's own blank remainder.
    assert_eq!(
        viewport.lines().filter(|line| !line.is_empty()).count(),
        2,
        "the row wrapped instead of being fitted: {viewport:?}",
    );
}

#[test]
fn the_selected_row_is_marked_by_more_than_colour() {
    let mut picker = providers();
    picker.key(key(KeyCode::Down));
    let (mut screen, _recorder) = support::screen_of(80, 24, 6);
    screen
        .draw(|frame| picker.render(frame, frame.area(), &DARK))
        .expect("frame");

    let viewport = screen.viewport_text();
    let marked: Vec<&str> = viewport
        .lines()
        .filter(|line| line.starts_with('›'))
        .collect();
    assert_eq!(
        marked.len(),
        1,
        "exactly one row carries the marker: {viewport:?}",
    );
    assert!(marked[0].contains("Anthropic"), "got {:?}", marked[0]);
}
