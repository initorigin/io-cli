# Public contract — IO CLI

What you may depend on, what may change, and what does not work today.

A library's contract is its API. A command-line tool's contract is what a script may rely on:
the argv it accepts, the codes it exits with, the JSON it emits, the keys it reads out of a
configuration file, and the paths it writes to. That is what this page enumerates.

The product is **pre-1.0 and stays pre-1.0** until its owner says otherwise. Within `0.x`, a
breaking change is a minor bump; there is no other room for one.

## The argv surface

```
io [-C DIR] [-m MODEL] [--profile NAME] [--plain] [<subcommand>]
```

All four flags are `global`, which means they are accepted on **either side** of a subcommand:
`io -C dir exec "…"` and `io exec -C dir "…"` are the same command. A flag whose acceptance
depends on which side of a word it is typed is a flag that works only on its author's machine,
and 0.5.0 shipped that defect once.

With no subcommand, `io` opens an interactive session.

| Subcommand | What it does |
| --- | --- |
| `io setup` | The first-run wizard: provider, credential, model, permission posture, theme |
| `io exec "<goal>"` | Runs one goal to completion with no terminal interaction |
| `io resume` | Carries on a run that stopped for a question, a plan or an interrupted call; `--list` shows what is waiting |
| `io mcp …` | Manage MCP servers without opening a session |
| `io plugin …` | Manage capability bundles and marketplaces |
| `io config …` | Read and write configuration keys |
| `io skill …` | Add, list and remove skills |

`--plain` reaches an interactive session and stops there. `io exec` builds no theme, draws
nothing and animates nothing already, so there is no second thing for the flag to switch off.

`io exec` additionally takes `--json`, `--sandbox <read-only|workspace-write|full-access>`,
`--policy <workspace|read-only>` and `--provider <openrouter|anthropic|openai>`.
**`--policy ask-writes` is refused**, because nothing headless can answer an approval.

## Exit codes

These are the contract a CI job depends on. **A code never changes meaning under a dependency
bump** — when io-harness 0.70.0 split `VerificationFailed` away from `StepCapReached`, mapping it
to the existing `3` would have moved exactly the runs `6` was invented for.

| Code | Name | Means |
| --- | --- | --- |
| `0` | OK | The run finished |
| `1` | FAILED | The run failed |
| `2` | REFUSED | Denied, refused, or the plan was rejected |
| `3` | CEILING | A step, time, cost or budget ceiling was reached, and nothing judged the work |
| `4` | PAUSED | The run is waiting on a question, a plan, an approval or an interrupted call — resumable with `io resume` |
| `5` | UNFINISHED | The run ended in a state this table does not name |
| `6` | UNVERIFIED | The work was judged by a verification gate and did not hold up |

Exit `4` names the `question_id`, `plan_id` or `attempt_id` that `io resume` needs.

## Configuration

The file is io-harness's `io.toml`, and its resolution is io-harness's, not this crate's:
`$IO_CONFIG` names a file outright and wins over everything; otherwise `$IO_CONFIG_HOME` names
the directory; otherwise the platform default. io-cli's only contribution is to set
`$IO_CONFIG_HOME` to its own home when the operator has set neither.

`[app.io-cli]` is the one section io-harness deliberately does not validate, so it is this
crate's own and this page is its contract. It carries **seventeen** keys:

| Key | Shape |
| --- | --- |
| `theme` | string; absent means "detect from the terminal background" |
| `diff` | `unified` or `minimal`; absent means `unified` |
| `glyphs` | `unicode` or `ascii`; absent means "ask the locale" |
| `plain` | bool; the same switch as `--plain`, and the flag wins |
| `keys` | a table of action name to chord |
| `containment` | the caps a fan-out runs under — **this key is what turns the fleet on** |
| `mcp` | MCP servers, merged with io-harness's own by id |
| `lsp` | LSP servers, merged the same way |
| `browser` | io-harness's `BrowserConfig` |
| `skills` | a path |
| `max_parallel_reads` | integer |
| `spawn_background_after_secs` | integer |
| `detached_spawns` | bool |
| `prices` | a price table |
| `gates` | the verification criterion |
| `conversational` | bool |
| `routing` | which model answers, and what happens when a provider stops |

**A sub-table's fields are optional and refused by name, never required.** A required field is a
*deserialization* failure that takes the whole `[app.io-cli]` section down with it — the theme,
the keys, the ceilings and the verification gate — over one misspelt line.

`[app.io-cli] max_steps` was **removed**. The notice survives it, read from the raw section,
because the removal would otherwise be silent.

Every key is documented with a worked example in [config.example.toml](config.example.toml).

## Where io keeps your things

