# What the store is holding

**`/store` commits a page: what the run store costs on disk, what is already free
inside it, and what each session in it holds.** Everything on that page is
io-harness's own arithmetic — `page_size × page_count` for the file, and
`page_size × freelist_count` for the free part — read from the store rather than
computed here.

The distinction between those two numbers is the whole reason this page exists.

**A deletion does not shrink the file.** SQLite frees pages *into* the database
rather than out of it, so removing a session moves bytes from the file's size
into the free space inside it and the file on disk stays exactly the size it was.
Every store this product has ever created was made without `auto_vacuum`, so
there is no incremental reclamation either: a `VACUUM` is the only thing that
returns the space, and it is `/store compact`.

That matters because `~/.io-cli/runs.db` has held every session, run, step,
event, provider call, snapshot and restore point since 0.15.0, with no retention
policy and no rotation. Until this release there was no way to look at it, and no
way to shrink it.

Three verbs change it, and each one shows what it will do before it does it:

| | |
|---|---|
| `/store rm <id>` | remove one session and everything keyed to it |
| `/store sweep <date>` | remove every session created before that timestamp |
| `/store compact` | rewrite the database without its free pages |

**Each descends into a confirmation whose first row is "leave it".** That is the
row the cursor starts on, so the keystroke you give by reflex is the one that
changes nothing.

**A removal is final and takes the session's restore points with it.** Its
*memory* stays — the agent's durable notes belong to the workspace rather than to
the session, so removing a session unlearns nothing — but the rewind for those
turns is gone, and that is the part that bites later.

**A sweep refuses a session that still holds a resumable run**, and names it. A
date is a policy applied to sessions nobody looked at, and a crash-resumable tree
that vanished because it was old would be the worst thing this command could do.

**The sweep asks you to agree to the rule rather than to a count**, and the
reason is a gap in io-harness rather than a choice: `sweep_sessions` filters on
`sessions.created_at` and nothing exposes that column, so the set a date selects
cannot be counted until the sweep has run. The nearest substitute — a session's
first turn — is always *later* than the session itself, so a count built on it
would under-state what is about to be deleted. The figures are reported the
moment it finishes, refusals included. Filed as io-harness#216.

**Compaction is not free while it runs.** It rewrites the whole database and
needs roughly the file's own size in free disk space to do it, so it is a thing
you ask for rather than something a deletion does on your behalf. It reports the
bytes the file actually shrank by, measured, not inferred.

Nothing here happens on its own — there is no retention setting, no threshold and
no sweep at startup — and no model can reach any of it.

## Putting work back

**`/undo` is the size of the mistake.** Until this release the only instrument was
the rewind chord, which undoes a whole turn, and that is the wrong size for *this
one file went wrong*.

| | |
|---|---|
| `/undo <path>` | put one file back as it was before the run |
| `/undo step <n>` | reverse-apply what one step wrote |
| `/undo` | the whole turn — the same thing the rewind chord does |

**One file has four possible answers and they are four different sentences.** It
came back; it was *removed*, because the run had created it; nothing changed
because the previous contents were not kept; nothing changed because this run
never wrote that path. The last two both mean your file is untouched, and they
mean it for different reasons.

**Undoing a step is order-sensitive.** Reverse-applying one step's diff while a
later step's change still sits on the same lines finds context that has moved,
and io-harness leaves the file alone rather than fuzzy-matching it into
something nobody wrote. Undo the newest step first and it applies — and io says
so when it happens, because "nothing changed" without that sentence reads as a
bug.

**A restore does not know about an edit you made afterwards.** The file comes
back from the snapshot taken before the run first wrote it, and that snapshot is
not compared against what is on disk now, so a change you made by hand after the
turn is overwritten. The confirmation says so before you agree to it.

Every restore goes through the same path policy a write does, and a *removal*
asks the policy separately and refuses anything that is not an outright allow.

## Taking the work out

**`/export` writes this conversation as markdown, and `/export trace` writes one
run's canonical trace.** For the review that happens in a pull request, or a text
editor, or a message to somebody who was not there.

| | |
|---|---|
| `/export` | the conversation, as markdown |
| `/export <path>` | the same, where you say |
| `/export trace` | the last run's canonical trace |
| `/export trace <path>` | the same, where you say |

Both are written into the workspace under the session's own path policy, and
**an existing file is refused rather than overwritten** — an export is a
snapshot, and the next one you take is a different snapshot.

**The trace is written exactly as io-harness produced it.** Its whole value is
that it is canonical: io-harness leaves wall-clock stamps, measured durations, an
ephemeral sandbox path and autoincrement ids out of it so that two runs of one
case can be compared. A trace io-cli reformatted would compare against nothing,
so io-cli does not parse it, reserialise it, or pretty-print it. It is
pipe-delimited text rather than JSON, and it takes a `.txt` extension for that
reason.

A turn that never finished is written as a turn that never finished, rather than
as one the agent had nothing to say to.

---

[README](../../README.md) · [All guides](../CAPABILITIES.md) · [What you may depend on](../CONTRACT.md)
