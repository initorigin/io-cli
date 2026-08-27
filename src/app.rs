//! The session's own state: what a keystroke means, and what the viewport holds.
//!
//! Kept apart from the code that drives io-harness so that both are testable. The
//! driver in `main.rs` owns the provider, the store and the turn future; this owns
//! everything a test needs to answer "what does `Ctrl+C` do in the middle of a
//! streaming turn".

use std::time::Duration;

use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use io_harness::RunEvent;
use ratatui::layout::Rect;
use ratatui::text::Line;
use ratatui::widgets::Paragraph;
use ratatui::Frame;

use crate::approval::{Answer, Approval, Ask};
use crate::composer::{Composer, Reply};
use crate::events::Events;
use crate::keys::{Action, Chord, Hit, Keys};
use crate::settings::Posture;
use crate::status::Status;
use crate::term::VIEWPORT_HEIGHT;
use crate::theme::{Theme, Tone};

/// How often the driver offers the session a tick while a turn is running.
///
/// Ten a second: fast enough that the indicator reads as motion rather than as a
/// character changing, slow enough that it is nothing next to the repaints a
/// streaming answer already causes. It is an offer rather than a repaint — see
/// [`App::tick`], which is what decides whether a frame is drawn.
pub const TICK: Duration = Duration::from_millis(100);

/// Rows the composer has at rest.
///
/// **One since 0.13.1, and two before it.** The second row was there for a paste
/// too big to read in one, and it was empty for every prompt anybody actually
/// types — a blank row between the rule and the line being written, which reads
/// as the field not knowing where it starts. The composer grows to the rows a
/// prompt needs the moment it needs them, so nothing is lost by not claiming them
/// in advance.
pub const COMPOSER_ROWS: u16 = 1;

/// The most rows a prompt may take, however long it is.
///
/// The viewport is subtracted from the terminal, so a composer allowed to grow
/// without a bound would push the transcript it is being written against off the
/// screen. Past this the prompt scrolls inside its own rows, which is what it did
/// at every length before 0.11.0.
pub const COMPOSER_MAX: u16 = 10;

/// Whether a turn is in flight.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Mode {
    Idle,
    Running,
}

/// What the driver should do about a keystroke.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Command {
    /// Nothing; the state already changed.
    None,
    /// Start a turn with this prompt.
    Submit(String),
    /// Run this slash command — the leading `/` removed.
    Slash(String),
    /// Run this line in the operator's own shell — the leading `!` removed.
    ///
    /// **The only value in this product that asks for a process to be spawned**,
    /// and it is built in exactly one place: `App::compose`, off a submitted
    /// composer line. That is the whole of the reachability argument
    /// `tests/dependencies.rs` asserts — nothing io-harness drives can produce
    /// one, so nothing on the event path can run a command. See
    /// [`crate::shell`] for what happens to it and why the terminal is never
    /// handed over.
    Shell(String),
    /// Ask the running turn to stop at its next step boundary.
    Interrupt,
    /// Stop the running turn now, without waiting for a boundary.
    ///
    /// The second press of the interrupt key. What the driver does with it is
    /// drop the turn's future, which is the only thing that ends a turn that is
    /// inside a slow tool call — and the reason it is a second press rather than
    /// the first is that a run dropped mid-flight closes no record of itself.
    Abandon,
    /// Leave.
    Exit,
    /// Repaint the viewport from scratch, never the scrollback.
    ClearViewport,
    /// Put the whole conversation back into the terminal's own scrollback.
    ///
    /// A command rather than something this type does, because the transcript
    /// lives in the harness's store and the store belongs to the driver.
    Transcript,
    /// Watch a child that is already running, by the run id the fleet is holding.
    ///
    /// **A command for exactly the reason [`Command::Transcript`] is one**: what
    /// this asks for lives in the harness's store, and the store belongs to the
    /// driver. `io_harness::Attach` reads the `run_events` table over a second
    /// connection, and nothing in this type has ever held one.
    ///
    /// Asked for on a **detached** child, which is the case that means anything: a
    /// detached child is still running and its parent has merely stopped waiting
    /// for it, so it is the one with events still to come and nobody watching
    /// them. Attaching to a child that is still being waited for is legal and
    /// io-harness does not care, but its events are already arriving on the
    /// stream this interface is drawing, so it would be a second copy of what is
    /// already on screen.
    Attach(i64),
    /// The first `Esc` at an empty prompt: say what undoing the last turn would
    /// undo, and wait for the second.
    ///
    /// Armed rather than fired, because this is the only key in the product that
    /// changes the operator's files on io-cli's own initiative rather than the
    /// agent's. Every write before it arrived through a tool call and passed a
    /// policy layer; this one does not, so it asks.
    ArmRewind,
    /// The second `Esc`: undo the last turn — its files, its memory and the
    /// conversation head.
    Rewind,
}

/// What a paste turned out to be.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Pasted {
    /// Ordinary text; the composer has it.
    Text,
    /// Paths naming files that exist, at least one of them an image. The driver
    /// stages each picture and puts its marker on the prompt — see
    /// [`App::attached`] and [`crate::composer::Composer::attach`] — and pastes
    /// anything that is not an image as the path it is.
    ///
    /// A list rather than one, because a drop of several files is one paste.
    Picture(Vec<String>),
    /// Something else owns the keyboard, so nothing was pasted at all.
    Refused,
}

pub struct App {
    pub composer: Composer,
    pub status: Status,
    pub events: Events,
    pub theme: Theme,
    mode: Mode,
    /// How many times `Ctrl+C` has been pressed with nothing to interrupt and
    /// nothing typed. Two in succession exits.
    quits: u8,
    /// Which action, if any, the last keystroke armed the first chord of.
    ///
    /// Cleared by *any* other key rather than by a timer, so nothing here reads a
    /// clock and an arming cannot outlive the moment the operator meant it.
    ///
    /// It holds the action rather than a bare flag now that the sequence is
    /// configurable: `rewind = "ctrl+r ctrl+r"` is as valid as the default
    /// `esc esc`, and a flag could not say which sequence was half-pressed.
    armed: Option<Action>,
    /// The bindings this session is running under.
    ///
    /// Held rather than consulted globally, because `/help` renders *this* —
    /// see [`crate::commands::rows`] — and a table that read the defaults from a
    /// constant while the handler read the file would be a help screen that lies
    /// about the machine in front of the reader.
    keys: Keys,
    /// Lines waiting to be committed to scrollback.
    pending: Vec<Line<'static>>,
    /// The question on screen, if the run is waiting on one.
    ///
    /// It lives here rather than beside the picker in the driver because it is
    /// not a choice the operator went looking for: it arrives mid-turn, it takes
    /// the keyboard while it is up, and the run is stopped until it is gone.
    approval: Option<Approval>,
    /// The agent's question about *intent*, if one is on screen.
    ///
    /// A second field rather than a second arm of `approval`, because the two are
    /// answered with different things — an approval with one of three keys, a
    /// question with prose — and because only one of them authorizes anything.
    /// Both are modal, and [`App::modal`] is the one place that knows it.
    intent: Option<crate::intent::Intent>,
    /// The plan on screen, if one is waiting to be decided.
    ///
    /// While it is up io-harness's own policy denies every write and every exec
    /// under a `plan-gate` layer, so this is the one overlay whose backdrop is
    /// guaranteed to be a workspace nothing has touched.
    plan: Option<crate::plan::Review>,
    /// Rules the operator has allowed for the rest of this session.
    ///
    /// The harness's own `remember` is run-scoped and dies with the turn, so
    /// without this a *this session* answer would ask again on the next prompt.
    /// F5 asserts it on the policy handed to the next turn.
    remembered: Vec<io_harness::Rule>,
    /// The workspace this session is held over.
    ///
    /// Kept so an approval can resolve a write's target: the harness sends it
    /// relative to the workspace, and the process's working directory is not the
    /// same thing under `io -C <dir>`.
    root: std::path::PathBuf,
    /// Pictures pasted while a turn was running, waiting to be staged.
    ///
    /// A turn holds the session, and staging an attachment needs it — so a
    /// picture dropped onto the prompt mid-turn waits here until the turn is
    /// over rather than being dropped or half-attached. The operator is told it
    /// is waiting, and a moment later it is `[Image #1]` like any other.
    queued: Vec<String>,
    /// Prompts finished while a turn was running, waiting for turns of their own.
    ///
    /// **A third thing, and neither of the two it sits between.** `queued` above
    /// holds pictures, which are staged onto whatever prompt comes next;
    /// `submitted` below is the one prompt *this* turn is about, kept single so
    /// [`App::undo_turn`] has exactly one line to put back. This is a line the
    /// operator finished while the session was busy: it is not part of the
    /// running turn, it is not an attachment to it, and there can be several of
    /// it. Overloading either neighbour would lose one of those three facts.
    ///
    /// Position in the vector is the whole of the ordering, and nothing here
    /// carries a time — which is what lets a session that reads no clock still
    /// fire these in the order they were typed.
    ///
    /// In memory for the length of the session and written nowhere. A prompt
    /// that outlived the terminal it was typed into would come back as a turn
    /// nobody asked for, against a conversation that had moved on without it.
    prompts: Vec<String>,
    /// Whether the surface that draws `prompts` has been shut by the operator.
    ///
    /// **Held rather than derived, and it is `Esc` that makes it a field.** The
    /// surface opens on its own — see [`App::queue_prompt`] — so "is it open" is
    /// almost the same question as "is anything queued", and it would be exactly
    /// the same question if it could not be dismissed. It can: F2 asks that `Esc`
    /// close the surface rather than the turn, and a dismissal that was inferred
    /// from the queue could only be honoured by dropping the queue. This is the
    /// one bit that says *the operator has seen it*, and it is set true again the
    /// moment something new is queued, because a new line is a new thing to have
    /// seen.
    ///
    /// [`App::queue_open`] is the predicate every caller asks, and it is the
    /// three facts together: opened, still queued, and a turn to be queued behind.
    queue_open: bool,
    /// Where the operator is inside `prompts`, and which line they have taken out
    /// of it to edit.
    ///
    /// **The queue is here and the mark is there, and that split is the point.**
    /// `prompts` is a fact about the session — the driver drains it, a turn runs
    /// off it, and it outlives every surface. A mark is a fact about the surface
    /// drawing it: meaningless with the surface shut, read by nothing but the
    /// rows. See [`crate::queue::Cursor`], whose verbs take `&mut Vec<String>` so
    /// that this stays the one owner of the queue itself.
    queue: crate::queue::Cursor,
    /// Every image attached in this session, by path, oldest first.
    ///
    /// The index into it is the number in `[Image #1]`, and the path is all that
    /// is kept: `/image` reads the file again rather than holding a screenshot's
    /// worth of bytes for the life of the session, and re-reading is also what
    /// makes it the file as it is now rather than as it was.
    images: Vec<String>,
    /// Rows this turn has committed to the scrollback so far.
    ///
    /// Counted where they are handed over — [`App::take_pending`] — because that
    /// is the one place every line of io-cli's own passes through on its way to
    /// the terminal. It is what tells [`App::undoable`] whether there is anything
    /// on screen worth keeping, and what tells the driver how far to erase back.
    turn_rows: u16,
    /// How many of those rows are the echo of the prompt itself.
    echo_rows: u16,
    /// The prompt this turn is about, kept so an undone turn can put it back in
    /// the composer exactly as it was typed.
    submitted: String,
    /// Whether the operator has already asked this turn to stop.
    ///
    /// Reset by `started`, so every turn gets its own first press. It is what
    /// makes the second press mean something different from the first.
    stopping: bool,
    /// Whether the turn now running is a *contained* turn.
    ///
    /// Set by the driver as a turn starts and cleared as it ends. It exists for
    /// one sentence: `Ctrl+C` stops an uncontained turn at the next step boundary
    /// and
    /// a contained one at the next boundary where no child is in flight, and an
    /// interface that promised the first while doing the second would look as
    /// though it had swallowed the key.
    pub contained: bool,
    /// The tree this turn has spawned, as the events have described it.
    ///
    /// Always folded, whether or not the view is open: an operator who opens it
    /// mid-turn must see what has already happened, and a model built only while
    /// somebody is watching would start empty at the moment it is wanted.
    pub fleet: crate::fleet::Fleet,
    /// What this session has seen of each configured MCP server.
    ///
    /// Beside [`Status`]'s aggregate `mcp` pair rather than inside it: that
    /// field answers "is anything connected" in one row of a status line, and
    /// this answers "what happened to each of them" for `/mcp`. Neither is
    /// derivable from the other — the pair carries no server names at all.
    pub servers: crate::servers::Observed,
    /// Whether the fleet view has the composer's rows.
    fleet_open: bool,
    /// How much of a change a diff shows. From `[app.io-cli]`, defaulting to
    /// unified, which is what every file written before 0.3.0 means.
    diff_style: crate::settings::DiffStyle,
    /// The permission posture this session is running under.
    ///
    /// `None` means the configuration file holds a policy that is none of the
    /// three the wizard offers, which io-harness's own file can express. The line
    /// says `custom` rather than naming one it is not, and the first press of the
    /// key moves to a posture the operator did choose.
    posture: Option<Posture>,
}

