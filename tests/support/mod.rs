//! The two test harnesses this release is built on.
//!
//! [`Recorder`] captures every byte written to the terminal, which is what F5 and
//! N3 assert over — both are properties of the escape sequences that leave the
//! process, and neither can be seen in a rendered buffer.
//!
//! [`Fixed`] is the reason the recorder works at all. `Terminal::with_options`
//! asks the backend for the terminal size and the cursor position before it can
//! place an inline viewport, and `CrosstermBackend` answers both by talking to a
//! real tty — a size query on stdout and, for the cursor, a DSR request whose
//! reply it reads back from stdin. Neither exists under `cargo test`. `Fixed`
//! answers those two questions from values the test chose and delegates
//! everything else to a real `CrosstermBackend`, so what lands in the recorder is
//! the byte stream crossterm actually produces rather than a simulation of it.
//!
//! This is deliberately not `TestBackend`. `TestBackend` renders into a cell grid
//! and emits no escape sequences at all, so F5 — "these sequences never appear" —
//! would pass against it for the wrong reason: nothing appears.

#![allow(dead_code)]

use std::io::{self, Write};
use std::sync::{Arc, Mutex};

use ratatui::backend::{Backend, ClearType, CrosstermBackend, WindowSize};
use ratatui::layout::{Position, Size};
use ratatui::Terminal;

/// A writer that keeps everything written to it.
#[derive(Clone, Default)]
pub struct Recorder {
    bytes: Arc<Mutex<Vec<u8>>>,
}

impl Recorder {
    pub fn new() -> Self {
        Self::default()
    }

    /// Everything written so far.
    pub fn bytes(&self) -> Vec<u8> {
        self.bytes.lock().expect("recorder poisoned").clone()
    }

    /// Everything written so far, lossily as text. Good enough for asserting that
    /// a message reached the terminal; use [`Recorder::bytes`] for sequences.
    pub fn text(&self) -> String {
        String::from_utf8_lossy(&self.bytes()).into_owned()
    }

    /// Whether the byte stream contains `needle` anywhere.
    pub fn contains(&self, needle: &str) -> bool {
        self.text().contains(needle)
    }
}

impl Write for Recorder {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        self.bytes
            .lock()
            .expect("recorder poisoned")
            .extend_from_slice(buf);
        Ok(buf.len())
    }

    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}

/// A `CrosstermBackend` whose size and cursor position come from the test rather
/// than from a tty.
pub struct Fixed {
    inner: CrosstermBackend<Recorder>,
    size: Size,
    cursor: Position,
}

impl Fixed {
    /// A backend of `width` by `height` with the cursor on the last row, which is
    /// where a shell leaves it when the binary starts.
    pub fn new(recorder: Recorder, width: u16, height: u16) -> Self {
        Self {
            inner: CrosstermBackend::new(recorder),
            size: Size { width, height },
            cursor: Position {
                x: 0,
                y: height.saturating_sub(1),
            },
        }
    }
}

impl Write for Fixed {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        self.inner.write(buf)
    }

    fn flush(&mut self) -> io::Result<()> {
        // `CrosstermBackend` implements both `Write` and `Backend`, and both name a
        // `flush`. Disambiguated rather than left to inference, which cannot
        // resolve it.
        Write::flush(&mut self.inner)
    }
}

impl Backend for Fixed {
    fn draw<'a, I>(&mut self, content: I) -> io::Result<()>
    where
        I: Iterator<Item = (u16, u16, &'a ratatui::buffer::Cell)>,
    {
        self.inner.draw(content)
    }

    fn append_lines(&mut self, n: u16) -> io::Result<()> {
        self.inner.append_lines(n)
    }

    fn hide_cursor(&mut self) -> io::Result<()> {
        self.inner.hide_cursor()
    }

    fn show_cursor(&mut self) -> io::Result<()> {
        self.inner.show_cursor()
    }

    fn get_cursor_position(&mut self) -> io::Result<Position> {
        Ok(self.cursor)
    }

    fn set_cursor_position<P: Into<Position>>(&mut self, position: P) -> io::Result<()> {
        // Recorded as well as forwarded: ratatui asks for the position back when
        // it recomputes an inline viewport after a resize, and a backend that
        // forgets where it was put reports the shell's original cursor forever.
        self.cursor = position.into();
        self.inner.set_cursor_position(self.cursor)
    }

    fn clear(&mut self) -> io::Result<()> {
        self.inner.clear()
    }

    fn clear_region(&mut self, clear_type: ClearType) -> io::Result<()> {
        self.inner.clear_region(clear_type)
    }

    fn size(&self) -> io::Result<Size> {
        Ok(self.size)
    }

    fn window_size(&mut self) -> io::Result<WindowSize> {
        Ok(WindowSize {
            columns_rows: self.size,
            pixels: Size {
                width: 0,
                height: 0,
            },
        })
    }

    fn flush(&mut self) -> io::Result<()> {
        Backend::flush(&mut self.inner)
    }
}

/// A screen of `width` by `height` writing into a fresh recorder, plus the
/// recorder itself.
pub fn screen(width: u16, height: u16) -> (io_cli::term::Screen<Fixed>, Recorder) {
    screen_of(width, height, io_cli::term::VIEWPORT_HEIGHT)
}

/// The same, with the viewport height chosen by the caller.
pub fn screen_of(
    width: u16,
    height: u16,
    viewport: u16,
) -> (io_cli::term::Screen<Fixed>, Recorder) {
    let recorder = Recorder::new();
    let backend = Fixed::new(recorder.clone(), width, height);
    let terminal = Terminal::with_options(
        backend,
        ratatui::TerminalOptions {
            viewport: ratatui::Viewport::Inline(viewport),
        },
    )
    .expect("inline terminal");
    (io_cli::term::Screen::from_terminal(terminal), recorder)
}

/// The escape sequences F5 forbids, with the names the contract uses.
pub const FORBIDDEN: &[(&str, &str)] = &[
    ("alternate screen (1049)", "\x1b[?1049h"),
    ("alternate screen (1047)", "\x1b[?1047h"),
    ("mouse capture (1000)", "\x1b[?1000h"),
    ("mouse capture (1002)", "\x1b[?1002h"),
    ("mouse capture (1003)", "\x1b[?1003h"),
];
