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
//!
//! [`Scripted`] is the third, and it answers a different question entirely: not
//! what leaves the terminal, but what a real turn writes into a real store. It
//! lives here rather than in one test file because more than one file now needs a
//! turn that actually happened — a rewind needs a run that took a restore point,
//! and a fork needs a conversation tree with more than one branch in it — and
//! neither can be forged. `Store`'s snapshot writer is crate-private and
//! `TranscriptTurn` is `#[non_exhaustive]`, so a hand-built fixture cannot exist
//! at all from outside io-harness; the only way to have a turn is to drive one.

#![allow(dead_code)]

use std::collections::VecDeque;
use std::io::{self, Write};
use std::sync::{Arc, Mutex};

use crossterm::event::{PopKeyboardEnhancementFlags, PushKeyboardEnhancementFlags};
use io_harness::provider::{CompletionRequest, CompletionResponse, ToolCall};
use io_harness::tools::WRITE_FILE_TOOL;
use io_harness::Provider;

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
    /// ratatui 0.30 gave a backend its own failure type. This one delegates to a
    /// `CrosstermBackend` and `Screen` requires `Backend<Error = io::Error>`, so
    /// naming anything else here would make the fixture stop being usable where
    /// the real terminal is.
    type Error = io::Error;

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

/// Re-place the viewport at `viewport` rows, exactly the way `Screen::replace`
/// does it against a real terminal.
///
/// This is the harness half of 0.32.0's growing viewport. `Screen::replace` and
/// `Screen::rewind` both live on the stdout-backed impl, because both build their
/// replacement with `Screen::attach_with`, which enables raw mode and asks a real
/// tty where its cursor is — so neither is reachable from here, and until 0.32.0
/// no test had ever executed a viewport re-placement. `Screen::replace_from`
/// takes the constructor as an argument for exactly this reason, and what it runs
/// is the same erase, restore, re-attach and fall-back the session runs.
///
/// **The recorder is deliberately the same one.** The property N5 asserts is
/// about the byte stream across a growth — content committed before the
/// re-placement must appear in it exactly once afterwards — and a fresh recorder
/// per screen would throw away the only evidence that matters.
pub fn replace(
    screen: &mut io_cli::term::Screen<Fixed>,
    recorder: &Recorder,
    width: u16,
    height: u16,
    viewport: u16,
) {
    let row = screen.terminal_mut().get_frame().area().y.saturating_add(1);
    let recorder = recorder.clone();
    screen
        .replace_from(row, viewport, move |rows| {
            let backend = Fixed::new(recorder.clone(), width, height);
            Terminal::with_options(
                backend,
                ratatui::TerminalOptions {
                    viewport: ratatui::Viewport::Inline(rows),
                },
            )
            .map(io_cli::term::Screen::from_terminal)
        })
        .expect("replace");
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

/// Every variant `io_harness::RunOutcome` declares, in the source this crate is
/// locked to, in declaration order and spelled as Rust spells them.
///
/// The gate that replaced a compile error. Before 0.65 `RunOutcome` was exhaustive,
/// so a variant added by a later harness broke `exec::code`'s match and the table
/// could not silently stop being total. 0.65 made the enum `#[non_exhaustive]`,
/// which means the wildcard arm is now mandatory and the build no longer says
/// anything — so the property moves here, where a new variant fails a test that
/// names it instead of disappearing into a catch-all.
pub fn harness_run_outcomes() -> Vec<String> {
    let source = std::fs::read_to_string(harness_source_path("run.rs"))
        .expect("io-harness's source is readable from the registry")
        .replace("\r\n", "\n");
    let body = source
        .split_once("pub enum RunOutcome {")
        .expect("io-harness declares RunOutcome in src/run.rs")
        .1;
    let body = body.split_once("\n}\n").expect("the enum is closed").0;

    let mut variants = Vec::new();
    for line in body.lines() {
        let Some(rest) = line.strip_prefix("    ") else {
            continue;
        };
        if !rest.starts_with(|c: char| c.is_ascii_uppercase()) {
            continue;
        }
        variants.push(
            rest.chars()
                .take_while(char::is_ascii_alphanumeric)
                .collect::<String>(),
        );
    }
    variants
}

/// Every tool name io-harness declares, in the source this crate is locked to.
///
/// The third reader of the dependency's own source, and it exists for the same
/// reason the first two do: a list of tool names copied into this repository
/// cannot notice the harness growing one. What it guards is the system prompt —
/// what the agent may reach is composed around that text by the harness, from the
/// contract, so a prompt that named a tool would be lying on every turn whose
/// contract omits it.
///
/// The names are `pub const …_TOOL: &str = "…"` in two files, which is a shape a
/// line-by-line read can take exactly: no parser is needed and none is written.
pub fn harness_tool_names() -> Vec<String> {
    let mut names = Vec::new();
    for file in [&["tools", "mod.rs"][..], &["run.rs"][..]] {
        let source = std::fs::read_to_string(harness_source_file(file))
            .expect("io-harness's source is readable from the registry")
            .replace("\r\n", "\n");
        for line in source.lines() {
            let Some(rest) = line.strip_prefix("pub const ") else {
                continue;
            };
            let Some((name, rest)) = rest.split_once(": &str = \"") else {
                continue;
            };
            if !name.contains("TOOL") {
                continue;
            }
            let Some((value, _)) = rest.split_once('"') else {
                continue;
            };
            // A prefix is not a tool: `MCP_TOOL_PREFIX` is `mcp__`, which no
            // prompt would contain and which would match nothing useful anyway.
            if value.is_empty() || value.ends_with("__") {
                continue;
            }
            names.push(value.to_string());
        }
    }
    names.sort();
    names.dedup();
    names
}

/// Where the io-harness version this crate is locked to unpacked its source.
fn harness_observe_path() -> std::path::PathBuf {
    harness_source_path("observe.rs")
}

/// One file of that source, by its name under `src/`.
fn harness_source_path(file: &str) -> std::path::PathBuf {
    harness_source_file(&[file])
}

/// One file of that source, by its path components under `src/`.
///
/// Components rather than a `"tools/mod.rs"` literal: a slash-bearing string
/// joined onto a `PathBuf` is a single file name on Windows, where CI runs, and
/// the failure it produces is "io-harness is not unpacked" rather than anything
/// naming the real mistake.
fn harness_source_file(components: &[&str]) -> std::path::PathBuf {
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
        let mut candidate = entry
            .path()
            .join(format!("io-harness-{version}"))
            .join("src");
        for component in components {
            candidate = candidate.join(component);
        }
        if candidate.is_file() {
            return candidate;
        }
    }
    panic!(
        "io-harness {version} is not unpacked under {}",
        registries.display()
    );
}

