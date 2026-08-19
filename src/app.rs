//! The session's own state: what a keystroke means, and what the viewport holds.
//!
//! Kept apart from the code that drives io-harness so that both are testable. The
//! driver in `main.rs` owns the provider, the store and the turn future; this owns
//! everything a test needs to answer "what does `Ctrl+C` do in the middle of a
//! streaming turn".

use std::time::Duration;

use crossterm::event::{KeyCode, KeyEvent};
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
    /// Call `Steer::interrupt` on the running turn.
    Interrupt,
    /// Leave.
    Exit,
    /// Repaint the viewport from scratch, never the scrollback.
    ClearViewport,
    /// Put the whole conversation back into the terminal's own scrollback.
    ///
    /// A command rather than something this type does, because the transcript
    /// lives in the harness's store and the store belongs to the driver.
    Transcript,
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
    /// Whether the turn now running is a *contained* turn.
    ///
    /// Set by the driver as a turn starts and cleared as it ends. It exists for
    /// one sentence: `Ctrl+C` stops a steered turn at the next step boundary and
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
            fleet: crate::fleet::Fleet::new(),
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
    pub fn set_root(&mut self, root: impl Into<std::path::PathBuf>) {
        self.root = root.into();
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
            Some(text) => self.say(Tone::Muted, format!("answered {} {text}", self.theme.glyphs.dash)),
            None => self.say(
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
            io_harness::PlanVerdict::Cancel => (
                Tone::Refused,
                format!("plan cancelled {dash} nothing ran"),
            ),
        };
        self.say(tone, said);
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
        self.set_posture(Some(next));
        self.say(
            Tone::Muted,
            format!(
                "policy:{} {} {}",
                next.short(),
                self.theme.glyphs.dash,
                next.detail()
            ),
        );
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
    pub fn paste(&mut self, text: &str, picker_open: bool) -> bool {
        if picker_open || self.modal() {
            return false;
        }
        self.composer.paste(text);
        true
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
        self.say(
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
        self.status.working = true;
        self.quits = 0;
        self.announce();
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
        self.say(Tone::Muted, state);
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

    /// The status line's share of an event.
    ///
    /// Only the events that carry a fact set a field, and nothing sets one to a
    /// default. A field this has never heard about stays `None`, which is what the
    /// line renders as nothing at all rather than as a zero.
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

    /// Open the view, or close it.
    ///
    /// Opening a view of nothing is not refused: a session in contained mode
    /// before its first spawn has an answer to give — "nothing has been spawned
    /// yet" — and a key that appeared to do nothing would read as broken.
    pub fn toggle_fleet(&mut self) {
        self.fleet_open = !self.fleet_open;
    }

    fn status_from(&mut self, event: &RunEvent) {
        match &event.kind {
            io_harness::EventKind::Step { tokens, .. } => {
                // The session's total, not the step's own. A field that swings
                // rather than climbs cannot be read at a glance.
                self.status.tokens = Some(self.status.tokens.unwrap_or(0) + tokens);
            }
            // The run's own total, which is authoritative over the sum of the steps
            // we happened to see. Guarded on the tag rather than inside the arm:
            // a run that reported no usage at all reports `0`, and overwriting a
            // real total with it would turn a known number into a wrong one.
            io_harness::EventKind::Finished { tokens, .. } if *tokens > 0 => {
                self.status.tokens = Some(*tokens);
            }
            io_harness::EventKind::Compacted { after_tokens, .. } => {
                // The denominator is io-harness's own declared budget, asked of the
                // harness rather than copied here — a `24_000` written into this
                // file would be wrong after some harness patch, and wrong silently.
                let budget = io_harness::ContextBudget::default().effective_tokens(None);
                let share = (*after_tokens as f64 / budget.max(1) as f64 * 100.0).round();
                self.status.context = Some(share.clamp(0.0, 100.0) as u8);
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

    /// Add a line of io-cli's own, rather than the harness's.
    pub fn say(&mut self, tone: Tone, text: impl Into<String>) {
        let line = self.theme.notice(tone, text);
        self.pending.push(line);
    }

    /// Everything waiting to go into the terminal's scrollback, emptied.
    pub fn take_pending(&mut self) -> Vec<Line<'static>> {
        std::mem::take(&mut self.pending)
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
                KeyCode::Esc if self.armed.is_none() => {
                    self.fleet_open = false;
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
                    self.say(
                        Tone::Muted,
                        format!(
                            "not while a turn is running {} a rewind moves the head this \
                             turn is writing to",
                            self.theme.glyphs.dash
                        ),
                    );
                    return Command::None;
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
    fn compose(&mut self, key: KeyEvent) -> Command {
        self.quits = 0;
        match self.composer.key(key) {
            Reply::Idle => Command::None,
            Reply::Submitted(text) => match text.strip_prefix('/') {
                Some(command) => Command::Slash(command.trim().to_string()),
                None => match text.strip_prefix('!').map(str::trim) {
                    Some("") => Command::None,
                    Some(line) => Command::Shell(line.to_string()),
                    None => Command::Submit(text),
                },
            },
        }
    }

    fn interrupt_or_quit(&mut self) -> Command {
        if self.mode == Mode::Running {
            // The turn is what gets stopped, not the process. `Steer::interrupt`
            // ends it at a step boundary, whole, and leaves it resumable.
            self.quits = 0;
            // The two paths end a turn at different moments, and the sentence
            // says which one this is. A contained turn is cancelled through the
            // observer, and io-harness honours that at the next boundary where
            // no child is in flight — so a wide fan-out can take a while, and an
            // operator told "the next step boundary" would think the key missed.
            let where_it_stops = if self.contained {
                "cancelling at the next point where no child is in flight"
            } else {
                "interrupting at the next step boundary"
            };
            self.say(Tone::Warning, where_it_stops);
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
        // One row for the streaming tail, one for the status line, the rest for
        // the composer. Content before metadata, top to bottom, so a reader
        // reaches the model's words before the token count.
        let live_rows = u16::from(area.height >= 3);
        let status_rows = u16::from(area.height >= 2);
        let composer_rows = area.height - live_rows - status_rows;

        if live_rows > 0 {
            let live = Rect {
                height: live_rows,
                ..area
            };
            frame.render_widget(
                Paragraph::new(self.events.live().to_string())
                    .wrap(ratatui::widgets::Wrap { trim: false }),
                live,
            );
        }

        let composer = Rect {
            y: area.y + live_rows,
            height: composer_rows,
            ..area
        };
        // Over the composer and never over the status line: the spend field is
        // on that line, and a view of what the fan-out is doing that hid what it
        // was costing would be the wrong half of the release.
        if self.fleet_open {
            self.fleet.render(frame, composer, &self.theme);
        } else {
            self.composer.render(frame, composer, &self.theme);
        }

        if status_rows == 1 {
            let status = Rect {
                y: area.y + live_rows + composer_rows,
                height: 1,
                ..area
            };
            self.status.render(frame, status, &self.theme);
        }
    }
}
