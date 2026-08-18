//! F1 — the slash palette.
//!
//! `/` at an empty prompt opens a [`Picker`] over the command inventory, and it
//! narrows as it is typed. There is nothing new underneath it: the rows are
//! [`io_cli::commands::COMMANDS`], the filtering is the picker's own and the
//! ranking is [`io_cli::fuzzy`]'s, both asserted in their own files. What is
//! asserted here is the part that is the palette's: which keystroke opens it,
//! what its rows are, that the ordering the contract promises survives being
//! applied to *these* labels, and that `Esc` and `Enter` leave the composer in
//! the two states the contract names.
//!
//! The driver in `src/main.rs` has no test that can reach it, so every decision
//! it makes about the palette is a library function called here:
//! [`commands::opens_palette`] is the condition it branches on and
//! [`commands::palette_command`] is what it puts in the composer.

mod support;

use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use io_cli::app::{App, Command};
use io_cli::commands::{self, COMMANDS};
use io_cli::picker::{Outcome, Picker};
use io_cli::theme::DARK;

fn key(code: KeyCode) -> KeyEvent {
    KeyEvent::new(code, KeyModifiers::NONE)
}

fn slash() -> KeyEvent {
    key(KeyCode::Char('/'))
}

/// The palette as the driver opens it.
fn palette() -> Picker {
    Picker::new("Which command?", commands::palette())
}

/// Type `text` at the picker, one character at a time, exactly as an operator
/// would.
fn type_at(picker: &mut Picker, text: &str) {
    for character in text.chars() {
        picker.key(key(KeyCode::Char(character)));
    }
}

/// Type `text` at the session.
fn type_at_app(app: &mut App, text: &str) {
    for character in text.chars() {
        app.key(key(KeyCode::Char(character)));
    }
}

/// The label under the marker.
fn marked(picker: &Picker) -> &str {
    &picker.rows()[picker.selected()].label
}

#[test]
fn f1_a_slash_at_an_empty_prompt_opens_the_palette_and_nowhere_else() {
    let mut app = App::new(DARK, "m");
    assert!(
        commands::opens_palette(slash(), app.composer.is_empty(), app.armed()),
        "the empty prompt is the one place the palette opens from",
    );

    // A `/` inside a line is a path separator or a fraction, and it has to stay
    // one. The palette taking the keyboard away in the middle of a sentence is
    // the defect this half of the condition exists for.
    type_at_app(&mut app, "docs/");
    assert_eq!(app.composer.text(), "docs/");
    assert!(
        !commands::opens_palette(slash(), app.composer.is_empty(), app.armed()),
        "a slash mid-line is an ordinary character",
    );

    // A chord is a command somebody meant, not a letter they typed — the same
    // rule the picker's own filter follows.
    let app = App::new(DARK, "m");
    for modifier in [KeyModifiers::CONTROL, KeyModifiers::ALT] {
        assert!(!commands::opens_palette(
            KeyEvent::new(KeyCode::Char('/'), modifier),
            app.composer.is_empty(),
            app.armed(),
        ));
    }
    assert!(!commands::opens_palette(
        key(KeyCode::Char('f')),
        app.composer.is_empty(),
        app.armed(),
    ));

    // A half-pressed rewind is the one sequence in this product whose second
    // press changes the operator's files. The driver opens the palette *in front
    // of* `App::key`, which is what disarms — so the palette declines while
    // something is armed, the `/` falls through to the session, and the arming is
    // cleared by the keystroke exactly as every other key clears it. Without this
    // the arming would survive the palette and a later `Esc` would fire a rewind
    // nobody was still expecting.
    let mut app = App::new(DARK, "m");
    assert_eq!(app.key(key(KeyCode::Esc)), Command::ArmRewind);
    assert!(app.armed());
    assert!(
        !commands::opens_palette(slash(), app.composer.is_empty(), app.armed()),
        "a half-pressed destructive sequence must be disarmed, not stepped over",
    );
    assert_eq!(app.key(slash()), Command::None);
    assert!(!app.armed(), "the slash disarmed it, as any other key does");
    assert_eq!(app.composer.text(), "/");
}

#[test]
fn f1_the_palette_opens_showing_every_command() {
    let rows = commands::palette();
    assert_eq!(rows.len(), COMMANDS.len());
    for (row, (name, what)) in rows.iter().zip(COMMANDS) {
        // The label is the command with its leading `/` removed, and that is a
        // matching decision rather than a cosmetic one: with the slash in place
        // every haystack begins with the same character, so no query could ever
        // be a prefix of a row and the whole exact-then-prefix half of the
        // ranking would be unreachable in the one surface built on it.
        assert_eq!(row.label, name.strip_prefix('/').expect("a command"));
        assert_eq!(row.detail.as_deref(), Some(*what));
    }

    // What the operator can see, through the terminal the product actually
    // writes to. Twelve rows so the ten commands and the title all fit.
    let mut picker = palette();
    let (mut screen, recorder) = support::screen_of(80, 24, 12);
    screen
        .draw(|frame| picker.render(frame, frame.area(), &DARK))
        .expect("frame");
    assert!(recorder.contains("Which command?"));
    let viewport = screen.viewport_text();
    for (name, _) in COMMANDS {
        let label = name.strip_prefix('/').expect("a command");
        assert!(
            viewport.contains(label),
            "the palette opens on every command, and {label} is missing: {viewport:?}",
        );
    }
}