/// A provider that plays a script of tool-call batches and then stops talking.
///
/// One batch per completion, in order; once the script is exhausted every later
/// completion is plain text with no calls, which is how the agent loop is told the
/// turn is over. A provider that returned its calls forever would never end a
/// turn, and a test that ended one with a step ceiling would be asserting the
/// ceiling rather than the work.
pub struct Scripted {
    batches: Mutex<VecDeque<Vec<ToolCall>>>,
}

impl Scripted {
    /// One batch that writes each `(path, content)` in the order given.
    ///
    /// An empty slice is a legitimate script and the one a conversation-shaped
    /// test wants: the single batch holds no calls, so the very first completion
    /// comes back as plain text and the turn is one exchange that touches no file.
    /// That is a turn in the tree exactly like any other — which is the whole point
    /// for a test about branching, where what is on disk is beside the question.
    pub fn writing(files: &[(&str, &str)]) -> Self {
        let batch = files
            .iter()
            .map(|(path, content)| write_call(path, content))
            .collect();
        Self {
            batches: Mutex::new(VecDeque::from(vec![batch])),
        }
    }

    /// One batch per **step**, so a turn takes as many steps as there are slices.
    ///
    /// [`Scripted::writing`] puts every file in one batch, which is one step. A
    /// test about undoing *one step* of a run needs the steps to be distinct, and
    /// the only way to get that is to make the provider answer more than once —
    /// io-harness's step boundary is a completion, not a tool call.
    ///
    /// Steps are numbered from one by the run loop, so `in_steps(&[a, b])` writes
    /// `a` at step 1 and `b` at step 2.
    pub fn in_steps(steps: &[&[(&str, &str)]]) -> Self {
        let batches = steps
            .iter()
            .map(|files| {
                files
                    .iter()
                    .map(|(path, content)| write_call(path, content))
                    .collect()
            })
            .collect::<Vec<Vec<ToolCall>>>();
        Self {
            batches: Mutex::new(VecDeque::from(batches)),
        }
    }
}