impl App {
    pub fn new(theme: Theme, model: impl Into<String>) -> Self {
        Self {
            composer: Composer::new(),
            status: Status::new(model),
            events: Events::new(theme),
            theme,
            mode: Mode::Idle,
            quits: 0,
            armed: None,
            contained: false,
            queued: Vec::new(),
            prompts: Vec::new(),
            queue_open: false,
            queue: crate::queue::Cursor::default(),
            images: Vec::new(),
            turn_rows: 0,
            echo_rows: 0,
            submitted: String::new(),
            stopping: false,
            fleet: crate::fleet::Fleet::new(),
            servers: crate::servers::Observed::default(),
            fleet_open: false,
            keys: Keys::default(),
            pending: Vec::new(),
            approval: None,
            intent: None,
            plan: None,
            remembered: Vec::new(),
            root: std::path::PathBuf::new(),
            diff_style: crate::settings::DiffStyle::default(),
            posture: None,
        }
    }

    /// Say which workspace this session is held over.
    ///
    /// Handed to `Events` as well, which shortens a tool's target against it.
    pub fn set_root(&mut self, root: impl Into<std::path::PathBuf>) {
        self.root = root.into();
        self.events.set_root(self.root.clone());
    }

    /// Say how much of a change a diff should show. Read from the harness's own
    /// configuration by the driver; never parsed here.
    pub fn set_diff_style(&mut self, style: crate::settings::DiffStyle) {
        self.diff_style = style;
    }

    /// Say whether this session runs in plain mode.
    ///
    /// Decided once, by [`crate::settings::plain`], and handed down — never
    /// re-derived here. There is one boolean and it lives on the status line,
    /// which is the surface the mode is about; this reads it back off there
    /// rather than keeping a second copy that could disagree with the one the
    /// indicator consults.
    pub fn set_plain(&mut self, plain: bool) {
        self.status.plain = plain;
        // The renderer needs it too from 0.11.0: the provider and the run's two
        // numbers are status-line fields now, and a plain session has no status
        // line a reader can follow. See `Events::set_plain`.
        self.events.set_plain(plain);
    }

    /// Whether this session runs in plain mode.
    pub fn plain(&self) -> bool {
        self.status.plain
    }

    /// A run stopped to ask. The overlay opens and takes the keyboard.
    ///
    /// Nothing is committed here. A question in the scrollback is one that can be
    /// scrolled away from a run which is blocked on it, which is what F1 asserts
    /// and the reason this surface is an overlay at all.
    pub fn open_approval(&mut self, ask: Ask) {
        self.approval = Some(Approval::new(ask, &self.root));
    }

    /// The agent asked what was meant. The overlay opens and takes the keyboard.
    ///
    /// Committed nowhere, for the same reason an approval is not: a question in
    /// the scrollback can be scrolled away from a run that is blocked on it. The
    /// transcript gets `QuestionAsked` from the event stream, which is the note
    /// that the run stopped rather than the question itself.
    pub fn open_intent(&mut self, asked: crate::intent::Asked) {
        self.intent = Some(crate::intent::Intent::new(asked));
    }

    /// Answer the open question, or decline it with `None`.
    ///
    /// Declining is not a refusal: io-harness persists the question and pauses
    /// the run, so the answer can arrive after this process has exited. What is
    /// said in the scrollback says which of the two happened.
    pub fn answer_intent(&mut self, answer: Option<String>) {
        let Some(intent) = self.intent.take() else {
            return;
        };
        match &answer {
            Some(text) => self.record(
                Tone::Muted,
                format!("answered {} {text}", self.theme.glyphs.dash),
            ),
            None => self.record(
                Tone::Warning,
                format!(
                    "left unanswered {} the run pauses and keeps the question",
                    self.theme.glyphs.dash
                ),
            ),
        }
        intent.resolve(answer);
    }

    /// A plan was proposed and nothing has been done about it yet.
    pub fn open_plan(&mut self, proposed: crate::plan::Proposed) {
        self.plan = Some(crate::plan::Review::new(proposed));
    }

    /// Decide the open plan. The overlay closes and the run acts on the verdict.
    ///
    /// The line committed here is io-cli's own note of what it sent; the
    /// harness's `PlanDecided` arrives separately on the event stream. They agree
    /// because the verdict travelled one way, and if they ever disagree this is
    /// where it shows.
    pub fn decide_plan(&mut self, verdict: io_harness::PlanVerdict) {
        let Some(plan) = self.plan.take() else {
            return;
        };
        let dash = self.theme.glyphs.dash;
        let (tone, said) = match &verdict {
            io_harness::PlanVerdict::Approve => (
                Tone::Muted,
                format!("plan approved {dash} {} steps", plan.plan().steps.len()),
            ),
            io_harness::PlanVerdict::Revise { correction } => {
                (Tone::Warning, format!("sent back {dash} {correction}"))
            }
            io_harness::PlanVerdict::Cancel => {
                (Tone::Refused, format!("plan cancelled {dash} nothing ran"))
            }
        };
        self.record(tone, said);
        plan.resolve(Some(verdict));
    }

    /// Whether a modal surface owns the keyboard.
    ///
    /// One answer for every caller. An approval, a question and a plan are all
    /// modal, and a guard that knew about only one of them is how a keystroke
    /// reaches a composer nobody can see.
    pub fn modal(&self) -> bool {
        self.approval.is_some() || self.intent.is_some() || self.plan.is_some()
    }

    /// The posture in force.
    /// Whether the next `Esc` at an empty prompt would perform a rewind.
    ///
    /// Read by the tests rather than by the driver, which is told what to do by
    /// the `Command` it gets back. A destructive key needs its state assertable
    /// from outside, or "one press does nothing" is a claim with nothing behind
    /// it.
    pub fn armed(&self) -> bool {
        self.armed.is_some()
    }

    /// Say which keys this session runs under. Resolved once by
    /// [`crate::keys::Keys::resolve`] from `[app.io-cli.keys]` and handed down;
    /// never parsed here.
    pub fn set_keys(&mut self, keys: Keys) {
        self.keys = keys;
    }

    /// The bindings in force, for the surface that has to show them.
    pub fn keys(&self) -> &Keys {
        &self.keys
    }

    pub fn posture(&self) -> Option<Posture> {
        self.posture
    }

    /// Say which posture the session started under. The status line follows it.
    pub fn set_posture(&mut self, posture: Option<Posture>) {
        self.posture = posture;
        self.status.policy = Some(match posture {
            Some(posture) => posture.short().to_string(),
            None => "custom".to_string(),
        });
    }

    /// Move to the next posture. One key, no menu, always visible — and it takes
    /// effect on the next turn, because io-harness takes a policy per turn and has
    /// no way to change one mid-flight.
    fn cycle_posture(&mut self) -> Command {
        let next = match self.posture {
            Some(posture) => posture.next(),
            // From a policy that is none of the three, the first press lands on the
            // first one rather than on nothing.
            None => Posture::Workspace,
        };
        // **Silently.** The posture is on the footer, which repaints on the same
        // keystroke, so the line this used to commit said in the scrollback what
        // the screen was already showing — and cycling through three postures to
        // reach the one you wanted left three of them behind, permanently, in the
        // transcript of a session that ran under one.
        self.set_posture(Some(next));
        Command::None
    }

