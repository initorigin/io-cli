//! `!` — one line, run in the operator's own shell.
//!
//! **This is the only module in `src/` that spawns a process, and
//! `tests/dependencies.rs` names it by path to permit it.** Everywhere else the
//! literal stays forbidden, because everywhere else a spawn would be a tool
//! implementation: the agent's commands go through io-harness, are governed by a
//! policy and are recorded in the run's trace, and one written here would be
//! governed by nothing and recorded nowhere.
//!
//! A `!` line is the other thing. The operator typed it, the operator governs it,
//! and its output goes into the scrollback and **not** into the trace — because
//! the agent did not do it, and a trace that recorded it would be a trace that
//! lies about who acted. Nothing here can reach the store, the conversation or
//! the event stream — it names none of io-harness's types, and
//! `tests/dependencies.rs` asserts that rather than trusting it, which is also
//! what makes the spawn unreachable from anything the harness drives.
//!
//! **The terminal is never handed over, and that is a hard constraint rather than
//! a preference.** ratatui's inline viewport sits at an absolute screen row
//! computed once, from a cursor query, and maintained afterwards by arithmetic
//! alone. Output that io-cli did not write scrolls the screen and ratatui learns
//! nothing about it, so the next frame paints over the child's text and the next
//! [`crate::term::Screen::commit`] pushes rows into the scrollback at the wrong
//! place — permanently, because on this renderer the transcript *is* the
//! terminal's scrollback and cannot be redrawn. Making a handover safe would
//! additionally need process-group and terminal-ownership transfer, or the
//! operator's `Ctrl+C` is a real signal to a shared foreground process group and
//! takes io-cli down with the child, unwinding nothing. That needs `nix`, which
//! `tests/dependencies.rs` forbids by name.
//!
//! So the output is captured, the child gets no tty, and the viewport is never
//! handed over, never restored and never rebuilt.
//!
//! **Interactive programs are therefore out of scope by construction.** `vim`,
//! `less`, a pager, a password prompt: the child's stdin is `/dev/null`, so a
//! program that waits for input reaches end-of-file immediately rather than
//! hanging behind a keyboard it can never reach, and a program that wants a tty
//! will say so or draw nothing. That is a shape, not an oversight, and it is
//! written down here so it is read rather than discovered.

// One name per line, and `std::process::Command` written out in full. A braced
// or aliased import would put a spawn in a file where the literal never appears,
// which is the evasion `tests/dependencies.rs` forbids everywhere — this module
// included, because a permission that can be spelled around is not a permission.
use std::process::Command;
use std::process::Stdio;

use ratatui::text::{Line, Span};

use crate::theme::{Theme, Tone};

/// The environment variable that names the operator's shell on this platform.
///
/// `$SHELL` on unix, `%COMSPEC%` on Windows. Public because it is the one thing
/// an operator can change about where a `!` line goes.
pub const SHELL_VAR: &str = if cfg!(windows) { "COMSPEC" } else { "SHELL" };

/// What runs when [`SHELL_VAR`] says nothing. POSIX requires `/bin/sh`; Windows
/// ships `cmd.exe`.
const FALLBACK: &str = if cfg!(windows) { "cmd.exe" } else { "/bin/sh" };

/// The flag that means *run this one line and exit*.
const RUN_FLAG: &str = if cfg!(windows) { "/C" } else { "-c" };

/// The shell a `!` line is handed to, and the flag that runs it.
///
/// **A function of the value rather than a reader of the environment**, which is
/// the same shape [`crate::app::App::tick`] uses for the clock and for the same
/// reason: a test can state an unset or empty `$SHELL` without a test process
/// mutating its own environment out from under every other test in the binary.
///
/// The operator's own shell rather than a fixed `/bin/sh`, because `!` should
/// mean what the same line means in the terminal they started io-cli from. What
/// it does *not* give them is their aliases and functions: a non-interactive
/// `-c` reads no startup file, in any shell, and nothing here can change that.
///
/// An empty or blank value is treated as absent. `%COMSPEC%` is routinely set to
/// the empty string, and `Command::new("")` fails with an error that names
/// nothing.
pub fn shell(named: Option<&str>) -> (&str, &'static str) {
    let program = named
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .unwrap_or(FALLBACK);
    (program, RUN_FLAG)
}

/// What running a `!` line produced.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Ran {
    /// The shell ran the line. Whether the line itself worked is [`Ran::Output`]'s
    /// `status`, which is the shell's answer rather than io-cli's.
    Output {
        stdout: String,
        stderr: String,
        /// `None` when the platform reported no code at all, which on unix means
        /// a signal ended it.
        status: Option<i32>,
    },
    /// The shell could not be started — a `$SHELL` that names nothing, or a
    /// binary that is not executable. Distinct from a command that does not
    /// exist, which is a shell that started fine and said so.
    Unstartable(String),
}

