//! The `Picker`. Every selection surface in the product is this widget, so its
//! behaviour is asserted once here rather than in each caller.

mod support;

use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use io_cli::app::App;
use io_cli::picker::{Outcome, Picker, Row};
use io_cli::theme::DARK;
use ratatui::text::Line;

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
    // Unmodified `c` is not a way out — it is a letter, and as of 0.7.0 it is the
    // filter the comment here used to be waiting for. So the assertion that used
    // to read `selected() == 0` now reads that the marker sits on a row the query
    // admits: `c` narrows the list, and row 0 has no `c` in it.
    let mut picker = providers();
    assert_eq!(picker.key(key(KeyCode::Char('c'))), Outcome::Idle);
    assert_eq!(picker.query(), "c");
    assert!(
        picker.rows()[picker.selected()]
            .label
            .to_lowercase()
            .contains('c'),
        "the marker is on a row the query does not admit: {:?}",
        picker.rows()[picker.selected()].label,
    );
}

#[test]
fn f8_typing_narrows_the_rows_and_backspace_widens_them_again() {
    let mut picker = providers();
    assert_eq!(picker.matching(), 4);

    picker.key(key(KeyCode::Char('a')));
    picker.key(key(KeyCode::Char('n')));
    assert_eq!(picker.query(), "an");
    // `Anthropic` and `Any OpenAI-compatible endpoint` carry an `a` then an `n`;
    // `OpenRouter` has no `a` at all and `OpenAI` has no `n` after its `A`.
    assert_eq!(picker.matching(), 2, "the query did not narrow the list");

    picker.key(key(KeyCode::Backspace));
    assert_eq!(picker.query(), "a");
    assert_eq!(picker.matching(), 3, "backspace did not widen the list");

    picker.key(key(KeyCode::Backspace));
    assert_eq!(picker.query(), "");
    assert_eq!(picker.matching(), 4, "an empty query admits every row");
    // A backspace at an empty query is not an error and not a way out.
    assert_eq!(picker.key(key(KeyCode::Backspace)), Outcome::Idle);
    assert_eq!(picker.matching(), 4);
}

#[test]
fn f8_the_chosen_index_addresses_the_callers_rows_and_not_the_filtered_view() {
    // The defect this exists for, and it is the expensive one: three call sites
    // index a slice raw with what `Chosen` carries — `Kind::ALL[index]`,
    // `Posture::ALL[index]`, `open.rows()[index]` — and `/resume` does
    // `ids.get(index)`. A picker handing back a position in its filtered view
    // panics at the first two and silently resumes the wrong session at the last.
    let mut picker = providers();
    // `c` admits `Anthropic` (row 1) and `Any OpenAI-compatible endpoint` (row 3),
    // and neither is row 0 — which is exactly what a filtered index would report.
    picker.key(key(KeyCode::Char('c')));
    assert_eq!(picker.matching(), 2);

    let Outcome::Chosen(index) = picker.key(key(KeyCode::Enter)) else {
        panic!("Enter on a matched row must choose it");
    };
    assert!(
        matches!(index, 1 | 3),
        "the index must address the caller's own rows: got {index}",
    );
    assert!(
        picker.rows()[index].label.to_lowercase().contains('c'),
        "the chosen row is not one the query admits: {:?}",
        picker.rows()[index].label,
    );

    // The other matched row is reachable, and reports its own row index too.
    picker.key(key(KeyCode::Down));
    let Outcome::Chosen(next) = picker.key(key(KeyCode::Enter)) else {
        panic!("the arrows still move inside a filtered list");
    };
    assert_ne!(next, index, "Down did not move within the filtered rows");
    assert!(matches!(next, 1 | 3), "got {next}");
}

#[test]
fn f9_the_label_read_back_is_the_row_that_was_marked() {
    // `src/main.rs:360` does `open.rows()[index]` — a **raw** slice index, so a
    // stale index panics the session rather than misbehaving in it — and hands
    // the label it gets to `/theme` and `/model`, which apply it. Those rows are
    // built here the way `Action::Theme` builds them.
    let themes = || Picker::new("Which theme?", vec![Row::new("dark"), Row::new("light")]);

    let mut picker = themes();
    // `l` admits `light` and not `dark`, so the list is one row long and that row
    // is row 1 — while the filtered position is 0, which is `dark`.
    picker.key(key(KeyCode::Char('l')));
    assert_eq!(picker.matching(), 1);

    // What the operator can see, through the terminal the product actually
    // writes to, so the row the assertion below names is a row that was drawn.
    let (mut screen, recorder) = support::screen_of(80, 24, 6);
    screen
        .draw(|frame| picker.render(frame, frame.area(), &DARK))
        .expect("frame");
    assert!(recorder.contains("light"));
    assert!(
        !screen.viewport_text().contains("dark"),
        "the query did not narrow the list, so the choice below proves nothing: {:?}",
        screen.viewport_text(),
    );

    let Outcome::Chosen(index) = picker.key(key(KeyCode::Enter)) else {
        panic!("Enter on a matched row must choose it");
    };
    assert_eq!(
        picker.rows()[index].label,
        "light",
        "the driver would have applied a theme that was not on the screen",
    );

    // The control, and the reason the assertion above has to be made against a
    // filtered list: with nothing typed the two index spaces are the same list,
    // so this passes whichever of them `Chosen` carries.
    let mut picker = themes();
    picker.key(key(KeyCode::Down));
    assert_eq!(picker.key(key(KeyCode::Enter)), Outcome::Chosen(1));
    assert_eq!(picker.rows()[1].label, "light");
}

