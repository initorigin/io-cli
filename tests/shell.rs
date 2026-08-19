//! F4 — `!` runs a line in the operator's shell and puts the output in the
//! scrollback.
//!
//! Four claims, asserted separately because they fail separately.
//!
//! **A `!` line is not sent to the agent.** That is the whole point of the
//! feature and it is decided in `App::compose`, off the first character of a
//! submitted line, next to the `/` that already worked this way — so it is
//! asserted through `App::key`, which is what the driver falls through to.
//!
//! **What the command printed reaches the scrollback**, along with how it ended.
//! A command that fails is reported with its status rather than silently, and a
//! command that writes nothing says so — because a header with nothing under it
//! looks exactly like a `!` line that never ran.
//!
//! **The viewport is never handed over**, so `Screen`'s inline origin cannot go
//! stale. This is the claim the whole captured-output design exists to satisfy
//! and it is the one a manual test on a short command would never see: the origin
//! is an absolute screen row maintained by arithmetic, so a handover corrupts it
//! *permanently* and quietly, and only for content committed afterwards.
//! Asserted over the real byte stream through `tests/support`'s `CrosstermBackend`
//! — the origin before and after, the restore hook that must never fire, and the
//! sequences `tests/structure.rs` already forbids, which have to stay absent when
//! the bytes going past include a child process's output.
//!
//! **Foreign output cannot destroy the transcript.** The last one is not in the
//! criterion's words but is implied by every one of them: on this renderer the
//! scrollback *is* the transcript, and a `!` line's stdout is bytes io-cli did
//! not write. A `\x1b[3J` arriving inside them would erase it, and no assertion
//! over io-cli's own output would ever catch that.
//!
//! The driver in `src/main.rs` has no test that can reach it, so what is exercised
//! here is exactly the sequence it runs: `App::key` for the decision,
//! [`shell::run`] for the spawn, [`shell::lines`] for what is committed, and
//! `Screen::commit` for the commit.

mod support;

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

use io_cli::app::{App, Command};
use io_cli::shell::{self, Ran};
use io_cli::theme::DARK;

/// A command that exits cleanly having printed nothing at all. `true` is POSIX;
/// `rem` is `cmd.exe`'s comment, which is the same idea spelled for the other
/// shell. Nothing that prints — `cd` on `cmd.exe` prints the directory — and
/// nothing that sleeps, because `tests/timing.rs` forbids a clock in here and a
/// shelled-out `sleep` is a clock with a subprocess in front of it.
const PRINTS_NOTHING: &str = if cfg!(windows) { "rem" } else { "true" };

fn key(code: KeyCode) -> KeyEvent {
    KeyEvent::new(code, KeyModifiers::NONE)
}

/// Type a line into a fresh session and press Enter, exactly as an operator does.
fn submit(text: &str) -> Command {
    let mut app = App::new(DARK, "opus-5");
    for character in text.chars() {
        app.key(key(KeyCode::Char(character)));
    }
    app.key(key(KeyCode::Enter))
}

/// The line a `Command::Shell` carries, or a failure that says what came back
/// instead.
fn shell_line(text: &str) -> String {
    match submit(text) {
        Command::Shell(line) => line,
        other => panic!("{text:?} should be a shell line, not {other:?}"),
    }
}

/// Everything [`shell::lines`] would commit, as the text a terminal would show.
fn rendered(line: &str, ran: &Ran) -> String {
    shell::lines(line, ran, &DARK)
        .iter()
        .map(|row| row.spans.iter().map(|span| span.content.as_ref()).collect())
        .collect::<Vec<String>>()
        .join("\n")
}