    /// Whether a question is on screen.
    pub fn asking(&self) -> bool {
        self.modal()
    }

    /// Take a bracketed paste, from either of the driver's input loops.
    ///
    /// Library-side and named for the same reason [`crate::sessions::resume`]
    /// is: the driver lives in a binary no integration test can link, so a
    /// paste routed inside a match arm there is a decision nothing can assert
    /// and nothing can sabotage. `picker_open` is passed in rather than read
    /// here because the picker belongs to the driver, the way the approval
    /// belongs to this type.
    ///
    /// Returns whether the text reached the composer, so a test can name which
    /// surface swallowed it instead of asserting that something, somewhere,
    /// did nothing.
    ///
    /// A modal surface refuses it. Both a picker and an approval take the
    /// keyboard while they are up, and a paste that slipped past either would
    /// land in a composer sitting behind the overlay — typed by nobody, seen by
    /// nobody, and sent with the next prompt.
    pub fn paste(&mut self, text: &str, picker_open: bool) -> Pasted {
        if picker_open || self.modal() {
            return Pasted::Refused;
        }
        // **A picture pasted is a picture attached, since 0.13.1.** Dragging an
        // image onto the prompt is how an operator attaches one — it is what they
        // already do in every other window — and `/attach` was a command they had
        // to know about first. What arrives is a path, and a path naming an image
        // that exists is the whole test; the driver does the staging, because the
        // session, the provider and the policy are its.
        // **A way to see what a terminal actually inserts.** Set `IO_DEBUG_PASTE`
        // to a file and every paste is appended to it verbatim, with the paths
        // this crate found in it underneath. A paste is the one input this
        // product cannot reproduce from the outside — what lands on the prompt is
        // whatever the terminal made of whatever the pasteboard held — so when a
        // copy of two pictures comes out as text, this is the only thing that
        // says why.
        if let Some(to) = std::env::var_os("IO_DEBUG_PASTE") {
            let found = crate::composer::pasted_paths(text);
            let _ = std::fs::OpenOptions::new()
                .create(true)
                .append(true)
                .open(to)
                .map(|mut file| {
                    use std::io::Write;
                    writeln!(
                        file,
                        "--- paste, {} bytes\n{text:?}\nfound {} path(s): {found:?}",
                        text.len(),
                        found.len(),
                    )
                });
        }
        let paths = crate::composer::pasted_paths(text);
        if paths
            .iter()
            .any(|path| io_harness::Media::source_type_for(path).is_some())
        {
            return Pasted::Picture(paths);
        }
        self.composer.paste(text);
        Pasted::Text
    }

    /// Answer the open question. The overlay closes, the run goes on, and the
    /// decision commits one line — so it is in the transcript as well as in the
    /// harness's own trace.
    pub fn answer_approval(&mut self, answer: Answer) {
        let Some(approval) = self.approval.take() else {
            return;
        };
        let act = crate::approval::act_word(approval.ask().act());
        let target = approval.ask().target().to_string();
        if answer == Answer::Session {
            self.remembered.push(approval.remembered());
        }
        approval.answer(answer);
        self.record(
            if answer == Answer::Deny {
                Tone::Refused
            } else {
                Tone::Success
            },
            format!(
                "{act} {target} {} {}",
                self.theme.glyphs.dash,
                answer.spoken()
            ),
        );
    }

    /// Everything the operator has allowed for the rest of this session, as
    /// io-harness rules. The driver merges these into the policy it hands the next
    /// turn; io-cli evaluates none of them.
    pub fn remembered(&self) -> &[io_harness::Rule] {
        &self.remembered
    }

    pub fn mode(&self) -> Mode {
        self.mode
    }

    /// A turn started.
    pub fn started(&mut self) {
        self.mode = Mode::Running;
        self.turn_rows = 0;
        self.echo_rows = 0;
        self.status.working = true;
        self.stopping = false;
        // The clock and the turn's own token count start here. What a reader
        // wants of the row above the prompt is how long THIS turn has been going
        // and what it is costing — not how long the terminal has been open.
        self.status.start_run();
        // **The fleet belongs to one turn, and it is cleared here rather than
        // when a turn ends.** `Fleet::forget` was reachable only from `/resume`,
        // `/fork`, a rewind and `/clear`, so a turn that fanned out left its
        // children in the model for the rest of the conversation. 0.19.0 could
        // afford that — the rows were stale and that was all. 0.20.0 cannot: the
        // model now also holds mail, and `note_fleet`'s cheap early return is
        // `Fleet::is_empty`, so a single fan-out anywhere in a session would (a)
        // draw the *previous* turn's agents and their messages as though they were
        // this turn's, and (b) put a `run_root` and a `tree_addresses` query on
        // every step of every ordinary turn afterwards — the exact cost that guard
        // exists to avoid.
        //
        // **At the start of a turn and not at the end of one**, so a fleet stays
        // readable after the turn that produced it. An operator whose agents have
        // just finished can still open the pane and see what happened; it is the
        // *next* prompt that means those rows are no longer about anything.
        self.fleet.forget();
        self.quits = 0;
        self.announce();
    }

    /// Whether this turn can simply be taken back.
    ///
    /// True while a turn has produced nothing but its own goal line: no step has
    /// finished, nothing has streamed, and nothing has been committed to the
    /// scrollback except the echo of what the operator typed. That is the turn an
    /// operator stops a moment after pressing Enter, and there is nothing in it
    /// worth keeping, nothing to wait for a step boundary for, and nothing worth
    /// a line of explanation.
    ///
    /// Past that point the ordinary two-press stop applies: a turn that has done
    /// work keeps it, and the record io-harness writes is the record of a run
    /// that was cancelled rather than one that never happened.
    pub fn undoable(&self) -> bool {
        self.mode == Mode::Running
            && self.status.steps.unwrap_or(0) == 0
            && self.events.live().trim().is_empty()
            && self.turn_rows <= self.echo_rows
    }

    /// Hold a picture until the running turn lets go of the session.
    pub fn queue_picture(&mut self, path: impl Into<String>) {
        self.queued.push(path.into());
    }

    /// The pictures that were waiting, in the order they were dropped.
    pub fn take_queued_pictures(&mut self) -> Vec<String> {
        std::mem::take(&mut self.queued)
    }

    /// Hold a prompt the operator finished while a turn had the session.
    ///
    /// **Kept, rather than sent or dropped.** Up to 0.16.0 this keystroke looked
    /// accepted and was not: the composer empties on `Enter` whatever happens
    /// next, so the line disappeared from the prompt exactly as a sent one does,
    /// and then fell through the driver's catch-all and was gone. That is the
    /// worst shape a lost keystroke can take — there is no error, no refusal and
    /// no text left to press `Enter` on again. Holding it is the smallest honest
    /// answer: the session cannot take it now, so it takes it next.
    ///
    /// The notice is the footer's rather than the scrollback's, because a queued
    /// prompt is not yet part of the record. It becomes part of it when it runs,
    /// as its own exchange with its own echo, and a line here would be a second
    /// entry for one prompt. What keeps it visible in between is the queue
    /// itself — state, which a surface can draw and a notice cannot.
    pub fn queue_prompt(&mut self, text: impl Into<String>) {
        self.prompts.push(text.into());
        // **The surface opens itself, and that is a decision rather than a
        // convenience.** Every other surface in this product is opened by a key:
        // the fleet view has one, the pickers have commands, and each of them is
        // something an operator went looking for. This one is not — the operator
        // went looking for a *turn*, and what they got instead was a line held
        // back. The queue is the whole of the explanation for that, so it appears
        // at the moment there is something to explain and leaves when there is
        // not. The key it does not have is the one it would have had to buy: an
        // `Action` of its own is an entry in `keys::Action::ALL` — which is
        // index-sensitive and asserted as such — plus a name, a default binding,
        // a rebindable flag, a row in `commands::KEYS` and two in the README, all
        // for a surface whose whole content is already known to the session.
        self.queue_open = true;
        // The status line's depth is assigned at each of the three sites that
        // move the queue rather than synced from a tick: `App::tick` returns
        // early unless a turn is running, and the queue drains precisely when one
        // ends, so a sync there would leave a stale depth on the line at the idle
        // prompt — which is the one moment the number is a lie an operator can
        // act on.
        self.status.queued_prompts = self.prompts.len();
        let waiting = self.prompts.len();
        let dash = self.theme.glyphs.dash;
        // Muted, like the picture held one line over: nothing has gone wrong and
        // nothing was refused. The count is in the sentence because the second
        // and third prompt otherwise produce a notice identical to the first,
        // which reads as a keystroke that did nothing.
        self.say(
            Tone::Muted,
            if waiting == 1 {
                format!("queued {dash} it runs when this turn ends")
            } else {
                format!("{waiting} queued {dash} they run in order when this turn ends")
            },
        );
    }

    /// The prompt that has waited longest, taken off the front.
    ///
    /// **One at a time, and the driver runs a whole turn between two calls.**
    /// That is what makes three queued lines three turns rather than one turn
    /// carrying three questions: each gets its own echo, its own answer under it
    /// and its own `Ctrl+C`. Joined into one prompt they would be a run that
    /// answers everything in one breath and cannot be stopped part-way through,
    /// which is the opposite of what queueing them was for.
    pub fn next_queued_prompt(&mut self) -> Option<String> {
        if self.prompts.is_empty() {
            return None;
        }
        let next = self.prompts.remove(0);
        self.status.queued_prompts = self.prompts.len();
        Some(next)
    }

    /// What is waiting, oldest first.
    ///
    /// For a surface that draws the queue and for the tests. The driver takes
    /// them one at a time through [`App::next_queued_prompt`] instead, so there
    /// is no path that reads the whole queue in order to run it.
    pub fn queued_prompts(&self) -> &[String] {
        &self.prompts
    }

    /// Whether the queue surface is on screen.
    ///
    /// **Three facts, and none of them is a fourth field.** It has been opened
    /// and not dismissed; there is something waiting to draw; and a turn is
    /// running to be waiting behind. The last is what closes it when the turn
    /// ends, for the same reason [`App::finished`] closes the fleet view — a
    /// surface left standing over an idle session describes a state that is no
    /// longer true — and it is `mode` that says so rather than a line in
    /// `finished`, which is why it comes *back* for the second queued turn. A
    /// flag cleared there would have shut the queue for the whole of the drain,
    /// which is the run in which the two lines still waiting most want a row.
    ///
    /// The middle fact is why nothing has to close it when the queue empties.
    pub fn queue_open(&self) -> bool {
        self.queue_open && self.mode == Mode::Running && !self.prompts.is_empty()
    }

