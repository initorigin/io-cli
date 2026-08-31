//! Turning io-harness run events into lines.
//!
//! Three rules shape this module.
//!
//! **Every kind is triaged, and nothing falls through.** `EventKind` is
//! `#[non_exhaustive]` and has fifty-two variants today, so a wildcard arm is not
//! a shortcut here, it is required by the type. Until 0.11.0 that wildcard
//! *rendered*, committing the variant's own snake-cased name in a muted line —
//! which is why a transcript said `prompt_composed` and `answered` at whoever was
//! reading it. Now [`crate::triage`] holds a disposition for every kind, the
//! wildcard commits nothing, and a kind that is not in the table at all is
//! counted by [`Events::unknown`] instead of being printed.
//!
//! **Streaming text is not committed a token at a time.** Tokens accumulate in a
//! live buffer that the viewport draws, and the whole passage is committed to
//! scrollback once when it is finished. Committing per token would put a line in
//! the terminal's scrollback for every few characters the model produced.
//!
//! **A tool call is a cell, not a line.** io-harness announces a call before it
//! runs — `EventKind::ToolCall` carries the tool and its target and nothing about
//! what came back, because nothing has come back yet — and reports the result
//! only in the `Step` that commits afterwards. So a call is held open from its
//! announcement, shown live in the viewport while it runs, and committed to
//! scrollback once, complete with its result and how long it took, when the step
//! lands. 0.2.0 committed the announcement immediately, which is why a
//! transcript said what the agent was about to do and never what happened.

use std::time::Duration;

use io_harness::{EventKind, RunEvent, TodoState, MCP_TOOL_PREFIX, NAMESPACE, TODO_MAX_ITEMS};
use ratatui::style::Modifier;
use ratatui::text::{Line, Span};

use crate::picker::fit;
use crate::theme::{Theme, Tone};

/// The width one committed plan row is fitted to.
///
/// A constant rather than the terminal's real width, because `event` is a
/// function of an event and a session age and nothing hands it a width. Eighty is
/// the terminal this product is audited at, and a committed line that overruns a
/// narrower one wraps rather than truncating — `tests/narrow.rs` pins that — so
/// being wrong here costs a wrapped row and never a lost fact.
///
/// The reason to fit at all is io-harness's own `TODO_TEXT_CAP`: one item may be
/// two hundred characters, and sixty-four of those wrapping three rows each is
/// not a list an operator reads, it is the transcript buried under one.
const ROW: usize = 80;

/// io-harness's tool names, and the verb an operator reads instead.
///
/// **A table, and nothing behind it.** A name that is not here is printed
/// exactly as io-harness sent it — never title-cased, never split on its
/// underscores, never guessed at. A verb io-cli invented for a tool it has never
/// seen would be this release's own version of the defect it exists to remove:
/// a word in front of an operator that nothing in the system actually means.
///
/// The rows are io-harness 0.66's own built-ins. An MCP tool arrives namespaced
/// under [`MCP_TOOL_PREFIX`] and a custom tool arrives under whatever name the
/// embedder gave it, and both fall through by design.
pub const VERBS: &[(&str, &str)] = &[
    ("read_file", "Read"),
    ("write_file", "Write"),
    ("edit_file", "Edit"),
    ("patch_file", "Patch"),
    ("list_dir", "List"),
    ("find", "Find"),
    ("grep", "Search"),
    ("exec", "Run"),
    ("shell", "Run"),
    ("shell_start", "Start"),
    ("shell_poll", "Poll"),
    ("shell_kill", "Stop"),
    ("check", "Check"),
    ("git_add", "Stage"),
    ("git_commit", "Commit"),
    ("git_diff", "Diff"),
    ("git_log", "Log"),
    ("git_status", "Status"),
    ("git_branch", "Branch"),
    ("git_worktree", "Worktree"),
    ("lsp_definition", "Definition"),
    ("lsp_hover", "Hover"),
    ("lsp_references", "References"),
    ("lsp_rename", "Rename"),
    ("lsp_symbols", "Symbols"),
    // `Skill` and not `Read skill`: from 0.34.0 this cell is a loaded-skill row
    // rather than a read of a file that happens to be a skill, and the verb is
    // the column that says which of the two a reader is looking at.
    ("read_skill", "Skill"),
    ("todo_write", "Todo"),
    ("remember", "Remember"),
    ("forget", "Forget"),
    ("ask_question", "Ask"),
    ("propose_plan", "Propose"),
    ("spawn_agent", "Spawn"),
    ("send_message", "Send"),
    ("read_messages", "Read messages"),
    ("view_image", "View"),
    ("browser_navigate", "Navigate"),
    ("browser_click", "Click"),
    ("browser_type", "Type"),
    ("browser_read", "Read page"),
    ("browser_scroll", "Scroll"),
    ("browser_screenshot", "Screenshot"),
    ("pdf_read", "Read PDF"),
    ("pdf_write", "Write PDF"),
    ("pdf_fill_form", "Fill form"),
    ("pdf_watermark", "Watermark"),
    ("docx_read", "Read document"),
    ("docx_write", "Write document"),
    ("pptx_read", "Read slides"),
    ("xlsx_read", "Read sheet"),
    ("xlsx_write", "Write sheet"),
    ("xlsx_sheets", "Sheets"),
    ("xlsx_set_cell", "Set cell"),
    ("barcode_decode", "Decode"),
];

/// The verb an MCP tool is drawn under.
///
/// A word rather than nothing, so the first column stays the column a reader
/// skims down. Every other cell in this transcript opens with a verb in accent
/// and bold and puts its target after it muted; an MCP cell that opened with the
/// server and the tool would be the one row whose first column is a target.
pub const MCP_VERB: &str = "Call";

/// The verb for a tool name, or the name exactly as it arrived.
///
/// **An MCP tool is answered before the table is consulted.** Its name is built
/// by io-harness from a prefix, a server id and the tool's own name, so it can
/// never appear in a static table — and up to 0.33.0 it fell through this
/// function unchanged and reached the transcript whole, separators and all. The
/// server and the tool are drawn as the cell's target; see `announce`.
pub fn verb(name: &str) -> &str {
    if name.starts_with(MCP_TOOL_PREFIX) {
        return MCP_VERB;
    }
    VERBS
        .iter()
        .find(|(tool, _)| *tool == name)
        .map_or(name, |(_, verb)| verb)
}

/// A tool call that has been announced and not yet closed.
///
/// Held rather than committed because the two facts a reader actually wants —
/// what came back and how long it took — are not on the announcing event at all.
struct Pending {
    name: String,
    /// io-harness's own name for the tool, before [`verb`] mapped it.
    ///
    /// Kept because the step's decision sentence is written in these words —
    /// `read io.toml`, `list_dir  (4 entries)` — and a cell that printed both
    /// the verb and the sentence said the same thing twice in two vocabularies.
    raw: String,
    target: String,
    /// io-harness's own target, before the display translation was applied to it.
    ///
    /// **The translation broke the deduplication, and this is the repair at the
    /// cause.** [`trim_result`] drops the step's decision sentence when the
    /// sentence only repeats what the cell already says, and it decides that by
    /// comparing words. Once 0.32.0 translated the target *in place*, the
    /// harness's `bundle__skill` stopped matching the cell's `bundle:skill` — so
    /// the sentence was no longer recognised as a repetition and the one string
    /// the translation existed to hide was printed in full, beside the
    /// translation of it.
    ///
    /// Kept *beside* the displayed form rather than instead of it, because a
    /// sentence may legitimately repeat either spelling and both have to count
    /// as things the cell has already said.
    target_raw: String,
    /// The session age at which the call was announced. An age handed in by the
    /// driver, never a clock read here: this module has no timer, and N1 is what
    /// keeps it that way.
    opened_at: Duration,
    /// A duration io-harness itself measured, when it reports one.
    ///
    /// `EventKind::Mcp` is the only event in the whole enum that carries a
    /// per-tool duration, so for an MCP tool this is a real measurement of how
    /// long the tool ran. For every other tool it stays `None` and the cell
    /// falls back to io-cli's own observation, which is a different kind of
    /// number and is printed as one.
    measured: Option<Duration>,
}

/// Accumulates streaming text and turns events into committed lines.
pub struct Events {
    theme: Theme,
    /// Model text received since the last commit.
    live: String,
    /// Calls announced and not yet closed, in the order they were announced.
    ///
    /// A `Vec` and never an `Option`: `read_batch` announces every call in a
    /// parallel batch up front and only then runs any of them, so two or more
    /// open calls before a single `Step` is the ordinary case rather than an
    /// edge one. A single slot would be wrong on the first parallel read.
    open: Vec<Pending>,
    /// Whether the permission boundary refused something during the step now in
    /// flight.
    ///
    /// A refusal does not end a step. io-harness turns it into an observation
    /// fed back to the model and the step commits anyway, so this *marks* the
    /// step rather than closing a cell — and it is the only honest result word
    /// available when the step's own decisions cannot be paired to the calls.
    refused_this_step: bool,
    /// Whether this session runs in plain mode.
    ///
    /// **The one thing that changes what a designed line contains rather than
    /// how it is drawn, and 0.11.0 is what made it necessary.** This release
    /// moved the provider and the run's step and token counts off the transcript
    /// and onto the status line, which is the right place for them — for a
    /// reader who can see the viewport. A plain session is defined by not having
    /// one: its whole promise since 0.6.0 is that every state change reaches the
    /// scrollback as text, and a fact that now lives only in a repainting row is
    /// a fact taken away from exactly the reader who cannot follow it.
    ///
    /// So in plain mode, and only there, the two lines this release removed are
    /// still committed — in the status line's vocabulary rather than the removed
    /// rows' own, so there is one spelling of each fact in the product.
    plain: bool,
    /// Whether an overlay in *this* process will draw a question that is asked.
    ///
    /// See [`Events::set_answering`] for why this is not the same question as
    /// whether a responder exists, and why `false` is the right default.
    answering: bool,
    /// Events whose kind [`crate::triage`] has never heard of.
    ///
    /// Counted rather than printed. A kind with no disposition is one io-harness
    /// began emitting after this release: the operator reading the transcript is
    /// not the person who can act on it, and a variant name in front of them is
    /// what this release exists to remove — but a session that discarded it
    /// without a trace would leave nobody able to find out either.
    unknown: usize,
    /// The session age at which the step now in flight opened.
    ///
    /// A thought's duration is the interval since then, which is the only
    /// number about a thought this crate can honestly report: io-harness does
    /// not say when the model started thinking, and the step boundary is the
    /// last thing that happened before it did.
    step_at: Duration,
    /// The last thought that did not fit, whole.
    ///
    /// Held because this event is the only place reasoning is ever visible —
    /// io-harness neither stores it nor folds it into the next prompt — so a
    /// bound that dropped the remainder would not be fitting the text, it would
    /// be destroying it. [`Events::thought`] is what `/expand` reads.
    thought: Option<String>,
    /// The act and target of an approval io-harness is blocked on, if it is.
    ///
    /// Set by `ApprovalRequested` and cleared by `ApprovalDecided`, so it is the
    /// harness's own account of whether the run is waiting on a person rather
    /// than io-cli's account of whether an overlay happens to be on screen. The
    /// two are not the same: a contained turn's approval can be answered by a
    /// responder with no overlay drawn here at all, and the run is still stopped.
    awaiting: Option<String>,
    /// Whether the last thing committed was a tool cell.
    ///
    /// The model's prose starts arriving a token at a time with nothing between
    /// it and the cell above, so against a real run an answer began on the row
    /// under `⋅ Search model · (1 hits) · ~0ms` and read as part of it. One blank
    /// goes in when the prose starts, rather than after every cell: a step that
    /// runs four calls should print four rows, not eight.
    after_cell: bool,
    /// Renders the model's markdown, and remembers an open code fence.
    ///
    /// Held here because a fence spans lines and the transcript commits a line
    /// at a time: the state has to outlive the line that opened it.
    markdown: crate::markdown::Markdown,
    /// Whether the last row committed was blank.
    ///
    /// Starts true, so nothing at the very top of a session opens with a blank
    /// row. It is what keeps the gap rule from doubling a blank a designed line
    /// has already ended its own block with.
    last_blank: bool,
    /// Whether the last row committed was the model's own prose.
    ///
    /// Starts true, so a turn's first tokens do not open with a blank row: the
    /// goal line above them already ends the block it belongs to.
    last_prose: bool,
    /// Whether the last thing that happened was the model thinking.
    ///
    /// Cleared by every event that says something else is now happening — a
    /// token, a call, a step, a turn beginning or ending — so "most recent" means
    /// what it says. A fact that reaches no surface, a spend draw for instance,
    /// leaves it alone: it is not a different thing happening, it is a number.
    thinking: bool,
    /// The workspace this session is held over, for shortening a tool's target.
    ///
    /// Empty until the driver says otherwise, and an empty root shortens
    /// nothing: a path is printed whole rather than trimmed against a guess at
    /// where the session is. `App::set_root` is the one caller.
    root: std::path::PathBuf,
    /// What the operator actually typed, when it is not what was submitted.
    ///
    /// **The prompt echo is io-harness's own event field, not a string io-cli
    /// holds** — the row is drawn from `EventKind::Started`'s `goal`, which is
    /// the text that was sent. For a slash-invoked skill those two deliberately
    /// differ: the submitted prompt is the catalogue name `ultraship__brainstorm`
    /// because that is the only string `read_skill` resolves, and the operator
    /// typed `/ultraship:brainstorm`. Up to 0.33.0 the machine's spelling was
    /// what came back.
    ///
    /// Carried rather than derived, and that is the point. Translating the goal
    /// in the `Started` arm would run the display translation over arbitrary
    /// operator prose — which is how `src/__init__.py` became `src/:init__.py` in
    /// 0.32.0's first draft, on the largest text surface in the product. Only the
    /// driver knows a submission was an invocation, so only the driver says so.
    ///
    /// Taken by the arm that draws it, so it cannot outlive the turn it belongs
    /// to and be shown over the next prompt.
    echo: Option<String>,
}