impl Provider for Scripted {
    async fn complete(
        &self,
        _request: CompletionRequest,
    ) -> io_harness::Result<CompletionResponse> {
        let calls: Vec<ToolCall> = self
            .batches
            .lock()
            .expect("the script is not poisoned")
            .pop_front()
            .unwrap_or_default();
        Ok(CompletionResponse {
            // Text only once there is nothing left to do, so the loop has exactly
            // one reason to stop and it is the ordinary one.
            text: calls.is_empty().then(|| "done".to_string()),
            tool_calls: calls,
            ..Default::default()
        })
    }
}

/// A provider that answers in one completion and keeps the system prompt it was
/// asked with.
///
/// **The only way to read a composed system prompt from outside io-harness.**
/// `run::prompts::compose` is `pub(super)`, and `EventKind::PromptComposed`
/// carries the prompt's *size* rather than its text — deliberately, since it can
/// hold a whole `AGENTS.md`. What does carry the text is the request that reaches
/// the provider, so a test about composition has to be a turn that really ran.
pub struct Capturing {
    systems: Mutex<Vec<String>>,
}

impl Capturing {
    pub fn new() -> Self {
        Self {
            systems: Mutex::new(Vec::new()),
        }
    }

    /// Every system prompt this provider was asked with, in order.
    pub fn systems(&self) -> Vec<String> {
        self.systems
            .lock()
            .expect("the capture is not poisoned")
            .clone()
    }
}

impl Provider for Capturing {
    async fn complete(&self, request: CompletionRequest) -> io_harness::Result<CompletionResponse> {
        let mut systems = self.systems.lock().expect("the capture is not poisoned");
        systems.push(request.system.clone());
        // **The first completion and every later one are composed from different
        // descriptions**, and a capture of one of them is a capture of half the
        // question: a turn that has not been decided to be work opens on the
        // crate's conversational description, and a turn that has is given its
        // workspace one. So this writes a file on the first completion — which is
        // what decides the turn is work — and answers in prose on the second.
        let first = systems.len() == 1;
        Ok(CompletionResponse {
            text: (!first).then(|| "done".to_string()),
            tool_calls: match first {
                true => vec![write_call("notes.txt", "written by the capture\n")],
                false => Vec::new(),
            },
            ..Default::default()
        })
    }
}

/// One `write_file` call, with its arguments built as JSON text and parsed.
///
/// `ToolCall::arguments` is a `serde_json::Value`, and `serde_json` is not a
/// dependency of this crate — io-harness carries it and does not re-export it, so
/// the type cannot be named here at all. It can still be *produced*: `Value`
/// implements `FromStr`, so `str::parse` builds one with the target type inferred
/// from the field it is assigned to. That is the whole reason this helper writes
/// its arguments as text rather than with a builder.
pub fn write_call(path: &str, content: &str) -> ToolCall {
    ToolCall {
        name: WRITE_FILE_TOOL.to_string(),
        arguments: format!(
            "{{\"path\":{},\"content\":{}}}",
            quoted(path),
            quoted(content)
        )
        .parse()
        .expect("the arguments were assembled as JSON and must parse as JSON"),
    }
}

/// `text` as a JSON string literal.
///
/// Hand-written because the crate has no JSON encoder to reach for. It escapes
/// only what the fixtures actually contain — quotes, backslashes and newlines —
/// and would produce invalid JSON for a control character, which is why
/// `write_call` asserts that the result parses rather than trusting it.
fn quoted(text: &str) -> String {
    let mut out = String::with_capacity(text.len() + 2);
    out.push('"');
    for character in text.chars() {
        match character {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            other => out.push(other),
        }
    }
    out.push('"');
    out
}

