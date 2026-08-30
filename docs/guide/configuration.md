# Configuration

io-cli has no configuration parser. io-harness owns discovery and layering, and
io-cli's own settings live in the `[app.io-cli]` section that io-harness
deliberately does not validate. See [`docs/config.example.toml`](../config.example.toml).

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

**A horizontal arrow opens a row's values, with the marker on the value in
force.** `Left` and `Right` do what `Enter` does, and they open on where you
already are so you can see it before you move; `Enter` on a value is the
confirmation, and the write goes into the file the descent names.

**No arrow key writes a configuration file, and until 0.33.0 one did.** `Left`
and `Right` on a boolean or a closed set of words used to step to the next value
and write it into a scope file on the keystroke — one press, one write, nothing
asked. It was the only unconfirmed write in this product reachable from a bare
arrow key, and it is the reason `/config` could not be opened while a turn was
running at all. Removing the write rather than guarding it is what made the bare
list safe enough to report mid-turn; see [Keys](keys.md) for the commands that now
answer while the agent works.

**The price refresh is one descent below `prices.as_of`, and it used to be the
last row of the bare list.** There it made a keystroke on `/config` read the
network, write a scope file and reassign the configuration a running turn was
holding — which is a list that acts, on a surface whose whole job is saying what
is in force. It is now where the act belongs: choose `prices.as_of`, the date the
refresh writes, and the descent offers *leave it* at the top and the re-read under
it, with the date it last read beside it. Row 0 declines and nothing happens,
which is what row 0 does on every confirmation in this product. What the refresh
then shows you before it writes is unchanged — see
[What it costs](accounting.md).