/// io-harness's name for the tool that opens a skill.
///
/// Named here because it is the one tool whose `target` can be a *name* rather
/// than a path, which is what decides whether the display translation applies to
/// it.
pub const READ_SKILL: &str = "read_skill";

/// What a `read_skill` cell says when the read returned and said nothing more.
pub const LOADED: &str = "loaded";

/// What a `read_skill` cell says when the call asked for the bundle itself.
///
/// io-harness writes an empty `path` into its own sentence as `.`, and says why
/// in its own comment: the empty path resolves to the root and lists it, and the
/// dot keeps that listing distinguishable from a body read in the durable trace.
/// A dot alone is that distinction spelled for a trace rather than for a person,
/// so the cell says the word.
pub const LISTED: &str = "listed";

/// Whether a `read_skill` target is a skill's *name* rather than a file inside
/// one.
///
/// **This stopped being a property of the tool in io-harness 0.73.0.** That
/// version gave `read_skill` an optional `path`, so a skill can read its own
/// bundle's `shared/` and `references/` files — and the announcement picks the
/// first argument present out of an ordered list in which `path` is checked
/// *before* `name`. So the same tool now announces a relative file path where
/// every earlier version announced a skill's name.
///
/// The display translation must not reach that path. It rewrites the first
/// separator to a colon, and a companion file called `__init__.py` would be
/// drawn as `:init__.py` — a path that does not exist, on the one surface an
/// operator checks to see what the agent touched. That is verbatim the failure
/// 0.32.0 found and gated against; 0.73.0 moved the ground under the gate.
///
/// A path separator or a dot is what tells them apart. A skill's name is a
/// directory or file stem io-harness joined to a bundle id, and the two things
/// this refuses — a separator and an extension — are what a path has and a name
/// does not. The authority is the step's own decision sentence, which carries
/// the skill and the file as two words; this is only what the *live* row can
/// know, before any sentence has arrived.
fn names_a_skill(target: &str) -> bool {
    !target.is_empty()
        && !target.contains('/')
        && !target.contains('\\')
        && !target.contains('.')
}

impl Events {
    pub fn new(theme: Theme) -> Self {
        Self {
            theme,
            live: String::new(),
            open: Vec::new(),
            refused_this_step: false,
            plain: false,
            answering: false,
            unknown: 0,
            step_at: Duration::ZERO,
            thought: None,
            awaiting: None,
            after_cell: false,
            markdown: crate::markdown::Markdown::default(),
            last_blank: true,
            last_prose: true,
            thinking: false,
            root: std::path::PathBuf::new(),
            echo: None,
        }
    }

    /// Say what the operator typed, for the turn about to start.
    ///
    /// Called by the driver immediately before submitting a prompt that is not
    /// the text that was typed — today, a slash-invoked skill, whose submitted
    /// form is the catalogue name. Every other prompt submits what was typed and
    /// needs no call at all.
    pub fn set_echo(&mut self, typed: impl Into<String>) {
        self.echo = Some(typed.into());
    }

    /// Forget everything this module holds about a conversation.
    ///
    /// What is dropped is what a new conversation must not inherit: the tail
    /// nobody committed, the calls nothing will ever close, the thought
    /// `/expand` would otherwise show from a conversation no longer on screen,
    /// and the two live-row states. The theme, the mode and the workspace stay —
    /// they are the session's, not the conversation's.
    pub fn forget(&mut self) {
        self.live.clear();
        self.open.clear();
        self.refused_this_step = false;
        self.thought = None;
        self.awaiting = None;
        self.after_cell = false;
        self.markdown.forget();
        self.last_blank = true;
        self.last_prose = true;
        self.thinking = false;
        self.step_at = Duration::ZERO;
        self.echo = None;
    }

    /// Say which workspace this session is held over, so a target inside it can
    /// be shown relative to it. Handed down by [`crate::app::App::set_root`].
    pub fn set_root(&mut self, root: impl Into<std::path::PathBuf>) {
        self.root = root.into();
    }

    /// The last thought that was fitted, whole, or `None` when the last one was
    /// committed in full.
    ///
    /// `/expand` is this product's one answer to "show me more", and this is
    /// what it shows when the thing there is more of is a thought.
    pub fn thought(&self) -> Option<&str> {
        self.thought.as_deref()
    }

    pub fn set_theme(&mut self, theme: Theme) {
        self.theme = theme;
    }

    /// Say whether this session runs in plain mode. Decided once, by
    /// [`crate::settings::plain`], and handed down through [`crate::app::App`].
    pub fn set_plain(&mut self, plain: bool) {
        self.plain = plain;
    }

    /// Say whether this process is holding the overlay that answers a question.
    ///
    /// **Not "is a responder registered" — that is always true.** io-cli attaches
    /// an `Answerer` to every contract it builds, so the harness always has
    /// somebody to ask. What varies is whether io-cli kept the receiving end: four
    /// sites drop it, three of them on contracts that never drive a turn, and one
    /// — the `/resume` continuation — that does. A question asked by that run
    /// reaches no overlay, so the durable transcript line has to render it.
    ///
    /// Default `false`, which is the safe direction: a path that forgets to say so
    /// prints the question twice, and a path that wrongly claimed an overlay would
    /// not print it at all.
    pub fn set_answering(&mut self, answering: bool) {
        self.answering = answering;
    }

    /// How many events arrived carrying a kind with no disposition.
    ///
    /// Zero on every run against the io-harness this release is locked to, by
    /// construction: `tests/triage.rs` fails if the table and that version's enum
    /// disagree. It stops being zero when a later harness is pinned and something
    /// new reaches a session, which is exactly when somebody needs to know.
    pub fn unknown(&self) -> usize {
        self.unknown
    }

