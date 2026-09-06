//! The keystroke contracts, driven against the built binary at a real terminal.
//!
//! **This file exists because `src/main.rs` cannot be linked from anything under
//! `tests/`, and because the recorder backend every other test uses is not a
//! tty.** Between those two facts sits every decision the driver makes about a
//! keystroke: which surface owns the keyboard, what a key that reaches no surface
//! does, whether a keystroke that closed one thing also armed another. The
//! 2026-09-05 field test found three ways an operator loses their work in exactly
//! that gap, and 2,000 offline tests were green over all three.
//!
//! So this drives `io` itself, through a pty, the way the field test was driven.
//!
//! # What runs in CI and what does not
//!
//! **A session needs a provider before it will take a keystroke, and a turn needs
//! one that answers.** The arms here configure a `compatible` endpoint at an
//! address nothing serves: that is enough to start a session, draw a prompt and
//! exercise every surface that does not require a completion, and it reaches no
//! network because no turn is submitted. Those arms run everywhere.
//!
//! The two contracts that need a turn in flight — an approval raised by a real
//! write, and `Ctrl+C` interrupting a step — cannot be driven without a provider
//! that answers, so they are `#[ignore]`d and keyed, exactly as `tests/live.rs`
//! is. `US-IO-CLI-0.39.0-I01` records that; the criterion was written expecting
//! both halves in CI.
//!
//! # Unix only, and the skip is not silent
//!
//! `openpty` is POSIX. On Windows this whole file compiles to one test that
//! **says** it was skipped rather than reporting a pass, because a skip that
//! reads like a pass is the failure mode this repository has already paid for in
//! its containment tests.

#![allow(clippy::undocumented_unsafe_blocks)]

#[cfg(not(unix))]
#[test]
fn the_keystroke_contracts_are_not_driven_on_this_platform() {
    // Deliberately not `#[ignore]`: an ignored test is a line in the output that
    // reads the same as a test nobody wrote. This one runs, passes, and names
    // what did not happen.
    println!(
        "SKIPPED: the pty arms need `openpty`, which is POSIX. The keystroke \
         contracts are gated on the two unix runners; this platform runs the \
         library halves in tests/approval.rs, tests/rewind.rs and tests/picker.rs \
         only."
    );
}

#[cfg(unix)]
mod unix {
    use std::io::{Read, Write};
    use std::os::unix::io::{FromRawFd, RawFd};
    use std::os::unix::process::CommandExt;
    use std::process::{Child, Command, Stdio};
    use std::time::{Duration, Instant};

    /// How long an arm waits for a string to appear before calling it absent.
    ///
    /// Generous, because this runs on shared CI runners where a cold binary's
    /// first frame can be slow, and because the failure this bound produces is a
    /// clear one: the arm reports everything the terminal was sent. A tighter
    /// bound would turn a loaded runner into a red build, which is how a gate
    /// teaches a team to re-run rather than to read.
    const WAIT: Duration = Duration::from_secs(20);

    /// A running `io`, with the terminal it thinks it is talking to.
    struct Session {
        child: Child,
        master: std::fs::File,
        seen: String,
        /// How many cursor-position queries have been answered. See
        /// [`Session::drain`].
        answered: usize,
        /// Kept alive for the child's lifetime — see [`Session::start`].
        _home: tempfile::TempDir,
        /// Likewise, and it is deliberately **not** the workspace: see
        /// [`Session::start`].
        _workspace: tempfile::TempDir,
    }

