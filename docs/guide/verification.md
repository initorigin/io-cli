# Verification gates

**An agent that stops is not an agent that is done.** Every release before 0.24.0
took the model's own word for it: the turn ended, the interface said so, and
whether the tests still passed was a question you asked afterwards. A gate is
where you say what "done" means *for this repository*, once, and the turn is not
finished until the criterion passes or the retry budget is spent.

`/gates` writes it and shows what the last turn was judged on. It is a section of
your configuration file like any other, so it can also be typed by hand:

```toml
[app.io-cli.gates]
command = ["cargo", "test", "--all"]
retries = 1
```

There are three kinds and you get exactly one, because a `TaskContract` in
io-harness holds one `Verification` and not a list. Naming none, or naming two, is
refused where you can still see what you typed rather than silently picking a
winner:

- **A command** that must exit a status you name — zero unless you say otherwise.
  It is an argv and never a shell line, because io-harness checks `argv[0]`
  against your permission boundary and runs it without a shell. This is the cheap
  kind: it costs a process, it is objective, and it is the same thing you would
  have run yourself.
- **A file** that must exist, and optionally must contain some text. Nearly free
  and deliberately narrow — it answers "did the change actually get written down",
  which is the failure a passing test suite is worst at catching.
- **A rubric** a second model answers: a sentence saying what the work has to be,
  judged by a reviewer you name.

**io-cli holds no list of test commands.** `/gates` offers you the one the
repository's own toolchain proposes — io-harness detects that from the project, so
a Rust checkout is offered a `cargo` line and a Node one is not — and accepting the
offer writes that command into your file, where you can read it and change it. What
is written is always a concrete argv: `command` is the program and its arguments,
never a shell line and never blank. That detection is the dependency's, and it is
deliberately not reimplemented here: a table of build tools inside this crate would
be a second opinion that goes stale the first time somebody's project does not look
like the ones it was written against.

**The criterion is run by io-harness, in the sandbox, and not by io-cli.** That is
not squeamishness. A criterion run from here could not be handed the cache
directories a real run gets from the detected toolchain, so a `cargo test` gate
would fail on a registry write that io-harness's own gate would have allowed. It
also keeps a rule worth keeping: exactly two modules in io-cli start a process at
all — the one behind `!` and the one that runs the `git` of a marketplace fetch.
`tests/dependencies.rs` names both by path and fails on a third.

The single exception is a `file` criterion with no `contains`. io-harness has no
criterion for bare existence — the nearest one treats a missing file and an empty
needle as a pass — so io-cli answers that one itself, with the reader that tells a
missing file from an empty one. It runs no process to do it, and it is the reason
that criterion costs nothing at all.

**`retries` defaults to 1, and `0` means report-only.** A failing gate sends the
agent back to work with the failure text — the compiler's output, the missing
file, the reviewer's sentence — because the failure *is* the instruction, and an
agent told "it did not pass" without being told what did not pass is being asked
to guess. One retry is the default because a retry is a whole turn against a real
model and not a loop counter; if you want several, say so. Set `retries = 0` and
the verdict is drawn and recorded and nothing is re-driven, which is what you want
in a run you are watching and what you want in a run you are only measuring.

**The criterion runs after every step, not once when the turn ends — and for a
rubric every one of those is a billed completion.** This is the number to know
before you configure anything. io-harness evaluates the contract's criterion at the
bottom of its step loop and keeps going until it passes, so a turn that takes nine
steps runs your command nine times, and a rubric on that turn is nine calls to the
reviewer rather than one. It is not "the agent finishes and then the work is
checked": it is checked continuously, and the run ends the moment the check
succeeds.

That is what makes a command criterion worth choosing carefully. `cargo test --all`
after every step of a long turn is a great deal of compilation, and a narrow
command — one test, one binary, one lint — is usually the right gate. For a rubric
the cost is money rather than time, which is the reason the three kinds are named
in `/gates`'s own one-line description rather than left to the surface. The call is
io-harness's, so it lands in the run's usage like any other and `/cost` counts it.

**A rubric needs a `reviewer`, and it is refused without one.** io-harness answers
a missing reviewer with a configuration error at run start — before the first
billed call, on every turn, in a place on screen nowhere near the keystroke that
caused it — so `/gates` refuses it while you are still looking at what you typed.
The reviewer is also never defaulted to the model doing the work. A model marking
its own paper is a decision rather than a convenience, and it is spelled
`allow_self_review = true`; without it, naming the working model as the judge is
the second refusal.

**What a gate is not is a test runner.** io-cli does not discover your tests, does
not parse their output, and does not decide what a passing suite looks like. It
carries one criterion on the contract and reports the verdict io-harness came
back with.

`io exec` gains exit `6` for this — the agent finished and the work does not hold
up. See [Exit status](#exit-status).

---

[README](../../README.md) · [All guides](../CAPABILITIES.md) · [What you may depend on](../CONTRACT.md)