    /// What an event with no designed line commits, which is nothing.
    ///
    /// Public so that a test can hand it a kind name io-harness does not have
    /// yet. Every kind this crate is locked to can be constructed and driven
    /// through [`Events::event`], and exactly one branch here cannot be reached
    /// that way — the one that matters, because a kind with no disposition is by
    /// definition one this release has never seen.
    pub fn undesigned(&mut self, name: &str) -> Vec<Line<'static>> {
        if crate::triage::disposition(name).is_none() {
            self.unknown += 1;
        }
        Vec::new()
    }

    /// The one live row the viewport draws: an open tool call if there is one,
    /// otherwise the unfinished tail of what is streaming.
    ///
    /// It is arbitrated rather than concatenated because the row is not free.
    /// No `Token` arrives while a tool is dispatching, so the buffer is frozen —
    /// but it is frozen holding the assistant's last unterminated line, which is
    /// usually not empty. The obvious fix, flushing at `ToolCall` to empty it,
    /// is wrong: committing the tail appends a blank line after it, so every tool
    /// cell in the transcript would arrive with a blank row in front of it.
    /// An open call is the more urgent thing to say, so it wins the row, and the
    /// tail stays live until something legitimately commits it.
    pub fn live(&self) -> String {
        let glyphs = &self.theme.glyphs;
        // **A person outranks a machine, and it is not close.** Everything else
        // this row can say describes work that is going on without the operator;
        // this one says the work has stopped and is waiting on them. Told in the
        // other order, the row reports the agent as busy at the exact moment it
        // is blocked on somebody who is reading this row to find out.
        if let Some(waiting) = &self.awaiting {
            return format!(
                "{} waiting for you {} {waiting}",
                glyphs.bullet, glyphs.dash
            );
        }
        let Some(call) = self.open.last() else {
            // No call open and the last thing that happened was the model
            // thinking. The thought itself commits when it arrives; this is the
            // row for the interval before the next thing does.
            if self.thinking {
                return format!("{} thinking {}", glyphs.bullet, glyphs.ellipsis);
            }
            return self.live.clone();
        };
        let mut row = format!("{} {}", glyphs.bullet, call.name);
        if !call.target.is_empty() {
            row.push(' ');
            row.push_str(&call.target);
        }
        row.push(' ');
        row.push_str(glyphs.ellipsis);
        if self.open.len() > 1 {
            row.push_str(&format!(" (+{} more)", self.open.len() - 1));
        }
        row
    }

    /// End the turn: commit whatever streamed, then close every call still open.
    ///
    /// The public entry point, called when a turn finishes or is interrupted. A
    /// `Step` may never arrive — io-harness skips `commit_step` when a sub-agent's
    /// child deferred — so this is the only place that can honestly close a call
    /// that nothing ever reported on.
    ///
    /// Those cells carry no duration. io-cli knows when the call was announced
    /// and nothing at all about when it stopped, and a number printed there would
    /// be a guess wearing a measurement's clothes.
    pub fn flush(&mut self) -> Vec<Line<'static>> {
        let mut lines = self.flush_text();
        let theme = self.theme;
        for call in std::mem::take(&mut self.open) {
            lines.push(cell_line(theme, &call, "unfinished", None, false));
        }
        self.refused_this_step = false;
        // The turn is over, so nothing is thinking and nobody is being waited
        // on. An interrupt between the request and the decision is the case that
        // needs this: the harness never sends the `ApprovalDecided` that would
        // otherwise clear it, and the row would go on asking for an answer to a
        // question that died with the run.
        self.awaiting = None;
        self.after_cell = false;
        self.thinking = false;
        lines
    }

    /// Commit whatever text has streamed so far, if any, leaving open calls open.
    ///
    /// Separate from the public `flush` because most of the callers below are
    /// mid-step — a refusal and an approval request both arrive while the call
    /// they are about is still running — and closing that call as unfinished
    /// there would report the opposite of what happened.
    fn flush_text(&mut self) -> Vec<Line<'static>> {
        if self.live.trim().is_empty() {
            self.live.clear();
            return Vec::new();
        }
        let text = std::mem::take(&mut self.live);
        let theme = self.theme;
        let mut lines: Vec<Line<'static>> = text
            .lines()
            .map(|line| self.markdown.line(line, &theme))
            .collect();
        lines.push(Line::from(""));
        lines
    }

    /// What this event commits to scrollback, given the session age the driver
    /// read the clock for.
    ///
    /// Empty means "nothing yet" — which for a token, and for a tool call that
    /// has not finished, is the correct answer rather than a dropped event.
    ///
    /// `at` is handed in for the same reason `App::tick` takes one: nothing here
    /// may read a clock, so a test can state the interval between two events by
    /// hand and assert on it without anything being timed.
    pub fn event(&mut self, event: &RunEvent, at: Duration) -> Vec<Line<'static>> {
        // **One blank row between a block of tool cells and whatever comes
        // next.** Taken here rather than pushed by the arms that need it, so the
        // rule holds for every one of them — the model's prose, a thought, a
        // harness warning — and so a step that ran four calls still prints four
        // rows rather than eight. The flag is set at the end of the `Step` arm,
        // which is why it is read at the start of the next event and not inside
        // the one that raised it.
        let after_cell = std::mem::take(&mut self.after_cell);
        // The model's own prose is one kind of row and everything this crate
        // designs is another, and a change from one to the other is worth a
        // blank: against a real run an answer began on the row directly under
        // `warning: nothing has changed in 3 steps…` and read as part of it.
        let prose = matches!(event.kind, EventKind::Token { .. });
        // **And the operator's next words are not the tail of the last block.**
        // The `›` line is the one row in a transcript its reader wrote, and it
        // opens the block below it — so it must not arrive welded to the block
        // above. `after_cell` covered only the case where that block ended in a
        // tool cell; a turn that ended on a thought footer or a harness warning
        // put the next goal on the row directly under it, and the two read as one
        // thing said by one voice.
        //
        // Taken here, beside the other two, and never pushed by the `Started` arm
        // itself (`Events::commit`, src/events.rs:540): that arm cannot see
        // `last_blank`, so a blank pushed there is pushed unconditionally and
        // doubles at the very top of a session — which is the one case
        // `last_blank` was introduced for, and is what starts it `true`
        // (src/events.rs:270).
        let goal = matches!(event.kind, EventKind::Started { .. });
        // Never against a blank that is already there. The goal line ends its own
        // block with one, so an answer arriving after it was given a second and
        // the prompt sat two rows above what it asked for. The same blank is what
        // keeps a goal following an ordinary finished turn to one row of air:
        // `Finished` ends its own block with one (src/events.rs:1055).
        let gap = (after_cell || goal || (prose && !self.last_prose)) && !self.last_blank;

        let mut lines = self.commit(event, at);
        if gap && !ends_blank(&lines[..1.min(lines.len())]) {
            lines.insert(0, Line::from(""));
        }
        if !lines.is_empty() {
            self.last_prose = prose;
            self.last_blank = ends_blank(&lines);
        }
        lines
    }

    /// What this event commits, before the spacing rule above is applied.
    fn commit(&mut self, event: &RunEvent, at: Duration) -> Vec<Line<'static>> {
        // What the live row says is a fact about the *last* thing that happened,
        // so it is decided here rather than in each arm: the arms below say what
        // an event commits, and this says what it means for the row that is not
        // committed at all. See `Events::live`.
        match &event.kind {
            EventKind::Reasoning { text, .. } if !text.trim().is_empty() => self.thinking = true,
            EventKind::Token { .. }
            | EventKind::ToolCall { .. }
            | EventKind::Step { .. }
            | EventKind::Started { .. }
            | EventKind::Finished { .. } => self.thinking = false,
            _ => {}
        }
        let theme = self.theme;
        let separator = theme.glyphs.separator;
        let dash = theme.glyphs.dash;
        match &event.kind {
            EventKind::Token { text } => {
                self.live.push_str(text);
                // Every COMPLETE line commits as it arrives; only the unfinished
                // tail stays live. That is what keeps the viewport a fixed few
                // rows while an answer of any length streams: a line that will
                // never change again belongs to the terminal, not to us.
                let mut lines = Vec::new();
                while let Some(newline) = self.live.find('\n') {
                    let finished: String = self.live.drain(..=newline).collect();
                    lines.push(self.markdown.line(finished.trim_end_matches('\n'), &theme));
                }
                lines
            }
            // The goal, and nothing else. Through 0.10.0 a second row said
            // `via {provider}` under every prompt an operator ever typed — the
            // same fact, repeated once per turn, about a setting that changes
            // perhaps twice in a session. It is a status-line field now
            // (`App::status_from`), which is where every other fact of that
            // shape already lives.
            //
            // `provider` is still destructured rather than ignored, so that a
            // release which stops setting the field cannot leave this arm
            // silently reading a value nobody uses.
            EventKind::Started { goal, provider } => {
                // A turn's first step opens here, and last turn's leftover
                // thought stops being the thing `/expand` has more of.
                self.step_at = at;
                self.thought = None;
                // **The operator's own words, weighted as such.** This row is the
                // only one in a transcript that the reader wrote, and in a
                // scrollback of tool cells and model prose it has to be findable
                // by eye at a scroll. The mark is accent and bold, and the words
                // are bold in the ordinary foreground — a colour would make them
                // one more coloured thing among many.
                //
                // **A prompt written on three lines is three rows.** A `Line` is
                // one row and a newline inside a span is not a break — ratatui
                // draws the cells and the newline is not one, so `abc\ndef` came
                // back as `abcdef` and the operator could not read their own
                // words. The rest of the prompt is indented under the first
                // character rather than under the mark, so the block reads as one
                // thing said once.
                //
                // **What the operator typed, when the driver said it differed
                // from what was sent.** Taken rather than read, so a turn cannot
                // show the previous turn's typing; a prompt submitted as itself
                // sets nothing and falls back to the goal, which is the same
                // string.
                let marker = theme.glyphs.marker;
                let indent = " ".repeat(marker.chars().count());
                let typed = self.echo.take().unwrap_or_else(|| goal.clone());
                let mut lines: Vec<Line<'static>> = typed
                    .split('\n')
                    .enumerate()
                    .map(|(row, line)| {
                        Line::from(vec![
                            Span::styled(
                                if row == 0 {
                                    marker.to_string()
                                } else {
                                    indent.clone()
                                },
                                theme.style(Tone::Accent).add_modifier(Modifier::BOLD),
                            ),
                            Span::styled(
                                line.to_string(),
                                theme.style(Tone::Normal).add_modifier(Modifier::BOLD),
                            ),
                        ])
                    })
                    .collect();
                if self.plain {
                    lines.push(Line::from(Span::styled(
                        format!("  provider:{provider}"),
                        theme.style(Tone::Muted),
                    )));
                }
                lines.push(Line::from(""));
                lines
            }
            EventKind::Step {
                decision,
                tool_call,
                tokens,
                changed,
            } => {
                // This step closing is the next one opening, and a thought
                // belonging to the next step is measured from here.
                self.step_at = at;

                // Taken before anything else, so that the tail flush below
                // cannot close these as unfinished a line before the step that
                // finished them says otherwise.
                let open = std::mem::take(&mut self.open);
                let refused = std::mem::replace(&mut self.refused_this_step, false);

                // A step's own narration commits after whatever streamed before
                // it, so the transcript reads in the order it happened.
                let mut lines = self.flush_text();

                // Then the calls it ran, in the order they were announced, then
                // the step line: chronological, which is the only order a
                // transcript can be skimmed in.
                //
                // `decision` is `decisions.join("; ")`, one sentence per thing the
                // step did, each written after that thing ran. Pairing them to the
                // calls positionally is right whenever there is one of each — but
                // the count is not guaranteed: the workspace loop pushes extra
                // decisions for spawned children, and both loops push an "awaiting
                // approval" segment. So the pairing is used only when the two
                // counts agree, and never indexed into blind.
                // An empty segment is not a result either, and a step whose
                // decision is blank pairs to a cell with nothing in its result
                // column — so a blank disqualifies the pairing rather than being
                // printed as an answer.
                let parts: Vec<&str> = decision.split("; ").collect();
                let complete = parts.iter().all(|part| !part.trim().is_empty());
                let paired = complete && parts.len() == open.len();
                let verdict = if *changed {
                    "changed files"
                } else {
                    "no change"
                };
                for (index, call) in open.iter().enumerate() {
                    // Never invented. The harness's own sentence when it can be
                    // matched, otherwise the coarsest true thing known about the
                    // step — a refusal marked it, or the step's own verdict.
                    let result = if paired {
                        parts[index]
                    } else if refused {
                        "refused"
                    } else {
                        verdict
                    };
                    lines.push(cell_line(theme, call, result, Some(at), paired));
                    self.after_cell = true;
                }

                // **The step line is committed only when it says something the
                // cells above it did not.** Through 0.10.0 it always was, and
                // against a real run that read as every fact printed twice in two
                // orders: a cell saying `List · (4 entries) · ~0ms` and a line
                // under it saying `list_dir  (4 entries) · List · no change ·
                // 3383 tok · step 1`. The token count and the step number are on
                // the status line since this release, so what is left that the
                // cells cannot say is: files changed, or a decision that could
                // not be paired to a call.
                //
                // A step with no call at all is the one that produced the answer.
                // Its decision is worth a line when it is a sentence — the model
                // saying what it did — and worth nothing when it is io-harness's
                // placeholder for a step that called no tool, which against a
                // real run committed `no tool call · no change · 3680 tok · step
                // 4` under the answer it was describing.
                let empty_handed = decision.trim().is_empty() || decision.trim() == "no tool call";
                // **Not even for `changed files`.** A step whose calls each got
                // their own cell has been fully described by them, and the change
                // itself is committed as a diff by the driver a line later — so
                // the step line under it read `wrote notes.md · Write · changed
                // files · 3397 tok · step 1` over a cell that had just said the
                // first two and a diff about to say the third.
                let say_step = (!open.is_empty() && !paired) || (open.is_empty() && !empty_handed);
                if !say_step {
                    return lines;
                }
                let mut spans = vec![Span::styled(decision.clone(), theme.style(Tone::Normal))];
                if !tool_call.is_empty() {
                    spans.push(Span::styled(separator, theme.style(Tone::Muted)));
                    spans.push(Span::styled(
                        tool_names(tool_call),
                        theme.style(Tone::Accent),
                    ));
                }
                // Always said, in both directions. A result that appears only
                // sometimes is a column a reader cannot skim down, and `changed`
                // is the one thing this event reports about what came back.
                spans.push(Span::styled(separator, theme.style(Tone::Muted)));
                spans.push(Span::styled(
                    if *changed {
                        "changed files"
                    } else {
                        "no change"
                    },
                    theme.style(if *changed { Tone::Success } else { Tone::Muted }),
                ));
                spans.push(Span::styled(
                    format!("{separator}{tokens} tok{separator}step {}", event.step),
                    theme.style(Tone::Muted),
                ));
                lines.push(Line::from(spans));
                lines
            }
            EventKind::ToolCall { name, target } => {
                // Nothing is committed here, and that is the point: this event is
                // emitted before the call runs, so a line written now could only
                // say what the agent was about to do. The call is held open, shown
                // by `live()` while it runs, and committed once — with its result
                // and its duration — by the `Step` above.
                //
                // One cell per call either way. The full output goes to the run's
                // durable trace rather than to the screen; uncollapsed tool output
                // is what makes a transcript unreadable.
                // Read into the operator's vocabulary here rather than at the
                // cell, so the live row and the committed cell say the same
                // word about the same call. The stutter guard in `cell_line`
                // compares the two fields, so a target io-harness fell back to
                // the tool's own name is mapped with it and stays equal to it.
                let shown = verb(name);
                self.open.push(Pending {
                    name: shown.to_string(),
                    raw: name.clone(),
                    // **A skill's target is a name, not a path, and since 0.32.0
                    // it is drawn the way the operator reads it** — `read_skill`
                    // was the one place io-harness's `bundle__skill` reached a
                    // person without going through a picker.
                    //
                    // **Gated on the tool, and the first draft was not.** It
                    // translated every target, on the reasoning that no path
                    // contains the separator — which is false and commonly so:
                    // `__init__.py`, `__pycache__`, `__tests__`, `__mocks__`,
                    // `__snapshots__`. `read src/__init__.py` was drawn as
                    // `read src/:init__.py`, a path that does not exist, in the one
                    // place an operator checks what the agent touched. Every Python
                    // and Jest repository would have met it on the first turn.
                    //
                    // **An MCP tool's identity IS its target, and it is answered
                    // first.** io-harness builds the name as a prefix, the server
                    // and the tool, so the cell's useful fact is which server ran
                    // which tool; the call's arguments go to the durable trace,
                    // where a whole argument list belongs. The prefix is stripped
                    // *before* translating, and that order is load-bearing: the
                    // prefix itself ends with the separator, so translating the
                    // whole name splits at the prefix's own join and yields
                    // `mcp:github__create_issue` — a string that is both wrong and
                    // still carrying the thing the translation exists to remove.
                    target: if let Some(tool) = name.strip_prefix(MCP_TOOL_PREFIX) {
                        crate::naming::display(tool)
                    } else if target == name {
                        shown.to_string()
                    } else if name == crate::events::READ_SKILL && names_a_skill(target) {
                        crate::naming::display(target)
                    } else {
                        relative(target, &self.root)
                    },
                    target_raw: target.clone(),
                    opened_at: at,
                    measured: None,
                });
                Vec::new()
            }
            // **Committed nowhere, and not dropped either.** A draw is emitted
            // on every step of a contained turn, so a line each would double the
            // transcript's length and say in prose what the status line says in
            // one field — and the field is where the design puts it. This is the
            // same shape as `ToolCall` above, whose fact is committed by the
            // `Step` that follows it rather than by a line of its own: the event
            // reaches a surface, just not this one. `App::status_from` is that
            // surface, and `tests/status.rs` asserts it.
            EventKind::SpendDraw { .. } => Vec::new(),
            // **Background work, said where it happens.** A `shell_start` is the
            // one tool call whose effect outlives the step that made it, so the
            // ordinary tool cell — opened here, committed by the `Step` that
            // follows — describes a thing that has already finished when in fact
            // it has only begun. These four say what the cell cannot.
            EventKind::HandleStarted { handle, line } => {
                let mut lines = self.flush_text();
                lines.push(Line::from(vec![
                    Span::styled(format!("job {handle} "), self.theme.style(Tone::Accent)),
                    Span::styled(
                        format!("started in the background: {line}"),
                        self.theme.style(Tone::Normal),
                    ),
                ]));
                lines
            }
            // **Nothing, and deliberately.** A poll carries a byte count and never
            // the bytes, and a line per poll would bury the transcript under the
            // progress of something the operator asked to run in the background
            // precisely so they would not have to watch it. The count on the
            // status line is where a poll's fact belongs, and it does not move it:
            // a poll is not an ending.
            EventKind::HandlePolled { .. } => Vec::new(),
            EventKind::HandleExited { handle, code } => {
                let mut lines = self.flush_text();
                // The code says which of two things happened and the tone follows
                // it. `None` is a process that ended without one — killed by a
                // signal, most often — and saying "exited with no status" is the
                // honest form of a number that does not exist.
                let (text, tone) = match code {
                    Some(0) => ("exited cleanly".to_string(), Tone::Success),
                    Some(code) => (format!("exited with status {code}"), Tone::Warning),
                    None => ("exited with no status".to_string(), Tone::Warning),
                };
                lines.push(Line::from(vec![
                    Span::styled(format!("job {handle} "), self.theme.style(Tone::Accent)),
                    Span::styled(text, self.theme.style(tone)),
                ]));
                lines
            }
            EventKind::HandleKilled { handle } => {
                let mut lines = self.flush_text();
                lines.push(Line::from(vec![
                    Span::styled(format!("job {handle} "), self.theme.style(Tone::Accent)),
                    Span::styled("killed".to_string(), self.theme.style(Tone::Muted)),
                ]));
                lines
            }
            // The run finished while this was still up. io-harness kills live
            // handles on the way out and says why; a run that leaves something
            // behind is exactly the case the operator needs told, because the
            // session is about to look idle for a reason that is not idleness.
            EventKind::HandleOrphaned { handle, reason } => {
                let mut lines = self.flush_text();
                lines.push(Line::from(vec![
                    Span::styled(format!("job {handle} "), self.theme.style(Tone::Accent)),
                    Span::styled(
                        format!("was left running: {reason}"),
                        self.theme.style(Tone::Warning),
                    ),
                ]));
                lines
            }
            // **The fleet, committed where it happens.** Four events, four lines,
            // and every one of them indented by the event's OWN depth: a spawn is
            // attributed to the PARENT's run id at the parent's depth, and the
            // child's events arrive afterwards one level deeper under the child's
            // own id. Indenting by the child's depth would put the spawn a level
            // too far in, which is invisible at depth one and wrong past it.
            EventKind::Spawned { child_run_id, goal } => {
                let mut lines = self.flush_text();
                lines.push(Line::from(vec![
                    Span::styled(nest(event.depth), theme.style(Tone::Muted)),
                    Span::styled(leader(separator), theme.style(Tone::Accent)),
                    Span::styled(
                        format!("run {child_run_id} {dash} {goal}"),
                        theme.style(Tone::Normal),
                    ),
                ]));
                lines
            }
            // A refusal, and never an error. The parent is told and carries on
            // with what it has, so this says which ceiling was reached and that
            // the run continues. **The cap is io-harness's own word** — `agents`,
            // `depth` or `budget` — and one it does not know is printed as it
            // came rather than folded into the nearest one this release knows,
            // which would be a line asserting a cap that did not refuse anything.
            //
            // Concurrency never appears here by construction: crossing
            // `max_concurrent_agents` queues a child and reports `Fleet`.
            EventKind::SpawnRefused { cap } => {
                let why = match cap.as_str() {
                    "agents" => "the tree already holds as many agents as it may".to_string(),
                    "depth" => "the child would nest deeper than allowed".to_string(),
                    "budget" => "the tree's token ceiling is spent".to_string(),
                    other => format!("the {other} cap refused it"),
                };
                let mut lines = self.flush_text();
                lines.push(Line::from(vec![
                    Span::styled(nest(event.depth), theme.style(Tone::Muted)),
                    Span::styled(leader(separator), theme.style(Tone::Warning)),
                    Span::styled(
                        format!("spawn refused {dash} {why}"),
                        theme.style(Tone::Warning),
                    ),
                ]));
                lines.push(Line::from(Span::styled(
                    format!("{}  the agent goes on with what it has", nest(event.depth)),
                    theme.style(Tone::Muted),
                )));
                lines
            }
            // **Attributed to the tree and to no child, because the event carries
            // no run id.** `ChildDetached` carries one and `ChildCollected` does
            // not, and with several children in flight their reports arrive in
            // whatever order they finish — so naming one here would be a guess
            // rendered in the same words as a fact.
            EventKind::ChildCollected { text } => {
                let mut lines = self.flush_text();
                lines.push(Line::from(vec![
                    Span::styled(nest(event.depth), theme.style(Tone::Muted)),
                    Span::styled(leader(separator), theme.style(Tone::Accent)),
                    Span::styled("a child reported back", theme.style(Tone::Normal)),
                ]));
                for line in text.lines() {
                    lines.push(Line::from(Span::styled(
                        format!("{}    {line}", nest(event.depth)),
                        theme.style(Tone::Muted),
                    )));
                }
                lines
            }
            // The parent stopped waiting; the child did not stop. `after` is what
            // says which of the two ways that happened — a wall clock it crossed,
            // or a spawn that never waited at all — and both are stated, because
            // "still running" is the part an operator would otherwise assume the
            // opposite of.
            EventKind::ChildDetached {
                child_run_id,
                after,
            } => {
                let how = match after {
                    Some(after) => format!("after {} seconds", after.as_secs()),
                    None => "without waiting".to_string(),
                };
                let mut lines = self.flush_text();
                lines.push(Line::from(vec![
                    Span::styled(nest(event.depth), theme.style(Tone::Muted)),
                    Span::styled(leader(separator), theme.style(Tone::Muted)),
                    Span::styled(
                        format!(
                            "run {child_run_id} left to itself {dash} {how}, and it is still \
                             running"
                        ),
                        theme.style(Tone::Muted),
                    ),
                ]));
                lines
            }
            // 0.65.0 — a resume found a call that was started and never finished,
            // and refused to drive rather than make it a second time. It is styled
            // rather than left to the catch-all because the muted word
            // `recovery_paused` says nothing an operator can act on, and the two
            // things they need — which tool, and the attempt id a decision has to
            // name — are both carried by the event.
            EventKind::RecoveryPaused { attempt_id, tool } => {
                let mut lines = self.flush_text();
                lines.push(Line::from(vec![
                    Span::styled(leader(separator), theme.style(Tone::Warning)),
                    Span::styled(
                        format!("paused {dash} {tool} was interrupted"),
                        theme.style(Tone::Warning),
                    ),
                ]));
                lines.push(Line::from(Span::styled(
                    format!(
                        "  whether it ran is unknown, so nothing was repeated; attempt {attempt_id}"
                    ),
                    theme.style(Tone::Muted),
                )));
                lines
            }
            EventKind::Refused {
                act,
                target,
                rule,
                layer,
            } => {
                // Marks the step, and closes nothing. A refusal is fed back to the
                // model as an observation and the step commits anyway, so there is
                // no call to close here — and often no open call at all, since a
                // dial, a verification and an MCP server spawn are all refused
                // outside any step. Nothing below indexes into the open list, which
                // is what keeps this arm correct at step zero.
                //
                // The act and the target are deliberately not matched against an
                // open call: `act` is a policy verb such as "write" or literally
                // "tool", and `target` is the policy's resolved path, while a
                // call's target is the raw argument the model wrote. Matching them
                // by string would pair the wrong things confidently.
                self.refused_this_step = true;

                // Act, target, rule, layer — in that order, and the last two are
                // the facts no other terminal agent can print, because no other
                // core records them. Asserted by position rather than by presence:
                // a `contains` assertion is just as green when the sentence is
                // inside out.
                let mut text = format!("{act} {target}");
                match (rule, layer) {
                    (Some(rule), Some(layer)) => {
                        text.push_str(&format!("{separator}rule {rule}{separator}layer {layer}"));
                    }
                    (Some(rule), None) => text.push_str(&format!("{separator}rule {rule}")),
                    // Said, not left blank. In io-harness a missing rule means the
                    // policy's own default for that act decided — the *least*
                    // vouched-for kind of action rather than the most — so silence
                    // here would read as the opposite of what happened.
                    (None, _) => text.push_str(&format!(
                        "{separator}no rule named it: the tier default decided"
                    )),
                }
                let mut lines = self.flush_text();
                lines.push(theme.notice(Tone::Refused, text));
                lines
            }
            EventKind::ApprovalRequested { act, target } => {
                // One line, and deliberately a thin one. The event carries only the
                // act and the target; the rule, the layer and the content a write
                // proposes arrive on the approver seam instead, and the overlay is
                // drawn from those. This is the transcript's note that the run
                // stopped, not the question itself — the question must never be
                // committed, which is what F1 asserts.
                self.awaiting = Some(format!("{act} {target}"));
                let mut lines = self.flush_text();
                // **The overlay is about to say this, larger and with the rule,
                // the layer and the diff under it.** Committing a line here as
                // well put `warning: write SUMMARY.md — waiting for you` directly
                // above `warning: write SUMMARY.md`, which is the same sentence
                // twice in two sizes.
                //
                // Not in plain mode, which draws no overlay at all: there the
                // line is the only account of a run that stopped, and its whole
                // promise is that every state change reaches the scrollback.
                if self.plain {
                    lines.push(theme.notice(
                        Tone::Warning,
                        format!("{act} {target} {dash} waiting for you"),
                    ));
                }
                lines
            }
            EventKind::ApprovalDecided {
                act,
                target,
                decision,
            } => {
                // The harness's own record of what it was told, which is not the
                // same line as io-cli's. They agree because the answer travelled
                // one way; if they ever disagree, this is where it shows.
                //
                // The run is moving again, so the live row stops saying it is
                // waiting. Cleared here rather than by whatever closed the
                // overlay, for the reason it was set here: this is the harness
                // saying so, and the overlay is only one of the ways it is asked.
                self.awaiting = None;
                let mut lines = self.flush_text();
                lines.push(theme.notice(
                    if decision == "deny" {
                        Tone::Refused
                    } else {
                        Tone::Muted
                    },
                    format!("{act} {target}{separator}{decision}"),
                ));
                lines
            }
            // **A turn ends on its answer.** Through 0.10.0 it ended on
            // `ok: finished · 0 steps · 4703 tok` — a row of arithmetic under the
            // thing the operator actually asked for, and the one line in this
            // module that put metadata in front of content rather than after it.
            // The two numbers are status-line fields now, and what is left here
            // is the half a reader cannot get anywhere else: an outcome that
            // stopped short says so, in `outcome_help`'s own sentence.
            //
            // A plain finish commits the blank line and nothing more. The blank
            // line is not decoration — it is what separates this turn's answer
            // from the next prompt, and `Screen::commit` relies on it.
            EventKind::Finished {
                outcome,
                steps,
                tokens,
            } => {
                let mut lines = self.flush_text();
                // A plain session's scrollback is its whole interface, so the two
                // numbers that moved to the status line are committed here — in
                // the status line's own words, so a reader meets `3 steps` and
                // `4703 tok` in one spelling wherever they meet them.
                if self.plain {
                    lines.push(theme.notice(
                        outcome_tone(outcome),
                        format!(
                            "{outcome}{separator}{steps} step{}{separator}{} tok",
                            if *steps == 1 { "" } else { "s" },
                            // The status line's own spelling, which is the whole
                            // point of committing this here: a plain session met
                            // `25106 tok` in the scrollback and `25.1k tok` on
                            // the line, which is one fact with two spellings.
                            crate::status::format_tokens(*tokens),
                        ),
                    ));
                }
                // A plain finish is the ordinary case and says nothing: the
                // answer above it is the outcome. Everything else stopped short
                // of one, and the word is io-harness's own — `stalled`,
                // `cancelled`, `budget_ceiling_reached` — because this interface
                // reports what the harness decided and never relabels it.
                //
                // Not in plain mode, where the row above has already said it. One
                // outcome, said once, whichever surface a reader is on.
                if !self.plain && !matches!(outcome.as_str(), "finished" | "success") {
                    lines.push(theme.notice(outcome_tone(outcome), outcome.clone()));
                }
                // The sentence, wherever the outcome was said. It exists only for
                // the outcomes an operator cannot otherwise act on, so it is
                // never printed under a plain finish.
                if let Some(help) = outcome_help(outcome) {
                    lines.push(Line::from(Span::styled(
                        format!("  {help}"),
                        theme.style(Tone::Muted),
                    )));
                }
                // One blank between turns, not two. `flush_text` already ends the
                // answer with one, and a second here is a gap an operator reads
                // as something having been left out — the ordinary turn, an
                // answer and then a prompt, is the case that has to look right.
                if !ends_blank(&lines) {
                    lines.push(Line::from(""));
                }
                lines
            }
            // **A pause the operator is watching, said rather than left blank.**
            // A retry is the one failure that looks exactly like a working
            // session: nothing arrives, the clock runs, and the interface has
            // nothing to say. `kind` is io-harness's own classification and is
            // printed as it came.
            EventKind::Retry {
                kind,
                attempt,
                delay_ms,
            } => {
                let mut lines = self.flush_text();
                lines.push(theme.notice(
                    Tone::Warning,
                    format!(
                        "the provider failed ({kind}) and will be asked again{separator}attempt \
                         {attempt}{separator}after {delay_ms} ms"
                    ),
                ));
                lines
            }
            // Who answered is not who was asked, and the status line's provider
            // field moves with it — see `App::status_from`. The line is here as
            // well as there because a fallback is a fact about *this moment* in a
            // conversation, and a field that quietly reads differently later
            // cannot say when it changed.
            EventKind::FellBackTo { provider } => {
                let mut lines = self.flush_text();
                lines.push(theme.notice(
                    Tone::Warning,
                    format!("the provider fell over{separator}{provider} answered instead"),
                ));
                lines
            }
            // The run continues, which is the half an operator would otherwise
            // assume the opposite of. `Stalled`, below, is the terminal one.
            EventKind::Replan { window } => {
                let mut lines = self.flush_text();
                lines.push(theme.notice(
                    Tone::Warning,
                    format!(
                        "nothing has changed in {window} steps, so the agent was told once to try \
                         something else"
                    ),
                ));
                lines
            }
            // **The one every ordinary session has been emitting and this one
            // has been discarding.** `TaskContract::workspace` carries
            // `StallPolicy::default()` — three steps that change nothing — so no
            // operator ever turned this on and none of them ever saw it either:
            // until 0.14.0 the fact reached its reader as a session that had
            // gone quiet, and then, once the run was already over, as the word
            // `stalled` on the outcome line. Being told after the fact that the
            // last two minutes were the agent going in circles is not the same
            // service as being told while it is happening, and the whole of F9
            // is that difference.
            //
            // **The variant carries nothing at all**, so there is no payload to
            // render and the line is composed from the run state around the
            // event instead: `RunEvent::step` is the step it stopped on, and
            // `step_at` is the session age that step opened at, set by the
            // `Step` arm above from an age the driver handed in. Neither is read
            // from a clock here, which N1 requires and which is also what makes
            // the line assertable — a test states the two ages and the interval
            // between them is arithmetic rather than timing.
            EventKind::Stalled => {
                let mut lines = self.flush_text();
                lines.push(theme.notice(
                    Tone::Warning,
                    format!(
                        "the agent is still going in circles, so the run stops \
                         here{separator}step {}{separator}{} since that step opened",
                        event.step,
                        format_millis(at.saturating_sub(self.step_at)),
                    ),
                ));
                lines
            }
            // **Durable memory is a side effect outside the workspace**, and the
            // only one this interface can show. A note written now is read by a
            // run tomorrow, so a session that never mentioned it would leave the
            // operator with no record of what the agent decided to keep.
            EventKind::MemoryWrote { key } => {
                let mut lines = self.flush_text();
                lines.push(Line::from(vec![
                    Span::styled(leader(separator), theme.style(Tone::Muted)),
                    Span::styled(format!("remembered {key}"), theme.style(Tone::Muted)),
                ]));
                lines
            }
            EventKind::MemoryForgot { key } => {
                let mut lines = self.flush_text();
                lines.push(Line::from(vec![
                    Span::styled(leader(separator), theme.style(Tone::Muted)),
                    Span::styled(format!("forgot {key}"), theme.style(Tone::Muted)),
                ]));
                lines
            }
            // A question about intent, which io-harness deliberately distinguishes
            // from an approval about permission: an answer to this authorizes
            // nothing. The overlay 0.10.0 added is where it is answered when a
            // responder is registered; this is the transcript's durable copy, and
            // it is what an operator sees at all on a turn that has no responder.
            EventKind::QuestionAsked { question, choices } => {
                let mut lines = self.flush_text();
                // **Committed only where nothing else will draw it (0.32.0).**
                // Until this release the line was committed unconditionally and
                // the overlay redrew the same question through `Tone::Warning`, so
                // an operator was asked twice and told the second time was a
                // warning. Two renderers, neither aware of the other.
                //
                // The condition is two facts and not one, and the second is the
                // one a `--plain`-only gate would have missed: an `Answerer` is
                // attached to **every** contract, so "no responder" never happens
                // in io-harness's sense — what varies is whether this process kept
                // the receiver. The `/resume` continuation drops it and then
                // drives a real turn, so a resumed run asking a new question has
                // no overlay anywhere and this line is the only thing that renders
                // it. Suppressing a question everywhere is a worse defect than
                // printing it twice.
                if self.plain || !self.answering {
                    lines.push(theme.notice(Tone::Accent, format!("the agent asks: {question}")));
                    for choice in choices {
                        lines.push(Line::from(Span::styled(
                            format!("  {} {choice}", theme.glyphs.bullet),
                            theme.style(Tone::Muted),
                        )));
                    }
                }
                lines
            }
            // **A whole ask, and the fact that it *is* one (0.33.0).** io-harness
            // 0.72.0 emits this instead of a `QuestionAsked` per question when the
            // agent asks several together, and deliberately does not emit both — so
            // an interface that knows only the singular arm above renders a batch
            // as nothing at all.
            //
            // The count is committed whatever else is on screen, and that is the
            // difference from the arm above rather than an oversight in it.
            // `crate::intent::Answerer` implements `Responder::answer` alone, so
            // io-harness's `answer_all` walks the batch and the overlay draws one
            // question at a time; nothing there says the three arrived together.
            // The questions themselves keep the singular's rule and are drawn only
            // where no overlay will draw them, because asking an operator the same
            // thing twice is the defect 0.32.0 removed and a batch would repeat it
            // once per question.
            EventKind::QuestionsAsked { questions } => {
                let mut lines = self.flush_text();
                let asked = questions.len();
                // A batch of one is a real batch — io-harness rejects an empty
                // list and accepts a single-element one — so the sentence is
                // written for it rather than saying `1 questions`.
                lines.push(theme.notice(
                    Tone::Accent,
                    if asked == 1 {
                        "the agent asks one question".to_string()
                    } else {
                        format!("the agent asks {asked} questions together")
                    },
                ));
                if self.plain || !self.answering {
                    for (index, asked_question) in questions.iter().enumerate() {
                        lines.push(Line::from(vec![
                            Span::styled(leader(separator), theme.style(Tone::Muted)),
                            // The ordinal is the whole of what makes this legible
                            // as a batch once the reader is past the heading, and
                            // it is words rather than a mark so that it survives
                            // both glyph sets unchanged.
                            Span::styled(
                                format!("{} of {asked}", index + 1),
                                theme.style(Tone::Muted),
                            ),
                            Span::styled(separator, theme.style(Tone::Muted)),
                            Span::styled(
                                asked_question.question.clone(),
                                theme.style(Tone::Normal),
                            ),
                        ]));
                        // `Question::choices` is `Vec<Choice>` from 0.72.0 — the
                        // label is what an answer is spelled with, and the rest of
                        // a `Choice` belongs to the overlay that can lay it out.
                        for choice in &asked_question.choices {
                            lines.push(Line::from(Span::styled(
                                format!("    {} {}", theme.glyphs.bullet, choice.label),
                                theme.style(Tone::Muted),
                            )));
                        }
                    }
                }
                lines
            }
            // **`by` is the fact, not the decoration.** "the machine decided" and
            // "a person decided" are different things to have happened to a run,
            // and the answer alone cannot tell them apart.
            EventKind::QuestionAnswered { answer, by } => {
                // The words avoid `answered`, which is a kind name of its own and
                // one of the six strings F2 asserts a transcript never shows.
                let who = match by.as_str() {
                    "responder" => "replied here".to_string(),
                    "human" => "replied by a person".to_string(),
                    other => format!("replied by {other}"),
                };
                let mut lines = self.flush_text();
                lines.push(Line::from(vec![
                    Span::styled(leader(separator), theme.style(Tone::Muted)),
                    Span::styled(answer.clone(), theme.style(Tone::Normal)),
                    Span::styled(separator, theme.style(Tone::Muted)),
                    Span::styled(who, theme.style(Tone::Muted)),
                ]));
                lines
            }
            // The proposal itself is the overlay's, which is on screen at the
            // moment it is made. What belongs in the scrollback is what was
            // decided, because that is the part still true afterwards.
            EventKind::PlanDecided { verdict, by, .. } => {
                let (text, tone) = match verdict.as_str() {
                    "approve" => ("the plan was approved".to_string(), Tone::Success),
                    "revise" => (
                        "the plan was sent back for revision".to_string(),
                        Tone::Warning,
                    ),
                    "cancel" => ("the plan was cancelled".to_string(), Tone::Warning),
                    // io-harness's own word, whatever it is. A verdict this
                    // release has never seen is reported rather than folded into
                    // the nearest one it knows.
                    other => (format!("the plan was {other}"), Tone::Muted),
                };
                let who = if by == "gate" { "here" } else { "by a person" };
                let mut lines = self.flush_text();
                lines.push(theme.notice(tone, format!("{text}{separator}decided {who}")));
                lines
            }
            // **The one place a reader can see why.** io-harness does not fold
            // thinking back into the next prompt and does not store it, so this
            // event is the only place it is ever visible — an absent one means the
            // model did not think, never that it thought nothing.
            //
            // The heading says `thought` rather than `reasoning`, which is the
            // variant's own name and one of the six strings F2 asserts a
            // transcript never shows.
            EventKind::Reasoning { text, tokens } => {
                // A thought with nothing in it is not a thought. The provider
                // billed for it and returned no text, and a heading over an
                // empty block would say the model thought nothing rather than
                // that it did not say what it thought.
                if text.trim().is_empty() {
                    return self.flush_text();
                }
                // **One row: that it thought, how long for, what it cost.** The
                // text itself is kept and not committed. A thought is the model
                // talking to itself, it is usually longer than the answer it
                // precedes, and a transcript that carried every one of them is a
                // transcript with the work buried in the deliberation — which is
                // what a real session showed and what the owner asked to stop.
                //
                // Kept rather than dropped, because this event is the only place
                // reasoning is ever visible: io-harness neither stores it nor
                // folds it into the next prompt. `/expand` is where it goes.
                let elapsed = format_millis(at.saturating_sub(self.step_at));
                self.thought = Some(text.clone());
                let mut lines = self.flush_text();
                lines.push(Line::from(vec![
                    Span::styled(leader(separator), theme.style(Tone::Muted)),
                    // Italic, which is what a thought is: the model's own voice
                    // rather than the interface's, and set apart from the tool
                    // cells around it without spending a colour on it.
                    Span::styled(
                        "thought",
                        theme.style(Tone::Muted).add_modifier(Modifier::ITALIC),
                    ),
                    Span::styled(
                        format!("{separator}{elapsed}{separator}{tokens} tok"),
                        theme.style(Tone::Muted),
                    ),
                ]));
                lines
            }
            // **Nothing in this process dialled anything.** The provider ran its
            // own search or fetch and reported it, so the line says which provider
            // and which of its tools — and reports the failures, because a search
            // that broke inside an otherwise good answer is why a reply is thin.
            EventKind::ServerToolUsed { provider, tool, ok } => {
                let mut lines = self.flush_text();
                lines.push(Line::from(vec![
                    Span::styled(leader(separator), theme.style(Tone::Muted)),
                    Span::styled(
                        format!("{provider} ran {tool} for the model"),
                        theme.style(Tone::Normal),
                    ),
                    Span::styled(separator, theme.style(Tone::Muted)),
                    Span::styled(
                        if *ok { "ok" } else { "failed" },
                        theme.style(if *ok { Tone::Muted } else { Tone::Warning }),
                    ),
                ]));
                lines
            }
            // The reasons are carried rather than summarised, for io-harness's own
            // reason: a refusal a human cannot argue with is a gate nobody trusts
            // twice. They are the operator's only account of why the work was
            // judged insufficient — the store has the same sentence joined by
            // semicolons, and a session that dropped them would be sending
            // somebody to a database to read a paragraph the run already said.
            //
            // **The failing verdict is a warning and not `Tone::Refused`, from
            // 0.24.0.** That tone carries the literal word `refused`, which
            // everywhere else in this transcript means the permission boundary
            // stopped an act; a reviewer that read the work and judged it did not
            // stop anything, and giving both the same word would make the two
            // things an operator most needs to tell apart — the policy would not
            // run my gate, versus my work did not meet the bar — read identically.
            //
            // **A review that did not happen is not drawn here, because it is not
            // emitted.** io-harness sends this event only for a verdict somebody
            // gave; a transport failure or an unparseable answer emits nothing and
            // is `GateOutcome::Errored` in the store. So this line always means a
            // review that ran, and its absence is not evidence that one passed.
            EventKind::Reviewed { passed, reasons } => {
                let mut lines = self.flush_text();
                lines.push(theme.notice(
                    if *passed {
                        Tone::Success
                    } else {
                        Tone::Warning
                    },
                    if *passed {
                        "the review passed"
                    } else {
                        "the review ran and did not pass"
                    },
                ));
                for reason in reasons {
                    lines.push(Line::from(Span::styled(
                        format!("  {} {reason}", theme.glyphs.bullet),
                        theme.style(Tone::Muted),
                    )));
                }
                lines
            }
            EventKind::Mcp {
                server,
                tool,
                millis,
                ..
            } => {
                // The one place io-harness reports how long a tool actually ran.
                // It is harvested onto the open cell rather than printed here, so
                // that an MCP call closes with a measured duration while every
                // other call closes with io-cli's observation of the interval —
                // the difference the `~` prefix exists to show.
                //
                // Matched by rebuilding io-harness's own namespaced tool name,
                // from io-harness's own two constants rather than from a spelling
                // of them typed here, which is what `announce` puts on the
                // `ToolCall`. Newest first, because a batch may hold two calls to
                // the same tool and the later one is the one still running.
                //
                // A connect, a discover or a disconnect carries no tool and no
                // duration, and matches nothing.
                // **Matched on the wire name and never on the drawn one.** From
                // 0.34.0 an MCP cell is drawn as a verb and a translated target,
                // so the field this used to compare against no longer holds the
                // string being rebuilt here. `raw` is io-harness's own name for
                // the tool and is the only field that still does — and getting
                // this wrong loses the measurement silently, on the one event in
                // the whole enum that reports how long a tool actually ran.
                if let (Some(tool), Some(millis)) = (tool, millis) {
                    let name = format!("{MCP_TOOL_PREFIX}{server}{NAMESPACE}{tool}");
                    if let Some(call) = self.open.iter_mut().rev().find(|c| c.raw == name) {
                        call.measured = Some(Duration::from_millis(*millis));
                    }
                }
                // And nothing committed. 0.3.0 rendered the muted word `mcp` here
                // and said in a comment that harvesting a number off an event is
                // not the same as designing a line for it — which was right while
                // there was nowhere else for the fact to go. There is now:
                // `mcp N/M tools` on the status line, since 0.10.0, is where a
                // server reaching a run and a tool it offered both land, and
                // `triage::TRIAGE` records that route.
                Vec::new()
            }
            // **What isolated the work, and how the criterion it was isolated
            // for answered, said in the operator's words rather than inferred
            // from a tool cell that happened to succeed.** Seven kinds reach this
            // channel and six of them are drawn.
            //
            // **`dial` is the one that is not.** io-harness builds it as a
            // `destroy` event with the kind overwritten and emits it immediately
            // beside `EventKind::Dialed` for the same outbound connection, so a
            // session drawing both would put every dial in the transcript twice —
            // and the copy here is the poorer one, carrying the word and nothing
            // else where the dial itself carries the host, the port and the
            // verdict. An empty `Vec` is this module's own word for "nothing yet"
            // and is the honest way to say it: a caller cannot tell it from a
            // dropped event because there is nothing there to tell apart.
            //
            // **`gate_phase_failed` and `gate_output` are drawn from 0.24.0.**
            // Before this release every contract this crate built left
            // `Verification::None` on it, so neither kind could arrive in a
            // session and neither was given a sentence written in advance of the
            // release that could check it. An operator now configures one
            // criterion — `[app.io-cli.gates]`, resolved by `crate::gates` —
            // io-harness runs it after the agent stops, and these two are the
            // whole of what it says about a criterion that did not hold on the
            // channel a session is already watching. A verdict that lives only in
            // the store is a verdict nobody reads.
            //
            // **Neither line carries the fact that made it worth emitting, and
            // that is io-harness's shape rather than a choice made here.** The
            // failing phase and the failing command's bounded output are both in
            // `SandboxEvent::detail`, and `EventKind::Sandbox` carries the kind
            // and the backend alone. So a session learns that the gate ran and
            // said no, and the text stays in the run's `sandbox_events` rows
            // where a diagnosis reads it. Naming a phase here, or printing an
            // empty string as though it were the command's output, would be this
            // module writing the half it was not given.
            //
            // **A gate that did not hold is not a refusal, and neither line says
            // that word.** `Tone::Refused` in this transcript means the
            // permission boundary stopped an act — which is exactly what a
            // criterion whose program the policy will not run produces: an
            // `EventKind::Refused` with act `exec`, and no gate event at all,
            // because nothing ran and nothing judged anything. That is
            // `GateOutcome::Errored` in the store, the one verdict io-harness
            // will retry. A gate line therefore only ever stands for a criterion
            // that ran and answered, and the two need opposite responses from
            // whoever is reading — fix the policy, or fix the work — so they are
            // told apart by the words and never by a colour.
            //
            // **`cap_hit` is a limit reached and not a failure.** The sandbox
            // did exactly what its configuration told it to; reporting that
            // through the error path would tell an operator their run broke at
            // the moment their cap held, which is the opposite of what happened
            // and the opposite of the reason they set one.
            //
            // The backend is carried where the event has one and never invented
            // where it does not. io-harness sets it on `create` and `exec`
            // alone — `SandboxEvent::cap_hit` and `SandboxEvent::destroy` both
            // write `None`, always — so the `Option` is rendered as it arrives
            // rather than filled in with a name this module worked out for
            // itself.
            EventKind::Sandbox { kind, backend } => {
                // The sentence and its tone decided together, in one place, so
                // that a kind cannot be given a line here and a weight somewhere
                // else that disagrees with it.
                let (said, tone) = match kind.as_str() {
                    "create" => ("a sandbox was created", Tone::Muted),
                    "exec" => ("a command ran in the sandbox", Tone::Muted),
                    "cap_hit" => ("the sandbox reached a limit it was given", Tone::Warning),
                    "destroy" => ("the sandbox was torn down", Tone::Muted),
                    // `ran and` is load-bearing: it is the whole of what
                    // separates a criterion that judged the work from one that
                    // never got to.
                    "gate_phase_failed" => ("the gate ran and did not pass", Tone::Warning),
                    "gate_output" => ("the gate command printed output", Tone::Muted),
                    _ => return Vec::new(),
                };
                let mut text = said.to_string();
                if let Some(backend) = backend {
                    text.push_str(&format!("{separator}{backend}"));
                }
                let mut lines = self.flush_text();
                // A tone that carries a word writes its own line at the left
                // margin, like every other notice in this module; one that does
                // not takes the muted leader, so an unweighted fact sits in the
                // same column as the tool cells it belongs among.
                lines.push(if tone.word().is_some() {
                    theme.notice(tone, text)
                } else {
                    Line::from(vec![
                        Span::styled(leader(separator), theme.style(Tone::Muted)),
                        Span::styled(text, theme.style(Tone::Normal)),
                    ])
                });
                lines
            }
            // **The one place in this product where a contained command's egress
            // is an observation rather than an inference.** A sandbox denies
            // egress structurally — the backend gives the child no route out —
            // so until io-harness put a loopback proxy in the route there was no
            // attempt to see, and the proxy decides by applying the run's own
            // `Policy` to `host:port`, which is the same rule that refuses this
            // crate's own network tools reaching a second caller.
            //
            // **The host as the command asked for it, and never an address.**
            // io-harness carries the unresolved name for the reason this line
            // prints it: the policy's patterns are written against names, so a
            // row showing `140.82.121.4` would not match the rule that decided
            // it and its reader could not tell which rule to change.
            //
            // A refusal is `Tone::Refused` and not `Tone::Error`, for the reason
            // that tone exists: nothing broke, the boundary worked. A permitted
            // dial says so in a word rather than in a colour alone, because a
            // colour is nothing under `NO_COLOR`, on a monochrome terminal or to
            // a screen reader.
            //
            // **An absent dial line is not evidence of no egress**, and nothing
            // in this interface should be read as though it were. The event has
            // one emit site behind three conjoined preconditions — the turn is
            // contained, the policy `names_hosts()`, and the selected backend
            // reaches the proxy — so a permissive or all-or-nothing policy names
            // no host and emits none of these ever, and a proxy that fails to
            // bind is logged and dropped while the run carries on.
            EventKind::Dialed {
                host,
                port,
                allowed,
            } => {
                let mut lines = self.flush_text();
                lines.push(if *allowed {
                    Line::from(vec![
                        Span::styled(leader(separator), theme.style(Tone::Muted)),
                        Span::styled(format!("dialled {host}:{port}"), theme.style(Tone::Normal)),
                        Span::styled(separator, theme.style(Tone::Muted)),
                        Span::styled("permitted", theme.style(Tone::Muted)),
                    ])
                } else {
                    theme.notice(Tone::Refused, format!("dialled {host}:{port}"))
                });
                lines
            }
            // Guarded on the items rather than only on the tag, because io-harness
            // accepts a write of none: `parse_todo_items` validates each item it is
            // given and never rejects an empty list, so `{"items": []}` dispatches
            // as a real `TodoWrote`. A header reading `0 of 0 done` over nothing at
            // all is the placeholder F12's sabotage arm names, arriving through the
            // transcript's door instead of the status line's. An empty write falls
            // through to the catch-all below and commits the muted `todo_wrote`
            // word: the event still happened, and this module never drops one.
            EventKind::TodoWrote { items } if !items.is_empty() => {
                // The plan commits after whatever streamed before it, so the prose
                // that led up to it is above rather than below. Not under the
                // `todo_write` cell that announced it: `ToolCall` commits nothing
                // and only holds the call open, and the next `Step` writes the cell
                // — so the cell lands *after* this list, and the transcript reads
                // plan, then the call that wrote it. Reordering that would mean
                // committing an open call early, which is the one thing `live()`
                // and every other tool cell in this module depend on not happening.
                let mut lines = self.flush_text();

                // io-harness's own arithmetic for a done count, and io-harness's
                // own caveat with it: nothing in the core verifies an item, so
                // this is what the agent says about its own work rather than a
                // checked fact, and the header says so in those words.
                let done = items
                    .iter()
                    .filter(|item| item.state == TodoState::Done)
                    .count();
                lines.push(Line::from(Span::styled(
                    format!(
                        "  plan{separator}{done} of {} done, by the agent's own account",
                        items.len(),
                    ),
                    theme.style(Tone::Muted),
                )));

                // A bullet, two spaces of indent and a space after the mark — the
                // same leader a tool cell wears, because both are one row of a
                // list under a heading.
                let bullet_leader = theme.glyphs.bullet.chars().count() + 3;
                for item in items {
                    // io-harness's own three words, from `TodoState::as_str`, and
                    // not a spelling io-cli invented: they are the wire form the
                    // model wrote and the column the store holds. A word rather
                    // than only a tone, because a colour is nothing under
                    // `NO_COLOR`, on a monochrome terminal or to a screen reader.
                    let state = item.state.as_str();
                    let tone = match item.state {
                        TodoState::Done => Tone::Success,
                        TodoState::Active => Tone::Accent,
                        TodoState::Pending => Tone::Muted,
                    };
                    // What is left of the row once the leader, the separator and
                    // the state word have taken theirs. Counted in characters,
                    // never in bytes.
                    let taken = bullet_leader + separator.chars().count() + state.chars().count();
                    let room = ROW.saturating_sub(taken);
                    lines.push(Line::from(vec![
                        Span::styled(
                            format!("  {} ", theme.glyphs.bullet),
                            theme.style(Tone::Muted),
                        ),
                        Span::styled(
                            fit(&item.text, room, &theme.glyphs),
                            theme.style(Tone::Normal),
                        ),
                        Span::styled(separator, theme.style(Tone::Muted)),
                        Span::styled(state, theme.style(tone)),
                    ]));
                }

                // The event carries the model's list *before* the store's cap is
                // applied: the dispatcher clones `items` and only then does
                // `Store::write_todos` keep `TODO_MAX_ITEMS` of them, and the
                // dropped count reaches no event at all. So this line is the only
                // place the whole length is knowable, and an operator not told
                // here would read a plan of sixty-four and never learn the agent
                // wrote more.
                if items.len() > TODO_MAX_ITEMS {
                    lines.push(theme.notice(
                        Tone::Warning,
                        format!(
                            "the agent wrote {} items; the run's store keeps the first \
                             {TODO_MAX_ITEMS}, so the last {} are in this transcript and \
                             nowhere else",
                            items.len(),
                            items.len() - TODO_MAX_ITEMS,
                        ),
                    ));
                }
                lines.push(Line::from(""));
                lines
            }
            // **What starting early bought, and what it cost.** 0.27.0, and the
            // one silence in `triage::TRIAGE` that had no route to a surface an
            // operator uses: it went to `io exec --json` and the durable trace,
            // which are both places somebody goes deliberately, afterwards,
            // already suspecting something.
            //
            // `discarded` is the figure worth the line. A read started before the
            // model had finished asking and then thrown away is work that was paid
            // for and not used, and it is the only number here an operator can act
            // on — by turning speculation off. `started` alone would read as a
            // brag about concurrency.
            //
            // **Only when something was discarded.** io-harness emits this
            // whenever `started > 0` (`run/step.rs:1272`), so a step that
            // speculated perfectly would otherwise put a line in every transcript
            // saying nothing happened. A run where speculation always pays is a
            // run with nothing to report, which is the same rule the sandbox arm
            // above follows for a cap that was not hit.
            EventKind::Speculated {
                started,
                used,
                discarded,
            } if *discarded > 0 => {
                let mut lines = self.flush_text();
                lines.push(Line::from(Span::styled(
                    format!(
                        "{}read ahead{separator}{used} of {started} used, {discarded} discarded",
                        leader(separator),
                    ),
                    theme.style(Tone::Muted),
                )));
                lines
            }
            // Every kind that commits no line of its own, and the one case that
            // is a defect rather than a decision.
            //
            // Three groups arrive here. A `Status` kind, whose fact is a field —
            // `App::status_from` is that surface. A `Silent` kind, whose route is
            // written down in `triage::TRIAGE` beside it. And a `Line` kind whose
            // arm above declined *this* event: an empty plan is the one that
            // exists today, and a disposition is about the kind rather than about
            // every payload it can carry.
            //
            // The fourth case is a kind the table has never heard of, which is
            // the only one that means something is wrong. It is counted rather
            // than printed, because printing it is what this release removed.
            other => self.undesigned(&kind_name(other)),
        }
    }
}

