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
//! eight rows and cannot grow, so `/expand` and `Ctrl+T` do not open a pane — they
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
    (
        "/resume",
        "reopen an earlier session and answer whatever its last run is waiting on",
    ),
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
    // `/status` says how full the window is; this says what is in it. The two are
    // one keystroke apart on purpose — the percentage is what makes an operator
    // ask the question this answers.
    (
        "/context",
        "what is in the model's window, read from the request that carried the turn",
    ),
    (
        "/steer",
        "send what is queued into the turn that is already running",
    ),
    // Beside `/steer` because it is the same kind of word: something an operator
    // says *to* a turn rather than about one, and the only other command whose
    // effect is decided by whether anything is running.
    (
        "/compact",
        "fold this conversation into a summary, at the next step",
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
    // Beside `/config` because they are the other two surfaces that write a file
    // the operator keeps, and because the scope question is the same one: three
    // files, and *which* one is half of every decision made here.
    (
        "/remember",
        "remember a line of guidance, in the scope you choose",
    ),
    (
        "/memory",
        "what io remembers: the instruction files, and the agent's own notes",
    ),
    // Beside `/memory` because the two answer the halves of one question: that
    // one says what io was *told*, this one says what it was *taught*. A skill is
    // the only thing in either list io-cli did not write and does not parse — the
    // rows come from io-harness's own discovery — so an operator who cannot see
    // which ones are on, and out of which file, has no way to account for a turn
    // that read one.
    (
        "/skills",
        "every skill, shipped or yours: what it is for, whether it is on, and its file",
    ),
    (
        "/mcp",
        "the MCP servers configured, and what this session has seen of each",
    ),
    (
        "/provider",
        "the providers configured, in the order a turn tries them",
    ),
    // Beside `/mcp` and `/provider` because it is the third of the same kind: a
    // surface that reads a declaration out of the configuration file and can
    // take one back out of it. A bundle is the widest of the three — it can hand
    // over skills, templates, agents, servers, hooks and policy in one directory —
    // which is the argument for it being visible at all, and the argument for it
    // being under "configure" rather than "inspect".
    //
    // **What it does not do is add one, and the description says "loaded" rather
    // than implying otherwise.** Declaring a bundle means naming a directory,
    // which is a path an operator types far more comfortably into their own file
    // than into a picker; removing one means finding the right entry among three
    // scope files, which is exactly the part a surface can do better than a
    // person. So this does the second and not the first.
    (
        "/plugin",
        "the capability bundles loaded, what each contributed, and the ones that failed",
    ),
    (
        "/import",
        "bring instructions, MCP servers, skills and a model across from another agent",
    ),
    (
        "/profile",
        "switch to a named profile from the configuration, for this session",
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
    // The two halves of one question, and they are two commands because they are
    // two questions. `/cost` says what the work cost; `/stats` says whether it
    // worked. Every agent that has both keeps them apart, and a single screen
    // carrying thirteen sections would be one nobody reads to the end.
    (
        "/cost",
        "commit what this run, this session and this install have spent",
    ),
    (
        "/stats",
        "commit how the runs have gone: outcomes, first-try, gates, latency",
    ),
];

/// What an operator is doing when they reach for a command.
///
/// **Grouped by the operator's intent rather than by which part of the harness
/// answers**, because the second is an implementation detail and the first is
/// the only thing somebody scanning a list of twenty-six is holding in their
/// head.
///
/// Four groups and none longer than ten, which is the bound `tests/commands.rs`
/// asserts. A flat list of twenty-six is a list nobody reads; 0.16.0 is the
/// release that grouped them, at twenty, and every release since has added to a
/// group rather than to a list. **The count in this paragraph is the one number
/// here that goes stale on its own** — it said twenty through 0.17.0, which had
/// twenty-three — so it is written out rather than left as "a few".
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum Group {
    /// The conversation itself: start one, leave one, come back to one.
    Session,
    /// What the next turn will do.
    Turn,
    /// Show me something. Everything here commits upward into the scrollback or
    /// opens a list; none of it changes what a turn does.
    Inspect,
    /// Change the configuration file.
    ///
    /// **A surface that lists the file before it writes to it is still here**, and
    /// `/mcp` and `/provider` are the two — they were under `Inspect` through
    /// 0.18.0 on the strength of their first screen. Both add, edit, disable and
    /// remove entries, so an operator who opened one to look is one keystroke from
    /// changing what the next turn talks to; that is the sentence `Inspect`
    /// promises it will never say.
    Configure,
}

impl Group {
    /// The heading this group draws under.
    pub fn title(self) -> &'static str {
        match self {
            Group::Session => "the session",
            Group::Turn => "this turn",
            Group::Inspect => "inspect",
            Group::Configure => "configure",
        }
    }

    /// Every group, in the order they are shown.
    ///
    /// Session first because it is what an operator reaches for when they are
    /// lost; configure last because it is the one that writes.
    pub fn all() -> [Group; 4] {
        [
            Group::Session,
            Group::Turn,
            Group::Inspect,
            Group::Configure,
        ]
    }
}

