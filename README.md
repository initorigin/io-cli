<div align="center">

# IO CLI

**A terminal agent that shows you what it is allowed to do, what it is spending,
and what it refused — while it works.**

[![CI](https://github.com/initorigin/io-cli/actions/workflows/ci.yml/badge.svg)](https://github.com/initorigin/io-cli/actions/workflows/ci.yml)
[![Release](https://img.shields.io/github/v/release/initorigin/io-cli)](https://github.com/initorigin/io-cli/releases)
[![MSRV](https://img.shields.io/badge/MSRV-1.95-blue)](Cargo.toml)
[![License](https://img.shields.io/badge/license-Apache--2.0-blue)](LICENSE)

</div>

`io` is an interface. The agent loop, the providers, the tools, the sandbox, the
permission boundary and the session store are all
[io-harness](https://github.com/initorigin/io-harness), and none of them are
reimplemented here. A test asserts that: `tests/dependencies.rs` fails the build
if this crate ever grows an HTTP client, a TLS stack, a database or a sandbox.

- [Install](#install) · [First run](#first-run) · [While it works](#while-it-works)
- [Keys](#keys) · [Commands](#commands) · [Configuration](#configuration)
- [The fleet](#the-fleet) · [Pictures](#pictures) · [Background jobs](#background-jobs)
- [Reading it without seeing it](#reading-it-without-seeing-it) · [Headless](#headless)
- [What this release is not](#what-this-release-is-not) · [Platform support](#platform-support) · [Stability](#stability)

![A session at rest: the IO CLI card in the terminal's own scrollback, carrying
the version, the model, the permission posture and the workspace; a prompt below
it; and a two-row footer under a rule, naming the state, the model and the clock
on one row and the keys and the posture on the next.](docs/screenshot.png)

## What you get

| | What it gives you |
| --- | --- |
| **A session you can read** | Every finished line in the terminal's own scrollback, designed rather than defaulted: a tool call as a verb and a path, a thought as a thought, an answer that ends the turn |
| **A working view** | Two sticky rows while a turn runs — a word for the turn with its clock and spend, and a line under it saying what is happening *now* |
| **The boundary, visible** | The posture on the footer, a refusal that names the act, the target, the rule and the layer, and `Shift+Tab` to change it from the next turn |
| **Approvals in place** | A write stops the run and shows the diff it proposes; `y`, `a` or `n`, answered where it was asked |
| **A fan-out you can watch** | Contained turns spawn children under one shared ceiling; `Ctrl+F` shows the tree and what it is costing |
| **Your file, in force** | Every section of `io.toml` bounds a session turn as it bounds `io exec`; the budgets in force are on the status line with what is left of them, and `/status` commits the whole state — policy layers, sandbox backend, caps, budgets, connections — into the scrollback |
| **Undo** | `Esc Esc` at an empty prompt rewinds the last turn — its files, its memory and the conversation head |
| **Conversations that survive** | `/resume` reopens an earlier session, `/fork` continues from an earlier turn, `/clear` starts fresh without leaving |
| **Headless** | `io exec` runs one goal to completion with documented exit codes and `--json` |
| **Readable without seeing it** | `--plain` animates nothing and commits every state change as text, for a screen reader, a braille display or a log |
| **Markdown, rendered** | Headings, bullets, code and emphasis drawn as themselves rather than printed as notation |

## It never takes your terminal

`io` does not enter the alternate screen and does not capture the mouse, in any
mode, behind any flag. Every finished message, tool call and system line is
committed into the terminal's own scrollback; eight rows at the bottom hold the
live row, the activity line, two rows of composer and a three-row footer, and
only those repaint.

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

## While it works

Two rows sit above the composer for exactly as long as a turn is in flight, and
neither is there before the first one or after the last one ends.

The top row is the **live row**: what is happening right now, in this order — the
run is waiting on you, or a tool call is open and this is the verb and the path,
or the model is thinking, or it is the tail of the answer as it streams. Waiting
on a person outranks everything, because every other thing that row can say is
about work going on without you. It sits directly under the transcript, which is
what it is the continuation of.

Under it, past a row of air, is the **activity line**: a word for the turn, chosen
once per step so it moves when the work does rather than on a timer of its own,
the elapsed clock, and the token count the run has been billed for. On a narrow
terminal it drops the count and then the clock, which is the rule the status line
under it already follows.

**These two swapped places in 0.13.1.** Through 0.13.0 the work was drawn under
the line describing it, so the newest words the agent had written read as a
footnote to a spinner.

**From 0.14.0 the status line also carries the ceilings actually in force**, each
beside what is left of it — `left 17/20 steps`, `left 12.4k/200.0k tok`,
`left 4m30s/10m00s` — for the step, token and duration budgets your `[run]` table
sets. A budget you did not set draws no field at all, so a session that
configured nothing looks exactly as it did: io-cli's own step floor is
scaffolding rather than a number you chose, and reporting it back to you as a
budget would be noise on every line. They appear once a turn has been built,
because the contract is the one place the order of precedence is already
resolved, and they are read off it rather than composed a second time from the
file. **A turn that ends on a budget now says which budget**, in the vocabulary
of a ceiling reached rather than of an error — `step_cap_reached`,
`time_budget_exceeded` and `cost_budget_exceeded` were reported through the error
path until this release, so what an operator met under a half-finished answer was
`error: step_cap_reached`. All four are successful calls in io-harness and always
were. The word stays the harness's; what changed is the weight it is said in.

What lands in the scrollback is designed rather than defaulted. A tool call reads
as a verb and a workspace-relative path — `Read src/lib.rs`, not `read_file` and
an absolute path — and a tool this release has never seen keeps the name
io-harness sent, because a verb invented for it would mean nothing. A turn ends
on its answer: the run's step and token counts are on the status line beside the
provider, which is where every other number in this interface lives.

**Three more things reach the transcript in 0.14.0**, all of them facts
io-harness has been emitting into every ordinary session and this interface has
been discarding:

- **Every outbound connection a contained command dialled**, with the host as the
  command asked for it, the port, and whether the policy permitted it — `dialled
  api.github.com:443 · permitted`, and a refusal drawn as a refusal rather than as
  an error, because nothing broke when a boundary worked. The host is never a
  resolved address: the policy's patterns are written against names, so a row
  showing `140.82.121.4` would not match the rule that decided it. **An absent
  dial line is not evidence of no egress** — the event has one emit site behind
  three conditions, and a permissive or all-or-nothing policy names no host and
  emits none of these ever.
- **Each sandbox created, capped or destroyed**, with the backend that isolated
  it where the event carries one — io-harness sets it on creation and on a command
  and not on the other two, and nothing here invents one. A cap reached is drawn
  as a limit reached and not as a failure: the sandbox did exactly what its
  configuration told it to.
- **A stalled agent, while it is stalling**, naming the step it stopped on and how
  long it has been there. This needed no configuration to fire — a workspace turn
  carries io-harness's default stall policy — and until now it reached you as a
  session that had gone quiet, and then, once the run was over, as the word
  `stalled` on the outcome line.

## The agent's manner

From 0.13.0 every turn carries a system prompt `io` wrote. Before it, every turn
ran io-harness's built-in description, which names the tools and says nothing
about how to answer — so an ordinary question came back from a model with a tool
catalogue and no idea what it was.

What the prompt sets is small and deliberate: what `io` is, that the person
reading is at a terminal in a pane a few rows tall, that the answer comes first
and briefly, that work is reported in the past tense once it is done rather than
narrated in advance, and that the output is monospaced text about eighty columns
wide — fenced code, no wide tables, no markup that expects a browser.

**It is appended, not substituted.** io-harness composes it into the prompt it
was already building, between its own tool catalogue and the boundary section, so
the harness keeps its framing, its catalogue, the sentence that decides how a turn
ends, and everything it says about what this run may do. `io` adds a manner; it
does not stand in for the description of the request.

It names no model and no vendor, because the model is yours to choose and there
are hundreds of them. It claims no tool, no skill and no permission, because what
the agent may reach is decided by the contract this turn was given — a prompt
that promised a browser would be lying on every session that has not configured
one.

**Per-repository voice belongs in the repository.** io-harness discovers what
`[instructions]` points at — `AGENTS.md` by default — and composes it into the
same prompt as a clearly attributed section. That is where "in this codebase, do
it this way" goes. There is no `[app.io-cli]` key for the prompt itself: a second
place the agent's manner is decided is a second thing to keep true.

## Keys

<!-- keys:start -->

| Key | Does |
| --- | --- |
| `Enter` | send the prompt |
| `Shift+Enter` | new line — or `Alt+Enter`, `Ctrl+J`, or end the line with \ |
| paste again | the same block again: shows it, then collapses it back |
| `Up / Down` | walk prompt history |
| `Ctrl+C` | stop the turn; again to stop it now; twice at an empty prompt, exit |
| `Ctrl+D` | exit, on an empty prompt |
| `Shift+Tab` | cycle the permission posture, from the next turn |
| `Ctrl+L` | clear the viewport, never the scrollback |
| `Esc Esc` | at an empty prompt, undo the last turn — its files and all |
| `Ctrl+T` | put the whole conversation back into the scrollback |
| `Ctrl+F` | show the fleet: the children this turn has spawned |
| `y / a / n` | answer an approval: allow once, allow this session, deny |
| `Esc` | stop the running turn, or close a picker without choosing |
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

**And from 0.13.0 `io` tells you which one that is.** The table above is the
shipped naming, and a README is read on a machine other than the one it
describes — so `/help` and the wizard's closing screen name the key *this*
terminal can report. On one that cannot report `Shift+Enter` they name
`Alt+Enter` and the trailing backslash, and say the key is unreportable here
rather than leaving you to press it and watch a half-written prompt go to the
model. Nothing about the composer changed: `Shift+Enter`, `Alt+Enter`, `Ctrl+J`
and a trailing `\` all still work wherever the terminal can distinguish them.

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

Grouped by what you are doing rather than by which part of the harness answers.
The `/` palette shows the same groups while you browse it and drops them the
moment you type, because a ranked list with headings interleaved puts a heading
above a row that ranked there for reasons having nothing to do with it.

**the session**

| Command | Does |
| --- | --- |
| `/clear` | start a new conversation; this one stays in /resume |
| `/resume` | reopen an earlier session where it stopped |
| `/fork` | continue from an earlier turn of this conversation |
| `/setup` | run the first-run wizard again |
| `/exit` | leave |

**this turn**

| Command | Does |
| --- | --- |
| `/model` | change the model the next turn is sent to |
| `/contain` | run turns contained, so the agent can fan out: on, off, or ask |
| `/plan` | make turns propose a plan before they work: on, off, or ask |
| `/profile` | switch to a named profile from the configuration, for this session |

**inspect**

| Command | Does |
| --- | --- |
| `/help` | this table |
| `/status` | commit the whole session state into the scrollback |
| `/expand` | commit the last step's full detail into the scrollback |
| `/fleet` | show the children this turn has spawned |
| `/mcp` | the MCP servers configured, and what this session has seen of each |
| `/provider` | the providers configured, in the order a turn tries them |
| `/image` | draw an attached image again: /image 1 |
| `/copy` | put the last answer on the system clipboard |
| `/copy diff` | put the whole run's patch on the system clipboard |

**configure**

| Command | Does |
| --- | --- |
| `/config` | every setting, the value in force and the file that decided it |
| `/theme` | change the theme for this session |

`/usage` answers what `/status` answers and is deliberately not listed above: an
alias earns no row of its own, because a second row for one screen reads as a
second screen.

In the palette each row carries a mark saying what it is — `:` runs a command,
`+` fills the prompt from a configured template, `*` names one of the agent's own
skills. The mark is beside the name rather than in the description, because the
description is the first thing dropped on a narrow terminal and the kind is what
you most need there.

<!-- commands:end -->

**Everything that shows you more of something writes it into the terminal's own
scrollback.** `Ctrl+T`, `/expand` and `/status` do not open a pane: the viewport is a few
rows and this product has no alternate screen, so the place to read something
long is the buffer where the terminal's search, selection and tmux copy-mode
already work. `/expand` reads the step's full output back out of the run's
durable trace, which is where it went in the first place — the screen is not the
archive.

`/expand` also holds the part of a long thought that did not fit. The model's
reasoning is committed as a thought — the word, how long the step had been going,
then the text — and a thought longer than ten rows is fitted with the rest kept
for `/expand`. io-harness neither stores reasoning nor folds it into the next
prompt, so that copy is the only one there is.

`/status` commits the whole session state, one fact per row: the workspace and
the session id with the turn its head is at, the provider and model, every policy
layer by name with the acts it governs, the containment caps and what has been
drawn against them, **the sandbox mode asked for beside the backend that actually
answered on this host**, every budget with what is left of it, how full the
context is, and what is connected — MCP servers and language servers as *answered
of configured*, the browser, the skills directory. Every field on it is a value
io-harness supplied; nothing is io-cli's account of it. It is not a table, because
a table has a column width and the widest cell here is a workspace path: a row too
long for the terminal is folded and never cut, so eighty columns is a supported
size rather than a degraded one. It reads the state and changes none of it — no
plan gate is registered to build the contract it reports on, because registering
one would turn the planning phase on.

`/clear` starts a new conversation: a new session id, no prior turn sent to the
model, and the run-scoped status fields back to zero. It clears the screen and
nothing else — the conversation it ends is still in the store and still listed by
`/resume`, and your terminal's scrollback is still your terminal's. It is refused
while a turn is running.

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

**And from 0.12.0 that is all it costs you.** `Ctrl+C` still ends the turn, at the
next point where no child is in flight, and the interface tells you that is what
it is waiting for rather than appearing to have missed the key. `/contain off`
gives the next turn back; `/contain on` takes it again.

Through 0.11.0 this switch carried more than the fan-out. io-harness's contained
entry point was then the only session entry point that took a task contract, so
turning containment on was also how you got skills, MCP servers, language servers,
a browser, an answer to the agent's questions and a plan gate — and turning it off
took all of them away. 0.11.0 gave the ordinary turn a contract too. Every one of
those capabilities is on every turn now, and containment means what its name says.

## Planning

`/plan on` makes the next turn propose a plan before it does anything. While the
planning phase is on, io-harness denies every write and every command until you
approve, so reading a proposal costs nothing and cancelling is not an undo —
there is nothing to undo yet. `Enter` on an empty prompt approves, typing a
correction sends it back, `Esc` cancels. The status line says `planning` for as
long as the phase is on, because it outlives the turn you set it on.

`/plan off` gives you back a turn that starts working immediately, and that is the
default. Bare `/plan` says which one you are in and changes nothing.

**This moved in 0.12.0.** Through 0.11.0 the plan gate rode
`[app.io-cli.containment]`, so configuring a fan-out silently made every turn stop
and propose first. If that is what you wanted, `/plan on` is where it lives now.

## Pictures

**Drag a picture onto the prompt, or copy it and paste.** That is the whole of
it — there is no command. What lands is `[Image #1]`, and the picture rides the
**next turn and only the next turn**. Paste the same file again to toggle between
the marker and the path it stands for; backspace takes the marker off in one
press, whichever backspace you use. `/image 1` draws the picture itself, at the
bottom, when you want to look at it — a committed row belongs to your terminal's
scrollback, so it cannot be opened in place.

**`/attach` was removed in 0.13.1**, alias and all. It was a command you had to
be told about before you could use the feature, and dropping a picture into the
window is what everyone already does. Typing it is answered the way any other
word that is not a command is.

A path **inside the workspace** is read through io-harness's own workspace, under
the same policy as everything else — its documentation is explicit that this is
the same gate a source read passes and not a second one — so an image the session
may not read is refused exactly the way a file it may not read already is.

A path **outside the workspace** is read directly, and that is deliberate: the
file you point at is almost never inside the repository, and every absolute path
was refused before — which made this unusable for the one thing most people
attach. This is the only read in the product that is not the agent's, and it is
the boundary `!` already crosses when it runs your own shell line. What may be
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
kitty, ghostty, WezTerm and Konsole a PNG is drawn as the **real image** instead,
and on iTerm2 so is a png, jpeg or gif — it decodes the file itself, so it is not
limited to the one format Kitty's transfer takes. Inside tmux or screen it is
always half blocks: passing a graphics protocol through a multiplexer needs
configuration that is off by default, and an escape the terminal cannot read is
unreadable bytes written permanently into your scrollback.

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

**A headless run takes io-cli's own step floor of a thousand from 0.14.0**, where
it used to take io-harness's twelve. Twelve steps is not a turn — a run that reads
a repository and writes a file spends them easily — so what an unattended job
produced was `error: step_cap_reached` over half-finished work with nobody
watching, which is the defect the floor exists to fix and is not made better by
the run being unattended. A `[run] max_steps` you wrote still beats the floor, in
either direction.

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

### Without leaving the session

**`/config` shows every key with the value in force and the file that decided
it** — `user`, `project`, `local`, or `default` where no file decided it. A key
no file named names no file rather than being blamed on the lowest-precedence
one: io-harness reports an empty origin for it, and that is its own default
speaking.

Choosing a row puts its key in the prompt. `/config <key> <value>` asks which of
the three files to write to, and only that choice writes. The change is in force
from the next turn.

**Your file survives it.** The comments, the blank lines, the order you chose and
every section io-cli has no type for come back byte for byte — one value's bytes
are replaced and the rest is copied through. The write is staged in a temporary
file and renamed over the original, so a failure cannot truncate a configuration,
and the mode is preserved.

**A project-scoped change that would widen the boundary is refused in
io-harness's own words**, and the same value is accepted in `io.local.toml` —
the rule is about which file, not which value. io-cli keeps no copy of those
rules: it writes, asks io-harness to read the file back, and restores it exactly
when the answer is no.

**`/mcp`** shows what is configured, which servers answered this session, how
many distinct tools each answered, and the last failure. A server the session has
not reached says so and is not shown as broken.

**`/provider`** shows the `[[provider]]` array as what it is: the order a turn
tries them. Reorder it and you have arranged the fallback chain io-harness has
supported since its 0.27.0. The twenty-one presets it reaches through one
`Compatible` provider are offered by name with the endpoint each resolves to.

**`/profile`** switches to a named `[profile.<name>]` for the session, and
`--profile <name>` picks one for a single run without writing anything.

Eight keys live there, and five tables:

| Key | Is |
| --- | --- |
| `theme` | `dark` or `light`. Absent detects from the terminal background. |
| `diff` | `unified` — the default, and what an absent key means — or `minimal`, the changed lines and the `@@` header without the context, for reviewing by file rather than by hunk. |
| `glyphs` | `unicode` or `ascii`. Absent asks the locale. |
| `plain` | `true` runs every session in plain mode. The same switch as `--plain`, which wins over it. |
| `skills` | a directory of skills for the agent. They appear in the `/` palette by name, and the agent reads them itself. Absent, it is `~/.io-cli/skills`. A leading `~` is your home directory — io-cli expands it before io-harness sees the path, because io-harness substitutes `${env:…}` and `${file:…}` and nothing else. |
| `max_parallel_reads` | how many read-only tool calls one turn may run at once. Absent, it is io-harness's own 10; `0` is clamped to 1 rather than meaning none. A `TaskContract` field with no io-harness configuration key of its own, which is why it is named here. |
| `spawn_background_after_secs` | how long a spawned child may run before it is backgrounded. Absent, a child is waited for however long it takes. |
| `detached_spawns` | whether a spawn may detach at all. Absent, it may. `false` buys a trace with every child's whole life in it, which a detached child gives up. |
| `[app.io-cli.keys]` | the session's keys, by action name. See [Moving a key](#moving-a-key). |
| `[app.io-cli.containment]` | the caps a fan-out runs under. Absent, a session cannot decompose anything. See [The fleet](#the-fleet). |
| `[[app.io-cli.mcp]]` | MCP servers for the turn, in io-harness's own shape. Merged with the top-level `[[mcp]]`, and wins a collision of ids. |
| `[[app.io-cli.lsp]]` | language servers for this workspace. Merged with the top-level `[[lsp]]`, and wins a collision of ids. |
| `[app.io-cli.browser]` | a browser the agent may drive. Never downloaded — it is one you already have. |

Because the section is unvalidated by design, an unrecognised *value* reads as the
default rather than stopping a session from starting. A section io-harness cannot
parse **at all** is a different case and is no longer silent: through 0.5.0 that
reverted the theme, the diff style and everything else in the section at once with
nothing said about it, and the session now starts on the defaults carrying
io-harness's own message — which names the key that broke — in its scrollback.

### Where io keeps your things

**One directory: `~/.io-cli`, or `%USERPROFILE%\.io-cli` on Windows.** The
configuration file is in it, and so is the run store `runs.db` with the `-wal`
and `-shm` SQLite keeps beside it — which is where the agent's durable memory
lives too, because that is rows inside the store rather than a file of its own —
and the skills directory, which is `~/.io-cli/skills` when `skills` names none.
That is one directory to copy to a new machine, and one path to put in a bug
report.

The file is found in this order, which is io-harness's and is unchanged:
`$IO_CONFIG`, else `$IO_CONFIG_HOME/io.toml`, else `$XDG_CONFIG_HOME/io/io.toml`
or `~/.config/io/io.toml`, and `%APPDATA%\io\io.toml` on Windows. What 0.15.0
changed is that io-cli sets `IO_CONFIG_HOME` to its own home before io-harness
resolves anything, so the second rung is the one that answers when you have named
no location yourself. Set `IO_CONFIG` or `IO_CONFIG_HOME` and io-cli sets nothing
and moves nothing — the location is yours. A project's own `io.toml` and a
gitignored `io.local.toml` layer on top of whichever file was found.

io-cli sets `IO_CONFIG_HOME` in its own process environment, which every child a
session starts inherits: a `!` shell line, a spawned agent, a nested `io`. For a
nested `io` that is the answer you want, since it reads the same home as the
session that started it. For anything else it is one more variable in the
environment that nothing reads.

**On the first 0.15.0 run an existing install is moved into the home** — the
configuration file and the store together, each file named on screen as it moves.
Nothing is deleted, and nothing is overwritten: where the home already holds a
file of that name, both are left where they are and the session says which one is
in force. To keep the location you have, set `IO_CONFIG_HOME` to it before the
first 0.15.0 run.

One thing worth knowing: a **project** file may narrow the permission boundary
and may never widen it, because a repository you cloned must not be able to grant
itself permission. The wizard therefore writes the user-scope file, which is
where widening is your own decision.

The policy's own defaults are what `Shift+Tab` cycles; a posture chosen with the
key lasts for the session and is not written back, because a keystroke that
rewrites a permission boundary on disk is the opposite of what that key is for.

**The whole file reaches a session turn from 0.14.0, and it reaches `io exec`
from the same call.** `[sandbox]` limits, `[run]` budgets, `[run.commit_identity]`,
`[[agent]]`, `[web]`, `[memory]`, `[instructions]`, `[[mcp]]`, `[[lsp]]` and
`[browser]` are all applied to a turn's contract, in your terminal exactly as in
CI. There is no longer a section of this file that a session reads past.

The layers run weakest to strongest, and that order is asserted rather than
described: io-harness's own defaults, then io-cli's step floor, then everything
io-harness's own sections say, then `[sandbox]`, then `[app.io-cli]`. So a
`[run] max_steps` you actually wrote beats io-cli's floor — a file that *lowers*
the cap is honoured, not only one that raises it — and an `[app.io-cli]` server
of the same id beats a top-level one. The two server lists are merged rather than
replaced, and the session names any id it dropped.

**Nothing rides `[app.io-cli.containment]` but the fan-out.** Through 0.11.0 the
contained turn was the only session entry point io-harness let a caller hand a
task contract to, so the responder, the plan gate, MCP servers, language servers,
the browser and the skills directory all arrived on that one switch. 0.11.0 gave
the ordinary turn a contract too. Every one of those has been on every turn since,
contained or not, and no session turn takes a steer inbox — so containment costs
no steering and grants no capability. It is the caps a fan-out runs under and
nothing else.

**What changes for a file you already have.** No key is added, removed or
renamed, and a 0.13.1 file is a valid 0.14.0 file. What changes is what it does:

- **A `[run]` block written for `io exec` now bounds your terminal.**
  `max_steps = 20` is a reasonable thing to have set for an unattended CI run and
  an unreasonable cap on a conversation. The status line carries each budget in
  force with what is left of it and `/status` lists them all, so a turn that will
  stop at a ceiling says which one before it gets there. If you want `[run]` for
  CI only, move it to a project file or narrow it by scope.
- **`io exec` now takes io-cli's own step floor of a thousand** instead of
  io-harness's twelve. A headless run used to end `error: step_cap_reached` under
  half-finished work with nobody watching. A `[run] max_steps` in the file still
  beats the floor.

**And `[web]` is a capability, not a preference.** Reaching a session turn, it
gives the model the provider's own search and fetch — and it is the *vendor* that
dials the URL, so the `net` rule in your policy is not what governs it. That rule
decides what this machine may reach. A `[web]` table that did nothing in your
terminal yesterday turns something on in it today, which is why the session says
so at start in its own words rather than folding it into a list.

**`[browser]` is refused in a project-scoped file**, by io-harness rather than by
io-cli: it names a program to execute, and a project's `io.toml` arrives with a
`git clone`. Write it in the user-scope file — the one `io setup` writes — where
widening the boundary is your own decision. There is no project-scope route to a
browser at all. io-cli's own `[app.io-cli.browser]` is read from either scope.

`NO_COLOR` is read from the environment rather than from this file, and so is the
locale behind `glyphs`. See [Reading it without seeing
it](#reading-it-without-seeing-it).

## What this release is not

0.14.0 is the release where the configuration file reaches your terminal: every
section of it bounds a session turn as it already bounded `io exec`, `/status`
commits the whole picture into the scrollback, and the ceilings in force are on
the status line beside what has been drawn against them.

**Four sections of the file are still not applied, and each has a reason.**
`[[hook]]` and capability bundles reach a contract through their own builders and
need a surface that reports what loaded and what was dropped; `[prices]` is not
part of a contract at all and belongs with the release that reads the
provider-call rows; there is no `[verify]` section to apply, and giving a session
verification gates needs its own surface; and `run.templates` is the thirteenth
`[run]` key, reachable only through its own accessor. None of them is a silent
omission any more — this is where they are named.

**There is no way to change a key from inside the session.** `/status` reads the
state and never writes it, and editing configuration is 0.16.0 — a surface for
changing a key is worth building once changing the key does something, which is
what this release is.

**Sixel is still absent**, because encoding it means palette quantisation and
another dependency, for terminals that either speak one of the two protocols
already here or draw half blocks correctly. The Kitty path covers PNG rather than
all four wire formats,
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

**A skill is listed by name and never pasted.** A template is expanded by io-cli
into prompt text, so nothing but this program is involved; a skill is read by the
*model*, through a tool, under the run's own policy. Choosing one from the
palette puts `use the <name> skill: ` in your prompt and stops there. io-cli
parses no skill file and keeps no copy of one.

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

Two more things are absent for reasons worth stating rather than hiding. **A diff
cannot be expanded beyond
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

## Platform support

| Platform | Build | Containment |
| --- | --- | --- |
| macOS, Apple silicon | `aarch64-apple-darwin` | io-harness's own: `sandbox-exec` |
| macOS, Intel | `x86_64-apple-darwin` | as above |
| Linux | `x86_64-unknown-linux-musl`, statically linked | io-harness's chain: Landlock, `bwrap`, namespaces, floor |
| Windows | `x86_64-pc-windows-msvc` | Job Object, with AppContainer opt-in |

The four artifacts and their `SHA256SUMS` are attached to every GitHub Release,
and the full test suite runs on Ubuntu, macOS and Windows in CI. **What confines
a command is io-harness's, not this product's** — `io` shows you which backend
actually answered on this host, in the footer, because the mode asked for and the
backend that applied are not the same fact.

Rust 1.95 or later to build from source. There is no crates.io publish: the
distribution channel is the GitHub Release, and `publish = false` makes an
accidental one impossible rather than merely discouraged.

## Stability

Pre-1.0 and staying there until the owner says otherwise. A minor release may
change what a session looks like — 0.11.0 rewrote the transcript's vocabulary,
and the release before it moved where a question is answered. What you can rely
on is that every one of those is in [CHANGELOG.md](CHANGELOG.md), said plainly,
and that a configuration file written for an older release keeps working. **One
key has been removed in the product's life so far**: `[app.io-cli] max_steps`,
deprecated in 0.14.0 and removed in 0.16.0, with two releases' notice given in
the terminal, the README and the changelog. A file that still carries it loads
exactly as before — the key is ignored rather than rejected — and the session
says so once at startup, naming the number that is no longer in force and
`[run] max_steps` as where the cap lives now. **A section that was ignored may
start being read**, which 0.14.0 did to eleven of them, and that is a behaviour
change for a file that already carried one; it is the migration note in
[Configuration](#configuration) and in the changelog rather than something to
find out from a turn.

## Security

Report vulnerabilities per [SECURITY.md](SECURITY.md). Your provider key is never
printed, never committed to the scrollback, and never written to disk by the
wizard when the provider's own environment variable is already set.

## Contributing

See [CONTRIBUTING.md](CONTRIBUTING.md). `develop` is the working branch; nothing
reaches it or `main` except through a pull request.

## Licence

Apache-2.0. Copyright 2026 Aakash Pawar (InitOrigin). See [LICENSE](LICENSE) and
[NOTICE](NOTICE).