/// The muted leader an unstyled event line starts with: two spaces of indent,
/// the separator's own mark, then a space.
///
/// The mark is trimmed out of the separator rather than written again, so there
/// is one of it in the product and not two. An event line and the status line
/// under it are meant to read as one surface, and two spellings of the same mark
/// is how that stops being true.
fn leader(separator: &str) -> String {
    format!("  {} ", separator.trim())
}

/// The indent for an event at `depth`, so a tree reads as a tree.
///
/// Two spaces a level, and nothing at the root — which is every event in a
/// session that configures no containment, so this costs an unconstrained
/// session exactly nothing. Not a glyph: a box-drawing tree would need to know
/// what comes next to close a branch, and the stream is arriving.
fn nest(depth: u32) -> String {
    "  ".repeat(depth as usize)
}

/// One committed tool cell: the tool, its target, what came back, how long it
/// took.
///
/// Content before metadata, like every other line in this interface, and every
/// fact in words rather than in colour — the result reads the same under
/// `NO_COLOR` and in a screen reader as it does on a colour terminal.
///
/// `at` is the session age this cell is being closed at, or `None` when the cell
/// is being closed without anything having reported on it.
/// A step's decision with the words the cell has already said taken off the
/// front.
///
/// Leading tokens are dropped while they are the tool's own name — in either
/// vocabulary — or the target the cell is already showing. Only leading ones: a
/// sentence that mentions the file again halfway through is the harness saying
/// something, and this is not in the business of editing it.
fn trim_result(result: &str, call: &Pending) -> String {
    /// Quotes, brackets and case dropped, so `"model =` and `model =` are the
    /// same word to this comparison and only the letters and digits decide.
    fn plain(text: &str) -> String {
        text.chars()
            .filter(|c| c.is_alphanumeric() || matches!(c, '.' | '/' | '_' | '-'))
            .collect::<String>()
            .to_lowercase()
    }

    // **Both spellings of the target, because the cell shows one and the sentence
    // is written in the other.** io-harness writes its decision in its own
    // vocabulary, so a translated target leaves the sentence saying
    // `bundle__skill` while the cell says `bundle:skill`. Comparing against the
    // displayed form alone stopped recognising the repetition the moment 0.32.0
    // introduced a translation — and `plain` keeps `_` while dropping `:`, so the
    // two sides do not merely differ by a character, they differ by the separator
    // itself, which is the whole string being hidden.
    let said = format!(
        "{}{}{}{}",
        plain(&call.raw),
        plain(&call.name),
        plain(&call.target),
        plain(&call.target_raw)
    );
    let mut rest = result.trim_start();
    while let Some((head, tail)) = rest.split_once(char::is_whitespace) {
        let head = plain(head);
        // A token of pure punctuation — the `=` of `model =` — is dropped only
        // while everything before it has been, so it goes with the words it
        // belongs to and never off the front of a real result.
        if !head.is_empty() && !said.contains(&head) {
            break;
        }
        rest = tail.trim_start();
    }
    // The whole of it was the tool and its target, so the cell has said it all.
    let last = plain(rest);
    if last.is_empty() || said.contains(&last) {
        return String::new();
    }
    // And the whole target again at the END is the same repetition at the other
    // end: `Write notes.md · wrote notes.md` says the file twice on one row.
    //
    // The WHOLE target, never its last word. Stripping one token turned
    // `Run cargo test · ran cargo test` into `ran cargo`, which is a sentence
    // this interface made up out of one the harness wrote.
    //
    // Tried in both spellings for the same reason `said` holds both: the sentence
    // ends in the words io-harness chose, which for a translated target are not
    // the words the cell is showing.
    for tail in [call.target.as_str(), call.target_raw.as_str()] {
        if tail.is_empty() {
            continue;
        }
        if let Some(head) = rest.strip_suffix(tail) {
            let head = head.trim_end();
            if !plain(head).is_empty() {
                return head.to_string();
            }
        }
    }
    rest.to_string()
}