#[test]
fn f9_resume_and_fork_key_their_ids_by_the_row_the_operator_saw() {
    // `/resume` and `/fork` are the two sites that do **not** panic on a stale
    // index. Each builds a `Vec<i64>` alongside its rows and reads it back with
    // `ids.get(index)`, so a filtered index reopens a different conversation, or
    // branches from a different turn, and says nothing at all — and `/fork` then
    // prints `index + 1` as the turn number, naming the turn it did not take.
    //
    // Neither arm can be driven from a test as it stands: the selection lives in
    // the event loop in `src/main.rs`, which no integration test links. What is
    // asserted here is the property both arms rest on entirely, against lists
    // built and paired the way `sessions::rows` and `sessions::turn_rows` pair
    // theirs.
    let ids: Vec<i64> = vec![41, 42, 43, 44];
    let mut picker = Picker::new(
        "Resume which session?",
        vec![
            Row::new("~/code/io-gateway   3 turns"),
            Row::new("~/code/io-harness   9 turns"),
            Row::new("~/code/io-cli       1 turn"),
            Row::new("~/notes             2 turns"),
        ],
    );
    // `harn` admits the second row alone: it is the only label with an `h` in it.
    for character in "harn".chars() {
        picker.key(key(KeyCode::Char(character)));
    }
    assert_eq!(picker.matching(), 1);

    let Outcome::Chosen(index) = picker.key(key(KeyCode::Enter)) else {
        panic!("Enter on a matched row must choose it");
    };
    assert_eq!(
        ids.get(index),
        Some(&42),
        "a session other than the visible one would have been reopened, silently",
    );
    // The same index is `/fork`'s turn number, one-based. A row that resolves to
    // the wrong id also announces the wrong turn, which is the only thing the
    // operator would have to notice the swap by.
    assert_eq!(
        index + 1,
        2,
        "the turn number printed is not the visible row's place in the caller's list",
    );
}

#[test]
fn f8_j_and_k_are_query_characters_and_the_arrows_are_what_move() {
    // A deliberate, documented behaviour change. `j` and `k` moved the marker
    // until 0.7.0; they are printable, and a picker that held back two letters
    // would be discovered by an operator typing a model name and watching the
    // list jump instead of narrow. The keybinding table has only ever named the
    // arrows, so the documented way to move is the way that still works.
    let mut picker = providers();
    picker.key(key(KeyCode::Char('j')));
    assert_eq!(picker.query(), "j", "`j` is a query character now");
    picker.key(key(KeyCode::Backspace));

    picker.key(key(KeyCode::Char('k')));
    assert_eq!(picker.query(), "k", "`k` is a query character now");
    picker.key(key(KeyCode::Backspace));

    picker.key(key(KeyCode::Down));
    assert_eq!(picker.selected(), 1, "the arrows still move");
    picker.key(key(KeyCode::Up));
    assert_eq!(picker.selected(), 0);
    // `Home` and `End` are movement too, and are never typed into the query.
    picker.key(key(KeyCode::End));
    assert_eq!(picker.selected(), 3);
    assert_eq!(picker.query(), "");
}

#[test]
fn f8_escape_cancels_the_picker_rather_than_clearing_the_query() {
    // An operator who has learned one escape should not find two. `tests/wizard.rs`
    // asserts `Esc` leaves at every depth, and a picker whose `Esc` swallowed
    // itself to clear a filter would make that false on four of those screens.
    let mut picker = providers();
    picker.key(key(KeyCode::Char('a')));
    assert_eq!(picker.key(key(KeyCode::Esc)), Outcome::Cancelled);
    // `Ctrl+C` is unchanged by the filter as well.
    let mut picker = providers();
    picker.key(key(KeyCode::Char('a')));
    assert_eq!(
        picker.key(KeyEvent::new(KeyCode::Char('c'), KeyModifiers::CONTROL)),
        Outcome::Cancelled,
    );
}

