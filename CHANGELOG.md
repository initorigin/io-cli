# Changelog

All notable changes to this project are documented here. The format follows
[Keep a Changelog](https://keepachangelog.com/en/1.1.0/) and the project uses
[Semantic Versioning](https://semver.org/spec/v2.0.0.html), staying on `0.x`.

## [Unreleased]

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

[Unreleased]: https://github.com/initorigin/io-cli/compare/v0.1.0...HEAD
[0.1.0]: https://github.com/initorigin/io-cli/releases/tag/v0.1.0
