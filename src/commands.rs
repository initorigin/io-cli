//! The slash commands, and the keybinding table they document.
//!
//! Each one is a [`Picker`](crate::picker::Picker), a print, or something
//! committed into the terminal's own scrollback — and as of 0.7.0 there is a
//! [`Picker`](crate::picker::Picker) in front of the whole list as well. `/` at
//! an empty prompt opens the palette over [`COMMANDS`]; see [`opens_palette`]
//! for why that decision lives here rather than in [`crate::app`], and
//! [`palette`] for why its rows drop the slash the composer gets back.
//!
//! The palette also reaches the prompt templates `[run] templates` points at —
//! one list rather than two, because a second palette would be a second thing to
//! learn and a second place a keystroke could go. [`templates`] is where a
//! configuration becomes a set, [`palette_pick`] is what a chosen row stands for,
//! and [`expand`] is what a chosen template puts in the composer.
//!
//! **Since 0.10.0 it reaches harness skills too, and the two are not the same
//! kind of row.** A template is expanded by this crate into prompt text, so
//! nothing but io-cli is involved. A skill is read by the *model*, through a
//! tool, and whether it may be is decided by a `TaskContract` — so it is listed
//! by name and [`invoke_skill`] puts only that name in the composer. The list
//! comes from [`skills`], which is io-harness's own discovery; nothing here
//! parses a skill file.
//!
//! The rows are listed whatever the session is, and since 0.11.0 the agent can
//! read one on any of them: both turn arms carry a contract, so the `skills`
//! directory reaches the run whether or not the session can fan out. Listing them
//! regardless was already deliberate — a palette that hid them on an unconfigured
//! session would answer "what did I teach it?" with silence — and it is no longer
//! a promise the turn might not keep.
//!
//! **Everything that shows more of something commits upward.** The viewport is
//! four rows and cannot grow, so `/expand` and `Ctrl+T` do not open a pane — they
//! write into the scrollback, where the terminal's own search, selection and
//! copy-mode already work. That is one answer to "show me more" rather than
//! three, and it is the same answer the transcript gives.

use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use io_harness::{Config, Templates};
use ratatui::text::{Line, Span};

// Qualified rather than imported: `Action` in this module is already a slash
// command's outcome, and two types with one name in one file is how a reader
// ends up reading the wrong one.
use crate::keys::{self, Keys, Newline};
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
///
/// **The newline row is the one row here that a terminal can overrule.** It is
/// written as [`Newline::of(true)`](Newline::of) — the spelling for a terminal
/// that can report `Shift+Enter` — because this list is also what the README
/// prints, and a README is read on a machine other than the one it describes. On
/// a terminal that cannot report it, [`rows`] substitutes the other spelling; the
/// join is on this row's key column, and `tests/keyboard.rs` asserts the pair
/// here is the pair [`Newline::of`] returns so the two cannot be worded apart.
pub const KEYS: &[(&str, &str)] = &[
    ("Enter", "send the prompt"),
    (
        "Shift+Enter",
        "new line \u{2014} or `Alt+Enter`, `Ctrl+J`, or end the line with \\",
    ),
    ("Up / Down", "walk prompt history"),
    (
        "Ctrl+C",
        "stop the turn; again to stop it now; twice at an empty prompt, exit",
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
        "Ctrl+F",
        "show the fleet: the children this turn has spawned",
    ),
    (
        "y / a / n",
        "answer an approval: allow once, allow this session, deny",
    ),
    (
        "Esc",
        "stop the running turn, or close a picker without choosing",
    ),
    ("/", "at an empty prompt, open the command palette"),
    ("@", "after a space, complete a path from the workspace"),
    (
        "!",
        "run the rest of the line in your shell; the agent never sees it",
    ),
];

