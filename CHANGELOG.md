# Changelog

All notable changes to this project are documented here. The format follows
[Keep a Changelog](https://keepachangelog.com/en/1.1.0/) and the project uses
[Semantic Versioning](https://semver.org/spec/v2.0.0.html), staying on `0.x`.

## [Unreleased]

## [0.34.0] - 2026-08-31

0.32.0 made io-harness's `__` a wire detail: a bundle's contribution is drawn with
a colon and translated back at the keyboard. It left three places where the
separator still reached a person — the sentence io-harness writes about a skill
read, the repetition guard that stopped recognising that sentence, and every MCP
tool call, drawn as the raw `mcp__github__create_issue`. This release closes all
three and puts a gate under the rule, so the next surface to draw a contributed
name cannot reintroduce the underscore quietly.

The pin moves to io-harness 0.73.0, which adds a seventh kind of contribution. A
bundle can ship an executable, and until now it contributed it to nothing: a model
could not tell *not installed* from *installed somewhere I may not look*. io-cli
makes it resolvable by appending the file's own directory to the run's `PATH` —
appended, so nothing a bundle ships can shadow a system command — and creates no
file to do it.

### Added

**A bundle's `[[bin]]` is invocable inside a run.** io-harness 0.73.0 accepts the
key, checks that the path stays inside the bundle, and places nothing itself; io
appends the directory holding the declared file to its own `PATH`, which every
command a run spawns inherits. Appended and never prepended: a prepended directory
lets anything a bundle ships answer to `git`, `cargo` or `ls` for every tool call
in the session, and the permission gate that would stop it matches a binary
*name*, which the wrong program under the right name satisfies. One entry per
directory, sorted, and none from a bundle switched off. No file is written and
nothing is linked, so the program resolves under the name it already has on disk —
a `[[bin]]` whose `name` is not that name is named at startup and does not
resolve, rather than being papered over with a wrapper io would then own inside
somebody else's directory. A bundle declared in the committed `io.toml` that
carries one is refused whole, exactly as one carrying a `[[hook]]` or an `[[mcp]]`
is. A bundle installed mid-session has its program placed at the next turn, with
everything else it contributes; on Windows a declared `review` shipped as
`review.exe` is correctly authored and is not reported, because `PATHEXT` is what
decides there.

**A skill can read the files beside it without a shell.** `read_skill` takes an
optional `path` on io-harness 0.73.0, so a bundle's `shared/*.md` and
`references/*` reach the model through the same tool and the same policy as the
skill body. Such a call is drawn with its path intact and untranslated: the
qualifier takes the colon and the path is left exactly as it was asked for.

**`tests/namespacing.rs` walks six operator-facing surfaces** — the transcript,
the status line, the pickers, and the plugin, skill and marketplace panes — and
fails when one of them draws a contributed name still in its wire spelling. Not
"the separator appears nowhere": a path carries one legitimately, and only the
first separator is the join, so `bundle__deep__nested` is drawn
`bundle:deep__nested` and keeps one. The rule had three exceptions and no gate,
which is how it had three exceptions.

### Changed

**A skill read is drawn as a skill loaded**, `Skill ultraship:plan · loaded`, and
io-harness's own decision sentence is not drawn for that tool. That sentence names
the skill on the wire, and the row above it already says what the call did — so
this removes the separator at its source rather than translating a sentence
io-cli did not write. A read that fails or is refused still says so, in
io-harness's words.

**An MCP tool call reads `Call github:create_issue`.** Its wire name is assembled
from a prefix, the server id and io-harness's separator, so
`mcp__github__create_issue` was a string nobody wrote and nobody reads.

**Invoking a bundle's skill echoes what you typed.** `/ultraship:brainstorm …`
goes into the scrollback as you wrote it, colon and all, while the prompt behind
it carries `ultraship__brainstorm` — the string the model's catalogue holds and
the only one `read_skill` resolves. The two differ deliberately: a transcript that
rewrote the line you typed would be the only place in the session that does.

**A bundle contributes seven kinds of thing, not six.** `Plugin::contributions()`
reports `bin` after `hooks` and before `policy`, and the pane, the guide and the
README say seven.

### Fixed

**The repetition guard stopped recognising io-harness's decision sentence.** A
pending call kept only the displayed target, so `trim_result` compared a sentence
naming the wire name against a call naming the colon form and printed both. The
pending call now keeps io-harness's own target beside the drawn one, which repairs
it at the cause rather than by matching two spellings at the comparison.

### Dependencies

- io-harness 0.72 → **0.73.0**. `Plugin::contributions()` gains `bin`, a manifest
  gains `[[bin]]`, and `read_skill` gains an optional `path`. The direct dependency
  set is unchanged at ten names.

### Upgrading

- **A `plugin.toml` carrying a `[[bin]]` cannot be read by an io-cli built against
  io-harness 0.72.0 or earlier.** `Manifest` is `deny_unknown_fields`, so that
  binary refuses the manifest **whole** and drops the bundle entirely — every
  skill, agent, hook and layer in it, not just the `[[bin]]`. This matters where
  two versions of io read one home: once io has written a `[[bin]]` into a manifest
  it generates under `~/.io-cli`, the older binary sees the bundle as gone rather
  than as shortened. Remove the `[[bin]]` tables before downgrading.

## [0.33.0] - 2026-08-31

io-harness 0.72.0 lets an agent ask several things at once and lets a choice carry
a description and a preview. All of it reached io-cli the moment the pin moved and
none of it was drawn: a batched ask emitted an event the transcript had no
disposition for, a described choice was a label with its sentence thrown away, and
a question that accepted several answers accepted one. A resumed batch was worse
than blind — it came back as one row of numbered prose with an empty choices
column, so what an operator saw was a wall of text and nothing to pick.

This release makes the question surface the one surface. A batch arrives as one
overlay and is answered in place; an offer explains itself and can show what
taking it would do; `Space` marks a set where the question takes one.

The other half is a write nobody chose. `/config` was refused mid-turn because its
bare list carried a row that re-read the provider's catalogue and wrote a scope
file — and because `Left` and `Right` on a row wrote the file on the keystroke,
with no confirmation, which was the only such write in the product. Both are gone
rather than guarded, and the bare `/config` reports while a turn runs.

### Added

**An agent can ask several questions at once and they arrive as one overlay.** One
question is on the screen at a time, with the same offers, context and free-text
row a single question has. Deciding one moves to the next undecided one; deciding
the last delivers the whole batch. `PgUp` and `PgDn` walk it, and a decided
question re-opens with your own answer back in the composer. Two lines of the head
say which question of how many, and what this one was already decided as.

There is no review pane and no submit key, deliberately. The answers are already
on the screen they were typed into, one page-key apart; a second rendering of them
is a second thing that can disagree with the first, and a submit key that answers
nothing is the reflexive `Enter` this overlay has always been careful about.
Nothing is sent until every question is decided, because io-harness commits a batch
only when every entry has an answer — four of five parks the run as thoroughly as
none. `Esc` decides the question on the screen as *nobody here can answer this* and
moves on; the run still parks, which is all `Esc` has ever promised.

**A choice can explain itself.** A description is drawn on a row of its own under
the label and stays there, because comparing five offers needs all five sentences
at once. A preview unfolds beneath the offer under the marker, one at a time, and
folds when the marker moves — five blocks at once is a wall nobody reads. It is
marked the way this product already marks quoted words, since a preview is
somebody else's text. `Enter` on an offer whose preview is open still answers with
that offer.

**A question that takes several answers takes them.** `Space` marks and unmarks
the offer under the marker, the offers carry a box from the moment the list opens,
and `Enter` sends the marked set — or, with nothing marked, the offer you are
looking at, because an empty answer is information the agent did not have and
would now believe. The set is joined by io-harness's own speller, so two interfaces
answering one question produce the same text. Marks are held against the list you
were given and survive the query that hides a row: an operator narrows a list in
order to find each row to mark, and a filter that un-marked as it went would throw
away the marks made under the last query.

**`/config` runs while a turn is in flight, in its bare form**, taking the
mid-turn set from ten commands to eleven. `/config <key>` and `/config <key>
<value>` descend toward a write and keep their refusal — the first time the
run-state guard has read past a command's first word. The whole-command refusal
still covers `/plugin`, `/mcp`, `/provider`, `/skills`, `/memory` and `/store`.

**`io plugin remove` takes a bundle's name as well as its directory.** The path is
read first and against the disk, so an existing script means what it always meant.
Two declared bundles of one name are refused with both directories printed rather
than resolved by order.

### Changed

**No arrow key writes a configuration file.** `Left` and `Right` on a `/config`
row used to step a boolean or a closed set of words to its next value and write
that value into a scope file on the keystroke, unconfirmed. They now open that
setting's values with the marker on the value in force, and `Enter` is the
confirmation every other managed surface already uses.

**The price refresh moved off the bare `/config` list** to one descent below
`prices.as_of`, the date it writes, with *leave it* at row 0. It was the last row
of the list, which made a keystroke on a surface whose whole job is reporting read
the network, write a file and reassign the running turn's configuration.

**`io resume --list` says how many questions a parked row is waiting on.** A
batched ask is one row with one id and one `--answer` answers all of it — there is
no per-question flag, because io-harness parks a batch as a single row and records
a single reply against it. The refusal now says so and tells you to number your
answers to match. The questions themselves are counted rather than pasted into a
one-line detail, which read as if the first were the whole ask.

### Fixed

**`io skill add <dir>/SKILL.md` installed a skill that could never be removed.**
The destination was named from the source's file name, so the commonest layout on
disk wrote `~/.io-cli/skills/SKILL.md` — a shape io-cli read as a folder skill and
then refused both to remove and to disable, forever, with a sentence about a
directory that did not exist. The installed file is named from the skill's own
name now, and `io skill remove <name>` takes it back out.

**A skill `/import` wrote as a folder could not be removed or turned off.** The
product shipped a verb that created state its own management surface refused to
manage. Both levers work on it, and a disabled folder skill stays visible on the
surface that re-enables it. Disabling a loose `SKILL.md` sitting directly in the
skills directory is refused: the move keeps the name, so it would land as
`disabled/SKILL.md` and be re-offered as a skill called *disabled*, taking every
parked skill's hiding place with it.

**`io plugin add` printed `plugin remove <id> takes it back out` and the verb read
its argument as a path**, so the sentence was false on the door that printed it.

**A batched ask emitted an event nothing in the transcript could dispose of.**
io-harness 0.72.0 emits `QuestionsAsked` for a batch and does not also emit
`QuestionAsked`, so the surface this release exists for was the one ask the
transcript was blind to. The declared event kinds move from 51 to 52.

**A resumed batch came back as a wall of text with nothing to pick.** The parked
row's question column is numbered prose and its choices column is empty, so a
resume read only those and drew both. The questions are carried whole now.

**Thirty-four of the forty-six in-page links in the documentation went nowhere.**
0.30.2 split a 2,847-line README into nineteen guide pages and moved the text
faithfully, but every anchor kept pointing inside the file it had left. The link
gate skipped fragment-only links by design, so nothing caught it and nothing
stopped it growing. Every anchor now names the page that holds the heading, and
the gate resolves both spellings instead of skipping one. Found while bringing this
release's own prose back to true, and repaired here rather than left for a version
that would have had to find it again.

**A comment said io-harness substitutes two forms when it substitutes three.**
`${cmd:…}` is the third, and the same sentence went on to say a manifest refuses
all three. The correction had already been made once, in 0.21.0, in the other file
that carries the claim.

### Known limitations

**A single question that takes several answers loses that fact when it parks.**
io-harness's `PendingQuestion` has no column for it and the singular writer records
none, so a lone multi-select that parks and is resumed comes back as a pick-one. A
batched ask keeps it, because a batch carries its questions whole. This is
upstream, and it is stated rather than papered over with a default that would read
as a fact.

## [0.32.0] - 2026-08-30

The agent could ask you a question and you could not answer it. `Intent::render`
drew the answer composer only where there was room left over, and a question with
a context line and five choices filled all eight rows of the viewport — so what
you actually saw was a question, some inert bullets, and nothing to type into.
The same question reached the screen twice, and the second copy was labelled
`warning:`.

That fixed eight-row viewport is one cause behind four defects, so this release
makes the viewport the size of what it has to show. The rest of what is here
follows from it: a plan overlay that keeps its own footer, a queue you can read,
pickers that say how much they are not showing.

Alongside it, the interface stops withholding things it already does. Ten
commands run while a turn is in flight. A message typed mid-turn reaches that
turn. A bundle's skill is drawn under a name you can read and type. The token
figure moves while the tokens are being spent. And the plugin set is resolved once
for the session rather than twice for every message.

### Added

**The viewport grows to what a surface asks for, and gives the rows back.** A
question overlay asks for its offers and its composer, a plan overlay for its
steps and its footer, the queue for one row per message, a picker for its list.
The ceiling is your terminal's height less four rows — not a ration, but the
exchange the surface is about, kept visible. On an 80×24 that is a twenty-row
viewport. `VIEWPORT_HEIGHT` is a floor now rather than a fixed size.

**The question overlay is answerable, as one list with no modes.** The agent's
offers and a row for an answer nobody offered are peers in the same list. `Enter`
on an offer sends it verbatim; the last row unfolds a composer directly beneath
it and typing there sends prose. Typing anywhere moves the marker to that row, so
you can simply start writing. The marker opens there too, never on the agent's
first suggestion — a reflexive `Enter` must not become silent agreement with
something you have not read.

**`Tab` takes the row under the marker, in every list in the product**, and
`Shift+Tab` steps back. Both were unbound and did nothing.

**Ten commands run while a turn is in flight**: `/status`, `/context`, `/cost`,
`/stats`, `/help`, `/theme`, `/copy`, `/expand`, `/fleet` and `/image`. `/` opens
the palette mid-turn and `@` completes a path. Everything that reassigns the
session or the provider, writes the store or a configuration file, or submits a
turn keeps its refusal.

**A message typed during a turn is delivered to that turn**, at its next step
boundary, and recorded in the transcript as it goes. `/steer` is unchanged. A
message that reaches no further step boundary still runs as its own turn
afterwards.

**A live token figure**, estimated from the streamed text and written `~1.2k tok`
so it cannot be read as settled. The provider's own number replaces it when the
step commits.

### Changed

**A bundle's contributions are drawn `bundle:name` rather than `bundle__name`** —
on `/skills`, in the palette, in the `Read skill` line, in `/plugin`'s inventory
and on `/status`. io-harness's separator is unchanged and still what the model
sees; io translates at the two edges. Choosing a skill now writes the command
`/bundle:name` instead of the sentence `use the <name> skill: `, which submitted
as an ordinary prompt and left it to the model to interpret.

**The plugin set is resolved once per session** rather than twice on the build
path of every turn, and re-read only when a declared manifest's stamp changes.
`/plugin` and `/skills` cost nothing to open when nothing has moved.

**Tones say what they mean.** `refused:` is reserved for an act the permission
boundary refused. A mistyped `/effort` argument, a run that is simply not in the
store, three watch errors and two `/memory` bookkeeping lines were all wearing it;
a plan proposal and a question were drawn as warnings. All corrected.

### Fixed

**A question is drawn once, and never as a warning.** It was committed to the
transcript and redrawn by the overlay, with the overlay's copy prefixed
`warning:`. The transcript line survives wherever nothing else will draw it —
under `--plain`, and on a resumed run, which has no overlay in this process at
all.

**Overlays measured themselves by counting lines while rendering through a
wrapping paragraph**, so a long question or plan step consumed more rows than the
count admitted: the offers and the footer fell off the bottom, and the composer
was then drawn over rows already painted. Everything measures wrapped rows now.

**The plan overlay keeps its `Enter approves / Esc cancels` footer.** It was the
last line pushed and so the first line lost, on the one overlay whose own
documentation forbids exactly that.

**Truncation says what it dropped.** A picker over four hundred rows, a fleet view
with more children than rows, and `/stats`' per-table breakdown all cut silently;
`/stats` was capping at five on a page with unlimited rows.

**`outcome_help` told you a parked question or plan could not be answered by this
release and to say it in your next prompt.** `/resume` has answered both since
0.23.0.

## [0.31.0] - 2026-08-30

io-cli has operated a marketplace nobody could stock. Every capability bundle
published in the field is a Claude Code plugin or a Codex plugin, and
`plugin.toml` is a format only this project writes — so the surface 0.29.0 and
0.30.0 built, disclosed and gated was reachable only by a bundle you had written
yourself, which is the one case that never needed a marketplace. A survey of the
marketplaces on the author's own machine found five repositories publishing 304
plugins between them and zero `.toml` files among them.

From this release io reads the three formats a bundle is actually published in.
The bet is that a plugin's value is in its skills, its templates and its agents
rather than in the syntax of its manifest, and that a tool which reads three
formats is better placed than one that asks 291 strangers to adopt a fourth.

### Added

**`.claude-plugin/marketplace.json` is read as a repository's index**, and
`.claude-plugin/plugin.json` and `.codex-plugin/plugin.json` as bundle manifests
wherever the existing walk already looks. The precedence is stated: a
`plugin.toml` at a repository's root suppresses the index and nothing else; where
there is no root manifest the index is the answer and the walk does not also run,
because a union would list bundles the author never published beside the ones
they did; where there is neither, the walk reads each directory natively first and
foreign second. An entry in a shape io does not read is listed with its reason
rather than dropped.

**An index may place a plugin in another repository**, which 238 of the official
marketplace's 291 entries do, and installing one fetches it at the commit the
index names. `git clone --revision` is not available below git 2.49, so a
commit-pinned shallow fetch is four invocations rather than one.

**An adapter manifest is generated** under
`~/.io-cli/adapters/<owner>/<repo>/<name>/plugin.toml`, with absolute paths into
the clone, so io-harness loads the bundle with no change to that crate. The
stranger's checkout is never written to. `/plugin` draws an adapted bundle under
its own mark with the generated manifest's directory on the row, so the
difference between what an author wrote and what io generated is not something
you have to infer, and the file to open when io-harness drops a bundle is named.

### Changed

**Skills follow `$IO_CONFIG` and `$IO_CONFIG_HOME`, on both the read and the
write.** They resolved through io-cli's default home while your memory file
resolved through the home actually in force, so pointing either variable
elsewhere moved one and not the other. A skill is something you wrote, and it
belongs where the rest of what you wrote is. The limitation this removes was
recorded in `docs/CONTRACT.md` since 0.30.1.

**io does not move skills you already have**, and it says so rather than leaving
you to notice. If you have set either variable and your old `~/.io-cli/skills`
still holds anything, io names both directories at startup — a release that moved
only the read would have had `/skills add` writing where nothing looks.

### Not included, deliberately

**A Claude Code or Codex plugin's hooks are not carried across.** io-harness's
hooks are argv against its own event tags and deliberately never a shell string,
and it refuses `${env:}`, `${file:}` and `${cmd:}` inside a manifest in every
scope — so a hook in either foreign format is a shell line, an unknown event and a
refused substitution at once. No adapter closes that, and an approximated hook is
a program running on your machine that nobody described accurately.

So every hook a bundle declares is **shown** before you install it, with its
event, its command unshortened, and the reason it will not run. If you want hooks
under io, the repository's author adds a `plugin.toml`, which then wins its own
root.

io also does not become compatible with another tool's runtime — the variables it
sets, the directory it runs a hook in, its permission model or its session
lifecycle. io reads what a manifest says and nothing about how another program
would have executed it.

## [0.30.2] - 2026-08-30

A documentation release. Thirty-one claims across the README, the changelog, the
contributing and security policies, the configuration reference and the shipped
skills did not survive being checked against the code that decides them. Two of
the thirty-one turned out to be defects in the product rather than in the prose,
which is the argument for treating a documentation pass as a gate: writing down
what a surface does is how you find out it does something else.

### Security

**A vulnerability had nowhere to be reported.** `SECURITY.md` forbade opening a
public issue and then gave a literal `<project-contact-email>` as the alternative,
and `CODE_OF_CONDUCT.md` carried the same unfilled placeholder. The README routes
every report at `SECURITY.md`, so the dead end was the only documented path, and
it had been there since 0.1.0. Both now route through GitHub's private
vulnerability reporting on this repository. No address is published.

### Fixed

**`io skill <bad-verb>` answered a sentence that contradicted itself.** The arm
listing a surface's verbs covered `mcp`, `plugin` and `config`, so a mistyped verb
on the fourth surface fell through to the unknown-*surface* arm and replied
"`skill` is not a surface io manages; they are `mcp`, `plugin`, `skill` and
`config`" — denying and asserting the same fact in one breath, and never naming
`add`, `list` or `remove`. The arm beside it already listed all four. Nothing in
the suite had ever read a refusal's words for this family.

**The bundle example in the configuration reference was a trap.** It wrote
`path = "~/bundles/rust-review"`, and no tilde is expanded for a `[[plugin]]`
path — io-harness resolves `${env:}`, `${file:}` and `${cmd:}` and nothing else —
so an operator copying it named a directory literally called `~` and got a bundle
that is dropped: recorded by the loader, listed by `/plugin`, otherwise silent.
The same file forbids tildes 130 lines earlier.

### Changed

**The README is a front page rather than a manual.** It was 2,847 lines with no
contents list. It now carries the project's logo and screenshot from `assets/`, a
contents list, what you get, install, first run and a table of guides; nineteen
pages under `docs/guide/` carry the depth. The split moved byte ranges rather
than rewriting: 2,695 lines moved, 175 kept, and no paragraph was reworded in
transit.

**New documentation surfaces.** `docs/CONTRACT.md` states what a script may
depend on — the argv surface, the seven exit codes, the seventeen `[app.io-cli]`
keys, the paths io writes, and the limits that hold today. `docs/CAPABILITIES.md`
indexes the guides. `AGENTS.md`, `docs/STYLE.md` and `docs/RELEASE_PROCESS.md`
write down what was previously held by imitation.

**Corrections worth naming individually.** `/contain` was documented under "this
turn" and is filed under `Group::Session`, which also made the printed table show
eleven rows against a bound of ten stated on the same page. The plugin install
was still described as writing `enabled = false` to your configuration before
disclosing — the mechanism 0.30.0 replaced, so a reader was told their file is
written to before they consent, when declining now writes no byte. One module was
said to start a process where two are permitted. A bundle was said to contribute
to four subsystems where it contributes to six. `io skill` was attributed to
0.30.0, where it did not reach the binary until 0.30.1. `/mcp` was said to offer
no disable, which `McpServer` has had since io-harness 0.70.0 and this crate
already writes. Exit `6` was described as one route when the 0.71.0 pin gave it
two. The viewport layout described a two-row composer where `COMPOSER_ROWS` is 1.

**The home is one directory only until you name another.** Under a custom
`IO_CONFIG_HOME`, `io.toml`, the run store and `IO.md` follow the variable while
the skills directory, `.skills-manifest`, the marketplace clones and the session
lock stay in `~/.io-cli`. Nothing said so. Copy both when you move machines.

**Counts nothing checks were deleted rather than corrected.** The README claimed
thirty-nine undrawn event kinds and `src/exec.rs` claimed eleven of fifty; there
are 51 variants, 36 drawn and 15 undrawn. A number no test reads goes stale.

**The changelog is a set of links again.** Thirty-three version headings had
three link definitions, so thirty rendered as literal text while this file's own
header claimed Keep a Changelog conformance, and `[Unreleased]` compared from a
tag four releases old.

### Added

Gates, so the corrections cannot silently come undone: a command must be
documented under the group the code files it in; the install disclosure must
state that nothing is written before consent, and `Plugins::inspect` must really
be called; a mistyped verb must name its surface's verbs; no guide page may be
orphaned and no relative link may be dead; `docs/CONTRACT.md` must agree with
`exec`'s constants, `clap`'s routing table and `CliSettings`' fields; no shipped
document may carry a contact placeholder; the configuration reference must name
every key the harness accepts; and every changelog heading must have a link
definition.

## [0.30.1] - 2026-08-30

### Fixed

**`io skill` did not exist.** 0.30.0 documented `io skill add|list|remove` in the
README and the CHANGELOG, and shipped a working parse, a working plan, a working
arm in the session — and a binary that answered `error: unrecognized subcommand
'skill'`. The argv door is the `Subcommand` enum in `src/cli.rs`, and nothing had
added `skill` to it; `manage::parse`'s own list of surfaces was missing it too, so
even once clap routed the word the parse refused it.

Every one of 0.30.0's 1,609 tests passed over this, because they all enter through
`manage::parse` and the routing that was missing sits in front of it. It was found
by running the published artifact, which is the last gate and the only one that
sees the product as a user meets it.

`io skill add ./x.md`, `io skill list` and `io skill remove <name>` now work.
`/skills add …` in a session was never affected — that path does not go through
clap.

A test now asks `clap` itself which subcommands it will route and compares that
against the surfaces `manage::parse` accepts, so the two cannot disagree again.

## [0.30.0] - 2026-08-30

The rest of the verb matrix closes: after this release there is nothing io
manages that you have to open a file to change.

### Security

**`io mcp add --url …` could report a URL as permitted that the runtime refuses.**
The preflight carried io-cli's own copy of io-harness's URL normaliser, because
that function was not public. The copy failed open on five shapes —
`https://user@/x`, `https://[]/x`, `https://[::1]:/x`, `https://[/x`, and worst
`https://[::1]evil.com/x`, where the bracket branch took `[::1]` as the host,
dropped `evil.com`, and reported *permitted, `[::1]:443`* for a URL that connects
to `evil.com`. A policy allowing loopback would have looked like it covered it.
io-harness 0.71.0 made `net::target` public and fixed all five; io-cli now calls
it and the copy is deleted. Affects 0.28.0 and 0.29.0. Nothing was ever dialled by
io-cli itself — the report was wrong, not the connection — but a permission check
must only ever fail closed.

**`{:?}` on a configuration no longer prints your keys.** io-harness up to and
including 0.70.0 printed resolved secrets through a derived `Debug` on `Config`,
`File` and `ProviderSpec`; 0.71.0 replaces the derive on ten types with
hand-written redacting ones. io-cli never formatted those types into a message, so
nothing was leaked from here, but every io-cli before this one shipped against a
version that could.

### Added

- **`/skills add <path>` and `/skills remove`**, with `io skill add|list|remove`
  through the same parse. Installing **copies** — the file you named stays yours —
  and records nothing, so it lists as yours. A destination that exists is refused
  rather than overwritten, and a file whose `name:` is already claimed is refused
  even when its filename is free, because two skills answering to one name make
  every turn of the next session fail before its first completion. Until now the
  only thing that had ever written a skill file was `/import`. **The `io skill`
  half of this entry did not ship in 0.30.0**: the parse and the plan were there,
  but `src/cli.rs` named no `skill` subcommand, so the argv door answered
  `unrecognized subcommand 'skill'` until 0.30.1 added it. The session commands
  are as described.
- **`/memory` edits and forgets an instruction note**, by line. Both splice the
  file, so your indent, your `*`, a `\r\n` and a last line with no newline all
  survive. A note changed underneath you is refused rather than overwritten. A
  note that carries a continuation body — which is how `/import` brings another
  tool's whole file across — says on its own row how many lines forgetting it will
  leave behind.
- **A forgotten agent memory can be put back.** `/memory`'s forget has returned a
  restore point since 0.29.0 and printed it in a sentence; nothing could spend it.
  Now a confirmation offers it.
- **`/memory` shows what the run recorded** — evictions, pin refusals and recalls.
  These emit no event of their own, so this page is their only witness, and it had
  always been asked for the empty case.
- **`/profile create|remove|clear`.** Creating refuses a name already taken;
  removing takes `[profile.x]` **and every sub-table under it**, so
  `[profile.x.run]` cannot be orphaned; clearing goes back to no profile. The list
  also now sees profiles declared in any scope rather than only the last file —
  switching to one always worked, so the list was wrong rather than narrow.
- **`io mcp enable|disable <id>` and a toggle row on `/mcp`.** A server can be
  switched off without being removed, which is a state io-harness has honoured and
  io-cli could only ever read.
- **A switch-off row on `/plugin`**, beside the removal that was previously the
  only way to stop a bundle loading.
- **`/mcp probe <id>` and `io mcp probe <id>`.** Starts a configured server the way
  a run would — same policy check, so a refused server is refused here and names
  the rule and layer — completes the handshake, lists the tools it offers, and
  shuts it down. Disabled, refused, unreachable, timed out and answering are five
  sentences, not one.

### Changed

- **io-harness `0.70` → `0.71.0`**, which closes six issues this project filed:
  #218 (`Effect::ALL`, `ExecMode::ALL`, `Effect::as_str`), #219 (the named step and
  retry defaults), #220 (`PriceTable::models`), #221 (`net::target` public and
  fixed), #223 (`Plugin::hooks`) and #224 (`Plugins::inspect`). Each closure
  retired a copy io-cli had shipped around the gap, so this release deletes about
  as much as it adds.
