//! The session's own state: what a keystroke means, and what the viewport holds.
//!
//! Kept apart from the code that drives io-harness so that both are testable. The
//! driver in `main.rs` owns the provider, the store and the turn future; this owns
//! everything a test needs to answer "what does `Ctrl+C` do in the middle of a
//! streaming turn".

use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use io_harness::RunEvent;
use ratatui::layout::Rect;
use ratatui::text::Line;
use ratatui::widgets::Paragraph;
use ratatui::Frame;

use crate::composer::{Composer, Reply};
use crate::events::Events;
use crate::status::Status;
use crate::term::VIEWPORT_HEIGHT;
use crate::theme::{Theme, Tone};

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
    /// Lines waiting to be committed to scrollback.
    pending: Vec<Line<'static>>,
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
            pending: Vec::new(),
        }
    }

    pub fn mode(&self) -> Mode {
        self.mode
    }

    /// A turn started.
    pub fn started(&mut self) {
        self.mode = Mode::Running;
        self.status.working = true;
        self.quits = 0;
    }

    /// A turn ended, however it ended — finished, cancelled or failed.
    ///
    /// Whatever streamed and was never closed by an event is committed here, so an
    /// interrupted turn keeps its partial output in the scrollback instead of
    /// losing it with the turn.
    pub fn finished(&mut self) {
        self.mode = Mode::Idle;
        self.status.working = false;
        let tail = self.events.flush();
        self.pending.extend(tail);
    }

    /// Take an event from the harness.
    pub fn event(&mut self, event: &RunEvent) {
        let lines = self.events.event(event);
        self.pending.extend(lines);
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
        match (key.code, control) {
            (KeyCode::Char('c'), true) => self.interrupt_or_quit(),
            // Only on an empty composer, so it never discards something typed.
            (KeyCode::Char('d'), true) if self.composer.is_empty() => Command::Exit,
            (KeyCode::Char('d'), true) => Command::None,
            // The viewport, never the scrollback. Clearing the terminal would
            // destroy the transcript, which on this renderer is the terminal's own
            // buffer rather than something this process can redraw.
            (KeyCode::Char('l'), true) => Command::ClearViewport,
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