`prices.as_of` is still the one key you cannot type a value into. A date typed by
hand is a claim about a fetch that never happened, so `/config prices.as_of
<value>`, `io config set prices.as_of` and every other typing door refuse it — and
they refuse it in the same words, because a key one door writes and another will
not is the asymmetry this surface exists to remove. The descent offers the fetch
instead of the date.

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
session](headless.md#managing-the-configuration-without-a-session) for the grammar, which is
one grammar.

**Adding one reports whether the policy will let it start.** io-harness denies
`net` by default and every posture `io setup` writes says so, so an HTTP server
will report *will not start* for almost everyone until a `[[policy.layers]]` rule
names its host — which is not a bug and is the first thing to check. The report is
a disclosure and never a veto: the entry is written either way. It is where a
server is added, rather than at the run that first needs one, because a refused
entry looks exactly as valid as a working one and the alternative is finding out
a turn later. The same paragraph in [Managing the configuration without a
session](headless.md#managing-the-configuration-without-a-session) has the rule to write.

**It switches a server off and back on**, which through io-harness 0.69.0 it
could not: `McpServer` carried no key for it, and an `enabled = false` invented
here would have been accepted by the file and ignored by the harness. io-harness
0.70.0 made `enabled` a real field, honoured before the server is spawned,
dialled or even checked against the policy, so 0.29.0 drew the state without a
writer for it and 0.30.0 adds both halves — a surface that can switch a server
off has to be able to switch it back on. `io mcp disable` and `io mcp enable` are
the same edit from a shell. There is still no *reconnect*: servers are attached
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
[Capability bundles](plugins.md#capability-bundles).

**From 0.29.0 the same verb also takes a name.** `/plugin add ultraship` installs
a bundle out of a marketplace you have added, and `ultraship@zeroonething/ultraship`
says which one where two carry that name — a bare name two marketplaces carry is
refused rather than resolved, because taking the first match installs code you did
not choose. `install` is accepted as the same verb. **A word is a path if it
resolves to a directory carrying a manifest, and a name otherwise**: the rule asks
the disk rather than the spelling, so one word cannot mean a directory on a machine
that has one and a marketplace bundle on a machine that does not.

Installing by name **declares the bundle switched off and shows you what it would
bring before it brings it** — see [Marketplaces](plugins.md#marketplaces). Installing by
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
| `skills` | a directory of skills for the agent. They appear in the `/` palette by name, and the agent reads them itself. Absent, it is `skills/` under the home in force — `~/.io-cli/skills` unless `$IO_CONFIG` or `$IO_CONFIG_HOME` moved it. A leading `~` is your home directory — io-cli expands it before io-harness sees the path, because io-harness substitutes `${env:…}`, `${file:…}` and `${cmd:…}`, and a tilde is none of the three. |
| `max_parallel_reads` | how many read-only tool calls one turn may run at once. Absent, it is io-harness's own 10; `0` is clamped to 1 rather than meaning none. A `TaskContract` field with no io-harness configuration key of its own, which is why it is named here. |
| `spawn_background_after_secs` | how long a spawned child may run before it is backgrounded. Absent, a child is waited for however long it takes. |
| `detached_spawns` | whether a spawn may detach at all. Absent, it may. `false` buys a trace with every child's whole life in it, which a detached child gives up. |
| `conversational` | whether a prompt that is only a question may be answered in one completion, with no steps and no tools. Absent leaves io-harness's own classification where it is, which is what every release before 0.26.0 did; `false` opens a full run for every prompt. See [Answered without opening a run](the-session.md#answered-without-opening-a-run). |
| `[app.io-cli.keys]` | the session's keys, by action name. See [Moving a key](keys.md#moving-a-key). |
| `[app.io-cli.containment]` | the caps a fan-out runs under. Absent, a session cannot decompose anything. See [The fleet](fleet.md#the-fleet). |
| `[[app.io-cli.mcp]]` | MCP servers for the turn, in io-harness's own shape. Merged with the top-level `[[mcp]]`, and wins a collision of ids. |
| `[[app.io-cli.lsp]]` | language servers for this workspace. Merged with the top-level `[[lsp]]`, and wins a collision of ids. |
| `[app.io-cli.browser]` | a browser the agent may drive. Never downloaded — it is one you already have. |
| `[app.io-cli.gates]` | what "done" means for this repository: one of `command` (with `expect_exit`), `file` (with `contains`), or `rubric` (with `reviewer`, and `allow_self_review` if the judge may be the model that did the work), plus `retries`, which defaults to 1 and is report-only at 0. Naming none of the three, or more than one, is refused rather than resolved by precedence. See [Verification gates](verification.md#verification-gates). |
| `[app.io-cli.routing]` | when a run should change models, and to which: `escalate_after` with `failures` and `model`, `downshift_under` with `bytes` and `model`, each a sub-table and both optional. Absent, a run asks one model from the first token to the last. **The rules do not fire under `[app.io-cli.containment]`**, which the session says at start, on `/config`, and when `/contain on` is typed. A rule that cannot be obeyed — half a rule, a threshold of zero, or an empty model — is refused by name and leaves the run unrouted. See [Which model a run asks](providers.md#which-model-a-run-asks). |
| `[app.io-cli.prices]` | where the rates in `[prices]` came from: `source_url` names a catalogue to read instead of io-harness's default, and `source` and `models` record what the last read was and how many models it priced. The last two are written by a fetch rather than by hand. See [Where a price comes from](accounting.md#where-a-price-comes-from). |

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
That directory is the `skills/` of the home in force — the same home your
`io.toml` and `IO.md` are in — so `$IO_CONFIG` or `$IO_CONFIG_HOME` moves all
three together, and io reads and writes skills in whichever one is in force.
`~/.io-cli/IO.md` is in it as well: the guidance you want in every project, which
`/remember` writes when you pick that scope. That is one directory to copy to a
new machine, and one path to put in a bug report — as long as you have named no
location of your own, which splits it in two, as the resolution order below
says.

Two more paths arrive with the shipped skills. `~/.io-cli/skills/disabled/` holds
the ones that are turned off, which is a directory rather than a setting — see
[Skills](skills.md#skills). And `~/.io-cli/.skills-manifest` is where io-cli records the
bytes it last wrote for each shipped skill, so an upgrade can tell an untouched
file from one you edited. It sits in the home and deliberately *not* in the
skills directory, because every markdown file in there is offered to the model
and a state file is not a skill.

io also records in the home that it has offered to bring your setup across from
another agent tool, so that offer is made once on a first run and never again
however many times you start a session. Opening it deliberately is `/import` —
see [Bringing your setup across](import.md#bringing-your-setup-across).

The file is found in this order, which is io-harness's and is unchanged:
`$IO_CONFIG`, else `$IO_CONFIG_HOME/io.toml`, else `$XDG_CONFIG_HOME/io/io.toml`
or `~/.config/io/io.toml`, and `%APPDATA%\io\io.toml` on Windows. What 0.15.0
changed is that io-cli sets `IO_CONFIG_HOME` to its own home before io-harness
resolves anything, so the second rung is the one that answers when you have named
no location yourself. Set `IO_CONFIG` or `IO_CONFIG_HOME` and io-cli sets nothing
and moves nothing — that location is yours. A project's own `io.toml` and a
gitignored `io.local.toml` layer on top of whichever file was found.

**That is also where the home splits, and the split is deliberate.** `io.toml`,
the run store beside it and `IO.md` follow the variable, because all three are
resolved from the configuration path in force. The skills directory,
`.skills-manifest`, `marketplaces/` with the staging directory a clone is
assembled in, and the session lock are built from `~/.io-cli` whatever the
variables say: a marketplace clone is io-cli's own cache of other people's
repositories rather than part of your configuration, and following the variable
there would leave every clone already fetched invisible. So under a home you
named yourself there are two directories rather than one, and both go in the
copy and in the bug report.

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
it](accessibility.md#reading-it-without-seeing-it).

---

[README](../../README.md) · [All guides](../CAPABILITIES.md) · [What you may depend on](../CONTRACT.md)