/// Which group each command belongs to.
///
/// A second table rather than a third column on [`COMMANDS`], and the reason is
/// the gate: `tests/commands.rs` asserts every name in `COMMANDS` appears here
/// **exactly once**, so a command added without a group fails a named test
/// rather than quietly appearing in no menu. A third column would make that
/// unrepresentable, which sounds better and is worse — the failure would be a
/// compile error in a file nobody was editing, rather than a test that says
/// which command has no home.
pub const GROUPS: &[(Group, &[&str])] = &[
    (
        Group::Session,
        &["/clear", "/resume", "/fork", "/setup", "/exit"],
    ),
    // **`/image`, `/copy` and `/copy diff` moved here in 0.22.0, and it is a
    // correction rather than a way of making room** — the same sentence 0.19.0
    // wrote when it moved `/mcp` and `/provider`, and it is worth being able to
    // say it twice. All three act on the turn that just finished: `/image` draws
    // an image attached to it, `/copy` puts its answer or its diff on the
    // clipboard. None of them asks the store a question, which is what `Inspect`
    // means. They were filed there because they *show* something, and showing is
    // not the same as inspecting.
    //
    // `/cost` and `/stats` are what made the misfiling worth correcting: `Inspect`
    // stood at nine of a bound of ten and both new commands belong in it, so the
    // choice was between re-filing three commands that were in the wrong group and
    // filing two more that would have been. The bound exists to stop the grouped
    // menu becoming the flat list it replaced, and answering it by weakening it
    // would have given up the thing it was protecting.
    (
        Group::Turn,
        &[
            "/model",
            "/contain",
            "/plan",
            "/profile",
            "/steer",
            "/compact",
            "/image",
            "/copy",
            "/copy diff",
        ],
    ),
    (
        Group::Inspect,
        &[
            "/help", "/status", "/context", "/expand", "/fleet", "/skills", "/cost", "/stats",
        ],
    ),
    // **`/mcp` and `/provider` moved here in 0.19.0, and it is a correction rather
    // than a way of making room.** Both were grouped by the screen they open, and
    // both go on from that screen to add, edit and remove entries in the
    // configuration file — which is what this group means and what `Inspect`
    // promises it does not do. It takes `Inspect` to eight and `/skills` puts it
    // back to nine, and the order of those two sentences is worth keeping: the
    // room was made by filing two commands where they belong, not by filing one
    // where there was space. A reader who finds `/mcp` under "configure" should be
    // able to work out why from the panel it opens, without knowing what the
    // release before it looked like.
    (
        Group::Configure,
        &[
            "/config",
            "/theme",
            "/remember",
            "/memory",
            "/mcp",
            "/provider",
            "/plugin",
            // **`/import` writes files, so `Configure` is the only group it can
            // be in** — and `Inspect` is full at nine besides. It is last because
            // it is the one command here an operator uses once: the others are
            // returned to for the life of the install.
            "/import",
        ],
    ),
];

/// The group a command belongs to, or `None` for a name that is in none.
pub fn group_of(name: &str) -> Option<Group> {
    GROUPS
        .iter()
        .find(|(_, names)| names.contains(&name))
        .map(|(group, _)| *group)
}

/// Every command, gathered under its group, in [`Group::all`] order.
///
/// Within a group the order is [`GROUPS`]' own, which is chosen rather than
/// alphabetical: `/clear` before `/exit` because one is what you reach for far
/// more often than the other.
pub fn grouped() -> Vec<(Group, Vec<(&'static str, &'static str)>)> {
    Group::all()
        .into_iter()
        .map(|group| {
            let names = GROUPS
                .iter()
                .find(|(g, _)| *g == group)
                .map(|(_, names)| *names)
                .unwrap_or(&[]);
            let rows = names
                .iter()
                .filter_map(|name| {
                    COMMANDS
                        .iter()
                        .find(|(command, _)| command == name)
                        .map(|(command, what)| (*command, *what))
                })
                .collect();
            (group, rows)
        })
        .collect()
}

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

/// What a row that RUNS when chosen is marked with.
///
/// Three marks, one per kind, and all three are ASCII so they survive the ASCII
/// glyph set unchanged — `NO_COLOR` and `--plain` likewise, because a mark is
/// text rather than a colour. They are the same width as each other so no
/// column shifts between kinds.
///
/// A command runs; a template and a skill write text into the prompt and stop.
/// That difference is what the marks carry, and before 0.16.0 it was carried
/// only in the detail column — which the picker drops first on a narrow
/// terminal, so the distinction disappeared exactly where a row is hardest to
/// read. A command carried no mark at all.
pub const COMMAND_MARK: &str = ":";

/// What a row that fills the prompt from a configured template is marked with.
pub const TEMPLATE_MARK: &str = "+";

/// What a row that names one of the agent's own skills is marked with.
pub const SKILL_MARK: &str = "*";

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
pub fn palette(templates: &Templates, skills: &[crate::skillview::Listed]) -> Vec<Row> {
    entries(templates, skills)
        .into_iter()
        .map(|entry| entry.row)
        .collect()
}

/// One row of the palette, and what choosing it stands for.
///
/// `chosen` is `None` for a group heading, which is a row nobody can pick.
struct Entry {
    row: Row,
    chosen: Option<Chosen>,
}

