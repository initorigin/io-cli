# Capability bundles

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

One directory can hand over seven kinds of thing at once: skills, prompt
templates, `[[agent]]` definitions for a fan-out to draw children from,
`[[mcp]]` servers, `[[hook]]` tables, `[[bin]]` executables, and policy layers.
That breadth is why `/plugin` exists. Every other capability in a session is one
you put there — a skill file is yours or io-cli's, an `[[mcp]]` entry is a line
you wrote, a policy layer came from a posture you chose. A bundle is a directory
somebody else wrote that adds names to seven subsystems on one line, and *what
did that directory put in my session* is a question whose only previous answer
was to open the manifest.

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

**From a shell it is `io plugin remove`, and it takes a directory or a name.**
`io plugin add` has always printed *`plugin remove <id>` takes it back out* and
the verb read its argument as a path only, so the sentence was false on the door
that printed it. The path is still read first and against the disk, so
`io plugin remove ./bundles/rust-review` means exactly what it always meant, and a
directory that is declared is always removed as one. Only when no configuration
file declares that directory is the word read as the name of a bundle you have
declared — across the loaded ones, the ones switched off and the ones that failed
to load, which are the entries you most want gone, since a bundle whose manifest
will not parse is one you cannot fix from the manifest.

**Two bundles of one name are refused, with both directories named.** A bundle's
id is unique among the ones io-harness actually *loaded*; two declared with
`enabled = false` may share one, which is the whole point of that flag. Taking
whichever was found first would delete a `[[plugin]]` entry you never pointed at,
silently, and you would find out when a bundle's skills stopped being offered. So
io says how many carry the name and prints each one's directory, which is the
spelling that tells them apart and the spelling the path reading above resolves.

The directory itself is never touched. This surface edits a configuration file,
and deleting somebody's work because they stopped loading it is not a thing a list
should do. Declaring a bundle is still a line you write yourself: a path is
something you type more comfortably into your own file than into a picker.

**Which file declared a bundle decides what it may contribute.** A bundle named
in the project-scoped `io.toml` — the file a `git clone` delivers — may
contribute skills, templates, agents and policy, and may **not** contribute
hooks, MCP servers or executables, because each of those three names a program
this machine would run. A project-scoped bundle that tries is refused **whole**:
it contributes nothing at all, not the half that would have been safe. Move the
`[[plugin]]` line into `io.local.toml` or into your user file and the same
directory loads completely. The rule is about which file names it, exactly as it
is for `[browser]`.

**A bundle's policy may only narrow.** Its layers may deny and may never allow: a
`[policy] defaults` table in a manifest is refused by name, and a single rule
whose effect is anything but `deny` drops the bundle. So the worst a bundle you
misjudged can do to your permission boundary is take something out of it.

**A bundle id must match `[a-z0-9][a-z0-9-]{0,31}`**, and every name it
contributes is rewritten by io-harness to `<bundle>__<name>` at load — an agent's
name, an MCP server's id, a policy layer's name. `/plugin` draws it **qualified**
rather than shortened, because the qualifier is what a refusal will name, what a
tool call will name, and what you will type to spawn the agent; a bare `reviewer`
would say nothing about where it came from.

Since 0.32.0 the qualifier is drawn with a colon — `rust-review:reviewer` — while
io-harness keeps its own separator on the wire. That is a translation at the edge
and not a third spelling: `io plugin list` and `io exec` report the underscore
form, because a script addresses the wire.

**From 0.34.0 the translation has no exceptions left.** Three places still put
`__` in front of a person: the cell for a skill read, the sentence io-harness
writes about that read, and an MCP tool call, which was drawn as the raw
`mcp__github__create_issue` and now reads `Call github:create_issue`. A gate
walks the drawn output of the transcript, the status line, the pickers and the
plugin, skill and marketplace panes and fails if the separator appears in any of
it, so the next surface to draw a bundle's name cannot reintroduce the underscore
quietly.

**Hooks are named, everywhere, from 0.30.0.** Each one gets a row: the event it
fires on, or the tool call it sits in front of, and the command it runs. This is
the contribution kind that runs programs, so it is the one you most need itemised
and it was the one io could not itemise: io-harness kept its `Hook` type private,
so through 0.29.0 `/plugin` drew a row saying the bundle contributed hooks and
that io could not say what they do, and only a marketplace install named them —
by reading the manifest itself, which is a second opinion about somebody else's
file.

