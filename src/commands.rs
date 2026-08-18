//! The slash commands, and the keybinding table they document.
//!
//! Each one is a [`Picker`](crate::picker::Picker), a print, or something
//! committed into the terminal's own scrollback — and as of 0.7.0 there is a
//! [`Picker`](crate::picker::Picker) in front of the whole list as well. `/` at
//! an empty prompt opens the palette over [`COMMANDS`]; see [`opens_palette`]
//! for why that decision lives here rather than in [`crate::app`], and
//! [`palette`] for why its rows drop the slash the composer gets back.
//!
//! **Everything that shows more of something commits upward.** The viewport is
//! four rows and cannot grow, so `/expand` and `Ctrl+T` do not open a pane — they
//! write into the scrollback, where the terminal's own search, selection and
//! copy-mode already work. That is one answer to "show me more" rather than
//! three, and it is the same answer the transcript gives.

use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use ratatui::text::{Line, Span};

// Qualified rather than imported: `Action` in this module is already a slash
// command's outcome, and two types with one name in one file is how a reader
// ends up reading the wrong one.
use crate::keys::{self, Keys};
use crate::picker::Row;
use crate::theme::{Theme, Tone};

/// Every key this release binds **by default**, as data rather than as prose.
///
/// The table is the documentation: the README renders this list, `/help` renders
/// [`rows`] — which is this list with the session's own bindings substituted into
/// it — and neither can drift from the other or from the code.
///
/// The first column of a rebindable row is the *default* spelling, and it is
/// what [`rows`] matches a [`keys::Action`] on. That is a join on a display string,
/// which would be fragile if either side could move on its own; `tests/keys.rs`
/// asserts that every action's default binding renders to a row that is in here,
/// so a default changed in one place and not the other fails a test rather than
/// quietly dropping a row out of the rebindable set.
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
    (
        "Shift+Tab",
        "cycle the permission posture, from the next turn",
    ),
    ("Ctrl+L", "clear the viewport, never the scrollback"),
    (
        "Esc Esc",
        "at an empty prompt, undo the last turn — its files and all",
    ),
    (
        "Ctrl+T",
        "put the whole conversation back into the scrollback",
    ),
    (
        "y / a / n",
        "answer an approval: allow once, allow this session, deny",
    ),
    ("Esc", "close a picker without choosing"),
];

/// Every slash command, likewise.
pub const COMMANDS: &[(&str, &str)] = &[
    ("/help", "this table"),
    ("/quit", "leave"),
    ("/setup", "run the first-run wizard again"),
    ("/theme", "change the theme for this session"),
    ("/model", "change the model the next turn is sent to"),
    ("/resume", "reopen an earlier session where it stopped"),
    (
        "/fork",
        "continue from an earlier turn of this conversation",
    ),
    (
        "/expand",
        "commit the last step's full detail into the scrollback",
    ),
    ("/copy", "put the last answer on the system clipboard"),
    (
        "/copy diff",
        "put the whole run's patch on the system clipboard",
    ),
];

/// Whether this keystroke opens the slash palette.
///
/// `/` at an empty prompt, and only there. A `/` inside a line is a path
/// separator or a fraction, and it stays an ordinary character: a palette that
/// took the keyboard away in the middle of a sentence would make the composer
/// unusable for exactly the prompts that name files.
///
/// **The driver asks this in front of [`crate::app::App::key`], not inside it**,
/// and both halves of that matter. In front, because the palette is a picker and
/// every picker in this product is opened and owned by the driver — and because
/// a `/` that never reaches the composer is what makes backing out leave the
/// prompt untouched. Not inside, because `App` must go on treating `/` as a
/// letter: `/theme` typed by hand submits through `Reply::Submitted` and
/// [`parse`] whether or not a palette exists, which is what keeps the palette a
/// faster way to type rather than a second dispatcher.
///
/// `armed` is the price of asking in front. `App::key` is what clears a
/// half-pressed sequence, so a keystroke that never reaches it clears nothing —
/// and the only sequence this product ships is the rewind, whose second press
/// changes the operator's files on io-cli's own initiative. So the palette
/// declines while something is armed: the `/` falls through to the session, the
/// arming is cleared by it exactly as any other key clears it, and one literal
/// slash is typed. That is the behaviour every release before the palette had,
/// which is the right thing for a rejected case to fall back to.
pub fn opens_palette(key: KeyEvent, prompt_empty: bool, armed: bool) -> bool {
    key.code == KeyCode::Char('/')
        // A `Ctrl` or `Alt` chord is a command somebody meant, not a letter they
        // typed — the same rule `Picker::key` applies to its own filter.
        && !key.modifiers.contains(KeyModifiers::CONTROL)
        && !key.modifiers.contains(KeyModifiers::ALT)
        && prompt_empty
        && !armed
}

