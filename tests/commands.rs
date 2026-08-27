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
            // 0.14.0 — a command and not a key. The keys are nearly all spoken
            // for, and a key is cheap to add later and expensive to take back
            // once it is in anybody's fingers.
            "/status",
            // 0.17.0 — `/status` says how full the window is and this says what
            // is in it. Beside it in the table for that reason.
            "/context",
            // 0.17.0 — a word rather than a default, because a delivered steer
            // emits no event this interface can draw.
            "/steer",
            // 0.17.0 — the other word said *to* a turn, and the only other
            // command whose effect is decided by whether one is running.
            "/compact",
            "/copy",
            "/copy diff",
            "/config",
            // 0.18.0 — the other two surfaces that write a file the operator
            // keeps, and they ask the same scope question `/config` does: three
            // files, and which one is half of every decision made here.
            "/remember",
            "/memory",
            // 0.19.0 — the other half of `/memory`'s question: that one is what io
            // was told, this one is what it was taught.
            "/skills",
            "/mcp",
            "/provider",
            // 0.20.0 — the third surface that lists what the file declared and
            // goes on to change it, and the widest of the three: one directory
            // can hand over skills, templates, agents, servers, hooks and policy
            // at once. That breadth is the argument for it being visible at all.
            "/plugin",
            "/profile",
            "/contain",
            // 0.12.0 — the planning phase stopped being something
            // `[app.io-cli.containment]` switched on by accident, so it needs a
            // switch of its own.
            "/plan",
            "/fleet",
            "/image",
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

