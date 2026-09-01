# Headless

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
without saying so. Every approval a *tool call* raises is declined, and the
refusal is fed back to the agent as an observation it can adapt to — which is
what it already does with a policy refusal.

**The provider's own endpoint is the exception, and it is not adaptable.** That
host is authorized once, before the run's first step, so a policy that puts it in
the `ask` tier refuses the run there — there is no turn yet to tell about it, and
nothing to adapt. Only an explicit `act = "net", effect = "ask"` rule matching the
provider host reaches this: the harness contributes an allow for that endpoint
from a layer of its own, so an ordinary `net`-denying posture still gets to the
model, and `io setup` writes no such rule. Worth knowing because nothing warns
first — the notice above reads `write` and `exec` and never `net`. **A run refused
this way exits `1` and not the `2` the table below gives for a boundary**: the
typed refusal is flattened to a message before an exit code is chosen. That is a
known defect rather than the contract.

### Exit status

| Code | Means |
| --- | --- |
| `0` | the run ended of its own accord |
| `1` | it never got that far — no provider, a bad credential, an unreadable configuration, a usage error |
| `2` | a boundary said no: denied, refused, or a rejected plan |
| `3` | a ceiling was reached: steps, time, tokens, or the tree's shared budget |
| `4` | the run stopped needing a human: it asked a question, proposed a plan, or was interrupted in the middle of a call |
| `5` | it ended without finishing: stalled, escalated, or cancelled |
| `6` | the work does not hold up: a gate you configured did not pass |

A ceiling is `3` and not `0` because io-harness returns one as a *successful
call* whose outcome says a limit was hit; a status read off the result alone
would call a truncated run a finished one.

