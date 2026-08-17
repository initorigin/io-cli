# Changelog

All notable changes to this project are documented here. The format follows
[Keep a Changelog](https://keepachangelog.com/en/1.1.0/) and the project uses
[Semantic Versioning](https://semver.org/spec/v2.0.0.html), staying on `0.x`.

## [Unreleased]

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