/// Whether the last line of `lines` is blank, or there are none.
///
/// `true` for an empty slice on purpose: nothing committed means nothing to
/// separate from, and a blank row opening a turn's committed output would push
/// the first thing it says down for no reason.
fn ends_blank(lines: &[Line<'static>]) -> bool {
    lines
        .last()
        .is_none_or(|line| line.spans.iter().all(|span| span.content.trim().is_empty()))
}

/// A `read_skill` decision split into the skill that loaded and what it read.
///
/// io-harness writes this decision as the skill's own name and, when the call
/// carried one, the companion file's relative path — two words, in that order.
/// The first is a *name* by construction, which makes it the one place the
/// display translation is certainly right rather than probably right: the live
/// row has to guess from the target's shape ([`names_a_skill`]) because no
/// sentence has arrived yet, and this does not have to guess at all.
///
/// `None` when the decision is not in that shape, which is every case where the
/// harness did not write a sentence for this call — a refusal, a step verdict
/// standing in for one, or a cell closed unfinished. Those keep the ordinary
/// cell, because a row asserting a skill loaded is exactly what must not be
/// drawn when the read did not happen.
fn skill_and_file(decision: &str) -> Option<(String, String)> {
    /// io-harness's own sentence for a skill that was read.
    const READ: &str = "read skill ";
    /// And for one that could not be. Both are matched rather than guessed at,
    /// because the difference between them is the difference between a row that
    /// says a skill loaded and a row that says so wrongly.
    const FAILED_HEAD: &str = "skill ";
    const FAILED_TAIL: &str = " read error";

    let decision = decision.trim();
    let (label, failed) = if let Some(label) = decision.strip_prefix(READ) {
        (label, false)
    } else if let Some(rest) = decision.strip_prefix(FAILED_HEAD) {
        (rest.strip_suffix(FAILED_TAIL)?, true)
    } else {
        return None;
    };
    // The label is the skill's name and, when the call carried one, the companion
    // file's relative path — io-harness builds it that way and writes it into the
    // decision, the observation header and the supersede target alike.
    let (skill, file) = match label.split_once(char::is_whitespace) {
        Some((skill, rest)) => (skill, rest.trim()),
        None => (label, ""),
    };
    if skill.is_empty() {
        return None;
    }
    let file = match file {
        "." => LISTED,
        other => other,
    };
    let detail = match (failed, file) {
        (false, "") => LOADED.to_string(),
        (false, file) => file.to_string(),
        (true, "") => FAILED_TAIL.trim().to_string(),
        (true, file) => format!("{file}{FAILED_TAIL}"),
    };
    Some((crate::naming::display(skill), detail))
}

fn cell_line(
    theme: Theme,
    call: &Pending,
    result: &str,
    at: Option<Duration>,
    paired: bool,
) -> Line<'static> {
    let separator = theme.glyphs.separator;
    // **A loaded skill is drawn as a loaded skill, in io-cli's own words.**
    //
    // This is what removes the separator at its source rather than filtering it
    // downstream: io-harness's decision sentence for this tool is not drawn at
    // all, so the string that used to carry the separator into the transcript is
    // no longer a thing the cell prints. The two facts a reader wants — which
    // skill, and what came of it — are taken from that sentence and said once
    // each.
    //
    // Gated on `paired`, so the sentence being read really is this call's. A
    // refusal, a step verdict or an unfinished flush all fall through to the
    // ordinary cell, because `loaded` is an assertion and the one thing worse
    // than an unreadable row is a confident wrong one.
    if paired && call.raw == READ_SKILL {
        if let Some((skill, file)) = skill_and_file(result) {
            let detail = if file.is_empty() {
                LOADED.to_string()
            } else {
                file
            };
            return with_duration(
                theme,
                vec![
                    Span::styled(
                        format!("  {} ", theme.glyphs.bullet),
                        theme.style(Tone::Muted),
                    ),
                    Span::styled(
                        call.name.clone(),
                        theme.style(Tone::Accent).add_modifier(Modifier::BOLD),
                    ),
                    Span::styled(format!(" {skill}"), theme.style(Tone::Muted)),
                    Span::styled(separator, theme.style(Tone::Muted)),
                    Span::styled(detail, theme.style(Tone::Normal)),
                ],
                call,
                at,
            );
        }
    }
    let mut spans = vec![
        Span::styled(
            format!("  {} ", theme.glyphs.bullet),
            theme.style(Tone::Muted),
        ),
        // The verb carries the weight: it is the column a reader skims down, and
        // in a run of eight cells it is the only part that differs at a glance.
        Span::styled(
            call.name.clone(),
            theme.style(Tone::Accent).add_modifier(Modifier::BOLD),
        ),
    ];
    // `!= call.name` because io-harness falls the target back to the tool's own
    // name when the call carries no path, pattern or key — so a `git_diff` with
    // no argument arrived as `git_diff git_diff`, which reads like a stutter.
    // Found in a live run, not in the suite.
    if !call.target.is_empty() && call.target != call.name {
        spans.push(Span::styled(
            format!(" {}", call.target),
            theme.style(Tone::Muted),
        ));
    }
    // **What the result adds, and not what it repeats.** io-harness writes a
    // step's decision in its own vocabulary — `read io.toml`, `list_dir  (4
    // entries)` — and the cell has already said the tool and the target in the
    // operator's. Printed whole, a cell read `Read io.toml · read io.toml`.
    // Stripped, it reads `Read io.toml` and `List · (4 entries)`: the tool once,
    // the target once, and whatever the harness added kept in full.
    let result = trim_result(result, call);
    if !result.is_empty() {
        spans.push(Span::styled(separator, theme.style(Tone::Muted)));
        spans.push(Span::styled(result, theme.style(Tone::Normal)));
    }

    with_duration(theme, spans, call, at)
}

