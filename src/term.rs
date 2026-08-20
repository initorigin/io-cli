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
//! Five properties are structural rather than conventional, and each has a test
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
//! - **What is pushed is popped.** The Kitty keyboard protocol is negotiated up
//!   on the terminals that advertise it, and every way out of the process — an
//!   orderly exit, a [`Drop`], a panic — pops it again. `tests/keyboard.rs`
//!   asserts the two balance in the byte stream, panic included: a protocol left
//!   pushed outlives the process, and what inherits it is the user's shell.
//! - **A frame whose content did not change is not drawn.** Not drawn cheaply:
//!   not drawn at all, no bytes. [`Screen::draw`] lays every frame out where
//!   nothing can see it first, and only presents it if it differs from what the
//!   terminal is already showing. `tests/frames.rs` asserts it over the byte
//!   count, which is the one thing that separates a skipped repaint from a
//!   cheap one.

use std::io::{self, Write};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Mutex, OnceLock};

use crossterm::event::{
    KeyboardEnhancementFlags, PopKeyboardEnhancementFlags, PushKeyboardEnhancementFlags,
};
use crossterm::terminal::{BeginSynchronizedUpdate, EndSynchronizedUpdate};
use crossterm::Command;
use ratatui::backend::{Backend, ClearType, CrosstermBackend, WindowSize};
use ratatui::buffer::Buffer;
use ratatui::layout::{Position, Rect, Size};
use ratatui::text::{Line, Text};
use ratatui::widgets::{Paragraph, Widget, Wrap};
use ratatui::{Frame, Terminal, TerminalOptions, Viewport};

/// Lines the live viewport occupies: a blank row, the activity line, the
/// unfinished tail of a streaming answer, two rows of composer, and the status
/// line.
///
/// **Six since 0.11.0, and two of them are new.** The activity line buys the one
/// thing the other four could not say — that a turn is alive and how long it has
/// been — and the blank row above it buys the thing a sticky row cannot have
/// otherwise: air between it and the transcript scrolling underneath. Committed
/// content ends exactly where the viewport begins, so without a row of its own
/// the activity line reads as the last line of the work rather than as the line
/// describing it.
///
/// Both rows are *claimed* whether or not a turn is running and *drawn* only
/// while one is, so the composer is two rows at every moment of a session. A
/// composer that changed height between turns moved the prompt under the
/// operator's hands on every Enter.
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
/// rather than expanding the viewport. 0.7.0 spends that ceiling rather than
/// raising it — the palette and path completion are pickers drawn into these
/// same four rows, a picker's query is drawn in place of its title so it costs
/// no row, and a paste too big for two rows becomes one line naming itself
/// instead of a prompt that has to grow.
pub const VIEWPORT_HEIGHT: u16 = 6;

/// Rows the wizard's viewport occupies.
///
/// Much taller than the session's, and it has to be: the wizard's screens are
/// pickers, and a picker draws `height - 1` rows. At the session's four that is
/// three visible options — which made a four-hundred-row model list unusable and
/// left the theme step, whose picker shares its space with a live sample,
/// drawing NO picker at all. A live first run found both.
///
/// A wizard viewport can afford to be large because nothing is streaming under
/// it and the transcript it would otherwise crowd out does not exist yet. It is
/// clamped to the terminal, so an eighty-by-twenty-four terminal gets what it can
/// spare rather than more rows than it has.
pub const WIZARD_VIEWPORT_HEIGHT: u16 = 14;

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
    /// Where a frame is laid out before anyone can see it. See [`Screen::draw`].
    probe: Terminal<Probe>,
    /// What the terminal is currently showing, and where the frame that put it
    /// there asked for the cursor. `None` means the next frame must be drawn
    /// whatever it contains, because something outside `draw` — a commit, a
    /// resize — has since erased the viewport.
    last: Option<(Buffer, Option<Position>)>,
    /// The terminal size the last frame was laid out against, kept only to
    /// notice that it changed. See [`Screen::draw`].
    size: Option<Size>,
    /// What the last frame drew, kept because ratatui's rendered buffer is not
    /// reachable once `draw` has returned. See [`Screen::viewport_text`].
    viewport: String,
    restore: Option<Restore>,
    restored: bool,
}