    /// Whether the queue will actually be drawn on the next frame.
    ///
    /// **Open is not drawn, and the difference is one row.** The fleet view is
    /// rendered in this surface's place, in the composer's own rect, so a queue
    /// that is open behind it draws nothing — and the layout must not release the
    /// blank row above the activity line for a surface that will not use it. The
    /// row bought nothing and the fleet quietly grew by one.
    ///
    /// A named predicate rather than the expression inline, because the two
    /// readings clippy offers for that expression are each shorter and neither
    /// says which surface wins.
    pub fn queue_drawn(&self) -> bool {
        self.queue_open() && !self.fleet_open
    }

    /// Drop everything still waiting, and report how much was dropped.
    ///
    /// An operator stopping a turn is stopping the session, not just the step in
    /// front of them. A queue that fired anyway would make the stop key start
    /// three more turns, which is a key that reads as broken — and the prompts
    /// were typed against a conversation that was going somewhere else.
    pub fn forget_queued_prompts(&mut self) -> usize {
        self.status.queued_prompts = 0;
        std::mem::take(&mut self.prompts).len()
    }

    /// Put lines back at the FRONT of the queue, in order, and say how many wait.
    ///
    /// **For a `/steer` that emptied the queue into a turn nothing read.** The
    /// send takes each line out to hand it over, and a turn that ends before its
    /// next step boundary hands nothing over — so without this the lines are gone,
    /// in the release whose promise is that a mid-turn prompt is not destroyed.
    ///
    /// The front rather than the back, because they were ahead of whatever was
    /// queued after them and putting them behind it would silently reorder the
    /// operator's work. The driver calls this only for a turn that ended on its
    /// own; one the operator stopped drops them, for the same reason a stopped
    /// turn drops the rest of the queue.
    pub fn requeue_prompts(&mut self, lines: Vec<String>) -> usize {
        for (at, line) in lines.into_iter().enumerate() {
            self.prompts.insert(at, line);
        }
        self.status.queued_prompts = self.prompts.len();
        self.queue_open = true;
        self.prompts.len()
    }

    /// Remember an attached image and return the number its marker carries.
    ///
    /// One-based, because the marker is read by a person: `[Image #1]` is the
    /// first picture of the session, and the numbering does not restart with a
    /// turn — a reader scrolling back through a conversation needs `#3` to mean
    /// one thing.
    pub fn attached(&mut self, path: impl Into<String>) -> usize {
        self.images.push(path.into());
        self.images.len()
    }

    /// The path `[Image #n]` stands for, if this session has one.
    pub fn image(&self, n: usize) -> Option<&str> {
        self.images.get(n.checked_sub(1)?).map(String::as_str)
    }

    /// How many images this session has attached.
    pub fn images(&self) -> usize {
        self.images.len()
    }

    /// Rows this turn has committed so far. The driver's bound for how far back
    /// it may erase.
    pub fn turn_rows(&self) -> u16 {
        self.turn_rows
    }

    /// Take the turn back: the rows it put on screen, and the prompt restored.
    pub fn undo_turn(&mut self) -> (u16, String) {
        let rows = self.turn_rows;
        let prompt = std::mem::take(&mut self.submitted);
        self.turn_rows = 0;
        self.echo_rows = 0;
        self.pending.clear();
        self.events.forget();
        self.status.forget_run();
        self.servers.forget();
        self.composer.set(&prompt);
        (rows, prompt)
    }

    /// A turn ended, however it ended — finished, cancelled or failed.
    ///
    /// Whatever streamed and was never closed by an event is committed here, so an
    /// interrupted turn keeps its partial output in the scrollback instead of
    /// losing it with the turn.
    pub fn finished(&mut self) {
        self.mode = Mode::Idle;
        self.status.working = false;
        // A per-turn fact, cleared with the turn. Left standing, an idle session
        // that had contained one turn would go on describing `Ctrl+C` in the
        // words of a turn that is no longer running — and `/contain off` between
        // turns would leave the sentence disagreeing with the mode.
        self.contained = false;
        // **The view closes with the turn, and the model does not.** A live run
        // found this: with the view up when the turn ended, the composer stayed
        // hidden behind a tree that had stopped moving, and a session that says
        // `ready` with nowhere to type reads as one that has hung. The tree is
        // still there — `/fleet` reopens it, and every spawn, refusal and report
        // is in the transcript — but the prompt comes back on its own.
        self.fleet_open = false;
        // An edit lapses with the turn it was made against. The line itself stays
        // in the composer, where the operator can see it and send it: putting it
        // back would queue a second copy behind the drain that is about to start,
        // and both would run. What is dropped is the *position*, which is the part
        // that goes stale — a slot remembered across a drain points at somebody
        // else's line.
        self.queue.lapsed();
        // A question outlives its run only as a stuck overlay over a session that
        // has moved on. Dropping it is the denial — see `Ask` — and the run it
        // belonged to has already ended, so there is nobody left to tell.
        self.approval = None;
        // The same for a question about intent, with the same consequence stated
        // differently: dropping the sender is `None`, which is the answer that
        // pauses a run rather than the one that denies it — and this run has
        // already stopped, so the pause costs nothing that was still moving.
        self.intent = None;
        // And for a plan, where `None` is the safe direction twice over: the run
        // is over, and a plan nobody decided is a plan nothing acted on.
        self.plan = None;
        let tail = self.events.flush();
        self.pending.extend(tail);
        // After the tail, so the line saying the session is idle again is the last
        // thing in the scrollback rather than a claim made over content still
        // arriving underneath it.
        self.announce();
    }

    /// In plain mode, commit the state word the status line has just changed to.
    ///
    /// **This is the one thing plain mode puts in the scrollback that the default
    /// does not, and it is deliberately the only one.** Every other state a run
    /// produces already commits: `Started`, `Step`, `Refused`,
    /// `ApprovalRequested`, `Finished` and the forty-odd kinds that fall through
    /// [`crate::events::Events::event`] each write at least one line, and the
    /// status fields fed from them — the token total, the context share, the
    /// containment backend — are all restatements of an event that has already
    /// been narrated. Committing those again would be a second rendering of the
    /// same facts, which is what "plain mode is a second consumer of the event
    /// stream, not a second renderer" rules out.
    ///
    /// What is left over is exactly this: whether a turn is running. It is
    /// io-cli's own state rather than io-harness's — [`App::started`] is called
    /// before the run exists and [`App::finished`] after it has returned, so the
    /// pair brackets the turn even when the harness never emitted a `Started` at
    /// all, which is what a provider that fails on its first call produces. In
    /// the default interface that state is carried by a word that only ever
    /// repaints and a spinner that only ever moves, so it is the one state change
    /// a reader who cannot see the viewport cannot follow.
    ///
    /// The words are the status line's own, verbatim, so there is one vocabulary
    /// for the state and not two spellings of it.
    ///
    /// The session's age is deliberately not narrated. It changes every second
    /// with nothing having happened, which makes it a clock rather than a state
    /// change — and a transcript that says the time once a second is one nobody
    /// can read, in a screen reader least of all.
    fn announce(&mut self) {
        if !self.status.plain {
            return;
        }
        let state = if self.status.working {
            "working"
        } else {
            "ready"
        };
        self.record(Tone::Muted, state);
    }

    /// The clock moved. Returns whether the viewport has to be redrawn.
    ///
    /// This is the whole of the release's liveness, and it is a *function of an
    /// age the caller supplies* rather than of a timer this type reads. That is
    /// deliberate and it is what makes the two properties assertable: a test
    /// advances `age` by hand and asks whether a frame is owed, with nothing
    /// sleeping and nothing measured.
    ///
    /// An idle session is told no, and is not even given the new time. A terminal
    /// interface that redraws forever is the thing this renderer exists not to
    /// be, so the tick is live only while a turn is: between turns the clock is
    /// answered by the next keystroke, which repaints anyway.
    pub fn tick(&mut self, age: Duration) -> bool {
        if self.mode != Mode::Running {
            return false;
        }
        self.status.elapsed = age;
        self.status.advance();
        true
    }

    /// Take an event from the harness.
    ///
    /// `at` is the session's age, handed in by the driver rather than read here.
    /// It is what lets a tool cell report how long its call took without any
    /// module but `src/main.rs` touching a clock — the same shape [`App::tick`]
    /// established, and the reason `tests/timing.rs` can still assert that no
    /// test anywhere reads one.
    pub fn event(&mut self, event: &RunEvent, at: Duration) {
        self.status_from(event);
        self.fleet.event(event);
        let lines = self.events.event(event, at);
        // **The echo, measured as it is written.** `undoable` asks whether this
        // turn has put anything on screen beyond the operator's own words, and
        // the only way to know how many rows those took is to count them here —
        // a multi-line prompt is as many rows as it has lines. The goal is kept
        // too, because it is exactly the text an undone turn puts back in the
        // composer.
        if let io_harness::EventKind::Started { goal, .. } = &event.kind {
            self.echo_rows = self
                .echo_rows
                .saturating_add(u16::try_from(lines.len()).unwrap_or(u16::MAX));
            self.submitted = goal.clone();
        }
        // The renderer counts what it could not place; the status line is where
        // that count is reachable. Read back rather than incremented here, so
        // there is one counter and not two that can disagree.
        self.status.unknown = self.events.unknown();
        self.pending.extend(lines);
    }

