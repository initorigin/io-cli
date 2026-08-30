# Which model a run asks

A gate says what "done" means. This says what to do when the model that has to
reach it keeps missing, and what to do when the work turns out not to have needed
the model you started with. `[app.io-cli.routing]` holds two optional rules, each
written as a sub-table because a rule is a threshold and a model that only mean
anything together — a threshold with no model and a model with no threshold are
both half a rule, and a sub-table makes the pair the unit the file itself
enforces:

```toml
[app.io-cli.routing.escalate_after]
failures = 3
model = "a-stronger-model"

[app.io-cli.routing.downshift_under]
bytes = 2000
model = "a-cheaper-model"
```

**`escalate_after` moves up after that many consecutive failed gate attempts** —
the gates above, counted consecutively rather than cumulatively, because a run
that fails, recovers, and fails again much later is a run doing hard work rather
than one that needs a bigger model. **`downshift_under` asks the cheaper model
while the run has written fewer than that many bytes to disk**, measured on what
was actually written rather than on what was planned, so it is a fact about the
run rather than a forecast of it. Neither key defaults: half a rule is a parse
error naming the key you left out, rather than a threshold quietly reading as
zero and escalating on the first gate attempt of every turn.

Escalation happens **once** and does not come back down, and escalation **wins**
over downshifting where both apply. Both of those are io-harness's rules rather
than io-cli's: it owns the consecutive-failure count, the byte total and the
decision, taken after every step of the run. io-cli evaluates none of it, because
a second implementation here would be a second answer that drifts from the one the
run actually used.

**Routing does not reach a contained turn, and that is the first thing to know
about it.** io-harness applies routing in its flat workspace loop only; a turn run
under `[app.io-cli.containment]` takes each agent's model from that agent's own
roster entry and never consults the rules. So for an operator who has configured
containment, the section parses, is listed by `/config`, reaches the contract —
and never fires. A session that has both is told so at three moments: at start, when `/config`
is opened on the keys themselves, and when `/contain on` is typed — which is the
one that matters most, because that operator began uncontained, was told nothing
because nothing applied, and has just moved into the mode where their rules do
not fire. A session with containment off is told nothing, because a caveat
attached to a feature that is working is how an operator learns to stop reading
the notices. A turn taken with
`/contain off` routes normally, and `io exec` uses the flat loop, so routing works
there. Nothing in io-cli can close this: the loop that would have to consult the
rules is the dependency's, and what this interface owes you meanwhile is the
disclosure.

**There is no `require_primary` key**, and its absence is a decision rather than
an omission. io-harness's own `Routing` carries the field, and it gates on
`Provider::reachable` — a defaulted trait method whose body answers yes, and which
no provider in io-harness 0.71.0 overrides. A key for it would be offered on a
surface, accepted from a file, and permanently inert: you would set it, believe an
unattended overnight run now refuses to start against a dead endpoint, and get
exactly the behaviour you had before. It goes in when a provider answers the
question.

---

[README](../../README.md) · [All guides](../CAPABILITIES.md) · [What you may depend on](../CONTRACT.md)