/// How many keyboard-protocol pushes and pops the recorded byte stream carries.
///
/// Deliberately not a [`FORBIDDEN`] entry, and the difference is the point.
/// `FORBIDDEN` is a list of sequences that must never appear at all; a keyboard
/// push is allowed to appear — it is what F7 asks for on a terminal that
/// advertises the protocol — and what has to hold is that it *balances*. A push
/// with no pop leaves the terminal in a mode io-cli chose, in a shell io-cli no
/// longer owns.
///
/// The sequences come from crossterm through [`io_cli::term::sequence`] rather
/// than from two escape strings typed in here, so a crossterm that changed what
/// it emits fails this rather than passing it by counting zero of both.
pub fn keyboard_balance(recorder: &Recorder) -> (usize, usize) {
    let text = recorder.text();
    let pushed = io_cli::term::sequence(PushKeyboardEnhancementFlags(io_cli::term::KEYBOARD_FLAGS));
    let popped = io_cli::term::sequence(PopKeyboardEnhancementFlags);
    (text.matches(&pushed).count(), text.matches(&popped).count())
}

/// The escape sequences F5 forbids, with the names the contract uses.
pub const FORBIDDEN: &[(&str, &str)] = &[
    ("alternate screen (1049)", "\x1b[?1049h"),
    ("alternate screen (1047)", "\x1b[?1047h"),
    ("mouse capture (1000)", "\x1b[?1000h"),
    ("mouse capture (1002)", "\x1b[?1002h"),
    ("mouse capture (1003)", "\x1b[?1003h"),
];

/// A real PNG of a stated size, encoded by the same crate that decodes it.
///
/// Hand-rolling a PNG here would be a second, disagreeing answer to what a PNG
/// is; asking the decoder's own encoder for one keeps the fixture honest and
/// keeps the test about `picture`, not about byte layout.
#[allow(dead_code)]
pub fn png_bytes(width: u32, height: u32) -> Vec<u8> {
    use image::ImageEncoder;

    let pixels = image::RgbaImage::from_pixel(width, height, image::Rgba([32, 64, 128, 255]));
    let mut out = Vec::new();
    image::codecs::png::PngEncoder::new(&mut out)
        .write_image(
            pixels.as_raw(),
            width,
            height,
            image::ExtendedColorType::Rgba8,
        )
        .expect("the png encoder this crate already declares");
    out
}

/// The same picture as a jpeg, which is the format that separates the two
/// graphics protocols: Kitty's `f=100` is PNG and iTerm2 decodes the file itself.
pub fn jpeg_bytes(width: u32, height: u32) -> Vec<u8> {
    let pixels = image::RgbaImage::from_pixel(width, height, image::Rgba([32, 64, 128, 255]));
    let mut out = std::io::Cursor::new(Vec::new());
    image::DynamicImage::ImageRgba8(pixels)
        .to_rgb8()
        .write_to(&mut out, image::ImageFormat::Jpeg)
        .expect("the jpeg encoder this crate already declares");
    out.into_inner()
}

/// A provider that answers immediately and remembers how many images each
/// request carried.
///
/// F7 is a claim about what reached the wire, so it has to be asserted on the
/// request rather than on a field io-cli could have cleared while keeping a copy
/// somewhere else. `Scripted` discards its request; this one reads it.
pub struct Watching {
    media: Mutex<Vec<usize>>,
}

impl Watching {
    #[allow(dead_code)]
    pub fn new() -> Self {
        Self {
            media: Mutex::new(Vec::new()),
        }
    }

    /// The media count of every completion since the last call, and reset.
    ///
    /// Taken rather than read so a test can speak about one turn at a time
    /// without counting how many completions that turn happened to need.
    #[allow(dead_code)]
    pub fn take_media_counts(&self) -> Vec<usize> {
        std::mem::take(&mut *self.media.lock().expect("not poisoned"))
    }
}

