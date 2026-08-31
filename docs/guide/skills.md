# Skills

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

**Delete one and it stays deleted.** `/skills`, choose it, *remove it for good*
— or `io skill remove io-mcp` from a shell — is the way to be rid of a shipped
skill: io remembers that it wrote that name, so it does not put the file back on
the next start. `rm ~/.io-cli/skills/io-mcp.md` does exactly the same thing, and
always has. A skill added in a *later* version has no such record, so upgrading
still brings you the new ones. If you only want one out of the way for now, turn
it off instead — that is reversible and `/skills` does it for you.

**And you can put one there.** `/skills add ./my-skill.md`, or `io skill add
./my-skill.md`, copies a skill file of your own into `~/.io-cli/skills` and
lists it as yours from the next turn. It is a **copy**: the file you named stays
where it is and stays yours. Two things are refused rather than done quietly — a
destination that already exists, because a skill is prose somebody wrote and
there is no undo, and a file whose `name:` is already claimed by another skill,
because two names resolving to one skill is the fatal case described below. A
bundle's skill is not yours to add or remove, and says so.

**`io skill add ./my-skill/SKILL.md` works, and until 0.33.0 it did not.** The
commonest layout on disk is a directory holding a `SKILL.md`, and the installed
file used to be named after the source's *file name* — so that add wrote
`~/.io-cli/skills/SKILL.md`, a shape io-cli then read as a folder skill and
refused to remove or disable forever, with a sentence about a directory that did
not exist. The installed file is now named from the skill's **own** name: the
frontmatter `name:` where there is one, the containing directory for a `SKILL.md`,
the file stem otherwise. `io skill add ./my-skill/SKILL.md` installs `my-skill.md`
and `io skill remove my-skill` takes it back out, and the name every check asks
about is the name a run will resolve rather than the word `SKILL`.

**A skill `/import` wrote as a folder is manageable too, and it never was.**
`/import` could write a skill into `~/.io-cli/skills/<name>/SKILL.md`, and neither
lever would touch that shape — so the product shipped a verb that created state
its own management surface refused to manage. Both work on it now: removing takes
the folder, and turning it off moves the whole `<name>/` into `disabled/` rather
than the file inside it. A loose `SKILL.md` sitting directly in the skills
directory is still refused by the off switch, and deliberately: the move keeps the
name, so it would land as `disabled/SKILL.md` and be re-offered as a skill called
*disabled*, taking every other parked skill's hiding place with it.

Until 0.30.0 the only thing that had ever written into that directory was
`/import`, following a tool io happened to detect. If you had written a skill
yourself, there was no door.

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

**A skill can also read the files beside it.** On io-harness 0.73.0 `read_skill`
takes an optional `path`, so a skill whose bundle ships `shared/style.md` or a
`references/` directory reaches them through the same tool and the same policy —
where before the only route was a shell command, which is a program run for what
is a read. Such a call is drawn with its path intact: the qualifier is
translated to the colon form and the path is not touched at all, because a path
is the bundle author's spelling and a translation applied to one is how
`src/__init__.py` becomes a file that does not exist.

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
turn was handed. They are in both now, qualified by the bundle they came from —
drawn `<bundle>:<name>` since 0.32.0, and addressed as io-harness's own
`<bundle>__<name>`, which is what a refusal or a tool call will name. See *The
name you read, and the name the model was shown* below.

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

---

[README](../../README.md) · [All guides](../CAPABILITIES.md) · [What you may depend on](../CONTRACT.md)

## The name you read, and the name the model was shown

A bundle's skill is qualified with the bundle it came from, so two bundles can
each contribute a `review` and both are addressable. io-harness joins the two
halves with a double underscore — `ultraship__brainstorm` — and that string is
load-bearing: it is what goes into the model's own catalogue, and `read_skill`
resolves it by exact match, so the model can only ask for the string it was shown.

**Since 0.32.0 you read `ultraship:brainstorm` instead.** The `/skills` list, the
`/` palette, the line in your scrollback, `/plugin`'s inventory and `/status`'s
policy rows all draw the colon form, and typing `/ultraship:brainstorm` runs it.
Nothing about the wire changed: io translates at the two edges — once on the way
to your screen, once on the way back from your keyboard — and io-harness never
sees a colon.

`io exec`, `io plugin` and `io skill` report the underscore form, because a script
reads their output and a script addresses the wire.

**A skill being read is drawn as a skill being loaded**, `Skill ultraship:plan ·
loaded`, and io-harness's own sentence about that call is not drawn beside it.
That sentence names the skill on the wire, so it was the one place the separator
reached you without passing through the translation — and the row above already
says what the call did. A read that fails or is refused still says so, in
io-harness's words: what is dropped is the decision, never the outcome.

**What you typed and what the model was sent deliberately differ.** Running
`/ultraship:brainstorm draft the release note` echoes your own line into the
scrollback, colon and all, because a transcript that rewrote what you typed would
be the only place in the session that does. The prompt behind it carries
`ultraship__brainstorm`, which is the string the model's catalogue holds and the
only string `read_skill` will resolve.

**Choosing a skill from the palette now writes a command rather than a sentence.**
It used to put `use the <name> skill: ` into the composer, which submits as an
ordinary prompt — so whether the skill actually ran depended on how the model read
an English request. It writes `/ultraship:brainstorm` now, and that dispatches.

A skill whose name collides with a built-in command resolves to the command. No
command contains a colon, so the two can always be told apart.
