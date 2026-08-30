# Hooks

**`[[hook]]` tables run from 0.20.0.** They were parsed before this release and
then installed on nothing, so a file asking for every event to be written to
`audit.jsonl` produced an empty file and no error. They now run in a session turn
and in `io exec` alike, from the same call that builds everything else.

A hook either writes events down or runs a program:

```toml
[[hook]]
on = []                       # the events to observe; empty means every one
append = "audit.jsonl"        # one JSON line per event, appended

[[hook]]
at = "before_tool"            # the only `at` there is
tools = ["shell"]             # which calls this one sees
run = ["./scripts/gate.sh"]   # argv, never a shell string
on_failure = "refuse"
timeout_ms = 5000             # the default
```

`on` and `at` are mutually exclusive, because the first is an observer of events
and the second is a gate in front of a tool call. An `at` hook must have a `run`.
Exactly one of `append` and `run`: a hook that did both would be a hook whose
failure meant two things.

**`on_failure` is where a hook's power actually is.** `continue` lets the turn go
on, which is what an audit hook wants. `cancel` ends the turn at the next step
boundary and **the run stays resumable** — it is a stop, not a crash. `refuse`
turns that single tool call back and leaves the turn running, which makes a
`before_tool` hook a rule of your own standing beside the policy engine's.

**`run` is an argv array and never a shell string.** Nothing is word-split and
nothing is expanded: the program you named is the program that runs, with the
arguments you wrote.

**A `run` hook runs on the turn's own critical path, and `timeout_ms` is the only
thing bounding it.** An observer is called synchronously by the run, and the run
shares a task with the loop that reads your keyboard — so while a hook's program
is running, the interface is not repainting and not answering keys. A script that
takes a tenth of a second, on a hook matching every event, costs that tenth of a
second per event. Keep a `run` hook fast, match it to the events you actually
want with `on`, and lower `timeout_ms` from its five-second default if the program
can hang. An `append` hook has none of this cost: it is a line written to a file.

**A hook that fails is quiet.** io-harness reports a failed hook through a log
this binary installs no subscriber for, so an `append` path that cannot be written
and a `run` program that does not exist both leave the session looking normal —
and a hook with `on_failure = "cancel"` ends the turn without saying which hook
did it. Verify a new hook by checking that it did something: read the file, or
give the program a visible side effect. This is a real limitation of 0.20.0 rather
than a subtlety.

**A project-scoped file may not declare `[[hook]]` at all.** io-harness refuses
the whole configuration rather than dropping the table — a hook runs a command on
this machine and `io.toml` is the file a `git clone` delivers. There is no
`Config` to be had, so `io` genuinely cannot start, and 0.20.0 does not soften
that. What it changes is the words: io-harness's own sentence, which names the
key, the reason and the two files that may carry it, under a line saying which
file was being read. Before this it arrived as a bare error string from a program
that had already exited, against a repository you had just cloned. Write the
table in `io.local.toml` or in your user file.

---

[README](../../README.md) · [All guides](../CAPABILITIES.md) · [What you may depend on](../CONTRACT.md)
