#!/usr/bin/env python3
"""Drive io's inline TUI on a pty that answers the queries a real terminal answers.

`script -q /dev/null ./io` is not enough: io asks the terminal where its cursor is
(`ESC[6n`) before drawing anything, and a bare pty has no emulator behind it to
reply, so the program exits with the sentence it prints for exactly that case.

This is the smallest thing that makes a headless run of the real binary possible:
a pty that replies to the cursor-position report and to the primary device
attributes query, echoes nothing, and writes the whole byte stream to a file so a
gate can assert on it.

Usage: drive.py <outfile> <cmd...>  with a script of keystrokes on stdin as
       "<delay-seconds>\t<text>" lines.

**A script that keys off a clock is a flaky script.** 0.11.0 spent three runs of
the same script producing three different captures — the palette not opening, the
palette not closing, and the whole thing working — because a keystroke sent at a
fixed second lands wherever the machine's load puts it. So an entry whose text is
`wait:<needle>` sends nothing and blocks until `<needle>` has appeared in what the
program wrote. A script that waits for the prompt before typing at it is a script
that does the same thing twice.
"""
import fcntl, os, pty, re, select, struct, sys, termios, time

CURSOR_QUERY = re.compile(rb"\x1b\[6n")
DA1_QUERY = re.compile(rb"\x1b\[(?:0)?c")
# The Kitty keyboard-enhancement query. Answering "not supported" keeps the
# negotiation from costing its full timeout.
KITTY_QUERY = re.compile(rb"\x1b\[\?u")
# Whether this pty pretends to be a terminal that speaks the Kitty keyboard
# protocol. Default yes, as every release before 0.13.0 assumed.
KITTY = os.environ.get("IO_DRIVE_KITTY", "1") != "0"

out_path, cmd = sys.argv[1], sys.argv[2:]
script = []
for raw in sys.stdin.read().splitlines():
    if not raw.strip():
        continue
    delay, _, text = raw.partition("\t")
    script.append((float(delay), text))

pid, fd = pty.fork()
if pid == 0:
    # The graphics decision is read from the environment, so a run that
    # INHERITED the parent shell's `TERM_PROGRAM` would be testing this machine's
    # terminal rather than the arm the gate is about. Every variable the decision
    # looks at is cleared, then set from IO_DRIVE_ENV.
    for name in ("TERM_PROGRAM", "KITTY_WINDOW_ID", "GHOSTTY_RESOURCES_DIR",
                 "KONSOLE_VERSION", "TMUX", "STY"):
        os.environ.pop(name, None)
    os.environ["TERM"] = "xterm-256color"
    size = os.environ.get("IO_DRIVE_SIZE", "30x100").split("x")
    os.environ["LINES"], os.environ["COLUMNS"] = size[0], size[1]
    for pair in os.environ.pop("IO_DRIVE_ENV", "").split(","):
        if "=" in pair:
            key, _, value = pair.partition("=")
            os.environ[key] = value
    os.execvp(cmd[0], cmd)

# `pty.fork` leaves the window at 0x0, and an inline viewport clamped to
# `rows - 2` is then zero rows tall — the program runs, draws nothing, and looks
# hung. Setting a real size is what makes this a terminal rather than a pipe.
rows, _, cols = os.environ.get("IO_DRIVE_SIZE", "30x100").partition("x")
fcntl.ioctl(fd, termios.TIOCSWINSZ, struct.pack("HHHH", int(rows), int(cols), 0, 0))

captured = bytearray()
started = time.monotonic()
# The first entry's delay gates the first entry, not the second.
next_at = started + (script[0][0] if script else 0)
deadline = started + float(os.environ.get("IO_DRIVE_DEADLINE", "60"))
sent = 0
# Where the next `wait:` starts looking. See the wait arm below.
seen_from = 0

while time.monotonic() < deadline:
    ready, _, _ = select.select([fd], [], [], 0.2)
    if ready:
        try:
            chunk = os.read(fd, 65536)
        except OSError:
            break
        if not chunk:
            break
        captured += chunk
        # Reply as a terminal would. Position is arbitrary but must be plausible.
        if CURSOR_QUERY.search(chunk):
            os.write(fd, b"\x1b[24;1R")
        # **The keyboard answer goes out BEFORE the device-attributes one, and the
        # order is the whole of whether it is heard.** crossterm asks the two
        # together — `CSI ? u` then `CSI c` — and treats the DA1 reply as the end
        # of the answer: whatever has not arrived by then is "not supported". Both
        # queries land in one read here, so a driver that answered in the order it
        # happened to check told every session this terminal speaks no protocol.
        # It did, until 0.13.0, and the first F9 capture pair was byte-identical
        # because of it.
        #
        # 0.13.0 — answering at all is now a choice, because F9 is about the
        # difference. A terminal that speaks the protocol replies with its current
        # flags; one that does not (Apple's Terminal, most `script` sessions)
        # replies never, and crossterm's two-second wait is paid in full and comes
        # back "no". `IO_DRIVE_KITTY=0` is that second terminal.
        if KITTY_QUERY.search(chunk) and KITTY:
            os.write(fd, b"\x1b[?0u")
        if DA1_QUERY.search(chunk):
            os.write(fd, b"\x1b[?1;2c")
    now = time.monotonic()
    if sent < len(script) and now >= next_at:
        delay, text = script[sent]
        # `wait:` sends nothing. It holds the script until the program has
        # written the text it names, which is how a keystroke lands where it was
        # meant to rather than where the machine's load put it.
        if text.startswith("wait:"):
            # Only what has arrived since the previous step counts. The capture
            # is cumulative, so a wait for `ready` that searched the whole of it
            # would match the status line the session drew before the turn ever
            # started — and every wait after it would fall through at once.
            if text[5:].encode() not in captured[seen_from:]:
                continue
            seen_from = len(captured)
            sent += 1
            next_at = now + (script[sent][0] if sent < len(script) else 0)
            continue
        # `raw:` sends the keystrokes with no Enter after them, which is what a
        # key that OPENS something needs: `/` opens the command palette on the
        # keypress itself, so a `/` followed by Enter would submit the palette
        # rather than open it. `\xNN` escapes carry control keys.
        if text.startswith("raw:"):
            payload = text[4:].encode().decode("unicode_escape").encode("latin-1")
        else:
            payload = text.encode().decode("unicode_escape").encode("latin-1") + b"\r"
        os.write(fd, payload)
        seen_from = len(captured)
        sent += 1
        next_at = now + (script[sent][0] if sent < len(script) else 0)
    if sent >= len(script) and now > next_at + 3:
        break

os.close(fd)
try:
    os.waitpid(pid, os.WNOHANG)
except ChildProcessError:
    pass
try:
    os.kill(pid, 9)
except ProcessLookupError:
    pass

with open(out_path, "wb") as handle:
    handle.write(bytes(captured))
print(f"captured {len(captured)} bytes")
