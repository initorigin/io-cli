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
    // four rows fixed at attach, so a query line above the title would leave
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