/// The palette, built once.
///
/// **[`palette`] and [`palette_pick`] both read this**, which is what keeps the
/// index the picker hands back addressing the row it was drawn from. Before
/// 0.16.0 the two walked the inventories separately and agreed because they were
/// written next to each other; a grouped list with headings in it makes that
/// agreement impossible to keep by inspection, because the rows and the things
/// they stand for are no longer the same length.
fn entries(templates: &Templates, skills: &[crate::skillview::Listed]) -> Vec<Entry> {
    let mut out: Vec<Entry> = Vec::new();

    // Commands, under their group headings. The headings are drawn while the
    // list is browsed and dropped the moment anything is typed — see
    // `Row::heading`.
    for (group, rows) in grouped() {
        if rows.is_empty() {
            continue;
        }
        out.push(Entry {
            row: Row::heading(group.title()),
            chosen: None,
        });
        for (name, what) in rows {
            out.push(Entry {
                // The label drops the leading slash for the reason it always
                // has: `crate::fuzzy` ranks an exact name above a prefix above a
                // scatter, and with the slash on, every label begins with the
                // same character so both top tiers are unreachable.
                row: Row::marked(COMMAND_MARK, name.strip_prefix('/').unwrap_or(name), what),
                chosen: Some(Chosen::Command(name)),
            });
        }
    }

    let templates_rows: Vec<_> = templates.iter().collect();
    if !templates_rows.is_empty() {
        out.push(Entry {
            row: Row::heading("prompt templates"),
            chosen: None,
        });
        for template in templates_rows {
            out.push(Entry {
                row: Row::marked(
                    TEMPLATE_MARK,
                    template.name.clone(),
                    format!("{TEMPLATE}{}", template.description),
                ),
                chosen: Some(Chosen::Template(template.name.clone())),
            });
        }
    }

    let skill_rows: Vec<_> = skills.iter().collect();
    if !skill_rows.is_empty() {
        out.push(Entry {
            row: Row::heading("skills"),
            chosen: None,
        });
        for skill in skill_rows {
            out.push(Entry {
                row: Row::marked(
                    SKILL_MARK,
                    skill.name.clone(),
                    // **The bundle is named in the detail, and the name carries
                    // the real signal.** A narrow terminal drops the detail column
                    // first, which is the 0.16.0 lesson about marks — so the
                    // origin must never be the only place the provenance lives.
                    // It is not: a bundle's skill is listed under the namespaced
                    // `<id>__<name>` the model actually addresses, and that prefix
                    // is in the label, which is the column that survives and the
                    // one `crate::fuzzy` ranks.
                    match &skill.origin {
                        crate::skillview::Origin::Bundle(id) => {
                            format!("{SKILL}{} · from the {id} bundle", skill.description)
                        }
                        _ => format!("{SKILL}{}", skill.description),
                    },
                ),
                chosen: Some(Chosen::Skill(skill.name.clone())),
            });
        }
    }

    out
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
    skills: &[crate::skillview::Listed],
    index: usize,
) -> Option<Chosen> {
    entries(templates, skills)
        .into_iter()
        .nth(index)
        .and_then(|entry| entry.chosen)
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
/// **Bundles are why this is no longer an `io_harness::Skills`.** A capability
/// bundle contributes its own directory, the harness merges every one of them
/// before the model sees the catalogue, and `Skills` has a private field with no
/// public constructor and a `pub(crate)` `merged` — so io-cli cannot build the
/// value that would describe what the run is actually offered. It carries the
/// rows instead, which it can build, and which carry the origin the palette needs
/// anyway.
///
/// The walk itself belongs to [`crate::skillview`] and is not repeated here.
/// `/skills` and the palette must never disagree about what the model was
/// offered, and the cheapest way to guarantee that is for both to come out of one
/// function. This one drops the disabled rows, because a disabled skill is
/// precisely one the model is not offered and the palette exists to say what it
/// is.
pub fn skills(
    home: &std::path::Path,
    dir: Option<&std::path::Path>,
    bundles: &[(String, std::path::PathBuf)],
) -> (Vec<crate::skillview::Listed>, Option<String>) {
    let view = match dir {
        Some(dir) => crate::skillview::view(home, dir, bundles),
        // Still the bundles. A home with no `skills/` of its own is the ordinary
        // fresh install, and it is exactly when everything on offer came from a
        // bundle.
        None => crate::skillview::view_of_bundles(bundles),
    };
    // **Both failures, not just the operator's own directory.**
    //
    // A bundle whose declared skills directory is not on disk ends every turn of
    // the session before the first completion, and this is the only call made at
    // startup — so dropping `bundles_failed` here meant the one class of failure
    // that kills the session said nothing at the one moment it was cheapest to
    // say it, leaving the operator to guess that `/skills` or `/plugin` is where
    // the reason lives.
    let mut sentences: Vec<String> = Vec::new();
    if let Some(error) = view.failed {
        sentences.push(format!(
            "{error}; this session lists no skills until that is fixed"
        ));
    }
    for (id, error) in view.bundles_failed {
        sentences.push(format!(
            "the {id} bundle contributes no skills: {error}. Every turn of this session ends on \
             that error until the directory exists or the bundle is removed with `/plugin`."
        ));
    }
    let sentence = (!sentences.is_empty()).then(|| sentences.join(" "));
    (
        view.skills.into_iter().filter(|row| row.enabled).collect(),
        sentence,
    )
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
    /// Open the picker over the sessions the store holds, and answer whatever
    /// the chosen one's last run stopped on.
    ///
    /// **The second half is 0.23.0's and it is what the word now means.** Until
    /// then this reopened a session and nothing more — a question the agent had
    /// asked, a plan it had proposed and a call that never finished all stayed in
    /// the store while the interface offered a fresh prompt. See
    /// [`crate::resume`] for the four kinds of pause and for the one that cannot
    /// be answered at all.
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
    /// viewport is eight rows and cannot grow, and the terminal's own search,
    /// selection and copy-mode already work on everything committed above it.
    /// Every field of it is a value io-harness supplied — the policy layers, the
    /// backend that actually answered, the draw against the tree's ceiling, the
    /// budgets in force, the context fill, the servers that came up — so what is
    /// committed is the state io-harness is in and not io-cli's account of it.
    Status,
    /// Commit what has been spent into the scrollback.
    ///
    /// The sibling of [`Action::Status`] and drawn the same way, for the same
    /// reason: this is a page a reader wants to keep beside the turns it accounts
    /// for, and the terminal's own scrollback is where it keeps things. What
    /// separates the two is the question. `/status` says what the session *is*
    /// — the policy, the caps, the budgets in force; this says what it has *cost*,
    /// which is a different fact about the same run and one that only ever climbs.
    Cost,
    /// Commit how the runs have gone into the scrollback.
    ///
    /// Deliberately not folded into [`Action::Cost`] under one name. What a run
    /// cost and whether it worked are two questions, and the answer to the second
    /// — outcomes, the first-try share, gate failures by phase, latency — is
    /// eight sections that have no money in them at all. One page carrying both
    /// would be thirteen sections deep and a reader looking for either would
    /// scroll past the other.
    Stats,
    /// Send what is queued into the turn that is already running.
    ///
    /// io-harness delivers it at the next step boundary, so the step in flight
    /// finishes whole and the agent reads the correction before it chooses what
    /// to do next. Deliberately a word rather than a default: a delivered steer
    /// emits no event this interface can render, so a line sent automatically
    /// would leave the screen with no echo at all — and `Steer::say` has no
    /// undo, which would make every note typed to oneself mid-turn an
    /// instruction to the agent.
    Steer,
    /// Fold the conversation into a summary and carry on.
    ///
    /// **Two triggers behind one word, chosen by whether a turn is running.** A
    /// running turn is asked through `Steer::fold`, which lands at its next step
    /// boundary; an idle prompt has no turn to ask, so the request rides the next
    /// turn's contract as `TaskContract::fold_now` and folds at that turn's first
    /// step. The driver answers the first case before [`parse`] is ever called —
    /// the same shape [`Action::Steer`] has — so this action is what an idle
    /// prompt means.
    ///
    /// **What it must never do is report the fold.** io-harness documents four
    /// conditions under which an accepted request folds nothing, and the request
    /// is spent under all of them, so the only thing that says a fold happened is
    /// `EventKind::Compacted`. [`crate::compact`] is where that rule lives.
    Compact,
    /// Commit what the model's window actually held, section by section.
    ///
    /// Read off the request that carried the last turn and never reconstructed:
    /// io-harness enumerates no context window, its prompt composer is private
    /// and the event announcing a composed prompt carries a byte count with no
    /// text. What it does hand the caller is the `CompletionRequest` itself, so
    /// the catalogue on this page includes tools io-cli never registered —
    /// because it is the catalogue the model was given rather than the one this
    /// crate believes it asked for.
    Context,
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
    /// `None` is a question and never a toggle: the two modes differ in whether
    /// a turn can fan out — steering is on both since 0.17.0 — and a switch that
    /// guessed which one the operator meant would be wrong half the time.
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
    /// Append one line of guidance to a memory file.
    ///
    /// The line as the operator typed it, and **no scope**: which of the three
    /// files it goes into is the other half of the decision and it is asked for
    /// afterwards, in a picker, exactly as [`Action::Config`]'s write is. The
    /// three differ only in who else reads them — a repository, a checkout, or
    /// this machine — so guessing one is guessing whether a private note is
    /// about to be committed.
    ///
    /// An empty string is the operator typing the word with nothing after it.
    /// It is carried rather than refused here so the driver can say what to
    /// type instead of opening a picker over a file it would write nothing to.
    Remember(String),
    /// The memory page: the instruction files, and the agent's own notes.
    ///
    /// **Two lists on one page, and they are two different memories.** The
    /// first is [`crate::memory`] — markdown a person writes, which io-harness
    /// reads as an instruction at the start of every run. The second is
    /// [`crate::recall`] — notes the *agent* wrote for itself, which the harness
    /// carries into every later prompt over the same workspace. One page,
    /// because "what does it already know" is one question; two lists, because
    /// the answer has two authors and only one of them is the operator.
    Memory,
    /// Show the configured MCP servers and what the session has seen of them.
    Mcp,
    /// List every skill — the five io-cli ships and the operator's own — with
    /// what it is for, whose it is, whether it is on, and the file it lives in.
    ///
    /// **The enabled set is read through `io_harness::Skills::discover`**, the
    /// same call the run makes, so what this lists is what the model is actually
    /// offered rather than io-cli's account of a directory. The disabled set is
    /// read by io-cli itself, because `skills/disabled/` holds no `SKILL.md` and
    /// is therefore invisible to that call by design — which is the whole
    /// mechanism, not a gap in it.
    Skills,
    /// Show the provider chain, in the order a turn tries it.
    Provider,
    /// List the named profiles, and switch to one for this session.
    Profile,
    /// List the capability bundles a `[[plugin]]` entry declared: what each one
    /// contributed, and what each dropped bundle failed on.
    ///
    /// **What can honestly be shown is bounded by what io-harness makes public,
    /// and the surface says so rather than implying more.** A bundle's agents,
    /// MCP servers and policy layers are all readable and are listed by the names
    /// io-harness namespaced them to. Its **hooks are not**: there is no
    /// `Plugin::hooks()` and `Hook` is `pub(crate)`, so the word `hooks` from
    /// `Plugin::contributions()` is the entire signal available — the bundle
    /// declared some, and io-cli cannot say how many or what they run.
    ///
    /// A dropped bundle carries io-harness's own sentence, re-worded by nobody.
    /// That matters most for the one an operator will actually hit: a bundle
    /// declared in the project file that contributes `[[hook]]` or `[[mcp]]` is
    /// refused **whole**, and the sentence is what names the two files it could
    /// move to instead.
    Plugin,
    /// Bring an operator's work across from another agent they already use.
    ///
    /// **Everything is shown before anything is written, and declining is the
    /// default.** The surface lists one item per thing found — an instructions
    /// file, an MCP server, a skill, a model — with where it came from and where
    /// it would go, and writes only what the operator accepted. A cancelled
    /// import is not a partial one.
    ///
    /// **An allowlist is read and deliberately not translated.** Another tool
    /// spells a permission as a command and its arguments; io-harness's
    /// `Act::Exec` matches a binary name and nothing else. What can be said
    /// honestly is what was found and that it does not carry over, so that is
    /// what is said — a boundary half imported is worse than one left alone.
    ///
    /// **No credential is read, at any point.** A server's environment values are
    /// discarded without being constructed and the variable *name* is what gets
    /// recorded. A key that was never on disk must not reach disk as a side
    /// effect of trying a new program.
    Import,
}

/// What `/copy` was asked for.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Copied {
    /// The last thing the agent said.
    Answer,
    /// Every change the run made, as one patch.
    Diff,
}

