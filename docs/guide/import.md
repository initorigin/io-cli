# Bringing your setup across

**You have almost certainly used another agent tool first, and `/import` brings
what you told it.** Four things carry over: the standing instructions you wrote,
the MCP servers you configured, the skills you collected, and the model you
settled on. Nothing else, and nothing without you saying so.

It is offered **once**, on a first run, and io records that it asked so it never
asks again. One key declines and the session carries straight on — declining
writes nothing at all, and there is no reminder later. `/import` opens the same
thing at any time.

**What it looks at:**

| Where | What is read |
| --- | --- |
| `~/.claude/` | `CLAUDE.md`, `settings.json`, and the skills under `skills/` and `plugins/` |
| `~/.claude.json` | the MCP servers, which live here and not beside `settings.json` |
| `~/.codex/` | `AGENTS.md`, `memories/MEMORY.md`, `config.toml`, `rules/default.rules` |
| `~/.gemini/` | `GEMINI.md` and `antigravity/mcp_config.json` |
| the repository | `.cursorrules` or `CONVENTIONS.md`, if either is there |

A tool that is not installed simply is not offered. A tool whose files are all
**empty** is a different row and says so — on a good many machines both Gemini
files exist and each one is zero bytes, and an import of nothing that
then reports success is the failure you cannot see.

**Where it writes:** instructions are appended to the memory file for the scope
you pick — one block per source file, with a line of provenance above the
original text, kept whole rather than shredded into a bullet per line. MCP
servers become `[[mcp]]` entries in io-harness's own spelling. Skills become
directories under `~/.io-cli/skills`. The model is *carried*, not written: a
`[[provider]]` entry needs a vendor and a foreign tool's model string does not
name one — `gpt-5` could be OpenAI or any of the twenty-one presets pointed at a
compatible endpoint — so io hands you the id and the entry is built once you have
chosen the vendor.

**Where a file *is* decides what it is, ahead of what it is called.** A loose
`CONVENTIONS.md` — or `CLAUDE.md`, or `MEMORY.md` — sitting inside a `skills/` or
`plugins/` directory is a skill, and from 0.22.0 it is imported as one. It used to
match on its basename and be appended whole into the instructions file that is
loaded on **every turn, forever**, instead of being a named skill the model reads
on demand.

**The whole plan is on screen before a single byte is written.** One row per
thing found, saying where it came from and where it would go, and you accept them
item by item. What you did not accept is not written. A cancelled import is not a
partial one.

**No credential is ever read or copied, and that is enforced by the code rather
than promised by it.** `~/.codex/auth.json` is not in the list of files this
program can open, so no path through it reaches one. A server's `env` values are
parsed and thrown away without ever being assembled into a string — only the
variable *name* is ever held — and what gets written is `${env:NAME}`, the name
pointing at itself, which io-harness resolves out of your own environment at the
moment a run needs it. Your shell has to have those variables set, and the import
says which. `~/.claude.json` is an entire application's state with OAuth material
in it and is read through narrow structs, so every field io does not name is
skipped by the parser instead of being loaded and then politely ignored.

**An allowlist is read, shown, and deliberately not translated.** Codex's
`prefix_rule(pattern=["bun","install"], …)` and Claude's `Bash(cargo yank *)` both
match a *command line*. io-harness's `Act::Exec` matches a **binary name and
nothing else** — it has no argument matching at all. So the closest faithful
import of `bun install` is a blanket allow on `bun`, which is a wider permission
than you ever granted, written by a tool you were trusting to be careful. io says
what it found and says it cannot express it, and produces no rule, no `[policy]`
table and no policy layer. A boundary half imported is worse than one left alone.

**Two skills of one name kills a session, so an import counts before it writes.**
A name already answered to in your skills directory is refused on its own row and
the rest of the import still goes through. Going over io-harness's ceiling refuses
**every** skill instead: the harness rejects a whole directory rather than the
excess, so an operator at 63 skills who imported three more would get a session in
which every turn dies at run start with nothing visible to blame. See
[Skills](skills.md#skills).

---

[README](../../README.md) · [All guides](../CAPABILITIES.md) · [What you may depend on](../CONTRACT.md)
