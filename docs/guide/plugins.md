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

One directory can hand over six kinds of thing at once: skills, prompt templates,
`[[agent]]` definitions for a fan-out to draw children from, `[[mcp]]` servers,
`[[hook]]` tables, and policy layers. That breadth is why `/plugin` exists.
Every other capability in a session is one you put there — a skill file is yours
or io-cli's, an `[[mcp]]` entry is a line you wrote, a policy layer came from a
posture you chose. A bundle is a directory somebody else wrote that adds names to
six subsystems on one line, and *what did that directory put in my session* is a
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

A bundle contributes to six subsystems at once, and until 0.29.0 every one you
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
