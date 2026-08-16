//! The renderer: an inline viewport with the transcript in the terminal's own
//! scrollback.
//!
//! There is one rendering model in this product and this module is all of it.
//! Finished content is handed to [`Screen::commit`], which pushes it *above* the
//! viewport with `Terminal::insert_before`, so it lands in the terminal's real
//! scrollback and stays there after the process exits. The viewport itself is a
//! few lines at the bottom holding the composer and the status line, and it is
//! the only region that repaints.
//!
//! Three properties are structural rather than conventional, and each has a test
//! that fails if it is lost:
//!
//! - **No alternate screen and no mouse capture.** There is no code path here
//!   that enters one or requests the other, in any mode, behind any flag, which
//!   is why the terminal's own search, selection and copy-mode keep working.
//!   `tests/structure.rs` asserts it over the byte stream.
//! - **No full-screen clear.** A clear is a bug, not a redraw strategy;
//!   `Terminal::clear` on an inline viewport erases from the cursor down, which
//!   is the viewport and nothing above it.
//! - **The terminal is always given back.** A panic hook restores it before the
//!   panic message is printed, and [`Drop`] restores it on every other path.

use std::io::{self, Write};
use std::sync::{Mutex, OnceLock};

use crossterm::terminal::{BeginSynchronizedUpdate, EndSynchronizedUpdate};
use ratatui::backend::{Backend, CrosstermBackend};
use ratatui::layout::Rect;
use ratatui::text::{Line, Text};
use ratatui::widgets::{Paragraph, Widget, Wrap};
use ratatui::{Frame, Terminal, TerminalOptions, Viewport};

/// Lines the live viewport occupies: the unfinished tail of a streaming answer,
/// two rows of composer, and the status line.
///
/// Fixed, and deliberately small. ratatui sets an inline viewport's height when
/// the terminal is constructed and there is no way to change it afterwards short
/// of rebuilding — which would re-query the cursor and risk shifting the
/// scrollback this product exists to protect. So the viewport does not grow;
/// instead everything that can be committed is committed, which is why a
/// streaming answer commits each line as it finishes and leaves only the tail
/// here.
///
/// The ceiling that buys: a prompt longer than two rows scrolls within them
/// rather than expanding the viewport. The composer's own release is 0.7.0.
pub const VIEWPORT_HEIGHT: u16 = 4;

/// What to run when the terminal has to be handed back.
type Restore = Box<dyn Fn() + Send + Sync + 'static>;

/// The renderer.
///
/// Generic over the backend so the tests can drive a real `CrosstermBackend`
/// writing into a recorder rather than into a tty. The bound includes [`Write`]
/// because the synchronized-output sequences are written to the backend directly:
/// they wrap the frame ratatui draws, so they cannot be a widget.
pub struct Screen<B: Backend + Write> {
    terminal: Terminal<B>,
    /// What the last frame drew, kept because ratatui's rendered buffer is not
    /// reachable once `draw` has returned. See [`Screen::viewport_text`].
    viewport: String,
    restore: Option<Restore>,
    restored: bool,
}

impl Screen<CrosstermBackend<io::Stdout>> {
    /// Take the terminal: raw mode on, bracketed paste on, and a panic hook that
    /// gives it back before anything is printed.
    ///
    /// Raw mode is the only thing taken. The alternate screen is not entered and
    /// the mouse is not captured, so the scrollback, the terminal's search and its
    /// selection are all still the terminal's own.
    pub fn attach() -> io::Result<Self> {
        crossterm::terminal::enable_raw_mode()?;

        // From here on the terminal is raw, so EVERY failure path has to give it
        // back before returning. Found the hard way: placing the inline viewport
        // asks the terminal where its cursor is and reads the answer back off
        // stdin, and a terminal that does not answer left the process exiting
        // with the user's shell still in raw mode — no echo, no line editing, and
        // an error message that did not say what had happened.
        Self::attach_raw().inspect_err(|_| restore_terminal())
    }

    fn attach_raw() -> io::Result<Self> {
        let mut out = io::stdout();
        crossterm::execute!(out, crossterm::event::EnableBracketedPaste)?;

        let terminal = Terminal::with_options(
            CrosstermBackend::new(out),
            TerminalOptions {
                viewport: Viewport::Inline(VIEWPORT_HEIGHT),
            },
        )
        .map_err(|error| {
            io::Error::new(
                error.kind(),
                format!(
                    "{error}. io asks the terminal where its cursor is before it \
                     draws anything, and this one did not answer. That usually \
                     means stdout is not a real terminal — a pipe, a CI job, or a \
                     pty with nothing behind it. `io exec` and a non-interactive \
                     mode are 0.5.0."
                ),
            )
        })?;

        install_panic_hook(restore_terminal);

        Ok(Self {
            terminal,
            viewport: String::new(),
            restore: Some(Box::new(restore_terminal)),
            restored: false,
        })
    }
}

impl<B: Backend + Write> Screen<B> {
    /// Wrap a terminal that has already been built. The tests' way in.
    pub fn from_terminal(terminal: Terminal<B>) -> Self {
        Self {
            terminal,
            viewport: String::new(),
            restore: None,
            restored: false,
        }
    }