io-harness 0.71.0 closed that (io-harness#223, reported by this project). Both
surfaces now read `Plugin::hooks()`, so what you are shown is what the harness
parsed rather than what io-cli made of the same bytes — and it sees two shapes
the hand-written reader could not: an inline `hook = [{…}]` array, and a
`[[hook]]` header with a comment after it.

## An executable a bundle ships

**A bundle can carry a program, and from 0.34.0 a run can find it.** io-harness
0.73.0 adds a `[[bin]]` table naming one:

```toml
[[bin]]
name = "review"
path = "bin/review"
```

`path` is relative to the bundle and may be neither absolute nor a climb out of
it with `..`; a manifest that tries is refused whole. A bundle contributes an
executable it ships, not one it points at somewhere else on your machine.

**io-harness places nothing and says so in its own contract.** What it hands back
is the declared name and the path resolved against the bundle root. Making that
program invocable is io-cli's, and it is one edit: the directory the declared
file sits in is **appended** to io's own `PATH`, which every command a run spawns
inherits. Appended, never prepended — a prepended directory lets anything a
bundle ships answer to `git`, `cargo` or `ls` for every tool call in the session,
and the permission gate that would stop it matches a binary *name*, which the
wrong program under the right name satisfies. Appended, a collision resolves to
the system command and the bundle's program is simply unreachable under that
name: the failure you can read rather than the one you cannot. Your shell's own
`PATH` is untouched.

One entry per directory, sorted, and only from bundles that are switched on — a
bundle written `enabled = false` is parsed in full and contributes nothing, and
its programs would otherwise be the one thing it still contributed.

**io creates no file.** The program resolves under the name it already has on
disk, so a `[[bin]]` whose `name` is not that name is reported at startup and
does not resolve: `name = "review"` against `path = "bin/review.mjs"` puts the
directory on the path and leaves a program that answers to `review.mjs`. Nothing
is written to close that gap — a wrapper or a link io wrote inside somebody
else's bundle is a file io would then own in a directory it did not install, and
io-cli installing a program is outside what this product does. Renaming it is the
bundle author's edit to make.

**A `[[bin]]` names a program, so the project-scope rule covers it.** A bundle
declared in the committed `io.toml` that carries one is refused whole, exactly as
one carrying a `[[hook]]` or an `[[mcp]]` is.

**And writing one costs an older binary the whole bundle.** An io-cli built
against io-harness 0.72.0 or earlier does not know the key, and a manifest is
`deny_unknown_fields` — so a `plugin.toml` carrying a `[[bin]]` is refused
entirely by that binary, taking every skill, agent, hook and layer in the bundle
with it. That matters for the manifests io generates for a Claude Code or Codex
bundle under `~/.io-cli/adapters/`, because two versions of io reading one home
is the ordinary case during a downgrade.

## Marketplaces

**A marketplace is a git repository you name.** It is cloned into your own home
and read for the bundles it publishes:

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

### Three manifest formats, and which one wins

A capability bundle in the field is a Claude Code plugin or a Codex plugin.
`plugin.toml` is a format this project writes and nobody else does, so io reads
all three rather than asking anyone to adopt a fourth:

| File | Read as |
| --- | --- |
| `plugin.toml` | A bundle, natively |
| `.claude-plugin/marketplace.json` | The repository's own index of what it publishes |
| `.claude-plugin/plugin.json`, `.codex-plugin/plugin.json` | A bundle manifest |

The precedence is stated rather than discovered:

- **A `plugin.toml` at the repository's root suppresses the index.** An author who
  writes io's own manifest has said what they publish in the format io owns, and a
  foreign index does not speak over it. It suppresses the index and nothing else —
  a repository with a root manifest and bundles beneath it still lists all of them.
- **Where there is no root `plugin.toml`, the index is the answer** and the
  directory walk does not also run. A union would list bundles the author did not
  publish beside the ones they did, and you would have no way to tell which was
  which.
- **Where there is neither, the walk runs**, reading each directory as a
  `plugin.toml` first and a foreign manifest only where it carries none.

An entry in a shape io does not read is **listed with its reason**, never dropped.

**An index may place a plugin in another repository**, and 238 of the 291 entries
in `anthropics/claude-plugins-official` do. Those are fetched when you install
one, at the commit the index names where it names one. io re-derives the
repository from the url and rebuilds it: a url that is not `<owner>/<repo>` on
GitHub is refused, because the only string io hands `git` is one io built.

**An adapted bundle is marked as adapted in `/plugin`**, under its own mark, with
the generated manifest's directory on the row — that file is what you open when
io-harness drops the bundle, and nothing else names it. io writes it under
`~/.io-cli/adapters/<owner>/<repo>/<name>/plugin.toml`, never inside the clone.
The directories it contributes are **copied into it** and named relatively, so
io-harness loads it as an ordinary bundle. The author's checkout is not written
to.

Until 0.35.0 those paths were absolute and pointed back into the clone. io-harness
0.74.0 refuses that in every scope, and the reason is one this product would give
itself: every `*.md` under a contributed directory is read into the model's system
prompt on every turn, so a manifest must ship what it contributes rather than
point at somebody else's checkout. The adapter now holds a copy.

**So installing again is how you update.** A `git pull` of the marketplace clone
changes the clone, not the copy. `io plugin add <name>` regenerates the adapter and
tells you which directories moved; it writes no second entry when one already names
the bundle, and a refused refresh leaves the installed adapter exactly as it was.

**A plugin name that is not a usable id is refused rather than mangled.** An id is
what you type at `plugin add` and what prefixes every name the bundle
contributes, so io accepts a name that is already an id, folds one that becomes an
id by lowercasing, and refuses everything else. Two entries in one index reaching
the same id are refused naming both.

### Hooks do not cross, and that is not a gap

**A Claude Code or Codex plugin's hooks are not carried across.** io-harness's
hooks are argv against its own events, deliberately never a shell string; a hook
in those formats is a shell line, an event io does not know, and a `${…}`
substitution io-harness refuses inside a manifest in every scope. All three at
once, and no adapter closes any of them.

So io **shows** them instead. Every hook a bundle declares is drawn before you
install it, with its event, its command **unshortened**, and the reason it will
not run. An approximated hook is a program running on your machine that nobody
described accurately, and a shortened one is a command you consented to without
reading.

If you want hooks under io, the repository's author adds a `plugin.toml` — one
file, which then wins its own root.

**Removing a marketplace removes the clone and nothing else.** A bundle you
declared out of it keeps its `[[plugin]]` entry — a cache being emptied is not a
reason to undo a decision you made about your configuration. What io owes you
instead is the consequence, so it names the bundles that will stop loading before
it deletes anything.

### What a bundle is allowed to do is shown before it is allowed to do it

A bundle contributes to seven subsystems at once, and until 0.29.0 every one you
declared came from a directory you had read. A marketplace removes that reading,
so the install puts it back.

**Nothing is written to your configuration before you agree to it.** io-harness
reads, parses, validates and trust-checks the directory with your file untouched,
and hands the bundle back contributing nothing. What you are shown is what
io-harness parsed: the skills and template directories, the agents, the MCP
servers, the hooks and the policy layers, in the **namespaced** names you will
actually see in a trace and type to spawn an agent. A bundle io-harness would
refuse is refused at that point, in its own words, before you are asked anything.

Saying yes writes the `[[plugin]]` entry, once. Saying no writes no byte at all —
there is nothing left behind to find later, and nothing to undo.

Hooks are disclosed from the same reading as everything else, so the list names
the programs rather than the bare word "hooks". Consenting on the bare word is
consenting to programs nobody named.

**Writing `enabled` costs something and io says so at the time.** An io-cli built
against io-harness 0.69.0 does not know the key and refuses the *whole file*
rather than ignoring it. Remove the `enabled` keys before downgrading.

---

[README](../../README.md) · [All guides](../CAPABILITIES.md) · [What you may depend on](../CONTRACT.md)

## When io re-reads your bundles

Reading a bundle is not free: every declared `plugin.toml` is opened, parsed,
validated and trust-checked, and every skill file inside it is read in full to
recover its name and description. Until 0.32.0 io did all of that **twice for every
message you sent**, and again each time you opened `/plugin` or `/skills`. With a
few bundles installed that is a pause before every turn.

**It is now read once for the session**, at startup, and again only when something
on disk has moved.

**How io decides it moved.** It stats each declared bundle's `plugin.toml` and
compares the modified time and the length against what it recorded. That is a
handful of `metadata` calls, not a re-read — which is why `/plugin` and `/skills`
open instantly when nothing has changed.

A bundle installed or removed is **always** seen, because the declared set itself is
compared rather than each entry in it.

**The limit, stated rather than implied.** On a filesystem whose modified-time
granularity cannot separate two writes inside one second, a second edit that leaves
the file exactly the same length is not distinguishable from the first, and io will
not notice it until something else about the bundle changes. Restart the session,
or touch the file, if you have hit that. io says this rather than claiming a
freshness it cannot prove.