#[test]
fn f8_the_query_is_drawn_in_place_of_the_title_and_costs_no_row() {
    // The sabotage: give the query a line of its own. The in-session viewport is
    // a height fixed at attach, so a query line above the title would leave
    // `/resume` two visible rows — and three was already the count a live run
    // found unusable. Both halves are asserted here: the line count, and that the
    // first matched row is drawn immediately under the query.
    let mut picker = providers();
    picker.key(key(KeyCode::Char('a')));
    picker.key(key(KeyCode::Char('n')));

    let (mut screen, recorder) = support::screen_of(80, 24, 6);
    screen
        .draw(|frame| picker.render(frame, frame.area(), &DARK))
        .expect("frame");

    let viewport = screen.viewport_text();
    let drawn: Vec<&str> = viewport.lines().filter(|line| !line.is_empty()).collect();
    assert_eq!(
        drawn.len(),
        3,
        "one line for the query and one for each of the two matches: {viewport:?}",
    );
    assert_eq!(drawn[0], "an", "the query did not take the title's line");
    assert!(
        !viewport.contains("Which provider?"),
        "the title is replaced while a query is being typed: {viewport:?}",
    );
    assert!(
        drawn[1].starts_with(DARK.glyphs.marker),
        "the first match is not immediately under the query: {drawn:?}",
    );
    assert!(
        recorder.contains("Anthropic"),
        "the matched row never reached the terminal",
    );

    // Clearing the query puts the title back, on the same one line.
    picker.key(key(KeyCode::Backspace));
    picker.key(key(KeyCode::Backspace));
    screen
        .draw(|frame| picker.render(frame, frame.area(), &DARK))
        .expect("frame");
    assert!(
        screen.viewport_text().contains("Which provider?"),
        "the title did not come back: {:?}",
        screen.viewport_text(),
    );
}