// ---------------------------------------------------------------------------
// The memory page, and the scope question `/remember` asks
// ---------------------------------------------------------------------------

/// What marks a note an operator pinned.
///
/// **Four marks on this page and every one of them is ASCII by rule**, the same
/// rule [`COMMAND_MARK`] and its two neighbours are held to: a mark is text
/// rather than a colour, so it survives `NO_COLOR`, `--plain` and the ASCII
/// glyph set unchanged, and `tests/glyphs.rs` sweeps the rendered page for
/// anything that is not.
///
/// They are all one cell wide, so no column shifts between rows, and they are
/// paired on one axis: `*` and `+` say **yes**, `-` says **no**, and the reader
/// learns one thing rather than four. A pin is what stops a run overwriting a
/// correction a person made and what stops the caps dropping it — see
/// [`crate::recall::pin`] — so it is the state worth the mark column on an
/// entry row.
pub const PINNED_MARK: &str = "*";

/// What marks a note nothing has pinned: a run may overwrite it, and the caps
/// may drop it.
pub const LOOSE_MARK: &str = "-";

/// What marks an instruction file io-harness is **actually reading**.
///
/// Read back from what the harness composed rather than from what was
/// configured — see [`crate::memory::view`] — so a project `[instructions]
/// files` that replaced the list shows here as [`UNREAD_MARK`] instead of being
/// argued away.
pub const READ_MARK: &str = "+";