    /// Take an event from a run this session is **watching** rather than driving.
    ///
    /// **The same lines as [`App::event`] and none of the bookkeeping**, and the
    /// difference is the whole reason this exists. `App::event` also folds the
    /// event into `status_from` and into the fleet — both of which describe *this
    /// conversation*. A detached child is somebody else's run: its tokens are not
    /// this session's spend, its provider is not the one this session is talking
    /// to, its `Fleet` tier is not this turn's fan-out shape, and its own children
    /// are not this turn's children.
    ///
    /// Routing a watched run through `App::event` therefore corrupts the footer
    /// permanently — `Status::start_run` resets the clock and the run's tokens and
    /// nothing else, so a session total inflated by a watched child never comes
    /// back down — and it grafts that child's tree into the pane the operator
    /// opened to look at it. Neither is recoverable without ending the session,
    /// and neither is visible as a bug: the numbers are simply wrong afterwards.
    ///
    /// So a watched event draws and does nothing else. What the operator asked for
    /// is to see the run; they did not ask for it to be counted as theirs.
    pub fn watched(&mut self, event: &RunEvent, at: Duration) {
        let lines = self.events.event(event, at);
        self.pending.extend(lines);
    }

    /// What a step changed, as diffs.
    ///
    /// Handed in by the driver rather than read here. The store belongs to the
    /// driver — it is the only thing that holds one — and keeping this a function
    /// of values is what lets a test state a hunk by hand instead of standing up
    /// a database to hold one.
    pub fn edits(&mut self, edits: &[io_harness::Edit], width: u16) {
        for edit in edits {
            // A blank above as well as the one the cell ends with. A diff is a
            // block and reads as one; committed straight under the tool cell
            // that produced it, its header looked like another row of that cell.
            self.pending.push(Line::from(""));
            self.pending
                .extend(crate::diff::cell(edit, &self.theme, width));
        }
    }

    /// A picture, committed where it happened.
    ///
    /// Beside [`App::edits`] and for the same reason: the read that produced it
    /// belongs to the driver, which is the only thing holding a workspace and a
    /// policy, so what arrives here is already lines.
    pub fn picture(&mut self, lines: Vec<Line<'static>>) {
        self.pending.extend(lines);
    }

    /// Whether the fleet view is up.
    pub fn fleet_open(&self) -> bool {
        self.fleet_open
    }

    /// Forget the run the fleet describes, and close the view.
    ///
    /// Called where the conversation changes under it — `/resume`, `/fork`, a
    /// rewind — beside `Status::forget_run`, and for the same reason: every fact
    /// in it belongs to one tree, and a view that went on showing them would be
    /// describing a run that is no longer on screen.
    pub fn forget_fleet(&mut self) {
        self.fleet.forget();
        self.fleet_open = false;
    }

    /// The viewport height this session wants right now.
    ///
    /// [`VIEWPORT_HEIGHT`] until the prompt outgrows its two rows, and then as
    /// many as the prompt needs, up to [`COMPOSER_MAX`]. The driver compares this
    /// with the viewport it has and re-places when they differ — which is the
    /// one operation in this product that re-queries the cursor, so it is done at
    /// an idle prompt and nowhere else.
    ///
    /// The cap is not a matter of taste. The viewport is subtracted from the
    /// terminal, and a composer allowed to take all of it would push the
    /// transcript it is being written against off the screen.
    pub fn viewport_wanted(&self, width: u16, rows: u16) -> u16 {
        if self.mode == Mode::Running || self.modal() {
            return VIEWPORT_HEIGHT;
        }
        let wanted = self.composer.rows_wanted(width).min(COMPOSER_MAX);
        VIEWPORT_HEIGHT
            .saturating_add(wanted.saturating_sub(COMPOSER_ROWS))
            .min(rows.saturating_sub(2).max(VIEWPORT_HEIGHT))
    }

    /// Start a new conversation, or refuse because a turn is in flight.
    ///
    /// **Everything `/clear` does that is not the driver's is here**, so that the
    /// refusal has somewhere a test can stand. The driver's half — a new session
    /// id from the harness's store, and the screen — cannot be reached by a test
    /// at all, and a guard written there would be a guard nothing could check.
    ///
    /// The refusal is not the only one: a turn in flight keeps the driver inside
    /// its own loop, where a slash command is already answered with the same
    /// sentence. This is the lock that can be proved, and the two agree.
    ///
    /// Returns whether the caller should go on and replace the session. Nothing
    /// is destroyed either way: the conversation this ends is in io-harness's
    /// store and is still reachable with `/resume`, which is what makes clearing
    /// the screen a display decision.
    pub fn clear_conversation(&mut self) -> bool {
        if self.mode == Mode::Running {
            let dash = self.theme.glyphs.dash;
            self.say(
                Tone::Muted,
                format!("not while a turn is running {dash} Ctrl+C interrupts it first"),
            );
            return false;
        }
        // The same three the conversation-changing commands already reset,
        // beside each other for the same reason: every fact in them belongs to
        // the conversation that is ending.
        //
        // **The numbering goes with them.** `[Image #4]` in a conversation that
        // has one picture in it is a number counting something the reader cannot
        // see: the attachments belonged to the conversation that just ended, and
        // the next one starts at `#1`. The composer is emptied for the same
        // reason — a prompt half-written against a conversation that no longer
        // exists, and a `[pasted text #3]` standing for a block nobody can reach.
        self.images.clear();
        self.composer.clear();
        self.status.forget_run();
        self.servers.forget();
        self.forget_fleet();
        self.events.forget();
        true
    }

    /// Open the view, or close it.
    ///
    /// Opening a view of nothing is not refused: a session in contained mode
    /// before its first spawn has an answer to give — "nothing has been spawned
    /// yet" — and a key that appeared to do nothing would read as broken.
    pub fn toggle_fleet(&mut self) {
        self.fleet_open = !self.fleet_open;
    }

    /// The status line's share of an event.
    ///
    /// Only the events that carry a fact set a field, and nothing sets one to a
    /// default. A field this has never heard about stays `None`, which is what
    /// the line renders as nothing at all rather than as a zero.
    fn status_from(&mut self, event: &RunEvent) {
        match &event.kind {
            // **The provider, which until 0.11.0 was named by a line under every
            // prompt and nowhere else.** The line is gone and the fact is here;
            // see `US-IO-CLI-0.11.0-I01` for why that is a relocation rather than
            // a removal.
            io_harness::EventKind::Started { provider, .. } => {
                self.status.provider = Some(provider.clone());
            }
            // A different provider answering the same turn. io-harness emits this
            // once, at the transition, so the field says who is serving now
            // rather than who was asked.
            io_harness::EventKind::FellBackTo { provider } => {
                self.status.provider = Some(provider.clone());
            }
            // The model the run is asking, changed mid-run by a routing rule.
            // `to` is empty for the provider's own default, and a field blanked
            // to nothing would read as a session with no model at all.
            io_harness::EventKind::Routed { to, .. } if !to.is_empty() => {
                self.status.model = to.clone();
            }
            io_harness::EventKind::Step { tokens, .. } => {
                // The session's total, not the step's own. A field that swings
                // rather than climbs cannot be read at a glance.
                self.status.tokens = Some(self.status.tokens.unwrap_or(0) + tokens);
                self.status.run_tokens = Some(self.status.run_tokens.unwrap_or(0) + tokens);
                // The envelope's own number rather than a count kept here: a
                // resumed run replays its backlog, and a counter incremented per
                // event would climb past the step the run is actually on.
                self.status.steps = Some(event.step);
            }
            io_harness::EventKind::Finished { tokens, steps, .. } => {
                // The run's own totals, which are authoritative over the sum of
                // the steps we happened to see. The token guard is on the value
                // rather than on the tag: a run that reported no usage at all
                // reports `0`, and overwriting a real total with it would turn a
                // known number into a wrong one. The step count has no such
                // ambiguity — a run that ended having taken no steps really did
                // take none, and a conversational turn is exactly that.
                if *tokens > 0 {
                    // The run's own total is authoritative for the run. The
                    // session's is the sum of its runs, so the difference
                    // between what the steps reported and what the run says is
                    // what gets added to it rather than replacing it.
                    let counted = self.status.run_tokens.unwrap_or(0);
                    let session = self.status.tokens.unwrap_or(0);
                    self.status.tokens = Some(session + tokens.saturating_sub(counted));
                    self.status.run_tokens = Some(*tokens);
                }
                self.status.steps = Some(*steps);
            }
            // **A fold is still the better answer at the one moment it happens.**
            // `Compacted` reports the section's new size the instant it shrinks,
            // before any step has assembled against it — so this arm survives the
            // trace read that now fills the same field at every step, and the two
            // cannot disagree: `after_tokens` and `ContextEvent::est_tokens` are
            // both the assembler's estimate of the observation section, and this
            // one is simply earlier.
            //
            // What is gone is the denominator this arm used to build for itself.
            // It asked `ContextBudget::default().effective_tokens(None)` — a flat
            // `24_000` on every session in existence — under a comment claiming
            // the denominator was "io-harness's own declared budget, asked of the
            // harness rather than copied here". It was the *crate's* default
            // budget, which is a different number from *this contract's* the
            // moment an operator writes a `[run.context]` table, and it was wrong
            // for them in silence from the release the field was added in.
            // `Status::note_context` divides by `Status::budgets`, which the
            // driver fills from the contract it actually built.
            io_harness::EventKind::Compacted { after_tokens, .. } => {
                self.status.note_context(*after_tokens);
            }
            // The draw is per step and the ceiling is the tree's. `tokens`
            // accumulates because a field that swings rather than climbs cannot
            // be read at a glance — the same argument the session token count
            // above rests on — while `remaining` is replaced, because it is what
            // the ledger says NOW and an accumulated remainder would be
            // arithmetic on somebody else's subtraction.
            io_harness::EventKind::SpendDraw { tokens, remaining } => {
                let drawn = self.status.spend.map(|(drawn, _)| drawn).unwrap_or(0) + tokens;
                self.status.spend = Some((drawn, *remaining));
            }
            // **A handle opens once and closes once, and `HandlePolled` is
            // neither.** io-harness documents the invariant: a `HandleStarted`
            // ends in exactly one of `HandleExited`, `HandleKilled` and
            // `HandleOrphaned`, and a run that finishes with live handles kills
            // them on the way out — so the count returns to zero on its own and
            // the field disappears without anyone clearing it.
            //
            // `saturating_sub` rather than a bare decrement: a resumed run replays
            // a backlog, and an ending whose start was never seen must not wrap
            // the count to eighteen quintillion background jobs.
            io_harness::EventKind::HandleStarted { .. } => {
                self.status.jobs += 1;
            }
            io_harness::EventKind::HandleExited { .. }
            | io_harness::EventKind::HandleKilled { .. }
            | io_harness::EventKind::HandleOrphaned { .. } => {
                self.status.jobs = self.status.jobs.saturating_sub(1);
            }
            io_harness::EventKind::Contained { mode, backend, .. } => {
                self.status.containment = Some(crate::status::format_containment(mode, backend));
            }
            // **The three connection fields, filled from what happened and never
            // from what was configured.** A server named in the file and a server
            // that answered are different facts, and the second is the one an
            // operator is asking about when they look at this line.
            //
            // `Mcp` is the one event in the enum that means two things: with no
            // `tool` it is the server itself reaching a run, and with one it is a
            // call. The first is a server; the second is a tool the server
            // offered. Counting a call as a server would multiply one server into
            // as many as it was asked to do.
            io_harness::EventKind::Mcp { tool, .. } => {
                let (servers, calls) = self.status.mcp;
                self.status.mcp = match tool {
                    None => (servers + 1, calls),
                    Some(_) => (servers.max(1), calls + 1),
                };
                // And the per-server half, for `/mcp`. Folded here rather than at
                // a second call site so the two cannot disagree about what the
                // session saw.
                self.servers.event(&event.kind);
            }
            io_harness::EventKind::LspStarted { .. } => {
                self.status.lsp += 1;
            }
            // A browser that started and has gone nowhere is `None` for the host,
            // which draws as `web ready` — it is running, and there is nothing yet
            // to say about where it went.
            io_harness::EventKind::BrowserStarted { .. } if self.status.browser.is_none() => {
                self.status.browser = Some((String::new(), None));
            }
            // Every navigation, including the ones nobody typed: a redirect, a
            // click, a script assigning `location`. The last one wins, and the
            // verdict rides with it, because "browser: example.com" over a
            // navigation the policy refused would report a block as a visit.
            io_harness::EventKind::BrowserNavigated { host, permitted } => {
                self.status.browser = Some((host.clone(), Some(*permitted)));
            }
            io_harness::EventKind::TodoWrote { items } => {
                // io-harness's own arithmetic for a done count, off the event's own
                // items. A write carries the whole list rather than a delta, so the
                // field is the plan as it now stands and no store is read to
                // complete it — a later write that moves an item back replaces this
                // instead of climbing past it.
                let done = items
                    .iter()
                    .filter(|item| item.state == io_harness::TodoState::Done)
                    .count();
                // And `None` when the agent wrote no items at all, which io-harness
                // accepts — `parse_todo_items` never rejects an empty list. That is
                // a plan erased rather than a plan of zero, and `0/0` pinned to the
                // line is the exact placeholder F12's sabotage arm names.
                self.status.plan = (!items.is_empty()).then_some((done, items.len()));
            }
            _ => {}
        }
    }

