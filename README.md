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
| `Ctrl+F` | show the fleet: the children this turn has spawned |
| `y / a / n` | answer an approval: allow once, allow this session, deny |
| `Esc` | close a picker without choosing |
| `/` | at an empty prompt, open the command palette |
| `@` | after a space, complete a path from the workspace |
| `!` | run the rest of the line in your shell; the agent never sees it |

<!-- keys:end -->

**`Shift+Enter` works where the terminal reports it.** `io` negotiates the Kitty
keyboard protocol on terminals that advertise it, asking for one flag —
`DISAMBIGUATE_ESCAPE_CODES` — because without it a terminal sends the same byte
for `Enter` and for `Shift+Enter` and the newline binding is unreachable. What is
pushed is popped again on every path out of the process, a panic included. The
trailing-backslash fallback still works everywhere, and on a terminal that does
not advertise the protocol nothing is written at all.

### Moving a key

The keys the session itself owns can be rebound in `[app.io-cli.keys]`, by action
name:

```toml
[app.io-cli.keys]
clear = "ctrl+k"
rewind = "ctrl+r ctrl+r"
```

| Action | Default |
| --- | --- |
| `exit` | `Ctrl+D` |
| `posture` | `Shift+Tab` |
| `clear` | `Ctrl+L` |
| `transcript` | `Ctrl+T` |
| `rewind` | `Esc Esc` |
| `fleet` | `Ctrl+F` |

A binding is a chord, or two chords separated by a space. Modifiers are `ctrl`,
`alt` and `shift`, joined to the key with `+`, in any order and any case; a key is
a single character, a named key — `esc`, `enter`, `tab`, `backtab`, `space`, the
four arrows, `home`, `end`, `pageup`, `pagedown`, `backspace`, `delete`, `insert`
— or `f1` through `f12`. Because `+` is the join, `+` itself cannot be bound; that
is a real limit of the syntax and stating it beats a rule that quietly works for
`plus` and not for the character anyone would type. This spelling is public
contract from 0.6.0 on: it is the one VS Code, Zed and helix already write.

The rest of the table is not rebindable, because those keys belong to whatever
owns the keyboard while it is up — the composer, an approval, a picker — and an
approval's `y`, `a` and `n` are the *words* of the answer rather than shortcuts
for it.

**`Ctrl+C` is fixed, and it is the only one that is.** It interrupts a running
turn and leaves `io`, so a configuration file able to take it away is one able to
lock you inside a running agent. Both spellings of that mistake are refused out
loud with the reason: naming `interrupt`, and putting any other action onto
`ctrl+c`.

Nothing about a bad line is fatal and nothing is silent. A value that cannot be
read leaves its action on the default and names the key it kept; a name that is no
action of ours says which names there are; and every notice is committed into the
scrollback as the session starts, rather than left to be discovered by pressing
something. `/help` renders the table as the session *actually behaves* rather than
the defaults that shipped, and marks `Ctrl+C` as fixed.

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
| `/contain` | run turns contained, so the agent can fan out: on, off, or ask |
| `/fleet` | show the children this turn has spawned |
| `/attach` | put an image in front of the agent, for the next turn only |

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

## The fleet

An agent can break a task into sub-agents and run them over the same workspace.
io-cli does not implement any of that — io-harness does — but it is the only
terminal interface that can *show* it, because the facts it draws are ones only
that core emits.

It is off until you configure the caps it runs under:

```toml
[app.io-cli.containment]
max_total_agents = 12
max_concurrent_agents = 4
max_depth = 2
max_total_tokens = 200000
```

With that table present your turns run **contained**, and `Ctrl+F` or `/fleet`
opens a live view over the prompt: one row per child with its own state and what
it has drawn, a per-tier count of what is working, waiting and finished, and the
tree's remaining budget on the status line beside everything else. A refused
spawn says which cap refused it and that the agent carries on with what it has. A
report collected from a child lands in the transcript where it arrives.