/// What marks an instruction file the harness is **not** reading.
///
/// The case this page exists for. A file that is there and is not read looks,
/// from everywhere else in this product, exactly like one that is read: nothing
/// warns, and a missing or skipped instruction file is passed over in silence
/// (`io-harness-0.69.0/src/config.rs:1886`, `:1892`).
pub const UNREAD_MARK: &str = "-";

/// What one row of the memory page stands for.
///
/// The parallel vector [`memory_page`] returns beside its rows, in the same
/// order and of the same length. It exists for the reason every other picker in
/// this product carries one: [`crate::picker::Outcome::Chosen`] indexes the
/// **caller's unfiltered rows**, and reading a key back off a rendered label
/// would be matching on a string the fitter may have shortened.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Held {
    /// A group heading. Nothing to act on, and nothing the picker can put the
    /// marker on — see [`Row::heading`].
    Nothing,
    /// One of the instruction files, whole, so choosing it can say what it is
    /// without the page being rebuilt to find out.
    File(crate::memory::Instruction),
    /// One of the agent's own notes: which of the two buckets holds it, the key
    /// it is recalled by, and whether an operator has pinned it.
    ///
    /// The bucket is carried because [`crate::recall::pin`] and
    /// [`crate::recall::forget`] both take one, and a key can exist in both: a
    /// verb applied to the wrong bucket either does nothing or acts on a
    /// same-named note the operator was not looking at.
    Note {
        scope: crate::recall::Scope,
        key: String,
        pinned: bool,
    },
}

/// The memory page's rows, and what each of them stands for.
///
/// **Built in one pass so the two can never be different lengths.** This is the
/// shape the palette's own `entries` already uses, and for the same reason it
/// was given one: a list with headings interleaved makes "the n-th row is the
/// n-th thing" false, and two functions that walked the same inventories
/// separately would agree only for as long as somebody kept checking.
///
/// The two lists are kept distinguishable by a heading each, and neither is
/// dropped when it is empty: an operator whose agent has learnt nothing is
/// entitled to see the heading over the absence rather than a page that quietly
/// looks like the other list is all there is.
///
/// `draws_cut` comes from [`crate::recall::View::draws_cut`]. When it is set the
/// draw counts are a **lower bound** — the scan stopped at
/// [`crate::recall::MAX_RUNS_SCANNED`] runs — and the detail says so in words
/// rather than printing a number that reads as exact.
pub fn memory_page(
    files: &[crate::memory::Instruction],
    entries: &[crate::recall::Remembered],
    draws_cut: bool,
    glyphs: &crate::glyphs::Glyphs,
) -> (Vec<Row>, Vec<Held>) {
    let mut rows = Vec::new();
    let mut held = Vec::new();
    let separator = glyphs.separator;

    rows.push(Row::heading("instruction files"));
    held.push(Held::Nothing);
    for file in files {
        rows.push(Row::marked(
            if file.read { READ_MARK } else { UNREAD_MARK },
            // The file's own name, which is what an operator knows it by and
            // what makes the three tell each other apart. The path rides in the
            // detail, where a narrow terminal may drop it — the name is the part
            // that must survive.
            crate::memory::file_name(file.scope),
            // **The state leads.** The detail is fitted from the head, so what
            // is drawn on the narrowest terminal that still has room for one is
            // the answer to the only question this list is asked.
            format!(
                "{}{separator}{}{separator}{}",
                state_of(file),
                crate::configure::Decided::File {
                    scope: file.scope,
                    path: file.path.clone(),
                }
                .word(),
                file.path.display(),
            ),
        ));
        held.push(Held::File(file.clone()));
    }

    rows.push(Row::heading("what the agent remembers"));
    held.push(Held::Nothing);
    for entry in entries {
        rows.push(Row::marked(
            if entry.pinned {
                PINNED_MARK
            } else {
                LOOSE_MARK
            },
            entry.key.clone(),
            format!(
                "{}{separator}{}{separator}run {} step {}{separator}{}",
                // The scope leads for the reason the file state does: "is this
                // true here, or true everywhere" is the only question the two
                // buckets exist to answer, and it is the first thing to go if
                // the detail is cut from the tail.
                entry.scope.label(),
                entry.kind,
                entry.run_id,
                entry.step,
                draws(entry.draws, draws_cut),
            ),
        ));
        held.push(Held::Note {
            scope: entry.scope,
            key: entry.key.clone(),
            pinned: entry.pinned,
        });
    }

    (rows, held)
}