/// **A listed command that `parse` has no arm for is advertised and inert**, and
/// until 0.19.0 nothing in this file could see that: every gate here derives from
/// `COMMANDS` and `GROUPS`, which a row satisfies merely by existing. `/skills`
/// was added to the table and to the palette and resolved, silently, to the help
/// listing — the same answer `/models` gets, which is the arm that exists to tell
/// somebody they typed a command that is not real.
///
/// So the property is stated once, over the whole inventory, rather than one
/// `assert_eq!` per command in `each_command_resolves` below — a list like that
/// stops covering the next row the moment somebody forgets to extend it, which is
/// exactly the failure this test exists for. Exactly ONE listed command may
/// resolve to the help listing, and it is `/help`.
#[test]
fn no_listed_command_falls_through_to_the_help_listing() {
    let listing = commands::parse("help", &defaults(), &DARK);
    let inert: Vec<&str> = commands::COMMANDS
        .iter()
        .map(|(name, _)| *name)
        .filter(|name| {
            let typed = name.strip_prefix('/').unwrap_or(name);
            commands::parse(typed, &defaults(), &DARK) == listing
        })
        .collect();
    assert_eq!(
        inert,
        vec!["/help"],
        "these commands are listed but resolve to the help listing, which is what \
         an unknown command resolves to — they are advertised and inert. Give each \
         one an arm in `commands::parse` and an arm in the driver, or take the row \
         out of `COMMANDS`."
    );
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

/// **0.14.0 F10 — `/status` is in the command set and dispatches to its own
/// action.**
///
/// Three claims and they are separable. It is *listed*, so a reader who opens
/// `/help` or the palette finds it without being told it exists. It is a
/// *command and not a key*, which is the decision the contract records: no
/// keybinding in `KEYS` claims it, so nothing has spent one of the few that are
/// left. And it *resolves*, to an action of its own rather than to the print
/// every unknown command falls through to — which is what the driver matches on
/// to commit the page.
///
/// Sabotage: render it as a table — under which F11 fails at eighty columns; the
/// arm this test guards is the one before that, where a command that parses to
/// `Action::Print` would put the *command list* in the scrollback and look, from
/// a distance, like a surface that worked.
#[test]
fn f10_status_is_listed_as_a_command_and_resolves_to_its_own_action() {
    assert!(
        COMMANDS.iter().any(|(name, _)| *name == "/status"),
        "a surface nobody is told about is a surface nobody uses",
    );
    assert_eq!(
        commands::parse("status", &defaults(), &DARK),
        Action::Status
    );
    // Arguments are tolerated and the first word decides, as everywhere else.
    assert_eq!(
        commands::parse("status now", &defaults(), &DARK),
        Action::Status,
    );
    // A command and not a key: nothing in the key table claims it, and the
    // palette row is the whole of how it is reached.
    assert!(
        !KEYS.iter().any(|(_, what)| what.contains("status")),
        "0.14.0 decided this is a command; a key spent on it is a key taken back \
         from somebody's fingers later",
    );
    // It commits into the scrollback like `/expand`, so it must not fall through
    // to the print every unknown command lands on.
    assert_ne!(
        commands::parse("status", &defaults(), &DARK),
        commands::parse("statuss", &defaults(), &DARK),
    );
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
fn attach_is_no_longer_a_command_at_all() {
    // **`/attach` went away in 0.13.1.** A picture is attached by dropping it on
    // the prompt or pasting it, which is what an operator already does in every
    // other window they talk to a model in; a command was something they had to
    // be told about first. The word is not kept as a hidden alias either: it is
    // answered by whatever answers any other word that is not a command.
    for line in ["attach @my pictures/shot.png", "attach"] {
        let Action::Print(said) = commands::parse(line, &defaults(), &DARK) else {
            panic!("{line:?} is still a command");
        };
        let text = text(&said);
        assert!(
            text.contains("there is no /attach"),
            "the answer names the word that was typed: {text}",
        );
        assert!(
            text.contains("/image"),
            "and lists what there is instead: {text}",
        );
        assert!(
            !text.contains("/attach "),
            "the command list does not still carry it: {text}",
        );
    }

    // And `/image` is the command that survived, with a number after it.
    assert_eq!(
        commands::parse("image 1", &defaults(), &DARK),
        Action::Image(Some(1)),
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

    // **The footer, since 0.13.1.** A refusal answers the key that was just
    // pressed; it is not part of the conversation, and it used to be committed
    // into the terminal's permanent scrollback where it stayed forever.
    let said = app
        .status
        .notice
        .as_ref()
        .map(|(_, text)| text.clone())
        .unwrap_or_default();
    assert!(
        said.contains("not while a turn is running"),
        "a refusal that says nothing is a key that appears to do nothing: {said:?}",
    );
    assert!(
        app.take_pending().is_empty(),
        "a refusal does not belong in the transcript",
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

    assert!(
        COMMANDS.iter().any(|(name, _)| *name == "/exit"),
        "`/exit` is in the inventory",
    );

    // Found by name rather than by the inventory's own index: since 0.16.0 the
    // palette is grouped, so its rows are the commands in GROUP order with a
    // heading before each group, and a position in `COMMANDS` addresses neither.
    let rows = palette(&Templates::none(), &io_harness::Skills::none());
    let index = rows
        .iter()
        .position(|row| row.label == "exit")
        .expect("`/exit` has a row");
    assert_eq!(
        palette_pick(&Templates::none(), &io_harness::Skills::none(), index),
        Some(Chosen::Command("/exit")),
        "the row is advertised and inert",
    );
    assert_eq!(commands::parse("exit", &defaults(), &DARK), Action::Quit);
}

/// F14 — `/image` takes the number off the marker.
#[test]
fn f14_image_parses_the_number_a_marker_carries() {
    use io_cli::commands::{parse, Action};

    let keys = io_cli::keys::Keys::default();
    assert_eq!(parse("image 2", &keys, &DARK), Action::Image(Some(2)));
    // `#2` is what is on the prompt, so it is what an operator retypes.
    assert_eq!(parse("image #2", &keys, &DARK), Action::Image(Some(2)));
    assert_eq!(parse("images 1", &keys, &DARK), Action::Image(Some(1)));
    // Nothing, or nonsense: the same answer, which names what there is.
    assert_eq!(parse("image", &keys, &DARK), Action::Image(None));
    assert_eq!(parse("image blue", &keys, &DARK), Action::Image(None));
}

// --- F13: the groups ----------------------------------------------------------

#[test]
fn f13_every_command_is_in_exactly_one_group() {
    // Asserted against COMMANDS rather than against a hand-written list, so a
    // command added later without a group fails here by name rather than
    // quietly appearing in no menu.
    use io_cli::commands::{group_of, Group, GROUPS};

    for (name, _) in COMMANDS {
        let homes: Vec<Group> = GROUPS
            .iter()
            .filter(|(_, names)| names.contains(name))
            .map(|(group, _)| *group)
            .collect();
        assert_eq!(
            homes.len(),
            1,
            "`{name}` is in {} groups; every command belongs to exactly one",
            homes.len(),
        );
        assert!(group_of(name).is_some());
    }

    // And nothing is grouped that is not a command — a stale name here would be
    // a heading over a row that does not exist.
    for (group, names) in GROUPS {
        for name in *names {
            assert!(
                COMMANDS.iter().any(|(command, _)| command == name),
                "`{name}` is grouped under {} and is not a command",
                group.title(),
            );
        }
    }
}

#[test]
fn f13_no_group_is_longer_than_ten() {
    use io_cli::commands::GROUPS;

    for (group, names) in GROUPS {
        assert!(
            names.len() <= 10,
            "the {} group holds {} commands; a group longer than ten is the \
             flat list this release replaced",
            group.title(),
            names.len(),
        );
        assert!(
            !names.is_empty(),
            "the {} group is empty, so it is a heading over nothing",
            group.title(),
        );
    }
}

#[test]
fn f13_grouped_covers_every_command_once() {
    use io_cli::commands::grouped;

    let mut seen: Vec<&str> = grouped()
        .into_iter()
        .flat_map(|(_, rows)| rows.into_iter().map(|(name, _)| name))
        .collect();
    let mut all: Vec<&str> = COMMANDS.iter().map(|(name, _)| *name).collect();
    seen.sort();
    all.sort();
    assert_eq!(seen, all, "the grouped view lost or duplicated a command");
}

// --- F15 and F17: help, and the alias -----------------------------------------

#[test]
fn f15_help_is_grouped_and_carries_every_command() {
    use io_cli::commands::{grouped, help, Group};

    let lines = help(&defaults(), &DARK, io_cli::keys::Newline::of(true));
    let text = lines
        .iter()
        .map(|line| {
            line.spans
                .iter()
                .map(|span| span.content.as_ref())
                .collect::<String>()
        })
        .collect::<Vec<_>>()
        .join("\n");

    for group in Group::all() {
        assert!(
            text.lines().any(|line| line.trim() == group.title()),
            "`/help` has no heading for the {} group: {text}",
            group.title(),
        );
    }
    for (name, _) in COMMANDS {
        assert!(
            text.contains(name),
            "`/help` is missing {name}, which is a command an operator can type",
        );
    }
    // And the grouping is the palette's own rather than a second arrangement.
    let order: Vec<&str> = grouped()
        .into_iter()
        .flat_map(|(_, rows)| rows.into_iter().map(|(name, _)| name))
        .collect();
    let mut at = 0usize;
    for name in order {
        let found = text[at..]
            .find(name)
            .unwrap_or_else(|| panic!("{name} is out of the grouped order in /help"));
        at += found + name.len();
    }
}

#[test]
fn f15_help_is_committed_rather_than_drawn_as_an_overlay() {
    // `/help` answers with `Action::Print`, which the driver commits into the
    // terminal's own scrollback. That is where this product puts anything worth
    // reading twice, and it is what makes help survive the next keystroke.
    match commands::parse("help", &defaults(), &DARK) {
        Action::Print(lines) => assert!(!lines.is_empty()),
        other => panic!("`/help` should be committed, not opened: {other:?}"),
    }
}

// --- 0.18.0: the two commands that reach the memory ---------------------------

/// `/remember` takes the REST of the line, and `/memory` takes nothing.
///
/// The first half is the one with a defect behind it. A line of guidance is a
/// *sentence*, and `split_whitespace().nth(1)` would remember its first word and
/// drop the rest with nothing said — the reader would see a confirmation naming a
/// real file and find one word in it. `/config`'s value arm already documents the
/// same trap for arrays with spaces in them; here what is lost is prose, which
/// nobody notices missing until a later run behaves as though it had never been
/// told.
#[test]
fn remember_keeps_the_whole_line_and_memory_takes_no_argument() {
    assert_eq!(
        commands::parse("remember run the linter before pushing", &defaults(), &DARK),
        Action::Remember("run the linter before pushing".to_string()),
    );
    // Interior spacing is the operator's; only the ends are trimmed.
    assert_eq!(
        commands::parse("remember   two  spaces   ", &defaults(), &DARK),
        Action::Remember("two  spaces".to_string()),
    );
    // The word alone is empty rather than a refusal here: the sentence saying
    // what to type belongs to the driver, which would otherwise open a picker
    // over three files it is about to write nothing into.
    assert_eq!(
        commands::parse("remember", &defaults(), &DARK),
        Action::Remember(String::new()),
    );
    assert_eq!(
        commands::parse("remember    ", &defaults(), &DARK),
        Action::Remember(String::new()),
    );

    assert_eq!(
        commands::parse("memory", &defaults(), &DARK),
        Action::Memory
    );
    // Arguments are tolerated and the first word decides, as everywhere else.
    assert_eq!(
        commands::parse("memory now", &defaults(), &DARK),
        Action::Memory,
    );

    // Neither falls through to the print an unknown command lands on, which is
    // the failure a listed-but-unwired command has: advertised and inert.
    assert!(!matches!(
        commands::parse("memory", &defaults(), &DARK),
        Action::Print(_)
    ));
    assert!(!matches!(
        commands::parse("remember x", &defaults(), &DARK),
        Action::Print(_)
    ));
}

/// Both are listed, both are in the `Configure` group, and both have a palette
/// row that resolves back to the command whole.
///
/// The sabotage arm is listing a command without wiring its row — the one way a
/// listed command must not fail. `palette_pick` is what the driver resolves a
/// chosen row through, so asking it is asking the driver.
#[test]
fn the_memory_commands_are_configure_commands_with_working_palette_rows() {
    use io_cli::commands::{group_of, palette, palette_pick, Chosen, Group};
    use io_harness::{Skills, Templates};

    for name in ["/remember", "/memory"] {
        assert!(
            COMMANDS.iter().any(|(command, _)| *command == name),
            "{name} is not in the inventory, so nobody is told it exists",
        );
        // Configure, and not Inspect: both of these write. That was already the
        // reason in 0.18.0, when Inspect was also full; it is the whole reason
        // now, because 0.19.0 moved `/mcp` and `/provider` out on the same
        // argument and left room behind them.
        assert_eq!(
            group_of(name),
            Some(Group::Configure),
            "{name} writes a file the operator keeps, so it belongs with the group that writes",
        );

        let rows = palette(&Templates::none(), &Skills::none());
        let bare = name.strip_prefix('/').expect("a command carries its slash");
        let index = rows
            .iter()
            .position(|row| row.label == bare)
            .unwrap_or_else(|| panic!("{name} has no palette row"));
        assert_eq!(
            palette_pick(&Templates::none(), &Skills::none(), index),
            Some(Chosen::Command(name)),
            "{name}'s row is advertised and inert",
        );
    }
}

// --- 0.19.0: `/skills`, and the two commands that stopped being inspections ----

/// `/mcp` and `/provider` are `Configure` commands, and `/skills` is the
/// `Inspect` command that took their place.
///
/// Two claims, and the first is the one with a decision behind it. Both of those
/// commands add, edit, disable and remove entries in the configuration file, so
/// filing them under a group whose documentation says "none of it changes what a
/// turn does" was a sentence the surface did not keep. The move is asserted by
/// name rather than by counting, because the count is the *consequence* — a later
/// release that grew `Inspect` back to ten must not be able to quietly file a
/// writer under it again to balance the tables.
///
/// The sabotage arm for the second claim is listing `/skills` without wiring its
/// row: `palette_pick` is what the driver resolves a chosen row through, so a row
/// that comes back as anything but the command whole is advertised and inert.
#[test]
fn the_writers_are_configure_commands_and_skills_is_the_inspection() {
    use io_cli::commands::{group_of, palette, palette_pick, Chosen, Group, GROUPS};
    use io_harness::{Skills, Templates};

    for name in ["/mcp", "/provider"] {
        assert_eq!(
            group_of(name),
            Some(Group::Configure),
            "{name} adds, edits, disables and removes entries in the file the \
             operator keeps, so it belongs with the group that writes",
        );
    }

    assert!(
        COMMANDS.iter().any(|(command, _)| *command == "/skills"),
        "a surface nobody is told about is a surface nobody uses",
    );
    assert_eq!(
        group_of("/skills"),
        Some(Group::Inspect),
        "`/skills` opens a list and writes nothing, which is what Inspect means",
    );

    // The room the move made is real, not an accounting trick: Inspect is under
    // the bound with a command to spare. `f13_no_group_is_longer_than_ten` is
    // still the gate; this only says the release did not spend every seat it
    // freed.
    let inspect = GROUPS
        .iter()
        .find(|(group, _)| *group == Group::Inspect)
        .map(|(_, names)| names.len())
        .expect("Inspect is a group with commands in it");
    assert!(
        inspect < 10,
        "Inspect holds {inspect}; the point of the move was to leave room",
    );

    let rows = palette(&Templates::none(), &Skills::none());
    let index = rows
        .iter()
        .position(|row| row.label == "skills")
        .expect("`/skills` has no palette row");
    assert_eq!(
        palette_pick(&Templates::none(), &Skills::none(), index),
        Some(Chosen::Command("/skills")),
        "`/skills`'s row is advertised and inert",
    );
}

/// `Forgotten::Refused` is reported as a refusal that names why, never as a
/// removal.
///
/// The one outcome of `Store::memory_forget` that must not be collapsed into the
/// others. The note is pinned, so it is not a run's to withdraw and io-cli asks on
/// a run's behalf: it stands, unchanged, and goes on being carried into every
/// later prompt. A surface reporting that as success tells the operator their note
/// is gone while the model keeps reading it — the same failure the pin flag exists
/// to prevent one level down.
#[test]
fn a_refused_withdrawal_is_a_refusal_and_names_the_pin() {
    use io_cli::commands::forgotten_said;
    use io_cli::glyphs::ASCII;
    use io_cli::recall::{Forgotten, Scope};
    use io_cli::theme::Tone;

    let (tone, said) = forgotten_said(
        "build-command",
        Scope::Workspace,
        Forgotten::Refused,
        &ASCII,
    );
    assert_eq!(tone, Tone::Refused, "a refusal is not a success: {said}");
    assert!(
        said.contains("pinned"),
        "the reason is what makes it actionable: {said}",
    );
    assert!(said.contains("Unpin"), "and the way out is named: {said}",);
    assert!(
        !said.contains("withdrawn") && !said.contains("removed"),
        "nothing was withdrawn, so nothing may say it was: {said}",
    );

    // The other two are three different things and stay three.
    let (removed, text) = forgotten_said(
        "k",
        Scope::Global,
        Forgotten::Removed { restore: 9 },
        &ASCII,
    );
    assert_eq!(removed, Tone::Success);
    assert!(text.contains('9'), "the restore point is named: {text}");
    let (absent, text) = forgotten_said("k", Scope::Global, Forgotten::Absent, &ASCII);
    assert_ne!(
        absent,
        Tone::Success,
        "an absent key was not removed: {text}"
    );
    assert_ne!(
        absent,
        Tone::Refused,
        "and it was not refused either: {text}"
    );
}

/// A note that exists and is not pinned reports "no entry" rather than success.
///
/// `Store::memory_pin` answers `false` for *there was no such entry*, which is not
/// "the pin failed" — and a `bool` at a call site reads as "did it work". A surface
/// that believed it shows a pin the store does not hold.
#[test]
fn pinning_a_key_that_is_not_there_is_not_reported_as_a_pin() {
    use io_cli::commands::pinned_said;
    use io_cli::recall::{Pinned, Scope};
    use io_cli::theme::Tone;

    let (tone, said) = pinned_said("gone", Scope::Workspace, true, Pinned::NoEntry);
    assert_ne!(tone, Tone::Success, "{said}");
    assert!(said.contains("nothing was changed"), "{said}");

    let (tone, said) = pinned_said("k", Scope::Workspace, true, Pinned::Set);
    assert_eq!(tone, Tone::Success);
    assert!(said.contains("pinned"), "{said}");
    let (_, unpinned) = pinned_said("k", Scope::Workspace, false, Pinned::Set);
    assert!(unpinned.contains("unpinned"), "{unpinned}");
}

/// The scope rows say what committing each file MEANS, not only its name.
///
/// That is the entire difference between the three: `IO.md`, `AGENTS.md` and
/// `AGENTS.local.md` are three filenames, and which of them goes to everybody who
/// clones the repository is not knowable from any of them. A picker offering three
/// filenames would be asking a question whose answer lives somewhere else.
#[test]
fn the_remember_scope_rows_name_the_file_and_what_committing_it_means() {
    use io_cli::commands::scope_rows;
    use io_cli::glyphs::ASCII;
    use io_harness::config::Scope;

    let paths: Vec<(Scope, std::path::PathBuf)> = [Scope::Project, Scope::Local, Scope::User]
        .into_iter()
        .map(|scope| {
            (
                scope,
                std::path::PathBuf::from("/w").join(io_cli::memory::file_name(scope)),
            )
        })
        .collect();
    let rows = scope_rows(&paths, &ASCII);
    assert_eq!(rows.len(), 3);

    for (row, (scope, path)) in rows.iter().zip(&paths) {
        assert_eq!(
            row.label,
            io_cli::memory::file_name(*scope),
            "the label is the file, which is what the operator knows it by",
        );
        let detail = row.detail.clone().unwrap_or_default();
        assert!(
            detail.contains(&path.display().to_string()),
            "the row names the file it writes: {detail}",
        );
    }

    let said = |n: usize| rows[n].detail.clone().unwrap_or_default();
    assert!(
        said(0).contains("committed"),
        "AGENTS.md is the one that goes to everybody who clones: {}",
        said(0),
    );
    assert!(
        said(1).contains("never committed"),
        "AGENTS.local.md is the one that goes nowhere: {}",
        said(1),
    );
    assert!(
        said(2).contains("every project on this machine"),
        "IO.md is the one that follows the operator: {}",
        said(2),
    );
    // The consequence leads and the path follows, because the picker fits a
    // detail from the head — so on a narrow terminal the sentence that decides
    // the answer is what survives and the path is what goes.
    for row in &rows {
        let detail = row.detail.clone().unwrap_or_default();
        assert!(
            !detail.starts_with('/'),
            "the path leads, so the consequence is what a narrow terminal drops: {detail}",
        );
    }
}

/// The memory page keeps its two lists distinguishable, and never presents a cut
/// draw scan as an exact count.
#[test]
fn the_memory_page_separates_the_two_memories_and_qualifies_a_cut_draw_count() {
    use io_cli::commands::{
        memory_notes, memory_page, Held, LOOSE_MARK, PINNED_MARK, READ_MARK, UNREAD_MARK,
    };
    use io_cli::glyphs::ASCII;
    use io_cli::memory::Instruction;
    use io_cli::recall::{Caps, Remembered, Scope, View};
    use io_harness::config::Scope as FileScope;

    let files = vec![
        Instruction {
            scope: FileScope::Project,
            path: "/w/AGENTS.md".into(),
            exists: true,
            lines: 12,
            read: true,
        },
        // The case the page exists for: there, and not being read.
        Instruction {
            scope: FileScope::Local,
            path: "/w/AGENTS.local.md".into(),
            exists: true,
            lines: 3,
            read: false,
        },
        Instruction {
            scope: FileScope::User,
            path: "/home/someone/.io-cli/IO.md".into(),
            exists: false,
            lines: 0,
            read: false,
        },
    ];
    let entries = vec![
        Remembered {
            scope: Scope::Workspace,
            key: "build-command".into(),
            value: "cargo test".into(),
            kind: "fact",
            pinned: true,
            run_id: 4,
            step: 2,
            created_at: "2026-08-26 09:00".into(),
            draws: 6,
        },
        Remembered {
            scope: Scope::Global,
            key: "prefers-terse".into(),
            value: "keep answers short".into(),
            kind: "decision",
            pinned: false,
            run_id: 9,
            step: 1,
            created_at: "2026-08-26 09:01".into(),
            draws: 0,
        },
    ];

    let (rows, held) = memory_page(&files, &entries, false, &ASCII);
    assert_eq!(
        rows.len(),
        held.len(),
        "the rows and what they stand for are built in one pass and cannot differ",
    );
    // Two headings, so the two memories are never one list.
    let headings: Vec<&str> = rows
        .iter()
        .filter(|row| row.heading)
        .map(|row| row.label.as_str())
        .collect();
    assert_eq!(headings.len(), 2, "got {headings:?}");
    for (row, held) in rows.iter().zip(&held) {
        assert_eq!(row.heading, *held == Held::Nothing);
    }

    let mark = |label: &str| {
        rows.iter()
            .find(|row| row.label == label)
            .unwrap_or_else(|| panic!("no row for {label}"))
            .mark
            .expect("every row but a heading carries a mark")
    };
    assert_eq!(mark("AGENTS.md"), READ_MARK);
    assert_eq!(mark("AGENTS.local.md"), UNREAD_MARK);
    assert_eq!(mark("build-command"), PINNED_MARK);
    assert_eq!(mark("prefers-terse"), LOOSE_MARK);

    // A file that exists and is not read SAYS so; nothing else in this product
    // does, because io-harness skips one without a word.
    let detail = |label: &str| {
        rows.iter()
            .find(|row| row.label == label)
            .and_then(|row| row.detail.clone())
            .unwrap_or_default()
    };
    assert!(
        detail("AGENTS.local.md").contains("NOT read"),
        "{}",
        detail("AGENTS.local.md")
    );
    assert!(
        detail("IO.md").contains("not written yet"),
        "{}",
        detail("IO.md")
    );
    assert!(
        detail("AGENTS.md").starts_with("read"),
        "{}",
        detail("AGENTS.md")
    );
    // The bucket a note came from leads its detail: "is this true here, or true
    // everywhere" is the only question the two scopes exist to answer.
    assert!(detail("build-command").starts_with("workspace"));
    assert!(detail("prefers-terse").starts_with("global"));

    // An uncut scan states a count.
    assert!(
        detail("build-command").contains("6 draws"),
        "{}",
        detail("build-command")
    );
    assert!(
        !detail("build-command").contains("or more"),
        "an uncut scan is a count, not a floor: {}",
        detail("build-command"),
    );
    // A cut one states a floor, in words, on every row.
    let (cut_rows, _) = memory_page(&files, &entries, true, &ASCII);
    for label in ["build-command", "prefers-terse"] {
        let detail = cut_rows
            .iter()
            .find(|row| row.label == label)
            .and_then(|row| row.detail.clone())
            .unwrap_or_default();
        assert!(
            detail.contains("draws or more"),
            "a cut scan makes every count a lower bound: {detail}",
        );
    }

    // And the caps are stated PER SCOPE, with the real ceiling named. One number
    // is half the ceiling for a run drawing on both, and half a ceiling makes an
    // ordinary eviction read as a defect.
    let view = View {
        workspace: "/w".into(),
        entries,
        caps: [Scope::Workspace, Scope::Global]
            .map(|scope| Caps {
                scope,
                limits: io_harness::MemoryLimits {
                    max_entries: 32,
                    max_chars: 8_000,
                    max_entry_chars: 500,
                },
            })
            .to_vec(),
        trace: Vec::new(),
        draws_cut: true,
    };
    let notes = memory_notes(&view, &ASCII).join("\n");
    assert!(notes.contains("per scope"), "{notes}");
    assert!(notes.contains("workspace 32 entries"), "{notes}");
    assert!(notes.contains("global 32 entries"), "{notes}");
    assert!(
        notes.contains("64 entries"),
        "the ceiling across both scopes is the honest number: {notes}",
    );
    assert!(
        notes.contains("lower bound"),
        "a cut scan is disclosed on the page, not only on a row: {notes}",
    );
    assert!(
        notes.contains("/w"),
        "the bucket that answered is named, so an empty list can be told from the \
         wrong bucket: {notes}",
    );
}

#[test]
fn f17_usage_answers_and_is_never_listed() {
    use io_cli::commands::{group_of, palette, GROUPS};

    // It answers, with what `/status` answers.
    assert_eq!(commands::parse("usage", &defaults(), &DARK), Action::Status);
    assert_eq!(
        commands::parse("status", &defaults(), &DARK),
        Action::Status
    );

    // And it is nowhere: not in the inventory, not in a group, not a palette row.
    assert!(
        !COMMANDS.iter().any(|(name, _)| *name == "/usage"),
        "an alias earns no row of its own",
    );
    assert!(group_of("/usage").is_none());
    for (_, names) in GROUPS {
        assert!(!names.contains(&"/usage"));
    }
    let rows = palette(&io_harness::Templates::none(), &io_harness::Skills::none());
    assert!(
        !rows.iter().any(|row| row.label == "usage"),
        "a second row for one screen reads as a second screen",
    );
}
