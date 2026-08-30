# While it works

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
apart. See [What it costs](accounting.md#what-it-costs).

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

**A line can be changed and taken back, from 0.30.0.** `/memory`, choose an
instruction file, and it lists the bullets in it with their line numbers. Picking
one puts its text in the prompt: edit it there and choose *replace it with what is
in the prompt*, or choose *forget it*. Both splice the file — the indent you used,
the `*` you wrote where io writes `-`, a `\r\n` from a Windows checkout and a last
line with no newline after it all survive, because a rewrite assembled from parsed
lines normalises all four in silence. A note that changed underneath you since the
list was drawn is refused rather than overwritten.

One case is called out on the row itself: `/import` brings another tool's whole
instructions file across as a *single* bullet with the document beneath it, and
markdown counts all of that as one list item. Forgetting that note removes the
bullet and leaves the document, so the row says how many lines it will leave
behind. Before 0.30.0, `/remember` could add a line and nothing anywhere could
change or remove one.

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

---

[README](../../README.md) · [All guides](../CAPABILITIES.md) · [What you may depend on](../CONTRACT.md)

## The viewport takes the rows a surface needs

`io` draws into an inline viewport — a few rows at the bottom of your terminal,
with everything finished committed above them into the terminal's own scrollback,
where your search and your selection already work. That viewport was a fixed eight
rows from 0.1.0 until 0.32.0, and a surface needing more quietly lost the
remainder.

**It is now the size of what it has to show.** A question overlay asks for its
offers and its composer, a plan overlay for its steps and its footer, the queue for
one row per message it is holding, and a picker for its list. When the surface
closes the rows go back.

**The ceiling is your terminal's height less four rows.** Those four are not a
ration — a surface may take the screen when it needs it — they are the exchange the
surface is *about*, kept visible. A question filling the terminal to the last row
is a question with no readable reason for being asked. On an 80×24 that leaves a
twenty-row viewport, which holds a twelve-step plan with its footer, or a
five-choice question with its context line and somewhere to type.

**A surface that cannot have what it asked for degrades rather than overflowing.**
Growth is a request. Every one of them elides with a count — `⋯ 3 more` — so a
list cut short never looks like a list that ended.

The scrollback above is untouched by any of this. Committed rows are the
terminal's, and growing or shrinking the viewport neither repeats them nor loses
them.

## A message typed mid-turn reaches that turn

Type while the agent is working and your line is queued rather than sent. The
queue is drawn above the composer, every message with its position.

**Since 0.32.0 those messages are delivered into the running turn**, at its next
step boundary, and each one is recorded in your transcript as it goes. Before, they
waited for the turn to end and then ran as turns of their own — so a correction
typed thirty seconds into a ten-minute turn arrived nine and a half minutes late,
against work it was meant to change.

`/steer` still sends immediately, and behaves exactly as it did.

**A message that reaches no further step boundary is not lost.** If the turn
finishes, is interrupted, or errors before reading it, it runs as its own turn
afterwards and the session says which of the two happened.

## The token figure while a step is streaming

Providers do not bill per chunk, so nothing in the event stream carries a running
token count. Until 0.32.0 the figure simply sat still for the whole of a turn while
the clock beside it ran.

It now moves, as an **estimate** taken from the streamed text, and it is written
with a leading tilde — `~1.2k tok` — so it can never be read as settled. When the
step commits, the provider's own number replaces it and the tilde goes. The two are
never added together.

## The agent asks everything in one surface

When the agent needs to know something, the offers and a row that takes your own
words are one list. Move to an offer and `Enter` sends it verbatim; move to the
last row and a composer unfolds under it and takes what you type. There is no key
that moves between two panes, because there are not two panes.

**An agent can ask several things at once, and they arrive as one overlay.** One
question is on the screen at a time — its question line, its context, its offers,
its free-text row: exactly the surface a single question has, so answering one
does not change because four others exist. Deciding it moves to the next question
that has not been decided, and deciding the last one sends the whole batch. Two
lines of the head say where you are: which question of how many, and what this one
was already decided as if you have come back to it.

**`PgUp` and `PgDn` walk the batch**, in either direction, at any point before it
is sent. A question you have already decided re-opens with your own answer back in
the composer, so changing your mind is retyping or re-sending your own words
rather than a second kind of screen with a second set of keys. There is no review
pane and no submit key: the answers are already on the screen you typed them into,
one page-key apart, and a second rendering of them is a second thing that can
disagree with the first.

**Nothing is sent until every question is decided.** io-harness commits a batch
only when every entry has an answer, so four out of five parks the run exactly as
thoroughly as none. Declining is a decision — `Esc` decides the question on the
screen as *nobody here can answer this* and moves on, and the run parks, which is
the only thing `Esc` has ever promised.

A batch of one is a batch of one. The overlay carries no batch chrome, `PgUp` and
`PgDn` do nothing, and every key does what it did before.

### An offer can say more than its label

A choice may carry a **description** — one sentence saying what taking it means —
and a **preview** — a short block showing what taking it would do. They are drawn
differently because they are different things.

A description is always on the screen, on a row of its own under the label it
explains, because someone comparing five offers needs all five sentences at once.
A preview is a block, and five blocks at once is a wall nobody reads — so it
unfolds beneath the offer under the marker, one at a time, and folds again when
the marker moves. It is marked as quoted words, the same way this product draws a
model's own blockquote, because a preview is somebody else's text and not io's.
On an offer carrying both, the open preview sits between the label and its
description.

`Enter` on an offer whose preview is open still answers with that offer. The
question is which row holds the marker, never whether anything is unfolded.

io-harness bounds a preview before it sends one and io-cli does not restate those
bounds as its own: what arrives is drawn. What is bounded here is the drawing —
the block asks for the rows it wraps to, and the viewport clamps that to what your
terminal can spare.

### A question that takes several answers

Where a question accepts more than one, each offer is drawn with a box in front of
it and `Space` marks and unmarks the one under the marker. `Enter` sends the
marked set; with nothing marked it sends the offer you are looking at, because an
empty answer is not an answer — it is information the agent did not have and would
now believe. What crosses the wire is io-harness's own spelling of a multiple
answer, not a joiner written here, so two interfaces answering one question
produce the same text.

The box is drawn on every offer from the moment the list opens, marked or not. A
column that appeared only once something was marked would be a list that shifts
sideways as you use it, and — worse — a question that takes several would look
exactly like one that takes one until you had already pressed `Enter`.

**A known limitation, and it is upstream.** io-harness's store has no column for
"this question takes several answers" on a *singular* ask, and the singular writer
records none — so a single multi-select question that parks and is resumed later
comes back as a pick-one. A batched ask keeps it, because a batch carries its
questions whole. Nothing io-cli can read recovers the flag for the singular case,
and defaulting it either way would be io-cli stating as fact something no longer
on disk.