/// The palette's rows: every command in [`COMMANDS`], in order.
///
/// **The label is the command with its leading `/` removed**, and that is a
/// matching decision rather than a cosmetic one. [`crate::fuzzy`] ranks an exact
/// name above a prefix above a scattered subsequence, and with the slash left on
/// every label begins with the same character — so no query the operator can
/// type is ever a prefix of a row, both of the top tiers are unreachable, and
/// `f` would order `fork` against `copy diff` by gap arithmetic alone. Stripped,
/// typing a command's name puts that command first, which is the whole promise.
///
/// The slash comes back at the other end: [`palette_command`] is what the chosen
/// row puts in the composer, and it reads the name out of [`COMMANDS`] whole.
///
/// The description rides along as the row's detail. It is the first thing the
/// picker drops on a narrow terminal and it is deliberately not matched — a row
/// kept by a hit inside text that is not on screen is a filter whose result the
/// operator cannot account for.
pub fn palette() -> Vec<Row> {
    COMMANDS
        .iter()
        // `strip_prefix` rather than a trim of every leading slash: a command is
        // spelled with exactly one, and a trim would quietly swallow a second.
        .map(|(name, what)| Row::with_detail(name.strip_prefix('/').unwrap_or(name), *what))
        .collect()
}

/// What choosing the palette's row at `index` puts in the composer.
///
/// The index is the one [`crate::picker::Outcome::Chosen`] carries, which
/// addresses the rows the picker was given — and those are [`palette`]'s, which
/// are [`COMMANDS`] in order. So this reads the inventory positionally, the same
/// way the `/resume` and `/fork` pickers read their id lists, and returns the
/// name whole: `/copy diff` rather than the two words the row was labelled with.
///
/// `None` for an index past the end. There is no such row today, and a caller
/// that finds one should put nothing in the prompt rather than a command it
/// guessed at.
pub fn palette_command(index: usize) -> Option<&'static str> {
    COMMANDS.get(index).map(|(name, _)| *name)
}

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
    /// Open the picker over the sessions the store holds.
    Resume,
    /// Open the picker over the turns of the conversation that is open.
    Fork,
    /// Commit the last step's stored detail into the scrollback.
    ///
    /// The detail is in the run's durable trace already — this reads it back
    /// rather than the screen having been the archive.
    Expand,
    /// Put something on the system clipboard over OSC 52.
    Copy(Copied),
    /// Put the whole conversation back into the scrollback.
    Transcript,
}

/// What `/copy` was asked for.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Copied {
    /// The last thing the agent said.
    Answer,
    /// Every change the run made, as one patch.
    Diff,
}

