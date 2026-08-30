# When a run stops for you

A turn does not always end on its own. The agent can ask what you meant, or
propose a plan before it works; a tool call can be interrupted in a way
io-harness records but cannot judge; and a process that goes away mid-loop leaves
a run with committed work and no ending. Through 0.22.0 all four were left where
they fell — the run was in the store, and nothing here would open it again.

**`/resume` says what each session's last run stopped on, on the row you choose
it from.** The mark is a word rather than a symbol, so it survives `NO_COLOR`,
`--plain` and the ASCII glyph set: `asks` for a question nobody answered, `plan`
for an approach nobody decided, `tool` for a call whose outcome nobody recorded,
`died` for a process that went away and left committed work behind, `ended` for a
turn you stopped yourself. A session with nothing outstanding carries no mark at
all, so the list is ragged by construction and there is no column to read down.

**Choosing a marked session opens the same overlay the run would have opened
while it was live**, and what you say carries *that* run on from the step it
stopped at: the observation ledger, the token budget and the elapsed clock are
the run's own rather than a new run's. A plan is approved, sent back with a
correction, or cancelled outright. An interrupted call is retried or abandoned
here — `r` and `a` — and can also be **asserted to have landed**, which takes an
account of what it returned and is therefore offered by `io resume --recovery
completed --account "…"` rather than by a keystroke. What you say it returned is
filed against the step the call was made on, not the step the run has now
reached, so the resumed run reads a transcript in which the tool answered where
it was asked. A run whose
process merely died carries on from its last committed step plus one. `Esc`
leaves any of them parked exactly as it was found.

**A turn you interrupted is finished, not paused, and it is the one pause that
cannot be answered.** `Ctrl+C` makes io-harness record the outcome `cancelled`,
which is mapped to a *completed* run, and every one of its resume entry points
short-circuits on a completed run and hands back the original outcome having
driven nothing. So the most common way a turn stops is the one way it cannot be
continued. `/resume` reports such a session as ended by you and points at `/fork`
from the turn before it, which is the honest neighbouring answer rather than a
button that would quietly do nothing. io-harness's published documentation says
such a turn "stays resumable"; it is contradicted by the run loop in the same
crate, and that is reported upstream rather than worked around here.

**A turn that ends parked now says so.** Through 0.22.0 the prompt came back with
no sign that a run was sitting in the store waiting for a sentence from you.

### One `io` at a time on one conversation

One store serves this whole machine, so two terminals in one repository is the
ordinary case rather than the exotic one — and they are **not** in conflict.
Starting `io` creates a new session every time, so each terminal gets its own
conversation and neither is refused.

What two of them can genuinely contend over is a single *session*, and that
happens in one place: `/resume`, when one process enters a session another
already has open. Until 0.23.0 nothing guarded it — both advanced the same
conversation head, and the loser of that race had paid for a turn that was then
orphaned off the head path: still in the store, correctly parented, and never
shown again by a history that walks back from the head.

Each session is held under an advisory whole-file lock, and `/resume` into one
another `io` is holding is refused rather than taken. The lock is the kernel's —
`flock` on unix, `LockFileEx` on Windows — so it is released on exit, on a panic
and on `kill -9`: there is no stale lock to reap and no pid file to sweep. The
lock a session takes when it starts never contests anything, because that session
did not exist a moment earlier; what it does is write down who owns it, so the
next process to reach for it can be told.

**What the refusal can say about the holder is what io-cli itself wrote beside
the lock**, and no more: the process id, the workspace root, the `io` version and
the instant that process started. It is not the operating system's account of
that process. Asking the operating system means `/proc` on one platform, `ps` on
another and `tasklist` on a third, or a dependency this crate does not carry — so
the pid you are shown is a number `io` wrote down, and on a machine that has
since reused it, it names something that is not `io`. The lease exists only for
the case the kernel cannot cover, a home on a network filesystem, where an
advisory lock is not this program's business; there the record's own timestamp is
all the evidence there is.

A lock that cannot be taken for an ordinary filesystem reason does not stop the
session — you are told, and it opens. The guard exists to prevent one specific
corruption, and trading it for "io will not start on this machine" would be the
worse failure.

**What it does not cover.** Two `io` in one repository on two different sessions
are not in conflict and are not stopped. `io exec` and `io resume` take no lock
at all, so an `io resume` run beside a terminal holding the same session is not
refused by this. For everything the lock does not see, the guard of last resort
is io-harness's own compare-and-swap on the conversation head: the second writer
is refused, told, and its turn is not silently orphaned — which is the defect
`/undo` carried until this release.

---

[README](../../README.md) · [All guides](../CAPABILITIES.md) · [What you may depend on](../CONTRACT.md)