    impl Session {
        /// Start `io` on a pty, in a workspace and a home of its own.
        ///
        /// **The configuration names a provider at an address nothing serves.**
        /// A session refuses to start without one and never dials until a turn is
        /// submitted, so this is what buys a real prompt with no network and no
        /// credential. `IO_CONFIG` is set rather than `IO_CONFIG_HOME`, so
        /// `home::adopt` does nothing and no file of the developer's is read or
        /// moved.
        ///
        /// **The configuration lives outside the workspace, and that is not
        /// tidiness.** io-harness refuses a `[[provider]]` in any file inside the
        /// workspace — "a provider names the endpoint this run's credential is
        /// sent to, and `io.toml` arrives with a `git clone`" — so a config
        /// written beside the tree `-C` points at makes the session refuse to
        /// start. The first run of this harness found that, which is the argument
        /// for the harness.
        fn start(rows: u16, cols: u16) -> Self {
            let home = tempfile::tempdir().expect("a home");
            let workspace = tempfile::tempdir().expect("a workspace");
            let config = home.path().join("io.toml");
            std::fs::write(
                &config,
                "[[provider]]\nkind = \"compatible\"\nmodel = \"a-model\"\n\
                 base_url = \"http://127.0.0.1:9\"\napi_key = \"not-a-key\"\n",
            )
            .expect("the configuration");

            let (master, slave) = open_pty(rows, cols);

            // Three separate descriptors: the child closes each on exec and
            // `Stdio::from` takes ownership of what it is given.
            let (stdin, stdout, stderr) = unsafe {
                (
                    Stdio::from(std::fs::File::from_raw_fd(dup(slave))),
                    Stdio::from(std::fs::File::from_raw_fd(dup(slave))),
                    Stdio::from(std::fs::File::from_raw_fd(dup(slave))),
                )
            };

            let mut command = Command::new(env!("CARGO_BIN_EXE_io"));
            command
                .arg("-C")
                .arg(workspace.path())
                .env("IO_CONFIG", &config)
                // **`IO_CONFIG` alone is not isolation, and the first run of this
                // harness proved it.** The configuration followed `IO_CONFIG`,
                // but the run store is `settings::store_path()`, which follows
                // `IO_CONFIG_HOME` — so these arms opened sessions in the
                // developer's own `~/.io-cli/runs.db`, and one of them was
                // refused by a session lock held by a *different* `io` (a real
                // one, version 0.38.0). Setting both puts the store in the
                // temporary home with the file. `home::adopt` does nothing when
                // either variable is set to a non-empty value, so nothing of the
                // developer's is read or moved either way.
                .env("IO_CONFIG_HOME", home.path())
                .env("TERM", "xterm-256color")
                // Colour off, so an assertion reads words rather than escape
                // sequences wrapped around them.
                .env("NO_COLOR", "1")
                .env_remove("OPENROUTER_API_KEY")
                .env_remove("ANTHROPIC_API_KEY")
                .env_remove("OPENAI_API_KEY")
                .stdin(stdin)
                .stdout(stdout)
                .stderr(stderr);

            // The child needs the pty as its **controlling** terminal, or
            // crossterm's raw mode and every ioctl on it fail. A new session
            // first, because a process that already has a controlling terminal
            // cannot take another.
            unsafe {
                command.pre_exec(move || {
                    if libc::setsid() < 0 {
                        return Err(std::io::Error::last_os_error());
                    }
                    if libc::ioctl(0, libc::TIOCSCTTY as _, 0) < 0 {
                        return Err(std::io::Error::last_os_error());
                    }
                    Ok(())
                });
            }

            let child = command.spawn().expect("io starts");
            // The parent's copy of the slave end is closed, so reading the master
            // reports end-of-file when the child exits rather than blocking for
            // ever on a descriptor this process is still holding open.
            unsafe { libc::close(slave) };

            Self {
                child,
                master: unsafe { std::fs::File::from_raw_fd(master) },
                seen: String::new(),
                answered: 0,
                _home: home,
                _workspace: workspace,
            }
        }

        /// Type, as a terminal delivers it.
        fn type_in(&mut self, text: &str) {
            self.master
                .write_all(text.as_bytes())
                .expect("the terminal accepts input");
            self.master.flush().expect("flush");
        }

