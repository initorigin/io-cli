---
name: io-permissions
description: Change what io asks about before it acts, by proposing an edit to the policy defaults in io.toml in the scope the operator means.
---

Use this when the operator asks for the approval posture to change: "stop asking
me before every write in this repository", "always ask before you run anything",
"let it read whatever it likes here", "I want to approve network calls".

## What decides it

`[policy.defaults]` in `io.toml`, four keys and no others:

```toml
[policy.defaults]
read  = "allow"
write = "allow"
exec  = "allow"
net   = "deny"
```

Each is `allow`, `ask` or `deny`. `read` and `write` are separate acts — there is
no single `fs` key, and inventing one is a file io-harness refuses to parse.

Narrower rules go in `[[policy.layers]]`, which are evaluated after the defaults
and may add capability but may never re-allow something an earlier layer denied.
A request like "never touch `.env`" is a layer, not a default:

```toml
[[policy.layers]]
name = "no-secrets"
rules = [{ act = "read", effect = "deny", pattern = ".env" }]
```

## Which file, which is half the answer

Three files, and the later one wins: `~/.io-cli/io.toml` (this operator, every
project on this machine), the project's own `io.toml` (committed, everyone who
clones it), and `io.local.toml` beside it (this checkout, not committed).

"In this repository" means the project file or the local one, never the user
file. io-harness **refuses** a project-scoped change that widens the boundary,
in its own words, and accepts the same value in `io.local.toml` — the rule is
about which file, not which value. So an uncommitted widening belongs in
`io.local.toml` and there is no argument to have about it.

## The surface

`/config` lists every key with the value in force and the file that decided it.
`/config policy.defaults.write allow` asks which of the three files to write, and
only that choice writes. `Shift+Tab` cycles the posture for the session alone and
writes nothing, which is the right answer to "just for now".

## What to do

Say which key, which value and which file, and why that file rather than another.
Then either give the operator the exact `/config` line to type, or make the edit
yourself — an edit to `io.toml` is an ordinary write, shown as a diff and gated
by the same policy as any other write, and the operator approves it or does not.

Do not report the boundary as moved. Until the write is approved and the next
turn begins, the file on disk still says what it said before, and a session that
claims otherwise is a session the operator cannot trust about anything else.