One directory: `~/.io-cli` on Unix, `%USERPROFILE%\.io-cli` on Windows, created `0700`.

| Path | Holds |
| --- | --- |
| `io.toml` | The user-scope configuration |
| `runs.db` (+ `-wal`, `-shm`) | The session store |
| `skills/` | Skills, with `skills/disabled/` for the ones switched off. Read from and written to the home in force, so `$IO_CONFIG`/`$IO_CONFIG_HOME` moves it with `io.toml` and `IO.md`. io does not move files you already own; it says so at startup if the old directory still holds any |
| `marketplaces/` | Cloned marketplaces, with `marketplaces/.entries/` for a repository an index pointed at |
| `adapters/<owner>/<repo>/<name>/` | The `plugin.toml` io generates for a Claude Code or Codex bundle |
| `.fetching/` | Staging for a clone in flight, renamed into place on success |
| `IO.md` | The user-scope memory file |
| `.import-offered` | Marks that the import offer was made, so declining is remembered |

An install found elsewhere is **moved** in, never onto a file that already exists, and nothing is
deleted that was not already copied. This does nothing at all when `$IO_CONFIG` or
`$IO_CONFIG_HOME` is already set.

## Limits that hold today

**A marketplace, and an entry inside one, resolve against `github.com` and no other host.** A
marketplace is named `<owner>/<repo>`; an index entry naming a repository elsewhere has its url
re-derived to that shape, and a url that is not two ordinary path segments on that host is refused
and listed with its reason. The only string io hands `git` is one io built — a url somebody else
wrote reaching a clone is how `ext::sh -c …` becomes a remote shell. All 238 remote entries of
`anthropics/claude-plugins-official` are on that host.

**A Claude Code or Codex plugin's hooks are read, shown, and translated into nothing.**
io-harness's hooks are argv against its own event tags and never a shell string, and it refuses
`${env:}`, `${file:}` and `${cmd:}` inside a manifest in every scope. This is an impossibility, not
a deferral: the author adds a `plugin.toml` if they want hooks under io.

**A turn the operator interrupted cannot be resumed.** It is reported as ended, with `/fork`
offered instead.

**An imported allowlist is read, shown, and translated into nothing.** io-harness's `Act::Exec`
matches a binary name and nothing else, so the nearest faithful import of `bun install` is a
blanket allow on `bun` — wider than the operator granted. This is an impossibility, not a
deferral.

**`servers::edit` is reachable from no keystroke.** `/mcp` adds and removes; editing an existing
entry in place is not wired to a key.

**Routing never reaches a contained turn.** io-harness applies routing in the flat loop only; a
tree takes each agent's model from its own roster entry. The session discloses this at all three
points an operator can enter the state.

**Every HTTP MCP server is refused by default**, because the default network effect is `Deny`. A
bare-host rule opens it.

**Ten commands run while a turn is in flight, and the rest are refused.** `/status`, `/context`,
`/cost`, `/stats`, `/help`, `/theme`, `/copy`, `/expand`, `/fleet` and `/image` report while the
agent works; `/` and `@` open the palette and path completion; `/compact` and `/steer` reach the
turn through their own arms as they always have. Every other command keeps its refusal, in the same
sentence, and the rule is what a command *does* rather than how harmless it looks: anything that
reassigns the session or the provider, writes the store or a configuration file, or submits a turn
of its own is refused.

**`/config` is refused mid-turn in every form, including the bare one.** Its picker offers a row
that re-reads the provider's catalogue, writes the prices into a scope file and reassigns the
configuration the running turn is holding. The run-state guard is on the whole command deliberately
— splitting a bare form from an argued one is where a mistake ships a write into a running turn —
and the same applies to `/plugin`, `/mcp`, `/provider`, `/skills`, `/memory` and `/store`.

**A mid-stream token figure is an estimate and is drawn as one.** `EventKind::Token` carries text
and no count, because providers do not bill per chunk, so the figure that moves while a step is
streaming is derived from the delta text and written `~1.2k tok`. It is replaced by the provider's
own number the moment the step commits, and the settled form has no tilde. Nothing adds the two
together.

**A bundle's contributions are displayed with a colon and addressed with io-harness's separator.**
`ultraship:brainstorm` is what you read; `ultraship__brainstorm` is what the model was shown and
what `read_skill` resolves. `io exec`, `io plugin` and `io skill` report the wire name, because a
script addresses the wire.

## What may change

Everything not named above. In particular: the layout of what is drawn on the terminal, the
wording of any line, the set of slash commands and which group each sits in, and the internal
module structure of the crate. The library target (`io_cli`) exists so that tests can link
against it and is not a supported API — `src/main.rs` is not linkable from `tests/` at all, which
is why any behaviour that needs a gate lives in the library rather than the driver.