/// The key table as this session actually behaves.
///
/// **`/help` renders this, never [`KEYS`] directly, and that is the whole point
/// of the release's rebinding half.** A help screen that showed the shipped
/// defaults to somebody who had moved a key would be worse than no rebinding at
/// all: rebinding without a truthful table leaves the operator with a product
/// whose documentation is confidently wrong about the machine in front of them,
/// and no way to find out but by pressing keys.
///
/// A row the session does not own — the composer's keys, an approval's letters,
/// the picker's `Esc` — passes through unchanged, because nothing in this
/// release can move it.
///
/// `Ctrl+C` is marked rather than silently identical to the others. It is fixed,
/// a reader consulting the table is exactly the reader who might be about to try
/// rebinding it, and a table that shows one immovable key beside five movable
/// ones without saying which is which is a table that invites the attempt.
pub fn rows(keys: &Keys) -> Vec<(String, String)> {
    let defaults = Keys::default();
    KEYS.iter()
        .map(|(name, what)| {
            let Some(action) = keys::Action::ALL
                .iter()
                .copied()
                .find(|action| defaults.binding(*action).to_string() == *name)
            else {
                return ((*name).to_string(), (*what).to_string());
            };
            let what = if action.rebindable() {
                (*what).to_string()
            } else {
                format!("{what} (fixed)")
            };
            (keys.binding(action).to_string(), what)
        })
        .collect()
}

/// Resolve a command. The leading `/` has already been removed.
///
/// An unknown command prints the list rather than erroring: a user who typed
/// `/models` wants to be told what does exist, not that they were wrong.
pub fn parse(input: &str, keys: &Keys, theme: &Theme) -> Action {
    match input.split_whitespace().next().unwrap_or("help") {
        "help" | "?" => Action::Print(help(keys, theme)),
        "quit" | "exit" | "q" => Action::Quit,
        "setup" => Action::Setup,
        "theme" => Action::Theme,
        "model" => Action::Model,
        // `/resume` and `/continue` mean the same thing. Both words are in the
        // field's vocabulary and a reader who has used another agent will type
        // whichever one that agent taught them.
        "resume" | "continue" => Action::Resume,
        "fork" | "branch" => Action::Fork,
        "expand" => Action::Expand,
        "copy" => match input.split_whitespace().nth(1) {
            // `/copy diff` and `/copy patch` mean the same thing. A reader who
            // has just been shown a diff will type the word they were shown.
            Some("diff") | Some("patch") => Action::Copy(Copied::Diff),
            _ => Action::Copy(Copied::Answer),
        },
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

/// The `/help` output: the keys in force, then the commands.
pub fn help(keys: &Keys, theme: &Theme) -> Vec<Line<'static>> {
    let bound = rows(keys);
    // Both tables, so `/help` lines up as one table rather than two — and
    // measured over the bindings in force rather than over the defaults, because
    // a rebinding can be wider than what it replaced.
    let width = column(&bound).max(column(COMMANDS));
    let mut lines = vec![Line::from(Span::styled(
        "Keys".to_string(),
        theme.style(Tone::Accent),
    ))];
    lines.extend(table(&bound, width, theme));
    lines.push(Line::from(""));
    lines.push(Line::from(Span::styled(
        "Commands".to_string(),
        theme.style(Tone::Accent),
    )));
    lines.extend(table(COMMANDS, width, theme));
    lines.push(Line::from(""));
    lines
}

/// The command table on its own, for the reader who typed a command that does
/// not exist. Its first column is measured over the defaults, because there is
/// no key table beside it here to line up with.
fn commands(theme: &Theme) -> Vec<Line<'static>> {
    table(COMMANDS, column(COMMANDS), theme)
}

/// The widest first column of a table.
fn column<S: AsRef<str>>(rows: &[(S, S)]) -> usize {
    rows.iter()
        .map(|(name, _)| name.as_ref().chars().count())
        .max()
        .unwrap_or(0)
}

fn table<S: AsRef<str>>(rows: &[(S, S)], width: usize, theme: &Theme) -> Vec<Line<'static>> {
    rows.iter()
        .map(|(name, what)| (name.as_ref(), what.as_ref()))
        .map(|(name, what)| {
            Line::from(vec![
                Span::styled(format!("  {name:width$}  "), theme.style(Tone::Normal)),
                // The em dash in a description is prose rather than a marker,
                // but it is still a glyph that reaches a terminal, and a table
                // is the one surface a reader consults precisely because they
                // could not read something else. Substituted here rather than
                // spelled per row, so a row added later cannot forget.
                Span::styled(
                    what.replace('\u{2014}', theme.glyphs.dash),
                    theme.style(Tone::Muted),
                ),
            ])
        })
        .collect()
}
