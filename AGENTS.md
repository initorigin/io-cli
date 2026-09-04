# AGENTS.md

Guidance for any coding agent working in this repository — Claude Code, Codex, Cursor, Gemini
CLI, Copilot, or whatever comes next. This file is the single source of truth; harness-specific
files such as `CLAUDE.md` point here rather than restating it.

`io-cli` is a terminal interface to [io-harness](https://github.com/initorigin/io-harness): one
crate, one binary (`io`), an inline ratatui viewport and a headless `io exec`. It contains no
agent loop, no provider client, no tool, no sandbox, no policy engine and no session store —
every one of those is io-harness's, and `tests/dependencies.rs` fails the build if this crate
grows one. MSRV is `rust-version` in `Cargo.toml` (1.95 today).

## Commands

```bash
# What CI's build-and-test job runs.
cargo build --all-targets
cargo test --all-targets
cargo test --doc                    # a separate step; --all-targets does NOT run doctests

# One test file / one test
cargo test --test docs
cargo test --test docs f3_          # substring match on the test name

# Lint gate — CI runs clippy WITH -D warnings. Running it without the flag is a
# weaker gate than the one that decides, and 0.30.0 shipped a red CI because of it.
cargo fmt --all -- --check
cargo clippy --all-targets -- -D warnings

# The live suite: real provider, real money, #[ignore]d so it never runs by accident.
# Source .env first.
cargo test -- --ignored --nocapture

# A sabotage cycle needs --no-fail-fast, or the run stops at the first failing
# binary and reads as "killed only its own test".
cargo test --no-fail-fast
```

`target/debug/incremental` grows to tens of gigabytes across a release. A build error that looks
impossible — a test binary that is 896 bytes, or `Permission denied (os error 13)` — is a full
disk writing truncated output, not a code failure. Run `df -h .` first.

## Architecture

**A renderer, not a runtime.** `src/main.rs` is the driver and owns the event loop; everything
it draws comes off io-harness's event stream. There are two entry points over the same session:
an interactive turn and `io exec`, and since 0.17.0 both are steered and both carry a contract.

**The dependency set is exactly ten names**, asserted in both directions by
`tests/dependencies.rs` so an *unused* permitted name also fails: `io-harness`, `ratatui`,
`crossterm`, `clap`, `tokio`, `serde`, `serde_json`, `toml`, `syntect`, `image`, with `tempfile`
the only dev-dependency. Adding one is a deliberate, argued act — read the comments in
`Cargo.toml`, which carry the argument for every name including the one that was given back.

**Three rules the dependency gate enforces by exact path, and they are exact paths compared with
`==` rather than substrings — a stem or substring match is a permission list that widens itself:**

- **Process spawns** are permitted in `src/shell.rs` (the `!` escape) and `src/fetch.rs` (one
  `git clone` for a marketplace) and nowhere else.
- **TOML parsing** is permitted in `src/edit.rs` alone. Every other module that needs a value out
  of a configuration file goes through `edit::value_at`, `edit::sections`, `edit::keys` or
  `edit::array`.
  `src/marketplace.rs` reads a stranger's `plugin.toml` that way, and `src/adapt.rs` *writes* one
  without ever parsing it.
- **JSON deserialization** is permitted in `src/import.rs` and `src/adapt.rs`, and nowhere else.
  The rule is over the parse rather than over the crate name: `src/exec.rs` serializes its own
  `--json` event lines and is not a second opinion about anybody's file. The set is compared with
  `==` and has its own near-miss test, because a widening makes a gate more permissive and so goes
  vacuous without going red.
- **No agent loop.** The sweep strips comments first, so a doc comment containing "while" is not
  a finding; `src/provider.rs` is exempted by path and held to four properties of its own.

**Module map (`src/`, 82 files).**

- Session surface — `app.rs` (state), `term.rs` (an inline viewport, never the alternate
  screen), `events.rs` + `triage.rs` (what each io-harness event does; a kind with no
  disposition is counted, never silently dropped), `status.rs`, `composer.rs`, `editor.rs`
  (the readline keymap, this crate's own since `tui-textarea` was dropped), `picker.rs`,
  `theme.rs`, `glyphs.rs`, `keys.rs`, `commands.rs`.
- Contract — `contract.rs` is the one place a session's `TaskContract` is built, and it is
  conditional in every field: nothing configured must reproduce io-harness's `default_contract`
  field for field, asserted by Debug equality.
- Configuration — `edit.rs` (a format-preserving writer that replaces one value's bytes),
  `settings.rs`, `configure.rs` (`/config`), `servers.rs` (`/mcp`), `providers.rs`,
  `home.rs` (`~/.io-cli`), `reload.rs`, `memory.rs`, `recall.rs`, `upgrade.rs` (which of the
  three installers placed the running binary, read from its own path and never from a network).
- Work — `exec.rs` (headless, and the exit-code mapping), `resume.rs`, `lock.rs`, `queue.rs`,
  `compact.rs`, `context.rs`, `fleet.rs`, `plan.rs`, `intent.rs`, `gates.rs`, `routing.rs`,
  `provider.rs`.
- Store — `store.rs`, `export.rs`, `undo.rs`, `rewind.rs`, `sessions.rs`.
- Bundles — `skills.rs`, `skillview.rs`, `plugin`-facing `pluginview`, `marketplace.rs`,
  `fetch.rs`, `import.rs`, `adapt.rs` (the three manifest formats io did not invent, read; and the
  `plugin.toml` io generates for one, written and never parsed), `bundle_path.rs` (where a
  declared `[[bin]]` goes, appended and creating no file — io-harness places nothing itself).
- Media and git — `picture.rs`, `attach.rs`, `repo.rs`, `commit.rs`.

**Nothing under `tests/` links `src/main.rs`.** A guard written in the driver cannot be tested
and cannot be sabotaged, so a criterion whose only site is the driver has no gate at all. Move
the decidable part into the library — `store::acts` exists for exactly this reason — or write a
source-text gate over the driver and say that is what it is.

**The command surface is capped.** 36 slash commands in four groups, occupancy
Session 7 · Configure 9 · Turn 10 · Inspect 10, with ten the hard bound per group. A new command
means re-filing an existing one, not widening the bound; `tests/commands.rs` asserts the
occupancy and names the group that moved.

## The tests that gate documentation

Several tests are checkers over prose, not over behaviour. They fail merges when the docs drift.

- `tests/docs.rs` — the largest of them. The key table, the rebindable actions, the shipped
  skills and the command list in the prose must agree with the constants the code renders. The
  same constants feed `/help`, so `/help` and the docs cannot disagree.
- `tests/commands.rs` — the group occupancy, and every command the prose names must exist.
- `tests/exec.rs` — the documented exit codes against `exec::code`'s real mapping, with the
  `RunOutcome` variants read out of the locked io-harness source.
- `tests/manage.rs` — what `clap` actually routes, asked of `clap::CommandFactory` itself,
  compared against the surfaces `manage::parse` accepts. This test exists because 0.30.0
  documented and shipped `io skill` while the argv door had no variant for it, and 1,609 tests
  passed over the gap because they all entered one layer below the door.
- `tests/configure.rs` — every key `docs/config.example.toml` names must deserialize.

**Write every new assertion by asking what makes it FAIL.** This repository has shipped a gate
that compared a filter against a filter of itself, one that asserted `parse(X) == parse(X)`, and
one whose right operand was always false. Where two doors call one function, **count the call
sites; never `contains`** — one site satisfies a `contains` forever.

## Conventions

**Prose register** — `docs/STYLE.md` is the rule, and it is not linted, so it is on you. Present
tense, no diary (history goes in `CHANGELOG.md`; a version number in a sentence is a citation,
not a story). Name the reason for a non-obvious decision where the decision is, once. No first
person, no "powerful"/"robust"/"simply".

**A bundle's name is translated where it is drawn, never renamed.**
`io_harness::NAMESPACE` is `__` and it is load-bearing on the wire — the system prompt, the tool
dispatch and every event `target` carry it — so `naming::display` is applied at the moment a name
reaches a person and `naming::wire` to a name they typed. `tests/namespacing.rs` walks the drawn
output of six operator-facing surfaces (the transcript, the status line, the pickers, and the
plugin, skill and marketplace panes) and fails when one of them draws a name still in the wire
spelling, so a new surface that draws a contributed name inherits the rule instead of rediscovering
it. **Not "the separator appears nowhere"** — that is a different and wrong property, and the
release that set this rule shipped the defect proving it. Translate the qualifier only: a path is
not a name, and `read src/__init__.py` drawn through `display` is a file that does not exist; and
only the FIRST separator is the join, so `bundle__deep__nested` is drawn `bundle:deep__nested` and
keeps one. A gate spelled as the blanket rule reads `display` written with `replace` as a fix.

**Comments carry the argument.** This codebase keeps long "why" comments — `Cargo.toml`'s
dependency notes, `tests/dependencies.rs`'s exemption rationale. Match that density; do not strip
them. A gate that reads prose can forbid a file from explaining itself: `tests/dependencies.rs`
matches its forbidden literals in raw text, so do not write the banned spelling into a comment
saying the file does not use it.

**Test names are sentences about behaviour** — `f6_a_modified_file_comes_back_as_it_was`, not
`test_undo_2`. The `f`/`n`/`o` prefixes point back at the release contract's numbered acceptance
criteria.

**Documentation is part of the change, not after it.** The README, the guide page, the CHANGELOG
entry and `docs/config.example.toml` are updated in the same commit stream as the code. A page
describing a version that no longer exists is worse than no page.

**Release contracts.** `.ultraship/products/io-cli/releases/X.Y.Z.yaml` holds a version's scope
and numbered acceptance criteria, written before the code and sealed after. `.ultraship/` is
gitignored whole, so the record lives on disk and never in the history — say so in the release
commit rather than letting the absence imply it was never written.

**Branch and release flow** (`CONTRIBUTING.md`, `docs/RELEASE_PROCESS.md`): work branches are
`feat/<version>` cut from `develop`; PRs go into `develop`; `main` holds released versions only
and a `develop` → `main` PR *is* the release. There is no registry — `publish = false`, and the
GitHub Release with four cross-compiled artifacts plus `SHA256SUMS` is the whole distribution
channel. Never add AI attribution to a commit, a PR or a release note.