**A waiting child is a number and not a row**, because until a concurrency slot
frees it has no run of its own to name. A fleet that is queueing and a fleet that
is stuck look identical without that count, which is why it is there.

**Turning this on costs you steering.** io-harness has one session entry point
that reaches its spawn loop, and it takes no steer inbox — so while a turn is
contained you cannot type at it to redirect it, and the agent roster, `[run]`
budgets and `[sandbox]` do not apply to it either. The session says so when it
starts. `Ctrl+C` still ends the turn, at the next point where no child is in
flight, and the interface tells you that is what it is waiting for rather than
appearing to have missed the key. `/contain off` gives the next turn back to
steering; `/contain on` takes it back.

## Pictures

`/attach @docs/shot.png` puts an image in front of the agent for the **next turn
and only the next turn**, and says so before the turn goes. Attach again for the
question after it.

The path is read through io-harness's own workspace, under the same policy as
everything else — its documentation is explicit that this is the same gate a
source read passes and not a second one — so an image outside what the session
may read is refused exactly the way a file outside it already is. What may be
sent is io-harness's decision too: bmp, tiff, ico, tga and pnm are converted to
PNG on the way in, jpeg, png, gif and webp go as they are, and svg, heic and avif
are refused **by name**, because a refusal that says which format it was is one
you can act on. A provider that does not accept images at all is refused at the
door rather than after you have typed the prompt.

**The agent can look at images in the workspace from this release**, using
io-harness's own `view_image` tool, which enabling its `media` feature switches
on. It is bounded by the same policy as any other read. When it looks, the same
picture goes into your scrollback at that point in the conversation, so you are
reading what it read rather than a path you would have to open yourself.

A picture is drawn from half blocks — `▀` splits a cell into two halves that are
each about square — fitted to your terminal's width and bounded in height. On
kitty, ghostty, WezTerm and Konsole a PNG is drawn as the **real image** instead.
Inside tmux or screen it is always half blocks: passing a graphics protocol
through a multiplexer needs configuration that is off by default, and an escape
the terminal cannot read is unreadable bytes written permanently into your
scrollback.

Under `--plain`, under `NO_COLOR`, and with the ASCII glyph set there is no
picture at all — one line naming the file, its format and its size. A half-block
picture is colour carrying the entire meaning, which is the one thing this
interface will not do.

## Background jobs

An agent can start something that outlives the step that started it — a dev
server, a watcher, a long build. That is the point of it and it is also the
problem: a run waiting on a background process looks exactly like a run that has
hung.

So the command is named when it starts, the status line grows a `bg 2` field
counting what is still alive, and each job says how it ended: exited with a
status, killed, or **left running** by a run that finished before it did. The
field is absent when nothing is running rather than showing `0` — a session that
has started no background work has not started zero jobs.

## Reading it without seeing it

`io --plain` runs the session without animation: nothing turns, nothing moves,
the ASCII glyph set is forced, and each state the session enters — `working`,
`ready` — is committed into the terminal's own scrollback as a line of text. In
the default interface that one state is carried by a word that only ever repaints
and an indicator that only ever moves, which makes it the single thing a reader
who cannot see the viewport cannot follow; everything else a run does already
writes a line. For a screen reader, a braille display, a serial console and a
captured log.

The flag is global, so `io --plain`, `io --plain exec "…"` and `io exec --plain
"…"` are all accepted. `[app.io-cli] plain = true` is the same switch for every
session. The flag wins over the file, and there is deliberately no `--no-plain`:
accessibility is something switched on on purpose, and a mode that can be lost to
a stray flag is not one to rely on. It reaches an interactive session and stops
there — `io exec` constructs no theme, draws nothing and animates nothing already.

Four properties this product keeps, whether or not that flag is set:

- **Colour is never the only thing carrying a meaning.** Every refusal, error and
  warning also carries a word, a diff's additions and removals are marked as well
  as coloured, and an approval says what is being asked for in words before it
  says it in colour. `NO_COLOR` is honoured on presence, whatever its value, and
  now survives the first-run wizard and `/theme`: choosing a theme with the
  variable set records the preference and leaves the session uncoloured, and says
  so.
- **Every mark has an ASCII form.** The separator, the tool bullet, the selection
  marker, the ellipsis, the elision, the dash, the transcript rule, the quotes,
  the credential mask and the working indicator each exist in two sets, and each
  ASCII form carries its counterpart's *meaning* rather than merely standing in
  the same column. `[app.io-cli] glyphs` names a set — `unicode` or `ascii` —
  and an absent key asks the locale: `LC_ALL`, then `LC_CTYPE`, then `LANG`, the
  first one present deciding. The set is an axis of its own, in both directions:
  `NO_COLOR` keeps the Unicode marks, and the ASCII set arrives at a fully
  coloured theme. The IO CLI wordmark is the one exception, and it is suppressed
  rather than transliterated — a wordmark redrawn in `#` is a different and worse
  image wearing its name.
- **The cursor sits where input is expected**, on every frame that accepts any:
  the composer, including at a width too narrow to draw it; the approval overlay;
  the selected row of a picker; and every step of the wizard. It is the focus
  indicator a screen reader follows, and a frame that leaves it hidden reports no
  focus at all.
- **A frame whose content did not change is not drawn.** An idle session writes
  no bytes to the terminal, so nothing announces itself twice for having merely
  repainted.

## Headless

`io exec "<goal>"` runs one goal to completion with no terminal, prints the
agent's reply on stdout, and exits with a status that says how the run ended.
It is the same session layer, the same policy, the same store and the same
events as the interactive product — a second consumer of io-harness rather than
a second program.

```sh
io exec "add a test for the parser and run it"
io exec --json "fix the failing test" | jq -r 'select(.event=="step") | .decision'
ANTHROPIC_API_KEY=… io exec --provider anthropic -m claude-sonnet-4 "tidy the imports"
```

| Flag | Does |
| --- | --- |
| `--json` | write the run's events to stdout as newline-delimited JSON instead of the reply |
| `--sandbox <mode>` | `read-only`, `workspace-write` or `full-access` — where a command this run executes may write |
| `--policy <posture>` | `workspace` or `read-only` — what the agent may attempt at all |
| `--provider <name>` | `openrouter`, `anthropic` or `openai` — take the credential and model from the environment instead of a file |

`--sandbox` and `--policy` are different axes and share the word `read-only`.
One is where the sandbox lets a command write; the other is what the policy
permits the agent to try.

**`--policy ask-writes` is refused.** Nothing in a headless run can answer an
approval, so honouring it would turn *ask before writes* into *deny writes*
without saying so. Every approval in a headless run is declined, and the
refusal is fed back to the agent as an observation it can adapt to — which is
what it already does with a policy refusal.

### Exit status

| Code | Means |
| --- | --- |
| `0` | the run ended of its own accord |
| `1` | it never got that far — no provider, a bad credential, an unreadable configuration, a usage error |
| `2` | a boundary said no: denied, refused, or a rejected plan |
| `3` | a ceiling was reached: steps, time, tokens, or the tree's shared budget |
| `4` | the run stopped needing a human: it asked a question, or proposed a plan |
| `5` | it ended without finishing: stalled, escalated, or cancelled |

A ceiling is `3` and not `0` because io-harness returns one as a *successful
call* whose outcome says a limit was hit; a status read off the result alone
would call a truncated run a finished one.

**Give the goal an end condition.** How a run ends is the agent's behaviour, not
this interface's: a goal with no clear stopping point can keep the agent working
after the useful part is done, until io-harness's stall policy ends the run — and
that is `5`, even though the work happened. The same goal on the same model
reached `Finished` on one run and `Stalled` on another while `io exec` was being
tested. `io` reports what the harness decided and never relabels it, so
"…, then stop" in the goal, or a `max_steps` in `[run]`, is worth more than
retrying.