/// A cell's spans with its duration appended, whichever shape the cell took.
///
/// Shared by the ordinary cell and the loaded-skill row so that the two kinds of
/// number below are told apart in one place. A second copy of this reasoning is
/// how one of the two rows ends up printing an observation as though it were a
/// measurement.
///
/// Two different kinds of number, told apart on the line itself. A measured
/// duration is io-harness's own and is printed plainly; anything else is the
/// interval io-cli observed between two events — which includes the model's own
/// turnaround and the queue in front of the tool — and wears a `~` to say that it
/// is an observation rather than how long the tool ran.
///
/// `at` is `None` when the cell is closed with nothing having reported on it.
/// io-cli does not know a duration there and says none, rather than printing the
/// age of the announcement as though it were one.
fn with_duration(
    theme: Theme,
    mut spans: Vec<Span<'static>>,
    call: &Pending,
    at: Option<Duration>,
) -> Line<'static> {
    let separator = theme.glyphs.separator;
    let observed = at.map(|at| format!("~{}", format_millis(at.saturating_sub(call.opened_at))));
    if let Some(duration) = call.measured.map(format_millis).or(observed) {
        spans.push(Span::styled(
            format!("{separator}{duration}"),
            theme.style(Tone::Muted),
        ));
    }
    Line::from(spans)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A call as `announce` would have built it, for the two functions below.
    ///
    /// In-crate because both are private, and they have to be: `trim_result` is
    /// the comparison this release repairs and `skill_and_file` is the parse that
    /// decides whether a row may say a skill loaded. A criterion about either is
    /// gated by nothing if the only way to reach it is through a rendered line
    /// that also exercises six other decisions.
    fn call(raw: &str, name: &str, target: &str, target_raw: &str) -> Pending {
        Pending {
            name: name.to_string(),
            raw: raw.to_string(),
            target: target.to_string(),
            target_raw: target_raw.to_string(),
            opened_at: Duration::ZERO,
            measured: None,
        }
    }

    /// **F3 — the exact pair from the live session that opened this release.**
    ///
    /// 0.32.0 translated the cell's target and left io-harness's sentence beside
    /// it in the untranslated vocabulary, so the comparison that exists to drop a
    /// repetition stopped recognising one — and printed in full the single string
    /// the translation had been added to hide.
    #[test]
    fn f3_a_decision_repeating_a_translated_target_is_dropped() {
        let call = call(
            READ_SKILL,
            "Skill",
            "ultraship:using-ultraship",
            "ultraship__using-ultraship",
        );
        assert_eq!(
            trim_result("read skill ultraship__using-ultraship", &call),
            "",
            "the sentence only repeats the cell and must leave the result column \
             empty, in either vocabulary",
        );
    }

    /// And the displayed spelling is dropped too, so the repair does not depend
    /// on which of the two io-harness happens to have written.
    #[test]
    fn f3_a_decision_repeating_the_displayed_target_is_also_dropped() {
        let call = call(
            READ_SKILL,
            "Skill",
            "ultraship:using-ultraship",
            "ultraship__using-ultraship",
        );
        assert_eq!(trim_result("read skill ultraship:using-ultraship", &call), "");
    }

    /// **F4 — and a genuine result is still not stripped.**
    ///
    /// The whole target at the end is a repetition; one token of it is a sentence
    /// this interface made up out of one the harness wrote. `Run cargo test · ran
    /// cargo` was that mistake.
    #[test]
    fn f4_a_genuine_result_survives_and_a_whole_repeated_target_does_not() {
        // **`ran`, not the empty string.** The cell has already said `Run` and
        // `cargo test`, so what the sentence *adds* is the one word `ran`, and
        // that is what it carries — the rule is "what the result adds, and not
        // what it repeats", never "drop a sentence that mentions the target".
        // 0.34.0's contract said this column was empty; the behaviour it
        // describes has been asserted since 0.11.0 and is the one that is right.
        let run = call("shell", "Run", "cargo test", "cargo test");
        assert_eq!(trim_result("ran cargo test", &run), "ran");
        let list = call("list_dir", "List", "src", "src");
        assert_eq!(trim_result("list_dir  (4 entries)", &list), "(4 entries)");
    }

    /// **The success sentence and the failure sentence are told apart.**
    ///
    /// A row saying a skill loaded is an assertion, so it is made only from the
    /// sentence io-harness writes when one did.
    #[test]
    fn a_loaded_skill_is_parsed_and_a_failed_read_is_not_mistaken_for_one() {
        assert_eq!(
            skill_and_file("read skill ultraship__plan"),
            Some(("ultraship:plan".to_string(), LOADED.to_string())),
        );
        assert_eq!(
            skill_and_file("read skill ultraship__plan shared/principles.md"),
            Some((
                "ultraship:plan".to_string(),
                "shared/principles.md".to_string()
            )),
        );
        // An empty `path` lists the bundle, which io-harness spells `.`.
        assert_eq!(
            skill_and_file("read skill ultraship__plan ."),
            Some(("ultraship:plan".to_string(), LISTED.to_string())),
        );
        // The failure sentence says the read failed and never says `loaded`.
        assert_eq!(
            skill_and_file("skill ultraship__plan read error"),
            Some(("ultraship:plan".to_string(), "read error".to_string())),
        );
        assert_eq!(
            skill_and_file("skill ultraship__plan missing.md read error"),
            Some((
                "ultraship:plan".to_string(),
                "missing.md read error".to_string()
            )),
        );
        // Anything else is not this tool's sentence and takes the ordinary cell:
        // a refusal, a step verdict standing in for a sentence, an unfinished
        // flush. None of the three may become a row claiming a skill loaded.
        for other in ["refused", "no change", "changed files", "unfinished", ""] {
            assert_eq!(
                skill_and_file(other),
                None,
                "{other:?} is not io-harness's sentence for a skill that was read",
            );
        }
    }

    /// **F11 — a companion path is not a name, and the pin made that a live
    /// question rather than a theoretical one.**
    ///
    /// io-harness 0.73.0 announces the path in preference to the skill's name, so
    /// the target this tool arrives with is now sometimes a file. Translating one
    /// draws a path that does not exist.
    #[test]
    fn f11_a_companion_path_is_never_taken_for_a_skills_name() {
        for name in ["ultraship__plan", "brainstorm", "a__b"] {
            assert!(
                names_a_skill(name),
                "{name} is a skill name and must still be translated",
            );
        }
        for path in [
            "references/__init__.py",
            "shared/principles.md",
            "__init__.py",
            "NOTES.md",
            "a\\b__c.md",
            "",
        ] {
            assert!(
                !names_a_skill(path),
                "{path} is a file and translating it draws a path that does not exist",
            );
        }
    }
}