/// How many runs have drawn on a note, and whether that is a count or a floor.
///
/// **Never a bare number when the scan was cut.** `n draws` and `n draws or
/// more` are different claims, and the second is the true one whenever
/// [`crate::recall::View::draws_cut`] is set — a store with more than
/// [`crate::recall::MAX_RUNS_SCANNED`] runs in it has evidence this page did not
/// look at. Reading a floor as a count is what makes an ordinary eviction look
/// like a note nothing ever used.
fn draws(count: usize, cut: bool) -> String {
    if cut {
        format!("{count} draws or more")
    } else {
        format!("{count} draws")
    }
}

/// What one instruction file's row says about itself, in three words or so.
///
/// The middle case is the whole reason this page exists, and it is stated rather
/// than implied: **a file that is there and is not being read**. Nothing else in
/// this product says so, because io-harness skips a file it will not read
/// without a word.
fn state_of(file: &crate::memory::Instruction) -> String {
    match (file.exists, file.read) {
        (false, _) => "not written yet".to_string(),
        (true, true) => format!("read{}", plural(file.lines)),
        (true, false) => format!("NOT read{}", plural(file.lines)),
    }
}

/// `, 12 lines` — or nothing at all for a file with none.
fn plural(lines: usize) -> String {
    match lines {
        0 => String::new(),
        1 => ", 1 line".to_string(),
        many => format!(", {many} lines"),
    }
}

/// What the memory page commits above its list.
///
/// **Committed rather than said, and the caps are why.** The bucket that
/// answered, the ceilings in force and the note about a cut scan are facts about
/// the configuration and the store: they outlive the keystroke that asked for
/// them, and [`crate::app::App::say`] would put each on the footer's one row,
/// where the next thing typed replaces it — and where only the last of the three
/// would ever be seen.
///
/// **The caps are per scope, and this says so rather than printing one number.**
/// `src/contract.rs:376-379` states it: each scope holds its own, so a run
/// drawing on both may carry up to twice `max_entries` and twice `max_chars`.
/// Quoting the single figure the contract carries tells an operator half the
/// real ceiling — and the half that makes a legitimate eviction read as a defect.
pub fn memory_notes(view: &crate::recall::View, glyphs: &crate::glyphs::Glyphs) -> Vec<String> {
    let dash = glyphs.dash;
    let each: Vec<String> = view
        .caps
        .iter()
        .map(|caps| {
            format!(
                "{} {} entries, {} chars",
                caps.scope.label(),
                caps.limits.max_entries,
                caps.limits.max_chars,
            )
        })
        .collect();

    let mut notes = vec![
        // Reported rather than assumed. The bucket is the **canonicalised** root
        // — a checkout reached through a symlink has its notes filed under the
        // resolved path — so a reader looking at an empty list can tell "the
        // agent has learnt nothing" from "you are looking at the wrong bucket".
        format!("workspace memory is keyed on {}", view.workspace),
        format!(
            "the caps are per scope {dash} {} {dash} so one run may carry {} entries \
             and {} chars in all",
            each.join(glyphs.separator),
            view.entries_ceiling(),
            view.chars_ceiling(),
        ),
    ];
    if view.draws_cut {
        notes.push(format!(
            "the draw scan stopped at {} runs, so every draw count on this page is a lower bound",
            crate::recall::MAX_RUNS_SCANNED,
        ));
    }
    notes
}

/// What choosing one instruction file's row says.
///
/// Three sentences for three states, and the middle one is the one worth
/// writing: a file that exists and is not read. It names what would make it
/// read rather than only reporting the fact, because `[instructions] files`
/// **replaces** the default list rather than adding to it
/// (`io-harness-0.69.0/src/config.rs:1879-1882`) and there is nothing on any
/// other surface to pull on.
pub fn instruction_said(
    file: &crate::memory::Instruction,
    glyphs: &crate::glyphs::Glyphs,
) -> String {
    let dash = glyphs.dash;
    let at = file.path.display();
    match (file.exists, file.read) {
        (false, _) => format!("{at} is not written yet {dash} /remember creates it"),
        (true, true) => {
            format!("{at} is read at the start of every run")
        }
        (true, false) => format!(
            "{at} exists and is NOT read {dash} nothing in `[instructions] files` reaches it, or \
             it holds only whitespace. Writing to any scope with /remember names all three.",
        ),
    }
}