    /// Push finished content into the terminal's scrollback, above the viewport.
    ///
    /// The height is measured rather than estimated. `insert_before` inserts
    /// exactly the number of rows it is given, so a caller that guesses low
    /// truncates the content — which on this renderer means losing part of the
    /// transcript permanently, since the viewport is not where it lives.
    pub fn commit(&mut self, lines: &[Line<'_>]) -> io::Result<()> {
        if lines.is_empty() {
            return Ok(());
        }

        let width = self.terminal.current_buffer_mut().area.width.max(1);
        let text = Text::from(lines.to_vec());
        let paragraph = Paragraph::new(text).wrap(Wrap { trim: false });
        let height = u16::try_from(paragraph.line_count(width)).unwrap_or(u16::MAX);
        if height == 0 {
            return Ok(());
        }

        self.terminal
            .insert_before(height, |buf| paragraph.render(buf.area, buf))
    }

    /// Draw one viewport frame, wrapped in synchronized output.
    ///
    /// The wrapping is what stops a streaming turn from strobing: the terminal is
    /// told to hold the display until the frame is complete, so a partially drawn
    /// composer is never presented. ratatui already diffs the buffer, which
    /// decides *what* is written; this decides *when* it becomes visible.
    pub fn draw<F: FnOnce(&mut Frame)>(&mut self, render: F) -> io::Result<()> {
        crossterm::queue!(self.terminal.backend_mut(), BeginSynchronizedUpdate)?;
        let mut drawn = String::new();
        self.terminal.draw(|frame| {
            render(frame);
            // Captured here because ratatui swaps its two buffers as the frame
            // ends: after `draw` returns, the current buffer is the cleared one
            // and what was just rendered is in the other, which has no public
            // accessor. Inside the closure is the only place it can be read.
            drawn = buffer_text(frame.buffer_mut());
        })?;
        self.viewport = drawn;
        crossterm::queue!(self.terminal.backend_mut(), EndSynchronizedUpdate)?;
        // `Backend` and `Write` both name a `flush`; the write one is meant here,
        // because the queued escape sequences are sitting in the writer.
        Write::flush(self.terminal.backend_mut())
    }

    /// Tell the renderer the terminal is a different size now.
    ///
    /// Recomputing the inline viewport is the whole job: the committed lines above
    /// it belong to the terminal and must not be redrawn, which is what produces
    /// the duplicated history a full-screen renderer shows on resize.
    pub fn resize(&mut self, width: u16, height: u16) -> io::Result<()> {
        self.terminal.resize(Rect::new(0, 0, width, height))
    }

    /// The width the renderer is currently laying out against, in cells.
    pub fn width(&mut self) -> u16 {
        self.terminal.current_buffer_mut().area.width
    }

    /// What the last frame put in the viewport, one row per line with trailing
    /// spaces trimmed. Empty until the first [`Screen::draw`].
    pub fn viewport_text(&self) -> &str {
        &self.viewport
    }

    /// Replace what runs when the terminal is handed back. Used by the tests, and
    /// by nothing else.
    pub fn on_restore<F: Fn() + Send + Sync + 'static>(&mut self, restore: F) {
        self.restore = Some(Box::new(restore));
    }

    /// Hand the terminal back: cooked mode, cursor shown, viewport left where it
    /// is so the transcript above it survives.
    ///
    /// Idempotent. `Drop` calls it, and so does an orderly exit, and restoring a
    /// terminal twice would show a cursor over whatever owns it by then.
    pub fn restore(&mut self) {
        if self.restored {
            return;
        }
        self.restored = true;
        if let Some(restore) = &self.restore {
            restore();
        }
    }

    /// The terminal underneath, for the few callers that need to ask it something.
    pub fn terminal_mut(&mut self) -> &mut Terminal<B> {
        &mut self.terminal
    }
}

impl<B: Backend + Write> Drop for Screen<B> {
    fn drop(&mut self) {
        self.restore();
    }
}

/// A buffer's contents as text: one row per line, trailing spaces trimmed.
fn buffer_text(buffer: &ratatui::buffer::Buffer) -> String {
    let area = buffer.area;
    (area.top()..area.bottom())
        .map(|y| {
            let row: String = (area.left()..area.right())
                .map(|x| buffer[(x, y)].symbol())
                .collect();
            row.trim_end().to_string()
        })
        .collect::<Vec<_>>()
        .join("\n")
}

/// Put the real terminal back: bracketed paste off, cursor shown, cooked mode.
///
/// Deliberately ignores its errors. It runs on the panic path, where returning a
/// `Result` nobody can act on would mean the terminal stays raw because restoring
/// it failed halfway.
pub fn restore_terminal() {
    let mut out = io::stdout();
    let _ = crossterm::execute!(
        out,
        crossterm::event::DisableBracketedPaste,
        crossterm::cursor::Show
    );
    let _ = crossterm::terminal::disable_raw_mode();
    let _ = out.flush();
}

/// The hook installed by [`Screen::attach`], kept so a second `attach` in one
/// process replaces the closure instead of stacking another hook on top of the
/// one that is already chained.
static HOOK: OnceLock<Mutex<Option<Restore>>> = OnceLock::new();

/// Run `restore` on a panic, *before* the previous hook prints anything.
///
/// The order is the whole point. A panic message printed into a raw-mode terminal
/// arrives without carriage returns, staircased down the screen, and leaves the
/// user in a shell that no longer echoes — which reads as the tool having crashed
/// the terminal rather than having crashed.
pub fn install_panic_hook<F: Fn() + Send + Sync + 'static>(restore: F) {
    let slot = HOOK.get_or_init(|| Mutex::new(None));
    let first = {
        let mut guard = slot.lock().expect("panic hook slot poisoned");
        let first = guard.is_none();
        *guard = Some(Box::new(restore));
        first
    };

    // Chain onto the previous hook exactly once. Every later call swaps the
    // closure the chained hook reads, so repeated `attach` calls in one process
    // do not nest hooks.
    if first {
        let previous = std::panic::take_hook();
        std::panic::set_hook(Box::new(move |info| {
            if let Some(slot) = HOOK.get() {
                if let Ok(guard) = slot.lock() {
                    if let Some(restore) = guard.as_ref() {
                        restore();
                    }
                }
            }
            previous(info);
        }));
    }
}