/// Every slash command, likewise.
pub const COMMANDS: &[(&str, &str)] = &[
    ("/help", "this table"),
    ("/exit", "leave"),
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
    // **A command and not a key, and the decision is recorded rather than left
    // open.** The keys are nearly all spoken for, nothing yet shows this is
    // checked often enough to spend one of the few that are left, and a key is
    // cheap to add later and expensive to take back once it is in anybody's
    // fingers. It sits beside `/expand` because the two are the same *kind* of
    // surface — both commit upward into the terminal's own scrollback rather
    // than opening a pane — and a reader looking for one will find the other.
    (
        "/status",
        "commit the whole session state into the scrollback",
    ),
    ("/copy", "put the last answer on the system clipboard"),
    (
        "/copy diff",
        "put the whole run's patch on the system clipboard",
    ),
    (
        "/config",
        "every setting, the value in force and the file that decided it",
    ),
    (
        "/mcp",
        "the MCP servers configured, and what this session has seen of each",
    ),
    (
        "/provider",
        "the providers configured, in the order a turn tries them",
    ),
    (
        "/contain",
        "run turns contained, so the agent can fan out: on, off, or ask",
    ),
    (
        "/plan",
        "make turns propose a plan before they work: on, off, or ask",
    ),
    ("/fleet", "show the children this turn has spawned"),
    ("/image", "draw an attached image again: /image 1"),
    (
        "/clear",
        "start a new conversation; this one stays in /resume",
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

/// What a template row's detail begins with, so a template is never mistaken for
/// a command.
///
/// It rides at the **front** of the detail rather than at the back, and that is
/// where the picker's own truncation rule puts the decision: a detail is fitted
/// rather than wrapped, so the head is what survives a narrow terminal and the
/// tail is what goes. A marker at the end would be the first thing to disappear
/// on exactly the screen where a row is hardest to read.
///
/// Not on the label, because the label is the haystack [`crate::fuzzy`] ranks. A
/// prefix there would give every template row the same first character, which is
/// the same defect [`palette`] strips the slash to avoid — no query could ever be
/// an exact name or a prefix of one, and the whole top of the ranking would be
/// unreachable for templates.
pub const TEMPLATE: &str = "template: ";

/// What marks a palette row as one of the agent's own skills.
///
/// A third kind of row and a third source: a command is this crate's, a template
/// is `[run] templates`, and a skill is whatever io-harness discovered in the
/// configured directory. io-cli parses no skill file — the name and the one-line
/// description below are `Skill`'s own fields.
pub const SKILL: &str = "skill: ";

/// The palette's rows: every command in [`COMMANDS`], then every template.
///
/// Commands first because they are the inventory this product ships and the
/// operator did not have to write; templates after, in the order
/// [`Templates::discover`] sorted them, which is by name and identical across
/// runs. Nothing renumbers between the two halves — see [`palette_pick`], which
/// reads an index back against exactly this ordering.
///
/// A configuration with no templates contributes no rows and no notice. That is
/// the whole of the "not configured" state: an empty section, not an error.
///
/// **The label is the command with its leading `/` removed**, and that is a
/// matching decision rather than a cosmetic one. [`crate::fuzzy`] ranks an exact
/// name above a prefix above a scattered subsequence, and with the slash left on
/// every label begins with the same character — so no query the operator can
/// type is ever a prefix of a row, both of the top tiers are unreachable, and
/// `f` would order `fork` against `copy diff` by gap arithmetic alone. Stripped,
/// typing a command's name puts that command first, which is the whole promise.
///
/// The slash comes back at the other end: [`palette_pick`] is what the chosen
/// row stands for, and it reads the name out of [`COMMANDS`] whole.
///
/// A template's label is its name, unadorned, for the same reason and with the
/// same effect. What says a row is a template is its detail — see [`TEMPLATE`].
///
/// The description rides along as the row's detail. It is the first thing the
/// picker drops on a narrow terminal and it is deliberately not matched — a row
/// kept by a hit inside text that is not on screen is a filter whose result the
/// operator cannot account for.
/// A skill's rows come last, after the commands and the templates, and a session
/// that configured no skills directory contributes none — the same "not
/// configured" shape the templates have.
pub fn palette(templates: &Templates, skills: &io_harness::Skills) -> Vec<Row> {
    COMMANDS
        .iter()
        // `strip_prefix` rather than a trim of every leading slash: a command is
        // spelled with exactly one, and a trim would quietly swallow a second.
        .map(|(name, what)| Row::with_detail(name.strip_prefix('/').unwrap_or(name), *what))
        .chain(templates.iter().map(|template| {
            Row::with_detail(
                template.name.clone(),
                format!("{TEMPLATE}{}", template.description),
            )
        }))
        .chain(skills.iter().map(|skill| {
            Row::with_detail(skill.name.clone(), format!("{SKILL}{}", skill.description))
        }))
        .collect()
}

// **`palette_height` was here until 0.13.0**, and its removal is the release.
// It answered "how tall must the viewport be to show every command at once",
// which made the palette the one surface whose row count decided a terminal
// size — and paying for that answer meant `Screen::replace` on open and again on
// close, each of which asks the terminal where its cursor is and takes the stdin
// lock to read the reply. The palette now draws in the viewport the session
// already has, and the rows that do not fit are reached the way `/model`'s four
// hundred models are: by scrolling and by typing.

/// What the palette's row at `index` stands for.
///
/// The index is the one [`crate::picker::Outcome::Chosen`] carries, which
/// addresses the rows the picker was given — and those are [`palette`]'s, which
/// are [`COMMANDS`] and then the templates, in that order. So this reads both
/// inventories positionally, the same way the `/resume` and `/fork` pickers read
/// their id lists, and there is no parallel array to drift: the one function that
/// lays the rows out and the one function that reads them back are these two, and
/// they are next to each other on purpose.
///
/// A command comes back whole — `/copy diff` rather than the two words the row
/// was labelled with — because what the composer gets is what the operator would
/// have typed. A template comes back by **name**, which is what
/// [`Templates::render`] asks for; the body is not carried here, so nothing has
/// to keep a rendered string alive next to the set it came from.
///
/// `None` for an index past the end. A caller that finds one should put nothing
/// in the prompt rather than something it guessed at.
pub fn palette_pick(
    templates: &Templates,
    skills: &io_harness::Skills,
    index: usize,
) -> Option<Chosen> {
    match COMMANDS.get(index) {
        Some((name, _)) => Some(Chosen::Command(name)),
        // Saturating is not needed: this arm is only reached when `index` is at
        // or past `COMMANDS.len()`.
        None => {
            let after_commands = index - COMMANDS.len();
            match templates.iter().nth(after_commands) {
                Some(template) => Some(Chosen::Template(template.name.clone())),
                None => skills
                    .iter()
                    .nth(after_commands - templates.iter().count())
                    .map(|skill| Chosen::Skill(skill.name.clone())),
            }
        }
    }
}

/// The prompt a chosen skill puts in the composer.
///
/// **By name, and nothing else.** io-harness gives the model a catalogue of the
/// skills discovered for the run and the model opens the file itself, under the
/// run's own policy — so a picker that pasted the instructions into the prompt
/// would be io-cli holding a copy of a skill, which is exactly the kind of model
/// this crate is forbidden to grow. It is left in the composer rather than sent,
/// like every other palette row, because the operator has more to say than the
/// name.
pub fn invoke_skill(name: &str) -> String {
    format!("use the {name} skill: ")
}

/// What a chosen palette row is.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Chosen {
    /// A slash command, spelled the way the composer wants it.
    Command(&'static str),
    /// A prompt template, by the name [`Templates::render`] knows it by.
    Template(String),
    /// One of the agent's own skills, by the name io-harness discovered it under.
    Skill(String),
}

/// Render a template into the text the composer is about to be given.
///
/// **The arguments are empty, deliberately.** There is no argument-collection
/// surface in this release, and inventing one inside a picker's `Enter` would be
/// a second modal in the middle of an existing one. So a template with a
/// `{{placeholder}}` in it is refused here, with io-harness's own sentence — which
/// already names the template, the placeholder and the two ways out of it — and
/// the operator can either pass the value by editing the template or use one that
/// does not need one. Refused rather than sent with a hole in it, because a goal
/// with a hole in it still reads like a goal.
///
/// It lives in the library rather than at its one call site in `src/main.rs`
/// because nothing under `tests/` can link the binary: what arguments this passes
/// is a decision, and a decision written there is one no test drives and no
/// sabotage can make fail.
pub fn expand(templates: &Templates, name: &str) -> Result<String, String> {
    templates
        .render(name, &[])
        .map_err(|error| error.to_string())
}

/// The prompt templates this configuration points at, and what went wrong.
///
/// **Three states, and the seam keeps all three**, because io-harness
/// distinguishes all three: `[run] templates` absent is [`Templates::none`] and
/// silence; a directory that reads is the set; and a path that is missing or is
/// not a directory is an empty set *and a sentence*. Collapsing the third into
/// the second is the shape 0.6.0 already paid for once — see [`crate::settings`],
/// where `.unwrap_or_default()` on `Config::app`'s `Result` silently reverted
/// every setting in the file — and it is worse here for the same reason: a
/// palette that quietly shows no templates looks exactly like a palette that was
/// never configured, and the operator has no thread to pull.
///
/// The notice carries **the harness's own message**, which already names the path
/// and, for the not-a-directory case, says what to point it at instead. Rewording
/// it would drop the only part that says where to look.
///
/// `Config::templates` reads nothing from disk and cannot fail; the walk is
/// [`Templates::discover`]'s, it is fallible, and it happens **once**, when the
/// session starts. A directory walk per keystroke into the palette would be the
/// wrong shape for a filter that runs on every character typed.
pub fn templates(config: &Config) -> (Templates, Option<String>) {
    let Some(dir) = config.templates() else {
        return (Templates::none(), None);
    };
    match Templates::discover(dir) {
        Ok(found) => (found, None),
        Err(error) => (
            Templates::none(),
            Some(format!(
                "{error}; this session has no templates until that is fixed"
            )),
        ),
    }
}

/// The agent's skills, and what went wrong finding them.
///
/// The same three states as [`templates`], for the same reason and with the same
/// consequence if they are collapsed: no directory configured is silence, a
/// directory that reads is the set, and a path that will not walk is an empty set
/// **and a sentence** carrying io-harness's own message. A palette that quietly
/// lists no skills looks exactly like one that was never pointed at any.
///
/// Discovered once, when the session starts — the same walk the contract's
/// `skills` field will do for the run, done here so the palette can list what the
/// agent will be told about without walking the directory on every keystroke.
pub fn skills(dir: Option<&std::path::Path>) -> (io_harness::Skills, Option<String>) {
    let Some(dir) = dir else {
        return (io_harness::Skills::none(), None);
    };
    match io_harness::Skills::discover(dir) {
        Ok(found) => (found, None),
        Err(error) => (
            io_harness::Skills::none(),
            Some(format!(
                "{error}; this session lists no skills until that is fixed"
            )),
        ),
    }
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
    /// Commit the whole session state into the scrollback.
    ///
    /// **Into the scrollback and never into a pane**, which is the same answer
    /// [`Action::Expand`] and [`Action::Transcript`] give to "show me more": the
    /// viewport is four rows and cannot grow, and the terminal's own search,
    /// selection and copy-mode already work on everything committed above it.
    /// Every field of it is a value io-harness supplied — the policy layers, the
    /// backend that actually answered, the draw against the tree's ceiling, the
    /// budgets in force, the context fill, the servers that came up — so what is
    /// committed is the state io-harness is in and not io-cli's account of it.
    Status,
    /// Put something on the system clipboard over OSC 52.
    Copy(Copied),
    /// Put the whole conversation back into the scrollback.
    Transcript,
    /// Clear the screen and start a new conversation.
    ///
    /// A new session id, no prior turn sent to the model, and the run-scoped
    /// status fields back to what they were before anything ran. The
    /// conversation it ends is not destroyed — it is in io-harness's store and
    /// still listed by `/resume`.
    Clear,
    /// Open the fleet view, or close it.
    Fleet,
    /// Draw an image this session has attached, by the number its marker carries.
    ///
    /// `None` means the operator typed `/image` with nothing after it, or with
    /// something that is not a number: the answer to both is the same sentence
    /// naming what there is.
    Image(Option<usize>),
    /// Run later turns contained, stop doing so, or say which it is now.
    ///
    /// `None` is a question and never a toggle: the two modes differ in what a
    /// turn can do — fan out, or be steered — and a switch that guessed which
    /// one the operator meant would be wrong half the time.
    Contain(Option<bool>),
    /// Make later turns propose a plan before they work, stop doing so, or say
    /// which it is now.
    ///
    /// `None` is a question for the same reason [`Action::Contain`]'s is, and a
    /// sharper one: while the planning phase is on io-harness denies every write
    /// and every exec until a proposal is approved, so a blind toggle is a coin
    /// flip between an agent that works and one that waits.
    Plan(Option<bool>),
    /// Browse every setting, or write one.
    ///
    /// `None` opens the surface. `Some` is a key and the TOML source of its
    /// value, which is what `/config <key> <value>` means — and the write does
    /// not happen until a scope is chosen, because *which file* is half the
    /// decision and this product has three of them.
    ///
    /// The value is carried as the operator typed it rather than parsed here.
    /// io-harness decides what a value means; this crate decides which bytes go
    /// where, and a value coerced on the way through would be io-cli inventing a
    /// second opinion about a schema it does not own.
    Config(Option<(String, String)>),
    /// Show the configured MCP servers and what the session has seen of them.
    Mcp,
    /// Show the provider chain, in the order a turn tries it.
    Provider,
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
///
/// **`newline` is the second thing that can move a row, and it is not the
/// configuration file's doing.** `Shift+Enter` is a key only on a terminal that
/// speaks the Kitty keyboard protocol; elsewhere it is the byte `Enter` sends,
/// so naming it in a row that says "new line" documents a keystroke that submits
/// the prompt. The decision is [`Newline::of`] and it arrives here as a value —
/// this function does not ask which terminal it is drawing for, because a table
/// that answered that for itself is how two surfaces end up naming two different
/// keys in one session.
pub fn rows(keys: &Keys, newline: Newline) -> Vec<(String, String)> {
    let defaults = Keys::default();
    KEYS.iter()
        .map(|(name, what)| {
            // Joined on the shipped spelling, which is `Newline::of(true)`'s own
            // key column rather than a literal repeated here — the same shape as
            // the `Action` join below, and asserted by `tests/keyboard.rs` for the
            // same reason.
            if *name == Newline::of(true).key {
                return (newline.key.to_string(), newline.what.to_string());
            }
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
///
/// This is where the newline naming enters the `/help` surface — one
/// [`Newline::here`] for the whole table, read from what the session recorded
/// when it attached. It is here rather than inside [`help`] or [`rows`] so that
/// everything downstream of it is a pure function of the value, which is the
/// property `tests/keyboard.rs` drives both ways.
pub fn parse(input: &str, keys: &Keys, theme: &Theme) -> Action {
    match input.split_whitespace().next().unwrap_or("help") {
        "help" | "?" => Action::Print(help(keys, theme, Newline::here())),
        // **`/exit` and nothing else.** `/quit` was the listed spelling through
        // 0.10.0 and `/exit` the unlisted alias, which is two commands doing one
        // thing and a palette with a row for each. One name, and `q` for the
        // hands that have typed it in every other tool.
        "exit" | "q" => Action::Quit,
        "setup" => Action::Setup,
        "theme" => Action::Theme,
        "model" => Action::Model,
        // `/resume` and `/continue` mean the same thing. Both words are in the
        // field's vocabulary and a reader who has used another agent will type
        // whichever one that agent taught them.
        "resume" | "continue" => Action::Resume,
        "fork" | "branch" => Action::Fork,
        // `/config` alone browses; `/config <key> <value>` writes. The value is
        // everything after the key rather than the next word, so an array or an
        // inline table can be typed whole — `allowed_domains = ["a", "b"]` is one
        // value with a space in it, and splitting on whitespace would take half.
        "mcp" | "servers" => Action::Mcp,
        "provider" | "providers" => Action::Provider,
        "config" | "settings" => {
            let mut rest = input.split_whitespace().skip(1);
            match rest.next() {
                Some(key) => {
                    let value = input
                        .split_once(key)
                        .map(|(_, after)| after.trim())
                        .unwrap_or("")
                        .to_string();
                    if value.is_empty() {
                        // A key with no value is a question, not a write. Naming
                        // the key back is what tells the operator the surface
                        // knows it, without touching a file on a half-typed line.
                        Action::Config(Some((key.to_string(), String::new())))
                    } else {
                        Action::Config(Some((key.to_string(), value)))
                    }
                }
                None => Action::Config(None),
            }
        }
        // `on` / `off` / nothing. Nothing REPORTS rather than toggles, because
        // this switch changes what a turn is — a blind toggle would be a coin
        // flip between a turn that can be steered and one that can fan out.
        "contain" | "containment" => match input.split_whitespace().nth(1) {
            Some("on") | Some("yes") => Action::Contain(Some(true)),
            Some("off") | Some("no") => Action::Contain(Some(false)),
            _ => Action::Contain(None),
        },
        // The same three answers, and nothing REPORTS here too. Turning the
        // planning phase on stops every turn until a proposal is approved, which
        // is a bigger thing to do by accident than containment is.
        "plan" | "planning" => match input.split_whitespace().nth(1) {
            Some("on") | Some("yes") => Action::Plan(Some(true)),
            Some("off") | Some("no") => Action::Plan(Some(false)),
            _ => Action::Plan(None),
        },
        "fleet" | "agents" => Action::Fleet,
        // **`/attach` is gone, and 0.13.1 is where it went.** A picture is
        // attached by dropping it on the prompt or pasting it — which is what an
        // operator already does in every other window they talk to a model in —
        // and a command was a thing they had to be told about first. The word is
        // not kept as an alias: an unknown command is answered by the same
        // sentence every other unknown one is, and a command list with a hidden
        // survivor in it is a list nobody can trust.
        // **The picture, on demand.** An attachment is `[Image #1]` on the prompt
        // and `[Image #1]` in the transcript — twenty rows of somebody's
        // screenshot in the middle of a conversation is not what a reader wants
        // by default — so this is how the picture is seen again. It draws a fresh
        // copy at the bottom rather than opening the old line: a committed row
        // belongs to the terminal's scrollback, and nothing in this process can
        // reach back into it.
        "image" | "images" => Action::Image(
            input
                .split_whitespace()
                .nth(1)
                .map(|word| word.trim_start_matches('#'))
                .and_then(|word| word.parse().ok()),
        ),
        "expand" => Action::Expand,
        // One spelling. `/state` is not taken as an alias: the word on the
        // status line, in the README and in this table is `status`, and a second
        // name for a surface nobody has typed yet is a name to keep working
        // forever in exchange for nothing.
        "status" => Action::Status,
        // `/clear` and `/new` mean the same thing, for the reason `/resume` and
        // `/continue` do: both words are in the field's vocabulary and a reader
        // arrives having been taught one of them by another agent.
        "clear" | "new" => Action::Clear,
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
///
/// It takes the newline naming rather than deciding it for the same reason
/// [`rows`] does, and passing it down is what lets a test render this table for
/// both kinds of terminal from a machine that is only one of them.
pub fn help(keys: &Keys, theme: &Theme, newline: Newline) -> Vec<Line<'static>> {
    let bound = rows(keys, newline);
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