#[test]
fn f8_a_query_that_matches_nothing_says_so_rather_than_drawing_blank_rows() {
    // What an empty picker renders today is a title over blank rows, which reads
    // as a widget that has broken rather than as a query nothing is spelled like.
    let mut picker = providers();
    picker.key(key(KeyCode::Char('z')));
    assert_eq!(picker.matching(), 0);

    let (mut screen, recorder) = support::screen_of(80, 24, 6);
    screen
        .draw(|frame| picker.render(frame, frame.area(), &DARK))
        .expect("frame");

    let viewport = screen.viewport_text();
    assert!(
        viewport.contains("No row matches"),
        "an empty result needs a stated form: {viewport:?}",
    );
    assert!(recorder.contains("No row matches"));
    assert!(
        !viewport
            .lines()
            .any(|line| line.starts_with(DARK.glyphs.marker)),
        "there is nothing to mark: {viewport:?}",
    );

    // Nothing is under the marker, so Enter takes nothing — the same guard an
    // empty row list has always had, reached by the other route.
    assert_eq!(picker.key(key(KeyCode::Enter)), Outcome::Idle);
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
fn f9_a_query_that_admits_nothing_does_not_destroy_the_opening_row() {
    // Every caller that opens on a row means it: `/fork` opens on the newest turn,
    // `/model` on the model in use, the wizard's theme step on the theme in use.
    // The marker used to be remembered as a row read out of the CURRENT match set,
    // so a query admitting nothing had nothing to read — the opening row was gone
    // after one keystroke, and the backspace that put the list back put the marker
    // on row 0. `/fork` then branched from turn 0 and discarded the conversation.
    let mut picker = providers().selecting(3);
    assert_eq!(picker.selected(), 3);

    // `z` is in no provider label, so there is no row under the marker at all.
    picker.key(key(KeyCode::Char('z')));
    assert_eq!(picker.matching(), 0);

    picker.key(key(KeyCode::Backspace));
    assert_eq!(picker.query(), "");
    assert_eq!(picker.matching(), 4);
    assert_eq!(
        picker.selected(),
        3,
        "a typo and a backspace changed what Enter takes",
    );

    // On the screen, which is the only place the operator can check it.
    let (mut screen, recorder) = support::screen_of(80, 24, 6);
    screen
        .draw(|frame| picker.render(frame, frame.area(), &DARK))
        .expect("frame");
    let viewport = screen.viewport_text();
    let marked: Vec<&str> = viewport
        .lines()
        .filter(|line| line.starts_with(DARK.glyphs.marker))
        .collect();
    assert_eq!(marked.len(), 1, "exactly one row is marked: {viewport:?}");
    assert!(
        marked[0].contains("Any OpenAI-compatible endpoint"),
        "the marker is not on the row the picker was opened with: {marked:?}",
    );
    assert!(recorder.contains("Any OpenAI-compatible endpoint"));

    assert_eq!(picker.key(key(KeyCode::Enter)), Outcome::Chosen(3));
}

#[test]
fn f9_the_opening_row_comes_back_when_the_query_widens_again() {
    // The narrowing case, which is the one an operator actually hits: the query
    // still admits rows, just not the one the picker opened on. While it is hidden
    // the marker is on the best match — Enter takes what is under it, and nothing
    // else would be honest — and the opening row is what a backspace restores.
    let mut picker = providers().selecting(3);
    // `h` admits `Anthropic` alone; no other provider label carries one.
    picker.key(key(KeyCode::Char('h')));
    assert_eq!(picker.matching(), 1);
    assert_eq!(picker.selected(), 1, "Enter takes the row under the marker");

    picker.key(key(KeyCode::Backspace));
    assert_eq!(
        picker.selected(),
        3,
        "the row the picker was opened on was not restored",
    );

    // A deliberate move replaces it. What is remembered is the row the operator
    // last put the marker on, which after an arrow is no longer the opening one.
    picker.key(key(KeyCode::Up));
    picker.key(key(KeyCode::Char('h')));
    picker.key(key(KeyCode::Backspace));
    assert_eq!(
        picker.selected(),
        2,
        "the arrows are what say which row to keep",
    );
}

#[test]
fn f8_no_row_under_the_marker_is_a_state_the_picker_can_state() {
    // `selected()` answers 0 when nothing matches, and 0 is a real row. The
    // wizard's theme step indexed `THEMES` with it after every keystroke, so a
    // letter no theme name carries previewed — and then wrote — `dark`. The fix is
    // an answer that can say "nothing", not a caller-side guard: `matching() == 0`
    // at each of the nine call sites is nine chances to forget.
    let mut picker = providers();
    assert_eq!(picker.selection(), Some(0));

    picker.key(key(KeyCode::Char('z')));
    assert_eq!(picker.matching(), 0);
    assert_eq!(
        picker.selection(),
        None,
        "row 0 is not under the marker; nothing is",
    );
    // And an empty picker, which has never had a row under its marker either.
    assert_eq!(Picker::new("Nothing here", vec![]).selection(), None);
}

#[test]
fn f8_rows_can_be_replaced_without_losing_what_was_typed() {
    // The wizard's model step opens on the provider's default while the catalogue
    // request is in flight, so the rows arrive after the picker is on the screen.
    // Replacing the whole picker discarded the query with it.
    let mut picker = Picker::new("Which model?", vec![Row::new("anthropic/claude-sonnet-4")]);
    for character in "gemini".chars() {
        picker.key(key(KeyCode::Char(character)));
    }
    assert_eq!(picker.matching(), 0, "the placeholder does not match it");

    picker.set_rows(
        vec![
            Row::new("anthropic/claude-sonnet-4"),
            Row::new("openai/gpt-5"),
            Row::new("google/gemini-3-pro"),
        ],
        0,
    );
    assert_eq!(
        picker.query(),
        "gemini",
        "the query did not survive the rows",
    );
    assert_eq!(picker.matching(), 1);
    assert_eq!(
        picker.key(key(KeyCode::Enter)),
        Outcome::Chosen(2),
        "the row the query left visible is not the row Enter took",
    );
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

/// F7: a paste does not leak past an open picker.
///
/// The driver used to take a paste above the check that hands a picker the
/// keyboard, so pasting with `/model` open inserted the text into the composer
/// sitting *behind* the overlay — typed by nobody, seen by nobody, and sent
/// with the next prompt. A picker's own documentation says it owns the keyboard
/// while it is up, and this is what makes that true of a paste as well as a
/// key.
///
/// This is the assertion the criterion's sabotage kills: fix the mid-turn arm,
/// leave the picker's, and only this one goes red.
#[test]
fn f7_a_paste_does_not_leak_past_an_open_picker() {
    let mut app = App::new(DARK, "opus-5");

    assert!(
        app.paste("from the clipboard", true) == io_cli::app::Pasted::Refused,
        "a picker owns the keyboard, and a paste is the keyboard",
    );
    assert!(
        app.composer.is_empty(),
        "the paste landed in the composer behind the overlay: {:?}",
        app.composer.text(),
    );
}

// ---------------------------------------------------------------------------
// O11 — Tab accepts, Shift+Tab steps back, in every picker in the product
// ---------------------------------------------------------------------------

/// **O11 — `Tab` takes the row under the marker.**
///
/// It applies to every list in the product rather than to the palette alone,
/// because the product ships one `Picker`. Until 0.32.0 it fell into the
/// catch-all arm and did nothing at all — a key that looks like it should work and
/// silently does not is worse than one that is not bound.
///
/// **The sabotage pass is why this test exists.** `Tab` was bound, documented in
/// the shipped key table and in the guide, and asserted nowhere: removing the arm
/// failed no test in the suite.
#[test]
fn o11_tab_takes_the_row_under_the_marker() {
    let mut picker = Picker::new(
        "Which command?",
        vec![Row::new("first"), Row::new("second"), Row::new("third")],
    );
    assert_eq!(picker.key(key(KeyCode::Down)), Outcome::Idle);
    assert_eq!(
        picker.key(key(KeyCode::Tab)),
        Outcome::Chosen(1),
        "Tab is the completion key an operator arrives already expecting, and it \
         must take the marked row exactly as Enter does",
    );
}

/// **O11 — `Shift+Tab` steps the marker back, in both spellings a terminal
/// sends.**
///
/// A terminal may send `BackTab`, or `Tab` with the shift modifier set.
/// `crate::keys` normalises the first into the second for a *binding*, but a
/// picker reads raw key events and never goes through that table — so both arrive
/// here as themselves, and matching only one is a key that works on some
/// terminals and not others.
#[test]
fn o11_shift_tab_steps_the_marker_back_in_both_spellings() {
    for back in [
        KeyEvent::new(KeyCode::BackTab, KeyModifiers::NONE),
        KeyEvent::new(KeyCode::Tab, KeyModifiers::SHIFT),
    ] {
        let mut picker = Picker::new(
            "Which command?",
            vec![Row::new("first"), Row::new("second"), Row::new("third")],
        );
        picker.key(key(KeyCode::Down));
        picker.key(key(KeyCode::Down));
        assert_eq!(picker.selected(), 2, "the marker moved down twice");

        assert_eq!(
            picker.key(back),
            Outcome::Idle,
            "stepping back chooses nothing"
        );
        assert_eq!(
            picker.selected(),
            1,
            "{back:?} did not step the marker back",
        );
    }
}

/// **O11 — `Tab` on a heading declines, exactly as `Enter` does.**
///
/// A heading cannot be reached by any path that moves the marker, but `Tab` is now
/// a second key that would turn a mistake there into the wrong action, so it
/// declines rather than trusting that.
#[test]
fn o11_tab_declines_on_a_heading_like_enter_does() {
    let mut picker = Picker::new(
        "Which command?",
        vec![Row::heading("a group"), Row::new("first")],
    );
    // The marker steps off the heading when the picker opens, so this asserts the
    // pair agree rather than manufacturing an unreachable state.
    assert_eq!(picker.key(key(KeyCode::Tab)), Outcome::Chosen(1));
}

// ---------------------------------------------------------------------------
// T04 — a marked set the spacebar toggles, and an unfold whose height is
// measured per row
// ---------------------------------------------------------------------------

fn space() -> KeyEvent {
    key(KeyCode::Char(' '))
}

/// **T04 — the spacebar marks and unmarks, and the screen says which rows.**
///
/// The whole of the plural mechanic in one place: a toggle, a set read back in
/// the caller's own numbering, and a box the operator can see it in. A mark that
/// is only in the struct is a selection nobody can check before they press Enter.
#[test]
fn t04_the_spacebar_marks_and_unmarks_on_a_plural_picker() {
    let mut picker = providers().accepting_several();
    assert!(
        picker.marked().is_empty(),
        "a picker opens with nothing marked",
    );
    // With nothing marked, the plural answer is the singular one: the row under
    // the marker, and not an empty list.
    assert_eq!(picker.chosen(), vec![0]);

    assert_eq!(
        picker.key(space()),
        Outcome::Idle,
        "marking chooses nothing"
    );
    assert_eq!(
        picker.marked(),
        vec![0],
        "the spacebar did not mark the row"
    );
    assert_eq!(
        picker.query(),
        "",
        "the space was typed into the query instead of marking",
    );

    // And unmarks: the same key, the same row.
    picker.key(space());
    assert!(
        picker.marked().is_empty(),
        "the second press did not unmark it: {:?}",
        picker.marked(),
    );

    // Two rows, reported in the caller's row order rather than in the order they
    // were pressed — row 2 is marked before row 1 here on purpose.
    picker.key(key(KeyCode::Down));
    picker.key(key(KeyCode::Down));
    picker.key(space());
    picker.key(key(KeyCode::Up));
    picker.key(space());
    assert_eq!(picker.marked(), vec![1, 2]);
    assert_eq!(
        picker.chosen(),
        vec![1, 2],
        "with rows marked, the plural answer is the marks and not the marker",
    );
    // Enter still answers with one index, in the spelling every existing caller
    // reads. The plural answer is a second question, asked of the same picker.
    assert_eq!(picker.key(key(KeyCode::Enter)), Outcome::Chosen(1));

    // On the screen, which is the only place the operator can check it.
    let (mut screen, recorder) = support::screen_of(80, 24, 8);
    screen
        .draw(|frame| picker.render(frame, frame.area(), &DARK))
        .expect("frame");
    let viewport = screen.viewport_text();
    let checked: Vec<&str> = viewport
        .lines()
        .filter(|line| line.contains("[x]"))
        .collect();
    assert_eq!(
        checked.len(),
        2,
        "two rows are marked and the screen shows a different number: {viewport:?}",
    );
    // `OpenAI` is also a substring of `Any OpenAI-compatible endpoint`, so the
    // second half says which of the two this is: a `contains` an unrelated row
    // satisfies is the vacuity this product keeps finding in its own gates.
    assert!(
        checked[0].contains("Anthropic"),
        "the first ticked row is not the first marked one: {checked:?}",
    );
    assert!(
        checked[1].contains("OpenAI") && !checked[1].contains("compatible"),
        "the tick is on `Any OpenAI-compatible endpoint` rather than on `OpenAI`: \
         {checked:?}",
    );
    let empty: Vec<&str> = viewport
        .lines()
        .filter(|line| line.contains("[ ]"))
        .collect();
    assert_eq!(
        empty.len(),
        2,
        "every markable row carries a box, ticked or not: {viewport:?}",
    );
    assert!(
        recorder.contains("[x]"),
        "the box never reached the terminal"
    );
}

/// **T04 — a single-answer picker is untouched: the spacebar is a query
/// character, as it has always been.**
///
/// The constraint the whole opt-in exists for. Nine call sites never mark
/// anything, and none of them can be edited to cope — so a space on any of them
/// has to do exactly what it did before this release: narrow the list.
#[test]
fn t04_the_spacebar_is_still_a_query_character_on_an_ordinary_picker() {
    let mut picker = providers();
    assert_eq!(picker.key(space()), Outcome::Idle);
    assert_eq!(
        picker.query(),
        " ",
        "the space was swallowed by a marked set no ordinary picker has",
    );
    assert!(
        picker.marked().is_empty(),
        "an ordinary picker marked a row: {:?}",
        picker.marked(),
    );
    // A space is a real filter character on these labels, which is why it cannot
    // be spent: `Any OpenAI-compatible endpoint` has one and `OpenRouter` does
    // not.
    assert_eq!(
        picker.matching(),
        1,
        "the space did not filter, so it was not treated as a query character",
    );
    assert_eq!(
        picker.rows()[picker.selected()].label,
        "Any OpenAI-compatible endpoint"
    );

    // And the box is not drawn on it either: the column belongs to the plural
    // picker alone.
    let mut picker = providers();
    let (mut screen, _recorder) = support::screen_of(80, 24, 8);
    screen
        .draw(|frame| picker.render(frame, frame.area(), &DARK))
        .expect("frame");
    assert!(
        !screen.viewport_text().contains("[ ]"),
        "a single-answer picker grew a checkbox column: {:?}",
        screen.viewport_text(),
    );
}

/// **T04 — a mark made in a filtered list is the caller's own row index.**
///
/// The expensive defect, one state along. Three call sites index a slice raw with
/// what this widget hands back; a plural answer carrying filtered positions
/// panics at those and acts on the wrong rows at the others — once per marked
/// row, silently.
#[test]
fn t04_a_mark_is_an_index_into_the_callers_rows_and_not_the_filtered_view() {
    let mut picker = providers().accepting_several();
    // `h` admits `Anthropic` alone — row 1 of the caller's list, and position 0
    // of the filtered one, which is exactly what a filtered index would report.
    picker.key(key(KeyCode::Char('h')));
    assert_eq!(picker.matching(), 1);

    picker.key(space());
    assert_eq!(
        picker.marked(),
        vec![1],
        "the marked row is a position in the filtered view, not a row",
    );
    assert_eq!(picker.rows()[picker.marked()[0]].label, "Anthropic");
}

/// **T04 — a marked row the query hides is still marked.**
///
/// The release's recorded open question, answered: marks are held against
/// unfiltered rows and survive the query. The alternative makes the filter
/// destructive — an operator marking five rows out of four hundred narrows the
/// list to find each one, and every narrowing would throw away the marks made
/// under the last.
#[test]
fn t04_a_marked_row_survives_the_query_that_hides_it() {
    let mut picker = providers().accepting_several();
    picker.key(key(KeyCode::Down));
    picker.key(space());
    picker.key(key(KeyCode::Down));
    picker.key(key(KeyCode::Down));
    picker.key(space());
    assert_eq!(picker.marked(), vec![1, 3]);

    // `h` admits `Anthropic` alone, so row 3 is not on the screen at all.
    picker.key(key(KeyCode::Char('h')));
    assert_eq!(picker.matching(), 1);
    assert_eq!(
        picker.marked(),
        vec![1, 3],
        "a keystroke that hid a marked row un-marked it",
    );

    // And a query that admits nothing at all, which is the state that has no
    // match set to read a mark out of.
    picker.key(key(KeyCode::Char('z')));
    assert_eq!(picker.matching(), 0);
    assert_eq!(
        picker.marked(),
        vec![1, 3],
        "an empty result forgot the marks"
    );

    // Widening brings the rows back, still marked, still drawn as marked.
    picker.key(key(KeyCode::Backspace));
    picker.key(key(KeyCode::Backspace));
    assert_eq!(picker.query(), "");
    assert_eq!(picker.matching(), 4);
    assert_eq!(picker.marked(), vec![1, 3]);

    let (mut screen, _recorder) = support::screen_of(80, 24, 8);
    screen
        .draw(|frame| picker.render(frame, frame.area(), &DARK))
        .expect("frame");
    let viewport = screen.viewport_text();
    let checked: Vec<&str> = viewport
        .lines()
        .filter(|line| line.contains("[x]"))
        .collect();
    assert_eq!(checked.len(), 2, "{viewport:?}");
    assert!(
        checked[1].contains("Any OpenAI-compatible endpoint"),
        "the row that was hidden came back unticked: {checked:?}",
    );
}

/// **T04 — a heading cannot be marked, for the reason it cannot be chosen.**
///
/// No path that *moves* the marker leaves it on a heading, so this puts it there
/// through [`Picker::focus`], which can — the same belt-and-braces `Enter` and
/// `Tab` keep, reached by the one door that actually opens.
#[test]
fn t04_a_heading_cannot_be_marked() {
    let mut picker = Picker::new(
        "Which command?",
        vec![
            Row::heading("a group"),
            Row::new("first"),
            Row::new("second"),
        ],
    )
    .accepting_several();

    assert!(
        picker.focus(0),
        "the heading is a row the marker can be put on"
    );
    assert_eq!(picker.selection(), Some(0), "the marker is on the heading");
    picker.key(space());
    assert!(
        picker.marked().is_empty(),
        "a heading was marked: {:?}",
        picker.marked(),
    );
    // The control: the same key on the row below it does mark, so the assertion
    // above is about headings rather than about a spacebar that never works.
    assert!(picker.focus(1));
    picker.key(space());
    assert_eq!(picker.marked(), vec![1]);
}

/// **T04 — the row demand does not move as the marker moves or the query
/// filters.**
///
/// The driver re-places the viewport whenever the demand changes, and a
/// re-placement is a terminal tear-down and a cursor query — on a surface that is
/// open while a turn is in flight. So a demand that followed the *focused* row's
/// unfold height would tear the terminal down on every arrow key between a short
/// preview and a tall one. `rows_wanted` reserves the largest configured unfold
/// instead, which is a function of the configuration and of nothing the operator
/// is doing.
///
/// **Asserted by comparing demands, never by a clock.** N1 forbids a sleeping or
/// clock-reading test anywhere under `tests/` — and it sweeps for the spellings
/// as plain strings, in comments too, so they are not written out here either. A
/// timing would be measuring the wrong thing regardless: the property is that a
/// number does not change, and a number is what is compared.
///
/// The list's own length is subtracted, because that half of the demand is
/// *supposed* to follow the query — a filter that admits two rows asks for two
/// rows. What must be constant is everything else: the head, and the reservation.
#[test]
fn t04_the_demand_does_not_move_with_the_marker_or_the_query() {
    let mut picker = Picker::new(
        "Which one?",
        vec![
            Row::new("alpha"),
            Row::new("beta"),
            Row::new("gamma"),
            Row::new("delta"),
            Row::new("epsilon"),
        ],
    );
    // Two unfolds of very different heights, which is the whole fixture: with one
    // height per picker there was nothing here to oscillate.
    picker.set_unfold(0, 1);
    picker.set_unfold(3, 7);

    // The demand minus the rows the query admits: the head plus the reservation.
    let overhead = |picker: &Picker| {
        picker
            .rows_wanted()
            .saturating_sub(u16::try_from(picker.matching()).expect("a small list"))
    };

    let opening = overhead(&picker);
    assert_eq!(
        opening, 8,
        "one row for the head and seven for the tallest unfold, whatever is focused",
    );

    let mut seen = vec![opening];
    let mut markers = vec![picker.selection()];
    let mut lists = vec![picker.matching()];
    // Down onto `delta`, which is the seven-row unfold, and past it; then a query
    // that hides *both* unfolding rows; then back out again.
    for stroke in [
        key(KeyCode::Down),
        key(KeyCode::Down),
        key(KeyCode::Down),
        key(KeyCode::Down),
        key(KeyCode::Up),
        key(KeyCode::Char('m')),
        key(KeyCode::Char('a')),
        key(KeyCode::Backspace),
        key(KeyCode::Backspace),
        key(KeyCode::Home),
    ] {
        picker.key(stroke);
        seen.push(overhead(&picker));
        markers.push(picker.selection());
        lists.push(picker.matching());
    }

    // The fixture has to actually exercise both halves, or the constant below is
    // a constant about nothing.
    assert!(
        markers.contains(&Some(0)) && markers.contains(&Some(3)),
        "the marker never visited both unfolding rows: {markers:?}",
    );
    assert!(
        lists.iter().any(|count| *count != 5),
        "the query never filtered anything: {lists:?}",
    );
    assert!(
        markers.contains(&Some(2)),
        "the marker never left the unfolding rows: {markers:?}",
    );

    assert!(
        seen.iter().all(|demand| *demand == opening),
        "the reservation moved as the operator did, which re-places the viewport \
         on a keystroke: {seen:?}",
    );

    // And what is *drawn* does follow the marker, which is the other half of the
    // pair: the reservation is the largest, the open block is the focused row's.
    assert!(picker.focus(0));
    assert!(picker.unfolded_now(), "row 0 opens a block of its own");
    assert!(picker.focus(2));
    assert!(
        !picker.unfolded_now(),
        "a row with no unfold configured opened one",
    );
}

/// **T04 — an unfold reserves the rows ratatui will paint, not the lines the
/// caller counted.**
///
/// `crate::rows::wrapped` is the one measurement in this crate, and this is the
/// fixture that tells it apart from the `lines.len()` it replaced: two logical
/// lines that wrap to more than two rows at the width they are drawn at. A gate
/// written against the line count fails here, which is the point — that count has
/// already cost this product two defects, both of them content painted over rows
/// something else had been promised.
#[test]
fn t04_a_wrapped_unfold_reserves_the_measured_height() {
    const WIDTH: u16 = 32;
    let preview = vec![
        Line::from("the first line of this preview is long enough to wrap several times over"),
        Line::from("and the second one wraps as well"),
    ];
    let height = io_cli::rows::wrapped(&preview, WIDTH);
    assert!(
        height > u16::try_from(preview.len()).expect("two lines"),
        "the fixture does not wrap at {WIDTH} columns, so it cannot tell a \
         measured height from a counted one: {height} rows for {} lines",
        preview.len(),
    );

    let mut picker = Picker::new("Which one?", vec![Row::new("aaa"), Row::new("bbb")]);
    picker.set_unfold(0, height);
    assert_eq!(
        picker.rows_wanted(),
        2 + height + 1,
        "the demand does not carry the measured height",
    );

    let (mut screen, _recorder) = support::screen_of(WIDTH, 24, 16);
    // The area's own origin, because an inline viewport does not start at row
    // zero — the rectangle `opened` reports is absolute and the viewport text is
    // indexed from the top of the area.
    let top = std::cell::Cell::new(u16::MAX);
    screen
        .draw(|frame| {
            top.set(frame.area().y);
            picker.render(frame, frame.area(), &DARK);
        })
        .expect("frame");

    let opened = picker.opened().expect("the block was reserved");
    assert_eq!(
        opened.height, height,
        "the reserved block is not the height the text was measured at",
    );
    assert_eq!(opened.width, WIDTH);

    // The rows are really reserved, not merely promised in a rectangle: the row
    // below the unfolding one is drawn `height` rows further down than it would
    // otherwise be.
    let viewport = screen.viewport_text();
    let lines: Vec<&str> = viewport.lines().collect();
    let first = lines
        .iter()
        .position(|line| line.contains("aaa"))
        .expect("the unfolding row");
    let second = lines
        .iter()
        .position(|line| line.contains("bbb"))
        .expect("the row below it");
    assert_eq!(
        second - first,
        usize::from(height) + 1,
        "the block took {} rows and the text needs {height}: {viewport:?}",
        second - first - 1,
    );
    assert_eq!(
        opened.y,
        top.get() + u16::try_from(first).expect("a small viewport") + 1,
        "the block does not open directly beneath the row that owns it",
    );
    for row in &lines[first + 1..second] {
        assert!(
            row.is_empty(),
            "the reserved rows are the caller's to draw into and must be left \
             blank: {row:?}",
        );
    }

    // A second row with a height of its own, which is what one height per picker
    // could not express: each keeps its own, and the taller is what is reserved.
    picker.set_unfold(1, 2);
    assert_eq!(
        picker.rows_wanted(),
        2 + height + 1,
        "the shorter unfold replaced the taller one instead of joining it",
    );
    picker.key(key(KeyCode::Down));
    screen
        .draw(|frame| picker.render(frame, frame.area(), &DARK))
        .expect("frame");
    assert_eq!(
        picker.opened().expect("the second block").height,
        2,
        "the second row opened the first row's height",
    );
}
