# The fleet

An agent can break a task into sub-agents and run them over the same workspace.
io-cli does not implement any of that — io-harness does — but it is the only
terminal interface that can *show* it, because the facts it draws are ones only
that core emits.

It is off until you configure the caps it runs under:

```toml
[app.io-cli.containment]
max_total_agents = 12
max_concurrent_agents = 4
max_depth = 2
max_total_tokens = 200000
```

With that table present your turns run **contained**, and `Ctrl+F` or `/fleet`
opens a live view over the prompt: one row per child with its own state and what
it has drawn, a per-tier count of what is working, waiting and finished, and the
tree's remaining budget on the status line beside everything else. A refused
spawn says which cap refused it and that the agent carries on with what it has. A
report collected from a child lands in the transcript where it arrives.

**From 0.20.0 a child is shown by the name it was spawned under.** io-harness
gives every admitted child an address — the `as` argument the parent chose, or
one derived from the agent it drew, like `reviewer#42` — and that is what the row
carries, with its roster role beside it, instead of a run number nobody picked. A
run id identifies a row in the store; an address is what the parent used to reach
the child, what a message between two of them names, and what you type to attach
to one.

**A message one agent sent a named sibling is drawn in the tree, with its body.**
Children talking to each other is the case a run number told you nothing about:
one addressed line with the text under it, landing where it happened.

**A child that detached can be selected and attached to.** A parent that stops
waiting is not a parent that stops the work — a detached child is still running,
and until now it was a row you could read and could not reach.

**A waiting child is a number and not a row**, because until a concurrency slot
frees it has no run of its own to name. It has no address either, for the same
reason: io-harness names a child when it admits one, so there is nothing to call
a queued child even now that the admitted ones have names. A fleet that is
queueing and a fleet that is stuck look identical without that count, which is
why it is there.

**And from 0.12.0 that is all it costs you.** `Ctrl+C` still ends the turn, at the
next point where no child is in flight, and the interface tells you that is what
it is waiting for rather than appearing to have missed the key. `/contain off`
gives the next turn back; `/contain on` takes it again.

Through 0.11.0 this switch carried more than the fan-out. io-harness's contained
entry point was then the only session entry point that took a task contract, so
turning containment on was also how you got skills, MCP servers, language servers,
a browser, an answer to the agent's questions and a plan gate — and turning it off
took all of them away. 0.11.0 gave the ordinary turn a contract too. Every one of
those capabilities is on every turn now, and containment means what its name says.

## Planning

`/plan on` makes the next turn propose a plan before it does anything. While the
planning phase is on, io-harness denies every write and every command until you
approve, so reading a proposal costs nothing and cancelling is not an undo —
there is nothing to undo yet. `Enter` on an empty prompt approves, typing a
correction sends it back, `Esc` cancels. The status line says `planning` for as
long as the phase is on, because it outlives the turn you set it on.

`/plan off` gives you back a turn that starts working immediately, and that is the
default. Bare `/plan` says which one you are in and changes nothing.

**This moved in 0.12.0.** Through 0.11.0 the plan gate rode
`[app.io-cli.containment]`, so configuring a fan-out silently made every turn stop
and propose first. If that is what you wanted, `/plan on` is where it lives now.

---

[README](../../README.md) · [All guides](../CAPABILITIES.md) · [What you may depend on](../CONTRACT.md)
