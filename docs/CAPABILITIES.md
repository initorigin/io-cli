# Capabilities — IO CLI

The index of the guide pages, and the map of what the product holds.

If you arrived in `docs/` rather than at the front page, this is the way in.
[README.md](../README.md) is the landing page and [CONTRACT.md](CONTRACT.md) is
what you may depend on — the argv surface, the exit codes, the seventeen
`[app.io-cli]` keys and the paths io writes.

`io` is an interface. The agent loop, the providers, the tools, the sandbox, the
permission boundary and the session store are all
[io-harness](https://github.com/initorigin/io-harness); nothing below is
reimplemented here, and `tests/dependencies.rs` fails the build if it ever is.

## Guides

One page per capability. Each carries the depth the README does not, including
the limits that capability actually has.

| Guide | What it covers |
| --- | --- |
| [While it works](guide/the-session.md) | The two sticky rows a running turn draws, the agent's manner, answering without opening a run, how much reasoning a turn buys, and background jobs |
| [Keys](guide/keys.md) | Every binding this release ships by default, which six are rebindable, and why `Ctrl+C` is refused in both spellings |
| [Commands](guide/commands.md) | Thirty-six commands in four groups, each group capped at ten, grouped by what you are doing rather than by which part of the harness answers |
| [Bringing your setup across](guide/import.md) | The agent tools already on this machine — instructions, MCP servers, skills and the model — offered item by item, with no credential read at any point |
| [When a run stops for you](guide/resume.md) | A question, a plan or an interrupted call decided where it was left; the turn an interrupt makes unresumable; and one `io` at a time on one conversation |
| [What it costs](guide/accounting.md) | Money and the token split by run, session, model and day; where a price comes from; and why an unpriced model makes a total a floor |
| [Skills](guide/skills.md) | The five this crate ships, adding and removing your own, the claimed name being the frontmatter name, and what a duplicate costs |
| [Capability bundles](guide/plugins.md) | A directory contributing to seven subsystems at once, marketplaces, installing a Claude Code or Codex plugin, the executable a bundle ships, and the disclosure that happens with your configuration file untouched |
| [Hooks](guide/hooks.md) | An audit log, a notification, a formatter or a check that stops the run, declared in `io.toml` — and the failure that is quiet |
| [The fleet](guide/fleet.md) | Contained turns spawning children under one shared ceiling, the live tree, and the plan a contained turn proposes before it acts |
| [Verification gates](guide/verification.md) | The criterion, the fact that it is evaluated after every step rather than once per turn, per-gate retry, and exit `6` |
| [Which model a run asks](guide/providers.md) | The provider chain, the routing rules that change which model answers, and the rule that never reaches a contained turn |
| [Git](guide/git.md) | `/commit`, the refusal it repairs and the half of it fixed upstream, and a checkout of its own |
| [Pictures and documents](guide/media.md) | Attaching an image, drawing the real thing only where the terminal can, and the twelve document tools of which six write |
| [Reading it without seeing it](guide/accessibility.md) | `--plain`: nothing turns, nothing moves, and every state change is committed as text |
| [Headless](guide/headless.md) | `io exec` and `io resume`, the seven exit codes, `--json`, and managing MCP servers, bundles, skills and configuration from a shell |
| [Configuration](guide/configuration.md) | One `io.toml` over io-harness's scopes, every section that bounds a turn, changing it without leaving the session, and where io keeps your things |
| [What the store is holding](guide/store.md) | Reading the run store, undo at three grains, and exporting a conversation or a canonical trace |
| [What this release is not](guide/limits.md) | The limits, stated plainly — including the ones that are decisions rather than gaps |

## Where a capability's truth lives

A claim about behaviour belongs on its guide page, next to the behaviour. A claim
a script depends on belongs in [CONTRACT.md](CONTRACT.md). History belongs in
[CHANGELOG.md](../CHANGELOG.md), and a version number inside a sentence is a
citation rather than a story — the register is written down in [STYLE.md](STYLE.md).

Several tests read these pages and fail when a claim drifts from the code that
decides it. That is deliberate: this product has shipped README claims that were
already false for three releases, and a documentation pass has twice found real
defects in the code. See [AGENTS.md](../AGENTS.md) for which tests gate which
pages.