impl Screen<CrosstermBackend<io::Stdout>> {
    /// Take the terminal: raw mode on, bracketed paste on, the Kitty keyboard
    /// protocol negotiated up where it is offered, and a panic hook that gives
    /// all of it back before anything is printed.
    ///
    /// Raw mode is the only thing taken. The alternate screen is not entered and
    /// the mouse is not captured, so the scrollback, the terminal's search and its
    /// selection are all still the terminal's own.
    pub fn attach() -> io::Result<Self> {
        Self::attach_with(VIEWPORT_HEIGHT)
    }

    /// Take the terminal with a viewport of `height` rows.
    ///
    /// The height is fixed for the life of the `Screen` — ratatui sets an inline
    /// viewport's height when the terminal is constructed and offers no way to
    /// change it. A caller that needs a different one drops this and attaches
    /// again, which is what the wizard and the session do at the boundary between
    /// them: nothing is streaming there, so re-placing the viewport cannot
    /// disturb anything in flight.
    pub fn attach_with(height: u16) -> io::Result<Self> {
        crossterm::terminal::enable_raw_mode()?;

        // From here on the terminal is raw, so EVERY failure path has to give it
        // back before returning. Found the hard way: placing the inline viewport
        // asks the terminal where its cursor is and reads the answer back off
        // stdin, and a terminal that does not answer left the process exiting
        // with the user's shell still in raw mode — no echo, no line editing, and
        // an error message that did not say what had happened.
        Self::attach_raw(height).inspect_err(|_| restore_terminal())
    }

    /// Give this viewport back and take one of `height` rows in its place.
    ///
    /// **The one operation in this product that re-queries the cursor while a
    /// session is running**, and the reason it is allowed at all is that its only
    /// caller does it at an empty prompt with nothing streaming. The scrollback
    /// above is the terminal's and survives; what is replaced is the viewport and
    /// the buffers behind it.
    ///
    /// A caller must park whatever is reading stdin first. Placing an inline
    /// viewport asks the terminal where its cursor is and reads the answer off
    /// stdin, so a reader still running would take the answer and this would hang
    /// on a terminal that had in fact replied.
    ///
    /// If the new height cannot be placed, the session's own is placed instead
    /// and the error returned: an operator who asked for a taller list and cannot
    /// have one keeps their session, rather than losing the viewport with it.
    pub fn replace(&mut self, height: u16) -> io::Result<()> {
        // **Erase what this viewport drew before letting go of it.** Its rows are
        // the terminal's screen, not its scrollback: nothing scrolls them away
        // and nothing repaints them once this `Screen` is gone. Without this the
        // next viewport is placed at the cursor and draws OVER the old rows,
        // which leaves half a palette standing behind a composer — a status line
        // spliced into the middle of a command's description, which is exactly
        // what a capture of the first version showed.
        //
        // `ESC[0J` from the viewport's own top row, so the committed transcript
        // above it is untouched and everything below is cleared. The cursor is
        // left there, which is also where `compute_inline_size` will place the
        // next viewport — so the new one starts exactly where the old one did.
        let top = self.terminal.get_frame().area().y;
        self.escape(&format!("\x1b[{};1H\x1b[0J", top.saturating_add(1)))?;
        self.restore();
        match Self::attach_with(height) {
            Ok(fresh) => {
                *self = fresh;
                Ok(())
            }
            Err(error) => {
                *self = Self::attach_with(VIEWPORT_HEIGHT)?;
                Err(error)
            }
        }
    }

