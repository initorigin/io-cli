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

    /// Report a different terminal size from now on.
    ///
    /// A real terminal answers the size query with its new dimensions the moment
    /// it is resized, and `Terminal::draw` re-reads it on every frame through
    /// `autoresize`. A harness whose size never moves therefore undoes any resize
    /// on the very next frame — which is a property of the harness, not of the
    /// renderer, and is why this exists.
    pub fn set_size(&mut self, width: u16, height: u16) {
        self.size = Size { width, height };
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

/// Resize the way a terminal does: the backend reports the new size *and* the
/// application is told about it, in that order.
pub fn resize(screen: &mut io_cli::term::Screen<Fixed>, width: u16, height: u16) {
    screen.terminal_mut().backend_mut().set_size(width, height);
    screen.resize(width, height).expect("resize");
}

/// Every event kind io-harness declares, snake-cased the way its serde tag is.
///
/// Read out of the dependency's own source rather than from a list copied into
/// this repository, because a copied list cannot notice that the harness grew a
/// fifty-first kind — which is exactly the drift F8 exists to catch. The version
/// comes from this crate's own lockfile, so the source read is the source built.
pub fn harness_event_kinds() -> Vec<String> {
    let source = std::fs::read_to_string(harness_observe_path())
        .expect("io-harness's source is readable from the registry")
        .replace("\r\n", "\n");
    let body = source
        .split_once("pub enum EventKind {")
        .expect("io-harness declares EventKind in src/observe.rs")
        .1;
    let body = body.split_once("\n}\n").expect("the enum is closed").0;

    let mut kinds = Vec::new();
    for line in body.lines() {
        // A variant sits at exactly four spaces and starts with a capital; a doc
        // line starts with `/`, an attribute with `#`, a field is indented eight.
        let Some(rest) = line.strip_prefix("    ") else {
            continue;
        };
        if !rest.starts_with(|c: char| c.is_ascii_uppercase()) {
            continue;
        }
        let variant: String = rest
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
        kinds.push(snake);
    }
    kinds
}

/// Where the io-harness version this crate is locked to unpacked its source.
fn harness_observe_path() -> std::path::PathBuf {
    let manifest = std::path::Path::new(env!("CARGO_MANIFEST_DIR"));
    let lock = std::fs::read_to_string(manifest.join("Cargo.lock")).expect("the lockfile is here");
    let version = lock
        .split("name = \"io-harness\"")
        .nth(1)
        .and_then(|rest| rest.split_once("version = \""))
        .and_then(|(_, rest)| rest.split_once('"'))
        .map(|(version, _)| version.to_string())
        .expect("io-harness is in the lockfile");

    let home = std::env::var_os("CARGO_HOME")
        .map(std::path::PathBuf::from)
        .or_else(|| {
            std::env::var_os("HOME").map(|home| std::path::PathBuf::from(home).join(".cargo"))
        })
        .expect("a cargo home");

    let registries = home.join("registry").join("src");
    let entries = std::fs::read_dir(&registries)
        .unwrap_or_else(|error| panic!("{} is readable: {error}", registries.display()));
    for entry in entries.flatten() {
        let candidate = entry
            .path()
            .join(format!("io-harness-{version}"))
            .join("src")
            .join("observe.rs");
        if candidate.is_file() {
            return candidate;
        }
    }
    panic!(
        "io-harness {version} is not unpacked under {}",
        registries.display()
    );
}

/// The escape sequences F5 forbids, with the names the contract uses.
pub const FORBIDDEN: &[(&str, &str)] = &[
    ("alternate screen (1049)", "\x1b[?1049h"),
    ("alternate screen (1047)", "\x1b[?1047h"),
    ("mouse capture (1000)", "\x1b[?1000h"),
    ("mouse capture (1002)", "\x1b[?1002h"),
    ("mouse capture (1003)", "\x1b[?1003h"),
];
