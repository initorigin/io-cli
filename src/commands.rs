//! The five slash commands, and the keybinding table they document.
//!
//! Five, and each one is a [`Picker`](crate::picker::Picker) or a print. The
//! fuzzy palette that reaches every command and every harness skill is 0.7.0's;
//! what matters now is that `/setup` exists, because it is what makes the wizard
//! reachable after the first run.

use ratatui::text::{Line, Span};

use crate::theme::{Theme, Tone};

/// Every key this release binds, as data rather than as prose.
///
/// The table is the documentation: the README renders this list, `/help` renders
/// this list, and neither can drift from the other or from the code.
pub const KEYS: &[(&str, &str)] = &[
    ("Enter", "send the prompt"),
    (
        "Shift+Enter",
        "new line (or end the line with \\ and press Enter)",
    ),
    ("Up / Down", "walk prompt history"),
    (
        "Ctrl+C",
        "interrupt the turn; twice at an empty prompt, exit",
    ),
    ("Ctrl+D", "exit, on an empty prompt"),
    ("Ctrl+L", "clear the viewport, never the scrollback"),
    ("Esc", "close a picker without choosing"),
];

/// Every slash command, likewise.
pub const COMMANDS: &[(&str, &str)] = &[
    ("/help", "this table"),
    ("/quit", "leave"),
    ("/setup", "run the first-run wizard again"),
    ("/theme", "change the theme for this session"),
    ("/model", "change the model for this session"),
];

/// What the driver should do about a slash command.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Action {
    /// Commit these lines and carry on.
    Print(Vec<Line<'static>>),
    Quit,
    Setup,
    /// Open the theme picker.
    Theme,
    /// Open the model picker.
    Model,
}

/// Resolve a command. The leading `/` has already been removed.
///
/// An unknown command prints the list rather than erroring: a user who typed
/// `/models` wants to be told what does exist, not that they were wrong.
pub fn parse(input: &str, theme: &Theme) -> Action {
    match input.split_whitespace().next().unwrap_or("help") {
        "help" | "?" => Action::Print(help(theme)),
        "quit" | "exit" | "q" => Action::Quit,
        "setup" => Action::Setup,
        "theme" => Action::Theme,
        "model" => Action::Model,
        unknown => {
            let mut lines = vec![theme.notice(
                Tone::Warning,
                format!("there is no /{unknown}. The commands are:"),
            )];
            lines.extend(commands(theme));
            lines.push(Line::from(""));
            Action::Print(lines)
        }
    }
}

/// The `/help` output: the keys, then the commands.
pub fn help(theme: &Theme) -> Vec<Line<'static>> {
    let mut lines = vec![Line::from(Span::styled(
        "Keys".to_string(),
        theme.style(Tone::Accent),
    ))];
    lines.extend(table(KEYS, theme));
    lines.push(Line::from(""));
    lines.push(Line::from(Span::styled(
        "Commands".to_string(),
        theme.style(Tone::Accent),
    )));
    lines.extend(commands(theme));
    lines.push(Line::from(""));
    lines
}

fn commands(theme: &Theme) -> Vec<Line<'static>> {
    table(COMMANDS, theme)
}

/// The widest first column across every row of both tables, so `/help` lines up
/// as one table rather than two.
fn column() -> usize {
    KEYS.iter()
        .chain(COMMANDS.iter())
        .map(|(name, _)| name.chars().count())
        .max()
        .unwrap_or(0)
}

fn table(rows: &[(&str, &str)], theme: &Theme) -> Vec<Line<'static>> {
    let width = column();
    rows.iter()
        .map(|(name, what)| {
            Line::from(vec![
                Span::styled(format!("  {name:width$}  "), theme.style(Tone::Normal)),
                Span::styled((*what).to_string(), theme.style(Tone::Muted)),
            ])
        })
        .collect()
}
