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
    /// Whether the last keystroke was the first `Esc` of a rewind.
    ///
    /// Cleared by *any* other key rather than by a timer, so nothing here reads a
    /// clock and an arming cannot outlive the moment the operator meant it.
    armed: bool,
    /// Lines waiting to be committed to scrollback.
    pending: Vec<Line<'static>>,
    /// The question on screen, if the run is waiting on one.
    ///
    /// It lives here rather than beside the picker in the driver because it is
    /// not a choice the operator went looking for: it arrives mid-turn, it takes
    /// the keyboard while it is up, and the run is stopped until it is gone.
    approval: Option<Approval>,
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
            armed: false,
            pending: Vec::new(),
            approval: None,
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

    /// The posture in force.
    /// Whether the next `Esc` at an empty prompt would perform a rewind.
    ///
    /// Read by the tests rather than by the driver, which is told what to do by
    /// the `Command` it gets back. A destructive key needs its state assertable
    /// from outside, or "one press does nothing" is a claim with nothing behind
    /// it.
    pub fn armed(&self) -> bool {
        self.armed
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
        self.approval.is_some()
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
        // A question outlives its run only as a stuck overlay over a session that
        // has moved on. Dropping it is the denial — see `Ask` — and the run it
        // belonged to has already ended, so there is nobody left to tell.
        self.approval = None;
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

    /// The status line's share of an event.
    ///
    /// Only the events that carry a fact set a field, and nothing sets one to a
    /// default. A field this has never heard about stays `None`, which is what the
    /// line renders as nothing at all rather than as a zero.
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
            io_harness::EventKind::Contained { mode, backend, .. } => {
                self.status.containment = Some(crate::status::format_containment(mode, backend));
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

    pub fn key(&mut self, key: KeyEvent) -> Command {
        let control = key.modifiers.contains(KeyModifiers::CONTROL);
        // An open question takes the keyboard, except for `Ctrl+C`. That is the
        // answer to "does `Ctrl+C` deny, or interrupt?": it interrupts, and the
        // question is denied as a consequence of the turn ending rather than as a
        // second meaning for one key. Nothing else can reach the composer while a
        // run is stopped waiting on an answer.
        let interrupting = control && key.code == KeyCode::Char('c');
        if let Some(open) = self.approval.as_mut().filter(|_| !interrupting) {
            if let Some(answer) = open.key(key) {
                self.answer_approval(answer);
            }
            return Command::None;
        }
        // Any key at all disarms a pending rewind, and the arming is read back
        // here rather than left standing. Taking it before the match means every
        // arm below clears it without having to remember to — the alternative is
        // a `self.armed = false` in a dozen places, one of which would be missing
        // and would leave a destructive key armed across an unrelated keystroke.
        let was_armed = std::mem::take(&mut self.armed);
        match (key.code, control) {
            (KeyCode::Char('c'), true) => self.interrupt_or_quit(),
            // The one key in this product that changes the operator's files on
            // io-cli's own initiative rather than the agent's. Every write before
            // it arrived through a tool call and passed a policy layer; this one
            // does not, so it arms, says what it would undo, and waits for the
            // second press.
            //
            // Only at an empty prompt, so it never discards something typed, and
            // never while a turn runs: a rewind moves the conversation head the
            // running turn is about to write to.
            (KeyCode::Esc, false) if self.composer.is_empty() => {
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
                if was_armed {
                    Command::Rewind
                } else {
                    self.armed = true;
                    Command::ArmRewind
                }
            }
            // Two spellings of one key. A terminal without the Kitty keyboard
            // protocol sends `BackTab` with no modifier; one that has negotiated it
            // sends `Tab` with shift. Binding either alone ships a key that works
            // on the developer's terminal and silently does nothing on somebody
            // else's, which is worse than not shipping it.
            (KeyCode::BackTab, _) => self.cycle_posture(),
            (KeyCode::Tab, _) if key.modifiers.contains(KeyModifiers::SHIFT) => {
                self.cycle_posture()
            }
            // Only on an empty composer, so it never discards something typed.
            (KeyCode::Char('d'), true) if self.composer.is_empty() => Command::Exit,
            (KeyCode::Char('d'), true) => Command::None,
            // The viewport, never the scrollback. Clearing the terminal would
            // destroy the transcript, which on this renderer is the terminal's own
            // buffer rather than something this process can redraw.
            (KeyCode::Char('l'), true) => Command::ClearViewport,
            // Upward, never into a pane. The conversation is already in the
            // terminal's scrollback; this puts back the part that has scrolled
            // out of reach, branched-away turns included, where the terminal's
            // own search and copy-mode already work on it.
            (KeyCode::Char('t'), true) => Command::Transcript,
            _ => {
                self.quits = 0;
                match self.composer.key(key) {
                    Reply::Idle => Command::None,
                    Reply::Submitted(text) => match text.strip_prefix('/') {
                        Some(command) => Command::Slash(command.trim().to_string()),
                        None => Command::Submit(text),
                    },
                }
            }
        }
    }

    fn interrupt_or_quit(&mut self) -> Command {
        if self.mode == Mode::Running {
            // The turn is what gets stopped, not the process. `Steer::interrupt`
            // ends it at a step boundary, whole, and leaves it resumable.
            self.quits = 0;
            self.say(Tone::Warning, "interrupting at the next step boundary");
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
        self.composer.render(frame, composer, &self.theme);

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