/// Run `line` through the operator's shell and bring back what it printed.
///
/// **This blocks, and that is a decision with a stated ceiling.** `!sleep 60`
/// freezes the interface for sixty seconds: nothing repaints, and although the
/// keyboard thread goes on queueing keystrokes into its channel — so nothing the
/// operator types is lost — none of them is *read* until the child exits, so
/// there is no way to abort it short of another terminal. The terminal is still
/// in raw mode throughout, so `Ctrl+C` is a byte on stdin rather than a signal
/// and kills nothing.
///
/// That is acceptable because of where this is reachable from and nowhere else:
/// the driver runs a `!` line only at an idle prompt and refuses one while a turn
/// is in flight, so a block here cannot stall the turn's `select!` loop, cannot
/// stop an event being drained, and cannot leave a run unable to reach its
/// interrupt. What it costs is a long `!` line, and the operator chose the line.
///
/// The upgrade, if that stops being acceptable, is `tokio::process` — the
/// `process` feature on the tokio already in `Cargo.toml`, which adds no new name
/// to the dependency set — and a `select!` arm around the child. It is a real
/// change to the tree and it is not this release's.
pub fn run(line: &str) -> Ran {
    let named = std::env::var(SHELL_VAR).ok();
    let (program, flag) = shell(named.as_deref());
    // All three streams are spelled out even though `output()` already wires them
    // this way. The wiring is the property F4 rests on — no tty for the child, so
    // the viewport is never at risk — and a reader checking that should not have
    // to know a default, nor should changing it look like a one-word tidy-up.
    let output = Command::new(program)
        .arg(flag)
        .arg(line)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .output();
    match output {
        Ok(output) => Ran::Output {
            stdout: String::from_utf8_lossy(&output.stdout).into_owned(),
            stderr: String::from_utf8_lossy(&output.stderr).into_owned(),
            status: output.status.code(),
        },
        Err(error) => Ran::Unstartable(format!("{program}: {error}")),
    }
}

/// What goes into the scrollback: the line that was run, what it printed, and how
/// it ended.
///
/// The shape is the driver's own `expand`: a header, the body indented under it,
/// a blank line to separate it from whatever comes next. One vocabulary, so a
/// `!` line reads like everything else the terminal already holds.
///
/// The leading `!` is the character the operator pressed rather than a themed
/// glyph, so it is written here and not looked up in [`crate::glyphs`] — there is
/// nothing for an ASCII set to substitute it with.
pub fn lines(line: &str, ran: &Ran, theme: &Theme) -> Vec<Line<'static>> {
    let mut out = vec![Line::from(Span::styled(
        format!("! {line}"),
        theme.style(Tone::Accent),
    ))];

    match ran {
        Ran::Unstartable(why) => {
            out.push(theme.notice(Tone::Error, format!("no shell to run that in: {why}")));
        }
        Ran::Output {
            stdout,
            stderr,
            status,
        } => {
            out.extend(body(stdout, theme.style(Tone::Normal)));
            // After stdout rather than interleaved with it, because the two were
            // captured separately and there is no honest way to put them back in
            // the order the child wrote them.
            out.extend(body(stderr, theme.style(Tone::Error)));

            if stdout.trim().is_empty() && stderr.trim().is_empty() {
                // Said rather than left blank. A `!` line that commits a header
                // and nothing under it looks exactly like one that did not run.
                out.push(theme.notice(Tone::Muted, "that printed nothing"));
            }
            match status {
                Some(0) => {}
                // `Warning` and not `Error`: the shell worked, the command
                // answered, and the answer was a number the operator may well
                // have wanted. The same distinction `Tone::Refused` draws.
                Some(code) => out.push(theme.notice(Tone::Warning, format!("exited {code}"))),
                None => out.push(theme.notice(Tone::Warning, "ended without an exit status")),
            }
        }
    }

    out.push(Line::from(""));
    out
}

/// Captured output, indented under its header and made safe to put in a terminal.
///
/// **The control characters are dropped, and that is the load-bearing half.**
/// This is arbitrary output from a program io-cli did not write, and on this
/// renderer the scrollback *is* the transcript: a `\x1b[3J` arriving inside a `!`
/// line's stdout would erase it, and no test of io-cli's own byte stream would
/// ever see that coming. So what reaches [`crate::term::Screen::commit`] is text
/// and only text. The cost is colour — `!ls --color=always` arrives plain — and
/// that is the right side to lose on: a transcript that cannot be destroyed by
/// something the operator ran once.
///
/// `str::lines` already drops a trailing newline and a `\r` before one; the
/// filter takes the rest, tabs included, which would otherwise put a cell count
/// and a column count out of step.
fn body(text: &str, style: ratatui::style::Style) -> Vec<Line<'static>> {
    text.lines()
        .map(|line| {
            let safe: String = line.chars().filter(|glyph| !glyph.is_control()).collect();
            Line::from(Span::styled(format!("  {safe}"), style))
        })
        .collect()
}