    fn attach_raw(height: u16) -> io::Result<Self> {
        let mut out = io::stdout();
        crossterm::execute!(out, crossterm::event::EnableBracketedPaste)?;

        let mut terminal = Terminal::with_options(
            CrosstermBackend::new(out),
            TerminalOptions {
                // Clamped: a viewport taller than the terminal is not a viewport,
                // and 80x24 is a supported size rather than a degraded one.
                viewport: Viewport::Inline(
                    height.min(
                        crossterm::terminal::size()
                            .map(|(_, rows)| rows.saturating_sub(2))
                            .unwrap_or(height),
                    ),
                ),
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

        // Last, and deliberately after the hook that pops it: asking the terminal
        // whether it speaks the protocol means writing a query and waiting for the
        // reply, and a terminal that never answers costs two seconds of it. By
        // here the inline viewport has already been placed, which is a round trip
        // this same terminal has already answered — so the two seconds are only
        // ever spent on a terminal that is talking back. The other order pays them
        // on every pipe and every CI job, just before failing anyway.
        negotiate_keyboard(terminal.backend_mut(), keyboard_advertised())?;

        Ok(Self {
            terminal,
            probe: probe_terminal(),
            last: None,
            size: None,
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
            probe: probe_terminal(),
            last: None,
            size: None,
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

        // `insert_before` ends by clearing the viewport off the screen, so what
        // the terminal is showing is no longer what the last frame drew and the
        // next frame is a repaint of an erased region however little it moved.
        self.last = None;

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

    /// Push a raw terminal payload into the scrollback, in a region of its own.
    ///
    /// For a graphics protocol, which is a byte sequence a `Paragraph` cannot
    /// carry: [`Screen::commit`] measures display widths and wraps, and an escape
    /// has no width at all.
    ///
    /// The mechanism is the one the spike settled. `insert_before` renders a
    /// `Buffer` through the backend, and `CrosstermBackend::draw` prints
    /// `cell.symbol()` verbatim — so the payload goes in the first cell. **Every
    /// other cell is emptied**, because a cell's default symbol is a space and a
    /// row of spaces printed after the placement would erase the picture it was
    /// just given. An empty symbol prints nothing at all.
    ///
    /// The region is exactly as tall as the caller says the picture is, so the
    /// scrollback keeps the rows the image occupies.
    pub fn commit_raw(&mut self, payload: &str, rows: u16) -> io::Result<()> {
        if rows == 0 || payload.is_empty() {
            return Ok(());
        }
        // Same reason as `commit`: `insert_before` ends by clearing the viewport,
        // so the next frame is a repaint of an erased region.
        self.last = None;
        self.terminal.insert_before(rows, |buf| {
            for cell in &mut buf.content {
                cell.set_symbol("");
            }
            if let Some(first) = buf.content.first_mut() {
                first.set_symbol(payload);
            }
        })
    }

    /// Draw one viewport frame, wrapped in synchronized output — or draw nothing
    /// at all, if the frame says exactly what the terminal is already showing.
    ///
    /// The wrapping is what stops a streaming turn from strobing: the terminal is
    /// told to hold the display until the frame is complete, so a partially drawn
    /// composer is never presented. ratatui already diffs the buffer, which
    /// decides *what* is written; this decides *when* it becomes visible.
    ///
    /// The skip is the other half, and it is why the frame is laid out twice.
    /// ratatui's diff suppresses the *cells* of an unchanged frame but nothing
    /// else: the synchronized-output pair, the colour resets its backend emits
    /// after every diff however empty, and the cursor it re-places on every frame
    /// are all written regardless, so a screen that has not moved still pays
    /// forty-odd bytes per repaint of a session that repaints on every keystroke
    /// and every token. To get that to zero the frame has to be *known* before
    /// anything is written, and the only way to know it is to render it — so it
    /// is rendered into a probe: a terminal whose backend discards its output
    /// and remembers only where the frame asked the cursor to go. If the result
    /// matches what the terminal is already showing, this returns having written
    /// nothing; otherwise the already-rendered buffer is handed to the real
    /// terminal, which does the diff, the cursor and the flush exactly as before.
    ///
    /// The comparison is over the whole buffer rather than over
    /// [`Screen::viewport_text`]: text alone would skip a frame whose only change
    /// is a style, which is what a picker's highlight moving between two rows
    /// looks like. The cursor is in it too, because moving the caret through
    /// unchanged text is a real change with no cell behind it.
    pub fn draw<F: FnOnce(&mut Frame)>(&mut self, render: F) -> io::Result<()> {
        // ratatui autoresizes at the top of its own `draw` and so must this, or a
        // terminal that changed size between frames is laid out against the old
        // one. The size is read separately because `autoresize` does not say
        // whether it fired, and a resize *clears* the viewport: the frame after
        // one can never be skipped, whatever it contains.
        let size = self.terminal.size()?;
        if self.size.replace(size) != Some(size) {
            self.last = None;
        }
        self.terminal.autoresize()?;

        // The probe's viewport is pinned to the real one rather than computed, so
        // `frame.area()` — which every widget lays out against — is the same
        // rectangle in both. `Viewport::Fixed` is what makes that possible: it
        // takes the area it is given and never autoresizes away from it.
        let area = self.terminal.get_frame().area();
        if self.probe.get_frame().area() != area {
            self.probe.resize(area)?;
        }
        let drawn = self.probe.draw(render)?.buffer.clone();
        let cursor = self.probe.backend_mut().cursor;
        self.viewport = buffer_text(&drawn);

        if self
            .last
            .as_ref()
            .is_some_and(|(shown, at)| *shown == drawn && *at == cursor)
        {
            return Ok(());
        }
        self.last = Some((drawn.clone(), cursor));

        crossterm::queue!(self.terminal.backend_mut(), BeginSynchronizedUpdate)?;
        self.terminal.draw(|frame| {
            // A move, not a render: the frame was laid out on the probe and the
            // two buffers cover the same area, so this is the same content
            // arriving by a shorter route.
            *frame.buffer_mut() = drawn;
            if let Some(position) = cursor {
                frame.set_cursor_position(position);
            }
        })?;
        crossterm::queue!(self.terminal.backend_mut(), EndSynchronizedUpdate)?;
        // `Backend` and `Write` both name a `flush`; the write one is meant here,
        // because the queued escape sequences are sitting in the writer.
        Write::flush(self.terminal.backend_mut())
    }

    /// Write a raw escape sequence straight to the terminal.
    ///
    /// For OSC 52, which is a message to the *terminal emulator* rather than
    /// content for the screen: it puts a payload on the system clipboard and
    /// draws nothing. It cannot be a widget, and it must not go through
    /// [`Screen::commit`] — a clipboard sequence in the scrollback would be
    /// text the user scrolls past rather than an instruction the terminal acted
    /// on.
    ///
    /// Nothing here can confirm it worked. No terminal answers an OSC 52 write;
    /// several cap the payload silently and tmux ignores it entirely without
    /// `set -g set-clipboard on`. The `Ok` this returns means the bytes left the
    /// process, which is the only thing that can honestly be reported.
    pub fn escape(&mut self, sequence: &str) -> io::Result<()> {
        let backend = self.terminal.backend_mut();
        backend.write_all(sequence.as_bytes())?;
        Write::flush(backend)
    }

    /// Tell the renderer the terminal is a different size now.
    ///
    /// Recomputing the inline viewport is the whole job: the committed lines above
    /// it belong to the terminal and must not be redrawn, which is what produces
    /// the duplicated history a full-screen renderer shows on resize.
    pub fn resize(&mut self, width: u16, height: u16) -> io::Result<()> {
        // Recomputing it clears it, so the next frame repaints an erased region
        // rather than a screen that already says what it is about to say.
        self.last = None;
        self.terminal.resize(Rect::new(0, 0, width, height))
    }

    /// The width the renderer is currently laying out against, in cells.
    pub fn width(&mut self) -> u16 {
        self.terminal.current_buffer_mut().area.width
    }

    /// Rows the viewport currently occupies.
    ///
    /// What a caller compares against the height it wants, so that re-placing
    /// happens when the two differ and never otherwise. Read off the buffer
    /// rather than remembered, because `attach_with` clamps to the terminal and
    /// the height asked for is not always the height given.
    pub fn rows(&mut self) -> u16 {
        self.terminal.current_buffer_mut().area.height
    }

    /// Rows the whole terminal has, viewport and scrollback together.
    pub fn terminal_rows(&self) -> u16 {
        crossterm::terminal::size()
            .map(|(_, rows)| rows)
            .unwrap_or(24)
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

/// The terminal a frame is laid out on before anyone can see it.
///
/// Zero-sized on purpose: it is only ever resized to the real viewport's area,
/// and a fixed viewport takes the area it is given without asking the backend
/// anything, so this cannot fail.
fn probe_terminal() -> Terminal<Probe> {
    Terminal::with_options(
        Probe::default(),
        TerminalOptions {
            viewport: Viewport::Fixed(Rect::ZERO),
        },
    )
    .expect("a fixed viewport asks the backend nothing and writes nothing")
}

/// A backend that throws its output away and remembers one thing: where the
/// frame it just drew asked the cursor to be.
///
/// That one thing is the reason this exists rather than an [`io::Sink`].
/// `Frame::set_cursor_position` writes into a private field that ratatui 0.29
/// exposes no way to read back, and [`Screen::draw`] has to know it to hand the
/// frame on to the real terminal. It reaches a backend, though — `Terminal::draw`
/// ends by calling either `hide_cursor` or `show_cursor` and `set_cursor_position`
/// — so a backend is where it can be caught.
#[derive(Default)]
struct Probe {
    /// Where the last frame drawn on this terminal put the cursor, or `None` if
    /// it asked for the cursor to be hidden.
    cursor: Option<Position>,
}

impl Backend for Probe {
    fn draw<'a, I>(&mut self, _content: I) -> io::Result<()>
    where
        I: Iterator<Item = (u16, u16, &'a ratatui::buffer::Cell)>,
    {
        Ok(())
    }

    fn hide_cursor(&mut self) -> io::Result<()> {
        self.cursor = None;
        Ok(())
    }

    fn show_cursor(&mut self) -> io::Result<()> {
        // Deliberately not recorded. ratatui always follows it with the position,
        // which is the call that says the cursor is wanted and where.
        Ok(())
    }

    fn get_cursor_position(&mut self) -> io::Result<Position> {
        Ok(self.cursor.unwrap_or(Position::ORIGIN))
    }

    fn set_cursor_position<P: Into<Position>>(&mut self, position: P) -> io::Result<()> {
        self.cursor = Some(position.into());
        Ok(())
    }

    fn clear(&mut self) -> io::Result<()> {
        Ok(())
    }

    fn clear_region(&mut self, _clear_type: ClearType) -> io::Result<()> {
        // Overridden because the default refuses everything but a full clear, and
        // resizing a fixed viewport clears row by row.
        Ok(())
    }

    fn size(&self) -> io::Result<Size> {
        Ok(Size {
            width: 0,
            height: 0,
        })
    }

    fn window_size(&mut self) -> io::Result<WindowSize> {
        Ok(WindowSize {
            columns_rows: Size {
                width: 0,
                height: 0,
            },
            pixels: Size {
                width: 0,
                height: 0,
            },
        })
    }

    fn flush(&mut self) -> io::Result<()> {
        Ok(())
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

/// The one keyboard enhancement io-cli asks for.
///
/// `DISAMBIGUATE_ESCAPE_CODES` and nothing else. It is the flag that makes
/// `Shift+Enter` a distinguishable key at all: without it a terminal sends the
/// same `CR` for `Enter` and for `Shift+Enter`, so the composer's newline binding
/// is unreachable and the trailing backslash is the only spelling there is.
///
/// The other three are deliberately out:
///
/// - `REPORT_EVENT_TYPES` starts delivering `Release` and `Repeat` events. Every
///   input loop in this product already discards anything that is not a `Press`
///   (`main.rs`, twice, and `wizard.rs`), so asking for them buys nothing and
///   doubles the events a streaming turn has to drain.
/// - `REPORT_ALTERNATE_KEYS` replaces the base keycode with the shifted one,
///   which silently moves bindings out from under the keys they are written for.
/// - `REPORT_ALL_KEYS_AS_ESCAPE_CODES` turns plain text into CSI-u sequences.
///   The one thing this product must not risk is a terminal where typing stops
///   working, and it buys nothing `Shift+Enter` needs.
///
/// Asking for the least is also what makes the negotiation safe to *not* undo
/// perfectly: one bit, popped once.
pub const KEYBOARD_FLAGS: KeyboardEnhancementFlags =
    KeyboardEnhancementFlags::DISAMBIGUATE_ESCAPE_CODES;

/// Whether this process has a keyboard-protocol push outstanding.
///
/// Process-wide because the thing that has to pop it is [`restore_terminal`],
/// which runs from a panic hook with no [`Screen`] anywhere in reach. `false`
/// until a push actually goes out, which is what keeps a terminal that never
/// advertised the protocol from seeing a stray pop.
static KEYBOARD_PUSHED: AtomicBool = AtomicBool::new(false);

/// Which inline-graphics protocol this terminal speaks, if any.
///
/// Two, and they are not interchangeable: Kitty takes PNG and can be told not to
/// move the cursor, iTerm2 decodes the file itself and cannot. What they share is
/// how they are found — by what the environment says the terminal is, never by
/// asking it. A graphics query is answered with an APC string crossterm's parser
/// does not model, so a reply would have to be read by a second raw-mode owner,
/// which this product does not have.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Graphics {
    /// Cells, and no escape at all — which is also what an unknown terminal gets,
    /// because an escape it cannot read is unreadable bytes in permanent
    /// scrollback.
    None,
    /// The Kitty graphics protocol, whose `C=1` leaves the cursor alone.
    Kitty,
    /// iTerm2's inline-image escape, which moves the cursor and is therefore
    /// bracketed by a save and a restore where it is written.
    Iterm2,
}

/// The protocol `var`'s environment describes.
///
/// A multiplexer hides both: it sits between this process and whatever would draw
/// the picture, and it is the terminal's answer rather than one protocol's.
///
/// **Read from the environment rather than queried, and that is a safety
/// decision.** The keyboard protocol is asked for with crossterm's own
/// `supports_keyboard_enhancement`, which knows how to parse the reply it gets.
/// A graphics query is an APC string that crossterm's event parser does not
/// model, so asking would mean a second reader on the same channel at exactly the
/// moment the first one is being set up — and a terminal that answered
/// unexpectedly could leave bytes in the queue that later arrive as keystrokes.
///
/// The cost of reading the environment is that an unknown terminal which does
/// speak the protocol gets half-block cells. That is the **safe** direction: a
/// picture drawn from cells is a picture, while an escape sent to a terminal that
/// cannot read it is unreadable bytes written permanently into a scrollback that
/// no later redraw can clean.
///
/// A multiplexer is refused outright. Kitty graphics inside tmux need explicit
/// passthrough that is off by default, and screen has no equivalent at all — so
/// the terminal underneath speaking the protocol is exactly the case where the
/// escape does the most damage.
///
/// Taking the lookup as an argument is what makes every branch testable without a
/// test mutating the process it runs in.
pub fn graphics_protocol(var: impl Fn(&str) -> Option<String>) -> Graphics {
    let term = var("TERM").unwrap_or_default();
    // A multiplexer is between us and whatever would draw it.
    if var("TMUX").is_some()
        || var("STY").is_some()
        || term.starts_with("screen")
        || term.starts_with("tmux")
    {
        return Graphics::None;
    }
    let program = var("TERM_PROGRAM").unwrap_or_default();
    if term.contains("kitty")
        || var("KITTY_WINDOW_ID").is_some()
        || var("GHOSTTY_RESOURCES_DIR").is_some()
        || var("KONSOLE_VERSION").is_some()
        || program.eq_ignore_ascii_case("ghostty")
        || program.eq_ignore_ascii_case("wezterm")
    {
        return Graphics::Kitty;
    }
    // `LC_TERMINAL` is what survives an ssh session, where `TERM_PROGRAM` is the
    // remote shell's and says nothing about the terminal drawing it.
    if program == "iTerm.app"
        || var("LC_TERMINAL")
            .unwrap_or_default()
            .eq_ignore_ascii_case("iterm2")
    {
        return Graphics::Iterm2;
    }
    Graphics::None
}

/// Whether [`graphics_protocol`] answers Kitty.
pub fn speaks_kitty_graphics(var: impl Fn(&str) -> Option<String>) -> bool {
    graphics_protocol(var) == Graphics::Kitty
}

/// [`graphics_protocol`] against this process's own environment.
pub fn graphics() -> Graphics {
    graphics_protocol(|name| std::env::var(name).ok())
}

pub fn kitty_graphics() -> bool {
    speaks_kitty_graphics(|name| std::env::var(name).ok())
}

/// Whether the terminal advertises the Kitty keyboard protocol.
///
/// Separated from [`negotiate_keyboard`] the way [`crate::theme::Background`]
/// separates `detect` from `from_colorfgbg`: this half talks to a real terminal
/// and cannot run under `cargo test`, the half that decides what to write is
/// pure and is driven both ways by `tests/keyboard.rs`.
///
/// An error is a "no". crossterm reports one when the terminal answers neither
/// the enhancement query nor the device-attributes query within two seconds, and
/// a terminal that will not say whether it speaks the protocol is not one to push
/// a protocol at.
pub fn keyboard_advertised() -> bool {
    /// Asked once per process, because the answer cannot change and asking is
    /// not free.
    ///
    /// **crossterm writes a query and waits up to two seconds for the reply.**
    /// A terminal that speaks the protocol answers at once; one that does not —
    /// Apple's Terminal, most `script` sessions — answers never, and the wait is
    /// paid in full. That was a one-off cost while an attach happened once per
    /// process, and 0.11.0 made it not one: the palette re-places the viewport
    /// when it opens and again when it closes, so an uncached probe would put
    /// two seconds on each `/` and two more on each `Esc`, on exactly the
    /// terminals that already have the worst of everything else.
    static ADVERTISED: OnceLock<bool> = OnceLock::new();
    *ADVERTISED
        .get_or_init(|| crossterm::terminal::supports_keyboard_enhancement().unwrap_or(false))
}

/// Negotiate the protocol up, if `advertised`; otherwise write nothing at all.
///
/// The whole decision is this one argument, which is why it is an argument. What
/// is pushed here is popped by [`restore_terminal`] on every path out of the
/// process, and the two are asserted to balance over the byte stream.
pub fn negotiate_keyboard<W: Write>(out: &mut W, advertised: bool) -> io::Result<()> {
    if !advertised {
        return Ok(());
    }
    // Recorded before the write rather than after it. A push that fails halfway
    // has still put part of a mode change on the wire, and the recovery for that
    // is a pop; the reverse order would decide there was nothing to undo.
    KEYBOARD_PUSHED.store(true, Ordering::SeqCst);
    out.write_all(sequence(PushKeyboardEnhancementFlags(KEYBOARD_FLAGS)).as_bytes())?;
    // Explicit, because stdout is line buffered and this sequence has no newline
    // in it: unflushed, the protocol would come up whenever the first frame
    // happened to be drawn.
    out.flush()
}

/// Everything handing the terminal back *writes*, into any writer.
///
/// Split out of [`restore_terminal`] because [`restore_terminal`] writes to the
/// real stdout, which no test can read. This is the same sequence aimed at
/// somewhere a recorder can see it, so `tests/keyboard.rs` asserts the pop that
/// actually ships rather than a copy of it written for the test.
///
/// The pop goes first, and the order is deliberate: a failure here takes the rest
/// of the sequence with it, and of the three things this writes the pop is the one
/// that must not be skipped. Bracketed paste left on is an annoyance in the next
/// program; a keyboard protocol left pushed is a shell reporting every key
/// differently, with nothing on screen to say why.
pub fn restore_into<W: Write>(out: &mut W) -> io::Result<()> {
    // `swap` is the whole safety argument, and it covers three cases at once.
    // Nothing pushed: the flag is `false`, nothing is written, and a terminal that
    // does not speak the protocol never sees a sequence it would print as text.
    // Restored twice — `Screen::restore` and then `Drop`, which is the ordinary
    // exit — the second call reads the `false` the first one left. And a panic on
    // one thread racing an orderly exit on another is the same swap: exactly one
    // of them takes the `true`, so one pop is written for one push, whichever
    // thread gets there first.
    if KEYBOARD_PUSHED.swap(false, Ordering::SeqCst) {
        out.write_all(sequence(PopKeyboardEnhancementFlags).as_bytes())?;
    }
    crossterm::execute!(
        out,
        crossterm::event::DisableBracketedPaste,
        crossterm::cursor::Show
    )
}

/// A crossterm command as the bytes it puts on the wire.
///
/// The two keyboard commands are written this way instead of through `execute!`
/// on purpose. Both declare the ANSI form unsupported on Windows and answer the
/// legacy console API with `Unsupported` instead, so `execute!` turns a push into
/// an error there — including on a Windows terminal that does speak the protocol
/// and answered the query saying so. The sequence itself is not platform
/// specific: a terminal that understood the query understands these bytes.
///
/// Public so the tests can name the exact sequences they count, rather than
/// hard-coding two escape strings that would keep passing if crossterm changed
/// what it emits.
pub fn sequence(command: impl Command) -> String {
    let mut ansi = String::new();
    // The only error a `fmt::Write` into a `String` can report is one that a
    // `String` has no way to produce.
    let _ = command.write_ansi(&mut ansi);
    ansi
}

/// Put the real terminal back: keyboard protocol popped, bracketed paste off,
/// cursor shown, cooked mode.
///
/// Deliberately ignores its errors. It runs on the panic path, where returning a
/// `Result` nobody can act on would mean the terminal stays raw because restoring
/// it failed halfway.
pub fn restore_terminal() {
    let mut out = io::stdout();
    let _ = restore_into(&mut out);
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