    /// Say something to the operator that is not part of the record.
    ///
    /// **The footer, not the scrollback, and 0.13.1 is where that moved.** These
    /// lines are io-cli talking about the session rather than the session's own
    /// content: `stopping at the next step`, `not while a turn is running`,
    /// `press Ctrl+C again to exit`. They answered a keystroke that had just been
    /// pressed and then stayed in the terminal's permanent scrollback forever, so
    /// stopping one turn left three warning-coloured rows sitting between two
    /// answers, and a reader scrolling back a week later read them as part of the
    /// conversation. Now they take the footer's last row, replace one another,
    /// and are gone at the next keystroke.
    ///
    /// What still goes into the scrollback is what belongs to the record: what
    /// the agent said, what a tool did, what a turn ended as, and why a turn
    /// failed. [`App::record`] is that half.
    pub fn say(&mut self, tone: Tone, text: impl Into<String>) {
        self.status.notice = Some((tone, text.into()));
    }

    /// Commit a line of io-cli's own into the transcript.
    ///
    /// For the few things that are part of the conversation rather than about it:
    /// a turn's failure, and the sentence that says a new conversation started.
    /// Everything else is [`App::say`].
    pub fn record(&mut self, tone: Tone, text: impl Into<String>) {
        let line = self.theme.notice(tone, text);
        self.pending.push(line);
    }

    /// Take the footer's notice off, which every keystroke does.
    ///
    /// A notice answers the key that was just pressed, so the next key is the
    /// moment it stops being the answer to anything.
    pub fn forget_notice(&mut self) {
        self.status.notice = None;
    }

