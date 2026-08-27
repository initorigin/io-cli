# Changelog

All notable changes to this project are documented here. The format follows
[Keep a Changelog](https://keepachangelog.com/en/1.1.0/) and the project uses
[Semantic Versioning](https://semver.org/spec/v2.0.0.html), staying on `0.x`.

## [Unreleased]

## [0.20.0] - 2026-08-27

A directory somebody else wrote can add to your session, and you can see exactly
what it added.

**Capability bundles load, and `/plugin` says what each one brought.** A bundle
is a directory with a `plugin.toml`, named by a `[[plugin]] path = "..."` entry
in a configuration file — a declaration and never a scan, so nothing loads by
being present on disk. One can contribute six kinds of thing at once: skills,
prompt templates, `[[agent]]` definitions, `[[mcp]]` servers, `[[hook]]` tables
and policy layers. `/plugin` lists what loaded and what each bundle contributed,
and under that the row it really exists for: every bundle that was declared and
dropped, carrying io-harness's own sentence for why. That loader has no error
path — a dropped bundle is recorded and otherwise silently absent while every
other one loads, which is how a bundle you believe is running stays gone for a
week.

**Which file declared a bundle decides what it may contribute.** A bundle named
in the project-scoped `io.toml` — the file a `git clone` delivers — may
contribute skills, templates, agents and policy, and may not contribute hooks or
MCP servers, because both run a program on this machine. One that tries is
refused **whole**: it contributes nothing, not the half that would have been
safe. The same directory named from `io.local.toml` or the user file loads
completely, so the rule is about which file, not about the bundle. Its policy may
only **deny** as well: a `[policy] defaults` table is refused by name and any rule
whose effect is not `deny` drops the bundle, so the worst a bundle can do to your
boundary is narrow it. An id matches `[a-z0-9][a-z0-9-]{0,31}`, and every agent
name, server id and layer name it contributes is namespaced to `<bundle>__<name>`
by io-harness — which is the string `/plugin` draws, because it is the one a
refusal, a call and a spawn all use.

**A bundle's hooks are the one contribution io-cli cannot itemise, and the row
says so rather than being left out.** io-harness's `Hook` type is private and
there is no accessor, so `/plugin` can report that a bundle contributed hooks and
nothing about what they run. An omitted row would read as a bundle with no hooks,
which is the false reading, on the contribution kind that runs programs.

**`[[hook]]` tables run.** They were parsed and inert before this — a file asking
for every event to be appended to `audit.jsonl` produced an empty file and no
error — and they now run in a session turn and in `io exec` from the same call. A
hook either appends one JSON line per event to a file or runs an argv array,
never a shell string. `on` names the events to observe and empty means all of
them; `at = "before_tool"` puts it in front of a tool call, with `tools` to
filter which calls it sees; the two are mutually exclusive and an `at` hook must
have a `run`. `on_failure` decides what a failure costs: `continue` carries on,
`cancel` ends the turn at the next step boundary and leaves the run resumable,
`refuse` turns that one tool call back. `timeout_ms` is 5000 when absent.

**A project-scoped file may not declare `[[hook]]`, and that refusal is now
readable.** io-harness refuses the whole configuration rather than dropping the
table, because a hook runs a command on this machine and `io.toml` arrives with a
`git clone`. There is no `Config` to be had and `io` genuinely cannot start —
what changed is the words. io-harness's own sentence names the key, the reason
and the two files that may carry it, and it is printed whole under a line saying
which file was being read. Before this it arrived as a bare error string from a
program that had already exited, against a repository the operator had just
cloned.

**The fleet has names.** A child is drawn by the address it was spawned under —
the `as` argument, or a derived one like `reviewer#42` — with its roster role
beside it, instead of by a run number nobody chose. A message one agent sent a
named sibling is shown in the tree with its body, which is the case a run number
told you nothing about. And a child that detached rather than being waited for
can be selected and attached to: a parent that stops waiting is not a parent that
stops the work, and that child was a row you could read and could not reach. A
queued child is still a count and still has no address, because io-harness names
a child when it admits one.

**Documents: twelve tools, and six of them write.** io-cli turns on io-harness's
`documents` feature — `xlsx`, `docx`, `pptx`, `pdf`, `barcode` — so the agent
gains `xlsx_sheets`, `xlsx_read`, `docx_read`, `pptx_read`, `pdf_read` and
`barcode_decode` to read with, and `xlsx_write`, `xlsx_set_cell`, `docx_write`,
`pdf_write`, `pdf_watermark` and `pdf_fill_form` to write with. `xlsx_write`
replaces a file that already exists. Every one of them passes the policy gate,
the approval prompt and the refusal rendering any other read or write passes —
and io-cli cannot take a tool out of io-harness's workspace tool set, which is
the ground `view_image` was disclosed on and the reason the six writers are named
here one by one rather than counted. The reader is chosen by the tool the model
called and not by the file's extension.

**What the document tools do not do is in the README**, and is worth reading
before pointing one at a file that matters. Word is generate-and-read with no
edit in place, so a read-then-write drops comments, content controls, fields and
vendor extensions; PowerPoint is read-only; PDF text extraction is best-effort
about reading order, and a scanned page comes back with empty text rather than an
error; `xlsx_set_cell` preserves the rest of a workbook in practice rather than
by guarantee, and not for chart-, pivot- or macro-heavy files. There is no OCR
and no barcode generation.

**`documents` costs 159 transitive crates and pins `image` down a patch.**
`barcode` is `rxing`, and every published `rxing 0.9.x` requires `image` at
exactly `=0.25.8`; this crate asks for `^0.25` and the lockfile stood at 0.25.10,
which cargo cannot reconcile, so the lock is pinned to 0.25.8 — the one version
both requirements accept. A `cargo update` that lifts `image` again will fail to
resolve until `rxing` relaxes. It is written here because an operator otherwise
meets it in a red resolve rather than in a release note.

**`/plugin` is the twenty-seventh command**, and it is in the **configure**
group beside `/mcp` and `/provider` because it is the third surface of that kind:
something a configuration file declares by name whose effect on the session is
otherwise invisible. Configure goes to seven.

## [0.19.0] - 2026-08-27

Ask for a thing io can do, in your own words, and have it happen.

**Five skills ship with io and answer the asks nobody should have to learn a
command for.** "Stop asking me before every write in this repository" is
`io-permissions`; "add the GitHub MCP server" is `io-mcp`; "point this at a
local model instead" is `io-provider`; "remember that we use pnpm here" is
`io-remember`; "update io" is `io-update`, which checks the published Release
and proposes the exact `install.sh` line rather than replacing the binary behind
you. Every one of them ends in a change you see before it lands — a proposed
edit, shown as a diff, gated by the same policy as anything else.

**They are ordinary files in `~/.io-cli/skills`, beside your own.** `SKILL.md`
bodies and nothing more clever than that: open one, edit it, fork it, delete it.
The model is offered each skill's name and description on every turn and reads
the body through io-harness's own `read_skill` tool, under this session's
policy. An upgrade brings forward the ones nobody has touched, leaves a file you
edited exactly as it is and says so, and never resurrects one you turned off.

**`/skills` is where all of it is visible**, and turning a skill off is moving
its file into `skills/disabled/` — a directory io-harness's discovery walks past
because it holds no `SKILL.md`. `/skills` does the move with a keystroke, it
takes effect on the next turn, and it works on your own skills too. There is no
`enabled` flag anywhere, because a second list disagreeing with the filesystem
is how a product grows two sources of truth.

**`/mcp` and `/provider` moved from the inspect group to the configure group**,
which matters to anyone who finds a command by scanning `/help` rather than by
typing it. Both open with a list and both go on to add, edit, disable and remove
entries in the configuration file, which is what configure means and what
inspect promises it does not do. The commands, their keys and their screens are
unchanged; only where they are listed is. Inspect goes to nine and configure to
six, with `/skills` the twenty-sixth command.

**Two guards you can watch work.** A skill of yours already answering to a
shipped name is never overwritten: io-cli installs four files instead of five
and names the one it withheld and the file that claimed the name — because
io-harness addresses a skill by name and two of one name ends every turn of the
session before the first completion. And io-harness accepts at most 64 skills in
a directory, rejecting the whole set rather than trimming it, so io-cli counts
before it writes, stops short of the ceiling and says how many it withheld. An
install that cannot write at all is reported and the session runs anyway.

## [0.18.0] - 2026-08-26

Tell it to remember something, and it is still remembered tomorrow.

**`/remember` writes a line into a markdown file you can open, read, edit, diff
and delete.** The scope is chosen at the moment it is written, because the
difference between the three is whether it gets committed: `AGENTS.md` for what
the team should share, `AGENTS.local.md` for what only this checkout should know,
and `~/.io-cli/IO.md` for what is true of every project. Nothing here is a store
nobody can inspect — these are files, and io-harness has read `AGENTS.md` since
its 0.45.0.

**A remembered line takes effect on the next turn, with nothing restarted.** The
configuration is discovered again for every turn rather than once at startup, so
an edit you make in your own editor counts too. A file that stops parsing no
longer ends the session: the last configuration that read cleanly stays in force,
you are told which file refused and why, and the repaired file is picked up with
nothing further asked.

**`/memory` shows both memories in one place.** The three instruction files with
what each holds and — the part no other surface can tell you — **which of them is
actually being read**. Naming files in `[instructions]` replaces the default
rather than adding to it, and a named file that does not exist is skipped in
silence, so a project's own table can quietly displace a wider list and nothing
warns you. Beside the files sits the durable memory the *agent* wrote for itself:
every entry with its key, its kind, whether it is pinned, the run and step that
wrote it, and how many runs have since drawn on it, in both the workspace scope
and the global one, with the caps named per scope because a run drawing on both
can carry twice the number a single figure suggests.

**Pin what should survive, forget what should not.** A pinned entry is exempt
from being overwritten and from being evicted when the cap is reached, which is
the only lever an operator has over a store the agent otherwise manages alone.
Forgetting leaves a restore point, so an entry removed by accident can be put
back; an entry that is pinned reports the refusal and names the pin rather than
quietly doing nothing. Evictions, refusals and recalls are shown too — they emit
no event at all and have been happening invisibly since the store gained a cap.

**`tui-textarea` is gone, and with it the last thing holding this product on
ratatui 0.29.** The composer and the wizard now use an editing model io-cli owns.
Nothing about typing changes: the multi-line keys, prompt-history recall, the
wrapped rows, the single block cursor, selection, cut and paste, the masked
credential field, and undo and redo on `Ctrl+U` and `Ctrl+R` all behave exactly as
they did, one step per character as before. ratatui moves to 0.30 and crossterm
stays at 0.28. The dependency that carried a security advisory with no fix
available for the version in reach is no longer in the tree.

## [0.17.0] - 2026-08-26

Say something while it works, and have it land.

**A prompt typed during a turn is kept.** Until now it was destroyed: the
composer took the keystrokes and drew them, and the `Enter` that followed reached
a branch that discarded the text while the composer that held it had already
cleared. The keystroke looked accepted and the prompt was gone. It now joins a
queue in the order it was typed, drawn above the composer, and fires when the
turn ends — one prompt per turn, each its own exchange in the scrollback and each
interruptible. A turn you stop drops what was waiting, because one press of the
stop key should not start the next three turns.

**The queue is on screen, and it cost the session no height.** The waiting lines
are drawn above the prompt, the arrows mark one, the shifted arrows move it, and
`Enter` on an empty prompt takes it back into the composer to edit. The rows come
out of the blank row that has sat above the activity line since 0.13.0 — it is
lent to the queue while the queue is open and taken back the moment it closes. The
viewport is the eight rows it always was, the composer keeps every row it has, and
the alternative — a frame that grew by one row per line typed — would have walked
the conversation off the top of the screen exactly when it was worth reading.

**`/steer` sends what is queued into the turn that is still running.**
io-harness delivers it at the next step boundary, so the step in flight completes
whole and the agent reads the correction before it chooses what to do next. That
is the difference between redirecting an agent and killing it. It is a word you
type rather than something a queued line does by itself, and that is deliberate: a
delivered steer emits no event this interface can draw, so a line that went on its
own would leave the screen with no echo at all. It is not instant and the
interface says so — a tool call in flight is not a safe place to change the
conversation out from under.

**A contained turn can be steered now, and that is the last thing containment
decided.** Through io-harness 0.66 no session entry point took a caller's
containment and a steer inbox on one call, so `[app.io-cli.containment]` bought a
fan-out and charged a mid-turn correction for it. 0.67.0 opened
`turn_bounded_steered` and `turn_contained_bounded_steered` — the same two calls
with an inbox appended — and io-cli takes both. `/contain` decides fan-out and
nothing else.

**`Ctrl+C` means exactly what it meant, and this release is where it was decided
not to move it.** Both arms now hold a `SteerInbox`, so `Steer::interrupt` would
reach the same `RunOutcome::Cancelled` at the same step boundary — and the stop
key stays on the observer's cancellation flag, where it has been since 0.1.0. The
two paths are recorded by different code in io-harness, an operator cannot tell
them apart from the screen, and this is the one key no configuration file may
rebind. It still pre-empts an approval, an intent question and a plan gate, and it
is still refused as a rebindable chord.

**`/context` says what is actually in the model's window.** The system block, the
tool catalogue, the repository's instructions, each MCP server's tools, the
recalled memory and the conversation — each with the tokens it costs, summing to
a total against the window your configuration declares. It is read off the
request that carried the turn rather than estimated beside it, which is why the
catalogue includes tools io-cli never registered: it is the catalogue the model
was handed.

**`/compact` folds the conversation when you know a long thread is finished**,
rather than when a threshold notices. It has two real triggers rather than one
word with two meanings: typed at an idle prompt it arms the next turn to fold at
its first step, and typed while a turn is running it goes down the same channel
`/steer` does, as `Steer::fold`, and lands at the next step boundary. It reports
what folded, not what was asked for — a request over a conversation shorter than
the fold's own floor does nothing, and with compaction turned off it does nothing,
and in both cases the line says so instead of claiming a fold.

**`/mcp` says how many tools a server offered**, beside how many have been asked
for. Two different questions, two different numbers. A server that offered
nothing says zero; a server whose events never carried the count says nothing,
because reading a missing count as zero would report a server offering no tools
while you watched it use them. This closes the deferral 0.16.0 recorded.

**`ctx N%` is true for the first time if you configured your own window.** Its
denominator was the crate default, so it was wrong for anyone who set `[context]`
or `[run] max_tokens`, and it was blank until the first fold — the whole period
in which the number was worth having.

**`/status` says "answering N calls" where it said "offering N tools".** That
number has counted calls since 0.10.0; it was the one site 0.16.0's rename
missed.

**io-harness 0.67 → 0.69.** 0.67.0 is what made this release possible:
`Session::turn_bounded_steered` and `Session::turn_contained_bounded_steered` take
a caller's `TaskContract`, an observer **and** a `SteerInbox` on one call, so
neither arm has to give one up to get the other. The inbox is a parameter of the
drive call rather than a field of the contract, which is why `contract.rs` needed
no change at all. Two breaking changes reach this crate and both land in tests
rather than in what you run: `EventKind::Mcp` gained the offered-tool count, and
`SteerInbox::pending` returns a struct rather than a tuple so that the third thing
an operator can send did not have to grow it. No dependency was added, the feature
list is unchanged, and every test that existed before this release still passes
unchanged.

## [0.16.0] - 2026-08-25

Nobody edits `io.toml` by hand.

**`/config` is the whole configuration file as a surface in the session.** Every
key io-harness validates, with the value in force and the file that decided it —
`user`, `project`, `local`, or `default` where no file decided it at all. A key
no file named names no file: io-harness returns an empty origin for it, and
attributing that to the lowest-precedence file would credit you for a value you
never wrote. Choose a row to put its key in the prompt; `/config <key> <value>`
asks which of the three files to write it to, and only that choice writes.

**Your file survives the write.** The comments, the blank lines, the key order
you chose, and every section io-cli has no type for — `[[agent]]`, `[[hook]]`,
`[instructions]`, `[toolchain]`, `[prices]`, `[[plugin]]` — come back byte for
byte. One value's bytes are replaced and everything else is copied through. The
new bytes go to a temporary file and are renamed over the original, so a failed
write cannot truncate a configuration, and the file's mode is preserved.

**A project-scoped change that would widen the boundary is refused, in
io-harness's own words.** All seven cases: a `[[hook]]` array, a `[browser]`
table, and `policy.defaults.exec = "allow"`, `policy.defaults.net = "allow"`,
`sandbox.allow_network = true`, `sandbox.force_floor = false` and
`sandbox.mode = "full-access"`. The same values are accepted in `io.local.toml`,
because the rule is about the scope rather than the value. io-cli holds no copy
of those rules: it writes, asks io-harness to read it back, and puts the file
back exactly as it was when the answer is no.

**`/mcp`** shows what is configured, which servers answered this session, how
many distinct tools each has answered, and the last failure. A server the session
has not reached says so and is **not** drawn as a failure — that is the state
every server is in before the first turn runs. Servers are added, edited and
removed from the file through the same writer.

**`/provider`** is where `[[provider]]` stops being a single entry. Several are
configured, each with its own credential, model and endpoint, and the order they
are listed in is the order a turn tries them — the fallback io-harness has
supported since its 0.27.0, which this interface has drawn an event for without
ever being able to cause. Entries are added, reordered and removed, and an entry
moved keeps its own comments. The twenty-one vendor presets io-harness reaches
through one `Compatible` provider are offered by name, with the endpoint each
resolves to.

**`/profile`** selects a named `[profile.<name>]` for the session, and
`--profile <name>` selects one for a single run without writing anything.
Profiles have been in io-harness since its 0.27.0 and no io-cli release had ever
selected one.

**Three ceilings gain a home**: `max_parallel_reads`, `spawn_background_after_secs`
and `detached_spawns`, under `[app.io-cli]`, because io-harness has no
configuration key for any of them. They apply to an interactive turn and to
`io exec` alike.

**The command surface is grouped**, because this release took it to twenty and a
flat list of twenty is a list nobody reads: the session, this turn, inspect,
configure, none longer than ten. The `/` palette shows the groups while you
browse and drops them the moment you type. `/help` is the same grouping, written
into the terminal's own scrollback. Each palette row now carries a mark saying
whether it runs a command or fills the prompt — beside the name rather than in
the description, because the description is what a narrow terminal drops first.
`/usage` answers what `/status` answers and is listed nowhere.

### Removed

**`[app.io-cli] max_steps` was removed, as 0.14.0 said it would be.** It was
deprecated in that release with a notice naming this one, in the terminal, in the
README and in this file. Use `[run] max_steps`, which bounds a session turn and an
`io exec` run alike.

A file that still carries the key **loads exactly as before** — the key is
ignored rather than rejected — and the session tells you once at startup, naming
the number that is no longer in force. That notice is deliberate: `[app.io-cli]`
is not schema-checked, so without it the key would simply stop working and your
step cap would change with nothing on screen to say why.

### Changed

The status line's `mcp N/M tools` now reads `mcp N/M calls`. The second number
has counted calls since 0.10.0: `EventKind::Mcp` carries no tool count and
io-harness exposes no catalogue accessor, so the number that field wanted was
never available and it counted the one that was. `/mcp` now draws a per-server
count beside it, and two numbers disagreeing about one word is worse than one
number with an honest label.

### Known limitations

`/mcp`'s tool count is how many distinct tools a server has **answered** this
session, which is a lower bound on what it offers. There is no channel for the
real number on io-harness 0.67. Disabling a configured server without removing it
is not offered either: `McpServer` has no key for it, and because the type is
`#[serde(flatten)]`-based an invented one would be accepted by the file and
ignored by the harness — so the server would start anyway. Both are io-cli
0.17.0's, behind an io-harness release.

## [0.15.0] - 2026-08-25

io keeps its things in one place.

**`~/.io-cli` is now the answer to "where does it live", on every platform, and an
existing install moves into it the first time you run this version.** Before this
release there were three answers and none of them carried the product's name:
`~/.config/io` on a Linux box, `$XDG_CONFIG_HOME/io` where that was set,
`%APPDATA%\io` on Windows — with the run store sitting beside whichever one
applied, undocumented and untested. One directory to back up, to copy to another
machine, to delete when you are finished with it, and to name in a bug report.

**The move is the part to read before upgrading.** On the first 0.15.0 run,
`io.toml` and the run store are moved from wherever they were into `~/.io-cli`,
and each file that moved is named on screen — in the scrollback in a session, on
stderr under `io exec`, never on stdout, because `--json` writes NDJSON there. If
the run cannot start at all — a configuration file that will not parse, a store
that will not open — the report is still said, ahead of the error. That is the
moment it matters most: an error naming a path you have never seen, one keystroke
after your old directory emptied, is the reading this release exists to prevent.
Nothing is overwritten: where the home already holds a file, the one already there
is the one that stays and the other is left exactly where it was. Nothing is
deleted, and a file copied across filesystems has its copy checked before the
original goes. If you would rather keep the old location, set `IO_CONFIG_HOME` to
it before you first run 0.15.0 — and if you have already set `IO_CONFIG` or
`IO_CONFIG_HOME`, nothing here happens to you at all: no variable is set, no file
is moved, and you are not told about a migration that did not occur.

**The store moves with the file because it has to.** The run store's path is
derived from the configuration file's own directory, so moving one without the
other would empty `/resume` on upgrade. Its write-ahead log moves with it for the
same reason one level down: SQLite opens a `runs.db` whose `-wal` was left behind
without complaining and simply does not contain the last session, which is a loss
that arrives as a session that vanished rather than as an error. The durable
memory the agent writes for itself is rows in that store rather than a file, so it
travels with it.

**This is a product choosing its home, not a second configuration system.**
io-harness has resolved `$IO_CONFIG`, then `$IO_CONFIG_HOME`, then the platform's
own place since the 0.19.0 that introduced `io.toml` at all, and it reads them at
the moment it is asked. So io-cli names the second one for you when you have named
neither, once, before the first read — and the resolution order itself is
unchanged. One consequence stated rather than left to be found: the variable is
set in io-cli's own process, so every child a session starts inherits it — a `!`
shell line, a spawned agent, a nested `io`. For a nested `io` that is the right
answer; for anything else it is one more variable in the environment.

**A tilde is a home directory now, in a skills path.** io-harness substitutes
`${env:…}` and `${file:…}` and nothing else, so a `~` written in a `skills` key
reached the directory walk verbatim and named a directory literally called `~` —
which the configuration example shipped as its own suggestion. io-cli expands a
leading `~` before handing the path over, and where no `skills` key is set at all
the default is `~/.io-cli/skills`, which is created with the home so that it is a
real place to drop a file into. Where you have chosen your own location and io-cli
made no home, a default naming a directory that does not exist is not used at all
— io-harness refuses such a directory outright rather than finding nothing in it,
which would be every turn failing rather than an empty catalogue.

**And `/` lists the skills the model was given.** The palette walked
`[app.io-cli] skills` alone, so a `[run] skills` reached the agent while the
command list showed nothing from it. It asks for the same directory the turn is
built with now, which closes that gap rather than widening it with a new default.

**`/status` gained a `home` row**, naming the directory your configuration file and
run store are actually in and the word that decided it — `default`, `IO_CONFIG` or
`IO_CONFIG_HOME`. It reports the directory in force rather than the one io-cli
would have chosen, which are the same thing until you have chosen otherwise, and
that is exactly when a row like this earns its place.

Rolling back is a variable or a move, because nothing here destroys anything.
Reinstall 0.14.0 and set `IO_CONFIG_HOME=~/.io-cli` and it reads the moved files
where they now are, with no history lost; without that variable it looks in the
old location, finds it empty, and your files are still on disk in the home.

## [0.14.0] - 2026-08-25

The configuration file reaches your terminal.

**Eleven sections of `io.toml` that an interactive session read past are now
applied to every turn.** This is a behaviour change for any file that already
carries one, so it is the first thing said: `[sandbox]`, `[run]`,
`[run.commit_identity]`, `[instructions]`, `[[mcp]]`, `[[lsp]]`, `[[agent]]`,
`[web]`, `[memory]` and `[browser]` were read by io-harness, validated by
io-harness, documented in this product's own README and configuration example,
and then discarded by every session turn — a developer who wrote
`[run] max_tokens = 200000` and watched a turn spend past it was reading a file
that did nothing, with nothing on screen saying so. The reason the documentation
gave had been stale for three releases: the harness's steerable turn built its
own contract, and the flat turn stopped taking that path in 0.11.0. What was left
was an omission and not a constraint. A session turn and an `io exec` run now
build the config-derived half of their contract from one call, so what the file
says has the same effect in a terminal as it has in CI.

**A `[run]` block written for CI now bounds a conversation.** `max_steps = 20` is
a reasonable thing to have set for an unattended run and an unreasonable cap on a
session, and if that is what your file says, that is now what your terminal does.
The session names what the file turned on when it starts, the status line carries
each budget in force, and `/status` lists them all — so a turn that will stop at a
ceiling says which one before it gets there rather than after. An operator who
wants `[run]` for CI only moves it to a project file or narrows it by scope.

**`[web]` is a capability and not a preference, and it deserves its own
sentence.** Reaching a session turn it gives the model the provider's own search
and fetch, and it is the *vendor* that dials the URL — so the `net` rule in your
permission policy is not what governs it. That rule decides what this machine may
reach. A `[web]` table that did nothing in your terminal yesterday turns something
on in it today, from a file you may have written for something else, which is why
the session says so in its own words at start rather than folding it into a list
of what was applied.

**`/status` commits the whole session state into the scrollback.** One fact per
row: the workspace and the session id with the turn its head is at, the provider
and model, every policy layer by name with the acts it governs, the containment
caps and the draw against them, the sandbox mode asked for beside the backend that
actually answered on this host, every budget with what is left, how full the
context is, and what is connected — MCP servers and language servers as answered
of configured, the browser, the skills directory. Every field on it is a value
io-harness supplied. It commits upward rather than opening a pane, the same answer
`Ctrl+T` and `/expand` already give, and it is not a table: a table has a column
width, the widest cell here is a workspace path, and a row too long for the
terminal is folded rather than cut. It is a command and not a key, deliberately —
a key is cheap to add later and expensive to take back once it is in anybody's
fingers.

**The ceilings in force are on the status line, beside what has been drawn against
them.** `left 17/20 steps`, `left 12.4k/200.0k tok`, `left 4m30s/10m00s`, for each
of the step, token and duration budgets that exists and for no others: a budget
you did not set draws no field, so a session that configured nothing looks exactly
as it did. They are read off the contract the turn was built from rather than
composed a second time out of the file, which is the only place the order of
precedence is already resolved.

**A turn that ends on a budget says which budget.** `step_cap_reached`,
`time_budget_exceeded` and `cost_budget_exceeded` were reported through the error
path, so what an operator met under a half-finished answer was
`error: step_cap_reached` — a ceiling drawn as a crash. All four outcomes are
successful calls in io-harness and always have been. The word stays the harness's,
because this interface reports what the harness decided and never relabels it;
what changed is the weight it is said in.

**Three event kinds reach the transcript that never have.** All three have been
emitted into every ordinary session and dropped here. **Every outbound connection
a contained command dialled** is now a line carrying the host as the command asked
for it, the port and whether the policy permitted it — never a resolved address,
because the policy's patterns are written against names and a row showing
`140.82.121.4` would not match the rule that decided it. A refusal is drawn as a
refusal and not as an error: nothing broke, the boundary worked. An absent dial
line is **not** evidence of no egress — a permissive or all-or-nothing policy
names no host and emits none of these ever. **Each sandbox created, capped or
destroyed** says so, with the backend that isolated it where io-harness carries
one and no invented name where it does not; a cap reached is drawn as a limit
reached, because the sandbox did exactly what its configuration told it to.
**A stalled agent is on screen while it is stalling**, naming the step it stopped
on and how long it has been there, rather than reaching you as a session that had
gone quiet and then, once the run was over, as one word on the outcome line.

**`io exec` takes io-cli's own step floor of a thousand**, where it used to take
io-harness's twelve. Twelve steps is not a turn, so an unattended job ended
`error: step_cap_reached` over half-finished work with nobody watching — the same
defect the floor exists to fix in a session, made worse rather than better by the
run being unattended. A `[run] max_steps` in the file still beats the floor, in
either direction.

**Servers named in both scopes are merged rather than one list replacing the
other.** io-harness's `with_mcp` and `with_lsp` assign the whole collection, so
applying `[[mcp]]` and then `[[app.io-cli.mcp]]` in sequence silently discarded
the first list — an operator with servers in both would have lost one set with no
message. They are concatenated and deduplicated by id, the `[app.io-cli]` entry
winning a collision because it is the more specific scope, and the session names
the id that lost.

**`[app.io-cli] max_steps` is deprecated, still honoured, and removed in 0.16.0.**
It exists because the flat turn once had no way to raise io-harness's cap of
twelve, and `[run] max_steps` now does that job — two spellings for one number in
one file, where the less discoverable of the two wins. Nothing about it changes
here: a file carrying it gets exactly the cap it asks for, and still beats
`[run] max_steps`. What is new is one line at session start naming the key, the
value it took and where the number moves to. A file carrying only `[run] max_steps`
says nothing, and neither does a file carrying neither — a deprecation notice on a
session that is not using the deprecated key teaches operators to stop reading the
start-up lines.

**A startup notice is committed rather than said.** Six things can put a sentence
in that list — a section io-harness could not read, a keybinding naming no action,
a templates directory that would not walk, a skills directory that would not
either, a server named in both scopes and this release's `max_steps` deprecation —
and they were written to the footer, which *replaces*: a file with several things
wrong with it showed the last one and silently dropped every earlier one. Each
takes a row of its own in the scrollback now.

**The documentation said the opposite of the code, and in five places it had been
wrong since 0.11.0.** `docs/config.example.toml` carried a block headed "Not read
by an interactive session" naming eight tables; the README marked `skills`,
`[[app.io-cli.mcp]]`, `[[app.io-cli.lsp]]` and `[app.io-cli.browser]` "contained
turns only", said the capabilities and the fan-out were one switch, and said an
uncontained session could not be given a responder or a plan gate. All four
capabilities have been applied unconditionally, and the responder with them, since
0.11.0 gave the ordinary turn a contract. Those claims are gone. Nothing rides
`[app.io-cli.containment]` but the fan-out.

No key is added, removed or renamed, and a 0.13.1 configuration file is a valid
0.14.0 configuration file; what changes is what an existing one does, which is the
migration note above. An older binary reading a file that has moved to
`[run] max_steps` falls back to io-cli's own floor, which is what it did before
the key existed. Nothing is asked of io-harness; the pin stays at 0.66.

## [0.13.1] - 2026-08-24

The session answers every keystroke.

**The prompt froze when it grew past two rows, and it is fixed.** Pressing the
newline key a second time — or running `/clear`, or expanding a large pasted block
back to its full text — could stop the session dead for seconds, and on a measured
run it stopped for 5.7 seconds and then answered nothing at all. All three do the
same thing underneath: they re-place the inline viewport, which needs the terminal
to itself for a moment, and the keyboard reader was taking it straight back every
time it let go. A reader now stands aside while a placement wants the terminal.
The same keystroke, measured on the same script against the same binary: **5.7
seconds of silence before, 11 milliseconds after.**

**A prompt written on more than one line is read back as more than one line.** A
two-line prompt was echoed as one run-together row, because a rendered line is one
row and a newline inside it draws as nothing.

**`/attach` takes the path you actually have.** Three things were wrong with it at
once. A path dragged in from Finder arrived quoted and the quotes were never taken
off, so the extension read as `png"` and io said your screenshot was not an image.
The quoting escaped every non-ASCII character, which includes the narrow no-break
space macOS puts in every screenshot's name — so the path named no file even
unquoted. And a path outside the workspace was refused outright, which is where
screenshots live. `/attach ~/Pictures/shot.png` now works. A path inside the
workspace still goes through the session's policy, unchanged; a path outside it is
read directly, because that is the operator's own file and the same boundary `!`
already crosses.

**The prompt wraps, and there is one cursor.** `tui-textarea` scrolls sideways
rather than wrapping and paints its own block cursor, while everything io-cli
measures assumes a wrap — so a long prompt was drawn clipped at the left, with
two cursor blocks on it in two different places, and the viewport had grown for
rows nothing used. The composer draws its own wrapped rows now: text that reaches
the right edge continues on the next line, the window follows the insertion
point, and the only cursor on screen is the terminal's own.

**Pasting the same block again toggles it both ways.** Expanding a collapsed
paste used to leave the block in the prompt with its placeholder gone, so the
next paste of the same clipboard appended a fresh one — `[pasted text #2]`, then
`#3`, then `#4`, piling up after text that was already there.

**A pasted block deletes as one thing on every backwards deletion.**
`Option+Backspace` and `Ctrl+W` used to eat `[pasted text #8, 464 characters]`
one word at a time, and a placeholder is matched by its exact text — so the first
press had already stopped it standing for the block it named.

**The composer is one row at rest** and grows to what a prompt needs. The second
row was there for a paste too big to read in one and was empty for every prompt
anybody types.

**`/attach` is gone; drag a picture onto the prompt or paste it.** A command was
something you had to be told about before you could use the feature. Pasting the
same file again toggles between the marker and the path it stands for. The word
is not kept as a hidden alias: `/attach` is answered the way any other word that
is not a command is — `there is no /attach. The commands are:` — and the list
under it is the truth.

**A marker deletes with the space written for it**, so one press removes one
thing under every backspace — `Option+Backspace` used to eat `1]` off the end and
leave `[Image #` on the prompt. The path a repeat paste toggles to is quoted, the
way any pasted path is. And `/clear` resets the numbering: `[Image #1]` and
`[pasted text #1]` count from one again, because the ones before them belonged to
the conversation that ended.

**An attachment is `[Image #1]`.** A picture is no longer drawn when you attach
it or when you send the turn: the marker is what the prompt carries, what the
agent is told and what the transcript keeps, and it deletes as one thing exactly
as a pasted block does. `/image 1` draws it when you want to see it — a fresh
copy at the bottom, because a committed row belongs to the terminal's scrollback
and nothing here can reach back into it. `/image` is no longer a second spelling
of `/attach`.

**Notices moved to the footer.** Stopping one turn used to leave three
warning-coloured rows in your scrollback — `stopping at the next step boundary`,
`stopping now`, `stopped` — sitting between two answers forever. None of them is
part of the conversation. A notice now takes the footer's last row, replaces the
one before it, and is gone at your next keystroke. What still reaches the
transcript is what belongs to the record: what was authorised, what was answered,
and why a turn failed.

**A drop of several files is several pictures.** A terminal writes a multiple
selection on one line, separated by spaces with any space inside a name escaped —
or one per line. Read as a single string none of that was a path, so dropping
three pictures at once did nothing at all.

**A turn stopped before it did anything is taken back whole.** No step, nothing
streamed, nothing on screen but the echo of your prompt: `esc` abandons it at
once, the rows come off the screen, and the prompt goes back in the composer
ready to edit or send again. Nothing is said, because nothing happened.

**A rule over the composer**, matching the one under it. The prompt had a
boundary on one side only, so it read as the tail of whatever the turn had last
written rather than as the field it is.

**A picture no longer lands on top of what was there.** The rows a committed
image occupies were the viewport a moment ago, and nothing erased them, so an
image that did not fill its box was drawn into a stale prompt and status line.

**A failed turn says what it means before it quotes the provider.** Attaching a
screenshot to a model that cannot look at one used to end with
`error: escalated_terminal` and a routing layer's JSON about HTTP 404. Six
conditions now get a sentence in front of the provider's own line — no image
support, no credit, a rate limit, a rejected credential, an unroutable model, and
a conversation past the context length. The provider's text is never replaced,
only prefaced.

**The work is now above the line that says it is working**, with a row of air
between them. Through 0.13.0 the streaming row was drawn under the activity line,
so the newest words the agent had written read as a footnote to a spinner rather
than as the transcript continuing. If you have a screenshot or a recording of an
older release, this is what looks different.

## [0.13.0] - 2026-08-24

Five defaults that were never set.

**Every turn now carries a system prompt io-cli wrote, and a model will answer
differently under it.** This is the change with the widest reach, so it is the
first thing said: through 0.12.0 every turn ran io-harness's built-in
description, which names the tools and says nothing about tone, format or length.
The new prompt sets what `io` is, that the reader is at a terminal, that the
answer comes first and briefly, that work is reported once done rather than
narrated in advance, and that the output is monospaced text about eighty columns
wide. It is **appended** to io-harness's own prompt, not substituted for it, so
the harness keeps its framing, its tool catalogue, the repository's own
instructions and the sentence that decides how a turn ends. It names no model and
no vendor and claims no tool. There is no configuration key for it: per-repository
voice belongs in the file io-harness's `[instructions]` discovers.

**The palette no longer grows the viewport, and no longer shows every command at
once.** If you are used to seeing the whole list on `/`, you will notice. What you
get instead is the rows the session's viewport already has, and the rest by typing
or arrowing — the behaviour `/model` has always had. What that buys is the
keystroke: opening the palette used to re-place the viewport, which asks the
terminal where its cursor is and takes the stdin lock to read the answer, and did
it again on the way out. On a terminal that does not answer that query, `/` cost
two seconds. It now costs a repaint.

**A blank row between a designed block and your next prompt.** A thought footer, a
tool cell or a harness warning followed by the `›` line used to read as one block
in one voice. One row of air, never two.

**`io` names the newline key that works on your terminal.** `Shift+Enter` is
unreportable unless the terminal speaks the Kitty keyboard protocol — it sends the
same byte as `Enter` — so on a terminal that does not, `/help`'s key reference and
the wizard's closing screen now name `Alt+Enter` and the trailing backslash and
say the key is unreportable here rather than listing it. The README's table keeps
the advertised spelling, because a README is read somewhere else. Nothing about
the composer changed: all four spellings work exactly as they did.

**The installers say what they are doing.** `install.sh` and `install.ps1` now
narrate every step on stdout: the target they resolved, where the version came
from, each URL as it is fetched, the expected and the computed checksum **both**
before comparing them, the destination and whether it is on `PATH`, and the
installed binary's own `--version`. Every failure path is unchanged, message and
exit status, and stays on stderr.

No configuration change: a 0.12.0 configuration file is a valid 0.13.0
configuration file, and no key is added, removed or reinterpreted.

## [0.12.0] - 2026-08-23

The capabilities stopped being a mode.

**A contained turn no longer proposes a plan unless you asked for one.** This is
the one thing in this release that takes something away, so it is the first thing
said: through 0.10.0 and 0.11.0, configuring `[app.io-cli.containment]` also
registered io-cli's plan gate, and registering a gate is the entire condition for
io-harness's planning phase — so every turn of a fan-out session stopped and
proposed before it did anything. If that is the behaviour you were relying on,
`/plan on` is where it lives now, and nothing else changed about it.

**`/plan on | off`, off by default.** It takes effect from the next turn, and bare
`/plan` reports which phase you are in without switching it, the same rule
`/contain` follows. While the phase is on the status line says `planning` — it is
not cleared when a run ends, because the phase outlives the turn it was set on and
an operator watching an agent that will not write needs the reason on screen.

**A question is answered on any turn.** The responder was on the contained turn
only, so on an ordinary turn the agent asking what you meant paused the run with
nobody offered the question. io-harness resolves a contract's responder inside the
tool dispatch on any run, so there was never a reason for that; there is now an
overlay wherever the question is asked.

**The contained-mode notice stopped overstating what the mode gives.** It offered
skills, MCP servers, language servers and a browser as things containment grants,
and named a lost mid-turn steer as its price. Neither has been true since 0.11.0
gave the ordinary turn a contract: all four are on every turn, and no turn takes a
steer inbox. The notice now names the caps and one difference — this is the only
turn that can fan out — and `/contain off` says "not contained" rather than
promising a steering this product no longer has.

Nothing is asked of io-harness; the pin stays at 0.66. A 0.11.0 configuration file
is a valid 0.12.0 configuration file.

## [0.11.0] - 2026-08-20

The transcript's vocabulary changed.

**Four tags stopped appearing: `prompt_composed`, `contained`, `reasoning` and
`answered`.** They were never designed lines. io-harness declares fifty-one event
kinds and thirty-seven of them fell through to a placeholder that committed the
variant's own snake-cased name, which is what put Rust identifiers in front of
whoever was reading a session. Every kind now has a disposition chosen by hand —
a line, a status-line field, or nothing — and a kind io-cli has never seen
commits nothing at all and is counted instead.

Nothing about the permission boundary, the approval overlay, the containment
seam, the scrollback contract or the io-harness pin changes. This release asked
io-harness for nothing.

### Added

- **The activity line**, a new top row of the viewport present for exactly as
  long as a turn is in flight: a word for the turn, the elapsed clock and the
  live token count. The word is chosen once per step from a fixed list, so it
  moves when the work does and not on a timer of its own. On a narrow terminal it
  drops the token count and then the clock, which is the rule the status line
  already follows.
- **A live row that says what is happening**, in one order: waiting on you, then
  an open tool call and its target, then the model thinking, then the streaming
  tail. Waiting on a person outranks everything else, because every other thing
  that row can say is about work going on without you.
- **The model's reasoning, committed as a thought** — one row: the word, how long
  the step had been going, and what it cost. The text is kept for `/expand` and
  not committed: a thought is usually longer than the answer it precedes, and a
  transcript carrying every one buries the work in the deliberation. `/expand` is
  the only place it can be read, because io-harness neither stores reasoning nor
  folds it into the next prompt.
- **Two status-line fields: the provider and the step count.** Both are set from
  the events that carry them and both are cleared when a run is forgotten. They
  are where the two removed rows' facts went.
- **`/clear`** — a new conversation without leaving the binary: a new session id,
  no prior turn sent to the model, and the run-scoped status fields back to zero.
  It clears the screen and nothing else; the conversation it ends is in
  io-harness's store and is still listed by `/resume`. Refused while a turn is
  running.
- **`/exit` is listed**, and `/quit` is gone. The parser has accepted `exit`
  since 0.1.0 and nothing ever advertised it; two commands doing one thing, with
  a row each in the palette, was the other half of that defect.
- **The model's markdown is rendered rather than printed.** Headings, bullets,
  quotes, rules, fenced code and inline bold, italic and code — a line at a time,
  because that is how the transcript commits. Anything unrecognised is left
  exactly as the model wrote it: a renderer that guessed would eat characters out
  of an answer.
- **The composer.** Pasting the same block twice expands the placeholder into the
  block; backspace over a placeholder removes the whole placeholder and the block
  it stands for; a pasted path that names a file is quoted and resolved, so a
  path with a space survives as one word.

### Changed

- **A tool cell reads as a verb and a path**: `Read src/lib.rs` rather than
  `read_file` and an absolute one. The mapping is a table of io-harness's own
  built-in tool names; a tool that is not in it keeps the name io-harness sent,
  because a verb invented for a tool this release has never seen would mean
  nothing. A target inside the workspace is shown relative to it and one outside
  is shown whole.
- **A turn ends on its answer.** The `finished · N steps · N tok` row is gone. An
  outcome that stopped short still commits its own line, because a run that
  stalled or hit a ceiling has to say so; a plain finish commits a blank line.
- **`via {provider}` is gone from under every prompt.** The provider is a
  status-line field now, spelled the way the posture is.
- **The viewport is eight rows**, not four: a blank, the activity line, the live
  row, two rows of composer, and a three-row footer. It is still clamped to the
  terminal, so 80x24 is a supported size — the rows go in the order they can be
  given up, and the composer keeps its two at every size.
- **The command palette shows the whole list.** Opening `/` re-places a taller
  viewport for as long as the palette is open and gives the rows back on close —
  by a choice, by `Esc`, or by the terminal resizing under it. It is done only at
  an empty prompt, where nothing is streaming.
- **`--plain` still commits the provider and the run's numbers.** The two rows
  this release removed moved to a line a plain session does not have, and a fact
  that lives only in a repainting row is a fact taken from exactly the reader who
  cannot follow one. It is committed in the status line's own spelling, so a
  number has one form wherever you meet it.
- **A step commits a line only when it says something its tool cells did not.**
  Through 0.10.0 every call was printed twice — once as a cell and once in the
  step line under it, in a different order and a different vocabulary. What is
  left for that line is what the cells cannot carry: files changed, or a decision
  that could not be paired to a call.
- **A cell's result column carries what io-harness added, not what the cell has
  already said.** `Read io.toml · read io.toml` is now `Read io.toml`, and
  `List · list_dir  (4 entries)` is `List · (4 entries)`.
- **An outcome that stopped short says what it means.** `step_cap_reached`,
  `stalled`, the three budgets, `plan_rejected`, `cancelled`,
  `awaiting_recovery` and `escalated` each get a sentence under io-harness's own
  word. A run used to end on `error: step_cap_reached` and nothing else.
- **Spacing.** One blank row between a block of tool cells and whatever follows,
  one between a designed line and the model's prose, one between turns rather
  than two, and a paragraph break inside a thought is one row rather than two.
- **A viewport erases its own rows before handing them back**, so the palette
  leaves nothing painted behind the session it returns to.
- **A turn is no longer capped at twelve steps.** Every turn now carries a
  contract io-cli built — the ordinary one through `turn_bounded_observed`, the
  contained one as before — so the step cap is this product's rather than
  io-harness's default. It is a thousand, which is not a number anybody reaches
  on purpose: what ends a turn should be the work finishing, a stall, a budget or
  you, never an arithmetic ceiling reported as `error: step_cap_reached` under a
  half-written file. `[app.io-cli] max_steps` sets your own.
- **`Esc` stops a running turn**, which is what it is for in every other agent,
  and a second press of `Esc` or `Ctrl+C` stops it *now* rather than at the next
  step boundary. The first press is still the clean stop: the run closes itself
  and the store records how it ended.
- **`Shift+Enter` has two fallbacks that always work.** It needs the Kitty
  keyboard protocol, and a terminal without it sends the same byte for `Enter`.
  `Alt+Enter` and `Ctrl+J` insert a newline everywhere, alongside the trailing
  backslash.
- **The footer is three rows: a rule, then two lines.** One long dot-separated
  run of eight fields is a sentence with the punctuation removed. Now the state
  and the model sit on one row with the clock at the right edge, the counts and
  the posture on the row under it, and exactly one thing is bold and one is
  coloured — which is what makes either mean anything.
- **The prompt takes the rows it needs**, up to ten, and gives them back.
- **`/clear` opens the session again**, banner and all, rather than leaving a
  cleared screen with one grey line on it.
- **The banner is a card with room in it**: the mark, the version, and the model,
  policy and workspace, one blank row inside each edge and two columns inside
  each side.
- **`Shift+Tab` cycles the posture silently.** The footer repaints on the same
  keystroke, so the line it used to commit said in the scrollback what the screen
  was already showing — and cycling through three postures to reach one left
  three of them behind, permanently, in the transcript of a session that ran
  under one.
- **The clock and the activity line's token count belong to the turn.** Both
  start at zero when a turn starts: a clock counting since the terminal opened
  said `22m12s` about a turn six seconds old. The footer keeps the session's
  token total, because that is what a spend is judged on.
- **A diff carries line numbers**, in each side's own file, with a blank row
  above it. A change you can see but cannot go to is half a diff.
- **An approval is said once.** The overlay carries the request, so the
  transcript no longer commits the same sentence directly above it. In plain
  mode, which draws no overlay, the line is still committed.
- **The footer says `working` only when nothing above it does.** The activity
  line already carries a spinner and a word; a second spinner under it turning at
  the same rate said one thing twice.
- **A step commits no line when its cells already said it**, `changed files`
  included — the diff underneath is what says a file changed.
- **The keyboard-protocol probe is asked once per process.** It costs two seconds
  on a terminal that never answers, and the palette re-places the viewport twice
  per open and close.

## [0.10.0] - 2026-08-19

A contained session answers.

The two places a run stops and waits for a person are answered where they
happened, the skills you gave the agent are in the palette, and the line says
what the session is connected to.

**All of it rides `[app.io-cli.containment]`, and that is worth reading before
you configure any of it.** io-harness offers exactly one session entry point that
takes a caller's `TaskContract` — the contained one — and a responder, a plan
gate, MCP servers, language servers, a browser and a skills directory are all
fields of that contract. So the capabilities and the fan-out are one switch. What
it costs is nothing that turn ever had: a contained turn has never taken a steer
inbox. A session without the table is the session 0.9.0 shipped, mid-turn
`Ctrl+C` included.

### Added

- **The agent's question about intent, answered in the session it was asked in.**
  Not an approval — an approval asks whether an act is permitted, this asks what
  you meant, and its answer authorizes nothing. So it is prose you type rather
  than one of three keys. `Esc` leaves it unanswered, which pauses the run with
  the question kept rather than sending the agent back with nothing.
- **A plan, decided before any of it runs.** Registering a gate turns
  io-harness's planning phase on, and while it is on the run's own policy denies
  every write and every exec — so cancelling is not an undo, there is nothing yet
  to undo. `Enter` on an empty prompt approves, a correction and `Enter` sends it
  back for another plan, `Esc` cancels and nothing runs.
- **Harness skills in the `/` palette**, after the commands and the templates,
  discovered by io-harness from the `skills` directory. Choosing one puts `use
  the <name> skill: ` in your prompt; the file is the model's to read, under the
  run's own policy. io-cli parses no skill file.
- **The line says what the session is connected to** — an MCP server and how many
  tools it offered, a language server that came up for this workspace, and the
  browser with the last host it was allowed or **refused**, drawn differently
  because a block that reads like a visit is worse than no field at all. Every
  one comes off the event stream, so a server that was configured and never
  answered leaves the line silent, which is the honest answer.
- **`[[app.io-cli.mcp]]`, `[[app.io-cli.lsp]]`, `[app.io-cli.browser]` and
  `skills`**, deserialized straight into io-harness's own types. io-cli defines
  no schema for any of them.
- **The real image on iTerm2.** Its escape has no equivalent of Kitty's `C=1`, so
  the placement is bracketed by a cursor save and restore — which is what that
  flag was doing — and states its width and height in cells, so the rows it
  costs are known before it is written. Terminals that speak neither protocol
  still get half blocks and no escape at all.

### Changed

- A contained turn is driven through `Session::turn_contained_bounded_observed`
  and carries a contract this crate built. A session that configures nothing
  builds a contract identical, field for field, to the one io-harness built for
  it before.
- **A contained turn now stops for a plan before it acts**, because registering
  a plan gate is what turns io-harness's planning phase on and a contained turn
  carries one. That is a round trip 0.9.0 did not have; `/contain off` gives back
  a turn that starts working immediately.

## [0.9.0] - 2026-08-19

The session gains sight, in both directions.

You can show the agent a picture, and you can see the picture the agent looked
at, in the terminal you are already in rather than by going and opening a file.

### Added

- **`/attach`, which puts an image in front of the agent for the next turn and
  only the next turn.** The path is read through io-harness's `Workspace`, which
  documents that as the same policy gate a source read passes rather than a
  second one — so an image the session may not read is refused exactly the way a
  file it may not read already is. The argument can be `@`-completed, because the
  path picker opens on `@` after any whitespace and not only at an empty prompt.
- **The picture the agent looked at, committed where it looked.** Enabling
  io-harness's `media` feature puts its own `view_image` tool into the workspace
  tool set, so **the agent gains the ability to look at images in this release**.
  It is governed by the same policy as any other read, and when it does look, the
  same picture goes into your scrollback at that point in the conversation.
- **Half-block rendering, which works on every terminal.** `▀` splits a cell into
  two halves that are each about square, so a picture is drawn from the cells the
  terminal already has, fitted to its width and bounded in height.
- **The real image where the terminal speaks the Kitty graphics protocol** —
  kitty, ghostty, WezTerm and Konsole. Placed with `C=1`, which is what lets it
  sit inside a renderer that draws the cells around it.
- **Background shell handles are named, counted and accounted for.** A
  `shell_start` outlives the step that launched it, which is the whole point of
  it and the whole problem: a run waiting on a dev server looks exactly like a
  run that has hung. The command is named when it starts, a `bg N` field counts
  what is still alive, and each job says how it ended — exited with a status,
  killed, or left running by a run that finished first.

### Changed

- `io-harness` is taken with `features = ["media"]`. The pin does not move; 0.9.0
  is still built against 0.65.
- `image` is the eleventh direct dependency, with `default-features = false` and
  exactly the nine formats io-harness will accept from a file. Its defaults would
  pull an AV1 encoder and rayon into a crate that only ever decodes a file the
  harness has already accepted.

### Not in this release

- **iTerm2's own inline-image protocol.** Its escape has no equivalent of Kitty's
  `C=1`: it advances the cursor and may scroll, and a scroll changes what every
  later absolute cursor move in the same draw means. It probably lines up against
  a region of exactly the right height — but "probably" is not good enough when
  the failure lands in scrollback that no later redraw can clean. iTerm2 gets the
  cell form, which is a picture. Deferred to 0.10.0.
- **Sixel.** Encoding it means palette quantisation, which means another
  dependency, for terminals that either also speak Kitty or render half blocks
  correctly.
- **The graphics path for jpeg, gif and webp.** Kitty's `f=100` is PNG, and the
  only base64 in reach is the one io-harness already computed — this crate takes
  no base64 dependency. `Media::attach` transcodes bmp, tiff, ico, tga and pnm to
  PNG on the way in, so those reach the graphics path along with png itself,
  while jpeg, gif and webp take the cell form.
- **Any check that the chosen *model* accepts images, as opposed to the
  provider.** `Provider::accepts_images` is asked before an attachment is
  accepted, but with OpenRouter in front of four hundred models that answer is
  yes while the model you picked may be text-only — and the failure then is the
  provider's own `HTTP 404: No endpoints found that support image input`, after
  the step and its tokens are spent. Found by running the built binary. It cannot
  be closed from here anyway: enabling images gave the agent `view_image`, and a
  tool in io-harness's workspace set is not io-cli's to remove.
- **Anything the agent was *given* rather than asked for.** An image returned by
  an MCP tool and a browser screenshot both become images inside io-harness, but
  through private plumbing and with no event of any kind — there is no media
  variant among its fifty-one — so nothing reaches this program to draw.
- **Live indicators for MCP servers, language servers and the browser.** All
  three are fields of a task contract supplied by the caller, and no io-harness
  session entry point takes one, so those events cannot fire in a session at all.
  They already work in `io exec`, whose contract carries them, and `--json`
  already emits them. Moved to 0.10.0, which waits on the same change.

## [0.8.0] - 2026-08-19

A decomposed task becomes visible while it runs.

An agent can break work into sub-agents and run them over the same workspace.
io-harness has been able to do that since 0.39.0 and nothing has ever shown it.
This release does: the children, the tiers, the refusals and what the fan-out is
costing are on screen while it happens, and every one of those is a fact only
this core emits.

### Added

- **`[app.io-cli.containment]`, and the contained turns it selects.** Four caps —
  agents, agents at once per tier, depth, and a token ceiling the whole tree
  draws down together — read as io-harness's own type, so there is one spelling
  of them. With the table present a session's turns go through the one entry
  point that reaches io-harness's spawn loop; with it absent, every turn is the
  turn 0.7.0 shipped. `/contain on|off` switches, and `/contain` on its own
  reports rather than guessing.
- **A live fleet view**, over the prompt, opened by `Ctrl+F` or `/fleet`. One row
  per admitted child with its state and its own draw, indented by its depth, and
  a per-tier line counting what is working, waiting and done. A waiting child is
  a count and never a row: until a concurrency slot frees it has no run of its
  own to name, and a placeholder for one would put an agent on screen that does
  not exist yet.
- **Spawns, refusals, collected reports and detached children in the
  transcript**, where they happen. A refusal says which cap refused it in words
  and that the agent carries on with what it has, because a refusal is not an
  error. A collected report names no child — the event carries none, and with
  several in flight the order they arrive in is not identity.
- **The spend field on the status line**, six releases after it was named. What
  this turn has drawn and what the tree has left, in tokens; a tree reporting no
  ceiling gets none stated rather than a zero. It was unreachable until now
  because io-harness emits the draw only from its contained loop.
- **A sixth rebindable action, `fleet`**, defaulting to `Ctrl+F`.

### Changed

- **io-harness moves from 0.64 to 0.65**, which makes `RunOutcome`
  `#[non_exhaustive]` and adds `AwaitingRecovery`. `io exec` maps the pause to
  its existing "paused" exit code and describes it; an outcome a later harness
  adds now exits as unfinished rather than breaking the build, so the property
  the old exhaustive match carried moved to a test that reads the variants out of
  the locked source and fails naming the one the table missed.
- `EventKind::RecoveryPaused` renders with the tool and the attempt id a recovery
  decision has to name, rather than as the muted word.

### Known limitations

- **A contained turn cannot be steered.** io-harness has no session entry point
  that takes a caller's containment and a steer inbox together, so a turn that
  fans out cannot be redirected while it runs. `Ctrl+C` still ends it, through
  the observer, at the next point where no child is in flight — the interface
  says that is what it is waiting for rather than appearing to have missed the
  key.
- **A contained turn applies no agent roster, no `[run]` budget and no
  `[sandbox]`.** It is built from the session's own default contract, the same
  reason a steered turn does not apply them. The containment table's own token
  ceiling is what bounds it.
- **A collected report is attributed to the tree and not to a child**, because
  `ChildCollected` carries no run id.
- **The view closes when the turn ends.** The tree is kept — `/fleet` reopens it,
  and every spawn, refusal and report is in the transcript — but the prompt comes
  back on its own rather than staying hidden behind a tree that has stopped
  moving.
- **The fleet view is four rows.** The viewport's height is fixed for the life of
  the terminal, and rebuilding it while a run is committing into scrollback is
  not a trade this release takes.

## [0.7.0] - 2026-08-18

The composer stops being a text box and becomes the way the product is driven.
A palette reaches every command and every prompt template, `@` completes paths
under the same permission boundary the agent runs under, `!` hands a line to
your own shell, a pasted file no longer floods the prompt, every picker filters
as you type, and the agent's own plan is on screen instead of being a word in
grey.

### Added

- **A slash palette.** `/` at an empty prompt opens a picker over every command,
  narrowing as you type and matched on a subsequence rather than a prefix, so
  `fk` reaches `/fork`. Enter puts the command in the prompt rather than running
  it, so you can see and edit it before it is sent.
- **Prompt templates in the palette.** Templates configured through `[run]
  templates` appear as rows carrying their name and description, and choosing
  one expands it into the composer. A templates directory that is missing or is
  not a directory is reported with io-harness's own message rather than being
  silently treated as an empty set.
- **`@` completes workspace paths**, one directory at a time, rooted at the
  session's own root and served by io-harness's `Workspace` under the policy the
  next turn will run under — so a path your posture denies is never offered.
  Listings are bounded per directory and a cut listing says so.
- **`!` runs a line in your own shell** — `$SHELL -c`, or `%COMSPEC% /C` on
  Windows — with its output, its errors and its exit status committed into the
  scrollback beside the conversation. The agent never sees the line. The child
  gets no terminal, so interactive programs such as `vim` and `less` are out of
  scope, and a slow command holds the interface until it finishes.
- **Type-to-filter on every picker** — the model list, the theme list, `/resume`,
  `/fork`, the palette and path completion. The query is drawn in place of the
  title, so it costs no row in a viewport that has four. `j` and `k` are query
  characters now; the arrows still move.
- **The agent's plan is rendered.** Each time the agent rewrites its todo list
  the whole list is committed to the scrollback, every item with its own state
  word, and the status line carries how much of it the agent claims is done. A
  plan longer than the store keeps says so rather than showing a trimmed one.
- **A large paste collapses to one line** naming what it is and how big, and is
  restored whole when the prompt is sent.

### Changed

- `/resume` offers every session the walk found rather than the twenty most
  recent. The cap existed only because a list nobody could filter was a list
  nobody could reach the bottom of.
- io-harness moves from 0.63 to 0.64.

### Fixed

- A paste during a turn was silently discarded, and a paste with a picker open
  was inserted into the composer hidden behind the overlay. Both now behave the
  way the surface they land on says they should.
- A picker's selection survived only as long as every keystroke matched
  something. One character that matched nothing lost it, and backspacing did not
  bring it back — so a typo before choosing could branch `/fork` from the first
  turn of a conversation, switch `/model` to the first of four hundred, or write
  a theme you never chose into your configuration.
- The theme step's live preview read a row index that could be fabricated when
  nothing matched, so typing a letter no theme contains changed the theme that
  would be saved.
- The matcher ranked a row that merely spelled your query above one that
  contained it whole, which on a real model catalogue put the wrong row first.
- An empty plan from the agent rendered as a plan of nothing and pinned `0/0` to
  the status line.
- The status line's tokens, context, containment and plan outlived the run that
  set them, so they went on describing a conversation you had already left after
  a resume, a fork or a rewind.
- A prompt holding only a large paste of whitespace could not be sent, would not
  let `Ctrl+D` exit, and said nothing about why.

## [0.6.0] - 2026-08-18

The interface can be read without being seen. Every mark has an ASCII form, the
one state a run does not narrate can be committed to the scrollback as text, the
cursor sits wherever input is expected, and the keys the session owns can be moved
to the ones your terminal and your muscle memory already have.

### Added

- **`--plain`** runs the session without animation: nothing turns, nothing moves,
  the ASCII glyph set is forced, and each state the session enters — `working`,
  `ready` — is committed into the terminal's own scrollback as a line of text.
  That one state is the only thing plain mode adds to the scrollback, and
  deliberately so: every other state a run produces already writes a line, and in
  the default interface this one is carried by a word that only ever repaints
  beside an indicator that only ever moves. It is a global flag, accepted on
  either side of a subcommand, and **`[app.io-cli] plain = true`** is the same
  switch for every session. The flag wins over the file, and there is no
  `--no-plain`: accessibility is switched on deliberately, and a mode that can be
  lost to a stray flag is not one to rely on. It reaches an interactive session
  and stops there — `io exec` builds no theme and animates nothing already.
- **An ASCII form for every mark.** Ten classes — the separator, the tool bullet,
  the selection marker, the ellipsis, the elision, the dash, the transcript rule,
  the quotes, the credential mask and the working indicator — now exist in two
  sets, chosen once at startup and carried to every surface, where before each was
  a literal typed at the place it was drawn. Every ASCII form carries its
  counterpart's *meaning* rather than merely standing in the same column: a
  product whose selection marker vanishes on a terminal that cannot draw it has
  lost the selection, not a decoration. **`[app.io-cli] glyphs`** names a set
  outright — `unicode` or `ascii` — and an absent key asks the locale: `LC_ALL`,
  then `LC_CTYPE`, then `LANG`, the first one present deciding whatever it says.
  The set is an axis of its own in both directions: `NO_COLOR` keeps the Unicode
  marks and the ASCII set arrives at a fully coloured theme, which
  `Theme::resolve` enforces structurally by taking the set as an argument and
  never deriving one. The IO CLI wordmark is the deliberate exception — it is
  suppressed when it cannot be drawn rather than transliterated, because a
  wordmark redrawn in `#` is a different and worse image wearing its name.
- **`[app.io-cli.keys]` moves the keys the session owns.** Five actions — `exit`,
  `posture`, `clear`, `transcript`, `rewind` — each take a chord, or two chords
  separated by a space, in the spelling VS Code, Zed and helix already write, so
  it is the one a reader guesses right on the first try. **`Ctrl+C` is fixed and
  is the only one that is**: it interrupts a running turn and leaves `io`, so a
  configuration file able to take it away is one able to lock an operator inside a
  running agent, and both spellings of the attempt — naming `interrupt`, and
  putting anything else onto `ctrl+c` — are refused with that reason rather than
  ignored. Nothing about a bad line is fatal or silent: an unreadable value leaves
  its action on the default and names the key it kept, a name that is no action
  says which names there are, and every notice is committed to the scrollback as
  the session starts. `/help` renders the table as the session actually behaves
  rather than the defaults that shipped — rebinding without a truthful table
  leaves the operator with documentation confidently wrong about the machine in
  front of them and no way to find out but by pressing keys.
- **The Kitty keyboard protocol is negotiated** where the terminal advertises it,
  which is what makes `Shift+Enter` a distinguishable key at all: without it a
  terminal sends the same `CR` for `Enter` and for `Shift+Enter`, and the trailing
  backslash was the only spelling there was. That fallback still works everywhere,
  and a terminal that does not advertise the protocol is written nothing. One flag
  is asked for, `DISAMBIGUATE_ESCAPE_CODES`; the other three are declined for
  stated reasons, the last of them because a terminal where typing stops working
  is the one risk this product must not take. What is pushed is popped on every
  path out of the process — an orderly exit, a `Drop`, a panic — and
  `tests/keyboard.rs` asserts the two balance in the byte stream.

### Fixed

- **Every frame that accepts input now sets a cursor position**: the composer,
  including at a width too narrow to draw it; the approval overlay; the selected
  row of a picker; and every step of the wizard. The terminal cursor is the focus
  indicator a screen reader follows, and a frame that leaves it where the last one
  put it reports focus somewhere the session is not reading from.
- **`NO_COLOR` survives the first-run wizard and `/theme`.** A theme picked at
  either is now *resolved* rather than assigned, so it is recorded as the
  preference it is and the session it was picked in stays uncoloured — and says
  so. The wizard also no longer seeds itself from the uncoloured theme's own name,
  which would have opened the picker on the wrong row and written down a
  preference no later launch could resolve.
- **A malformed `[app.io-cli]` is a notice rather than silence.** io-harness
  answers `Config::app` with three outcomes — parsed, absent, unreadable — and the
  old `.unwrap_or_default()` collapsed the third into the second, so one mistyped
  value silently reverted the theme and the diff style together with nothing said
  about either. The session now starts on the defaults carrying io-harness's own
  message, which already names the section and the key that broke; rewording it
  here would drop the only part that says where to look.
- **`Ctrl+C` closes an open picker.** Every arm of the picker's key handling
  matched on the key code alone, so `Ctrl+C` fell through to the idle arm and did
  nothing: the shipped table promised a key that inside a picker neither
  interrupted nor exited. It backs out rather than taking a second, picker-only
  meaning — the press closes the overlay and the one after it reaches the session,
  where the table's meaning is the one that applies.
- **An approval names the act in a word as well as in a colour.** The act was
  styled through a bare span rather than through the theme's notice, which left it
  the one place in the product where colour was the sole carrier of a meaning:
  under `NO_COLOR` the row read `write src/main.rs` with nothing on it saying a
  decision was being asked for. The word leads the row, because this viewport
  clips and the load-bearing fact must not be the part that goes.

### Changed

- **A frame whose content did not change is not drawn at all.** The frame is
  rendered into a probe terminal whose backend discards its output and remembers
  only where the cursor was asked to go; if the result matches what the terminal
  is already showing, nothing is written. The comparison is over the whole buffer
  *and* the cursor rather than over the viewport's text, because a picker
  highlight moving between two rows changes only a style and moving the caret
  through unchanged text is a real change with no cell behind it. The frame after
  a resize is never skipped: a resize clears the viewport.
- **io-harness moves to 0.63.0**, from 0.62.0.

### Notes

- No dependency is added. The direct set is the same ten names 0.5.0 shipped, and
  `tests/dependencies.rs` asserts that in both directions.
- The uncoloured theme is renamed internally from `PLAIN` to `MONO`, because
  `--plain` gives the word a second and unrelated meaning. Nothing user-visible
  changes: it was never a name a configuration file could select, under either
  spelling.
- `[app.io-cli] plain` distinguishes `Some(false)` from absent only so a file can
  state the default; the wizard writes neither it nor `glyphs` nor `keys`. A glyph
  set detected from the machine the wizard ran on would freeze into a file later
  read on another terminal, and plain mode is asked for rather than inferred.

## [0.5.0] - 2026-08-17

The same agent runs unattended. `io exec` runs one goal to completion with no
terminal, exits with a status a script can branch on, and with `--json` emits the
run's own event stream — the same events the interactive session renders, from
the same stream, with no interface code on the path.

### Added

- **`io exec "<goal>"`** runs one goal to completion without a terminal, prints
  the agent's reply on stdout, and exits with a status derived from io-harness's
  own `RunOutcome`. Six codes: `0` ended of its own accord, `1` never got that
  far, `2` a boundary said no, `3` a ceiling was reached, `4` stopped needing a
  human, `5` ended without finishing. The mapping is exhaustive by construction —
  a variant added by a later harness breaks the build rather than being folded
  silently into a wrong code.
- **`--json`** writes the run's events to stdout as newline-delimited JSON, one
  object per line and nothing else on stdout. The objects are
  `io_harness::RunEvent` serialized by io-harness's own derive, which is the same
  shape its `[[hook]]` writer appends to a file and its store keeps in
  `run_events.json` — so no format was invented here, and every event kind
  reaches the stream including the ones the interactive renderer cannot draw.
- **`--sandbox`** picks `read-only`, `workspace-write` or `full-access`, and
  **`--policy`** picks `workspace` or `read-only` in the same words the status
  line and the wizard use. `--policy ask-writes` is refused: nothing in a
  headless run can answer an approval, so honouring it would turn *ask before
  writes* into *deny writes* without saying so.
- **`--provider openrouter|anthropic|openai`** builds the provider from the
  environment, using the credential and model variables io-harness's own
  `from_env` constructors read — so a CI container needs nothing written to disk.

### Changed

- **`[sandbox]` limits and `[run]` budgets now apply to a headless run**, which
  is the first time either section has had an effect in this product. A run with
  nobody to steer it can hand the harness a task contract of its own, and that
  contract is what those sections travel in. An interactive session still cannot
  use them without giving up `Ctrl+C`.
- **A non-TTY stdout is no longer a refusal for `io exec`.** The check moved
  after the subcommand is known: a session still refuses to draw into a pipe, and
  its message now names `io exec` as the thing to use instead.
- Every provider is constructed in one place, reached by both the interactive
  session and `io exec`, so the next provider io-harness gains cannot arrive on
  one path and not the other.

### Notes

- `io exec` runs one goal and stops. There is no `io resume`, which is why every
  approval is declined rather than deferred and why exit code `4` cannot happen
  yet; it is mapped so that adding that subcommand later renumbers nothing.
- `RunEvent` carries no timestamp, so the JSON has none. Adding an envelope to
  supply one would make this a format io-cli owns rather than one it passes
  through.
- `serde_json` becomes the tenth direct dependency. It is already an
  unconditional dependency of io-harness, so nothing new enters the tree.

## [0.4.0] - 2026-08-17

Work survives the session. A conversation can be come back to, restarted from the
turn it went wrong at, moved to a different model, or undone.

### Added

- **`/resume` lists the sessions the store already holds** — each with its
  workspace, how many turns it ran, when it last ran and what it was first asked
  to do — and reopens the one you pick where it stopped, putting the conversation
  back into the terminal's own scrollback so you can read where you were. Every
  session `io` has ever run on this machine is there, including the ones from
  before this release: they were always being recorded, and nothing was reading
  them.
- **`/fork` continues from an earlier turn of the open conversation.** What came
  after the fork point is not deleted and not hidden — it stays in the store, and
  `Ctrl+T` marks it as branched away. That marking has been in the product since
  0.3.0 with nothing able to produce the state it renders; this is what produces
  it.
- **`/model` now changes the model.** It previously opened a picker holding one
  row and changed nothing at all, so a session that started on the wrong model had
  to be abandoned to correct it. The model list comes from the provider's live
  catalogue, the same call the first-run wizard makes, and a catalogue that cannot
  be read offers the configured model and says why rather than showing an empty
  list. No context is lost, because the conversation lives in the session and only
  the provider changes.
- **`Esc Esc` at an empty prompt undoes the last turn** — the files it wrote, the
  files it created, the notes it left, the children it queued, and the
  conversation head, so the next thing you type answers from where you actually
  are. The undo is written into the run's durable trace as a record of its own;
  nothing in the trace is deleted.

  It arms on the first press and acts on the second, and any other key cancels.
  This is the only key in `io` that changes your files on the interface's own
  initiative rather than the agent's, and the prompt says what it will cost before
  you confirm: **files go back to the state before that turn first wrote them, so
  anything you have edited by hand since is lost.** A path whose earlier contents
  could not be kept — over the snapshot cap, or not text — is reported as left
  alone, with the reason, ahead of anything that was restored.

- Three new rows in the key and command tables, which `/help` and the README
  render from the same constants.

### Changed

- **io-harness moves to 0.62.0**, from 0.60.1. Its run leases mean that two `io`
  processes driving one run now get a refusal instead of silently interleaving
  their steps into a single trace — which is exactly the hazard a resume feature
  introduces, so this release wants that version rather than merely tolerating it.

### Known limitations

- **A rewind does not check whether you edited a file yourself since the turn.**
  It restores from the snapshot taken before the run's first write and does not
  compare that against what is on disk now, so a hand edit made afterwards is
  overwritten. `io` cannot detect this — the snapshot is not readable from
  outside io-harness — so what it does instead is say so before the second
  keystroke. This is the behaviour of `git checkout -- <path>`. Making it
  preventable is an io-harness change.
- **The resume picker does not filter as you type**, so the list is bounded at the
  twenty most recent sessions, and it says when it has cut the list rather than
  quietly showing you a subset. Filtering arrives with the rest of the composer
  work in 0.7.0.
- **The resume picker cannot tell you which sessions another `io` process is
  driving.** Choosing one that is busy fails at the moment of use, loudly, rather
  than being greyed out in the list.
- A rewind undoes one turn — the last one. Walking a run further back is a
  surface with its own confirmation problem and is not in this release.

## [0.3.0] - 2026-08-17

The operator can read what the agent did to their files without leaving the
terminal and without losing the thread. An edit stops being a line saying a file
changed and becomes the change itself.

### Added

- **Edits render as diffs.** Not diffs io-cli computed — io-harness already
  renders a unified diff for every edit its tools make and keeps it in the run's
  durable trace, so what you see carries the file's own `@@` line numbers and is
  the same text `Store::patch` would hand `patch`. Additions and removals are
  coloured *and* marked, so the meaning survives `NO_COLOR`.
- **Word-level emphasis inside a changed line.** A run of removals is paired with
  the run of additions after it only when the two are the same length; anything
  else takes the whole wash. A `write_file` that rewrote two distant regions of a
  file arrives as one hunk spanning both, and a greedier rule would emphasise the
  difference between lines that have nothing to do with each other.
- **Syntax highlighting**, drawn in io-cli's own theme tokens rather than in a
  second palette. The three new tokens — keyword, string, literal — are the
  theme's, so a highlighted diff and the rest of the interface stay one look, and
  `NO_COLOR` is still decided in one place. Green still means added: the parts of
  a line both sides share are syntax coloured and the words that actually changed
  keep the diff's colour, so the add/remove colour now points at the exact words
  instead of washing the line.
- **A defined form below a hundred columns**, where word-level emphasis gives way
  to the line — a bolded fragment in the middle of a line that now takes three
  rows is harder to find than a whole row that is red. Nothing is truncated.
- **`diff = "minimal"` in `[app.io-cli]`** for reviewing by file rather than by
  hunk: the changed lines and the `@@` header, without the context. Its absence
  means `unified`, so no existing configuration file needs touching.

### Changed

- **An approval shows a write as a diff** against the file on disk, which is the
  clause of the approval surface 0.2.0 shipped unmet — the harness hands an
  approver the whole resulting file rather than a patch, so the old side is
  io-cli's to supply. A file that does not exist yet reads as all addition. At
  the tightest size the one row available is spent on `+3 -1` rather than on the
  first line of the change, because the size of a write is the decision.

### Fixed

- The answers row in the approval overlay no longer carries a double space after
  each separator.

## [0.2.0] - 2026-08-17

The operator can see the boundary the agent is working under, change it, and
answer it when it asks. Through 0.1.0 and 0.1.1 the approver handed to io-harness
was `DenyAll`, so the *ask before writes* posture declined every write and every
command it was named for; that dead end is what this release closes.

### Added

- **An approval overlay.** When an action needs permission the run stops and asks
  in an overlay that cannot scroll away, because a question committed to the
  transcript can be scrolled above the fold while the run is blocked on it. It
  states the act and the target, then the rule and the layer that are asking on a
  row of their own, then the content a write proposes. Answer it with `y` (allow
  once), `a` (allow for the rest of this session) or `n` (deny) — or with the
  arrows and `Enter`, since a key that only works for a reader who already knows
  it is not an interface. The overlay opens on the least committal answer.
- **Every decision in the transcript.** Answering commits exactly one line naming
  the act, the target and what was decided, so the decision is in the terminal's
  own scrollback as well as in the run's durable trace.
- **`Shift+Tab` cycles the permission posture**, and the status line names the one
  in force. It changes this session, like `/theme` and `/model`; `io setup` is
  what makes a choice permanent. It takes effect on the next turn, because
  io-harness takes a policy per turn. Both spellings a terminal can send —
  `BackTab`, and `Tab` with a shift modifier under the Kitty keyboard protocol —
  are the same key.
- **A refusal names its rule and its layer.** `write /etc/hosts · rule fs.deny ·
  layer ops-baseline` — the two facts no other terminal agent can print, because
  no other core records them. When no rule named the action, the line says the
  tier default decided rather than showing a blank: in io-harness that is the
  least vouched-for kind of action, not the most.
- **Three more status fields**: the tokens the session has spent, how full the
  assembled context was at the last fold, and how this run's commands are actually
  contained — the mode asked for *and* the backend that answered on this host,
  never the mode alone, which is an intention rather than a fact. Each is absent
  until something supplies it. A field that invents its own value is worse than no
  field.

### Changed

- The *ask before writes* posture now asks. Its description in the wizard said
  `declined until the approval surface lands`, which was true and is not any more.
- An outcome that stopped waiting on a human points at what to do about it now
  that there is something to do.

### Not in this release

- **Spend against the tree ceiling.** `EventKind::SpendDraw` is emitted only by a
  contained turn, and io-harness's contained entry point takes no steer inbox — so
  rendering spend today would cost `Ctrl+C`. It moves to 0.8.0, the fleet release,
  which is contained by definition.
- Diffs, syntax highlighting and collapsible tool output: 0.3.0. The harness hands
  an approver the full post-write content rather than a patch, so the overlay shows
  that content plainly and the diff surface is designed where it belongs.
- Deferring an approval, and approving a rewritten action. Both are real io-harness
  affordances; deferring is only useful alongside the resume that arrives in 0.4.0,
  and rewriting is an editor inside an overlay.
- Answering a question the agent asked about *intent*, which io-harness
  deliberately distinguishes from an approval about permission: 0.7.0.

## [0.1.1] - 2026-08-17

The session stops looking frozen while it works. Remediation of what 0.1.0
shipped, not new capability: no new key, no new command, no new setting, and
nothing about the permission boundary, the renderer or the wizard changes.

### Added

- **A moving indicator beside the state word.** A small animation next to
  `working`, advancing on the same tick that drives the clock. The word stays —
  it is what survives `NO_COLOR`, a screen reader and a log — and the motion is
  beside it, never instead of it. Suppressed entirely under `NO_COLOR`, where an
  animation is noise a reader cannot use.
- **A repaint tick.** The viewport redraws while a turn is in flight, so the
  clock advances and the indicator moves without an event having to arrive. It
  runs only while a turn is running: an idle session does not repaint, because a
  terminal interface that redraws forever is what this renderer exists not to be.
  Both halves are asserted against a clock the tests advance by hand, so no test
  sleeps and no test measures how long anything took.
- **A mechanical check that no test in the repository sleeps or reads a clock**,
  and that the driver is the only module that reads one at all.

### Changed

- **A step reads as a step.** The line is now the decision, then the tool it
  called with its target, then the result, with the token count and the step
  number trailing as muted detail. 0.1.0 put the step number and the token count
  in the middle of the decision. The result is stated in both directions —
  `changed files` or `no change` — so a transcript can be skimmed down one
  column instead of parsed.

## [0.1.0] - 2026-08-16

First release: a terminal interface over
[io-harness](https://github.com/initorigin/io-harness) that renders run events,
edits a prompt and reads a keyboard — and contains no agent loop, provider
client, tool, sandbox, policy engine or session store of its own.

### Added

- **A hybrid inline renderer.** Finished messages, tool calls and system lines
  are committed into the terminal's own scrollback; a few lines at the bottom
  hold the composer and the status line and are the only region that repaints.
  The alternate screen is never entered and the mouse is never captured, in any
  mode, behind any flag, so the terminal's own search, selection and copy-mode
  keep working after `io` exits. A streaming answer commits each line as it
  finishes, so the viewport is the same size after two hundred lines as before
  them.
- **A first-run wizard.** Provider, credential, a live verification call against
  the endpoint, model from that provider's catalogue, theme with the sample
  transcript re-rendering as the selection moves, and a default permission
  posture — then a screen naming the exact path and the exact contents, and
  nothing written until it is confirmed. Re-runnable as `io setup`. The
  credential is masked, never rendered, and not written at all when the
  provider's environment variable already carries it.
- **A `Picker` overlay**, built once and used by every selection surface, so the
  wizard, `/theme` and `/model` are visibly one product.
- **A composer** on `tui-textarea`: multiline on `Shift+Enter` with a `\` + Enter
  fallback for terminals that cannot report it, prompt history on the arrow keys,
  and bracketed paste so a pasted block is one prompt rather than several.
- **A status line** with the model, whether a turn is running, and elapsed time,
  laid out so 0.2.0's policy, context, spend and containment fields slot in.
- **Themes** — nine tokens, two shipped themes, terminal background detection,
  and `NO_COLOR`. Colour is never the only carrier of a meaning: every refusal,
  error and warning also carries a word.
- **Five slash commands** — `/help`, `/quit`, `/setup`, `/theme`, `/model` — and
  a documented keybinding table that the README and `/help` render from the same
  constants.
- **`Ctrl+C` interrupts the turn** through `Steer::interrupt` and keeps the
  session; the partial answer stays in the scrollback and the composer takes the
  next prompt. Twice at an empty prompt exits, and so does `Ctrl+D`.
- **Distribution**: prebuilt binaries for `aarch64-apple-darwin`,
  `x86_64-apple-darwin`, `x86_64-unknown-linux-musl` and `x86_64-pc-windows-msvc`
  attached to the GitHub Release with a `SHA256SUMS` beside them, plus
  `install.sh` and `install.ps1`, which verify the artifact before unpacking it
  and install into a per-user directory with no administrator rights.

### Notes

- 80×24 is a supported terminal size, not a degraded one.
- An action that needs approval is **declined** in this release and says so. The
  overlay that asks a human, and the refusal surface that names the rule and the
  policy layer, are 0.2.0.
- There is no crates.io publish and `cargo install` is not an install path.
- No test in this release asserts on wall-clock time.

[Unreleased]: https://github.com/initorigin/io-cli/compare/v0.1.1...HEAD
[0.1.1]: https://github.com/initorigin/io-cli/releases/tag/v0.1.1
[0.1.0]: https://github.com/initorigin/io-cli/releases/tag/v0.1.0
