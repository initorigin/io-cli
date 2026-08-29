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

- [Install](#install) · [First run](#first-run) · [Bringing your setup across](#bringing-your-setup-across) · [While it works](#while-it-works)
- [Keys](#keys) · [Commands](#commands) · [What it costs](#what-it-costs) · [Configuration](#configuration)
- [Capability bundles](#capability-bundles) · [Marketplaces](#marketplaces) · [Hooks](#hooks) · [The fleet](#the-fleet)
- [Pictures](#pictures) · [Documents](#documents) · [Background jobs](#background-jobs)
- [Reading it without seeing it](#reading-it-without-seeing-it) · [Headless](#headless)
- [What this release is not](#what-this-release-is-not) · [Platform support](#platform-support) · [Stability](#stability)

![A session at rest: the IO CLI card in the terminal's own scrollback, carrying
the version and the tagline above the model, the permission posture and the
workspace; a muted line saying this is a new conversation and the last one is
still in /resume; an empty prompt below it; and a two-row footer under a rule,
naming the state, the model and the clock on one row and the keys and the
posture on the next.](docs/screenshot.png)

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
| **Conversations that survive** | `/resume` reopens an earlier session and answers whatever its last run stopped on, `/fork` continues from an earlier turn, `/clear` starts fresh without leaving |
| **A paused run, answered** | A question, a plan or an interrupted call is decided where it was left and the run carries on from the step it stopped at — from a session, or from a script with `io resume` |
| **Headless** | `io exec` runs one goal to completion with documented exit codes and `--json`, and `io resume` carries a parked one on |
| **Readable without seeing it** | `--plain` animates nothing and commits every state change as text, for a screen reader, a braille display or a log |
| **Markdown, rendered** | Headings, bullets, code and emphasis drawn as themselves rather than printed as notation |
| **Documents, read and written** | Spreadsheets, Word, slide decks, PDFs and barcodes through io-harness's own tools — twelve of them, six of which write, every one under the same gate as any other read or write |
| **Your setup, brought across** | `/import` finds the agent tools already on this machine and offers their instructions, MCP servers, skills and model, item by item, with the whole plan shown before a byte is written and no credential read at any point |
| **What it spent, and whether it worked** | `/cost` commits the money and the token split — this run, this session, by model, by day — and `/stats` commits the outcomes, the first-try counts, the gate failures and the latencies. Nothing is estimated: an unpriced model makes a total a floor and the page says so |

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

## Bringing your setup across

**You have almost certainly used another agent tool first, and `/import` brings
what you told it.** Four things carry over: the standing instructions you wrote,
the MCP servers you configured, the skills you collected, and the model you
settled on. Nothing else, and nothing without you saying so.

It is offered **once**, on a first run, and io records that it asked so it never
asks again. One key declines and the session carries straight on — declining
writes nothing at all, and there is no reminder later. `/import` opens the same
thing at any time.

**What it looks at:**

| Where | What is read |
| --- | --- |
| `~/.claude/` | `CLAUDE.md`, `settings.json`, and the skills under `skills/` and `plugins/` |
| `~/.claude.json` | the MCP servers, which live here and not beside `settings.json` |
| `~/.codex/` | `AGENTS.md`, `memories/MEMORY.md`, `config.toml`, `rules/default.rules` |
| `~/.gemini/` | `GEMINI.md` and `antigravity/mcp_config.json` |
| the repository | `.cursorrules` or `CONVENTIONS.md`, if either is there |

A tool that is not installed simply is not offered. A tool whose files are all
**empty** is a different row and says so — on a good many machines all three
Gemini files exist and every one is zero bytes, and an import of nothing that
then reports success is the failure you cannot see.

**Where it writes:** instructions are appended to the memory file for the scope
you pick — one block per source file, with a line of provenance above the
original text, kept whole rather than shredded into a bullet per line. MCP
servers become `[[mcp]]` entries in io-harness's own spelling. Skills become
directories under `~/.io-cli/skills`. The model is *carried*, not written: a
`[[provider]]` entry needs a vendor and a foreign tool's model string does not
name one — `gpt-5` could be OpenAI or any of the twenty-one presets pointed at a
compatible endpoint — so io hands you the id and the entry is built once you have
chosen the vendor.

**Where a file *is* decides what it is, ahead of what it is called.** A loose
`CONVENTIONS.md` — or `CLAUDE.md`, or `MEMORY.md` — sitting inside a `skills/` or
`plugins/` directory is a skill, and from 0.22.0 it is imported as one. It used to
match on its basename and be appended whole into the instructions file that is
loaded on **every turn, forever**, instead of being a named skill the model reads
on demand.

**The whole plan is on screen before a single byte is written.** One row per
thing found, saying where it came from and where it would go, and you accept them
item by item. What you did not accept is not written. A cancelled import is not a
partial one.

**No credential is ever read or copied, and that is enforced by the code rather
than promised by it.** `~/.codex/auth.json` is not in the list of files this
program can open, so no path through it reaches one. A server's `env` values are
parsed and thrown away without ever being assembled into a string — only the
variable *name* is ever held — and what gets written is `${env:NAME}`, the name
pointing at itself, which io-harness resolves out of your own environment at the
moment a run needs it. Your shell has to have those variables set, and the import
says which. `~/.claude.json` is an entire application's state with OAuth material
in it and is read through narrow structs, so every field io does not name is
skipped by the parser instead of being loaded and then politely ignored.

**An allowlist is read, shown, and deliberately not translated.** Codex's
`prefix_rule(pattern=["bun","install"], …)` and Claude's `Bash(cargo yank *)` both
match a *command line*. io-harness's `Act::Exec` matches a **binary name and
nothing else** — it has no argument matching at all. So the closest faithful
import of `bun install` is a blanket allow on `bun`, which is a wider permission
than you ever granted, written by a tool you were trusting to be careful. io says
what it found and says it cannot express it, and produces no rule, no `[policy]`
table and no policy layer. A boundary half imported is worse than one left alone.

**Two skills of one name kills a session, so an import counts before it writes.**
A name already answered to in your skills directory is refused on its own row and
the rest of the import still goes through. Going over io-harness's ceiling refuses
**every** skill instead: the harness rejects a whole directory rather than the
excess, so an operator at 63 skills who imported three more would get a session in
which every turn dies at run start with nothing visible to blame. See
[Skills](#skills).

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

**From 0.22.0 the status line also carries what the run cost**, immediately right
of the token count it is derived from, so the two read in the order they are
computed in: the tokens are what happened and the money is what they came to. It
is **absent** — not `$0` — where there is no price table, where the table prices
none of the models this run called, or where nothing has run yet. Those three
things are different and none of them means free; `/cost` is where they are told
apart. See [What it costs](#what-it-costs).

**Two things about that line were corrected in the same release, both of them
priority inversions.** The footer used to drop its whole right-hand group — the
policy layer, the containment mode, the planning phase — to keep every counter,
so a narrow terminal gave up the standing modes that explain *why nothing is
happening* in order to keep a number about what already happened; the counters
yield now. And `planning` was pushed right of the counters, so the narrow line
dropped it before the token count, inverting the rule written five lines above it
in the code. A standing mode that stops the agent writing outranks what the last
turn spent, on both rows.

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

**Naming files in `[instructions]` replaces that default rather than adding to
it.** `AGENTS.md` is the whole automatic list, so a file that says
`files = ["docs/RULES.md"]` has stopped `AGENTS.md` reaching the agent — silently,
because a named file that does not exist is skipped without complaint. To keep it,
list it. `/remember` writes the complete list for you, and `/memory` shows which
files are actually being read, which is the only way to see that a project-scope
`[instructions]` table has replaced a wider one from your own configuration:
`files` is not one of the keys io-harness appends across scopes, so the
nearest file wins outright.

**Three scopes, and the difference between them is whether it gets committed.**
`/remember` asks which one at the moment it writes: `AGENTS.md` for what the team
should share, `AGENTS.local.md` for what only this checkout should know, and
`~/.io-cli/IO.md` for what is true of every project. The first two are named
relative to the repository; the third is written as an absolute path, because
instruction names resolve against the directory the run was started in and io's
own home is not that directory. A line takes effect on the **next turn** of the
same session — io re-reads the configuration for every turn, so an edit you make
in your own editor counts too, and a file that stops parsing leaves the last good
one in force rather than ending the session.

### Answered without opening a run

**A prompt that is only a question is answered in one completion**, with no steps,
no tools, no checkpoint and no verification gate. io-harness has classified turns
that way for longer than this interface has existed — a contract carrying no
criterion turns the classification on, and the contract io-cli builds carries none
until you write a gate — so a greeting has always come back from a single call.
What io-cli never did was *say* so. Every line this product draws about a turn is
drawn from events that a turn like this does not emit, so what reached you was
silence, and a fast answer with nothing above it reads as something having gone
wrong. From 0.26.0 such a turn commits one line: `answered without opening a run —
one completion, no steps and no tools`.

**`conversational` in `[app.io-cli]` is where you overrule it.** `false` opens a
full run for every prompt; absent leaves the behaviour exactly as it is, which is
why there is nothing to write here for the ordinary case. The key earns its place
on the other side of the gate: attaching a criterion is what io-harness reads as
"this turn is not a conversation", so a repository with a gate would otherwise
have every idle question turned into an agent run that then runs the test suite
after each of its steps. io-cli sets classification back on wherever a criterion
was attached, and this key is how you say you wanted the runs.

### How much reasoning a turn buys

**`/effort low`, `/effort medium`, `/effort high`, `/effort off`.** The level is a
posture and not a one-shot: it holds for the turn you set it on and for every turn
after it until you change it, it sits on the status line as `effort high`, and a
bare `/effort` reports what is in force and changes nothing.

What reaches the wire is the vendor's own spelling, and io-harness owns the
translation: `reasoning_effort` on the OpenAI wire, `reasoning: { effort }` on
OpenRouter, and a converted thinking budget on Anthropic — 1024, 4096 and 16384
tokens for `low`, `medium` and `high`. io-cli names a level and nothing else,
because a per-vendor number chosen here would be a second opinion about somebody
else's request body.

**`off` is not a fourth level below `low`.** It goes back to sending no reasoning
field at all, which is what every release before 0.26.0 sent, and the line it
commits says that rather than naming a level — calling it "off" on screen would
suggest a setting between `low` and nothing.

The level is this session's and is written to no file. There is no `[app.io-cli]`
key for it: how much thinking to buy is a thing you change while you work — a
cheap question, then a hard one — and a value on disk would be one more setting
that is in force on a session you have forgotten setting it for.

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

**Four more keys exist only while something is queued**, and they are deliberately
not in the table above: that table is what the session binds all the time, and
these are borrowed by the queue surface for as long as it is open and handed back
the moment it shuts. While a prompt is waiting behind a running turn, the arrows
mark a queued line instead of walking prompt history, `Shift`+the arrows move the
marked line up and down the queue, `Enter` on an empty prompt takes the marked
line back into the composer to edit — `Enter` again puts it back where it was —
and `Esc` abandons an edit in progress, or closes the surface and gives all four
keys back. Every other key still falls through to the composer, because typing is
how the next line joins the queue. It is the same trade the fleet view makes with
the same two arrows, and `/steer` is what sends the queue into the turn rather
than waiting for it.

**Two more are borrowed by `/config` from 0.28.0**, and they are left out of the
table above for that same reason: it is what the session binds all the time, and
these are held for as long as one list is open and handed back when it shuts.
`Right` and `Left` on
a `/config` row whose setting is a boolean or a closed set of words change it to
the next value — and the value after that, and back again — writing each one and
redrawing the row from the file's own answer rather than from an account of what
was just done. No picker does anything with a horizontal arrow — they fall
through to a do-nothing arm — and the interception is scoped to this one list, so
it takes no key away from any other surface. The composer still moves its cursor
with them and an approval still moves between its answers; neither has the
keyboard while a picker is up.

**It is the arrows and not the spacebar, and that is a compromise rather than a
preference.** `Space` is the obvious key for a toggle and it is unavailable: a
picker treats every printable character as a fuzzy filter, so the space in a
two-word query is a keystroke the list has already claimed, and binding it would
change a setting in the middle of typing a search for a different one. `Left` and
`Right` are simply the keys the picker does not want. The cost is
discoverability: an arrow does not announce itself the way a spacebar would, and
nothing on the row says to press one. That is a real limit of the compromise, and
stating it beats leaving it to be found. What makes it a small one is that `Enter`
opens the same values as a list and does everything the arrows do and more — so a
key nobody finds costs a keystroke rather than a capability.

A number is deliberately not cycled. Its values are a ladder rather than a pair,
too long to step through without seeing where you are, and a held arrow would
write the file once per key repeat — so `Enter` opens it instead. A key io-cli
does not know the values of says so and asks you to press `Enter`, rather than
absorbing the keystroke and looking broken.

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
| `/resume` | reopen an earlier session and answer whatever its last run is waiting on |
| `/fork` | continue from an earlier turn of this conversation |
| `/profile` | switch to a named profile from the configuration, for this session |
| `/setup` | run the first-run wizard again |
| `/exit` | leave |

**this turn**

| Command | Does |
| --- | --- |
| `/model` | change the model the next turn is sent to |
| `/effort` | how much reasoning the next turn buys: low, medium, high, or off |
| `/undo` | put work back: `<path>` for one file, `step <n>` for one step, bare for the run |
| `/contain` | run turns contained, so the agent can fan out: on, off, or ask |
| `/plan` | make turns propose a plan before they work: on, off, or ask |
| `/steer` | send what is queued into the turn that is already running |
| `/compact` | fold this conversation into a summary, at the next step |
| `/image` | draw an attached image again: /image 1 |
| `/copy` | put the last answer on the system clipboard |
| `/copy diff` | put the whole run's patch on the system clipboard |
| `/commit` | ask the agent to describe this turn's work and commit it; allow to permit git |

**inspect**

| Command | Does |
| --- | --- |
| `/help` | this table |
| `/status` | commit the whole session state into the scrollback |
| `/context` | what is in the model's window, read from the request that carried the turn |
| `/expand` | commit the last step's full detail into the scrollback |
| `/fleet` | show the children this turn has spawned |
| `/skills` | every skill, shipped or yours: what it is for, whether it is on, and its file |
| `/cost` | commit what this run, this session and this install have spent |
| `/stats` | commit how the runs have gone: outcomes, first-try, gates, latency |
| `/store` | commit what the run store holds; `rm <id>`, `sweep <date>` and `compact` change it |
| `/export` | write this conversation as markdown, or `trace` for a run's canonical trace |

**configure**

| Command | Does |
| --- | --- |
| `/config` | every setting, the value in force and the file that decided it |
| `/theme` | change the theme for this session |
| `/remember` | remember a line of guidance, in the scope you choose |
| `/memory` | what io remembers: the instruction files, and the agent's own notes |
| `/mcp` | the MCP servers configured, and what this session has seen of each |
| `/provider` | the providers configured, in the order a turn tries them |
| `/plugin` | the capability bundles declared, the marketplaces they come from, and what failed |
| `/gates` | the check a turn must pass before it is done: a command, a file, or a rubric |
| `/import` | bring instructions, MCP servers, skills and a model across from another agent |

**No command was added in 0.28.0, and four of them stopped being half a surface.**
Every list here could already be pruned and none of them could be grown: `/mcp`
edited and removed servers and could not declare one, `/provider` promoted,
demoted and removed links and could not add one or change a key of one, `/plugin`
listed bundles and removed them and could not take one on, and `/config` named a
key and then left you to type a value out of a set it already knew. All four do
the other half now. No row was added above because none was wanted: a verb belongs
to the surface that already owns the list, and a second row for one screen reads
as a second screen — the rule `/usage` follows below.

Which of them is typed and which is a row is not arbitrary. The verbs `add`,
`edit`, `remove` and `get` on `/mcp`, `add` and `remove` on `/plugin`, and `set`
and `unset` on `/config` are read by the same parse `io mcp`, `io plugin` and
`io config` are, so a line typed in the composer and the same line typed at a
shell produce identical bytes rather than two readings that agree today. A bare
`/mcp` or `/plugin` still opens its panel, because a
panel is a better answer in a session than a text dump is, and `/config <key>`
still answers what that key is set to without writing anything. `/provider`'s two
new verbs are rows on its panel and never words you type: which link to add is
chosen from what your shell can already authenticate, and a list is the only
honest way to ask that. See [Without leaving the
session](#without-leaving-the-session).

`/effort` is new in 0.26.0 and sits under **this turn** beside `/model`, because
the two are the same question asked twice: which model the work goes to, and how
much thinking it is worth buying from it. It is a posture rather than a one-shot —
the level it sets holds for that turn and for every turn after it until you change
it — and a bare `/effort` reports the level in force and changes nothing.
`/effort off` is not a fourth level below `low`: it goes back to sending no
reasoning field at all, which is what every release before this one sent. See [How
much reasoning a turn buys](#how-much-reasoning-a-turn-buys).

`/profile` sits under **the session** from 0.26.0, and it is a correction rather
than a reshuffle — the third time this sentence has been written, after 0.19.0's
`/mcp` and `/provider` and 0.22.0's `/image` and `/copy`. **this turn** means a
command acting on the work the turn just finished, and `/profile` acts on nothing
that has happened: it changes which configuration overlay every *later* turn is
built from, which is a property of the session. It was filed under **this turn**
because switching one feels like something you do between turns, and where a
command sits is decided by what it acts on rather than by when it is typed. That
it also made room for `/effort` is the order the bound was meant to force: the
group stood at ten of ten, and the answer had been written down in advance —
re-file what is in the wrong group, do not widen the bound.

`/commit` is new in 0.25.0 and sits under **this turn** beside `/copy` and `/copy
diff` because it is the third thing you do with work that has just finished: one
puts the answer somewhere, one puts the patch somewhere, and this one makes the
patch permanent. Its description says *ask the agent* because that is literally
what the word does — it sends a prompt, and the agent reviews, stages and writes
the message. See [Git](#git).

`/plugin` is new in 0.20.0 and sits beside those two because it is the third
surface of the same kind: something a configuration file declares by name, whose
effect on the session is otherwise invisible. See [Capability
bundles](#capability-bundles).

`/import` is new in 0.21.0 and is last in the group because it is the one command
here you use once: the others are returned to for the life of the install. It
writes files, which is the whole reason it is under **configure**. See [Bringing
your setup across](#bringing-your-setup-across).

`/gates` is new in 0.24.0 and is under **configure** for that same reason and not
because of what its first screen shows. It opens on the criterion in force and the
last turn's verdict, which reads like an inspection — and then writes
`[app.io-cli.gates]`, which is the one thing **inspect** promises it never does.
That promise is worth more here than it is for a server list: a gate set by
accident does not merely change what the next turn talks to, it can spend a whole
extra turn against a real model deciding the first one was not finished. It sits
before `/import` because it is one you come back to. See [Verification
gates](#verification-gates).

`/resume` does more than reopen from 0.23.0: each row says what that session's
last run stopped on, and choosing one answers it. No command was added for that —
the one that was already there was extended. See [When a run stops for
you](#when-a-run-stops-for-you).

`/cost` and `/stats` are new in 0.22.0 and are two commands rather than one
because they are two questions: `/cost` says what the work cost, `/stats` says
whether it worked. See [What it costs](#what-it-costs).

`/image`, `/copy` and `/copy diff` sit under **this turn** from 0.22.0, and it is
a correction rather than a way of making room — the same sentence 0.19.0 wrote
about `/mcp` and `/provider`, and it is worth being able to say twice. All three
act on the turn that just finished: `/image` draws an image attached to it,
`/copy` puts its answer or its diff on the clipboard. None of them asks the store
a question, which is what **inspect** means. They were filed there because they
*show* something, and showing is not the same as inspecting. `/cost` and `/stats`
are what made it worth correcting: no group may hold more than ten, **inspect**
stood at nine, and the choice was between re-filing three commands that were in
the wrong group and filing two more that would have been.

`/mcp` and `/provider` sit under **configure** from 0.19.0, and it is a
correction rather than a promotion: both open with a list, and both go on from
that list to add, edit and remove entries in the configuration file, which is the
one thing **inspect** promises it never does. That second half was a promise
rather than a fact until 0.21.0 — the writers existed and nothing called them, so
both panels could only read. They write now. `/steer` and
`/compact` are listed under **this turn**, which is the group the code has
always filed them in — the table above said otherwise until 0.21.0, and
nothing but a reader was misled by it.

`/usage` opens `/cost` and is deliberately not listed above: an alias earns no
row of its own, because a second row for one screen reads as a second screen. It
resolved to `/status` until 0.22.0, which was the closest thing there was to an
answer and was not one — `/usage` means spend everywhere else it exists, and this
product now has a spend to report.

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

## When a run stops for you

A turn does not always end on its own. The agent can ask what you meant, or
propose a plan before it works; a tool call can be interrupted in a way
io-harness records but cannot judge; and a process that goes away mid-loop leaves
a run with committed work and no ending. Through 0.22.0 all four were left where
they fell — the run was in the store, and nothing here would open it again.

**`/resume` says what each session's last run stopped on, on the row you choose
it from.** The mark is a word rather than a symbol, so it survives `NO_COLOR`,
`--plain` and the ASCII glyph set: `asks` for a question nobody answered, `plan`
for an approach nobody decided, `tool` for a call whose outcome nobody recorded,
`died` for a process that went away and left committed work behind, `ended` for a
turn you stopped yourself. A session with nothing outstanding carries no mark at
all, so the list is ragged by construction and there is no column to read down.

**Choosing a marked session opens the same overlay the run would have opened
while it was live**, and what you say carries *that* run on from the step it
stopped at: the observation ledger, the token budget and the elapsed clock are
the run's own rather than a new run's. A plan is approved, sent back with a
correction, or cancelled outright. An interrupted call is retried or abandoned
here — `r` and `a` — and can also be **asserted to have landed**, which takes an
account of what it returned and is therefore offered by `io resume --recovery
completed --account "…"` rather than by a keystroke. What you say it returned is
filed against the step the call was made on, not the step the run has now
reached, so the resumed run reads a transcript in which the tool answered where
it was asked. A run whose
process merely died carries on from its last committed step plus one. `Esc`
leaves any of them parked exactly as it was found.

**A turn you interrupted is finished, not paused, and it is the one pause that
cannot be answered.** `Ctrl+C` makes io-harness record the outcome `cancelled`,
which is mapped to a *completed* run, and every one of its resume entry points
short-circuits on a completed run and hands back the original outcome having
driven nothing. So the most common way a turn stops is the one way it cannot be
continued. `/resume` reports such a session as ended by you and points at `/fork`
from the turn before it, which is the honest neighbouring answer rather than a
button that would quietly do nothing. io-harness's published documentation says
such a turn "stays resumable"; it is contradicted by the run loop in the same
crate, and that is reported upstream rather than worked around here.

**A turn that ends parked now says so.** Through 0.22.0 the prompt came back with
no sign that a run was sitting in the store waiting for a sentence from you.

### One `io` at a time on one conversation

One store serves this whole machine, so two terminals in one repository is the
ordinary case rather than the exotic one — and they are **not** in conflict.
Starting `io` creates a new session every time, so each terminal gets its own
conversation and neither is refused.

What two of them can genuinely contend over is a single *session*, and that
happens in one place: `/resume`, when one process enters a session another
already has open. Until 0.23.0 nothing guarded it — both advanced the same
conversation head, and the loser of that race had paid for a turn that was then
orphaned off the head path: still in the store, correctly parented, and never
shown again by a history that walks back from the head.

Each session is held under an advisory whole-file lock, and `/resume` into one
another `io` is holding is refused rather than taken. The lock is the kernel's —
`flock` on unix, `LockFileEx` on Windows — so it is released on exit, on a panic
and on `kill -9`: there is no stale lock to reap and no pid file to sweep. The
lock a session takes when it starts never contests anything, because that session
did not exist a moment earlier; what it does is write down who owns it, so the
next process to reach for it can be told.

**What the refusal can say about the holder is what io-cli itself wrote beside
the lock**, and no more: the process id, the workspace root, the `io` version and
the instant that process started. It is not the operating system's account of
that process. Asking the operating system means `/proc` on one platform, `ps` on
another and `tasklist` on a third, or a dependency this crate does not carry — so
the pid you are shown is a number `io` wrote down, and on a machine that has
since reused it, it names something that is not `io`. The lease exists only for
the case the kernel cannot cover, a home on a network filesystem, where an
advisory lock is not this program's business; there the record's own timestamp is
all the evidence there is.

A lock that cannot be taken for an ordinary filesystem reason does not stop the
session — you are told, and it opens. The guard exists to prevent one specific
corruption, and trading it for "io will not start on this machine" would be the
worse failure.

**What it does not cover.** Two `io` in one repository on two different sessions
are not in conflict and are not stopped. `io exec` and `io resume` take no lock
at all, so an `io resume` run beside a terminal holding the same session is not
refused by this. For everything the lock does not see, the guard of last resort
is io-harness's own compare-and-swap on the conversation head: the second writer
is refused, told, and its turn is not silently orphaned — which is the defect
`/undo` carried until this release.

## What it costs

**`/cost` commits what has been spent, and `/stats` commits how the runs have
gone.** Two commands rather than one, because they are two questions — what the
work cost, and whether it worked — and a single screen carrying thirteen sections
is one nobody reads to the end. `/usage` opens `/cost`.

`/cost` has four sections: **this run**, **this session**, **by model** and **by
day**, each with the calls, the money, and the token split under it. Every figure
is a row io-harness has been writing into `runs.db` since its 0.18.0 and this
interface has never read — the model, the prompt, completion, cache-read,
cache-write and reasoning split, the latency and the time to first token, one row
per provider call.

**Nothing is estimated.** There is no sampling, no extrapolation from a partial
window, and no model of what a token "usually" costs. Three rules follow from
that and each is on the page rather than in this file:

- **Cache reads and cache writes are the breakdown of the prompt they are part
  of, never an addition to it.** They are already inside `prompt_tokens`, so the
  page shows the prompt total and the parts under it — read, written, fresh — and
  the parts sum to the total rather than past it. Reasoning is the same
  arrangement under completion: a vendor that reports it separately still bills it
  as output.
- **A call the provider reported no usage for is *unknown*, never free.**
  io-harness stores that as SQL `NULL` and reads it back as `None` for exactly
  this reason; summing it as zero would report a turn that cost something as a
  turn that cost nothing. The count of them is a line on the page.
- **A model with no rate in the table makes the total a *floor*, and the page
  says so** — in that word, with the number of calls it applies to, and on the
  row itself in the grouped sections so a reader scanning for the largest figure
  can see which of them is incomplete without reading past the list.

`/stats` is the other half: runs by outcome, runs by day, the first-try counts,
gate failures by phase, recovery, the slowest calls and the time to first token
over the last 200 runs, and what the store holds on disk. **First-try is
io-harness's own definition** — finished *and* successful *and* carrying no gate
failure — and it arrives as three counts rather than as a rate, because the
denominator is a choice. Where a share is shown, the row names what it is a share
*of*. The sandbox gate's phases and the review, command and contains gates use
two vocabularies that do not overlap, so they are two lists and are never merged
into one. Nothing on the page compacts anything: `Store::compact` is a full
`VACUUM` that needs free disk roughly equal to the file, so the free figure is
reported and the reclaiming is not offered here.

### Where a price comes from

**io-cli compiles no prices in.** A rate baked into a binary is a promise the
binary cannot keep: providers move prices without announcing it, a release cadence
is not a pricing cadence, and an operator reading a confident wrong number is
worse off than one reading no number at all. So the table ships empty, and **an
install that has connected nothing sees tokens and no currency** — which is the
honest answer, not a bug.

It is filled from **the model catalogue your connected provider already serves** —
the same fetch `io setup` has made since 0.1.0 to offer you a list of models,
whose prices it read and threw away on the same row. The rates land in `[prices]`,
which is io-harness's own section: `as_of` for the date, and one line per model
under `[prices.models]`.

**Refreshing is a row on `/config`, and it shows every rate that would move before
it writes anything** — what each was and what it would become — and you can
decline the lot. That is not courtesy. The file records what a rate *is* and never
where it came from, so io-cli cannot tell a correction you made by hand from a
value an older catalogue served; it does not guess, it shows you. A refetch that
comes back empty, or far shorter than the table it would replace, is **refused**
and the old table kept: a truncated response that replaced a full table with a
handful of rows would turn most of your spending into "unpriced" and shrink your
reported bill with nothing failing anywhere. A first fill has nothing to compare
against and is never refused. Rows for models the catalogue no longer serves are
left alone, because io-harness prices a call by the model name on it and an old
row is what prices an old run correctly.

**Whose price it is gets said, on every surface that draws money.** OpenAI and
Anthropic publish no prices on any endpoint — their model endpoints
carry capabilities and limits and no cost field — so for those two the rates
necessarily come from the reference catalogue rather than from the vendor. The
page names which and on what date. On OpenRouter the two coincide, because the
reference catalogue is OpenRouter's own. `[app.io-cli.prices] source_url` points
at a different catalogue, which is how an operator on a self-hosted or
`compatible` endpoint gets prices at all.

**The unit is `$`.** io-harness prices in micro-units and names no currency,
because it is not the layer that knows one. io-cli is: every catalogue it can read
quotes USD and every provider it can connect to bills in it. Four decimals below a
unit and two above, in integer arithmetic — a turn costs a fraction of a cent and
a month costs dollars, and one precision cannot show both.

## Skills

**Five of the things io can do have a plain-language door.** Say what you want
in your own words — "stop asking me before every write in this repository", "add
the GitHub MCP server", "point this at a local model instead", "remember that we
use pnpm here", "update io" — and the model reaches for the skill that answers
it, instead of you reaching for the command that does.

| Skill | For |
| --- | --- |
| `io-permissions` | changing what io asks about before it acts |
| `io-mcp` | adding, changing or removing an MCP server |
| `io-provider` | switching provider, or pointing the session at a local model |
| `io-remember` | writing something down where the next session reads it |
| `io-update` | finding out whether a newer io has been released, and proposing the installer line for it |

**They are files, and nothing more clever than that.** Five ordinary `SKILL.md`
bodies written into `~/.io-cli/skills`, beside whatever skills you keep there
yourself. Open one, read it, edit it, copy it into a skill of your own, delete
it — the same things you would do to any other markdown file in a directory you
own. There is no registry, no index and no remote source; the five are carried
in the binary and written out the first time io has a home to put them in.

**Delete one and it stays deleted.** `rm ~/.io-cli/skills/io-mcp.md` is the way
to be rid of a shipped skill for good: io remembers that it wrote that name, so
it does not put the file back on the next start. A skill added in a *later*
version has no such record, so upgrading still brings you the new ones. If you
only want one out of the way for now, turn it off instead — that is reversible
and `/skills` does it for you.

**Each of them ends in a change you see before it lands.** A skill instructs the
model in what io can already do and which surface does it, so what comes back is
a proposed edit to `io.toml`, or to a memory file, or a command to run — shown
as a diff or as a command, gated by exactly the policy everything else is gated
by, and refusable. A skill can no more move the permission boundary behind your
back than the agent can, because moving it *is* a write, and a write is a thing
you approve.

**The model is offered a name and a description, and reads the rest only when it
matters.** Every turn's system prompt carries the catalogue — five names and
five short descriptions — and the body of a skill reaches the model through
io-harness's own `read_skill` tool, under this session's policy, like any other
read. So a skill costs the prompt one line until it is relevant, and it is
subject to the same boundary as everything else in the session.

**An upgrade refreshes what nobody touched and leaves the rest alone.** io-cli
records the bytes it last wrote for each shipped skill; a file that still
matches gets the new text, and a file you have edited is kept exactly as it is
and named on screen as kept. A skill with no record at all is treated as yours.
The bias is deliberate: there is no restore point behind these files, so the
failure mode of a lost or unreadable record is that io-cli stops refreshing,
never that it writes over something you wrote.

**Turning a skill off is moving its file into `~/.io-cli/skills/disabled/`.**
io-harness admits a subdirectory only when it holds a `SKILL.md`, so a folder of
loose `.md` files is invisible to discovery, to the catalogue and to
`read_skill` — which makes a directory the whole mechanism, with no second list
to disagree with the filesystem. It works on your own skills too, and it
survives an upgrade: a shipped skill sitting in `disabled/` is not written back
into `skills/` on the next launch, because a switch that turns itself on again
every morning is not a switch. `/skills` does the move for you and shows, for
every skill in both directories, what it is for, whether it came from io-cli or
from you, whether it is on, and the file it lives in.

**Two names resolving to one skill is fatal, which is why io-cli withholds
rather than overwrites.** io-harness addresses a skill by name, and a directory
holding two of the same name fails discovery outright — not as a listing quirk,
but as an error raised at the start of a run, so *every turn of that session*
dies before the first completion. The resolved name is the `name:` in a file's
frontmatter where there is one, not the filename, so a file of yours called
anything at all can claim `io-mcp`. io-cli therefore reads the directory before
it writes to it, and never installs a shipped skill over a name your own files
already claim: it installs four instead of five, and says which one it withheld
and which file claimed the name. Rename yours, or leave it — the choice stays
with the file you wrote.

**And there is a ceiling: io-harness accepts at most 64 skills in a directory.**
It rejects the whole set rather than trimming it, so an operator sitting near
the limit who gains five more would otherwise get no skills at all as their
upgrade. io-cli counts first, installs up to the ceiling and stops, and says how
many it installed and how many it withheld. `/import` counts against the same
ceiling before it writes a byte, and refuses the whole import rather than leave
you over it.

**That ceiling is per directory, and the directories are not bounded together.**
Every skills directory is discovered on its own — yours, and one more for each
capability bundle that declares any — so six bundles can put far more than 64
names in front of the model with nothing failing anywhere and nothing said about
it. What the limit protects is one directory's discovery, not the size of the
catalogue a turn is handed. If the palette has grown longer than you can read,
that is why, and `/skills` is where you see which directory each name came from.

**A bundle's skills are listed too, under the name the model actually uses.**
Until 0.21.0 they reached the model and appeared on no surface that lists a skill,
so `/skills` and the `/` palette were lists that disagreed with the catalogue the
turn was handed. They are in both now, spelled `<bundle>__<name>` — io-harness's
own namespacing, and the string a refusal or a tool call will name — with the
bundle named as where the row came from.

**Turning a bundle's skill on or off is refused, and the refusal is the honest
answer.** Turning a skill off is moving its file into a `disabled/` directory
beside it. For a bundle skill that would mean io-cli creating a directory inside
somebody else's bundle and moving their file into it — a directory io-cli did not
install, does not own and cannot put back. Stop the bundle instead: `/plugin`
removes its `[[plugin]]` entry and everything it contributed goes with it.

**And a bundle naming a skills directory that is not on disk killed every turn of
that session, silently, in 0.20.0.** io-harness joins the manifest's word onto the
bundle root with no existence check at all, and the walk that discovers skills
fails the run before the first completion — so a typo in a `plugin.toml` somebody
else wrote reads as io being broken. `/skills` and `/plugin` name the bundle and
say what it costs, one row per bundle: a second broken bundle does not hide behind
the first, and a broken one no longer takes the surface that could explain it down
as well.

## Capability bundles

**A bundle is a directory with a `plugin.toml`, and it is in your session because
a file of yours named it.** One `[[plugin]]` entry, and nothing else:

```toml
[[plugin]]
path = "~/bundles/rust-review"
```

That is a declaration and never a scan. There is no directory io walks looking
for bundles and nothing that loads by being present on disk — declaring one is
the whole of installing one, and deleting the line is the whole of removing one.
There is still no registry either; from 0.29.0 there are
[marketplaces](#marketplaces), which are repositories you name and clone, and
installing out of one writes exactly the entry above.

One directory can hand over six kinds of thing at once: skills, prompt templates,
`[[agent]]` definitions for a fan-out to draw children from, `[[mcp]]` servers,
`[[hook]]` tables, and policy layers. That breadth is why `/plugin` exists.
Every other capability in a session is one you put there — a skill file is yours
or io-cli's, an `[[mcp]]` entry is a line you wrote, a policy layer came from a
posture you chose. A bundle is a directory somebody else wrote that adds names to
four subsystems on one line, and *what did that directory put in my session* is a
question whose only previous answer was to open the manifest.

**`/plugin` answers it, and it answers the dropped ones too.** One row per bundle
with its id, its root and what it contributed; choosing one opens what it brought,
by name. Under those, one row per bundle that was *declared and did not load*,
carrying io-harness's own sentence whole. That second list is what the surface is
really for: io-harness's plugin loader has no error path, so a bundle with no
manifest, unparseable TOML, an unusable id or a contribution its scope may not
make is dropped, recorded, and otherwise silently absent while every other bundle
loads. A bundle you believe is running can be gone for a week. This is where that
week ends.

**From 0.29.0 there is a third list, for the same reason.** io-harness 0.70.0
lets an entry say `enabled = false`, and a bundle written that way is read,
parsed and held to the whole trust rule while contributing nothing. It is a
state, not a failure — it is doing exactly what your file asked — so it is drawn
under its own mark with what switching it back on would bring, rather than
beside the ones you have to fix. It counts as declared, too: a configuration
whose bundles are all switched off is no longer reported as declaring none, which
is the sentence above inverted and just as misleading.

**And a bundle can be stopped from the same list.** The last row under a bundle's
contributions removes its `[[plugin]]` entry, after a confirmation that names the
scope and the entry it will take out. io finds that entry by matching the
directory across all three scope files rather than by counting rows on screen —
the two lists have no relation to the order entries appear in any file, and a row
number read against the wrong list removes a bundle you never mentioned. Where no
file names the directory, io says so and removes nothing.

The directory itself is never touched. This surface edits a configuration file,
and deleting somebody's work because they stopped loading it is not a thing a list
should do. Declaring a bundle is still a line you write yourself: a path is
something you type more comfortably into your own file than into a picker.

**Which file declared a bundle decides what it may contribute.** A bundle named
in the project-scoped `io.toml` — the file a `git clone` delivers — may
contribute skills, templates, agents and policy, and may **not** contribute
hooks or MCP servers, because both of those run a program on this machine. A
project-scoped bundle that tries is refused **whole**: it contributes nothing at
all, not the half that would have been safe. Move the `[[plugin]]` line into
`io.local.toml` or into your user file and the same directory loads completely.
The rule is about which file names it, exactly as it is for `[browser]`.

**A bundle's policy may only narrow.** Its layers may deny and may never allow: a
`[policy] defaults` table in a manifest is refused by name, and a single rule
whose effect is anything but `deny` drops the bundle. So the worst a bundle you
misjudged can do to your permission boundary is take something out of it.

**A bundle id must match `[a-z0-9][a-z0-9-]{0,31}`**, and every name it
contributes is rewritten by io-harness to `<bundle>__<name>` at load — an agent's
name, an MCP server's id, a policy layer's name. `/plugin` draws that namespaced
string unchanged rather than a prettier short form, because it is what a refusal
will name, what a tool call will name, and what you will type to spawn the agent.
A shorter name here would be a third spelling of the same thing.

**Hooks are the one contribution io cannot itemise, and the row says so.**
io-harness applies a bundle's hooks and keeps its `Hook` type private, so there
is no API by which this program can count them or say what any of them runs.
`/plugin` therefore draws a row saying the bundle contributed hooks and that io
cannot say what they do. The alternative was to leave the row out, which reads as
a bundle with no hooks — the one reading that is false, on the contribution kind
that runs programs. **The one place io does name them is a marketplace install**,
below, where it reads them out of the manifest itself — because that is the one
moment you are being asked to accept a directory you have not read. Reported
upstream as io-harness#223; the reading goes when the accessor arrives.

## Marketplaces

**A marketplace is a git repository you name.** It is cloned into your own home
and walked for directories carrying a `plugin.toml`:

```
/plugin marketplace add zeroonething/ultraship
/plugin marketplace list
/plugin marketplace remove zeroonething/ultraship
```

The same words work from a shell — `io plugin marketplace add …` — through one
parse. There is no index file to write and none to disagree with the directories
it describes, and io operates no registry: it hosts nothing, curates nothing and
ranks nothing. The fetch is a `git` invocation and nothing else, so this adds no
HTTP client and no network path beside io-harness's. A machine with no `git` is
told so by name, and installing from a directory you already have is unaffected.

Installing is the verb you already had:

```
/plugin add ultraship
/plugin install ultraship               # the same verb, another word
/plugin add ultraship@zeroonething/ultraship   # when two marketplaces carry it
/plugin search review
```

**A bare name two marketplaces carry is refused**, naming both qualified
spellings. They are two strangers' repositories, and installing whichever the
walk reached first is installing code you did not choose.

**Removing a marketplace removes the clone and nothing else.** A bundle you
declared out of it keeps its `[[plugin]]` entry — a cache being emptied is not a
reason to undo a decision you made about your configuration. What io owes you
instead is the consequence, so it names the bundles that will stop loading before
it deletes anything.

### What a bundle is allowed to do is shown before it is allowed to do it

A bundle contributes to four subsystems at once, and until 0.29.0 every one you
declared came from a directory you had read. A marketplace removes that reading,
so the install puts it back.

The entry is written **`enabled = false`** first. io-harness then reads, parses,
validates and trust-checks the bundle for real — there is no public way to ask it
about a directory that no configuration declares — and hands it back contributing
nothing at all. What you are shown is what io-harness parsed: the skills and
template directories, the agents, the MCP servers and the policy layers, in the
**namespaced** names you will actually see in a trace and type to spawn an agent.
A bundle io-harness would refuse is refused at that point, in its own words,
before you are asked anything.

Saying yes changes one key. Saying no leaves the bundle declared, switched off,
and listed in `/plugin` — visible, and one keystroke from being switched on if
you change your mind.

Hooks are named here too, read from the manifest, because `enabled = false` is
still not enough to make io-harness tell you what a hook runs. Consenting on the
bare word "hooks" is consenting to programs nobody named.

**Writing `enabled` costs something and io says so at the time.** An io-cli built
against io-harness 0.69.0 does not know the key and refuses the *whole file*
rather than ignoring it. Remove the `enabled` keys before downgrading.

## Hooks

**`[[hook]]` tables run from 0.20.0.** They were parsed before this release and
then installed on nothing, so a file asking for every event to be written to
`audit.jsonl` produced an empty file and no error. They now run in a session turn
and in `io exec` alike, from the same call that builds everything else.

A hook either writes events down or runs a program:

```toml
[[hook]]
on = []                       # the events to observe; empty means every one
append = "audit.jsonl"        # one JSON line per event, appended

[[hook]]
at = "before_tool"            # the only `at` there is
tools = ["shell"]             # which calls this one sees
run = ["./scripts/gate.sh"]   # argv, never a shell string
on_failure = "refuse"
timeout_ms = 5000             # the default
```

`on` and `at` are mutually exclusive, because the first is an observer of events
and the second is a gate in front of a tool call. An `at` hook must have a `run`.
Exactly one of `append` and `run`: a hook that did both would be a hook whose
failure meant two things.

**`on_failure` is where a hook's power actually is.** `continue` lets the turn go
on, which is what an audit hook wants. `cancel` ends the turn at the next step
boundary and **the run stays resumable** — it is a stop, not a crash. `refuse`
turns that single tool call back and leaves the turn running, which makes a
`before_tool` hook a rule of your own standing beside the policy engine's.

**`run` is an argv array and never a shell string.** Nothing is word-split and
nothing is expanded: the program you named is the program that runs, with the
arguments you wrote.

**A `run` hook runs on the turn's own critical path, and `timeout_ms` is the only
thing bounding it.** An observer is called synchronously by the run, and the run
shares a task with the loop that reads your keyboard — so while a hook's program
is running, the interface is not repainting and not answering keys. A script that
takes a tenth of a second, on a hook matching every event, costs that tenth of a
second per event. Keep a `run` hook fast, match it to the events you actually
want with `on`, and lower `timeout_ms` from its five-second default if the program
can hang. An `append` hook has none of this cost: it is a line written to a file.

**A hook that fails is quiet.** io-harness reports a failed hook through a log
this binary installs no subscriber for, so an `append` path that cannot be written
and a `run` program that does not exist both leave the session looking normal —
and a hook with `on_failure = "cancel"` ends the turn without saying which hook
did it. Verify a new hook by checking that it did something: read the file, or
give the program a visible side effect. This is a real limitation of 0.20.0 rather
than a subtlety.

**A project-scoped file may not declare `[[hook]]` at all.** io-harness refuses
the whole configuration rather than dropping the table — a hook runs a command on
this machine and `io.toml` is the file a `git clone` delivers. There is no
`Config` to be had, so `io` genuinely cannot start, and 0.20.0 does not soften
that. What it changes is the words: io-harness's own sentence, which names the
key, the reason and the two files that may carry it, under a line saying which
file was being read. Before this it arrived as a bare error string from a program
that had already exited, against a repository you had just cloned. Write the
table in `io.local.toml` or in your user file.

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

**From 0.20.0 a child is shown by the name it was spawned under.** io-harness
gives every admitted child an address — the `as` argument the parent chose, or
one derived from the agent it drew, like `reviewer#42` — and that is what the row
carries, with its roster role beside it, instead of a run number nobody picked. A
run id identifies a row in the store; an address is what the parent used to reach
the child, what a message between two of them names, and what you type to attach
to one.

**A message one agent sent a named sibling is drawn in the tree, with its body.**
Children talking to each other is the case a run number told you nothing about:
one addressed line with the text under it, landing where it happened.

**A child that detached can be selected and attached to.** A parent that stops
waiting is not a parent that stops the work — a detached child is still running,
and until now it was a row you could read and could not reach.

**A waiting child is a number and not a row**, because until a concurrency slot
frees it has no run of its own to name. It has no address either, for the same
reason: io-harness names a child when it admits one, so there is nothing to call
a queued child even now that the admitted ones have names. A fleet that is
queueing and a fleet that is stuck look identical without that count, which is
why it is there.

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

## Verification gates

**An agent that stops is not an agent that is done.** Every release before 0.24.0
took the model's own word for it: the turn ended, the interface said so, and
whether the tests still passed was a question you asked afterwards. A gate is
where you say what "done" means *for this repository*, once, and the turn is not
finished until the criterion passes or the retry budget is spent.

`/gates` writes it and shows what the last turn was judged on. It is a section of
your configuration file like any other, so it can also be typed by hand:

```toml
[app.io-cli.gates]
command = ["cargo", "test", "--all"]
retries = 1
```

There are three kinds and you get exactly one, because a `TaskContract` in
io-harness holds one `Verification` and not a list. Naming none, or naming two, is
refused where you can still see what you typed rather than silently picking a
winner:

- **A command** that must exit a status you name — zero unless you say otherwise.
  It is an argv and never a shell line, because io-harness checks `argv[0]`
  against your permission boundary and runs it without a shell. This is the cheap
  kind: it costs a process, it is objective, and it is the same thing you would
  have run yourself.
- **A file** that must exist, and optionally must contain some text. Nearly free
  and deliberately narrow — it answers "did the change actually get written down",
  which is the failure a passing test suite is worst at catching.
- **A rubric** a second model answers: a sentence saying what the work has to be,
  judged by a reviewer you name.

**io-cli holds no list of test commands.** `/gates` offers you the one the
repository's own toolchain proposes — io-harness detects that from the project, so
a Rust checkout is offered a `cargo` line and a Node one is not — and accepting the
offer writes that command into your file, where you can read it and change it. What
is written is always a concrete argv: `command` is the program and its arguments,
never a shell line and never blank. That detection is the dependency's, and it is
deliberately not reimplemented here: a table of build tools inside this crate would
be a second opinion that goes stale the first time somebody's project does not look
like the ones it was written against.

**The criterion is run by io-harness, in the sandbox, and not by io-cli.** That is
not squeamishness. A criterion run from here could not be handed the cache
directories a real run gets from the detected toolchain, so a `cargo test` gate
would fail on a registry write that io-harness's own gate would have allowed. It
also keeps a rule worth keeping: exactly one module in io-cli starts a process at
all, and it is the one behind `!`.

The single exception is a `file` criterion with no `contains`. io-harness has no
criterion for bare existence — the nearest one treats a missing file and an empty
needle as a pass — so io-cli answers that one itself, with the reader that tells a
missing file from an empty one. It runs no process to do it, and it is the reason
that criterion costs nothing at all.

**`retries` defaults to 1, and `0` means report-only.** A failing gate sends the
agent back to work with the failure text — the compiler's output, the missing
file, the reviewer's sentence — because the failure *is* the instruction, and an
agent told "it did not pass" without being told what did not pass is being asked
to guess. One retry is the default because a retry is a whole turn against a real
model and not a loop counter; if you want several, say so. Set `retries = 0` and
the verdict is drawn and recorded and nothing is re-driven, which is what you want
in a run you are watching and what you want in a run you are only measuring.

**The criterion runs after every step, not once when the turn ends — and for a
rubric every one of those is a billed completion.** This is the number to know
before you configure anything. io-harness evaluates the contract's criterion at the
bottom of its step loop and keeps going until it passes, so a turn that takes nine
steps runs your command nine times, and a rubric on that turn is nine calls to the
reviewer rather than one. It is not "the agent finishes and then the work is
checked": it is checked continuously, and the run ends the moment the check
succeeds.

That is what makes a command criterion worth choosing carefully. `cargo test --all`
after every step of a long turn is a great deal of compilation, and a narrow
command — one test, one binary, one lint — is usually the right gate. For a rubric
the cost is money rather than time, which is the reason the three kinds are named
in `/gates`'s own one-line description rather than left to the surface. The call is
io-harness's, so it lands in the run's usage like any other and `/cost` counts it.

**A rubric needs a `reviewer`, and it is refused without one.** io-harness answers
a missing reviewer with a configuration error at run start — before the first
billed call, on every turn, in a place on screen nowhere near the keystroke that
caused it — so `/gates` refuses it while you are still looking at what you typed.
The reviewer is also never defaulted to the model doing the work. A model marking
its own paper is a decision rather than a convenience, and it is spelled
`allow_self_review = true`; without it, naming the working model as the judge is
the second refusal.

**What a gate is not is a test runner.** io-cli does not discover your tests, does
not parse their output, and does not decide what a passing suite looks like. It
carries one criterion on the contract and reports the verdict io-harness came
back with.

`io exec` gains exit `6` for this — the agent finished and the work does not hold
up. See [Exit status](#exit-status).

## Which model a run asks

A gate says what "done" means. This says what to do when the model that has to
reach it keeps missing, and what to do when the work turns out not to have needed
the model you started with. `[app.io-cli.routing]` holds two optional rules, each
written as a sub-table because a rule is a threshold and a model that only mean
anything together — a threshold with no model and a model with no threshold are
both half a rule, and a sub-table makes the pair the unit the file itself
enforces:

```toml
[app.io-cli.routing.escalate_after]
failures = 3
model = "a-stronger-model"

[app.io-cli.routing.downshift_under]
bytes = 2000
model = "a-cheaper-model"
```

**`escalate_after` moves up after that many consecutive failed gate attempts** —
the gates above, counted consecutively rather than cumulatively, because a run
that fails, recovers, and fails again much later is a run doing hard work rather
than one that needs a bigger model. **`downshift_under` asks the cheaper model
while the run has written fewer than that many bytes to disk**, measured on what
was actually written rather than on what was planned, so it is a fact about the
run rather than a forecast of it. Neither key defaults: half a rule is a parse
error naming the key you left out, rather than a threshold quietly reading as
zero and escalating on the first gate attempt of every turn.

Escalation happens **once** and does not come back down, and escalation **wins**
over downshifting where both apply. Both of those are io-harness's rules rather
than io-cli's: it owns the consecutive-failure count, the byte total and the
decision, taken after every step of the run. io-cli evaluates none of it, because
a second implementation here would be a second answer that drifts from the one the
run actually used.

**Routing does not reach a contained turn, and that is the first thing to know
about it.** io-harness applies routing in its flat workspace loop only; a turn run
under `[app.io-cli.containment]` takes each agent's model from that agent's own
roster entry and never consults the rules. So for an operator who has configured
containment, the section parses, is listed by `/config`, reaches the contract —
and never fires. A session that has both is told so at three moments: at start, when `/config`
is opened on the keys themselves, and when `/contain on` is typed — which is the
one that matters most, because that operator began uncontained, was told nothing
because nothing applied, and has just moved into the mode where their rules do
not fire. A session with containment off is told nothing, because a caveat
attached to a feature that is working is how an operator learns to stop reading
the notices. A turn taken with
`/contain off` routes normally, and `io exec` uses the flat loop, so routing works
there. Nothing in io-cli can close this: the loop that would have to consult the
rules is the dependency's, and what this interface owes you meanwhile is the
disclosure.

**There is no `require_primary` key**, and its absence is a decision rather than
an omission. io-harness's own `Routing` carries the field, and it gates on
`Provider::reachable` — a defaulted trait method whose body answers yes, and which
no provider in io-harness 0.69 overrides. A key for it would be offered on a
surface, accepted from a file, and permanently inert: you would set it, believe an
unattended overnight run now refuses to start against a dead endpoint, and get
exactly the behaviour you had before. It goes in when a provider answers the
question.

## Git

A gate says the work holds up. This says what becomes of it.

The agent has had seven git built-ins on every workspace run since long before
this interface existed: `git_status`, `git_diff`, `git_log`, `git_add`,
`git_commit`, `git_branch` and `git_worktree`. They are io-harness's, and each is
a fixed argv that can reach no other subcommand — there is no `push`, no `remote`
and no `reset` among them. What was missing until 0.25.0 was any of it reaching
you.

**The branch the working tree is on is on the status line.** It is read out of
`.git/HEAD` as text, because git writes it there in a format that has not changed
in the lifetime of the tool and this program starts no process to ask — so it
costs a file read, and it follows the agent when the agent switches branch. A
detached head is drawn as a short object id rather than as nothing, and a
directory that is not a repository draws no field at all: `io` runs in plenty of
them and must not get worse there.

**A commit the agent makes is committed into the scrollback** — the branch it
landed on and the message the model wrote. The
diff is not drawn a second time. It is already on screen immediately above, from
the step that wrote it, and drawing it twice would cost you the reason the block
is there at all.

**`/commit` hands this turn's work to the agent, and the agent writes the
message.** io-cli runs no git and composes no subject line: the command sends a
prompt asking it to review what changed with those tools, stage what belongs to
this turn, and say what the change does and why. That is a billed turn against a
real model, and it is why the row says *ask the agent* rather than promising a
deterministic act.

**`[run.commit_identity]` decides who the commit is authored as**, and io-cli
reads that value rather than picking one. io-harness hands the name and email
from that section to git on the commit invocation itself, and the section always
resolves to something, so a repository with no identity of its own is told which
default io-harness will use. You are told before the turn is spent, because the
author of a commit is the one thing about it that cannot be corrected afterwards
without rewriting history.

### The refusal this repairs — and the half of it that is now fixed upstream

**Through io-harness 0.69.0, all seven tools were refused before they ran, for
most operators, and nobody was ever asked.** io-harness's git spawn checked the
`exec` policy itself and accepted only an outright allow. Every other gated act
turned an *ask* into a question on your screen and waited for it; this one
returned a refusal instead, so `ask` behaved exactly as `deny` did — and `ask
before writes`, the posture the wizard recommends, sets `exec` to ask. io-cli
0.25.0 found that and filed it as io-harness#214.

**io-harness 0.70.0 closed it, and 0.29.0 pins 0.70.0.** An asking posture now
raises an ordinary approval: you get the question, and git runs if you say so.
If you run the recommended posture, none of the rest of this section applies to
you any more.

What is left is `read only`, where `exec` is a **deny** and there is still no
question for you to answer. There io-cli names the refusal and offers one rule:
`exec` allowed for `git`, one binary, for this session. `/commit` asks that
*before* it spends the turn, because a commit the policy was always going to
refuse still costs a real completion to discover. The rule goes through the same
remembered layer as anything else you allow for a session, so it is exactly as
strong as those and no stronger — and it is offered **only** where the deny came
from the posture's own default rather than from a rule somebody wrote, because a
later layer can add capability but can never take back a denial. Offering it
against a deny rule would be advice that can never be taken. One binary name is
also the narrowest grant that works: an `exec` pattern has no notion of a
subcommand, so `git` says *this program may be spawned* and nothing about any
other.

**Under a posture that denies rather than asks, no rule is offered.** A rule is
matched before a default, so the same allowance would work under `read only` too,
and a keystroke that quietly defeats the one posture whose name is a promise is
not a convenience. What you are told there is to change posture — a decision, and
not a shortcut.

### A checkout of its own

Every agent in a tree shares one working directory, so two children editing the
same file are one overwriting the other. `worktree = true` on an `[[agent]]` entry
gives that child its own checkout instead:

```toml
[[agent]]
name = "reviewer"
worktree = true
```

io-harness roots it under `.worktrees/` in your repository, on a new branch
created before the child's first step. **The branch is not named after the
entry** — it is the entry's name, the parent run, the step and a digest of the
child's goal, so `reviewer` becomes something like `reviewer-12-3-a1b2c3d4`.
That is what makes two children of one entry, spawned in the same step, land in
two checkouts rather than one; and since nothing here removes either, it is also
the shape to look for when you go and find the work afterwards. If the worktree
cannot be
made — no git, not a repository, the boundary refusing that path — the spawn
fails with the reason and **no child starts**, rather than quietly sharing the
parent's tree and reintroducing the collision the switch exists to remove.
`/fleet` marks the rows whose roster entry asked for one. See [The
fleet](#the-fleet).

That mark is a property of the roster entry and not a directory. io-harness
records a child's actual worktree path and hands it back to nobody, so a path
drawn on that row could only be reconstructed — and a reconstruction is an
address that is wrong the moment either side changes, which matters here because
you would `cd` into it.

**What none of this does.** Nothing removes a worktree and nothing deletes a
branch: that is yours to do, because removing one throws away the work the child
was spawned to produce. io-cli does not open a pull request, and the seven tools
reach no remote at all. The work ends as commits on a branch in your own
checkout, and what happens to it after that is a decision this program does not
make for you.

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

**The agent can look at images in the workspace**, using io-harness's own
`view_image` tool, which enabling its `media` feature switches on. It is bounded
by the same policy as any other read. When it looks, the same picture goes into
your scrollback at that point in the conversation, so you are reading what it
read rather than a path you would have to open yourself.

That is the shape every capability of this kind arrives in, and 0.20.0 adds
twelve more of them: **io-cli cannot take a tool out of io-harness's workspace
tool set**, so a feature this crate turns on is a tool the agent has, and the
only honest thing to do with that is say so. See [Documents](#documents).

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

## Documents

**The agent can read and write spreadsheets, Word files, slide decks, PDFs and
barcodes from 0.20.0**, because io-cli turns on io-harness's `documents` feature
— `xlsx`, `docx`, `pptx`, `pdf` and `barcode`. That is twelve tools in its
workspace tool set, and **six of them write**:

| Format | Reads with | Writes with |
| --- | --- | --- |
| Spreadsheets | `xlsx_sheets`, `xlsx_read` | `xlsx_write`, `xlsx_set_cell` |
| Word | `docx_read` | `docx_write` |
| PowerPoint | `pptx_read` | — |
| PDF | `pdf_read` | `pdf_write`, `pdf_watermark`, `pdf_fill_form` |
| Barcodes | `barcode_decode` | — |

Every one of them is a read or a write like any other: the same policy gate, the
same approval prompt answered where it was asked, and the same refusal naming the
act, the target, the rule and the layer. **`xlsx_write` replaces a file that
already exists** — under the write gate, so it is proposed to you before it
happens rather than reported afterwards.

**Which reader runs is decided by the tool the model called, not by the file's
extension.** A `.docx` handed to `pdf_read` is a failed read rather than a guess,
and renaming a file changes nothing about what can be done to it.

**What they do not do**, because a document tool that half-works is worse than
one that is absent:

- **Word is generate-and-read, with no edit in place.** A read followed by a
  write produces a new document out of the text that came back, so comments,
  content controls, fields and vendor extensions that were in the original are
  not in the result. It is the right tool for producing a document and the wrong
  one for touching up somebody else's contract.
- **PowerPoint is read-only.** There is no `pptx_write`, and the table above is
  the whole of it.
- **PDF text extraction is best-effort about reading order**, which is what
  extracting text from a page-description format means. **A scanned page comes
  back with empty text rather than an error**, because there is nothing in it to
  extract — there is no OCR anywhere in this.
- **`xlsx_set_cell` preserves the rest of a workbook in practice rather than by
  guarantee.** It is the tool for changing a value in a sheet of data, and not
  the tool for a workbook heavy with charts, pivot tables or macros.
- **There is no barcode generation**, only decoding.

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
| `4` | the run stopped needing a human: it asked a question, proposed a plan, or was interrupted in the middle of a call |
| `5` | it ended without finishing: stalled, escalated, or cancelled |
| `6` | the agent finished and the work does not hold up: a gate you configured did not pass |

A ceiling is `3` and not `0` because io-harness returns one as a *successful
call* whose outcome says a limit was hit; a status read off the result alone
would call a truncated run a finished one.

**Exit `6` is new in 0.24.0 and it is the only one that is.** It says something no
other row could: the run ended the way `0` ends, of its own accord, and then the
criterion you set in [`[app.io-cli.gates]`](#verification-gates) failed anyway —
the tests did not pass, the file was not written, the reviewer said no. It is not
`1`, because nothing went wrong with the invocation; it is not `5`, because the
agent did not stall or give up; and it is emphatically not `0`, which is the
status a build script reads as permission to carry on. A run that never had a gate
configured can never return it.

**No exit code was renumbered, and `6` is the first one added since `io exec`
shipped.** `0` through `5` have meant exactly what they mean in the table above
since 0.5.0, and they mean it unchanged here: a script branching on them is a
script this release did not break. What changes is that a script branching on `0`
alone now has a sixth answer to handle, which is the point — before this release
there was no status a gated run could return that said the work was not good
enough, because there were no gates.

**Exit `4` names the pause from 0.23.0, and the invocation that answers it.**
The closing line used to name the run id and nothing else, which addressed
none of the four pauses; it now names the question, plan or call the run stopped
on and the `io resume` that decides it. That release renumbered nothing and added
nothing: `4` had been given to a pause that could not yet be answered for exactly
that release.

An approval is the one pause `io resume` cannot take: it is answered by the
person the run asked, at the terminal it asked from, and there is no resume entry
point in io-harness that takes one. A headless run never reaches it, because
every approval there is declined.

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

### Resuming without a terminal

`io resume` is the headless door to a run that stopped for a person. `--list`
enumerates the runs waiting for one and drives nothing — it reads the store,
calls no provider and takes no lease on anything it lists. Naming a run by id
resumes it, with the decision on the command line:

```sh
io resume --list
io resume --list --json | jq -r 'select(.waiting_on=="question") | .run_id'
io resume 41 --answer "use the parser that is already there"
io resume 41 --plan revise --correction "leave the public API alone"
io resume 41 --recovery completed --account "the tag was pushed; CI is green"
io resume 41 --goal "add a test for the parser and run it"
```

| Flag | Does |
| --- | --- |
| `--list` | list the runs waiting for a person and carry none of them on |
| `--answer <text>` | answer the question the run stopped on |
| `--plan <verdict>` | `approve`, `revise` or `cancel` the plan the run proposed |
| `--correction <text>` | what the plan should do differently; required by `--plan revise` and refused without it |
| `--recovery <decision>` | `retry`, `abandon` or `completed` — what happened to the call the run was interrupted in the middle of |
| `--account <text>` | what that call returned; required by `--recovery completed` and refused without it |
| `--goal <text>` | what the run was asked to do, for a run whose goal cannot be recovered |
| `--json` | write the resumed run's events, and `--list`'s rows, as newline-delimited JSON |
| `--policy <posture>` | the posture for the rest of this run; defaults to the one the run itself recorded |
| `--provider <name>` | `openrouter`, `anthropic` or `openai` — take the credential and model from the environment |

**Each pause takes its own input, and exactly one.** clap cannot see which pause
a run is on, so a flag for the wrong one is checked against the store and refused
rather than ignored: `--plan approve` typed at a run holding a question is
somebody acting on the wrong thing, and the refusal says what that run is
actually waiting on and what to type. A run whose process merely died takes no
flag at all — `io resume <id>` carries it on from its last committed step.

**There is no `--sandbox`.** A resumed run already started under a boundary, and
the confinement it carries on under is the project's `[sandbox]`. A flag that
widened it halfway through a run would be a widening nobody asked for at the
point nobody is watching.

**`--goal` is required for a run that served no session turn** — one `io exec`
started, or any other non-session caller. io-harness publishes no reader for
`runs.goal`, so for a run that served a session turn your own words are
recoverable from the turn, and for a bare run they are not. Rather than resuming
against an empty goal and spending a budget on a task nobody set, `io resume`
asks for it. Supplying `--goal` for a run that has one is you re-aiming your own
run, and it wins.

**A turn you interrupted is refused here in the same words the session uses**,
before a provider is built — see [When a run stops for
you](#when-a-run-stops-for-you) for why it cannot be carried on.

The exit status is the table above: a resumed run that pauses again exits `4`
naming the new pause, and `io resume --list` exits `0` whether or not it found
anything.

### Managing the configuration without a session

`io mcp`, `io plugin` and `io config` are new in 0.28.0 and do from a shell what
`/mcp`, `/plugin` and `/config` do inside a session. They open no session, start
no run and touch no store — a configuration listing that had to build a task
contract before it could print is a listing nobody can put in a script — and they
are answered before the terminal check, so `io config list` works in CI where an
interactive session is refused for having no terminal.

```sh
io mcp add semlith -- semlith --store /path/to/.semlith mcp
io mcp add linear --url https://mcp.linear.app/mcp --header 'Authorization=Bearer ${env:LINEAR_TOKEN}'
io mcp add --transport http linear-server https://mcp.linear.app/mcp
io mcp list
io mcp get semlith
io mcp edit semlith --timeout-secs 30
io mcp remove semlith

io plugin add ./bundles/rust-review
io plugin add ultraship                       # or a name from a marketplace
io plugin install ultraship                   # the same verb
io plugin search review
io plugin list
io plugin remove ./bundles/rust-review

io plugin marketplace add zeroonething/ultraship
io plugin marketplace list
io plugin marketplace remove zeroonething/ultraship

io config get run.max_steps
io config set run.max_steps 40
io config set app.io-cli.gates.command cargo test --all-features
io config unset run.max_steps
io config list
```

It is the same parse the composer uses. `/mcp add semlith -- semlith --store
/path/to/.semlith mcp` typed at the prompt and the first line above are one
sentence arriving through two doors, read by one function, planned into one edit
and written by one writer — so the two cannot come to write different bytes, which
is what two hand-written readings of one grammar always eventually do.

**`--` ends io's arguments, and everything after it is the server's, verbatim.**
The `--store` on the first line is semlith's flag and never io's. A parser that
went on looking for its own past that point would eat an argument out of the
middle of somebody's command line and start a server that behaves differently
from the one they wrote down.

**A URL means HTTP; a command after `--` means stdio.** That is the whole rule.
`--transport` is accepted because it is what another tool's users have learned to
type, and their muscle memory is not a thing to punish — but it is read as an
*assertion about the form* rather than as a way of choosing one. It is checked
against what you actually wrote and refused by name when the two disagree, so
`--transport stdio --url …` is a sentence naming which half to delete rather than
a silent discard of one of them. The third line above is that other tool's
ordering — the flag, then the name, then the URL as a second word — and it
produces the same `[[mcp]]` entry as the second line, because a second positional
*is* a URL wherever the flag sits. `--env` is refused on an HTTP server and
`--header` on a stdio one, each saying which of the two the server actually takes.

**`--scope user|project|local` says which file, where the file is yours to
choose.** `mcp add` and `plugin add` default to `user`, because that is the file
that is yours and is not committed — defaulting to `project` would put one
operator's server into a repository everyone else clones, which is a disclosure
rather than a convenience. `config set` and `config unset` have no default: with
no `--scope` they inherit the file already deciding that key, which is the only
answer that *changes* a setting instead of shadowing it with a copy somewhere
higher. `mcp edit`, `mcp remove` and `plugin remove` take no `--scope` at all and
refuse one by name — the change goes to the file that declares the entry, and a
scope chosen here would aim a position counted in one file's array at another
file's.

**`io plugin add <name>` from a marketplace declares the bundle and stops there.**
It writes the entry switched off, prints the disclosure to standard error, and
exits zero; switching it on is `/plugin`, in a session, where there is somebody to
ask. There is deliberately no `--yes`: consent to a stranger's code is not a flag
a script sets on your behalf. A path you already have is unaffected — `io plugin
add ./bundles/rust-review` behaves exactly as it did.

**`io config list` prints the origin column**, tab-separated after the value:
`user`, `project`, `local`, or `default` for a key no file names. There is no flag
to drop it. A value without the file that decided it is half an answer — that is
the whole argument of the `/config` surface — and a headless listing that left it
out would be a second, weaker truth about the same configuration.

**Only the answer goes to stdout.** `mcp list`, `mcp get`, `plugin list`,
`config get` and `config list` write tab-separated rows and nothing else, so they
pipe. Everything else goes to stderr, including a `[[plugin]]` entry that was
declared and dropped: it is not part of the list a script asked for, and it is
exactly what an operator reading that list needs to see.

**A refusal exits `1` and writes nothing.** Every refusal names what was wrong and
what is accepted instead — there is no bare "invalid argument" in this parse,
because you are at a terminal with no `--help` open and a refusal that does not
say what to type next costs you a round trip to this page.

**`io mcp add` reports whether the policy will let that server start, on stderr,
and exits `0` anyway.** An MCP server is the one piece of configuration whose
failure mode is silence: a refused entry looks exactly as valid as one that works,
and you find out on the next turn, from a run that ends before its first step. So
the entry is written and then the same `Policy::check` io-harness will ask is
asked here — naming the act, the target, and the rule and the layer that decided,
or saying the tier default did, which is a different repair. **It is a disclosure
and not a veto.** Declining to record what you typed because a policy you can edit
would currently refuse it would make your configuration file depend on the posture
at the moment of typing, so the write happens and the status stays `0`. A script
that wants the verdict reads stderr.

The two doors ask slightly different policies, and each is right about the run it
describes. `io mcp add` asks the `[policy]` section of the configuration in force
— io-harness's own defaults where a file has none, and those *ask* on `exec`.
`/mcp add` in a session asks the policy that session is actually running under:
the same section, plus the posture `Shift+Tab` is on, plus whatever you have
allowed for this session.

**Every HTTP MCP server is refused by default, and this is the paragraph to read
before filing a bug.** io-harness denies `net` unless a rule allows it —
`Policy::default()` does, a policy deserialized from a file with no `net` field
does, and all three of the postures `io setup` writes say `net = "deny"`
outright. So on almost every install:

```sh
io mcp add linear --url https://mcp.linear.app/mcp
```

writes the entry, exits `0`, and prints on stderr:

```
`linear` will not start: net `mcp.linear.app:443` is denied by the policy's own default for that act (no rule matched).
```

Nothing is broken and nothing needs undoing. The server is declared and the
boundary has simply not been told about it. Naming the host in a policy layer is
what starts it:

```toml
[[policy.layers]]
name = "mcp"
rules = [{ act = "net", effect = "allow", pattern = "mcp.linear.app" }]
```

A net rule matches with or without a port: the pattern above allows that host on
any port, `mcp.linear.app:443` allows exactly one, and `*.linear.app` works the
way a `*` works on a path. Servers are attached per turn, so the next turn is what
picks the rule up — there is nothing to re-add and nothing to restart.

A stdio server is a different question and usually a quieter one. It is checked
against `exec` on the command exactly as the file spells it, and a file written
for the sandboxed-workspace posture says `exec = "allow"`, so a server declared
after a `--` normally needs nothing added at all. A file with no `[policy]`
section is the case to know about: io-harness's own default *asks* on `exec`, and
a server is spawned before the first step of a run, with nobody there to ask — so
the preflight reports it as refused rather than as a question, which is what
io-harness will do to it.

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

**From 0.28.0 a value is chosen rather than typed.** Choosing a row used to put
its key in the prompt and leave the value to you — so setting
`policy.defaults.write` meant guessing a word out of a set the pinned dependency
has made public, and there was no way to tell a typo from an option that does not
exist. A row now descends into its own values: the three effects and the three
sandbox modes come from io-harness's own types rather than from a list here, a
model comes from the `[prices.models]` already in your file, a path comes from the
same workspace reader the composer's `@` opens, and a number comes from a ladder.
Nothing on that screen reaches the network, and there is no per-key table of
options to go stale.

**A number descends into a one-two-five ladder built around the value in force.**
1, 2, 5 at each magnitude, ordered outwards from where you are, with the value the
file currently says always present as a rung whether or not it sits on the ladder
— because a list that quietly omits your own setting is a list you cannot find it
in. The anchor is the value in force and not a default, because there is no
default to anchor on: `max_tokens` and `max_duration_secs` are absent in both of
io-harness's contract constructors and `max_steps` is 8 in one and 12 in the
other, so "the default" is not a thing this surface could read. A key no file
names ladders from 1. `app.io-cli.gates.expect_exit` is the one signed key and its
ladder runs through zero into the negatives, because a process may legitimately
be expected to exit on one.

**A horizontal arrow changes a boolean or a closed set of words where it stands**,
without opening anything — see [Keys](#keys) for why it is the arrows and not the
spacebar. Each press writes, re-reads and redraws the row from the file's own
answer rather than from an account of what was just done, and each one is
committed to the scrollback, so cycling through four values leaves four lines
saying what happened rather than one footer notice overwriting the other three.

**Every row also offers *unset it*, which removes the key rather than writing a
default's text into a file.** The distinction is the one this whole surface opens
with: after an unset the origin column says `default` and names no path, which is
io-harness's own default speaking. Writing the default's *value* instead would
attribute a crate default to a file you never wrote it in, and that is a lie a
reader has no way to detect.

**A write goes into the file already deciding the key, and the confirmation says
so before you choose.** Asking every time would cost more than the change did —
the value was chosen in one keystroke — and answering "your own file" every time
is worse than asking: it silently shadows a committed project setting with a
personal one, which is the change you are least able to see afterwards. A key no
file names has nothing to inherit and goes to your own file, and the title says
that too. *write it to another file…* is the row for moving a key between the
three scopes, and it carries the current value along so a move does not also ask
you to retype what you had.

What is still typed is only what no menu can hold — a substring, a rubric, a URL,
a command — and each of those now says what shape it wants and shows a worked
example before the composer opens. Nothing opens a bare prompt with a key in it
and no candidates any more.

Three ways in, and they differ in one thing. `/config` opens the list.
`/config <key> <value>` is the shorthand this surface has always had and asks
which of the three files to write to. `/config set <key> <value>` — and
`/config unset <key>`, and `io config set` and `io config unset` — inherit the
deciding file the way the picker does, and take `--scope user|project|local` to
override it. The change is in force from the next turn.

**Your file survives it.** The comments, the blank lines, the order you chose and
every section io-cli has no type for come back byte for byte — one value's bytes
are replaced and the rest is copied through. The write is staged in a temporary
file and renamed over the original, so a failure cannot truncate a configuration,
and the mode is preserved. **That now holds for a removal as well**: through
0.21.0, removing or moving an entry took the *next* section's comment block away
with it, and moving one into the last position of a file with no trailing newline
concatenated it onto the final value. A key whose name carries a dot — a model id,
an MCP server id — is addressed correctly too; it could only be written quoted,
and the path splitter cut it in half, which surfaced as an unexplainable "the edit
would have produced a file that does not parse".

**A project-scoped change that would widen the boundary is refused in
io-harness's own words**, and the same value is accepted in `io.local.toml` —
the rule is about which file, not which value. io-cli keeps no copy of those
rules: it writes, asks io-harness to read the file back, and restores it exactly
when the answer is no.

**From 0.28.0 the row says so before it writes**, and that is worth a second
mechanism rather than being left to the round trip, because the cost is not one
key. There are exactly five (key, value) pairs a committed `io.toml` may not
carry — `policy.defaults.exec = "allow"`, `policy.defaults.net = "allow"`,
`sandbox.allow_network = true`, `sandbox.force_floor = false` and
`sandbox.mode = "full-access"` — and io-harness's check runs *before* the file is
deserialized, so choosing one of them in a project file does not get you a
rejected setting: it gets you a configuration that no longer parses. The write is
still verified by io-harness reading it back, and still rolled back to the exact
bytes that were there. What the row adds is that the file is not written at all,
and that the refusal says the whole file is what would have been refused. On
`config set` it goes further and names the two scopes that will take the value:
`--scope local` for this checkout, `--scope user` for yourself.

**`/mcp`** shows what is configured, which servers answered this session, how
many tools each announced, how many distinct ones this session has asked for, and
the last failure. A server the session has not reached says so and is not shown as
broken. From 0.21.0 it **edits and removes** `[[mcp]]` entries, through the same
write `/config` uses: staged, read back by io-harness, and rolled back whole when
the answer is no. **From 0.28.0 it declares them too** — `/mcp add <id> -- <command>
[args…]` for a server io starts, `/mcp add <id> --url <URL>` for one it dials,
with `--env`, `--header`, `--timeout-secs` and `--scope`, read by the same parse
`io mcp add` uses so the two doors write the same bytes. That is the verb this
panel was missing: `servers::add` existed, was tested, and was called from
nothing, so a list you could prune was a list you could not grow. See [Managing
the configuration without a
session](#managing-the-configuration-without-a-session) for the grammar, which is
one grammar.

**Adding one reports whether the policy will let it start.** io-harness denies
`net` by default and every posture `io setup` writes says so, so an HTTP server
will report *will not start* for almost everyone until a `[[policy.layers]]` rule
names its host — which is not a bug and is the first thing to check. The report is
a disclosure and never a veto: the entry is written either way. It is where a
server is added, rather than at the run that first needs one, because a refused
entry looks exactly as valid as a working one and the alternative is finding out
a turn later. The same paragraph in [Managing the configuration without a
session](#managing-the-configuration-without-a-session) has the rule to write.

It offers no *disable*, because `McpServer` has no
key for one — an `enabled = false` invented here would be accepted by the file and
ignored by the harness, and a panel saying "disabled" over a running server is
worse than a panel with one fewer verb. Nor a *reconnect*: servers are attached
per turn, so the next turn is what picks up your edit.

**`/provider`** shows the `[[provider]]` array as what it is: the order a turn
tries them. From 0.21.0 you can **arrange** it — promote an entry, demote one,
remove one — which is the fallback chain io-harness has supported since its
0.27.0 and that this interface has drawn an event for without ever being able to
cause one. Reordering moves an entry with its own comments and its own keys rather
than rebuilding the array, because a chain rebuilt from io-cli's model would
silently drop whatever io-cli does not model.

**From 0.28.0 it also adds a link, and changes the model on one it already has.**
*Add a provider* offers the presets, reads the model catalogue that endpoint
actually serves and offers you one from it, then verifies the credential — in that
order, and the order is the guarantee rather than a preference: the check happens
before a single byte is written, so a rejected credential leaves your
configuration exactly as it was, and a catalogue that cannot be read is a reason
to send you to `io setup` rather than to refuse you for being offline. A new link
is appended, so it is a *fallback* and not the provider in force, and the line
that confirms it says which position it landed in — promote it if you meant it to
answer the next turn. Nothing writes a key into the file: a vendor entry is
written with no `api_key` line at all, which is what io-harness reads as "use my
own environment variable".

*Change the model* is the only key this panel edits, from the same catalogue, and
it is deliberately narrow. `kind`, `preset` and `base_url` are the link's
*identity* — an entry pointed at a different vendor is a different link, and
remove-then-add says so in words rather than leaving behind the both-bases entry
that a `preset` written over a `base_url` would be. A model change is not a claim
about a credential, so nothing is verified for one: pinging an endpoint to rename
a field would spend your money answering a question nobody asked.

**Add only offers a preset whose API-key variable is already set in your shell** —
`OPENROUTER_API_KEY`, `ANTHROPIC_API_KEY`, `OPENAI_API_KEY` — and names the
variable beside each, never anything about its contents. **That is a decision and
not a shortfall.** A credential that has to be *typed* has one flow in this
product: `io setup`, which asks for it, verifies it against the endpoint and
writes it. A second credential prompt grown inside the session loop would be a
second thing to keep correct, a second place a key can be pasted, and a second
answer to "where did my key end up". So this surface offers exactly the case that
needs no typing, and with no variable set it says so and sends you to the flow
that already exists. Export one and the row appears.

**From 0.26.0 the chain is what runs.** Since 0.21.0 that panel has drawn the
whole order while the product only ever asked its head, so a second
`[[provider]]` entry was a row on a screen and nothing else. It answers now: when
the first provider fails in a way another vendor might survive — a transport
error, a timeout, a rate limit, a 5xx — the next entry is asked, the fall-through
is committed to the scrollback as it happens, and the status line's provider field
moves to whoever actually answered. **A failure that will fail identically
everywhere does not fall through**, a bad API key above all, so a wrong credential
on the primary can never start spending at the secondary. That predicate is
io-harness's own — the same one its `Fallback` and its in-run retry ask — and
io-cli holds no opinion here and must not grow one. **This is a behaviour change
for a file that already has more than one entry**: the second one starts being
used.

`--provider` on `io exec` and `io resume` replaces the whole chain rather than
heading it. Naming a provider on the command line is saying which endpoint this
run uses, and keeping the file's fallbacks underneath it would let a run you
scoped to one vendor spend at another.

**Both of those panels could only list until 0.21.0.** The writers were there and
tested and called from nothing, while three places in this documentation said they
"add, edit, disable and remove". They genuinely write now, and the word *disable*
is gone from that sentence because `/mcp` does not offer it.

**Half of that was still true until 0.28.0, and this page said otherwise.** What
0.21.0 actually reached was edit, promote, demote and remove. `servers::add` and
`providers::add` stayed exactly where the paragraph above found their siblings —
written, tested, and called from nothing — so both panels could shorten a list
neither of them could lengthen, and this page listed "add an entry" among
`/provider`'s verbs for seven releases over a row that was never drawn. 0.28.0 is
the release that makes it true, and this is the second time the same mistake has
had to be written down: a writer with no caller reads exactly like a feature to
whoever is documenting it.

No verb here takes a row number. An entry is addressed by finding its id in the
file's own bytes, because a row on screen and a position in a file's array are
different numbers the moment anything sorts or filters, and getting that wrong
does not fail loudly — it removes a server you never named, or bills the next turn
to a vendor you did not choose.

**`/plugin`** shows the capability bundles a `[[plugin]]` entry declared: what
each one contributed, by name, and every bundle that was declared and dropped
with io-harness's own reason beside it. It has been able to remove one since
0.20.0; **from 0.28.0 it declares one as well.** The add row does not ask you to
type a path — it walks up to three directories below the workspace root, skipping
`target`, `node_modules` and anything dotted, and offers every directory that
carries a `plugin.toml`. A path typed from memory is a path that gets mistyped,
and io-harness's plugin loader has no error path: an entry naming a directory with
no manifest is *dropped*, recorded and otherwise silently absent, which is a bundle
you believe is loaded and is not. So existence is checked before the entry is
written and again on the keystroke that writes it, because a candidate can lose
its manifest between the row being drawn and the row being chosen. A directory
below the root is declared by its **relative** path, which is what makes a bundle
vendored into a repository work for everyone who clones it; one kept elsewhere is
written absolute. A bundle deeper than three directories, or outside the root
entirely, is named outright — `/plugin add <path>`, or `io plugin add <path>` from
a shell — and is refused by the same check rather than by a shallower one. See
[Capability bundles](#capability-bundles).

**From 0.29.0 the same verb also takes a name.** `/plugin add ultraship` installs
a bundle out of a marketplace you have added, and `ultraship@zeroonething/ultraship`
says which one where two carry that name — a bare name two marketplaces carry is
refused rather than resolved, because taking the first match installs code you did
not choose. `install` is accepted as the same verb. **A word is a path if it
resolves to a directory carrying a manifest, and a name otherwise**: the rule asks
the disk rather than the spelling, so one word cannot mean a directory on a machine
that has one and a marketplace bundle on a machine that does not.

Installing by name **declares the bundle switched off and shows you what it would
bring before it brings it** — see [Marketplaces](#marketplaces). Installing by
path does not: that directory is one you already have.

**`/profile`** switches to a named `[profile.<name>]` for the session, and
`--profile <name>` picks one for a single run without writing anything.

Nine keys live there, and eight tables:

| Key | Is |
| --- | --- |
| `theme` | `dark` or `light`. Absent detects from the terminal background. |
| `diff` | `unified` — the default, and what an absent key means — or `minimal`, the changed lines and the `@@` header without the context, for reviewing by file rather than by hunk. |
| `glyphs` | `unicode` or `ascii`. Absent asks the locale. |
| `plain` | `true` runs every session in plain mode. The same switch as `--plain`, which wins over it. |
| `skills` | a directory of skills for the agent. They appear in the `/` palette by name, and the agent reads them itself. Absent, it is `~/.io-cli/skills`. A leading `~` is your home directory — io-cli expands it before io-harness sees the path, because io-harness substitutes `${env:…}`, `${file:…}` and `${cmd:…}`, and a tilde is none of the three. |
| `max_parallel_reads` | how many read-only tool calls one turn may run at once. Absent, it is io-harness's own 10; `0` is clamped to 1 rather than meaning none. A `TaskContract` field with no io-harness configuration key of its own, which is why it is named here. |
| `spawn_background_after_secs` | how long a spawned child may run before it is backgrounded. Absent, a child is waited for however long it takes. |
| `detached_spawns` | whether a spawn may detach at all. Absent, it may. `false` buys a trace with every child's whole life in it, which a detached child gives up. |
| `conversational` | whether a prompt that is only a question may be answered in one completion, with no steps and no tools. Absent leaves io-harness's own classification where it is, which is what every release before 0.26.0 did; `false` opens a full run for every prompt. See [Answered without opening a run](#answered-without-opening-a-run). |
| `[app.io-cli.keys]` | the session's keys, by action name. See [Moving a key](#moving-a-key). |
| `[app.io-cli.containment]` | the caps a fan-out runs under. Absent, a session cannot decompose anything. See [The fleet](#the-fleet). |
| `[[app.io-cli.mcp]]` | MCP servers for the turn, in io-harness's own shape. Merged with the top-level `[[mcp]]`, and wins a collision of ids. |
| `[[app.io-cli.lsp]]` | language servers for this workspace. Merged with the top-level `[[lsp]]`, and wins a collision of ids. |
| `[app.io-cli.browser]` | a browser the agent may drive. Never downloaded — it is one you already have. |
| `[app.io-cli.gates]` | what "done" means for this repository: one of `command` (with `expect_exit`), `file` (with `contains`), or `rubric` (with `reviewer`, and `allow_self_review` if the judge may be the model that did the work), plus `retries`, which defaults to 1 and is report-only at 0. Naming none of the three, or more than one, is refused rather than resolved by precedence. See [Verification gates](#verification-gates). |
| `[app.io-cli.routing]` | when a run should change models, and to which: `escalate_after` with `failures` and `model`, `downshift_under` with `bytes` and `model`, each a sub-table and both optional. Absent, a run asks one model from the first token to the last. **The rules do not fire under `[app.io-cli.containment]`**, which the session says at start, on `/config`, and when `/contain on` is typed. A rule that cannot be obeyed — half a rule, a threshold of zero, or an empty model — is refused by name and leaves the run unrouted. See [Which model a run asks](#which-model-a-run-asks). |
| `[app.io-cli.prices]` | where the rates in `[prices]` came from: `source_url` names a catalogue to read instead of io-harness's default, and `source` and `models` record what the last read was and how many models it priced. The last two are written by a fetch rather than by hand. See [Where a price comes from](#where-a-price-comes-from). |

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
`~/.io-cli/IO.md` is in it as well: the guidance you want in every project, which
`/remember` writes when you pick that scope. That is one directory to copy to a
new machine, and one path to put in a bug report.

Two more paths arrive with the shipped skills. `~/.io-cli/skills/disabled/` holds
the ones that are turned off, which is a directory rather than a setting — see
[Skills](#skills). And `~/.io-cli/.skills-manifest` is where io-cli records the
bytes it last wrote for each shipped skill, so an upgrade can tell an untouched
file from one you edited. It sits in the home and deliberately *not* in the
skills directory, because every markdown file in there is offered to the model
and a state file is not a skill.

io also records in the home that it has offered to bring your setup across from
another agent tool, so that offer is made once on a first run and never again
however many times you start a session. Opening it deliberately is `/import` —
see [Bringing your setup across](#bringing-your-setup-across).

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
CI — and from 0.20.0 so are `[[plugin]]` and `[[hook]]`, which are the last two
that reached nothing. There is no longer a section of this file that a session
reads past.

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
contained or not — and since 0.17.0 every session turn carries a steer inbox as
well, so a contained turn can be steered mid-flight exactly as an ordinary one
can. Containment grants no capability and costs no steering. It is the caps a
fan-out runs under and nothing else.

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

## What the store is holding

**`/store` commits a page: what the run store costs on disk, what is already free
inside it, and what each session in it holds.** Everything on that page is
io-harness's own arithmetic — `page_size × page_count` for the file, and
`page_size × freelist_count` for the free part — read from the store rather than
computed here.

The distinction between those two numbers is the whole reason this page exists.

**A deletion does not shrink the file.** SQLite frees pages *into* the database
rather than out of it, so removing a session moves bytes from the file's size
into the free space inside it and the file on disk stays exactly the size it was.
Every store this product has ever created was made without `auto_vacuum`, so
there is no incremental reclamation either: a `VACUUM` is the only thing that
returns the space, and it is `/store compact`.

That matters because `~/.io-cli/runs.db` has held every session, run, step,
event, provider call, snapshot and restore point since 0.15.0, with no retention
policy and no rotation. Until this release there was no way to look at it, and no
way to shrink it.

Three verbs change it, and each one shows what it will do before it does it:

| | |
|---|---|
| `/store rm <id>` | remove one session and everything keyed to it |
| `/store sweep <date>` | remove every session created before that timestamp |
| `/store compact` | rewrite the database without its free pages |

**Each descends into a confirmation whose first row is "leave it".** That is the
row the cursor starts on, so the keystroke you give by reflex is the one that
changes nothing.

**A removal is final and takes the session's restore points with it.** Its
*memory* stays — the agent's durable notes belong to the workspace rather than to
the session, so removing a session unlearns nothing — but the rewind for those
turns is gone, and that is the part that bites later.

**A sweep refuses a session that still holds a resumable run**, and names it. A
date is a policy applied to sessions nobody looked at, and a crash-resumable tree
that vanished because it was old would be the worst thing this command could do.

**The sweep asks you to agree to the rule rather than to a count**, and the
reason is a gap in io-harness rather than a choice: `sweep_sessions` filters on
`sessions.created_at` and nothing exposes that column, so the set a date selects
cannot be counted until the sweep has run. The nearest substitute — a session's
first turn — is always *later* than the session itself, so a count built on it
would under-state what is about to be deleted. The figures are reported the
moment it finishes, refusals included. Filed as io-harness#216.

**Compaction is not free while it runs.** It rewrites the whole database and
needs roughly the file's own size in free disk space to do it, so it is a thing
you ask for rather than something a deletion does on your behalf. It reports the
bytes the file actually shrank by, measured, not inferred.

Nothing here happens on its own — there is no retention setting, no threshold and
no sweep at startup — and no model can reach any of it.

## Putting work back

**`/undo` is the size of the mistake.** Until this release the only instrument was
the rewind chord, which undoes a whole turn, and that is the wrong size for *this
one file went wrong*.

| | |
|---|---|
| `/undo <path>` | put one file back as it was before the run |
| `/undo step <n>` | reverse-apply what one step wrote |
| `/undo` | the whole turn — the same thing the rewind chord does |

**One file has four possible answers and they are four different sentences.** It
came back; it was *removed*, because the run had created it; nothing changed
because the previous contents were not kept; nothing changed because this run
never wrote that path. The last two both mean your file is untouched, and they
mean it for different reasons.

**Undoing a step is order-sensitive.** Reverse-applying one step's diff while a
later step's change still sits on the same lines finds context that has moved,
and io-harness leaves the file alone rather than fuzzy-matching it into
something nobody wrote. Undo the newest step first and it applies — and io says
so when it happens, because "nothing changed" without that sentence reads as a
bug.

**A restore does not know about an edit you made afterwards.** The file comes
back from the snapshot taken before the run first wrote it, and that snapshot is
not compared against what is on disk now, so a change you made by hand after the
turn is overwritten. The confirmation says so before you agree to it.

Every restore goes through the same path policy a write does, and a *removal*
asks the policy separately and refuses anything that is not an outright allow.

## Taking the work out

**`/export` writes this conversation as markdown, and `/export trace` writes one
run's canonical trace.** For the review that happens in a pull request, or a text
editor, or a message to somebody who was not there.

| | |
|---|---|
| `/export` | the conversation, as markdown |
| `/export <path>` | the same, where you say |
| `/export trace` | the last run's canonical trace |
| `/export trace <path>` | the same, where you say |

Both are written into the workspace under the session's own path policy, and
**an existing file is refused rather than overwritten** — an export is a
snapshot, and the next one you take is a different snapshot.

**The trace is written exactly as io-harness produced it.** Its whole value is
that it is canonical: io-harness leaves wall-clock stamps, measured durations, an
ephemeral sandbox path and autoincrement ids out of it so that two runs of one
case can be compared. A trace io-cli reformatted would compare against nothing,
so io-cli does not parse it, reserialise it, or pretty-print it. It is
pipe-delimited text rather than JSON, and it takes a `.txt` extension for that
reason.

A turn that never finished is written as a turn that never finished, rather than
as one the agent had nothing to say to.

## What this release is not

The configuration file has reached your terminal since 0.14.0: every section of
it bounds a session turn as it already bounded `io exec`, `/status` commits the
whole picture into the scrollback, and the ceilings in force are on the status
line beside what has been drawn against them.

**`[[hook]]` and capability bundles are applied from 0.20.0**, each with the
surface the omission was waiting on: `/plugin` for what a bundle brought and what
was dropped, and io-harness's own refusal sentence for a hook a project file may
not declare. **`[prices]` is read from 0.22.0**, which is the release that reads
the provider-call rows it prices. **A contract carries a verification criterion
from 0.24.0**, which was the last of these to be named here and was named with its
reason: it needed a surface of its own, and `/gates` is that surface. There is
still no `[verify]` section, because there is none in io-harness's schema to
apply — what a session carries comes from `[app.io-cli.gates]`, which is io-cli's
own. See [Verification gates](#verification-gates). **`[run.commit_identity]` is
*read* from 0.25.0**: it has reached a turn's contract since 0.14.0 like every
other section, and until there was a commit to author it was a value nothing in
this interface had cause to look at. See [Git](#git).

That leaves one key still unapplied, and it has a reason too: `run.templates` is
the thirteenth `[run]` key, reachable only through its own accessor. It is not a
silent omission — this is where it is named.

**A price is never invented, and a missing one is never a zero.** io-cli compiles
no rates in and estimates nothing, so an install that has connected no provider
reports tokens and no currency, and a total containing a model the table does not
price is reported as a floor. See [What it costs](#what-it-costs).

**`/import` does not bring a permission boundary across, and never will.** Another
tool spells a permission as a command line; io-harness matches a binary name. The
nearest honest translation is wider than what you granted, so io reports the
allowlist it found and writes no rule at all. Setting the boundary here is
`Shift+Tab`, `/config`, or the `io-permissions` skill — three surfaces where you
can see what you are granting. See [Bringing your setup
across](#bringing-your-setup-across).

**Git stops at your own checkout.** The seven built-ins the agent has are fixed
argvs with no remote among them, so nothing here pushes, fetches or opens a pull
request, and 0.25.0 adds no surface that does. Nothing removes a worktree or
deletes a branch either — both throw away work, and both are yours to decide. And
io-cli starts no git process of its own: the branch on the status line is a read
of `.git/HEAD`, and `/commit` is a prompt. What lifts the tools when your posture
refuses them is a permission rule, not a code path that runs git behind the
policy. See [Git](#git).

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

**The twelve document tools cannot be taken out of that tool set either**, and
six of them write. A model that reaches for `docx_write` in a session where you
never meant a document to be written is stopped by the write gate rather than by
the absence of the tool — which is what the gate is for, and why the writers are
named one by one in [Documents](#documents) rather than counted.

**An image the agent was *given* rather than asked for is not shown.** A picture
returned by an MCP tool, and a browser screenshot, both become images inside
io-harness — but through private plumbing and with no event of any kind, so
nothing reaches this program to draw.

**A skill is listed by name and never pasted.** A template is expanded by io-cli
into prompt text, so nothing but this program is involved; a skill is read by the
*model*, through a tool, under the run's own policy. Choosing one from the
palette puts `use the <name> skill: ` in your prompt and stops there. io-cli
parses no skill file: the five it ships from 0.19.0 are bodies it carries and
writes to disk, and after that they are read the same way yours are — by the
model, through the tool, under the policy.

**`io exec` runs one goal and stops, and a run that pauses is still not answered
by a machine.** An agent that asks a question about what you meant, or proposes a
plan, ends the run at exit `4` with the pause persisted in the store. That is
io-harness's behaviour and it is the right one — a machine answering a question
about intent on your behalf sends the agent down a path nobody chose — so `io
exec` parks the run and says which pause it is parked on. Answering it is a
person's job, and from 0.23.0 there is a door for that: `io resume` from a
script, `/resume` from a session. Approvals are the one pause that cannot happen
in a headless run, because they are declined rather than deferred.

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
