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
//! F2 — the prompt templates, in that same palette.
//!
//! `[run] templates` is a directory io-harness walks, and what this asserts is
//! the seam between that walk and the row list: the three states
//! [`commands::templates`] keeps apart, that a template's row carries its name
//! and its description, and that choosing one puts [`io_harness::Templates::render`]'s
//! output in the composer to be edited rather than on the wire.
//!
//! The driver in `src/main.rs` has no test that can reach it, so every decision
//! it makes about the palette is a library function called here:
//! [`commands::opens_palette`] is the condition it branches on,
//! [`commands::palette_pick`] is what a chosen row stands for, and
//! [`commands::expand`] is what a chosen template puts in the composer.

mod support;

use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use io_cli::app::{App, Command};
use io_cli::commands::{self, Chosen, COMMANDS, SKILL, TEMPLATE};
use io_cli::picker::{Outcome, Picker};
use io_cli::theme::DARK;
use io_harness::{Config, Templates};

fn key(code: KeyCode) -> KeyEvent {
    KeyEvent::new(code, KeyModifiers::NONE)
}

fn slash() -> KeyEvent {
    key(KeyCode::Char('/'))
}

/// The palette as the driver opens it, with nothing configured to extend it.
fn palette() -> Picker {
    palette_over(&Templates::none())
}

/// The palette as the driver opens it over a set of templates.
fn palette_over(templates: &Templates) -> Picker {
    Picker::new(
        "Which command?",
        commands::palette(templates, &io_harness::Skills::none()),
    )
}

/// A templates directory with two templates in it: one that renders as it
/// stands, and one that cannot render without an argument.
fn written() -> tempfile::TempDir {
    let dir = tempfile::tempdir().expect("a temporary directory");
    std::fs::write(
        dir.path().join("review.md"),
        "Read every changed line and say what is wrong with it.\n",
    )
    .expect("a template is written");
    std::fs::write(
        dir.path().join("bugfix.md"),
        "Fix the bug described in {{file}} and add a test for it.\n",
    )
    .expect("a template is written");
    dir
}