- **A marketplace install no longer writes before it discloses.** 0.29.0 had to
  declare a bundle `enabled = false` to make io-harness read it at all;
  `Plugins::inspect` reads and fully validates a directory with nothing on disk
  naming it. A bundle io-harness would refuse now leaves your configuration file
  byte for byte unchanged, and declining leaves nothing behind.
- **Hook rows come from io-harness.** `Plugin::hooks()` replaces io-cli's own
  reader of somebody else's manifest, and it sees two shapes that reader could
  not — an inline `hook = [{…}]` array and a `[[hook]]` header with a trailing
  comment.
- `io plugin add <name>` from a shell now **installs**, printing the full
  disclosure to stderr first. Naming a bundle on that door is the consent; there is
  nobody there to ask, and a `--yes` would be a second reading of the word.

### Fixed

- **The model picker was empty for a whole legal price-table shape.** `/config` on
  a model key scraped the files for a literal `[prices.models]` header, so a table
  written as `[prices.models."gpt-4.1"]` — which io-cli itself names as a supported
  shape — offered no models at all. It now asks `PriceTable` what it prices.
- **Opening the model picker no longer re-runs your credential commands.** The
  fix above re-read the configuration, and reading it resolves `${cmd:}` — which
  for a key fetched from a keychain is a Touch-ID prompt raised to draw a menu.
  Caught before release.