/// **F4.** A line beginning `!` is not sent to the agent.
///
/// The first half of the criterion, and the half that is a routing decision
/// rather than a process. `Command::Submit` here would mean the operator's
/// `!git status` had been handed to a model as a question.
#[test]
fn f4_a_bang_line_is_not_sent_to_the_agent() {
    assert_eq!(shell_line("!echo hello"), "echo hello");
    // The whole of the rest of the line, whatever is in it. A `!` line is handed
    // to a shell verbatim — quoting, pipes and redirection are the shell's
    // business and not something this crate parses.
    assert_eq!(
        shell_line("!git log --oneline | head -3"),
        "git log --oneline | head -3"
    );
    // Surrounding space is trimmed, which is what the slash path already does.
    assert_eq!(shell_line("!  ls -l  "), "ls -l");
}

/// **F4.** A `!` anywhere but the front is an ordinary character.
///
/// The rule is on the first character of a submitted line and nothing else. A
/// prompt that mentions a shell, or ends in an exclamation mark, is a prompt.
#[test]
fn f4_a_bang_that_is_not_the_first_character_is_a_prompt() {
    assert_eq!(
        submit("fix this!"),
        Command::Submit("fix this!".to_string()),
    );
    assert_eq!(
        submit("what does !ls do"),
        Command::Submit("what does !ls do".to_string()),
    );
    // And a slash still wins, because it is tested first and a `/` line has
    // never been a shell line.
    assert_eq!(submit("/help"), Command::Slash("help".to_string()));
}

/// **F4.** A bare `!` runs nothing.
///
/// It is neither a command nor a prompt: there is nothing to hand a shell, and a
/// lone `!` submitted to a model is a keystroke that missed rather than a
/// question. `Command::Shell("")` would spawn a shell to run an empty line, which
/// is a process started for nothing.
#[test]
fn f4_a_bang_with_nothing_after_it_runs_nothing() {
    assert_eq!(submit("!"), Command::None);
    assert_eq!(submit("!   "), Command::None);
}

/// **F4.** The shell is the operator's own, and an empty name is not a name.
///
/// A function of the value rather than of the environment, so this states an
/// unset `$SHELL` without a test process mutating its own environment out from
/// under every other test in this binary.
#[test]
fn f4_the_shell_is_the_operators_own_with_a_fallback_that_exists() {
    let (named, flag) = shell::shell(Some("/usr/bin/fish"));
    assert_eq!(named, "/usr/bin/fish", "the operator's own shell is used");
    assert_eq!(flag, if cfg!(windows) { "/C" } else { "-c" });

    // `%COMSPEC%` is routinely set to the empty string and `Command::new("")`
    // fails with an error that names nothing, so blank is treated as absent.
    for blank in [None, Some(""), Some("   ")] {
        let (fallback, _) = shell::shell(blank);
        assert_eq!(
            fallback,
            if cfg!(windows) { "cmd.exe" } else { "/bin/sh" },
            "an absent or blank {} should fall back to a shell that exists",
            shell::SHELL_VAR,
        );
    }
}

/// **F4.** What a command printed is what goes into the scrollback.
#[test]
fn f4_the_output_of_a_command_reaches_the_scrollback() {
    let line = shell_line("!echo io-cli-probe");
    let ran = shell::run(&line);

    let Ran::Output {
        stdout,
        stderr,
        status,
    } = &ran
    else {
        panic!("a shell that cannot echo is not a shell: {ran:?}");
    };
    assert!(
        stdout.contains("io-cli-probe"),
        "stdout was not captured: {stdout:?} / {stderr:?}",
    );
    assert_eq!(*status, Some(0));

    let shown = rendered(&line, &ran);
    assert!(
        shown.contains("! echo io-cli-probe"),
        "the line that was run is echoed above its output: {shown:?}",
    );
    assert!(shown.contains("io-cli-probe"), "{shown:?}");
    // Nothing is said about how it ended, because it ended the way a command is
    // expected to. A session that announced every success would be a session
    // that narrates `ls`.
    assert!(!shown.contains("exited"), "{shown:?}");
    assert!(!shown.contains("printed nothing"), "{shown:?}");
}