/// `420ms`, `1.4s`, `92.0s`. A tool cell's own duration.
///
/// Separate from [`format_elapsed`](crate::status::format_elapsed), which floors
/// to whole seconds and would print `0s` for every tool call that took less than
/// one — which is most of them. Two formats because they answer two questions:
/// how long the session has been open, and how long one call took.
fn format_millis(duration: Duration) -> String {
    let millis = duration.as_millis();
    if millis < 1_000 {
        return format!("{millis}ms");
    }
    format!("{:.1}s", duration.as_secs_f64())
}

/// How a run's outcome should read.
///
/// The vocabulary is io-harness's, and the distinction that matters here is
/// between `success` and `finished`: `success` means a verification criterion
/// passed, and `finished` means a run with no criterion ended on its own terms.
///
/// **Both are reachable from 0.24.0, and until this release only one was.** Every
/// contract this crate built left `Verification::None` on it, so every io-cli
/// turn ended `finished` and `success` could not be produced from this interface
/// at all. An operator now configures one criterion — see [`crate::gates`] — and
/// a turn that carries one ends `success` when it holds. Neither word is a
/// failure and both are drawn as the same thing; what separates them is how much
/// the run is claiming, and that claim is the operator's to read.
///
/// Found in a live run, not in the suite: treating anything that was not
/// `success` as a warning meant every ordinary, completely successful turn ended
/// the transcript with the word "warning".
pub fn outcome_tone(outcome: &str) -> Tone {
    match outcome {
        // Ended well, with or without a criterion to end against.
        "success" | "finished" => Tone::Success,
        // Somebody or something stopped it deliberately. Not a failure, and not
        // nothing either — the work did not complete.
        "cancelled" | "denied" | "refused" | "plan_rejected" | "stalled" => Tone::Warning,
        // **A ceiling, which is not a failure — 0.14.0's F6.** All four of these
        // are `Ok` in io-harness and always have been: a run that spends its
        // steps, its seconds or its tokens returns a `RunOutcome` saying which,
        // and nothing went wrong. Three of the four fell through to `Tone::Error`
        // until this release, so what an operator met under a half-finished
        // answer was `error: step_cap_reached` — a ceiling reported as a crash,
        // and the exact sentence `src/contract.rs` names as the reason
        // `contract::MAX_STEPS` exists at all. Raising the cap made it rarer
        // without making it right, and 0.14.0 hands the operator budgets of their
        // own to reach, so the vocabulary has to be correct before the feature
        // ships rather than after.
        //
        // `budget_ceiling_reached` was already here, alone, which is what made
        // its three siblings read as a different class of event from a fact they
        // are indistinguishable from.
        //
        // **The word stays io-harness's own.** This interface reports what the
        // harness decided and never relabels it; what changed is the tone it is
        // said in, and `outcome_help` below is the sentence that says what each
        // of them means here.
        "step_cap_reached"
        | "time_budget_exceeded"
        | "cost_budget_exceeded"
        | "budget_ceiling_reached" => Tone::Warning,
        // Waiting on a human this release has no way of asking. A warning rather
        // than an error: nothing went wrong, the run simply cannot go on from
        // here, and `outcome_help` is what says so on screen.
        "awaiting_answer" | "awaiting_approval" | "awaiting_plan" => Tone::Warning,
        // The run gave up and wants a human. Anything unrecognised lands here too:
        // an outcome this release has never seen is better over- than
        // under-reported.
        _ => Tone::Error,
    }
}