#[test]
fn f1_each_further_character_narrows_the_list() {
    let mut picker = palette();
    assert_eq!(picker.matching(), COMMANDS.len());

    type_at(&mut picker, "o");
    let after_o = picker.matching();
    assert!(
        after_o < COMMANDS.len(),
        "the first character narrowed nothing"
    );

    type_at(&mut picker, "p");
    let after_p = picker.matching();
    assert!(after_p < after_o);

    type_at(&mut picker, "d");
    assert_eq!(picker.query(), "opd");
    assert_eq!(picker.matching(), 1);
    assert_eq!(
        marked(&picker),
        "copy diff",
        "`opd` is spread across `copy diff` and across nothing else",
    );
}

/// **The sabotage test.** Swap [`io_cli::fuzzy::score`] for a `contains` check
/// and this is the one that goes red: `fk` appears in no command as a substring,
/// so a substring matcher narrows the list to nothing and the palette says so
/// instead of offering `/fork`.
#[test]
fn f1_fk_selects_fork_though_no_command_begins_with_it() {
    // The premise first, so a later release that renames a command into `fk…`
    // fails here rather than passing the assertion below for the wrong reason.
    for (name, _) in COMMANDS {
        assert!(
            !name.strip_prefix('/').expect("a command").starts_with("fk"),
            "{name} makes `fk` a prefix match, and this test then proves nothing",
        );
    }

    let mut picker = palette();
    type_at(&mut picker, "fk");
    assert_eq!(
        picker.matching(),
        1,
        "`fk` is a subsequence of `fork` and of nothing else",
    );
    let Outcome::Chosen(index) = picker.key(key(KeyCode::Enter)) else {
        panic!("Enter on the one matched row must choose it");
    };
    assert_eq!(commands::palette_command(index), Some("/fork"));
}

#[test]
fn f1_an_exact_name_outranks_a_prefix_which_outranks_a_scattered_match() {
    // The ordering the contract names, asserted against these rows rather than
    // taken on trust from `tests/fuzzy.rs`: what is at stake here is that the
    // labels the palette is built from can express all three tiers at all.
    let mut picker = palette();
    type_at(&mut picker, "copy");
    assert_eq!(picker.matching(), 2);
    assert_eq!(marked(&picker), "copy", "the exact name is not first");
    picker.key(key(KeyCode::Down));
    assert_eq!(marked(&picker), "copy diff", "the prefix is not second");

    let mut picker = palette();
    type_at(&mut picker, "se");
    assert_eq!(
        marked(&picker),
        "setup",
        "`setup` begins with `se`; `resume` merely contains the two letters in order",
    );
}

#[test]
fn f1_equal_scores_keep_the_first_row_still_between_keystrokes() {
    // Both rows begin with `c`, so both score the same and the tie-break is the
    // order they were handed in. The defect: an unstable sort swaps them on a
    // keystroke that did not change the result, and `Enter` takes a row nobody
    // chose.
    let mut picker = palette();
    type_at(&mut picker, "c");
    assert_eq!(picker.matching(), 2);
    assert_eq!(marked(&picker), "copy");
    type_at(&mut picker, "o");
    assert_eq!(picker.matching(), 2);
    assert_eq!(
        marked(&picker),
        "copy",
        "the first row moved under the marker"
    );
}

#[test]
fn f1_escape_leaves_the_composer_exactly_as_it_was() {
    // The `/` opened the palette rather than being inserted, so "intact" means
    // untouched — including the case the operator reaches by typing something and
    // then clearing it, which is an empty prompt that has been used.
    let mut app = App::new(DARK, "m");
    type_at_app(&mut app, "note to self");
    app.composer.clear();
    assert!(commands::opens_palette(
        slash(),
        app.composer.is_empty(),
        app.armed()
    ));

    let mut picker = palette();
    type_at(&mut picker, "fo");
    assert_eq!(picker.key(key(KeyCode::Esc)), Outcome::Cancelled);
    assert_eq!(
        app.composer.text(),
        "",
        "the slash never reached the composer, so backing out leaves nothing behind",
    );

    // And from a prompt that was empty all along.
    let app = App::new(DARK, "m");
    let mut picker = palette();
    assert_eq!(picker.key(key(KeyCode::Esc)), Outcome::Cancelled);
    assert_eq!(app.composer.text(), "");
    assert!(app.composer.is_empty());
}

#[test]
fn f1_enter_puts_the_chosen_command_in_the_composer_rather_than_running_it() {
    let mut app = App::new(DARK, "m");
    let mut picker = palette();
    type_at(&mut picker, "fk");
    let Outcome::Chosen(index) = picker.key(key(KeyCode::Enter)) else {
        panic!("Enter must choose");
    };
    let command = commands::palette_command(index).expect("a chosen row is a command");
    app.composer.set(command);

    // Typed, not run. What is in the prompt is what the operator would have typed
    // by hand, and pressing `Enter` on it goes down the submit path the palette
    // did not touch — which is the whole reason the palette is not a second
    // dispatcher.
    assert_eq!(app.composer.text(), "/fork");
    assert_eq!(app.key(key(KeyCode::Enter)), Command::Slash("fork".into()));
}

#[test]
fn f1_a_palette_row_addresses_the_command_it_was_built_from() {
    // The picker hands back an index into the rows it was given, and those rows
    // are `COMMANDS` in order — so the index reads straight back against the
    // inventory. A palette that renumbered would put a different command in the
    // prompt from the one under the marker, and nothing on screen would say so.
    for (index, (name, _)) in COMMANDS.iter().enumerate() {
        assert_eq!(commands::palette_command(index), Some(*name));
        assert_eq!(
            commands::palette()[index].label,
            name.strip_prefix('/').expect("a command"),
        );
    }
    assert_eq!(commands::palette_command(COMMANDS.len()), None);
}