/// **F4.** A command that fails is reported with its status rather than silently.
///
/// The tone is a warning and not an error: the shell worked and the command
/// answered, and the answer was a number the operator may well have been asking
/// for. What must not happen is nothing.
#[test]
fn f4_a_command_that_fails_is_reported_with_its_status() {
    let line = shell_line("!exit 3");
    let ran = shell::run(&line);

    let Ran::Output { status, .. } = &ran else {
        panic!("the shell should have started: {ran:?}");
    };
    assert_eq!(*status, Some(3), "the child's own code, unmodified");

    let shown = rendered(&line, &ran);
    assert!(shown.contains("exited 3"), "{shown:?}");
}

/// **F4.** A command that writes nothing says so.
///
/// Otherwise a `!` line commits a header with nothing under it, which looks
/// exactly like one that never ran — and the operator's next move is to press
/// Enter again.
#[test]
fn f4_a_command_that_prints_nothing_says_so() {
    let line = shell_line(&format!("!{PRINTS_NOTHING}"));
    let ran = shell::run(&line);

    let Ran::Output { status, .. } = &ran else {
        panic!("the shell should have started: {ran:?}");
    };
    assert_eq!(*status, Some(0));

    let shown = rendered(&line, &ran);
    assert!(shown.contains("printed nothing"), "{shown:?}");
}

/// **F4.** A command that does not exist is the shell's own answer, not io-cli's.
///
/// The shell started fine — this is not [`Ran::Unstartable`] — and it has a
/// better sentence about a missing command than anything written here could be,
/// because it knows what it looked for and where.
#[test]
fn f4_a_command_that_does_not_exist_comes_back_as_the_shells_own_message() {
    let line = shell_line("!io-cli-no-such-command-7f3a");
    let ran = shell::run(&line);

    let Ran::Output { stderr, status, .. } = &ran else {
        panic!("the shell itself exists; only the command does not: {ran:?}");
    };
    assert_ne!(*status, Some(0), "a missing command is not a success");
    assert!(
        !stderr.trim().is_empty(),
        "the shell's complaint is what the operator needs and it arrives on stderr",
    );

    // Asserted against whatever the shell actually said rather than against a
    // wording, because every shell words it differently and none of them is
    // io-cli's to choose. What matters is that it arrives.
    let said = stderr.lines().next().expect("a shell that says why");
    let shown = rendered(&line, &ran);
    assert!(
        shown.contains(said.trim()),
        "the shell's own answer has to survive into the scrollback: \
         {said:?} is not in {shown:?}",
    );
    assert!(shown.contains("exited"), "{shown:?}");
}

/// **F4.** A shell that cannot be started says so, and is a different thing from
/// a command that does not exist.
#[test]
fn f4_a_shell_that_cannot_be_started_is_reported_as_such() {
    let shown = rendered(
        "ls",
        &Ran::Unstartable("/nowhere/sh: No such file or directory".to_string()),
    );
    assert!(shown.contains("no shell to run that in"), "{shown:?}");
    assert!(shown.contains("/nowhere/sh"), "{shown:?}");
}