- **A `[[mcp]]` entry's `enabled` written through `/config` was quoted**, producing
  `enabled = "false"`, which io-harness refuses. It failed closed, so nothing was
  lost, but the surface offered a key it could not write.
- Twelve source comments cited line numbers inside io-harness versions this project
  no longer pins — three of them five pins old. All re-verified against 0.71.0, and
  a test now fails on any citation into a version `Cargo.lock` does not name.
- A broken intra-doc link shipped in 0.29.0 (`preflight.rs` linked an item that
  release deleted), which means 0.29.0's own record of a clean documentation gate
  was wrong.

### Migration

An `enabled` key in your configuration cannot be read by an io-cli built against
io-harness 0.69.0, **and the two halves fail in opposite directions**: in a
`[[plugin]]` entry that binary refuses the whole file, loudly; in an `[[mcp]]`
entry it *ignores the key and starts the server anyway*, silently. io-cli says
which, at the moment it writes either.

A plugin manifest may no longer carry a `${env:}`, `${file:}` or `${cmd:}`
substitution, in any scope — io-harness 0.71.0 refuses all three, where only
`${cmd:}` was refused before. Write the value out literally. A bundle is a third
party's directory, and there is deliberately no opt-out.

## [0.29.0] - 2026-08-29

A plugin can come from somewhere other than a directory you already had — and you
see what it is allowed to do before it is allowed to do it.

