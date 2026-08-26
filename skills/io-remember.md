---
name: io-remember
description: Write something down where the next session reads it, choosing between AGENTS.md, AGENTS.local.md and ~/.io-cli/IO.md by who else sees it.
---

Use this when the operator wants a thing to stick: "remember that we use pnpm
here", "always run the formatter before you commit", "stop suggesting that
crate", "note that the staging database is read-only".

## Three files, and the difference is the audience

| File | Who reads it |
| --- | --- |
| `AGENTS.md` | Everyone who clones this repository. It is committed. |
| `AGENTS.local.md` | This checkout alone. It is not committed and nobody else sees it. |
| `~/.io-cli/IO.md` | This operator, in every project on this machine. |

That is the whole decision, and getting it wrong is how a private note about a
colleague ends up in a pull request. A rule about the project's code belongs in
`AGENTS.md`. A rule about this machine, this branch or this person's habits
belongs in one of the other two. If the operator has not said which and the line
could embarrass them, ask before proposing the committed one.

## What actually gets read back

io-harness reads `AGENTS.md` with no configuration at all. The other two are read
only where `[instructions] files` in `io.toml` names them, and that list
**replaces** the default rather than adding to it — so a list that names
`AGENTS.local.md` and `IO.md` and stops there silently stops the repository's own
`AGENTS.md` being read. `/remember` writes the complete list; do not hand-edit
`[instructions] files` to add one name.

`/memory` shows which of the three exist, how many lines each holds, and which
ones io-harness is really reading — the answer to "I wrote it down and it did not
work".

## The surface

`/remember` asks which of the three scopes at the moment it writes, appends the
line as a markdown bullet, and creates the file with a header saying who else
reads it if it was not there. It appends: every byte already in the file stays
where it is.

## What to do

Propose the line in the words it should be written in — short, specific, and
useful to a reader who has none of this conversation — and name the scope you
would put it in and why. Then either send the operator to `/remember`, or append
it yourself: writing one of these files is an ordinary write, shown as a diff and
gated by the session's policy.

Do not say it has been remembered. Nothing is written until the operator approves
the write, and a session that reports a note it never made is one whose next
session behaves as though the operator never typed it.