/// **F4.** The viewport is never handed over, so its inline origin survives.
///
/// **This is the assertion the whole design exists to satisfy**, and the one a
/// manual test would not see: ratatui places an inline viewport at an absolute
/// screen row computed once from a cursor query and maintained afterwards by
/// arithmetic alone. Output that io-cli did not write scrolls the screen and
/// ratatui learns nothing, so a handed-over terminal leaves the origin wrong
/// *from then on* — and the transcript is real scrollback, so nothing can be
/// redrawn to fix it. A short command would look perfect and the next commit
/// would land in the wrong place.
///
/// So three things are asserted over one real session: the viewport's own
/// rectangle is identical before and after a `!` line, the terminal was never
/// handed back — `Screen::restore` is what a handover would have to call, and it
/// is observable — and the byte stream still contains none of what
/// `tests/structure.rs` forbids, now that a child process's output has gone past.
#[test]
fn f4_the_viewport_origin_survives_a_shell_line() {
    let (mut screen, recorder) = support::screen(80, 24);
    // A settled session rather than a fresh one: the first commit is what places
    // everything, and comparing across it would be comparing against a viewport
    // that had not been used yet.
    screen
        .commit(&[ratatui::text::Line::from("io-cli")])
        .expect("a preamble");
    screen.draw(|_| {}).expect("a first frame");
    let before = screen.terminal_mut().get_frame().area();

    let handed_back = Arc::new(AtomicBool::new(false));
    let flag = Arc::clone(&handed_back);
    screen.on_restore(move || flag.store(true, Ordering::Relaxed));

    // Exactly the sequence `src/main.rs` runs, and nothing else.
    let line = shell_line("!echo io-cli-probe");
    let ran = shell::run(&line);
    screen
        .commit(&shell::lines(&line, &ran, &DARK))
        .expect("the output goes into the scrollback");
    screen.draw(|_| {}).expect("a frame afterwards");

    let after = screen.terminal_mut().get_frame().area();
    assert_eq!(
        before, after,
        "the inline viewport moved across a `!` line. Every commit after this one \
         lands in the wrong rows, permanently, because the scrollback above it \
         belongs to the terminal and cannot be redrawn.",
    );
    assert!(
        !handed_back.load(Ordering::Relaxed),
        "the terminal was handed back to run a `!` line. Restoring it and taking it \
         again is the one thing this design exists not to do — see `io_cli::shell` \
         for what it would cost and why `nix` is what it would take to make safe.",
    );

    let text = recorder.text();
    // A liveness check and nothing more: that the `!` line reached this terminal
    // at all, so the sequence assertions below are about a session something
    // actually happened in. *What* was captured is asserted separately, over the
    // values, where a failure names the stream rather than the byte stream.
    assert!(
        text.contains("io-cli-probe"),
        "the `!` line never reached the terminal",
    );
    for (name, sequence) in support::FORBIDDEN {
        assert!(
            !text.contains(sequence),
            "running a `!` line put {name} ({}) in the byte stream",
            sequence.escape_debug(),
        );
    }
    assert!(!text.contains("\x1b[2J"), "a `!` line cleared the screen");
    assert!(
        !text.contains("\x1b[3J"),
        "a `!` line erased the scrollback, which is where the transcript lives",
    );
}

/// **F4.** Foreign output cannot destroy the transcript.
///
/// Captured output is arbitrary bytes from a program io-cli did not write, and on
/// this renderer the scrollback *is* the transcript. Passing it through
/// unfiltered means any command whose output contains `ESC [ 3 J` — a stray log
/// line, a `cat` of a binary, a program being clever about its own screen —
/// erases the whole conversation, and every assertion this repository makes about
/// io-cli's own byte stream would still be green.
///
/// Driven through the real backend rather than over the rendered string, because
/// the question is what leaves the process.
#[test]
fn f4_foreign_output_cannot_erase_the_transcript() {
    let (mut screen, recorder) = support::screen(80, 24);
    let ran = Ran::Output {
        stdout: "\x1b[2Jbefore\x1b[3Jafter".to_string(),
        stderr: "\x1b[?1049hand this".to_string(),
        status: Some(0),
    };
    screen
        .commit(&shell::lines("cat log", &ran, &DARK))
        .expect("commit");
    screen.draw(|_| {}).expect("a frame");
    drop(screen);

    let text = recorder.text();
    // The words survive; the sequences do not. What is left of a stripped
    // sequence is its printable tail — `[2J` arrives as those three characters —
    // which is the right outcome: the operator sees that something was in the
    // output, and the terminal never acts on it.
    for word in ["before", "after", "and this"] {
        assert!(
            text.contains(word),
            "{word:?} should have survived: {text:?}"
        );
    }
    assert!(
        !text.contains("\x1b[2J"),
        "a `!` line's output cleared the screen"
    );
    assert!(
        !text.contains("\x1b[3J"),
        "a `!` line's output erased the scrollback, and with it the transcript",
    );
    for (name, sequence) in support::FORBIDDEN {
        assert!(
            !text.contains(sequence),
            "a `!` line's output put {name} ({}) on the wire",
            sequence.escape_debug(),
        );
    }
}
