//! The five slash commands and the keybinding table.

use io_cli::commands::{self, Action, COMMANDS, KEYS};
use io_cli::theme::DARK;

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
fn the_five_commands_are_the_five_commands() {
    let names: Vec<&str> = COMMANDS.iter().map(|(name, _)| *name).collect();
    assert_eq!(
        names,
        ["/help", "/quit", "/setup", "/theme", "/model"],
        "the palette is 0.7.0; this release ships five",
    );
    for (name, what) in COMMANDS {
        assert!(name.starts_with('/'), "{name}");
        assert!(!what.is_empty(), "{name} needs a description");
    }
}

#[test]
fn each_command_resolves() {
    assert!(matches!(commands::parse("help", &DARK), Action::Print(_)));
    assert_eq!(commands::parse("quit", &DARK), Action::Quit);
    assert_eq!(commands::parse("setup", &DARK), Action::Setup);
    assert_eq!(commands::parse("theme", &DARK), Action::Theme);
    assert_eq!(commands::parse("model", &DARK), Action::Model);

    // Arguments are tolerated; the first word decides.
    assert_eq!(commands::parse("model gpt-5", &DARK), Action::Model);
    // An empty command is help, which is what a bare `/` and Enter means.
    assert!(matches!(commands::parse("", &DARK), Action::Print(_)));
}

#[test]
fn an_unknown_command_says_what_does_exist() {
    let Action::Print(lines) = commands::parse("models", &DARK) else {
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
    let printed = text(&commands::help(&DARK));
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
    ] {
        assert!(documented.contains(&key), "{key} is bound but undocumented");
    }
    assert_eq!(
        documented.len(),
        7,
        "a key was added to the table without being added to this list, or the \
         other way round",
    );
}