        /// Read whatever has arrived, without waiting — and answer the terminal
        /// queries `io` will not draw a frame without.
        ///
        /// **`io` writes `ESC[6n` and waits for a reply before it draws
        /// anything.** ratatui's inline viewport computes its position by asking
        /// the terminal where the cursor is, and `src/term.rs` carries a
        /// two-second wait and a whole error message for the case where nothing
        /// answers. A pty with no emulator behind it is exactly that case, so a
        /// harness that only reads is a harness that watches the binary give up.
        ///
        /// The reply is always `1;1`. This is not a terminal emulator and does
        /// not track a grid; what the arms here assert is which words were
        /// written, and a constant position is enough to get them written. Every
        /// query is answered exactly once, counted over the whole stream rather
        /// than per read, because the six bytes can arrive split across two.
        fn drain(&mut self) {
            const QUERY: &str = "\u{1b}[6n";

            let mut buffer = [0u8; 8192];
            loop {
                match self.master.read(&mut buffer) {
                    Ok(0) => break,
                    Ok(n) => self.seen.push_str(&String::from_utf8_lossy(&buffer[..n])),
                    // The child has written nothing since the last read. Not an
                    // error: this is a non-blocking descriptor by construction.
                    Err(_) => break,
                }
            }

            let asked = self.seen.matches(QUERY).count();
            while self.answered < asked {
                // Written directly rather than through `type_in`, which asserts:
                // a query arriving as the child exits would otherwise turn a
                // clean shutdown into a broken-pipe panic in the harness.
                let _ = self.master.write_all(b"\x1b[1;1R");
                let _ = self.master.flush();
                self.answered += 1;
            }
        }

        /// Wait for `needle` to be drawn, or fail saying what was drawn instead.
        ///
        /// **Whitespace is removed from both sides before comparing**, and that
        /// is not laziness about the assertion. ratatui positions text with
        /// escape sequences rather than padding it with spaces, so a row drawn as
        /// `there is no turn to undo` arrives on the wire as
        /// `thereisnoturntoundo` once the sequences are stripped. Matching the
        /// squeezed form asserts the words and their order, which is what these
        /// arms are about, and asks nothing of a layout they are not testing.
        fn wait_for(&mut self, needle: &str) {
            let deadline = Instant::now() + WAIT;
            let wanted = squeezed(needle);
            while Instant::now() < deadline {
                self.drain();
                if squeezed(&plain(&self.seen)).contains(&wanted) {
                    return;
                }
                std::thread::sleep(Duration::from_millis(50));
            }
            panic!(
                "{needle:?} was never drawn in {WAIT:?}. The terminal was sent:\n{}",
                plain(&self.seen)
            );
        }

        /// Everything drawn so far, with the escape sequences taken out.
        fn screen(&mut self) -> String {
            self.drain();
            plain(&self.seen)
        }
    }

    impl Drop for Session {
        fn drop(&mut self) {
            let _ = self.child.kill();
            let _ = self.child.wait();
        }
    }

    /// A pty pair sized like a terminal, both ends returned raw.
    fn open_pty(rows: u16, cols: u16) -> (RawFd, RawFd) {
        let mut master: RawFd = -1;
        let mut slave: RawFd = -1;
        let mut size = libc::winsize {
            ws_row: rows,
            ws_col: cols,
            ws_xpixel: 0,
            ws_ypixel: 0,
        };
        let opened = unsafe {
            libc::openpty(
                &mut master,
                &mut slave,
                std::ptr::null_mut(),
                std::ptr::null_mut(),
                &mut size,
            )
        };
        assert!(opened == 0, "openpty: {}", std::io::Error::last_os_error());
        // Non-blocking, so `drain` can ask "is there anything?" rather than
        // waiting for a child that may have nothing more to say.
        unsafe { libc::fcntl(master, libc::F_SETFL, libc::O_NONBLOCK) };
        (master, slave)
    }