/// A sentence to print under an outcome the operator cannot otherwise act on.
///
/// A turn that ends waiting for a human is a dead end in this release: the
/// approval overlay is 0.2.0 and answering a question is 0.7.0, so there is
/// nothing on screen that can resolve it. Saying only "awaiting_answer" leaves
/// somebody stuck with no next action, which a live first run found by walking
/// straight into it — the agent was denied three times, asked for permission,
/// and the session had no way to give it.
pub fn outcome_help(outcome: &str) -> Option<&'static str> {
    match outcome {
        // **Both of the sentences that stood here were false, and had been since
        // 0.23.0.** They told an operator that a run waiting on an answer or a
        // plan could not be answered by this release and to say it in the next
        // prompt — while `Intent::resumed`, `Review::resumed` and the `/resume`
        // continuation had all existed for nine releases. The product was
        // documenting the absence of a capability it shipped, in the one place an
        // operator reads when they are stuck.
        "awaiting_answer" => Some(
            "the agent asked what you meant and the turn ended before it was \
             answered. `/resume` reopens the question and carries the run on from \
             where it stopped.",
        ),
        // **Split, because `/resume` answers one of these and not the other.** A
        // parked plan is a run io-harness can continue; an approval belongs to a
        // turn that is over, and there is nothing left to authorize.
        "awaiting_plan" => Some(
            "the run proposed a plan and the turn ended before it was decided. \
             `/resume` reopens the plan and continues from there.",
        ),
        "awaiting_approval" => Some(
            "the run stopped waiting on a decision it never got, and an approval \
             belongs to the turn that asked for it. Ask again, or press Shift+Tab \
             to choose a posture that does not need one.",
        ),
        "denied" | "refused" => Some(
            "the permission boundary stopped it. The line above names the rule and \
             the layer; press Shift+Tab to change the posture for the next turn.",
        ),
        // **0.11.0 — the six an operator meets most and could act on, and could
        // not act on before.** Every one of these ended a real run with nothing
        // on screen but io-harness's own token: `error: step_cap_reached` over a
        // prompt, and no way to know whether that was a crash, a refusal or a
        // ceiling. The word stays the harness's — this interface never relabels
        // an outcome — and the sentence under it says what it means here.
        "step_cap_reached" => Some(
            "the turn used every step it was allowed. Say what to do next and it \
             carries on from where it stopped.",
        ),
        "stalled" => Some(
            "the agent repeated itself without changing anything, and stopped \
             rather than spending the rest of its steps. Try saying it differently.",
        ),
        "time_budget_exceeded" | "cost_budget_exceeded" | "budget_ceiling_reached" => Some(
            "the turn reached a budget in the configuration file. `[run]` sets the \
             step, token and time budgets.",
        ),
        "plan_rejected" => Some(
            "the plan was turned down, so nothing was written. Nothing has changed \
             in the workspace.",
        ),
        "cancelled" => Some("the turn was interrupted. Whatever it had finished is above."),
        "awaiting_recovery" => Some(
            "the run stopped in the middle of a call whose effect cannot be \
             established from here. Check whether it landed before asking again.",
        ),
        // **All three spellings.** io-harness writes `escalated_terminal` for a
        // failure it will not retry and `escalated_retryable` for one it would
        // have, and only the bare `escalated` was matched here — so the outcome
        // an operator actually meets, `error: escalated_terminal`, printed as a
        // token with nothing under it. It is what a provider that refuses the
        // request outright ends a turn as, which is the case a model that cannot
        // take an image produces, and the line above it is now the sentence
        // `crate::failure` writes.
        "escalated" | "escalated_terminal" => Some(
            "the provider refused the request and the run gave up. The line above \
             says what it refused.",
        ),
        "escalated_retryable" => Some(
            "the provider kept failing and the run gave up. The retries are in the \
             transcript above; asking again may work.",
        ),
        _ => None,
    }
}

/// The tools a step called, by name.
///
/// `Step.tool_call` is `name:arguments` per call, joined by `" | "`, and the
/// arguments are raw JSON. 0.1.1 put the whole thing on the step line, which was
/// the best available then — a reader had nothing else saying what ran. A live
/// run of 0.3.0 showed what it costs now that tool cells exist: every step
/// printed its arguments as a wall of escaped JSON directly under a cell that had
/// just said the same thing readably.
///
/// So the names are kept and the arguments are dropped. 0.1.1's F5 asks that a
/// step read as decision, then what it ran, then what came back, and it still
/// does — the tool cell above it is where the arguments live now.
fn tool_names(tool_call: &str) -> String {
    tool_call
        .split(" | ")
        .map(|call| verb(call.split_once(':').map_or(call, |(name, _)| name)))
        .collect::<Vec<_>>()
        .join(", ")
}

/// A target inside the workspace, shown relative to it; anything else whole.
///
/// A path outside the workspace is not shortened at all — a `../../..` chain is
/// less readable than the path it was computed from, and the fact worth seeing
/// about a file outside the workspace is precisely that it is outside.
fn relative(target: &str, root: &std::path::Path) -> String {
    if root.as_os_str().is_empty() {
        return target.to_string();
    }
    match std::path::Path::new(target).strip_prefix(root) {
        // The workspace root itself. `.` is what a shell would call it, and an
        // empty column would read as a call with no target at all.
        Ok(rest) if rest.as_os_str().is_empty() => ".".to_string(),
        Ok(rest) => rest.display().to_string(),
        Err(_) => target.to_string(),
    }
}

/// The snake-case name of a kind, taken from its `Debug` form.
///
/// `EventKind` is `#[non_exhaustive]` with no accessor for its own tag, and its
/// serde tag is only reachable by serializing — which would mean carrying a
/// serializer for a label. `Debug` is derived, its first token is the variant
/// name, and the mapping to snake case is the one `#[serde(rename_all)]` uses.
pub fn kind_name(kind: &EventKind) -> String {
    let debug = format!("{kind:?}");
    let variant: String = debug
        .chars()
        .take_while(char::is_ascii_alphanumeric)
        .collect();
    let mut snake = String::new();
    for (index, character) in variant.char_indices() {
        if character.is_ascii_uppercase() && index > 0 {
            snake.push('_');
        }
        snake.push(character.to_ascii_lowercase());
    }
    snake
}