    /// Everything waiting to go into the terminal's scrollback, emptied.
    pub fn take_pending(&mut self) -> Vec<Line<'static>> {
        let lines = std::mem::take(&mut self.pending);
        if self.mode == Mode::Running {
            self.turn_rows = self
                .turn_rows
                .saturating_add(u16::try_from(lines.len()).unwrap_or(u16::MAX));
        }
        lines
    }

    /// Rows the viewport uses. Fixed — see [`VIEWPORT_HEIGHT`] for why, and for
    /// what that costs.
    pub fn viewport_height(&self) -> u16 {
        VIEWPORT_HEIGHT
    }

    /// What a keystroke means to the session.
    ///
    /// **The `match` is on an [`Action`], not on a `KeyCode`.** Which chord
    /// reaches which action is [`Keys`]'s business and the configuration file's;
    /// what an action *does* — and the guards around it, which are the
    /// interesting part — is this function's, and moving a key must not move
    /// those. `Ctrl+C` is the exception in both directions: it cannot be
    /// rebound, so it is still exactly one key here.
    pub fn key(&mut self, key: KeyEvent) -> Command {
        // The notice answered the key before this one. Cleared here rather than
        // by each arm, so nothing can leave one standing over a session that has
        // moved on.
        self.forget_notice();
        let chord = Chord::of(key);
        // An open question takes the keyboard, except for `Ctrl+C`. That is the
        // answer to "does `Ctrl+C` deny, or interrupt?": it interrupts, and the
        // question is denied as a consequence of the turn ending rather than as a
        // second meaning for one key. Nothing else can reach the composer while a
        // run is stopped waiting on an answer.
        //
        // Asked of `Keys` rather than spelled out, so this and the handler below
        // cannot disagree about which chord interrupts — but the answer is fixed,
        // because `Action::Interrupt` is the one binding a file cannot move.
        let interrupting = self.keys.hit(chord, None) == Some(Hit::Fire(Action::Interrupt));
        if let Some(open) = self.approval.as_mut().filter(|_| !interrupting) {
            if let Some(answer) = open.key(key) {
                self.answer_approval(answer);
            }
            return Command::None;
        }
        // A question about intent takes the keyboard on exactly the same terms,
        // `Ctrl+C` included: the answer to "does Ctrl+C decline, or interrupt?" is
        // that it interrupts, and the question goes unanswered as a consequence of
        // the turn ending rather than as a second meaning for one key. Every other
        // key is the operator typing prose, which is why this arm is below the
        // approval's and above everything else.
        if let Some(open) = self.intent.as_mut().filter(|_| !interrupting) {
            if let Some(answer) = open.key(key) {
                self.answer_intent(answer);
            }
            return Command::None;
        }
        // And a plan, on the same terms again. `Ctrl+C` ends the turn; every
        // other key is either the approval, the correction being written, or the
        // cancel.
        if let Some(open) = self.plan.as_mut().filter(|_| !interrupting) {
            if let Some(verdict) = open.key(key) {
                self.decide_plan(verdict);
            }
            return Command::None;
        }
        // Any key at all disarms a half-pressed sequence, and the arming is read
        // back here rather than left standing. Taking it before the match means
        // every arm below clears it without having to remember to — the
        // alternative is a `self.armed = None` in a dozen places, one of which
        // would be missing and would leave a destructive key armed across an
        // unrelated keystroke.
        let armed = std::mem::take(&mut self.armed);
        let hit = self.keys.hit(chord, armed);
        // The view owns three keys while it is up, and nothing else: the arrows
        // move the marker and `Esc` closes it. Every other key falls through to
        // the match below, so `Ctrl+C` still interrupts and the composer still
        // takes typing — the view is drawn over the prompt, not in front of the
        // keyboard, because the moment it is worth reading is mid-turn and a
        // reader must not have to close it to stop the turn.
        if self.fleet_open {
            match key.code {
                KeyCode::Up => {
                    self.fleet.move_by(-1);
                    return Command::None;
                }
                KeyCode::Down => {
                    self.fleet.move_by(1);
                    return Command::None;
                }
                // **`Enter` on a detached child, at an empty prompt, and nothing
                // on any other row.** The pane has had a selection since 0.8.0
                // that drove only the highlight; this is what it was for. A
                // detached child is the one an operator can usefully go and look
                // at — it is still running and nothing is watching it — so it is
                // the one row where `Enter` means something.
                //
                // **`self.composer.is_empty()` is the whole guard, and leaving it
                // out is a real defect this arm shipped without for one adversarial
                // review.** This pane is drawn *over the prompt*, not in front of
                // the keyboard — the module comment above says so — and the queue
                // surface below gets it right for exactly this reason. Without the
                // guard, every `Enter` while the pane is open returns here: a line
                // typed to be queued behind a running turn is silently dropped, and
                // `Shift+Enter` never reaches the composer, so a multi-line prompt
                // cannot even be written. Both are invisible — no message, no
                // change on screen — and the pane is open precisely when a turn is
                // running, which is when queueing a line is most likely.
                //
                // With text in the composer `Enter` goes on meaning what it means
                // everywhere else, which is *queue this*.
                KeyCode::Enter if self.composer.is_empty() => {
                    if let Some(child) = self.fleet.selected_child() {
                        if child.state == crate::fleet::State::Detached {
                            return Command::Attach(child.run_id);
                        }
                    }
                    return Command::None;
                }
                // `armed` and not `self.armed`: this function took the arming out
                // a few lines above, so the field is always `None` here and the
                // guard was always true. Pre-existing, and found while writing the
                // queue's version of the same guard — which gets it right, and
                // whose comment describes the trap this one was in.
                KeyCode::Esc if armed.is_none() => {
                    self.fleet_open = false;
                    return Command::None;
                }
                _ => {}
            }
        }
        // The queue surface owns four keys, and only while it is up: the arrows
        // move the mark at an empty prompt, the shifted arrows move the marked
        // line, `Enter` at an empty prompt takes that line into the composer and
        // puts it back when it is done, and `Esc` shuts the surface. Everything
        // else falls through to the match below — `Ctrl+C` still interrupts and
        // the composer still takes typing, which it has to, because typing is how
        // the *next* line joins the queue this is drawing.
        //
        // **Scoped to the open surface, and that scope is the whole binding.**
        // `Up` at the first line of the composer is prompt history and has been
        // since it was documented — `commands::KEYS` carries the row and
        // `tests/docs.rs` mirrors it into the README. Bound at the bare composer
        // these arrows would work perfectly for an operator with something queued
        // and silently cost history to everyone else: a feature nobody asked to
        // trade, broken by a release about a different one. Two guards keep it
        // narrow — the surface has to be open and the prompt has to be empty — so
        // a recall that is *continuing*, and the arrows inside a multi-line prompt
        // being written, never reach this block at all. `Esc` hands both back.
        //
        // An edit in flight keeps these keys live even when the take emptied the
        // queue and closed the surface: otherwise the `Esc` cancelling an edit of
        // the last queued line would fall through and interrupt the turn.
        //
        // It costs the turn one extra `Esc`, and that is the right way round.
        // While a turn runs `Esc` stops it, and an operator who has just been
        // shown a list has a reading of that key which is not "stop the run" — so
        // the first press answers the surface and the second reaches the turn. The
        // same trade the fleet view makes, below it for the same reason: the view
        // that was opened by a key is the one that should close first.
        //
        // Guarded on the *taken* arming rather than on `self.armed`, which this
        // function emptied a few lines above: `Esc` can be the second key of a
        // rebound chord, and a surface that stole it would be answering a sequence
        // the operator was half way through.
        // Not while the fleet view is up: it is drawn in this surface's place and
        // takes the arrows above, so a queue acting here would be a surface acting
        // while invisible — `Enter` at an empty prompt would take a line out of the
        // queue into a composer the fleet is covering.
        if !self.fleet_open && (self.queue_open() || self.queue.editing().is_some()) {
            let dash = self.theme.glyphs.dash;
            match key.code {
                KeyCode::Up | KeyCode::Down
                    if self.queue.editing().is_none() && self.composer.is_empty() =>
                {
                    let delta = if key.code == KeyCode::Up { -1 } else { 1 };
                    // Shifted moves the line, bare moves the mark. `false` means
                    // the key was never ours — nothing marked, or an end of the
                    // list — and it falls through to the composer rather than
                    // being swallowed.
                    let moved = if key.modifiers.contains(KeyModifiers::SHIFT) {
                        self.queue.reorder(delta, &mut self.prompts)
                    } else {
                        self.queue.move_by(delta, self.prompts.len())
                    };
                    if moved {
                        return Command::None;
                    }
                }
                // Finishing an edit is not sending a turn. Above the arm below and
                // above the match: a `Reply::Submitted` reaching `compose` would
                // queue the line a second time.
                KeyCode::Enter if self.queue.editing().is_some() => {
                    let text = self.composer.text();
                    self.composer.clear();
                    let put = self.queue.put_back(&mut self.prompts, &text);
                    self.status.queued_prompts = self.prompts.len();
                    let said = match put {
                        Some(crate::queue::Put::Kept(at)) => {
                            format!("line {} edited {dash} it runs in its own place", at + 1)
                        }
                        Some(crate::queue::Put::Dropped(was)) => format!(
                            "dropped {}{}{} {dash} {} still queued",
                            self.theme.glyphs.quote_open,
                            crate::picker::fit(&was, 32, &self.theme.glyphs),
                            self.theme.glyphs.quote_close,
                            self.prompts.len(),
                        ),
                        None => unreachable!("the arm is guarded on an edit in flight"),
                    };
                    self.say(Tone::Muted, said);
                    return Command::None;
                }
                // Only at an empty prompt, so an edit can never start on top of a
                // half-typed line: with text in the composer `Enter` goes on
                // meaning what it has meant all release, which is *queue this*.
                KeyCode::Enter if self.composer.is_empty() => {
                    if let Some(text) = self.queue.take(&mut self.prompts) {
                        self.composer.set(&text);
                        self.status.queued_prompts = self.prompts.len();
                        let at = self.queue.editing().unwrap_or(0);
                        self.say(
                            Tone::Muted,
                            format!(
                                "editing line {} {dash} Enter puts it back where it was, \
                                 Esc leaves it as it was",
                                at + 1
                            ),
                        );
                        return Command::None;
                    }
                }
                KeyCode::Esc if armed.is_none() => {
                    if self.queue.cancel(&mut self.prompts).is_some() {
                        self.composer.clear();
                        self.status.queued_prompts = self.prompts.len();
                        self.say(
                            Tone::Muted,
                            format!("edit cancelled {dash} the line is as it was"),
                        );
                    } else {
                        self.queue_open = false;
                    }
                    return Command::None;
                }
                _ => {}
            }
        }
        match hit {
            Some(Hit::Fire(Action::Interrupt)) => self.interrupt_or_quit(),
            // The one key in this product that changes the operator's files on
            // io-cli's own initiative rather than the agent's. Every write before
            // it arrived through a tool call and passed a policy layer; this one
            // does not, so it arms, says what it would undo, and waits for the
            // second press.
            //
            // Only at an empty prompt, so it never discards something typed, and
            // never while a turn runs: a rewind moves the conversation head the
            // running turn is about to write to. The guards are on the *action*,
            // so they survive the key being moved — which is the whole reason
            // the rebinding is a lookup in front of this match rather than a
            // rewrite of it.
            Some(hit) if hit.action() == Action::Rewind && self.composer.is_empty() => {
                if self.mode == Mode::Running {
                    // **`Esc` stops the turn.** It is the key every other agent
                    // in this field stops with, and while a turn runs there is
                    // nothing else for it to mean: the rewind it is otherwise
                    // bound to is refused during a turn anyway, because it moves
                    // the head the turn is writing to. So rather than saying no,
                    // it does the thing the operator pressed it for.
                    return self.interrupt_or_quit();
                }
                match hit {
                    Hit::Fire(_) => Command::Rewind,
                    Hit::Arm(action) => {
                        self.armed = Some(action);
                        Command::ArmRewind
                    }
                }
            }
            // The rewind chord when the guard above did not hold — something is
            // typed, or a turn is running. It has to be named HERE, in front of
            // the generic arm below, and that ordering is the whole of it: that
            // arm arms *any* sequence, this one included, so a guard that only
            // rejects the arm above hands the rejected action straight to the
            // next arm and it is armed anyway. The second press then fires the
            // one key in this product that changes the operator's files on
            // io-cli's own initiative, from a prompt with their text visible in
            // it. The `_` arm at the bottom already claimed to cover this case;
            // it never reached it.
            Some(hit) if hit.action() == Action::Rewind => self.compose(key),
            // A sequence bound to anything else. Nothing has happened yet and
            // nothing is said: only the rewind has something to confirm, and a
            // session that narrated every half-pressed chord would be one that
            // narrates typing.
            Some(Hit::Arm(action)) => {
                self.armed = Some(action);
                Command::None
            }
            // Only on an empty composer, so it never discards something typed.
            Some(Hit::Fire(Action::Exit)) if self.composer.is_empty() => Command::Exit,
            Some(Hit::Fire(Action::Exit)) => Command::None,
            Some(Hit::Fire(Action::Posture)) => self.cycle_posture(),
            // The viewport, never the scrollback. Clearing the terminal would
            // destroy the transcript, which on this renderer is the terminal's own
            // buffer rather than something this process can redraw.
            Some(Hit::Fire(Action::Clear)) => Command::ClearViewport,
            // Upward, never into a pane. The conversation is already in the
            // terminal's scrollback; this puts back the part that has scrolled
            // out of reach, branched-away turns included, where the terminal's
            // own search and copy-mode already work on it.
            Some(Hit::Fire(Action::Transcript)) => Command::Transcript,
            // Reachable mid-turn, which is the point of it having a key at all.
            Some(Hit::Fire(Action::Fleet)) => {
                self.toggle_fleet();
                Command::None
            }
            // The rewind chord with something typed, and every key this session
            // does not bind: the composer's, which is where they belong.
            _ => self.compose(key),
        }
    }

    /// Hand the keystroke to the prompt.
    ///
    /// A submitted line is one of three things, decided here by its first
    /// character and nowhere else. `/` is a slash command. `!` is a line for the
    /// operator's own shell — see [`Command::Shell`] — and, like `/`, it is
    /// **not sent to the agent**: the point of `!` is that the agent never hears
    /// about it. Anything else is a prompt.
    ///
    /// `!` with nothing after it is nothing to run, and does nothing. It is not
    /// treated as a prompt, because a bare `!` submitted to a model is a
    /// keystroke that missed rather than a question.
    ///
    /// The first character decides *what* the line is; the mode decides whether a
    /// prompt can be sent at all. A prompt finished while a turn holds the
    /// session is queued rather than returned — see [`App::queue_prompt`].
    fn compose(&mut self, key: KeyEvent) -> Command {
        self.quits = 0;
        match self.composer.key(key) {
            Reply::Idle => Command::None,
            Reply::Submitted(text) => match text.strip_prefix('/') {
                Some(command) => Command::Slash(command.trim().to_string()),
                None => match text.strip_prefix('!').map(str::trim) {
                    Some("") => Command::None,
                    Some(line) => Command::Shell(line.to_string()),
                    // **The guard is here and not in the driver.** A
                    // `Command::Submit` handed out mid-turn is an instruction
                    // nobody can carry out: there is one session, one turn may
                    // hold it, and every caller of [`App::key`] would otherwise
                    // have to know that separately — which is how the driver's
                    // turn loop came to drop this line in its catch-all while the
                    // idle loop ran it. One session-wide fact, asked once, where
                    // the mode is already known.
                    //
                    // The two arms above deliberately keep falling through. A
                    // slash command and a `!` line are *refused* mid-turn with a
                    // sentence, and refusing is right for them: `/model` or
                    // `/fork` held for later would take effect at a moment nobody
                    // could predict, and a shell line is the operator's own,
                    // wanted now or not at all. Only a prompt is the kind of
                    // thing that keeps its meaning after the turn in front of it.
                    None if self.mode == Mode::Running => {
                        self.queue_prompt(text);
                        Command::None
                    }
                    None => Command::Submit(text),
                },
            },
        }
    }

    fn interrupt_or_quit(&mut self) -> Command {
        if self.mode == Mode::Running {
            // **Once asks, twice takes.** The first press cancels through the
            // observer, which io-harness honours at the next step boundary — the
            // run closes itself, the store records how it ended, and the work so
            // far is kept. That is the right stop and it is not always a fast
            // one: a step in the middle of a slow tool call, or a wide fan-out
            // waiting on children, can take seconds to reach a boundary.
            //
            // So the second press does not wait. The driver drops the turn
            // future, which ends it where it stands.
            self.quits = 0;
            // **A turn that has done nothing yet is simply undone.** No boundary
            // to wait for, nothing streamed to keep, and nothing worth a line on
            // screen: the operator pressed the key a moment after Enter, which is
            // the shape of a prompt sent by accident or one they immediately
            // thought better of. The driver takes the goal line back off the
            // screen and puts the prompt back in the composer, so the session is
            // exactly where it was before Enter.
            if self.undoable() {
                return Command::Abandon;
            }
            if self.stopping {
                // Said once, by the end of the turn, and not three times on the
                // way there. Through 0.13.0 this printed `stopping at the next
                // step boundary`, then `stopping now`, then `stopped` — three
                // warning-coloured rows for one decision the operator had
                // already made.
                return Command::Abandon;
            }
            self.stopping = true;
            let where_it_stops = if self.contained {
                "stopping when no child is in flight"
            } else {
                "stopping at the next step"
            };
            // Muted, not warning. Nothing has gone wrong: the operator asked for
            // this, and a colour that means "something is wrong" spends attention
            // it should not.
            self.say(
                Tone::Muted,
                format!("{where_it_stops} — press esc again to stop now"),
            );
            return Command::Interrupt;
        }
        if !self.composer.is_empty() {
            self.composer.clear();
            self.quits = 0;
            return Command::None;
        }
        self.quits += 1;
        if self.quits >= 2 {
            return Command::Exit;
        }
        self.say(Tone::Muted, "press Ctrl+C again to exit, or Ctrl+D");
        Command::None
    }

    /// Draw the viewport: streaming text, then the composer, then the status line.
    ///
    /// Content before metadata, top to bottom, so a reader reaches the model's
    /// words before the token count.
    pub fn render(&self, frame: &mut Frame, area: Rect) {
        if area.height == 0 {
            return;
        }
        // A question takes the whole viewport while it is up. There is nothing to
        // type at — the run is stopped — and the alternative, squeezing it beside
        // a composer nobody can use, is how an approval ends up too small to read.
        if let Some(open) = &self.approval {
            open.render(frame, area, &self.theme);
            return;
        }
        // The same rule for the same reason: the run is stopped waiting on prose,
        // so there is nothing behind this worth half a screen.
        if let Some(open) = &self.intent {
            open.render(frame, area, &self.theme);
            return;
        }
        // A plan needs the rows most of the three: it is a list, and a list
        // truncated to fit beside a composer is a plan somebody approved without
        // having read the end of it.
        if let Some(open) = &self.plan {
            open.render(frame, area, &self.theme);
            return;
        }
        // The activity line while a turn is in flight, one row for the streaming
        // tail, one for the status line, the rest for the composer. Content
        // before metadata, top to bottom, so a reader reaches the model's words
        // before the token count.
        //
        // The rows are claimed in the order they can be given up: the composer
        // last, then the status line, then the live row, and the activity line
        // first — it is the newest row and the one a session can be read
        // without. At an idle prompt it costs nothing at all, because there is
        // no turn for it to be about and the composer takes the row back.
        // **Three rows, not one: a rule and two lines of footer.** They go last
        // and come back first, in that order — the identity row is what a
        // terminal too short for the rest keeps, because a footer that can only
        // say one thing should say what this session is and whether it is
        // working.
        let status_rows = match area.height {
            0..=1 => 0,
            2..=6 => 1,
            _ => 3,
        };
        let live_rows = u16::from(area.height >= 3);
        // **The row is reserved whether or not there is a turn to put in it.**
        // Drawn only while one is in flight — that is F5 — but *claimed* always,
        // because a composer that is three rows at an idle prompt and two while a
        // turn runs moves the prompt up a row the moment you press Enter and back
        // down when it finishes. The row costs nothing when it is empty and the
        // layout is worth more than the row.
        // On a terminal too short for this release's viewport the rows that go
        // are these two, and the composer keeps the two it has had since 0.1.0.
        // A one-row composer is a prompt you cannot read a pasted line in.
        let activity_rows = u16::from(area.height >= 7);
        // The blank above the activity line. It is the last row claimed and the
        // first given up, because it carries nothing — but what it buys is the
        // sticky row reading as a header over the work rather than as the last
        // line of it.
        //
        // **The queue takes it while it is open, and this is the whole reason
        // the surface is visible at all (0.17.0).** At the viewport a running
        // turn actually asks for — `term::VIEWPORT_HEIGHT`, eight rows — the
        // composer's allowance works out to exactly `COMPOSER_ROWS`, so there is
        // no spare row above it and a surface drawn there would draw nothing on
        // every real session. The alternatives were to grow the viewport, which
        // costs every session a row of scrollback for a surface that is empty
        // almost always, or to take the composer's own row, which is the one
        // thing this layout has refused since 0.1.0. The blank is the honest
        // third answer: it carries nothing by its own argument above, the queue
        // carries something, and the frame is the same height either way — which
        // is what N2 is really about. It comes back the moment the queue closes.
        // `&& !fleet_open` because the fleet view is drawn INSTEAD of the queue,
        // in the composer's own rect. Without that clause, queueing a line behind
        // an open fleet view took the blank row for a surface that then did not
        // draw — the row bought nothing and the fleet quietly grew by one.
        let air_rows = u16::from(area.height >= 8 && !self.queue_drawn());
        // **A rule over the composer, matching the one under it.** The footer has
        // opened with one since 0.1.0, and the prompt had a boundary on one side
        // only — so the composer read as part of whatever the turn had last
        // written rather than as the field it is. It is the second row given up
        // on a short terminal, after the blank: a boundary is worth less than the
        // row it would take from the prompt itself.
        let rule_rows = u16::from(area.height >= 8);
        let activity = if activity_rows == 1 {
            self.status.activity(area.width, &self.theme)
        } else {
            None
        };
        let composer_rows =
            area.height - air_rows - activity_rows - live_rows - rule_rows - status_rows;

        // **The work first, then the line that says it is working.** Up to
        // 0.13.0 the streaming row was drawn under the activity line, so the
        // newest words the agent had written read as a footnote to a spinner
        // rather than as the transcript continuing — and the transcript is
        // directly above them. The order is now: what was said, a row of air, and
        // then the state of the turn, sitting immediately over the composer where
        // the operator's attention already is. The blank is still the first row
        // given up on a short terminal, and giving it up does not reorder
        // anything.
        if live_rows > 0 {
            let live = Rect {
                y: area.y,
                height: live_rows,
                ..area
            };
            frame.render_widget(
                Paragraph::new(self.events.live().to_string())
                    .wrap(ratatui::widgets::Wrap { trim: false }),
                live,
            );
        }

        if let Some(activity) = activity {
            frame.render_widget(
                Paragraph::new(activity),
                Rect {
                    y: area.y + live_rows + air_rows,
                    height: 1,
                    ..area
                },
            );
        }

        if rule_rows > 0 {
            frame.render_widget(
                Paragraph::new(Line::from(ratatui::text::Span::styled(
                    self.theme
                        .glyphs
                        .rule
                        .to_string()
                        .repeat(usize::from(area.width)),
                    self.theme.style(Tone::Muted),
                ))),
                Rect {
                    y: area.y + live_rows + air_rows + activity_rows,
                    height: 1,
                    ..area
                },
            );
        }

        let composer = Rect {
            y: area.y + live_rows + air_rows + activity_rows + rule_rows,
            height: composer_rows,
            ..area
        };
        // Over the composer and never over the status line: the spend field is
        // on that line, and a view of what the fan-out is doing that hid what it
        // was costing would be the wrong half of the release.
        if self.fleet_open {
            self.fleet.render(frame, composer, &self.theme);
        } else {
            // **The queue takes what the composer can spare, and the subtraction
            // above is untouched.** Every term in `composer_rows` is a row the
            // frame already had; a term for the queue would be a frame that grew
            // with the queue, which is a session whose own scrollback is walked
            // upward one row per line typed into it. So the rows come out of the
            // composer's allowance the way the fleet view's do — the difference
            // being that the fleet takes all of them and this takes only what is
            // left over `COMPOSER_ROWS`, because a prompt nobody can see is a
            // worse trade than a queue nobody can see.
            //
            // At the viewport height a running turn holds that leaves exactly one
            // row, and only because the blank above the activity line is released
            // while the queue is open — see `air_rows`. Without it `composer_rows`
            // is exactly `COMPOSER_ROWS` at eight rows and this would draw nothing
            // on every real session. Below eight there is no blank to release and
            // it draws nothing, which is F2's "on a terminal tall enough to hold
            // them" in one line of arithmetic rather than a height compared
            // against a number.
            let spare = composer.height.saturating_sub(COMPOSER_ROWS);
            let want = u16::try_from(self.prompts.len()).unwrap_or(u16::MAX);
            let queue_rows = if self.queue_open() {
                spare.min(want)
            } else {
                0
            };
            if queue_rows > 0 {
                crate::queue::render(
                    &self.prompts,
                    self.queue.selection(self.prompts.len()),
                    frame,
                    Rect {
                        height: queue_rows,
                        ..composer
                    },
                    &self.theme,
                );
            }
            // Under the queue and never over it: the rows go in send order, and
            // the row the operator is typing into is the one that has not been
            // sent at all, so it belongs at the bottom of that order.
            self.composer.render(
                frame,
                Rect {
                    y: composer.y + queue_rows,
                    height: composer.height - queue_rows,
                    ..composer
                },
                &self.theme,
            );
        }

        if status_rows > 0 {
            let status = Rect {
                y: area.y + live_rows + air_rows + activity_rows + rule_rows + composer_rows,
                height: status_rows,
                ..area
            };
            self.status.render(frame, status, &self.theme);
        }
    }
}
