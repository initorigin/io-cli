# IO CLI

A terminal agent that shows you what it is allowed to do, what it is spending,
and what it refused — while it works.

`io` is an interface. The agent loop, the providers, the tools, the sandbox, the
permission boundary and the session store are all
[io-harness](https://github.com/initorigin/io-harness), and none of them are
reimplemented here. A test asserts that: `tests/dependencies.rs` fails the build
if this crate ever grows an HTTP client, a TLS stack, a database or a sandbox.

![A session in flight: the IO CLI mark and the finished transcript sitting in the
terminal's own scrollback, a prompt, and one status line naming the model, a
moving indicator beside the word `working`, and the elapsed clock — which
advances on its own while the first token is still on its way.](docs/screenshot.png)

## It never takes your terminal

`io` does not enter the alternate screen and does not capture the mouse, in any
mode, behind any flag. Every finished message, tool call and system line is
committed into the terminal's own scrollback; a few lines at the bottom hold the
composer and the status line, and only those repaint.

So when the session ends the whole conversation is still there. Your terminal's
search finds it, tmux copy-mode scrolls it, and a mouse drag selects it — none of
which is implemented here. It works because it was never taken away.

That is a property, not an intention. `tests/structure.rs` captures every byte
`io` writes to the terminal over a scripted session and fails if the
alternate-screen or mouse-capture sequences appear, so no later release can
reintroduce fullscreen without turning a named test red.

## Install

macOS and Linux:

```sh
curl -fsSL https://raw.githubusercontent.com/initorigin/io-cli/main/install.sh | sh
```

Windows, in PowerShell:

```powershell
irm https://raw.githubusercontent.com/initorigin/io-cli/main/install.ps1 | iex
```

Both scripts pick the right build for your machine, **verify it against the
published `SHA256SUMS` before unpacking it**, and install into a directory you
own — `~/.local/bin`, or `%LOCALAPPDATA%\io\bin` on Windows. Neither needs
administrator rights and neither edits your shell profile: if the directory is
not on your `PATH`, the script prints the line to add.

Re-running the script is how you update. There is no auto-update and no version
check: a terminal tool that contacts a server you did not ask it to contact is
one of this product's stated non-goals.

Set `IO_VERSION` to install a specific version, and `IO_INSTALL_DIR` to install
somewhere else.

**What the checksum is and is not.** It defends against a truncated download and
a tampered asset. It does not defend against a compromised repository — piping a
script from the internet into a shell is a trust-the-publisher model however the
script is written, and this is the honest description of it. Read
[`install.sh`](install.sh) first if you would rather not take that on trust.

There is no crates.io publish and `cargo install io-cli` is not a path. To build
from source, clone this repository and run `cargo build --release`; the binary is
`target/release/io`.

## First run

Run `io` in a repository. With no configuration it walks you through a provider,
a key it verifies against the live endpoint before continuing, a model from that
provider's catalogue, a theme with the sample re-rendering as you move, and a
default permission posture. It shows exactly what it will write and where, and
writes nothing until you say so. The file is io-harness's own, at mode `0600`.

Run it again at any time with `io setup`.

Your key never appears on screen, in the scrollback, or in a log line. If the
provider's environment variable is already set, the wizard offers to use it and
writes no key to disk at all.

## Keys

<!-- keys:start -->

| Key | Does |
| --- | --- |
| `Enter` | send the prompt |
| `Shift+Enter` | new line (or end the line with \ and press Enter) |
| `Up / Down` | walk prompt history |
| `Ctrl+C` | interrupt the turn; twice at an empty prompt, exit |
| `Ctrl+D` | exit, on an empty prompt |
| `Shift+Tab` | cycle the permission posture, from the next turn |
| `Ctrl+L` | clear the viewport, never the scrollback |
| `Esc Esc` | at an empty prompt, undo the last turn — its files and all |
| `Ctrl+T` | put the whole conversation back into the scrollback |
| `y / a / n` | answer an approval: allow once, allow this session, deny |
| `Esc` | close a picker without choosing |

<!-- keys:end -->

## Commands

<!-- commands:start -->

| Command | Does |
| --- | --- |
| `/help` | this table |
| `/quit` | leave |
| `/setup` | run the first-run wizard again |
| `/theme` | change the theme for this session |
| `/model` | change the model the next turn is sent to |
| `/resume` | reopen an earlier session where it stopped |
| `/fork` | continue from an earlier turn of this conversation |
| `/expand` | commit the last step's full detail into the scrollback |
| `/copy` | put the last answer on the system clipboard |
| `/copy diff` | put the whole run's patch on the system clipboard |

<!-- commands:end -->

**Everything that shows you more of something writes it into the terminal's own
scrollback.** `Ctrl+T` and `/expand` do not open a pane: the viewport is a few
rows and this product has no alternate screen, so the place to read something
long is the buffer where the terminal's search, selection and tmux copy-mode
already work. `/expand` reads the step's full output back out of the run's
durable trace, which is where it went in the first place — the screen is not the
archive.

`/copy` uses OSC 52, so it reaches the clipboard of the machine you are *sitting
at* rather than the one you are ssh'd into. Nothing acknowledges an OSC 52 write:
the line it prints says what was sent and how large it was, never that it
succeeded. Inside tmux it needs `set -g set-clipboard on`, and some terminals
refuse a large payload without saying so.

`/theme` and `/model` change this session only and say so. Making a choice
permanent is `io setup`.

## Configuration

io-cli has no configuration parser. io-harness owns discovery and layering, and
io-cli's own settings live in the `[app.io-cli]` section that io-harness
deliberately does not validate. See [`docs/config.example.toml`](docs/config.example.toml).

Two keys live there: `theme`, and `diff`, which is `unified` (the default, and
what an absent key means) or `minimal` — the changed lines and the `@@` header
without the context, for reviewing by file rather than by hunk. Because the
section is unvalidated by design, an unrecognised value reads as the default
rather than stopping a session from starting.

The file is found in this order: `$IO_CONFIG`, else `$IO_CONFIG_HOME/io.toml`,
else `$XDG_CONFIG_HOME/io/io.toml` or `~/.config/io/io.toml`, and
`%APPDATA%\io\io.toml` on Windows. A project's own `io.toml` and a gitignored
`io.local.toml` layer on top of it.

One thing worth knowing: a **project** file may narrow the permission boundary
and may never widen it, because a repository you cloned must not be able to grant
itself permission. The wizard therefore writes the user-scope file, which is
where widening is your own decision.

**What this release reads from that file, and what it does not.** io-cli reads
the provider, the permission policy and its own `[app.io-cli]` section. The
policy's own defaults are what `Shift+Tab` cycles; a posture chosen with the key
lasts for the session and is not written back, because a keystroke that rewrites
a permission boundary on disk is the opposite of what that key is for. io-cli does
**not** yet apply `[sandbox]` limits, `[run]` budgets, `[[mcp]]`, `[[lsp]]`,
`[[agent]]` or an `AGENTS.md` instruction file to a turn. The reason is specific
rather than an oversight: io-harness's steerable turn builds its own task
contract, and the entry point that takes a caller's contract does not take a
steer inbox — so honouring those sections today would mean giving up `Ctrl+C`.
The sandbox itself **is** on: a workspace turn runs commands inside it, with no
resource ceilings until the harness offers a turn that is both contracted and
steerable.

`NO_COLOR` is honoured. Colour is never the only thing carrying a meaning — every
refusal, error and warning also carries a word.

## What this release is not

0.3.0 is what the agent did to your files, made readable: diffs from the
harness's own stored hunks, word-level emphasis, syntax highlighting, collapsed
tool output, the transcript on one key and the clipboard over SSH. Resume, fork
and rewind are 0.4.0; the headless subcommand and NDJSON are 0.5.0; the
screen-reader mode is 0.6.0; the slash palette and type-to-filter pickers are
0.7.0.

Three things are absent for reasons worth stating rather than hiding. **Spend
against the tree ceiling is not in the status line**: io-harness emits
`SpendDraw` only from a contained turn, and its contained entry point takes no
steer inbox, so rendering spend today would cost `Ctrl+C`. It arrives at 0.8.0
with the fleet, which is contained anyway. **A diff cannot be expanded beyond
the context the harness stored** — three lines either side, which is what
`diff -u` has always carried; more than that is not in the trace, and reading it
off disk would be reading a version of the file that no longer exists. And
**there is no split view**: this renderer commits into the terminal's own
scrollback at its real width, a two-column comparison doubles the horizontal
budget for every line, and word-level emphasis inside a unified diff already
answers the question split view answers.

One ceiling worth knowing about: a hunk is a fragment of a file, and each of its
lines is highlighted from a clean parse. A block comment or a multi-line string
that was opened *above* the hunk is not known here, so those lines read as code.

## Contributing

See [CONTRIBUTING.md](CONTRIBUTING.md). `develop` is the working branch; nothing
reaches it or `main` except through a pull request.

## Licence

Apache-2.0. Copyright 2026 Aakash Pawar (InitOrigin). See [LICENSE](LICENSE) and
[NOTICE](NOTICE).