/// What `[run] templates = <dir>` parses to. `from_toml` rather than `discover`,
/// so nothing on this machine's disk outside the temporary directory is read.
fn configured(dir: &std::path::Path) -> Config {
    Config::from_toml(&format!(
        "[run]\ntemplates = {:?}\n",
        dir.display().to_string()
    ))
    .expect("io-harness parses its own file")
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
fn f1_the_palette_opens_on_the_rows_the_viewport_has() {
    let rows = commands::palette(&Templates::none(), &io_harness::Skills::none());
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
    // writes to, **in the viewport the session already has** — 0.13.0 stopped
    // growing it, so the height here is the session's own and the rows past it
    // are `f7_every_row_below_the_fold_is_still_reachable`'s business.
    let mut picker = palette();
    let height = io_cli::term::VIEWPORT_HEIGHT;
    let (mut screen, recorder) = support::screen_of(80, height + 4, height);
    screen
        .draw(|frame| picker.render(frame, frame.area(), &DARK))
        .expect("frame");
    assert!(recorder.contains("Which command?"));
    let viewport = screen.viewport_text();
    for (name, _) in COMMANDS.iter().take(usize::from(height) - 1) {
        let label = name.strip_prefix('/').expect("a command");
        assert!(
            viewport.contains(label),
            "the palette opens on the rows the viewport has, and {label} is missing: {viewport:?}",
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
    assert_eq!(
        commands::palette_pick(&Templates::none(), &io_harness::Skills::none(), index),
        Some(Chosen::Command("/fork")),
    );
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
    // `c` matches FIVE rows since 0.16.0 added `/config`: `copy`, `copy diff`,
    // `contain`, `clear` and `config` all begin with it, and `attach`, which
    // merely held one, stopped being a command in 0.13.1 — a picture is attached
    // by dropping it on the prompt.
    // `co` narrows to the four that begin with `co`, because `clear` has no `o`
    // after its `c`.
    // The three score the same and the tie-break is the order they were handed
    // in. The defect: an unstable sort swaps them on a keystroke that did not
    // change the result, and `Enter` takes a row nobody chose.
    //
    // The counts moved with the command table and the property did not. They are
    // stated rather than derived from `COMMANDS.len()` on purpose: a count that
    // recomputed itself would keep passing while the narrowing it describes
    // quietly stopped happening.
    let mut picker = palette();
    type_at(&mut picker, "c");
    assert_eq!(picker.matching(), 5);
    assert_eq!(marked(&picker), "copy");
    type_at(&mut picker, "o");
    assert_eq!(picker.matching(), 4);
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
    let Some(Chosen::Command(command)) =
        commands::palette_pick(&Templates::none(), &io_harness::Skills::none(), index)
    else {
        panic!("a chosen row in the command half is a command");
    };
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
    let none = Templates::none();
    for (index, (name, _)) in COMMANDS.iter().enumerate() {
        assert_eq!(
            commands::palette_pick(&none, &io_harness::Skills::none(), index),
            Some(Chosen::Command(name))
        );
        assert_eq!(
            commands::palette(&none, &io_harness::Skills::none())[index].label,
            name.strip_prefix('/').expect("a command"),
        );
    }
    assert_eq!(
        commands::palette_pick(&none, &io_harness::Skills::none(), COMMANDS.len()),
        None
    );
}

// ---------------------------------------------------------------------------
// F2 — the palette reaches prompt templates, and expands one into the composer.
//
// Three states, because io-harness distinguishes three: no `[run] templates` is
// an empty section and silence; a directory that reads is a set of rows; and a
// path that is missing or is not a directory is an empty set *and* a sentence.
// `commands::templates` is where all three are decided, which is why it is a
// library function rather than four lines in `drive`.

#[test]
fn f2_no_templates_configured_is_an_empty_section_and_not_an_error() {
    let config = Config::from_toml("").expect("an empty configuration");
    let (found, complaint) = commands::templates(&config);
    assert!(found.is_empty());
    assert_eq!(
        complaint, None,
        "a configuration that never mentioned templates has nothing to complain about",
    );
    assert_eq!(
        commands::palette(&found, &io_harness::Skills::none()).len(),
        COMMANDS.len(),
        "an empty section contributes no rows",
    );
}

#[test]
fn f2_every_template_is_a_row_carrying_its_name_and_its_description() {
    let dir = written();
    let (found, complaint) = commands::templates(&configured(dir.path()));
    assert_eq!(complaint, None, "this directory is exactly what it says");
    assert_eq!(found.len(), 2);

    let rows = commands::palette(&found, &io_harness::Skills::none());
    assert_eq!(rows.len(), COMMANDS.len() + found.len());
    for (offset, template) in found.iter().enumerate() {
        let row = &rows[COMMANDS.len() + offset];
        assert_eq!(row.label, template.name, "the row is not the template");
        // The description as io-harness computed it — the frontmatter's, else the
        // first prose line, else its own `(no description)`. Asserted against the
        // harness's own value rather than against a string written here, because
        // what the contract promises is *the* description and not a restatement.
        //
        // And the marker is at the front, where the picker's fitting rule leaves
        // it: a detail is truncated from the tail, so a marker at the end is the
        // first thing lost on the narrow terminal where a row is hardest to read.
        let detail = format!("{TEMPLATE}{}", template.description);
        assert_eq!(row.detail.as_deref(), Some(detail.as_str()));
    }

    // What the operator can actually see. The templates sit after every command,
    // so in the session's own viewport they are below the fold — typed for, the
    // way `f7_every_row_below_the_fold_is_still_reachable` says they are reached.
    let mut picker = palette_over(&found);
    let height = io_cli::term::VIEWPORT_HEIGHT;
    for template in found.iter() {
        let mut typed = palette_over(&found);
        type_at(&mut typed, &template.name);
        let (mut screen, _recorder) = support::screen_of(80, height + 4, height);
        screen
            .draw(|frame| typed.render(frame, frame.area(), &DARK))
            .expect("frame");
        let viewport = screen.viewport_text();
        assert!(
            viewport.contains(&template.name),
            "{} cannot be reached in the palette: {viewport:?}",
            template.name,
        );
        assert!(
            viewport.contains(TEMPLATE.trim_end()),
            "nothing on screen says which rows are templates: {viewport:?}",
        );
    }

    let (mut screen, recorder) = support::screen_of(80, height + 4, height);
    screen
        .draw(|frame| picker.render(frame, frame.area(), &DARK))
        .expect("frame");
    assert!(recorder.contains("Which command?"));
}

/// **The sabotage test.** Treat `Templates::discover`'s `Err` as an empty set —
/// `Err(_) => (Templates::none(), None)` — and this is the one that goes red.
/// Every other assertion in this file passes under that arm, because an empty set
/// and a set that could not be read draw the identical palette: ten command rows
/// and nothing else. The notice is the only thing that tells them apart, which is
/// exactly what makes swallowing it the defect rather than a shortcut — the same
/// shape `Config::app`'s `.unwrap_or_default()` had in 0.6.0.
#[test]
fn f2_a_configured_directory_that_cannot_be_walked_is_disclosed_with_the_harness_message() {
    let dir = written();

    let missing = dir.path().join("nope");
    let (found, complaint) = commands::templates(&configured(&missing));
    assert!(found.is_empty(), "nothing was discovered, which is true");
    assert_eq!(
        commands::palette(&found, &io_harness::Skills::none()).len(),
        COMMANDS.len(),
        "and the palette therefore looks exactly like the unconfigured one",
    );
    let complaint =
        complaint.expect("a directory that is not there is a mistake, not an empty set");
    assert!(
        complaint.contains("templates directory") && complaint.contains("does not exist"),
        "the harness's own sentence is what says where to look: {complaint:?}",
    );
    assert!(
        complaint.contains(&missing.display().to_string()),
        "and it names the path: {complaint:?}",
    );

    // The other half of the same state: a path that exists and is a file. Its
    // sentence says what to point it at instead, which is the part a rewording
    // here would throw away.
    let one_file = dir.path().join("review.md");
    let (found, complaint) = commands::templates(&configured(&one_file));
    assert!(found.is_empty());
    let complaint = complaint.expect("a file is not a directory of templates");
    assert!(
        complaint.contains("is not a directory")
            && complaint.contains("point it at a directory of markdown files"),
        "{complaint:?}",
    );
}

#[test]
fn f2_choosing_a_template_puts_the_rendered_text_in_the_composer_rather_than_sending_it() {
    let dir = written();
    let (found, _) = commands::templates(&configured(dir.path()));
    let mut app = App::new(DARK, "m");

    let mut picker = palette_over(&found);
    type_at(&mut picker, "review");
    assert_eq!(marked(&picker), "review");
    let Outcome::Chosen(index) = picker.key(key(KeyCode::Enter)) else {
        panic!("Enter must choose");
    };
    let Some(Chosen::Template(name)) =
        commands::palette_pick(&found, &io_harness::Skills::none(), index)
    else {
        panic!("the row under the marker is a template");
    };
    assert_eq!(name, "review");

    let rendered = commands::expand(&found, &name).expect("a template with no placeholder renders");
    app.composer.set(&rendered);
    // The body as it was discovered, reached through a different accessor than the
    // one that rendered it, so this is the file's text rather than a restatement.
    assert_eq!(
        app.composer.text(),
        found.get("review").expect("the template").body,
    );

    // Editable before it is sent, which is the whole difference between a template
    // and a macro. Nothing left this process when the row was chosen: sending is
    // still the operator's own `Enter`, down the submit path the palette never
    // touched.
    type_at_app(&mut app, " Start with the tests.");
    let edited = app.composer.text();
    assert!(edited.ends_with(" Start with the tests."));
    assert_eq!(app.key(key(KeyCode::Enter)), Command::Submit(edited));
}

#[test]
fn f2_a_placeholder_with_no_argument_is_refused_with_the_harness_sentence() {
    let dir = written();
    let (found, _) = commands::templates(&configured(dir.path()));
    let mut app = App::new(DARK, "m");

    // There is no argument-collection surface in this release, so the palette
    // renders against no arguments at all — and a template that needs one is
    // refused rather than sent with a hole in it.
    let error = commands::expand(&found, "bugfix").expect_err("`{{file}}` has no argument");
    assert!(
        error.contains("bugfix") && error.contains("file"),
        "the sentence names the template and the placeholder: {error:?}",
    );
    assert!(
        error.contains("a placeholder resolves or fails and is never empty"),
        "the harness's own words, not a rewording of them: {error:?}",
    );

    // And nothing reached the prompt. A refusal that half-filled the composer
    // would be the hole this refusal exists to prevent, arriving by another door.
    assert!(app.composer.is_empty());
    app.say(io_cli::theme::Tone::Error, error);
    assert!(
        app.composer.is_empty(),
        "the disclosure goes to the scrollback, never to the prompt",
    );
}

#[test]
fn f2_a_row_addresses_the_command_or_the_template_it_was_built_from() {
    // One row list, two inventories, and no parallel array between them: the
    // commands come first and the templates follow in the order `discover` sorted
    // them. A palette that renumbered would expand a different template from the
    // one under the marker, and nothing on screen would say so.
    let dir = written();
    let (found, _) = commands::templates(&configured(dir.path()));
    let rows = commands::palette(&found, &io_harness::Skills::none());

    for (index, (name, _)) in COMMANDS.iter().enumerate() {
        assert_eq!(
            commands::palette_pick(&found, &io_harness::Skills::none(), index),
            Some(Chosen::Command(name)),
        );
    }
    for (offset, template) in found.iter().enumerate() {
        let index = COMMANDS.len() + offset;
        assert_eq!(rows[index].label, template.name);
        assert_eq!(
            commands::palette_pick(&found, &io_harness::Skills::none(), index),
            Some(Chosen::Template(template.name.clone())),
        );
    }
    assert_eq!(
        commands::palette_pick(&found, &io_harness::Skills::none(), rows.len()),
        None
    );
}

/// **F5 — the palette lists what the workspace actually taught the agent.**
///
/// Three inventories now, in one order: the commands, the templates, then the
/// skills io-harness discovered. The names and the descriptions are the harness's
/// own `Skill` fields, asserted against those fields rather than against strings
/// written here — what the contract promises is *the* skill, not a restatement of
/// one.
///
/// Sabotage: build the list from filenames with `std::fs` instead of
/// `Skills::discover`, under which only these tests fail — and they fail by
/// putting a second skill model in a product whose whole rule is that it holds
/// none. (The `SKILL.md` case is what catches it: its name is its *directory*.)
#[test]
fn f5_every_discovered_skill_is_a_row_after_the_templates() {
    let dir = tempfile::tempdir().expect("a directory");
    std::fs::write(
        dir.path().join("migrations.md"),
        "---\nname: migrations\ndescription: how this repo changes a schema\n---\nbody\n",
    )
    .expect("write");
    std::fs::create_dir(dir.path().join("api-style")).expect("mkdir");
    std::fs::write(
        dir.path().join("api-style").join("SKILL.md"),
        "the house style for an endpoint\n",
    )
    .expect("write");

    let (skills, complaint) = commands::skills(Some(dir.path()));
    assert_eq!(complaint, None, "this directory is exactly what it says");
    assert_eq!(skills.len(), 2, "both layouts are discovered");

    let none = Templates::none();
    let rows = commands::palette(&none, &skills);
    assert_eq!(rows.len(), COMMANDS.len() + 2);

    for (offset, skill) in skills.iter().enumerate() {
        let index = COMMANDS.len() + offset;
        assert_eq!(rows[index].label, skill.name, "the row is not the skill");
        assert_eq!(
            rows[index].detail,
            Some(format!("{SKILL}{}", skill.description)),
            "the marker is at the front, where the picker's fitting rule leaves it",
        );
        assert_eq!(
            commands::palette_pick(&none, &skills, index),
            Some(Chosen::Skill(skill.name.clone())),
            "and the row addresses the skill it was built from",
        );
    }
    assert_eq!(commands::palette_pick(&none, &skills, rows.len()), None);
}

/// A skill goes into the prompt by name. The body is the agent's to read, under
/// the run's own policy — a picker that pasted the instructions in would be this
/// crate holding a copy of a skill.
#[test]
fn f5_choosing_a_skill_puts_its_name_in_the_prompt_and_not_its_body() {
    let invocation = commands::invoke_skill("migrations");

    assert!(invocation.contains("migrations"), "{invocation}");
    assert!(
        !invocation.contains("how this repo changes a schema"),
        "no description, and certainly no body: {invocation}",
    );
    assert!(
        invocation.ends_with(' '),
        "the operator has more to say than the name: {invocation:?}",
    );
}

/// No skills directory configured is silence; one that will not walk is an empty
/// set **and a sentence**. Collapsing the second into the first is the shape this
/// product has already paid for twice.
#[test]
fn f5_a_skills_directory_that_will_not_walk_says_so() {
    assert_eq!(
        commands::skills(None).1,
        None,
        "a session that configured none has nothing to complain about",
    );
    assert!(commands::skills(None).0.is_empty());

    let missing = std::path::Path::new("/tmp/io-cli-no-such-skills-dir");
    let (skills, complaint) = commands::skills(Some(missing));
    assert!(skills.is_empty());
    let complaint = complaint.expect("a path that is not there is a sentence, not silence");
    assert!(
        complaint.contains("io-cli-no-such-skills-dir"),
        "the harness's own message names the path: {complaint}",
    );
}

/// **0.13.0 F6 — opening the palette re-places no viewport.**
///
/// The decision is in `src/main.rs`, which nothing links, and `Screen::replace`
/// re-attaches to the real terminal — so this is read off the driver's own text
/// and the wire half is `live_f6_the_palette_opens_without_asking_the_terminal_anything`.
/// Weak on its own and said so; together they are the criterion.
///
/// Sabotage: restore the `replace_viewport(screen, tall)` call — under which this
/// fails, and it fails by putting a terminal round trip back on a keystroke.
#[test]
fn f6_the_palette_path_in_the_driver_replaces_no_viewport() {
    let driver = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("src/main.rs");
    let text = std::fs::read_to_string(driver)
        .expect("the driver")
        .replace("\r\n", "\n");

    // From the condition that opens the palette to the paint that follows it —
    // the whole of the open path, however rustfmt breaks it.
    let opens = text
        .find("opens_palette(")
        .expect("the driver opens the palette somewhere");
    let block = &text[opens..];
    let block = &block[..block
        .find("paint_picker(")
        .expect("the open path ends in a paint")];
    assert!(
        !block.contains("replace_viewport"),
        "opening the palette re-places the viewport: {block}",
    );

    // And the close path: the palette is no longer a height, so nothing anywhere
    // in the driver may ask for one.
    assert!(
        !text.contains("palette_height"),
        "the palette still decides a terminal size",
    );
    assert!(
        text.matches("replace_viewport(").count() <= 3,
        "replace_viewport has grown a call site; the palette's two were removed in 0.13.0",
    );
}

/// **0.13.0 F7 — the whole command list is still reachable at eight rows.**
///
/// 0.11.0's F7 asked the opposite question: it grew the viewport until every row
/// was on screen at once, and paid for that with a terminal round trip on `/` and
/// another on `Esc`. 0.13.0 gives that up, so the property that replaces it is
/// the one `/model` has always had against four hundred models — a row below the
/// fold is reached by typing its name, and by arrowing to it.
///
/// Both halves are here because either alone passes while the other is broken: a
/// picker that filters but will not scroll leaves a row unreachable to an
/// operator who does not know its name, and one that scrolls but will not filter
/// makes a fourteen-row list eight keystrokes deep.
///
/// Sabotage: clamp the picker's rows to what fits and drop the rest — under which
/// only this fails, and it fails by making a command that exists unreachable,
/// which is worse than the round trip this release removed.
#[test]
fn f7_every_row_below_the_fold_is_still_reachable() {
    let dir = written();
    let (templates, _) = commands::templates(&configured(dir.path()));
    let rows = commands::palette(&templates, &io_harness::Skills::none());
    let height = io_cli::term::VIEWPORT_HEIGHT;
    assert!(
        rows.len() > usize::from(height),
        "this test is about a list longer than the viewport, and {} rows is not one",
        rows.len(),
    );

    // The fold, drawn: the picker keeps its top row for the title, so what a
    // session's viewport shows is `height - 1` rows and no more.
    let mut picker = Picker::new("Which command?", rows.clone());
    let (mut screen, _recorder) = support::screen_of(80, height + 4, height);
    screen
        .draw(|frame| picker.render(frame, frame.area(), &DARK))
        .expect("frame");
    let opening = screen.viewport_text();
    let last = rows.last().expect("a row").label.clone();
    assert!(
        !opening.contains(last.as_str()),
        "this list is supposed to be longer than the fold, and {last:?} is already on screen",
    );

    // Reachable by typing its name — the whole of it, one character at a time,
    // which is what an operator actually does.
    let mut typed = Picker::new("Which command?", rows.clone());
    type_at(&mut typed, &last);
    assert!(
        typed.matching() >= 1,
        "the query {last:?} admits no row at all",
    );
    let (mut screen, _recorder) = support::screen_of(80, height + 4, height);
    screen
        .draw(|frame| typed.render(frame, frame.area(), &DARK))
        .expect("frame");
    let filtered = screen.viewport_text();
    assert!(
        filtered.contains(last.as_str()),
        "typing {last:?} did not bring it on screen: {filtered:?}",
    );
    // And the query costs no row: it is drawn in place of the title, which is the
    // rule 0.7.0 set and the reason this fits in the viewport at all.
    assert!(
        !filtered.contains("Which command?"),
        "the query is drawn in place of the title, not above it: {filtered:?}",
    );

    // And reachable by arrowing to it, for an operator who does not know the
    // name: the list scrolls to the selection rather than stopping at the fold.
    let mut arrowed = Picker::new("Which command?", rows.clone());
    for _ in 0..rows.len() {
        arrowed.key(crossterm::event::KeyEvent::from(
            crossterm::event::KeyCode::Down,
        ));
    }
    assert_eq!(
        arrowed.selection(),
        Some(rows.len() - 1),
        "arrowing past the fold did not reach the last row",
    );
    let (mut screen, _recorder) = support::screen_of(80, height + 4, height);
    screen
        .draw(|frame| arrowed.render(frame, frame.area(), &DARK))
        .expect("frame");
    let scrolled = screen.viewport_text();
    assert!(
        scrolled.contains(last.as_str()),
        "the list did not scroll to the selection: {scrolled:?}",
    );
}