impl Provider for Watching {
    async fn complete(&self, request: CompletionRequest) -> io_harness::Result<CompletionResponse> {
        self.media
            .lock()
            .expect("not poisoned")
            .push(request.media.len());
        Ok(CompletionResponse {
            text: Some("done".to_string()),
            tool_calls: Vec::new(),
            ..Default::default()
        })
    }

    /// True, or every turn in these tests would be refused by
    /// `ensure_media_accepted` before it began.
    fn accepts_images(&self) -> bool {
        true
    }
}

/// The one environment lock for a test binary.
///
/// Every fixture below sets `IO_CONFIG`, which is process-global, so two tests
/// building a configuration at once would each see the other's file. Fourteen
/// test files used to declare a `Mutex` of their own for this; two different
/// mutexes in one binary exclude nothing from each other, so they delegate here
/// and a fixture taking this lock now excludes every environment-touching test
/// beside it rather than only the ones that happened to pick the same mutex.
///
/// A poisoned lock is taken anyway. The environment is restored by
/// [`UserScope`]'s own drop rather than by unwinding, so a panicking test leaves
/// nothing for the next one to trip over, and refusing to hand out the guard
/// would turn one real failure into every subsequent test failing for a reason
/// that is not its own.
pub fn env_lock() -> std::sync::MutexGuard<'static, ()> {
    static LOCK: Mutex<()> = Mutex::new(());
    LOCK.lock().unwrap_or_else(|poisoned| poisoned.into_inner())
}

/// A configuration fixture in the one scope io-harness still trusts.
///
/// **io-harness 0.74.0 is why this exists.** A workspace-resident configuration
/// file may no longer declare `[[provider]]`, `[[mcp]]`, `[[lsp]]` or — in
/// `io.local.toml` as well as `io.toml` — `[[hook]]`, may not widen the policy in
/// any of ten ways, and may not name an absolute `run.skills` or `run.templates`.
/// The reason is stated in `read_scope`: those files arrive with a `git clone`,
/// or sit in a root the run's own agent can write to, so a single `write_file` of
/// an unremarkable name would otherwise declare an endpoint or a command that the
/// next `Config::discover` acts on, outside the `Policy` and outside the sandbox.
///
/// `Scope::User` is the sole exemption and the exemption is the whole reason the
/// scope exists — `$IO_CONFIG` is outside every workspace, so a run that can
/// write its own root cannot reach it.
///
/// **The trap this type exists to close.** The obvious fixture writes `io.toml`
/// into the directory it then hands to `Config::discover`, and points `IO_CONFIG`
/// at that same file. That file is a candidate *twice* — once as `Scope::User`
/// through `IO_CONFIG` and once as `Scope::Project` because it is `root/io.toml`
/// — and the project read refuses it. Pointing `IO_CONFIG` somewhere is not
/// enough; the file has to be somewhere the discovery root does not reach. So
/// this keeps two directories, and the workspace it hands back is empty.
///
/// **`Config::from_toml` cannot be used for any of this.** It hard-codes
/// `Scope::Project` and calls `refuse_widening` unconditionally, so there is no
/// argument that makes it produce a user-scoped configuration. A fixture that
/// needs one of these sections has to go through discovery, which is why the
/// migration converted the `from_toml` call sites rather than adjusting them.
pub struct UserScope {
    /// Holds the user-scope `io.toml`. Never the discovery root.
    home: tempfile::TempDir,
    /// The workspace `Config::discover` was pointed at. Deliberately empty.
    workspace: tempfile::TempDir,
    /// Whether `IO_CONFIG` is still set, so `Drop` knows what to undo.
    kept: bool,
    pub config: io_harness::Config,
}

impl UserScope {
    /// The workspace root the configuration was discovered against.
    pub fn root(&self) -> &std::path::Path {
        self.workspace.path()
    }

    /// The user-scope configuration file itself.
    pub fn path(&self) -> std::path::PathBuf {
        self.home.path().join("io.toml")
    }