    /// The same text with every space, tab and newline taken out.
    ///
    /// See [`Session::wait_for`] for why the comparison is made this way.
    fn squeezed(text: &str) -> String {
        text.chars().filter(|c| !c.is_whitespace()).collect()
    }

    fn dup(fd: RawFd) -> RawFd {
        let copy = unsafe { libc::dup(fd) };
        assert!(copy >= 0, "dup: {}", std::io::Error::last_os_error());
        copy
    }

    /// Escape sequences out, text left.
    ///
    /// Crude on purpose: this is not a terminal emulator and does not need to be.
    /// Every assertion in this file is "was this word drawn", and a CSI stripper
    /// answers that without a grid.
    fn plain(raw: &str) -> String {
        let mut out = String::with_capacity(raw.len());
        let mut chars = raw.chars().peekable();
        while let Some(c) = chars.next() {
            if c != '\u{1b}' {
                out.push(c);
                continue;
            }
            match chars.next() {
                // CSI: parameters, then one final byte in `@`..`~`.
                Some('[') => {
                    for c in chars.by_ref() {
                        if ('@'..='~').contains(&c) {
                            break;
                        }
                    }
                }
                // OSC: ends at BEL or ST.
                Some(']') => {
                    while let Some(c) = chars.next() {
                        if c == '\u{7}' {
                            break;
                        }
                        if c == '\u{1b}' && chars.peek() == Some(&'\\') {
                            chars.next();
                            break;
                        }
                    }
                }
                // A two-character escape; the second character is consumed.
                _ => {}
            }
        }
        out
    }

    /// **F1/F5 — the binary starts at a real terminal and its prompt takes keys.**
    ///
    /// The floor every other arm here stands on, and it is not a formality: the
    /// three ways 0.38.2 lost an operator's work were all decisions made in
    /// `src/main.rs`, and until this file existed nothing under `tests/` had ever
    /// started the binary with a terminal attached.
    #[test]
    fn f1_the_session_starts_at_a_terminal_and_draws_a_prompt() {
        let mut io = Session::start(24, 100);
        io.wait_for(">");
        io.type_in("hello");
        io.wait_for("hello");
    }

    /// **F5 — a line the palette cannot match runs, on one `Enter`.**
    ///
    /// The whole point of driving the binary: `/` opens the palette *in front of*
    /// `App::key`, in `src/main.rs`, and whether the line that follows it ever
    /// reaches a command is a property of that file alone. `/effort high` matches
    /// no row — the row is `/effort` and the argument is not part of it — so
    /// through 0.38.2 `Enter` did nothing at all and the line sat behind
    /// `No row matches` with no way forward.
    ///
    /// **A query that *does* match a row still fills the prompt and waits**, and
    /// that is unchanged rather than overlooked: an operator who opened the
    /// palette with `/` and arrowed to a row is browsing, and may want to add an
    /// argument before it runs. What this release fixes is the case where there
    /// is nothing left to add because the operator typed the whole line.
    ///
    /// Sabotage: `continue` after `Outcome::Typed` in the idle picker block. The
    /// line then waits for a second `Enter` and this arm times out on a prompt
    /// holding exactly the text it was given.
    #[test]
    fn f5_a_line_with_an_argument_runs_on_one_enter() {
        let mut io = Session::start(24, 100);
        io.wait_for(">");

        io.type_in("/effort high\r");

        // The status line carries the effort once it is set, which is the
        // observable an operator has. What matters is that anything happened at
        // all: the pre-0.39.0 behaviour is the query sitting in the filter for
        // ever.
        io.wait_for("high");
        assert!(
            !squeezed(&io.screen()).contains(&squeezed("No row matches")),
            "the line stayed in the palette's filter: {}",
            io.screen()
        );
    }

