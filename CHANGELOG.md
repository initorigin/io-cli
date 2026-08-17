# Changelog

All notable changes to this project are documented here. The format follows
[Keep a Changelog](https://keepachangelog.com/en/1.1.0/) and the project uses
[Semantic Versioning](https://semver.org/spec/v2.0.0.html), staying on `0.x`.

## [Unreleased]

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