    /// Re-read the file from disk, for a test that has just written to it.
    ///
    /// The write-then-rediscover shape is what `configure::write` does, and a
    /// test asserting over it has to see the same thing the product would.
    pub fn reload(&mut self) -> io_harness::Result<()> {
        std::env::set_var("IO_CONFIG", self.path());
        let discovered = io_harness::Config::discover(self.workspace.path());
        if !self.kept {
            std::env::remove_var("IO_CONFIG");
        }
        self.config = discovered?;
        Ok(())
    }

    /// Discover again and hand back whatever came out, refusal included.
    ///
    /// For the tests that assert a refusal's own sentence: they need the `Err`,
    /// not a fixture that panics on it.
    pub fn rediscover(&self) -> io_harness::Result<io_harness::Config> {
        std::env::set_var("IO_CONFIG", self.path());
        let discovered = io_harness::Config::discover(self.workspace.path());
        if !self.kept {
            std::env::remove_var("IO_CONFIG");
        }
        discovered
    }
}

impl Drop for UserScope {
    fn drop(&mut self) {
        if self.kept {
            std::env::remove_var("IO_CONFIG");
        }
    }
}

/// A user-scoped configuration built from `toml`, with `IO_CONFIG` unset again.
///
/// Takes [`env_lock`] and releases it before returning, so a caller must not be
/// holding it — `std::sync::Mutex` is not reentrant and doing both deadlocks.
/// Where a test needs the lock held across more than the fixture, take it and
/// call [`user_scope_locked`].
pub fn user_scope(toml: &str) -> UserScope {
    let _guard = env_lock();
    user_scope_locked(toml, false)
}

/// As [`user_scope`], but `IO_CONFIG` stays set for the fixture's lifetime.
///
/// For a test that goes on to exercise a product path which resolves the
/// configuration for itself — `configure::write` re-discovers after writing, and
/// would otherwise find the operator's real file.
pub fn user_scope_kept(toml: &str) -> UserScope {
    let _guard = env_lock();
    user_scope_locked(toml, true)
}

/// The body of both, for a caller that already holds [`env_lock`].
pub fn user_scope_locked(toml: &str, keep: bool) -> UserScope {
    match try_user_scope_locked(toml, keep) {
        Ok(scope) => scope,
        Err(error) => panic!("the user-scope fixture parses: {error}"),
    }
}

/// As [`user_scope_locked`], returning the refusal instead of panicking on it.
///
/// The tests that assert io-harness's own refusal sentence need this: they are
/// asserting over an `Err`, and a fixture that unwraps has nothing to give them.
pub fn try_user_scope_locked(toml: &str, keep: bool) -> io_harness::Result<UserScope> {
    let home = tempfile::tempdir().expect("a directory for the user-scope file");
    let workspace = tempfile::tempdir().expect("a workspace root");
    let path = home.path().join("io.toml");
    std::fs::write(&path, toml).expect("the fixture is written");

    std::env::set_var("IO_CONFIG", &path);
    let discovered = io_harness::Config::discover(workspace.path());
    if !keep {
        std::env::remove_var("IO_CONFIG");
    }

    match discovered {
        Ok(config) => Ok(UserScope {
            home,
            workspace,
            kept: keep,
            config,
        }),
        Err(error) => {
            if keep {
                std::env::remove_var("IO_CONFIG");
            }
            Err(error)
        }
    }
}

/// A user-scope file beside a project-scope one, for the refusal tests.
///
/// The user file is trusted and the project file is not, which is the whole
/// shape T13 asserts over: a repository that arrives with a `git clone` declaring
/// something only the operator's own file may declare.
pub fn user_scope_with_project(user: &str, project: &str) -> io_harness::Result<io_harness::Config> {
    let _guard = env_lock();
    let home = tempfile::tempdir().expect("a directory for the user-scope file");
    let workspace = tempfile::tempdir().expect("a workspace root");
    let path = home.path().join("io.toml");
    std::fs::write(&path, user).expect("the user fixture is written");
    std::fs::write(workspace.path().join("io.toml"), project).expect("the project fixture is written");

    std::env::set_var("IO_CONFIG", &path);
    let discovered = io_harness::Config::discover(workspace.path());
    std::env::remove_var("IO_CONFIG");
    discovered
}
