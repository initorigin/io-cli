//! The slash commands and the keybinding table.

use io_cli::commands::{self, Action, COMMANDS, KEYS};
use io_cli::keys::{Keys, Newline};
use io_cli::theme::DARK;

/// The bindings a session with no `[app.io-cli.keys]` runs under. What each of
/// these tests asserts about the defaults, `tests/keys.rs` asserts about a file
/// that has moved them.
fn defaults() -> Keys {
    Keys::default()
}

fn text(lines: &[ratatui::text::Line<'_>]) -> String {
    lines
        .iter()
        .map(|line| {
            line.spans
                .iter()
                .map(|span| span.content.as_ref())
                .collect::<String>()
        })
        .collect::<Vec<_>>()
        .join("\n")
}

#[test]
fn the_commands_are_the_commands() {
    let names: Vec<&str> = COMMANDS.iter().map(|(name, _)| *name).collect();
    assert_eq!(
        names,
        [
            "/help",
            "/exit",
            "/setup",
            "/theme",
            "/model",
            "/resume",
            "/fork",
            "/expand",
            "/copy",
            "/copy diff",
            "/contain",
            // 0.12.0 — the planning phase stopped being something
            // `[app.io-cli.containment]` switched on by accident, so it needs a
            // switch of its own.
            "/plan",
            "/fleet",
            "/attach",
            "/clear",
        ],
        "the fuzzy palette is still 0.7.0; this list is written out so that adding \
         a command is a decision somebody makes rather than a line somebody adds",
    );
    for (name, what) in COMMANDS {
        assert!(name.starts_with('/'), "{name}");
        assert!(!what.is_empty(), "{name} needs a description");
    }
}

#[test]
fn each_command_resolves() {
    assert!(matches!(
        commands::parse("help", &defaults(), &DARK),
        Action::Print(_)
    ));
    assert_eq!(commands::parse("exit", &defaults(), &DARK), Action::Quit);
    assert_eq!(commands::parse("setup", &defaults(), &DARK), Action::Setup);
    assert_eq!(commands::parse("theme", &defaults(), &DARK), Action::Theme);
    assert_eq!(commands::parse("model", &defaults(), &DARK), Action::Model);
    assert_eq!(
        commands::parse("expand", &defaults(), &DARK),
        Action::Expand
    );
    // `/copy` with no argument is the answer; `diff` and `patch` are the same
    // thing, because a reader who has just been shown a diff types the word they
    // were shown.
    assert_eq!(
        commands::parse("copy", &defaults(), &DARK),
        Action::Copy(io_cli::commands::Copied::Answer),
    );
    assert_eq!(
        commands::parse("copy diff", &defaults(), &DARK),
        Action::Copy(io_cli::commands::Copied::Diff),
    );
    assert_eq!(
        commands::parse("copy patch", &defaults(), &DARK),
        Action::Copy(io_cli::commands::Copied::Diff),
    );

    // 0.11.0 — `/clear` and its alias, and `/exit`, which has resolved to the
    // same action as `/quit` since 0.1.0 and was listed nowhere until now.
    assert_eq!(commands::parse("clear", &defaults(), &DARK), Action::Clear);
    assert_eq!(commands::parse("new", &defaults(), &DARK), Action::Clear);
    assert_eq!(commands::parse("exit", &defaults(), &DARK), Action::Quit);

    // Arguments are tolerated; the first word decides.
    assert_eq!(
        commands::parse("model gpt-5", &defaults(), &DARK),
        Action::Model
    );
    // An empty command is help, which is what a bare `/` and Enter means.
    assert!(matches!(
        commands::parse("", &defaults(), &DARK),
        Action::Print(_)
    ));
}

#[test]
fn an_unknown_command_says_what_does_exist() {
    let Action::Print(lines) = commands::parse("models", &defaults(), &DARK) else {
        panic!("an unknown command should print rather than do nothing");
    };
    let printed = text(&lines);
    assert!(printed.contains("there is no /models"), "{printed:?}");
    assert!(
        printed.contains("warning"),
        "the notice should carry a word, not only a colour: {printed:?}",
    );
    for (name, _) in COMMANDS {
        assert!(printed.contains(name), "{name} is missing from {printed:?}");
    }
}

#[test]
fn help_prints_every_key_and_every_command() {
    // `KEYS` is the table for a terminal that can report `Shift+Enter`, so the
    // naming this renders with is that one. What the *other* terminal's table
    // says is F9's own question and `tests/keyboard.rs` asks it.
    let printed = text(&commands::help(&defaults(), &DARK, Newline::of(true)));
    for (key, what) in KEYS {
        assert!(printed.contains(key), "{key} is missing from /help");
        assert!(printed.contains(what), "{key}'s description is missing");
    }
    for (name, what) in COMMANDS {
        assert!(printed.contains(name), "{name} is missing from /help");
        assert!(printed.contains(what), "{name}'s description is missing");
    }
}

#[test]
fn the_key_table_covers_every_key_this_release_binds() {
    // The table is the documentation, so a key that is bound and undocumented is
    // folklore. These are the bindings `App::key` and `Composer::key` implement.
    let documented: Vec<&str> = KEYS.iter().map(|(key, _)| *key).collect();
    for key in [
        "Enter",
        "Shift+Enter",
        "Up / Down",
        "Ctrl+C",
        "Ctrl+D",
        "Ctrl+L",
        "Esc",
        // The one key in this product that changes the operator's files on
        // io-cli's own initiative rather than the agent's, which is why it is two
        // presses and why it is documented as two.
        "Esc Esc",
        "Shift+Tab",
        "Ctrl+T",
        // 0.8.0. It has a key as well as `/fleet` because the moment it is worth
        // opening is mid-turn, and a slash command cannot be typed then.
        "Ctrl+F",
        "y / a / n",
        // The three prefixes 0.7.0 adds. None is a chord, and none is
        // rebindable: they are characters the composer would otherwise have
        // taken, so what they cost is a literal `/`, `@` or `!` in the one
        // position each is claimed in.
        "/",
        "@",
        "!",
    ] {
        assert!(documented.contains(&key), "{key} is bound but undocumented");
    }
    assert_eq!(
        documented.len(),
        15,
        "a key was added to the table without being added to this list, or the \
         other way round",
    );
}

/// `/attach` takes the REST of the line, not its second word.
///
/// A path may contain spaces and the `@` completion that produced it does not
/// quote, so taking one token would attach the wrong file — or nothing — for
/// exactly the paths a reader is least able to retype. The `@` itself is left on:
/// stripping it belongs to `attach::prepare`, beside the read it guards, rather
/// than in two places.
#[test]
fn attach_takes_the_whole_rest_of_the_line() {
    assert_eq!(
        commands::parse("attach @my pictures/shot.png", &defaults(), &DARK),
        Action::Attach("@my pictures/shot.png".to_string()),
    );
    assert_eq!(
        commands::parse("attach", &defaults(), &DARK),
        Action::Attach(String::new()),
        "no argument is a request for the sentence, not an error",
    );
    // `/image` is the other word a reader might reach for, the way `/continue`
    // stands beside `/resume`.
    assert_eq!(
        commands::parse("image shot.png", &defaults(), &DARK),
        Action::Attach("shot.png".to_string()),
    );
}

/// 0.11.0 F8 — `/clear` starts over from an idle prompt.
///
/// The half of it that is not the driver's: every run-scoped field back to what
/// it was before anything ran, and nothing left in `Events` that belonged to the
/// conversation being ended — a tail nobody committed, a call nothing will close,
/// or the thought `/expand` would otherwise show from a conversation no longer on
/// screen.
#[test]
fn f8_clear_at_an_idle_prompt_resets_every_run_scoped_field() {
    use io_cli::app::App;

    let mut app = App::new(DARK, "anthropic/claude-sonnet-4");
    app.status.provider = Some("openrouter".into());
    app.status.tokens = Some(4_703);
    app.status.steps = Some(3);
    app.event(
        &io_harness::RunEvent::new(
            1,
            1,
            io_harness::EventKind::Reasoning {
                text: "x ".repeat(400),
                tokens: 90,
            },
        ),
        std::time::Duration::ZERO,
    );
    assert!(app.events.thought().is_some(), "the thought was held");

    assert!(app.clear_conversation(), "an idle prompt is not refused");
    assert_eq!(app.status.provider, None);
    assert_eq!(app.status.tokens, None);
    assert_eq!(app.status.steps, None);
    assert_eq!(app.events.thought(), None);
    assert!(app.events.live().is_empty());
}

/// 0.11.0 F8 — and refuses while a turn is in flight, changing nothing.
///
/// The criterion's own sabotage arm is clearing while a turn is running, which
/// orphans a live run behind a cleared screen. The driver refuses a slash command
/// during a turn too — this is the lock a test can stand on, and the two agree.
#[test]
fn f8_clear_refuses_while_a_turn_is_in_flight_and_changes_nothing() {
    use io_cli::app::App;

    let mut app = App::new(DARK, "m");
    app.status.tokens = Some(4_703);
    app.status.steps = Some(3);
    app.started();

    assert!(
        !app.clear_conversation(),
        "a live run was cleared out from under"
    );
    assert_eq!(
        app.status.tokens,
        Some(4_703),
        "the run's own facts survived"
    );
    assert_eq!(app.status.steps, Some(3));

    let said = text(&app.take_pending());
    assert!(
        said.contains("not while a turn is running"),
        "a refusal that says nothing is a key that appears to do nothing: {said:?}",
    );
}

/// 0.11.0 F9 — `/exit` is listed, and the palette row it is listed as leaves.
///
/// The sabotage arm is listing it without wiring the row, which is the one way a
/// listed command must not fail: advertised and inert. `palette_pick` is what the
/// driver resolves a chosen row through, so asking it is asking the driver.
#[test]
fn f9_exit_is_listed_and_its_palette_row_leaves() {
    use io_cli::commands::{palette, palette_pick, Chosen, COMMANDS};
    use io_harness::Templates;

    let index = COMMANDS
        .iter()
        .position(|(name, _)| *name == "/exit")
        .expect("`/exit` is in the inventory");

    let rows = palette(&Templates::none(), &io_harness::Skills::none());
    assert_eq!(rows[index].label, "exit");
    assert_eq!(
        palette_pick(&Templates::none(), &io_harness::Skills::none(), index),
        Some(Chosen::Command("/exit")),
        "the row is advertised and inert",
    );
    assert_eq!(commands::parse("exit", &defaults(), &DARK), Action::Quit);
}