### The JSON

One object per line, and nothing else on stdout, so it pipes straight into a
reader. The objects are `io_harness::RunEvent` serialized by io-harness's own
derive — the same shape its `[[hook]]` writer appends to a file and its store
keeps in `run_events.json`:

```json
{"run_id":41,"step":2,"depth":0,"event":"step","decision":"wrote src/lib.rs","tool_call":"write_file","tokens":812,"changed":true}
```

The variant's fields sit beside the envelope's rather than under a key of their
own. Because io-cli forwards the harness's type rather than modelling one of its
own, every event kind reaches the stream — including the thirty-nine the
interactive renderer has no way to draw. **There is no timestamp**: `RunEvent`
does not carry one, and inventing an envelope to add one would make this a
format io-cli owns rather than one it passes through.

Progress, warnings and the closing summary go to stderr, so redirecting it
leaves the data alone.

## Configuration

io-cli has no configuration parser. io-harness owns discovery and layering, and
io-cli's own settings live in the `[app.io-cli]` section that io-harness
deliberately does not validate. See [`docs/config.example.toml`](docs/config.example.toml).

Four keys live there, and one table:

| Key | Is |
| --- | --- |
| `theme` | `dark` or `light`. Absent detects from the terminal background. |
| `diff` | `unified` — the default, and what an absent key means — or `minimal`, the changed lines and the `@@` header without the context, for reviewing by file rather than by hunk. |
| `glyphs` | `unicode` or `ascii`. Absent asks the locale. |
| `plain` | `true` runs every session in plain mode. The same switch as `--plain`, which wins over it. |
| `[app.io-cli.keys]` | the session's keys, by action name. See [Moving a key](#moving-a-key). |
| `[app.io-cli.containment]` | the caps a fan-out runs under. Absent, a session cannot decompose anything. See [The fleet](#the-fleet). |

Because the section is unvalidated by design, an unrecognised *value* reads as the
default rather than stopping a session from starting. A section io-harness cannot
parse **at all** is a different case and is no longer silent: through 0.5.0 that
reverted the theme, the diff style and everything else in the section at once with
nothing said about it, and the session now starts on the defaults carrying
io-harness's own message — which names the key that broke — in its scrollback.

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
a permission boundary on disk is the opposite of what that key is for. **An interactive session** does **not** yet apply `[sandbox]` limits, `[run]`
budgets, `[[mcp]]`, `[[lsp]]`, `[[agent]]` or an `AGENTS.md` instruction file to
a turn. The reason is specific rather than an oversight: io-harness's steerable
turn builds its own task contract, and the entry point that takes a caller's
contract does not take a steer inbox — so honouring those sections in a session
would mean giving up `Ctrl+C`. The sandbox itself **is** on: a workspace turn
runs commands inside it, with no resource ceilings.

A **contained** turn does not apply them either, and for a neighbouring reason:
the entry point that fans out builds the session's own default contract, so
`[run]`, `[sandbox]` and `[[agent]]` are as absent there as in a steered turn.
What bounds a contained turn is the containment table's own token ceiling.

**`io exec` does apply them**, and that is not a second implementation — it is
the same boundary reached from the other side. A headless run has nobody to
steer it, so it can hand the harness a contract of its own, which is what
`[sandbox]` and `[run]` travel in. So a `max_steps` or a `max_wall_secs` you set
today has an effect in CI and none in your terminal, and that asymmetry is a
property of the harness's entry points rather than a decision made here.

`NO_COLOR` is read from the environment rather than from this file, and so is the
locale behind `glyphs`. See [Reading it without seeing
it](#reading-it-without-seeing-it).

## What this release is not

0.9.0 is the release where the session gains sight: you can show the agent a
picture, it can look at one itself, and both land in your scrollback. Background
work that outlives a step is named and counted rather than looking like a hang.

**iTerm2's own inline-image protocol is not here.** Its escape has no equivalent
of the Kitty flag that says "place this without moving the cursor", so it
advances the cursor and may scroll — and a scroll changes what every later cursor
move in the same draw means. It very likely lines up against a region of exactly
the right height, but "very likely" is not good enough when the failure is
written permanently into your scrollback. iTerm2 gets half blocks, which are a
picture. Sixel is absent because encoding it means palette quantisation and
another dependency, for terminals that either speak Kitty too or draw half blocks
correctly. And the real-image path covers PNG rather than all four wire formats,
because Kitty's own transfer format is PNG and the only base64 this program has
is the one io-harness already computed — a screenshot is a PNG everywhere that
takes one.

**A text-only model plus an image is a failed run, and it fails at the wire.**
Whether a provider takes image input is asked before an attachment is accepted,
but that is a question about the *provider* — with OpenRouter in front of four
hundred models, the answer is yes while the particular model you have chosen may
still be text-only. What you get then is the provider's own refusal, mid-run:

```
error: provider error (Request, HTTP 404): {"error":{"message":"No endpoints
found that support image input","code":404}}
```

The step and its tokens are already spent when it arrives. This also reaches you
without attaching anything, because enabling images gave the agent `view_image`
and the agent may decide to use it on a model that cannot see — and io-cli cannot
take a tool out of io-harness's own workspace tool set. Checking the model rather
than the provider would mean reading the live catalogue on every attach, and it
would still not close the door the agent opens. **If you work with images, choose
a model that accepts them.**

**An image the agent was *given* rather than asked for is not shown.** A picture
returned by an MCP tool, and a browser screenshot, both become images inside
io-harness — but through private plumbing and with no event of any kind, so
nothing reaches this program to draw.

**Harness skills are not in the palette, and prompt templates are.** The
difference is not effort. A template is expanded by io-cli into prompt text, so
nothing but this program is involved. A skill is read by the *model*, through a
tool, and whether it may be is decided by a task contract — and io-harness has no
session entry point that takes a contract from the caller alongside the steer
inbox that carries `Ctrl+C`. So an interface can offer skills or it can offer
interruption, and this one keeps interruption. The same boundary is why the agent
asking what you meant still cannot be answered in the session, and why a proposed
plan is not shown: all three wait on the same change to io-harness.

**`io exec` runs one goal and stops, and a run that pauses stays paused.** An
agent that asks a question about what you meant, or proposes a plan, ends the run
at exit `4` with the question persisted in the store. That is io-harness's
behaviour and it is the right one — a machine answering a question about intent
on your behalf sends the agent down a path nobody chose — but there is no `io
resume` in this release to answer it and carry on, so the run is parked rather
than lost. The closing line names its id. Approvals are the one pause that cannot
happen, because they are declined rather than deferred.

There are no `--max-steps`, `--timeout` or `--max-tokens` flags either: `[run]`
in the configuration file expresses all three, and a CI job's limits belong with
the project rather than in every invocation.

**A rewind does not check whether you edited a file yourself since the turn.** It
puts each file back to the state before that turn first wrote it, and it does not
compare that against what is on disk now — so a hand edit made afterwards is
overwritten. `io` cannot detect this, because the snapshot it restores from is not
readable from outside io-harness; what it does instead is tell you, in the prompt,
before the second keystroke. This is what `git checkout -- <path>` does too, and
it is said here rather than left to be discovered. Making it preventable is a
change to io-harness, not to this interface.

**A rewind undoes one turn**, the last one. **`/resume` lists every session the
walk found** — the twenty-row cap is gone, because it existed only to keep a list
short that nobody could filter. One bound is left, on how many runs the walk will
look at, and the list still says so when it has cut rather than quietly showing
you a subset.

Three more things are absent for reasons worth stating rather than hiding. **Spend
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