**Marketplaces.** `/plugin marketplace add zeroonething/ultraship` resolves a
name to a git repository, clones it shallow into `~/.io-cli/marketplaces`, and
lists what it holds. A bundle inside one is any directory carrying a
`plugin.toml`, so there is no index file to write and none to disagree with the
directories it describes. `list` and `remove` are there too, and removing a
marketplace takes the clone and nothing else: a bundle you declared out of it
stays declared, and you are told beforehand which ones will stop loading.

**Install by name.** `/plugin add ultraship`, or `ultraship@ultraship` where two
marketplaces carry that name, and `install` works as the same verb. `/plugin add`
still takes a path — a word is a path if it resolves to a directory carrying a
manifest, and a name otherwise, so it cannot mean different things in different
working directories. **A bare name two marketplaces carry is refused**, naming
both qualified spellings: taking the first match installs code you did not
choose. `/plugin search` reads names and descriptions across every marketplace
you have added.

**What a bundle may do is shown before it is switched on.** A bundle contributes
skills, prompt templates, agents, MCP servers, hooks and policy layers to four
subsystems at once, and until now every one of them came from a directory you had
read yourself. The install writes the entry `enabled = false` first, so io-harness
reads, parses, validates and trust-checks the bundle for real and hands it back
contributing nothing — then shows you what it parsed, in the namespaced names you
will actually see. Saying yes flips one key. Saying no leaves the bundle
declared, off, and listed. A bundle io-harness would refuse is refused at that
point, in its own words, before you are asked anything.

Hooks are the exception and are disclosed anyway. io-harness publishes no
accessor for them, so the disclosure reads the `[[hook]]` tables out of the
manifest and names each one's event and command. Consenting on the bare word
"hooks" is consenting to programs nobody named. Filed upstream as io-harness#223.

**`/plugin` now shows a bundle you switched off.** io-harness 0.70.0 splits what
a configuration declared into three sets rather than two, and this interface was
reading two of them — so a bundle declared `enabled = false` appeared nowhere at
all, and a configuration whose bundles were all switched off was reported as
declaring none. It is listed under its own mark, with what switching it back on
would bring.

**Git is asked about rather than refused, if you run the recommended posture.**
Not io-cli's fix — io-harness 0.70.0 closed the issue io-cli 0.25.0 filed against
it. Through 0.69.0 the git spawn accepted only an outright allow, so `ask before
writes` behaved exactly as `deny` did and all seven git tools were refused with
nobody consulted. You now get the question. `/commit allow` and the refusal
explanation are still there and still needed, but only for `read only`, where
`exec` is a deny and there is nothing to answer — and they are offered only where
that deny came from the posture's own default rather than from a rule, because a
rule cannot be widened by a later layer. **The live gate that asserted the old
behaviour is what caught this**, on the first real run after the pin.

**A run that failed its verification exits 6 again.** io-harness 0.70.0 closed
this project's own issue #212 and gave that run its own outcome, which — because
the enum is `#[non_exhaustive]` — quietly moved it out of the set `io exec` maps
to a ceiling and out of the set the gate retry acts on. Both are named again, so
a failing gate is retried and reported as what it is.

### Fixed

- **`/plugins <verb>` and `/servers <verb>` reach the parse that serves them.**
  The plural was accepted by the router and refused one module later, so
  `/plugins install x` came back "`plugins` is not a surface io manages" while
  bare `/plugins` opened the panel. Folded in `manage::parse`, which is the one
  door both the slash form and `io plugins …` go through.