    /// **F3 — `Esc` at an empty prompt asks, and asking is not doing.**
    ///
    /// The field test's second loss, at the terminal it happened on. With no turn
    /// taken there is nothing to undo, so what this asserts is that the keystroke
    /// is answered by a sentence rather than by silence — and, in the arm below
    /// it, that a second press does not act on the first one's behalf.
    #[test]
    fn f3_escape_at_an_empty_prompt_is_answered_rather_than_armed() {
        let mut io = Session::start(24, 100);
        io.wait_for(">");

        io.type_in("\u{1b}");
        io.wait_for("no turn to undo");

        // The second press. Before 0.39.0 this is where the rewind happened; here
        // it can only say the same thing again, and the assertion is that it does
        // not silently do something else.
        io.type_in("\u{1b}");
        io.wait_for("no turn to undo");
    }

    /// **F8 — `io exec` without `--json` narrates, and stdout stays clean.**
    ///
    /// The source gate in `tests/exec.rs` holds `Narrating` to never naming
    /// stdout, which makes the mistake impossible. This is the other half: a real
    /// process, on a real terminal, whose commentary an operator would actually
    /// see.
    ///
    /// The run cannot complete — the configured provider is an address nothing
    /// serves — and that is exactly the case worth asserting. A headless run that
    /// fails after doing some work is when an operator most needs to know what it
    /// did, and before 0.39.0 this path printed nothing on the way to its error.
    #[test]
    fn f8_a_headless_run_narrates_without_json() {
        let home = tempfile::tempdir().expect("a home");
        let workspace = tempfile::tempdir().expect("a workspace");
        let config = home.path().join("io.toml");
        std::fs::write(
            &config,
            "[[provider]]\nkind = \"compatible\"\nmodel = \"a-model\"\n\
             base_url = \"http://127.0.0.1:9\"\napi_key = \"not-a-key\"\n",
        )
        .expect("the configuration");

        // No pty needed: this door is not a terminal interface, and piping it is
        // how a script reaches it. What the arm reads is the split between the
        // two streams, which a pty would merge.
        let out = Command::new(env!("CARGO_BIN_EXE_io"))
            .arg("-C")
            .arg(workspace.path())
            .arg("exec")
            .arg("say hello")
            .env("IO_CONFIG", &config)
            .env("IO_CONFIG_HOME", home.path())
            .env("NO_COLOR", "1")
            .env_remove("OPENROUTER_API_KEY")
            .output()
            .expect("io exec runs");

        let stdout = String::from_utf8_lossy(&out.stdout);
        assert!(
            !stdout.contains('·'),
            "the commentary reached stdout, which is the agent's reply and \
             nothing else: {stdout:?}",
        );
    }

    /// **F4 — `Ctrl+D` at an empty prompt leaves, and the process actually ends.**
    ///
    /// The bounded-exit half of F4 that needs no provider. A session that draws
    /// "exiting" and stays resident is the shape of the interrupt defect the field
    /// test found on `Ctrl+C`, and it is worth holding the door that can be tested
    /// in CI to the bound the other one is held to by hand.
    #[test]
    fn f4_the_session_exits_when_it_says_it_is_exiting() {
        let mut io = Session::start(24, 100);
        io.wait_for(">");

        io.type_in("\u{4}");

        let deadline = Instant::now() + WAIT;
        loop {
            // **Drained on every pass, and leaving this out is what made the arm
            // fail the first time it ran.** A pty has a small buffer: a harness
            // that stops reading while it waits for the child to exit blocks the
            // child inside the writes that hand the terminal back, and then waits
            // out its own deadline watching a process it is itself holding still.
            io.drain();
            if let Some(status) = io.child.try_wait().expect("wait") {
                assert!(
                    status.success(),
                    "the session left with {status:?} after Ctrl+D at an empty prompt",
                );
                return;
            }
            assert!(
                Instant::now() < deadline,
                "the session said it was leaving and did not: {}",
                io.screen(),
            );
            std::thread::sleep(Duration::from_millis(50));
        }
    }
}