/// The three memory files, as the rows that ask which one a line goes into.
///
/// **Each row says what committing it means**, because that is the entire
/// difference between the three: the file names are `IO.md`, `AGENTS.md` and
/// `AGENTS.local.md`, and an operator who has not read [`crate::memory`] cannot
/// tell from those which one goes to everybody who clones the repository. A
/// picker that offered three filenames would be asking a question whose answer
/// is only knowable somewhere else.
///
/// The consequence leads and the path follows, because the detail is fitted from
/// the head: on a narrow terminal the sentence that decides the answer is what
/// survives, and the path — which the confirmation names in full afterwards —
/// is what goes.
pub fn scope_rows(
    paths: &[(io_harness::config::Scope, std::path::PathBuf)],
    glyphs: &crate::glyphs::Glyphs,
) -> Vec<Row> {
    paths
        .iter()
        .map(|(scope, path)| {
            Row::with_detail(
                crate::memory::file_name(*scope),
                format!("{} {} {}", committing(*scope), glyphs.dash, path.display()),
            )
        })
        .collect()
}

/// What writing into one scope commits the operator to.
///
/// The same three facts [`crate::memory`] writes into the head of each file it
/// creates, said before the write rather than after it.
fn committing(scope: io_harness::config::Scope) -> &'static str {
    match scope {
        io_harness::config::Scope::User => {
            "every project on this machine, and part of no repository"
        }
        io_harness::config::Scope::Project => {
            "committed: everyone who clones this repository reads it"
        }
        io_harness::config::Scope::Local => "this checkout only, and never committed",
    }
}

/// A verb `/memory` offers on one of the agent's notes.
///
/// Two at a time and never three: pinning and unpinning are one switch, and
/// offering both would put a row on screen that does nothing.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Verb {
    /// Stop a run overwriting it, and stop the caps dropping it.
    Pin,
    /// Let both happen again — and the prerequisite for [`Verb::Forget`] on a
    /// pinned note.
    Unpin,
    /// Withdraw it, leaving a restore point.
    Forget,
}

impl Verb {
    /// What is offered on a note, given whether it is already pinned.
    pub fn of(pinned: bool) -> [Verb; 2] {
        [if pinned { Verb::Unpin } else { Verb::Pin }, Verb::Forget]
    }

    /// The word on the row.
    pub fn label(self) -> &'static str {
        match self {
            Verb::Pin => "pin",
            Verb::Unpin => "unpin",
            Verb::Forget => "forget",
        }
    }

    /// What it does, in the detail column.
    ///
    /// The pin row states the cost as well as the effect: a pinned note still
    /// counts towards both caps (`src/state/memory.rs:736-739`), so pinning
    /// everything buys writes that fail loudly rather than a bigger memory.
    /// io-harness made that choice and this surface does not soften it.
    pub fn detail(self) -> &'static str {
        match self {
            Verb::Pin => {
                "no run may overwrite it and the caps may not drop it; it still counts \
                          towards both"
            }
            Verb::Unpin => "a run may overwrite it again, and the caps may drop it",
            Verb::Forget => "withdraw it, leaving a restore point; a pinned note is refused",
        }
    }
}

/// The verbs offered on one note, as rows.
pub fn verb_rows(pinned: bool) -> Vec<Row> {
    Verb::of(pinned)
        .into_iter()
        .map(|verb| Row::with_detail(verb.label(), verb.detail()))
        .collect()
}

/// What pinning or unpinning one note is reported as.
///
/// **Two outcomes and not a `bool`.** `Store::memory_pin` answers `false` for
/// *there was no such entry*, which is not "the pin failed" — see
/// [`crate::recall::Pinned`]. A surface that read it as success would draw a pin
/// the store does not hold.
///
/// It lives here rather than at its one call site in `src/main.rs` because
/// nothing under `tests/` can link the binary: a sentence written there is one
/// no test drives and no sabotage can make fail.
pub fn pinned_said(
    key: &str,
    scope: crate::recall::Scope,
    pinned: bool,
    outcome: crate::recall::Pinned,
) -> (Tone, String) {
    match outcome {
        crate::recall::Pinned::Set if pinned => (
            Tone::Success,
            format!(
                "{key} is pinned in the {} memory; no run may overwrite it",
                scope.label()
            ),
        ),
        crate::recall::Pinned::Set => (
            Tone::Muted,
            format!(
                "{key} is unpinned in the {} memory; a run may overwrite it again",
                scope.label()
            ),
        ),
        crate::recall::Pinned::NoEntry => (
            Tone::Muted,
            format!(
                "there is no {key} in the {} memory, so nothing was changed",
                scope.label()
            ),
        ),
    }
}