- **A bundle whose path holds a `"` or a `\` can be read back.** The writer
  escaped both and the two readers decoded neither, so such a bundle could be
  declared and then not located again — which meant the disclosure read a
  different directory's manifest, and the entry could not be removed from the
  surface that declared it.
- **`/mcp`, `io mcp list` and `io mcp get` say when a server is switched off.**
  io-harness 0.70.0 honours `enabled` on an `[[mcp]]` entry before anything is
  spawned or dialled; io-cli was reading none of it, so a server that could never
  start was listed exactly like a live one and every turn quietly ran without its
  tools. Writing the key is still not offered — that verb is next.
- Text out of a marketplace manifest is filtered of control characters and
  bounded before it is drawn, the way output from `git` already was. A
  description holding a raw newline could otherwise forge lines in `/plugin
  search` — the surface used to decide whose code to install.
- Where two bundles inside one marketplace share a name, each is offered by its
  own directory. The refusal used to print one spelling twice and neither
  resolved, so that bundle could not be installed by name at all.

### Dependencies

- io-harness 0.69 → **0.70.0**. `McpServer` gains a required public `enabled`
  field; `RunOutcome` gains `VerificationFailed`. One transitive crate added,
  `quick-xml`, through the `documents` feature. The direct dependency set is
  unchanged at ten names, and this release adds no HTTP client and no second
  network path: a marketplace is fetched by running `git`.

### Upgrading

- **A `[[plugin]]` entry carrying `enabled` cannot be read by an io-cli built
  against io-harness 0.69.0**, which refuses the whole file rather than ignoring
  the key. io-cli says so at the moment it writes one. If you downgrade, remove
  the `enabled` keys first.

## [0.28.0] - 2026-08-29

Every list this interface can prune, it can now grow; and every setting whose
values are knowable is chosen rather than typed.

**All thirty-seven settings stop being free text.** Until now, choosing a row in
`/config` prefilled the composer with the key and left the value to be guessed —
so an operator setting `policy.defaults.write` was being asked to type a value
out of a set the pinned dependency has made public. A boolean is now toggled, a
closed enum cycled, a number chosen from a one-two-five ladder, a model taken
from `[prices.models]` already in the file, and a file taken from the workspace.
What is still typed is only what no menu can hold — a substring, a rubric, a URL,
a command — and each of those states the shape it wants and shows a worked
example before it asks.

**Horizontal arrows cycle a value where it stands, and it cannot be the
spacebar.** Every picker in this product consumes printable characters as a
fuzzy filter, so Space would toggle a setting in the middle of a two-word query.
The arrows were already free.

**Unsetting a key removes it, rather than writing a default's text into your
file.** This needed a new write primitive: `Edit::remove` takes a whole
`[section]` or `[[array]]` entry and errors on a key path, and nothing in the
codebase deleted a single `key = value` line. Writing the default's text instead
would have attributed a crate default to a file you never wrote it in — a lie a
reader cannot detect, and the exact one `/config`'s origin column exists to
prevent. After an unset, the origin column says `default` and names no file.

**A write goes into the file that already decides the key**, stated in the
confirmation, rather than asking every time or silently choosing your own file.
Answering "the user scope" every time would shadow a committed project setting
with a personal one, which is the change you are least able to see afterwards.

**`/provider`, `/mcp` and `/plugin` each gained the verb they never had.**
`servers::add` had been written and tested seven times over while reachable from
no keystroke at all; `pluginview::add` had no caller whatsoever. `/provider` also
gains the edit it has never had — until now a rotated API key meant opening a
file.

**Adding an MCP server says whether it will be allowed to start.** A stdio server
starts under an `Act::Exec` check on its binary and an HTTP one under an
`Act::Net` check on its host, so a server the policy refuses dies before its
process exists. io now reports that at the moment of adding and names the rule
and the layer that decided it, instead of leaving it to be found on the next run.
The report is a disclosure and not a veto: the entry is written either way and
the command exits zero, because configuring a server before writing the rule that
lets it run is an ordinary order to do things in.

**Note that HTTP servers are refused by default.** Every permission posture sets
`net` to deny, so `io mcp add --url …` will report a refusal until a rule names
the host. That is the boundary working, and the report says how to open it.

**Every verb has an argument form, and both forms share one parse.**

```
io mcp add semlith -- semlith --store /path/to/.semlith mcp
io mcp add --transport http linear-server https://mcp.linear.app/mcp
io plugin add ./bundles/rust-review
io config set policy.defaults.write ask
io config unset app.io-cli.plain
io config list
```

`io mcp`, `io plugin` and `io config` open no session, start no run and touch no
store, so they work in CI. `io config list` prints the origin column, because a
value without its deciding file is half an answer and the headless surface must
not tell a weaker truth than the interactive one. A line written for another
harness — `--transport http` before the id, with the URL as a positional — is
accepted through the same parse rather than a second branch.

**No command was added.** Everything here is a verb inside a command that already
existed.

## [0.27.0] - 2026-08-28

The store stops being something you cannot see, undo stops being
all-or-nothing, and the work stops ending when the terminal closes.

**`/store` says what the run store is holding, and three verbs change it.**
`~/.io-cli/runs.db` has held every session, run, step, event, provider call,
snapshot and restore point since 0.15.0, with no retention policy, no rotation
and no way to look at it. The page reports the file's own page arithmetic —
what it costs on disk, what is already free inside it, and what each session in
it holds. `/store rm <id>` removes one session, `/store sweep <date>` removes
every session created before a timestamp, and `/store compact` returns the free
pages to the filesystem. Each descends into a confirmation whose first row is
"leave it", which is the row the cursor starts on.

**A deletion does not shrink the file, and io says so.** SQLite frees pages into
the database rather than out of it, so a removal moves bytes into the free space
*inside* the file and the file on disk stays the size it was. A `VACUUM` is the
only reclamation available, because every store this product has created was made
without `auto_vacuum` — so `/store compact` is a thing you ask for, it needs
roughly the file's own size in free disk space while it runs, and it reports the
bytes the file actually shrank by rather than an inference from the freelist.

A sweep refuses a session that still holds a resumable run, and names it. It asks
you to agree to the rule rather than to a count, because io-harness exposes no
reader for the column the sweep filters on and the nearest substitute would
under-state what is about to go — filed as io-harness#216. The figures are
reported the moment it finishes, refusals included.

**`/undo` is the size of the mistake.** `/undo <path>` puts one file back,
`/undo step <n>` reverse-applies one step's diff, and a bare `/undo` is the whole
turn — the same thing the rewind chord has always done, now reaching the same
implementation. One file has four possible answers and they read as four
different sentences: it came back, it was removed because the run created it, or
nothing changed — for either of two different reasons.

Undoing a step is order-sensitive: a later step still standing on the same lines
makes the revert stale, io-harness leaves the file alone rather than
fuzzy-matching it, and io says why. A restore does not know about an edit you
made after the turn, and the confirmation says that before you agree to it.

**`/export` writes the conversation as markdown, and `/export trace` writes one
run's canonical trace.** Both go into the workspace under the session's own path
policy, and an existing file is refused rather than overwritten. The trace is
written exactly as io-harness produced it — not parsed, not reserialised, not
pretty-printed — because being canonical is the whole of what it is for.

**Undoing a turn now announces itself.** `EventKind::Rewound` and
`EventKind::Reverted` are emitted only by the observed forms of io-harness's
rewind calls, and this product had called the plain ones since 0.4.0 — so neither
event had ever fired, and the code drawing one of them had been unreachable since
the day it was written.

**One event that reached nobody now has a line.** A read started before the model
had finished asking, and thrown away — work that was paid for and not used. It
draws only when something was discarded, because a line in every transcript
saying nothing went wrong is a line nobody reads. The other eight silences in
this interface were reviewed in the same pass and keep their routes, which are
better arguments than drawing them would be.

**`/contain` moves from *this turn* to *the session*.** It is a posture that
changes how every later turn is driven rather than something that acts on the
turn just finished, and moving it is what makes room for `/undo` without widening
the ten-command bound. `Inspect` reaches ten with `/store` and `/export`.

No configuration key is added, nothing is read at startup that was not read
before, and no model can reach any of the store operations.


## [0.26.0] - 2026-08-28

A turn asks for the model, the vendor and the amount of thinking the work
actually needs. Three of the four things in this release were already sitting in
io-harness, reachable and unasked for; the fourth was a list this interface had
been drawing for five releases without ever running it.

**`/effort low`, `/effort medium`, `/effort high`, `/effort off`.** How much
reasoning a turn buys, said once and held: the level applies to that turn and to
every turn after it until you change it, it shows on the status line as `effort
high`, and a bare `/effort` reports what is in force and changes nothing.
io-harness owns the translation to each vendor's spelling — `reasoning_effort` on
the OpenAI wire, `reasoning: { effort }` on OpenRouter, a converted thinking
budget on Anthropic — and io-cli names a level and nothing else. `off` is not a
fourth level below `low`: it goes back to sending no reasoning field at all, which
is what every release before this one sent, and the line it commits says that
rather than naming a level. The level is the session's and is written to no file.

**`/profile` moves from *this turn* to *the session*, and it is a correction
rather than a reshuffle** — the third time after 0.19.0's `/mcp` and `/provider`
and 0.22.0's `/image` and `/copy`. *this turn* means a command acting on the work
the turn just finished; `/profile` changes which configuration overlay every later
turn is built from, which is a property of the session. It is where `/help` lists
it now, which is the only visible trace of the move. That it also made room for
`/effort` is the order the bound was meant to force: the group stood at ten of
ten, and the answer written down in advance was to re-file what is in the wrong
group rather than widen the bound.

**`[app.io-cli.routing]` changes the model mid-run.** `escalate_after` moves up to
a stronger model after that many consecutive failed verification-gate attempts;
`downshift_under` asks a cheaper one while the run has written fewer than that
many bytes to disk. Escalation happens once and does not come back down, and it
wins over downshifting where both apply — io-harness's rules, and it evaluates
them, so io-cli holds no counter of its own.

**A rule that could only misfire is refused by name.** Half a rule, `failures = 0`
— which io-harness reads as "escalate before anything has failed", pinning every
run to the escalation model from its first request — `bytes = 0`, which can never
be true, and a model written as the empty string are each refused with the key
named, and the run goes unrouted rather than obeying them. The two keys inside
each rule are optional for the same reason: making them required meant a half
rule failed to deserialize `[app.io-cli]` entirely, which silently took the
theme, the keys, the ceilings, the capabilities and the verification gate with
it.

**Routing does not reach a contained turn, and this release says so rather than
letting it be discovered.** io-harness applies routing in its flat workspace loop
only; a turn run under `[app.io-cli.containment]` takes each agent's model from
that agent's own roster entry and never consults the rules. For an operator with
containment configured the section parses, is listed by `/config`, reaches the
contract and never fires. A session with both is told at start, on `/config`, and
when `/contain on` is typed, and a session without containment is told nothing, because a caveat attached to a working
feature is how somebody learns to stop reading the notices. `io exec` uses the
flat loop, so routing works there, and so does a turn taken with `/contain off`.
There is no `require_primary` key: io-harness has the field, it gates on
`Provider::reachable`, which is defaulted to yes and which no provider in
io-harness 0.69 overrides, so a key for it would be permanently inert.

**The `[[provider]]` chain is finally what runs, and this changes behaviour for a
file that already has more than one entry.** `/provider` has drawn the whole chain
since 0.21.0 while the product only ever asked its head. From this release the
next entry answers when the first fails in a way another vendor might survive — a
transport error, a timeout, a rate limit, a 5xx — with the fall-through committed
to the scrollback and the status line's provider field moving to whoever actually
answered. **A failure that will fail identically everywhere does not fall
through**, a bad API key above all, so a wrong credential on the primary cannot
start spending at the secondary. The predicate is io-harness's own. `--provider`
on `io exec` replaces the whole chain rather than heading it, because a run scoped
to one vendor on the command line must not spend at another.

**A question that is only a question says so.** io-harness has answered such a
turn in one completion — no steps, no tools, no gate — for longer than this
interface has existed, and io-cli drew every line it draws about a turn from
events that turn does not emit, so what reached you was silence. It now commits
`answered without opening a run`. `conversational = false` in `[app.io-cli]` turns
the classification off so every prompt opens a full run; absent leaves the
behaviour exactly as it was.

## [0.25.0] - 2026-08-28

The work a turn does ends as something somebody can review. io-harness has
offered seven git built-ins — `git_status`, `git_diff`, `git_log`, `git_add`,
`git_commit`, `git_branch`, `git_worktree` — on every workspace run since long
before this interface existed, with no feature gate and a fixed argv that can
reach no other subcommand. io-cli had never surfaced one of them.

The branch the working tree is on is on the status line, read from `.git/HEAD`
rather than from a subprocess, and it follows the agent when it switches. A
commit the agent makes is committed to the scrollback with the message it wrote
and the branch it landed on. `/commit` hands the turn's work to the agent to
describe and stage, under the identity `[run.commit_identity]` names — the
identity 0.14.0 made reach a turn and which nothing had yet read.

**And it repairs what made all of that unreachable.** Under `ask-writes` — the
posture the wizard recommends — io-harness's git spawn treats an asking `exec`
policy as a hard refusal rather than raising an approval, so every one of the
seven tools was refused before it ran and the operator was never asked. io-cli
now names that refusal, before a turn is spent and whenever one arrives, and
offers the single rule that lifts it: `exec` allowed for `git`, one binary,
for the session.

A sub-agent can also work in a checkout of its own. `worktree = true` on an
`[[agent]]` entry roots that child under `.worktrees/` on its own branch before
its first step, and the fleet says which children have one.

## [0.24.0] - 2026-08-28

A turn proves its work instead of asserting it. io-harness has carried a
verification pillar since long before this interface existed — a contract holds a
criterion, the run executes it after the agent stops, and the run comes back as
`Success` rather than `Finished` when it passed. io-cli has never once supplied
one, which is why every clean run this product has ever reported said `finished`
and meant "the agent stopped", not "the work holds up".

**You say what done means for this repository, in the repository's own
language.** A command that must exit zero, a file that must exist and say
something, or a rubric a second model answers. The command is proposed from what
the repository actually is: io-harness reads its marker files and names its own
test command, so a Rust checkout offers `cargo test`, a `package.json` offers
whatever its lockfile implies, and a repository with no marker offers nothing
rather than guessing. io-cli holds no list of test commands and no list of marker
files.

**The verdict is on screen while it happens.** The status line carries the gate's
standing and, once there has been more than one, which attempt this is. After a
gated turn the scrollback takes the phase that ran, what it answered and what the
command printed — a gate that failed because the tests went red reads differently
from one that failed because the policy would not let the program run at all.

**A failing gate sends the agent back to work with the failure in hand**, up to a
retry budget you set and defaulting to one. The harness's own loop does not do
this: it records the failure and takes another step without telling the model
anything, so a run can spend its whole budget failing the same gate for the same
reason. What io-cli does instead is drive a follow-up turn carrying the criterion
and the output it produced.

**`io exec` gains exit `6`** — the agent finished and the work does not hold up.
It is distinct from `5`, the agent stopping without finishing, and from `3`, the
ceiling a failing gate used to hide behind. No existing code changed meaning.

Also: `/mcp` can edit a server at last, closing a limitation this product has
carried and stated since 0.21.0.

## [0.23.0] - 2026-08-28

A run that paused is answered rather than abandoned. Everything needed to do it
has been public in io-harness since 0.10.0; what this interface did with a paused
run until now was print its id and walk away.

**`/resume` reads what each session's last run stopped on and puts it on the row
you choose from.** A word rather than a symbol, so it survives `NO_COLOR`,
`--plain` and the ASCII glyph set: `asks` for a question nobody answered, `plan`
for an approach nobody decided, `tool` for a call whose outcome nobody recorded,
`died` for a process that went away leaving committed work behind, `ended` for a
turn the operator stopped themselves. A session with nothing outstanding carries
no mark, so the list is ragged by construction — which is what buys a mark that
says *which* state it is to somebody choosing on it with no legend on screen.
There is no new store read to pay for it: the walk that builds the list already
knows each session's newest run, because that run is what put the session where
it is in the list.

**Choosing a marked session opens the same overlay the run would have opened
while it was live, and the answer carries that run on from the step it stopped
at.** Not a new run with the answer pasted into its goal: the observation ledger,
the token budget and the elapsed clock are the original run's. A plan is
approved, sent back with a correction, or cancelled — `cancel` ends the run as
`PlanRejected`, which is what "do not do this at all" means, rather than spending
the rest of the budget on the approach just refused. An interrupted call is
retried, abandoned, or asserted to have landed, and the operator's account of
what it returned is filed against the step the call was *made* on, so the resumed
run reads a transcript in which the tool answered where it was asked. A run whose
process merely died carries on from `last_step + 1`. `Esc` leaves any of them
parked exactly as it was found.

**A turn the operator interrupted cannot be continued, and this release says so
instead of pretending.** `Ctrl+C` returns `Flow::Cancel`, io-harness records the
outcome `cancelled`, `finish_run` maps that to a *completed* status, and every
resume entry point short-circuits on a completed run and hands back the original
outcome having driven nothing. So the single most common way a turn stops is the
one way it cannot be resumed. `/resume` reports it as ended by you and offers
`/fork` from the preceding turn, which is the honest neighbouring answer; `io
resume` refuses it in the same sentence, before a provider is built. The
published io-harness documentation disagrees — `Steer::interrupt` says such a
turn "stays resumable" — and is contradicted by the run loop in the same crate.
Reported upstream; not worked around here, because a workaround for a
short-circuit that returns success would be this interface inventing an outcome.

**`io resume` is the headless door.** `--list` enumerates the parked runs — a
row per run with what it is waiting on, the id that addresses that pause and the
step it stopped at, as NDJSON under `--json` — and one is carried on by id with
the decision on the command line: `--answer`, `--plan` with `--correction`,
`--recovery` with `--account`, or nothing at all for a run whose process died. It
takes the same `--json`, `--policy` and `--provider` an `io exec` takes, meaning
the same three things. There is deliberately no `--sandbox`: a resumed run
already started under a boundary, and a flag that widened it halfway through
would be a widening nobody asked for at the point nobody is watching. clap cannot
see which pause a run is on, so a flag for the wrong one is settled against the
store and **refused** rather than ignored — `--plan approve` at a run holding a
question is somebody acting on the wrong thing.

**Exit `4` now names the pause and the invocation that answers it.** It named the
run id, which addresses none of the four pauses; the closing line now names the
`question_id`, `plan_id` or `attempt_id` and prints the `io resume` that decides
it. **No exit code was renumbered and none was added** — the six have meant what
they mean since 0.5.0, and `4` was given to a pause nothing could yet answer for
exactly this release. An approval remains the one pause `io resume` cannot take:
it is answered by the person the run asked, at the terminal it asked from, and
io-harness publishes no entry point that takes one.

**A bare run needs `--goal`, and is refused without it.** `runs.goal` has no
public reader, so a contract cannot be rebuilt from a run alone. For a run that
served a session turn the operator's own words are recoverable from the turn; for
a run `io exec` or any other non-session caller started they are not, and
resuming against an empty goal would spend a budget on a task nobody set. A
`--goal` supplied for a run that has one wins, because that is an operator
re-aiming their own run.

**One `io` at a time on one conversation.** One store serves the whole machine,
so two terminals in one repository is the ordinary case — and they are not in
conflict, because starting `io` creates a new session every time and each gets
its own. What two processes can genuinely contend over is a single *session*,
and that happens in one place: `/resume`, when one enters a session another
already has open. Nothing guarded it, so both advanced the same head and the
loser of that race had paid for a turn that was then orphaned off the head path
— still in the store, correctly parented, never shown again by a history that
walks back from the head.

Each session is now held under an advisory whole-file lock through
`std::fs::File::try_lock`, which is stable on this crate's MSRV and needs no
dependency: `flock` on unix, `LockFileEx` on Windows, released by the kernel on
exit, on panic and on `kill -9`. There is no stale lock to reap and no pid file
to sweep, and a lock that cannot be taken for an ordinary filesystem reason
warns rather than refusing to start — trading the guard for "io will not run on
this machine" would be the worse failure. The lock a session takes when it opens
contests nothing, since that session did not exist a moment earlier; what it
does is write down who owns it, so the next process to reach for it can be told.
`/resume` into a session another `io` holds is refused. Two `io` on two
different sessions are not, `io exec` and `io resume` take no lock at all, and
for everything the lock does not see the guard of last resort is io-harness's
own compare-and-swap on the head.

**What the refusal can say about the holder is what io-cli itself wrote beside
the lock**: the pid, the workspace root, the `io` version and the instant that
process started. It is not the operating system's account of that process, and
the release says so plainly rather than implying more. `/proc` is one platform's
answer to a three-platform question, `ps` and `tasklist` are two more, and the
dependency that would abstract them is one this crate's own gate forbids — so a
pid shown here is a number io wrote down. The record is a second file beside the
lock rather than the lock's contents, because a Windows byte-range lock is
mandatory and the refused process could not read the file it is being refused on.
The twelve-hour lease exists only for a home on a network filesystem, where
an advisory lock is not this program's business and the record's own timestamp is
the only evidence there is.

**Two defects are fixed.** `/undo` wrote the session head unconditionally, so
undoing in one terminal silently clobbered a head another had just advanced,
orphaning a turn that had been asked for, answered and paid for; it is a
compare-and-swap now — `set_session_head_if` with the head the undo believed it
was replacing — and a lost race is **refused and reported** rather than retried
or forced, because which turn survives is the operator's call and the losing turn
is only recoverable while it is still on somebody's head. And `failure::advice`
had no arm for `Error::Conflict`, whose own text reads `run {run_id} is held by
another owner until {expires_at}` — on the head shape that calls a session id a
run and ends on the word "until", because a head conflict populates no expiry.
That variant is now matched on its value before any text is read, and the
harness's line is dropped rather than kept underneath it: the module's rule is
that terse text is worth putting in front of an operator, and it was never a rule
about text that is wrong.

**An interactive turn that ends parked says so.** io-harness returns
`AwaitingAnswer`, `AwaitingPlan` and `AwaitingRecovery` as an ordinary `Ok`, so
through 0.22.0 the arm matched and dropped them, and the operator got their
prompt back with no sign that a run was sitting in the store waiting for a
sentence from them. Every other way a turn can end had a line.

**No command was added and none changed group.** `/resume` was extended rather
than joined, so the palette still holds thirty commands in the same four groups,
and `/resume`'s own description is the one line in that table this release
rewrites.

**Three limitations are named rather than left to be found.** An interrupted call
in a *contained* run has no resume entry point — io-harness 0.69 publishes
`resume_tree_with_answer` and `resume_tree_with_plan_decision` and no recovery
twin, and driving a tree root through the flat one would silently drop the
containment it was running under, so that case is refused. A conversational turn
that paused and was resumed reads back afterwards as a run, because
`Store::turn_kind` and `Store::set_turn_kind` are both `pub(crate)`. And
`io resume --list` classifies every run in the store rather than querying for the
parked ones, which is linear in the store's whole history: the store-side query
that would fix it is not published.

## [0.22.0] - 2026-08-27

What the work cost and whether it worked — two questions this interface has been
carrying the answers to since io-harness 0.18.0 and has never asked.

**`/cost` commits what has been spent: this run, this session, by model, by day.**
Tokens and money, four sections, every figure a row already in `runs.db`. The
harness has recorded one row per provider call for four releases — the model, the
prompt, completion, cache-read, cache-write and reasoning split, the latency and
the time to first token — and io-cli reported token counts and treated the money
question as unanswerable. It was answerable then.

**Nothing on that page is estimated, and three rules follow from it.** Cache reads
and cache writes are shown as the breakdown of the prompt they already are, never
added on top: they live inside `prompt_tokens`, and adding them would over-report
every cached turn, which is most of them. A call the provider reported no usage
for is **unknown**, never free — io-harness stores it as SQL `NULL` for exactly
that reason, and summing it as zero would report a turn that cost something as a
turn that cost nothing. And a model with no rate in the table makes the total a
**floor** rather than a total, which the page says in that word, with the number
of calls it applies to, and on the row itself in the grouped sections — a reader
scanning for the largest figure has to see which of them is incomplete without
reading past the list. io-harness's own pricing documentation calls a renderer
that hides that count "lying by omission", and it is right.

**`/stats` commits how the runs have gone**: runs by outcome, runs by day, the
first-try counts, gate failures by phase, recovery, the slowest calls and the time
to first token over the last 200 runs, and what the store holds on disk. **First-try
is io-harness's own definition** — finished *and* successful *and* carrying no
gate failure — and it arrives as three counts rather than as a rate, because the
denominator is a choice: first-try over every run counts runs that are still
going. Where a share is drawn, the row names what it is a share *of*, instead of
printing a percentage whose meaning two readers would reconstruct differently. The
sandbox gate's phases and the review, command and contains gates are two
vocabularies that do not overlap, so they stay two lists; merging them would
produce a chart whose categories mean two things. Nothing here compacts anything —
`Store::compact` is a full `VACUUM` needing free disk roughly equal to the file,
and a page that reported free space and then reclaimed it would be a page that
surprised somebody mid-session.

**They are two commands rather than one because they are two questions.** Every
agent that has both keeps them apart, and a single screen carrying thirteen
sections is one nobody reads to the end. `/usage` is an alias for `/cost` and
earns no row in any table — it resolved to `/status` until now, which was the
closest thing there was to an answer and was not one.

**io-cli compiles no prices in, and this is the part worth reading twice.** A rate
baked into a binary is a promise the binary cannot keep: providers move prices
without announcing it, a release cadence is not a pricing cadence, and an operator
reading a confident wrong number is worse off than one reading no number at all.
So the table ships **empty**, and an install that has connected nothing sees
tokens and no currency. It is filled from the model catalogue the operator's own
provider serves — the same fetch `io setup` has made since 0.1.0 to offer a list
of models, whose `price`, `price_tiers` and `price_source` it read off the same
row and threw away. No JSON is parsed here and no dependency is added: io-harness
did the parsing and the unit conversion. The rates are written into `[prices]`,
which is io-harness's own section — `as_of` for the date, one line per model under
`[prices.models]`, a row per line so that correcting one by hand is a one-line
change you can find again.

**Refreshing shows every rate that would move before it writes anything**, with
what each was and what it would become, and declining writes nothing — the shape
`/import` established in 0.21.0. That is not courtesy. io-cli records what a rate
*is* and never where it came from, so it cannot tell a correction you made by hand
from a value an older catalogue served; it does not guess, it shows you and lets
you refuse the lot. **A refetch that comes back empty, or far shorter than the
table it would replace, is refused and the old table kept** — the one failure in
this area that loses money quietly, because a truncated response replacing a full
table with a handful of rows would turn most of an operator's spending into
"unpriced" and shrink their reported bill with nothing failing anywhere. A first
fill has nothing to compare against and is never refused. Rows for models the
catalogue no longer serves are left alone rather than pruned: io-harness prices a
call by the model name on it, so an old row is what prices an old run correctly.

**Whose price it is gets said, on every surface that draws money.** OpenAI and
Anthropic publish no prices on any endpoint — their model endpoints
carry capabilities and limits and no cost field, and their cost APIs report what
was *spent* rather than what a token *costs*. So for those two the rates
necessarily come from the reference catalogue rather than from the vendor, and the
page names which and on what date. io-harness models the distinction already, and
io-cli carries it through instead of flattening it into the connected provider's
name. On OpenRouter the two coincide, because the reference catalogue is
OpenRouter's own.

**The status line carries a cost field**, right of the token count it is derived
from, so the two read in the order they are computed in. It is **absent — not
`$0`** — where there is no price table, where the table prices none of the models
this run called, or where nothing has run. Those are three different things and
none of them means free.

**Three new keys and one new section for a file you already have.**
`[app.io-cli.prices]` carries `source_url`, which names a catalogue to read
instead of io-harness's default and is the only way an operator on a self-hosted
or `compatible` endpoint gets prices at all; and `source` and `models`, which
record what the last read was and how many models it priced and are written by a
fetch rather than by hand. They are in io-cli's own section rather than beside the
rates because `[prices]` is `deny_unknown_fields` and carries exactly `as_of` and
`models` — a key of io-cli's own put next to them would not be ignored, it would
make the operator's whole configuration file unreadable. `/config` gains
`prices.as_of` and `app.io-cli.prices.source_url` as rows; `[prices.models]` is
deliberately not one, because it is a list rather than a setting.

**`/image`, `/copy` and `/copy diff` move from inspect to this turn**, and it is a
correction rather than a way of making room — the same sentence 0.19.0 wrote about
`/mcp` and `/provider`, and it is worth being able to say twice. All three act on
the turn that just finished; none of them asks the store a question, which is what
**inspect** means. They were filed there because they *show* something, and
showing is not the same as inspecting. `/cost` and `/stats` are what made it worth
correcting: no group may hold more than ten, **inspect** stood at nine, and the
choice was between re-filing three commands that were in the wrong group and
filing two more that would have been. Weakening the bound would have given up the
thing it protects — the grouped menu not turning back into the flat list of thirty
it replaced.

**Two status-line priority inversions, both corrected here because this release
adds another counter to that row.** The footer dropped its whole right-hand group
— the policy layer, the containment mode, the planning phase — to keep every
counter, so a narrow terminal gave up the standing modes that explain why nothing
is happening in order to keep a number about what already happened; the counters
yield now. And `fields` pushed `planning` right of the counters, so the narrow
line dropped it before the token count — the exact inversion of the rule written
five lines above it in the same file, standing for four releases.

**Four defects carried over from 0.21.0 are fixed.** `edit::split_path` is
quote-aware: a dot inside a quoted key is not a separator, TOML spells a bare key
out of `A-Za-z0-9_-` and nothing else, and a model id or MCP server name carrying
a dot was cut in half — the read half answered nothing and the write half appended
a second copy of a table that was already there, surfacing as an unexplainable
"the edit would have produced a file that does not parse". Removing or moving a
TOML entry no longer carries the *next* section's comment block away with it.
`move_entry` into the last position of a file with no trailing newline no longer
concatenates onto the final value. And a loose `CONVENTIONS.md` or `CLAUDE.md`
inside a `skills/` or `plugins/` directory is imported as a **skill**: it matched
on its basename before, and its whole body was appended into the instructions file
loaded on every turn, forever, instead of a named skill read on demand. Where a
file is decides what it is, ahead of what it is called.

**crossterm moves to 0.29**, taken together with ratatui's `crossterm_0_29`
feature so the two agree on one version of the terminal backend. The dependabot
ignore that held it back is gone.

## [0.21.0] - 2026-08-27

What you already told another agent tool, brought across once — and two surfaces
that finally do what they have been described as doing.

**`/import` brings your setup over from the tool you were using before.** io looks
for `~/.claude` and `~/.claude.json`, `~/.codex`, `~/.gemini`, and a `.cursorrules`
or `CONVENTIONS.md` in the repository, and offers four things out of whatever it
finds: the standing instructions you wrote, which are appended whole to the memory
file for the scope you pick rather than shredded into a bullet per line; the MCP
servers you configured, translated into io-harness's `[[mcp]]` spelling; the skills
you collected, written into your skills directory; and the model you settled on. It
is offered once on a first run — io records that it asked, so it never asks twice —
and one key declines and carries straight on into the session. `/import` opens the
same thing whenever you want it. **The whole plan is on screen before a single byte
is written**, one row per thing found with where it came from and where it would go,
and you accept item by item. Declining writes nothing, and a cancelled import is not
a partial one. A tool whose files are all empty is a distinct row and says so, which
matters more than it sounds: on a good many machines every Gemini file exists and
every one is zero bytes, and an import of nothing that then reports success is the
failure you cannot see.

**No credential is read or copied, and the code is what enforces that rather than
the intention behind it.** `~/.codex/auth.json` is not in the list of files this
program can open, so no path through it reaches one. A server's environment values
are parsed and discarded without ever being assembled into a string — only the
variable *name* is held — and what gets written is `${env:NAME}`, the name pointing
at itself, which io-harness resolves from your own environment when a run needs it.
`~/.claude.json` is a whole application's state with OAuth material in it and is
read through narrow structs, so every field io does not name is skipped by the
parser instead of being loaded and then politely ignored.

**An allowlist is read, shown, and deliberately not translated.** Codex spells a
permission as `prefix_rule(pattern=["bun","install"], …)` and Claude as
`Bash(cargo yank *)`; both match a command line, and io-harness's `Act::Exec`
matches a binary name and nothing else. The nearest faithful import of `bun install`
is therefore a blanket allow on `bun` — a wider permission than you ever granted,
written by a tool you were trusting to be careful. io says what it found and says it
cannot express it, and produces no rule, no `[policy]` table and no policy layer. A
boundary half imported is worse than one left alone. A model id is carried rather
than written for a smaller version of the same reason: `[[provider]]` needs a vendor
and `gpt-5` does not name one, so the entry is built once you have chosen.

**Skills are counted before any are written.** A name your directory already answers
to is refused on its own row and the rest of the import goes through; a set that
would cross io-harness's 64-skill ceiling refuses **every** skill instead, because
the harness rejects a whole directory rather than the excess and the alternative is
a session in which every turn dies at run start with nothing visible to blame.

**A capability bundle's skills are on a surface at last.** 0.20.0 let a bundle
contribute skills and every one of them reached the model, under no row in `/skills`
and no entry in the `/` palette — a list that disagreed with the catalogue the turn
was handed. Both now carry them, spelled `<bundle>__<name>`, which is io-harness's
own namespacing and the string a refusal or a tool call will name, with the bundle
shown as the origin. Enabling or disabling one is **refused**: turning a skill off
is moving its file into a `disabled/` directory, and for a bundle skill that means
io-cli creating a directory inside somebody else's bundle and moving their file into
it. Stop the bundle with `/plugin` instead.

**And 0.20.0 shipped a session-killer behind that omission.** `Plugin::skills_dir`
is the manifest's word joined onto the bundle root with no existence check, and the
walk that discovers skills fails the run with `?` before the first completion — so a
bundle declaring a skills directory that is not on disk ended **every turn of that
session**, with nothing anywhere naming the cause. `/skills` and `/plugin` now name
the bundle and say what it costs, one row per bundle, so a second broken bundle does
not hide behind the first and a broken one does not take down the surface that
explains it.

**`/mcp` and `/provider` write.** Both could only list. The writer functions existed
and were tested and were called from nothing, while three places in the code and the
documentation said the two panels "add, edit, disable and remove entries" — a
sentence that has been describing an intention since 0.19.0. They now genuinely add,
edit and remove through the same staged write `/config` uses, read back by io-harness
and rolled back whole on a refusal. `/provider` also arranges the chain: promote,
demote, or add an entry, which is the fallback order io-harness has read since its
0.27.0 and that this interface has drawn an event for without ever being able to
cause one. Reordering moves an entry with its own comments and its own keys rather
than rebuilding the array. *Disable* is gone from that sentence rather than
implemented: `McpServer` has no key for it, and an `enabled = false` invented here
would be accepted by the file and ignored by the harness, so the server would start
anyway.

**Seven defects were fixed in those writers on the way**, and one of them is the
reason `[[mcp]]` is the section to be careful in: it is one of only two io-harness
exempts from `deny_unknown_fields`, so a misspelled key spliced into an entry was
accepted by the parser, reported as written, and ignored by every turn afterwards —
the server running with the setting the operator thought they had changed still at
its default. `/mcp` refuses a key it does not know instead of writing it, and the
round-trip assertions deserialise into `McpServer` rather than looking for a string
in a file. Neither panel takes a row number either: an entry is addressed by finding
it in the file's own bytes, because a row on screen and a position in a file's array
stop agreeing the moment anything sorts or filters, and that failure is silent — it
removes a server nobody named, or bills the next turn to a vendor nobody chose.

**`/import` is the twenty-eighth command**, and it joins the **configure** group
because it writes files, which is what that group means and what **inspect**
promises it never does. It is last in the group because it is the one command there
an operator uses once; the others are returned to for the life of the install.
Configure goes to eight.

**One documentation correction that had nothing to do with any of the above.**
`src/configure.rs` said io-harness substitutes `${env:...}` and `${file:...}` "and
nothing else". There are three: `${cmd:...}` is the third, and it is refused in a
project-scoped file. The README's `skills` row said the same thing and has been
corrected too. `${cmd:}` is still not passed through by the credential redaction —
it is a command line rather than a name — but that is now written down as a choice
instead of resting on a false premise.

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
week. A bundle can also be stopped from that list: the last row under its
contributions removes its `[[plugin]]` entry after a confirmation naming the scope
it will edit. The entry is found by matching the directory across all three scope
files rather than by counting rows on screen, and where no file names it, io says
so and removes nothing. The directory itself is never touched.

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

[Unreleased]: https://github.com/initorigin/io-cli/compare/v0.34.0...HEAD
[0.34.0]: https://github.com/initorigin/io-cli/compare/v0.33.0...v0.34.0
[0.33.0]: https://github.com/initorigin/io-cli/compare/v0.32.0...v0.33.0
[0.32.0]: https://github.com/initorigin/io-cli/compare/v0.31.0...v0.32.0
[0.31.0]: https://github.com/initorigin/io-cli/compare/v0.30.2...v0.31.0
[0.30.2]: https://github.com/initorigin/io-cli/compare/v0.30.1...v0.30.2
[0.30.1]: https://github.com/initorigin/io-cli/compare/v0.30.0...v0.30.1
[0.30.0]: https://github.com/initorigin/io-cli/compare/v0.29.0...v0.30.0
[0.29.0]: https://github.com/initorigin/io-cli/compare/v0.28.0...v0.29.0
[0.28.0]: https://github.com/initorigin/io-cli/compare/v0.27.0...v0.28.0
[0.27.0]: https://github.com/initorigin/io-cli/compare/v0.26.0...v0.27.0
[0.26.0]: https://github.com/initorigin/io-cli/compare/v0.25.0...v0.26.0
[0.25.0]: https://github.com/initorigin/io-cli/compare/v0.24.0...v0.25.0
[0.24.0]: https://github.com/initorigin/io-cli/compare/v0.23.0...v0.24.0
[0.23.0]: https://github.com/initorigin/io-cli/compare/v0.22.0...v0.23.0
[0.22.0]: https://github.com/initorigin/io-cli/compare/v0.21.0...v0.22.0
[0.21.0]: https://github.com/initorigin/io-cli/compare/v0.20.0...v0.21.0
[0.20.0]: https://github.com/initorigin/io-cli/compare/v0.19.0...v0.20.0
[0.19.0]: https://github.com/initorigin/io-cli/compare/v0.18.0...v0.19.0
[0.18.0]: https://github.com/initorigin/io-cli/compare/v0.17.0...v0.18.0
[0.17.0]: https://github.com/initorigin/io-cli/compare/v0.16.0...v0.17.0
[0.16.0]: https://github.com/initorigin/io-cli/compare/v0.15.0...v0.16.0
[0.15.0]: https://github.com/initorigin/io-cli/compare/v0.14.0...v0.15.0
[0.14.0]: https://github.com/initorigin/io-cli/compare/v0.13.1...v0.14.0
[0.13.1]: https://github.com/initorigin/io-cli/compare/v0.13.0...v0.13.1
[0.13.0]: https://github.com/initorigin/io-cli/compare/v0.12.0...v0.13.0
[0.12.0]: https://github.com/initorigin/io-cli/compare/v0.11.0...v0.12.0
[0.11.0]: https://github.com/initorigin/io-cli/compare/v0.10.0...v0.11.0
[0.10.0]: https://github.com/initorigin/io-cli/compare/v0.9.0...v0.10.0
[0.9.0]: https://github.com/initorigin/io-cli/compare/v0.8.0...v0.9.0
[0.8.0]: https://github.com/initorigin/io-cli/compare/v0.7.0...v0.8.0
[0.7.0]: https://github.com/initorigin/io-cli/compare/v0.6.0...v0.7.0
[0.6.0]: https://github.com/initorigin/io-cli/compare/v0.5.0...v0.6.0
[0.5.0]: https://github.com/initorigin/io-cli/compare/v0.4.0...v0.5.0
[0.4.0]: https://github.com/initorigin/io-cli/compare/v0.3.0...v0.4.0
[0.3.0]: https://github.com/initorigin/io-cli/compare/v0.2.0...v0.3.0
[0.2.0]: https://github.com/initorigin/io-cli/compare/v0.1.1...v0.2.0
[0.1.1]: https://github.com/initorigin/io-cli/compare/v0.1.0...v0.1.1
[0.1.0]: https://github.com/initorigin/io-cli/releases/tag/v0.1.0
