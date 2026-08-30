<div align="center">

<img src="https://raw.githubusercontent.com/initorigin/io-cli/main/assets/initorigin-logo.png" alt="InitOrigin" width="112" height="112">

# IO CLI

**A terminal agent that shows you what it is allowed to do, what it is spending,
and what it refused — while it works.**

[![CI](https://github.com/initorigin/io-cli/actions/workflows/ci.yml/badge.svg)](https://github.com/initorigin/io-cli/actions/workflows/ci.yml)
[![Release](https://img.shields.io/github/v/release/initorigin/io-cli)](https://github.com/initorigin/io-cli/releases)
[![MSRV](https://img.shields.io/badge/MSRV-1.95-blue)](Cargo.toml)
[![License](https://img.shields.io/badge/license-Apache--2.0-blue)](LICENSE)

</div>

`io` is an interface. The agent loop, the providers, the tools, the sandbox, the
permission boundary and the session store are all
[io-harness](https://github.com/initorigin/io-harness), and none of them are
reimplemented here. A test asserts that: `tests/dependencies.rs` fails the build
if this crate ever grows an HTTP client, a TLS stack, a database or a sandbox.

## Contents

- [What you get](#what-you-get)
- [It never takes your terminal](#it-never-takes-your-terminal)
- [Install](#install)
- [First run](#first-run)
- [Guides](#guides)
- [Platform support](#platform-support)
- [Stability](#stability)
- [Security](#security)
- [Contributing](#contributing)
- [Licence](#licence)

![A session at rest: the IO CLI card in the terminal's own scrollback, carrying
the version and the tagline above the model, the permission posture and the
workspace; a muted line saying this is a new conversation and the last one is
still in /resume; an empty prompt below it; and a two-row footer under a rule,
naming the state, the model and the clock on one row and the keys and the
posture on the next.](assets/screenshot.png)

## What you get

| | What it gives you |
| --- | --- |
| **A session you can read** | Every finished line in the terminal's own scrollback, designed rather than defaulted: a tool call as a verb and a path, a thought as a thought, an answer that ends the turn |
| **A working view** | Two sticky rows while a turn runs — a word for the turn with its clock and spend, and a line under it saying what is happening *now* |
| **The boundary, visible** | The posture on the footer, a refusal that names the act, the target, the rule and the layer, and `Shift+Tab` to change it from the next turn |
| **Approvals in place** | A write stops the run and shows the diff it proposes; `y`, `a` or `n`, answered where it was asked |
| **A fan-out you can watch** | Contained turns spawn children under one shared ceiling; `Ctrl+F` shows the tree and what it is costing |
| **Your file, in force** | Every section of `io.toml` bounds a session turn as it bounds `io exec`; the budgets in force are on the status line with what is left of them, and `/status` commits the whole state — policy layers, sandbox backend, caps, budgets, connections — into the scrollback |
| **Undo** | `Esc Esc` at an empty prompt rewinds the last turn — its files, its memory and the conversation head |
| **Conversations that survive** | `/resume` reopens an earlier session and answers whatever its last run stopped on, `/fork` continues from an earlier turn, `/clear` starts fresh without leaving |
| **A paused run, answered** | A question, a plan or an interrupted call is decided where it was left and the run carries on from the step it stopped at — from a session, or from a script with `io resume` |
| **Headless** | `io exec` runs one goal to completion with documented exit codes and `--json`, and `io resume` carries a parked one on |
| **Readable without seeing it** | `--plain` animates nothing and commits every state change as text, for a screen reader, a braille display or a log |
| **Markdown, rendered** | Headings, bullets, code and emphasis drawn as themselves rather than printed as notation |
| **Documents, read and written** | Spreadsheets, Word, slide decks, PDFs and barcodes through io-harness's own tools — twelve of them, six of which write, every one under the same gate as any other read or write |
| **Your setup, brought across** | `/import` finds the agent tools already on this machine and offers their instructions, MCP servers, skills and model, item by item, with the whole plan shown before a byte is written and no credential read at any point |
| **What it spent, and whether it worked** | `/cost` commits the money and the token split — this run, this session, by model, by day — and `/stats` commits the outcomes, the first-try counts, the gate failures and the latencies. Nothing is estimated: an unpriced model makes a total a floor and the page says so |

## It never takes your terminal

`io` does not enter the alternate screen and does not capture the mouse, in any
mode, behind any flag. Every finished message, tool call and system line is
committed into the terminal's own scrollback; eight rows at the bottom hold the
unfinished tail of a streaming answer, a blank row, the activity line, a rule,
one row of composer and a three-row footer, and only those repaint.

So when the session ends the whole conversation is still there. Your terminal's
search finds it, tmux copy-mode scrolls it, and a mouse drag selects it — none of
which is implemented here. It works because it was never taken away.

That is a property, not an intention. `tests/structure.rs` captures every byte
`io` writes to the terminal over a scripted session and fails if the
alternate-screen or mouse-capture sequences appear, so no later release can
reintroduce fullscreen without turning a named test red.

## Install

macOS and Linux:

```sh
curl -fsSL https://raw.githubusercontent.com/initorigin/io-cli/main/install.sh | sh
```

Windows, in PowerShell:

```powershell
irm https://raw.githubusercontent.com/initorigin/io-cli/main/install.ps1 | iex
```

Both scripts pick the right build for your machine, **verify it against the
published `SHA256SUMS` before unpacking it**, and install into a directory you
own — `~/.local/bin`, or `%LOCALAPPDATA%\io\bin` on Windows. Neither needs
administrator rights and neither edits your shell profile: if the directory is
not on your `PATH`, the script prints the line to add.

Re-running the script is how you update. There is no auto-update and no version
check: a terminal tool that contacts a server you did not ask it to contact is
one of this product's stated non-goals.

Set `IO_VERSION` to install a specific version, and `IO_INSTALL_DIR` to install
somewhere else.

**What the checksum is and is not.** It defends against a truncated download and
a tampered asset. It does not defend against a compromised repository — piping a
script from the internet into a shell is a trust-the-publisher model however the
script is written, and this is the honest description of it. Read
[`install.sh`](install.sh) first if you would rather not take that on trust.

There is no crates.io publish and `cargo install io-cli` is not a path. To build
from source, clone this repository and run `cargo build --release`; the binary is
`target/release/io`.

## First run

Run `io` in a repository. With no configuration it walks you through a provider,
a key it verifies against the live endpoint before continuing, a model from that
provider's catalogue, a theme with the sample re-rendering as you move, and a
default permission posture. It shows exactly what it will write and where, and
writes nothing until you say so. The file is io-harness's own, at mode `0600`.

Run it again at any time with `io setup`.

Your key never appears on screen, in the scrollback, or in a log line. If the
provider's environment variable is already set, the wizard offers to use it and
writes no key to disk at all.

## Guides

One page per capability, each carrying the depth this page does not — including
the limits that capability actually has.

| Guide | What it covers |
| --- | --- |
| [While it works](docs/guide/the-session.md) | What a running turn shows you, how the agent's manner is drawn, answering without opening a run, how much reasoning a turn buys, and background jobs |
| [Keys](docs/guide/keys.md) | Every default binding, and moving one |
| [Commands](docs/guide/commands.md) | All thirty-six, grouped by what you are doing |
| [Bringing your setup across](docs/guide/import.md) | What `/import` finds on this machine, item by item, and the credential it never reads |
| [When a run stops for you](docs/guide/resume.md) | A question, a plan or an interrupted call answered where it was left — and one `io` at a time on one conversation |
| [What it costs](docs/guide/accounting.md) | The money and the token split, where a price comes from, and why an unpriced model makes a total a floor |
| [Skills](docs/guide/skills.md) | The five shipped, yours, and what a duplicate name costs |
| [Capability bundles](docs/guide/plugins.md) | A directory contributing to six subsystems, marketplaces, and what a bundle is allowed to do being shown before it may do it |
| [Hooks](docs/guide/hooks.md) | Reacting to a run from `io.toml`, and the failure that is quiet |
| [The fleet](docs/guide/fleet.md) | Contained turns, the tree `Ctrl+F` shows, and the plan a contained turn proposes |
| [Verification gates](docs/guide/verification.md) | The criterion, what it costs per step, and exit `6` |
| [Which model a run asks](docs/guide/providers.md) | The chain, the routing rules, and the one that is inert |
| [Git](docs/guide/git.md) | `/commit`, the refusal it repairs, and a checkout of its own |
| [Pictures and documents](docs/guide/media.md) | Attaching an image, drawing it where the terminal can, and the twelve document tools |
| [Reading it without seeing it](docs/guide/accessibility.md) | `--plain`, and what a screen reader, a braille display or a log gets |
| [Headless](docs/guide/headless.md) | `io exec`, the exit codes, the JSON, resuming without a terminal, and managing the configuration from a shell |
| [Configuration](docs/guide/configuration.md) | Every section of `io.toml`, changing it without leaving the session, and where io keeps your things |
| [What the store is holding](docs/guide/store.md) | Reading the store, putting work back, and taking the work out |
| [What this release is not](docs/guide/limits.md) | The limits, stated plainly, and the ones that are decisions rather than gaps |

[docs/CAPABILITIES.md](docs/CAPABILITIES.md) indexes them.
[docs/CONTRACT.md](docs/CONTRACT.md) is what a script may depend on: the argv
surface, the exit codes, the configuration keys and the paths io writes.

## Platform support

| Platform | Build | Containment |
| --- | --- | --- |
| macOS, Apple silicon | `aarch64-apple-darwin` | io-harness's own: `sandbox-exec` |
| macOS, Intel | `x86_64-apple-darwin` | as above |
| Linux | `x86_64-unknown-linux-musl`, statically linked | io-harness's chain: Landlock, `bwrap`, namespaces, floor |
| Windows | `x86_64-pc-windows-msvc` | Job Object, with AppContainer opt-in |

The four artifacts and their `SHA256SUMS` are attached to every GitHub Release,
and the full test suite runs on Ubuntu, macOS and Windows in CI. **What confines
a command is io-harness's, not this product's** — `io` shows you which backend
actually answered on this host, in the footer, because the mode asked for and the
backend that applied are not the same fact.

Rust 1.95 or later to build from source. There is no crates.io publish: the
distribution channel is the GitHub Release, and `publish = false` makes an
accidental one impossible rather than merely discouraged.

## Stability

Pre-1.0 and staying there until the owner says otherwise. A minor release may
change what a session looks like — 0.11.0 rewrote the transcript's vocabulary,
and the release before it moved where a question is answered. What you can rely
on is that every one of those is in [CHANGELOG.md](CHANGELOG.md), said plainly,
and that a configuration file written for an older release keeps working. **One
key has been removed in the product's life so far**: `[app.io-cli] max_steps`,
deprecated in 0.14.0 and removed in 0.16.0, with two releases' notice given in
the terminal, the README and the changelog. A file that still carries it loads
exactly as before — the key is ignored rather than rejected — and the session
says so once at startup, naming the number that is no longer in force and
`[run] max_steps` as where the cap lives now. **A section that was ignored may
start being read**, which 0.14.0 did to eleven of them, and that is a behaviour
change for a file that already carried one; it is the migration note in
[Configuration](#configuration) and in the changelog rather than something to
find out from a turn.

## Security

Report vulnerabilities per [SECURITY.md](SECURITY.md). Your provider key is never
printed, never committed to the scrollback, and never written to disk by the
wizard when the provider's own environment variable is already set.

## Contributing

See [CONTRIBUTING.md](CONTRIBUTING.md). `develop` is the working branch; nothing
reaches it or `main` except through a pull request.

## Licence

Apache-2.0. Copyright 2026 Aakash Pawar (InitOrigin). See [LICENSE](LICENSE) and
[NOTICE](NOTICE).