/// What withdrawing one note is reported as.
///
/// **[`crate::recall::Forgotten::Refused`] is a refusal and names why, never a
/// success.** The note is pinned, so it is not a run's to withdraw and io-cli
/// asks on a run's behalf; it stands, unchanged, and it will go on being carried
/// into every later prompt. Reporting that as a removal is the same failure the
/// pin flag exists to prevent one level down — the operator believes the note is
/// gone and it is not.
///
/// [`crate::recall::Forgotten::Absent`] is a third thing again: not an error and
/// not a removal.
pub fn forgotten_said(
    key: &str,
    scope: crate::recall::Scope,
    outcome: crate::recall::Forgotten,
    glyphs: &crate::glyphs::Glyphs,
) -> (Tone, String) {
    match outcome {
        crate::recall::Forgotten::Removed { restore } => (
            Tone::Success,
            format!(
                "{key} is withdrawn from the {} memory; run {restore} holds the way back",
                scope.label(),
            ),
        ),
        crate::recall::Forgotten::Refused => (
            Tone::Refused,
            format!(
                "{key} is pinned, so it is not a run's to withdraw {} it is still there, and \
                 still carried into every later prompt. Unpin it, then forget it.",
                glyphs.dash,
            ),
        ),
        crate::recall::Forgotten::Absent => (
            Tone::Muted,
            format!(
                "there is no {key} in the {} memory, so nothing was withdrawn",
                scope.label()
            ),
        ),
    }
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
        // **The REST of the line, not its second word.** A line of guidance is a
        // sentence, and `input.split_whitespace().nth(1)` would remember one word
        // of it and drop the rest without saying so — the same trap `/config`'s
        // value arm documents, and worse here, because what is lost is prose
        // rather than a token somebody would notice missing.
        //
        // Empty when nothing followed the word. Carried rather than refused,
        // because the sentence that says what to type instead belongs to the
        // driver — see [`Action::Remember`].
        "remember" => Action::Remember(
            input
                .split_once(char::is_whitespace)
                .map(|(_, rest)| rest.trim())
                .unwrap_or("")
                .to_string(),
        ),
        // **One spelling.** `/remembered`, `/notes` and `/recall` are all words
        // somebody might reach for and none of them has been typed at this
        // prompt yet; a second name is a name to keep working forever in
        // exchange for nothing, which is the rule `/status` and `/compact`
        // already follow.
        "memory" => Action::Memory,
        "mcp" | "servers" => Action::Mcp,
        "skills" => Action::Skills,
        "provider" | "providers" => Action::Provider,
        "profile" | "profiles" => Action::Profile,
        // `/plugins` is admitted for the same reason `/servers` and `/providers`
        // are: the thing being listed is plural, so the plural is what a hand
        // reaches for, and refusing it teaches nothing.
        "plugin" | "plugins" => Action::Plugin,
        // `/migrate` is admitted because it is the other word for this act, and
        // an operator arriving from another tool is by definition someone with no
        // idea what this one calls things. Neither spelling is an alias for a
        // second screen: both open the one surface.
        "import" | "migrate" => Action::Import,
        // **An alias earns no row of its own.** `/usage` is what an operator
        // coming from another agent types for "what is this costing me", and a
        // second row for one screen reads as a second screen — so this is
        // answered and never listed: it is absent from `COMMANDS`, from the
        // palette and from every group.
        //
        // **It answered `/status` until 0.22.0, and the argument for that was
        // sound right up until it was not.** The note here read: the answer is
        // `/status`, which already commits the spend, the budgets and what is left
        // of them. That was the closest thing this program had to a spending
        // surface, and it was a token draw against a ceiling rather than a cost.
        // The moment a page exists whose whole subject is what has been spent, an
        // operator typing the field's word for that page and landing on the
        // session's configuration is being answered a question they did not ask.
        //
        // It stays an alias rather than becoming a third screen. `/usage` means
        // plan and rate-limit status everywhere else it exists, and this product
        // has no plan and no rate limit of its own to report — so a distinct
        // `/usage` here would be a screen with nothing on it.
        "usage" => Action::Cost,
        "cost" => Action::Cost,
        "stats" => Action::Stats,
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
        // flip between a turn that can fan out and one that does the work itself.
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
        // Answered by the driver while a turn is running, which is the only time
        // there is a turn to steer. It reaches this arm only at an idle prompt,
        // where the honest answer is what it would have done and why it cannot —
        // not "there is no such command", which is what an unregistered name
        // gets and would be a lie about a command the palette lists.
        "steer" => Action::Steer,
        // **One spelling, for the reason `/status` has one.** The driver's
        // mid-turn arm matches the literal word `compact` before `parse` is
        // reached — the shape `/steer` already has — so a second name here would
        // be a name that worked at an idle prompt and did nothing mid-turn, which
        // is the worst kind of alias. `/fold` is deliberately not taken: it is the
        // word io-harness uses internally, and nobody has typed it at a prompt
        // yet.
        //
        // It reaches this arm at an idle prompt, where the answer is not "there is
        // no turn". A request made here is honoured at the *next* turn's first
        // step, through the contract, so the idle case is a real feature rather
        // than a refusal with a sentence.
        "compact" => Action::Compact,
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
        "context" => Action::Context,
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
    // **Grouped, and the groups are the palette's own.** The whole inventory in
    // one column is a list nobody reads, and two surfaces disagreeing about how they
    // are organised is worse than either arrangement — so both render
    // [`grouped`] and neither holds an order of its own.
    for (group, rows) in grouped() {
        if rows.is_empty() {
            continue;
        }
        lines.push(Line::from(Span::styled(
            format!("  {}", group.title()),
            theme.style(Tone::Muted),
        )));
        lines.extend(table(&rows, width, theme));
        lines.push(Line::from(""));
    }
    lines
}

/// The command table on its own, for the reader who typed a command that does
/// not exist. Its first column is measured over the defaults, because there is
/// no key table beside it here to line up with.
fn commands(theme: &Theme) -> Vec<Line<'static>> {
    let width = column(COMMANDS);
    let mut lines = Vec::new();
    for (group, rows) in grouped() {
        if rows.is_empty() {
            continue;
        }
        lines.push(Line::from(Span::styled(
            format!("  {}", group.title()),
            theme.style(Tone::Muted),
        )));
        lines.extend(table(&rows, width, theme));
    }
    lines
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
