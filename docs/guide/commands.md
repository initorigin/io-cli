# Commands

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
| `/profile` | switch to a named profile for this session, or create, remove and clear one |
| `/contain` | run turns contained, so the agent can fan out: on, off, or ask |
| `/setup` | run the first-run wizard again |
| `/exit` | leave |

**this turn**

| Command | Does |
| --- | --- |
| `/model` | change the model the next turn is sent to |
| `/effort` | how much reasoning the next turn buys: low, medium, high, or off |
| `/undo` | put work back: `<path>` for one file, `step <n>` for one step, bare for the run |
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
| `/skills` | every skill, shipped or yours: what it is for, whether it is on, and its file; add and remove one |
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
| `/memory` | what io remembers: the instruction files and the agent's own notes, each editable |
| `/mcp` | the MCP servers configured, what this session has seen of each, and whether one answers |
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

**0.30.0 closes the rest of it, and adds no command either.** A skill can be put
there and taken away (`/skills add`, `/skills remove`, and the same words from a
shell as `io skill …` from 0.30.1, which is where that door actually opened); an
instruction note can be changed and forgotten (`/memory`); a profile can be
created, removed and cleared (`/profile create|remove|clear`); an MCP server and a
capability bundle can each be switched **off without being removed**, which until
now was the one state you could see and not write; and a configured server can be
started on request to report whether it actually answers. After this release there
is nothing io manages that you have to open a file to change.

Which of them is typed and which is a row is not arbitrary. The verbs `add`,
`edit`, `remove`, `get`, `enable`, `disable` and `probe` on `/mcp`, `add` and
`remove` on `/plugin`, `add`, `list` and `remove` on `/skills`, and `set` and
`unset` on `/config` are read by the same parse `io mcp`, `io plugin`, `io skill`
and `io config` are, so a line typed in the composer and the same line typed at a
shell produce identical bytes rather than two readings that agree today. A bare
`/mcp` or `/plugin` still opens its panel, because a
panel is a better answer in a session than a text dump is, and `/config <key>`
still answers what that key is set to without writing anything. `/provider`'s two
new verbs are rows on its panel and never words you type: which link to add is
chosen from what your shell can already authenticate, and a list is the only
honest way to ask that. See [Without leaving the
session](configuration.md#without-leaving-the-session).

`/effort` is new in 0.26.0 and sits under **this turn** beside `/model`, because
the two are the same question asked twice: which model the work goes to, and how
much thinking it is worth buying from it. It is a posture rather than a one-shot —
the level it sets holds for that turn and for every turn after it until you change
it — and a bare `/effort` reports the level in force and changes nothing.
`/effort off` is not a fourth level below `low`: it goes back to sending no
reasoning field at all, which is what every release before this one sent. See [How
much reasoning a turn buys](the-session.md#how-much-reasoning-a-turn-buys).

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
the message. See [Git](git.md#git).

`/plugin` is new in 0.20.0 and sits beside those two because it is the third
surface of the same kind: something a configuration file declares by name, whose
effect on the session is otherwise invisible. See [Capability
bundles](plugins.md#capability-bundles).

`/import` is new in 0.21.0 and is last in the group because it is the one command
here you use once: the others are returned to for the life of the install. It
writes files, which is the whole reason it is under **configure**. See [Bringing
your setup across](import.md#bringing-your-setup-across).

`/gates` is new in 0.24.0 and is under **configure** for that same reason and not
because of what its first screen shows. It opens on the criterion in force and the
last turn's verdict, which reads like an inspection — and then writes
`[app.io-cli.gates]`, which is the one thing **inspect** promises it never does.
That promise is worth more here than it is for a server list: a gate set by
accident does not merely change what the next turn talks to, it can spend a whole
extra turn against a real model deciding the first one was not finished. It sits
before `/import` because it is one you come back to. See [Verification
gates](verification.md#verification-gates).

`/resume` does more than reopen from 0.23.0: each row says what that session's
last run stopped on, and choosing one answers it. No command was added for that —
the one that was already there was extended. See [When a run stops for
you](resume.md#when-a-run-stops-for-you).

`/cost` and `/stats` are new in 0.22.0 and are two commands rather than one
because they are two questions: `/cost` says what the work cost, `/stats` says
whether it worked. See [What it costs](accounting.md#what-it-costs).

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

**Eleven of these run while a turn is in flight and the rest are refused**, and
the rule is what a command does rather than how harmless it looks. `/config`
joined that first set in 0.33.0, in its bare form only: `/config <key>` and
`/config <key> <value>` descend toward a write and keep the refusal. The whole
list, and the reason each side of it is where it is, is on
[Keys](keys.md) — one place, so the two cannot come to disagree.

`/copy` uses OSC 52, so it reaches the clipboard of the machine you are *sitting
at* rather than the one you are ssh'd into. Nothing acknowledges an OSC 52 write:
the line it prints says what was sent and how large it was, never that it
succeeded. Inside tmux it needs `set -g set-clipboard on`, and some terminals
refuse a large payload without saying so.

`/theme` and `/model` change this session only and say so. Making a choice
permanent is `io setup`.

---

[README](../../README.md) · [All guides](../CAPABILITIES.md) · [What you may depend on](../CONTRACT.md)