**Exit `6` is new in 0.24.0 and it is the only one that is.** It says something no
other row could: the criterion you set in
[`[app.io-cli.gates]`](verification.md#verification-gates) did not pass — the tests failed, the
file was not written, the reviewer said no. **Two routes reach it.** The run ends
the way `0` ends, of its own accord, and the gate then answers failed; or
io-harness ends the run itself as `VerificationFailed`, which is a run that spent
its whole step budget and never passed the gate. The second is a ceiling by
mechanism and is deliberately not `3`: what is worth reporting is that the work
was judged and did not hold up, and `3` would move exactly the runs `6` was
invented for. It is not `1`, because nothing went wrong with the invocation; it
is not `5`, because the agent did not stall or give up; and it is emphatically
not `0`, which is the status a build script reads as permission to carry on. A
run that never had a gate configured can never return it.

**No exit code was renumbered, and `6` is the first one added since `io exec`
shipped.** `0` through `5` have meant exactly what they mean in the table above
since 0.5.0, and they mean it unchanged here: a script branching on them is a
script this release did not break. What changes is that a script branching on `0`
alone now has a sixth answer to handle, which is the point — before this release
there was no status a gated run could return that said the work was not good
enough, because there were no gates.

**Exit `4` names the pause from 0.23.0, and — for three of the four — the
invocation that answers it.** The closing line used to name the run id and
nothing else, which addressed none of the four pauses; it now names the question,
plan or call the run stopped on and the `io resume` that decides it. That release renumbered nothing and added
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
keeps in the `json` column of its `run_events` table:

```json
{"run_id":41,"step":2,"depth":0,"event":"step","decision":"wrote src/lib.rs","tool_call":"write_file","tokens":812,"changed":true}
```

The variant's fields sit beside the envelope's rather than under a key of their
own. Because io-cli forwards the harness's type rather than modelling one of its
own, every event kind reaches the stream — including every kind the interactive
renderer has no way to draw. **There is no timestamp**: `RunEvent`
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
| `--answer <text>` | answer the question the run stopped on — all of it, when the run stopped on several asked at once |
| `--plan <verdict>` | `approve`, `revise` or `cancel` the plan the run proposed |
| `--correction <text>` | what the plan should do differently; required by `--plan revise` and refused without it |
| `--recovery <decision>` | `retry`, `abandon` or `completed` — what happened to the call the run was interrupted in the middle of |
| `--account <text>` | what that call returned; required by `--recovery completed` and refused without it |
| `--goal <text>` | what the run was asked to do, for a run whose goal cannot be recovered |
| `--json` | write the resumed run's events, and `--list`'s rows, as newline-delimited JSON |
| `--policy <posture>` | the posture for the rest of this run; defaults to the one the run itself recorded |
| `--provider <name>` | `openrouter`, `anthropic` or `openai` — take the credential and model from the environment |

**A batched ask is one row, one id and one `--answer`.** An agent can ask several
things at once, and io-harness parks the whole ask as a single `pending_questions`
row answered through a single `question_id` — so the pause is still
`waiting_on: "question"` and there is no per-question flag to reach for. What
`--list` adds is a `questions` count on the row, because the one thing you cannot
see from `waiting_on` alone is how much that single answer has to cover: it is the
number of questions for a question row and `null` for the three pauses that are
not questions.

So one text answers the lot, and the refusal that names the pause says so —
number your answers to match the questions and send them as one string:

```sh
io resume --list --json | jq -r 'select(.questions > 1) | .run_id'
io resume 41 --answer "1. the parser that is already there  2. no, leave the CLI alone"
```

**The questions themselves are not on this command line.** `--list` counts them —
`3 questions` on the row, and the `questions` field in `--json` — and says so
rather than pasting the numbered ask into a one-line detail, which reads as if the
first question were the whole thing. To see them, resume the run at a terminal;
`io` draws them one at a time there. What the door cannot do is take the ask
apart: io-harness records one reply against one row, and the per-question
breakdown is written only by a responder inside the running process.

**One limitation to know before you script against it.** A *single* question that
takes several answers loses that fact in the store: io-harness's
`PendingQuestion` has no column for it and the singular writer records none, so a
lone multi-select that parks and is resumed comes back as a pick-one. A batched
ask keeps it, because a batch carries its questions whole. This is upstream of
io-cli and is stated rather than papered over with a default that would read as a
fact.

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
you](resume.md#when-a-run-stops-for-you) for why it cannot be carried on.

The exit status is the table above: a resumed run that pauses again exits `4`
naming the new pause, and `io resume --list` exits `0` whether or not it found
anything.

### Managing the configuration without a session

`io mcp`, `io plugin` and `io config` are new in 0.28.0, `io skill` joins them in
0.30.1, and they do from a shell what `/mcp`, `/plugin`, `/config` and `/skills`
do inside a session. They open no session, start no run and touch no store — a
configuration listing that had to build a task contract before it could print is a
listing nobody can put in a script — and they are answered before the terminal
check, so `io config list` works in CI where an interactive session is refused for
having no terminal.

`io mcp probe` is the exception to "start no run", and deliberately: it is the one
verb here that finds out rather than reports. It starts the server the way a run
would — under the same policy check, so a server your policy refuses is refused
here too, naming the rule and the layer — completes the MCP handshake, asks what
tools it offers, and shuts it down again. Disabled, refused, unreachable, timed
out and answering are five different sentences rather than one, because the thing
you want to know when a server is not working is *which* of those it is.

```sh
io mcp add semlith -- semlith --store /path/to/.semlith mcp
io mcp add linear --url https://mcp.linear.app/mcp --header 'Authorization=Bearer ${env:LINEAR_TOKEN}'
io mcp add --transport http linear-server https://mcp.linear.app/mcp
io mcp list
io mcp get semlith
io mcp edit semlith --timeout-secs 30
io mcp disable semlith                        # still configured, not started
io mcp enable semlith
io mcp probe semlith                          # start it and ask what it offers
io mcp remove semlith

io skill add ./my-skill.md
io skill add ./my-skill/SKILL.md              # installed as my-skill.md, from its own name
io skill list
io skill remove my-skill

io plugin add ./bundles/rust-review
io plugin add ultraship                       # or a name from a marketplace
io plugin install ultraship                   # the same verb
io plugin search review
io plugin list
io plugin remove ./bundles/rust-review        # the directory
io plugin remove rust-review                  # or the name of a bundle you declared

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

**`io plugin remove` takes a directory or a bundle's name.** The path is read
first and against the disk, so `io plugin remove ./bundles/rust-review` means what
it has always meant; only when no configuration file declares that directory is
the word matched against the names of the bundles you have declared — loaded,
switched off and failed to load alike. Two of one name are refused with both
directories printed, because taking whichever was found first would delete a
`[[plugin]]` entry you never pointed at and nothing would say so. Until 0.33.0
only the path was read, while `plugin add` printed a line telling you to remove it
by id.

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

---

[README](../../README.md) · [All guides](../CAPABILITIES.md) · [What you may depend on](../CONTRACT.md)
