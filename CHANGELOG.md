# Changelog

All notable changes to this project are documented here. The format follows
[Keep a Changelog](https://keepachangelog.com/en/1.1.0/) and the project uses
[Semantic Versioning](https://semver.org/spec/v2.0.0.html), staying on `0.x`.

## [Unreleased]

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
- **The model's reasoning, committed as a thought** — the word, how long the step
  had been going, then the text, wrapped and muted. A thought longer than ten
  rows is fitted and the rest goes to `/expand`; io-harness neither stores
  reasoning nor folds it into the next prompt, so that copy is the only one there
  is.
- **Two status-line fields: the provider and the step count.** Both are set from
  the events that carry them and both are cleared when a run is forgotten. They
  are where the two removed rows' facts went.
- **`/clear`** — a new conversation without leaving the binary: a new session id,
  no prior turn sent to the model, and the run-scoped status fields back to zero.
  It clears the screen and nothing else; the conversation it ends is in
  io-harness's store and is still listed by `/resume`. Refused while a turn is
  running.
- **`/exit` is listed.** The parser has accepted it since 0.1.0 and nothing ever
  advertised it, which is the same defect as not having it.

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
- **The viewport is five rows**, not four: the activity line, the live row, two
  rows of composer and the status line. It is still clamped to the terminal, so
  80x24 is a supported size.
- **The command palette shows the whole list.** Opening `/` re-places a taller
  viewport for as long as the palette is open and gives the rows back on close —
  by a choice, by `Esc`, or by the terminal resizing under it. It is done only at
  an empty prompt, where nothing is streaming.
- **`--plain` still commits the provider and the run's numbers.** The two rows
  this release removed moved to a line a plain session does not have, and a fact
  that lives only in a repainting row is a fact taken from exactly the reader who
  cannot follow one.

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
