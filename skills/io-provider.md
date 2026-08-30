---
name: io-provider
description: Switch io to another provider or a local model by proposing an edit to the [[provider]] array, whose order is the order a turn tries them.
---

Use this when the operator wants a different model behind the session: "point
this at a local model instead", "use my own OpenAI key", "fall back to something
cheaper when the first one is rate limited", "try Ollama".

## The array is the fallback chain

`[[provider]]` is an array of tables and **its order is the order a turn tries
them**. The first entry is what runs; the ones after it are what a turn falls
back to when an earlier one fails. Reordering is therefore a real change, not
tidying, and adding an entry at the end changes nothing until the first one
fails.

```toml
[[provider]]
kind = "openrouter"
model = "anthropic/claude-sonnet-4"
api_key = "${env:OPENROUTER_API_KEY}"
```

Any OpenAI-shaped endpoint — a proxy, a gateway, a local runtime — is the
`compatible` kind, with **exactly one** of `preset` and `base_url`:

```toml
[[provider]]
kind = "compatible"
base_url = "http://localhost:11434/v1"
model = "llama3.2"
auth = "none"
```

A local runtime is that shape: its own `base_url`, the model name it serves, and
`auth = "none"` because there is no key to send. The endpoint has to be one this
machine can actually reach, and nothing here starts it.

## Credentials

`api_key = "${env:…}"` rather than the key itself, always in a project file and
preferably everywhere. io-harness substitutes `${env:…}`, `${file:…}` and
`${cmd:…}` and nothing else — `${cmd:…}` is refused in the project scope,
because that file travels with a clone, so it belongs in `io.local.toml` or the
user file — and an unset variable is a hard parse error rather than a silent
empty string — so name a variable the operator has, or say which one they need to
export.

## The surface

`/provider` shows the chain as what it is: the order a turn tries them. It adds
entries, removes them, and promotes or demotes one to reorder the chain. It also
offers the presets io-harness reaches through the one `compatible` provider, by
name, with the endpoint each resolves to — worth looking at before writing a
`base_url` by hand.

## What to do

Say which entry you would add, change or move, where in the order it would sit,
and what that means for which provider actually runs. Then either send the
operator to `/provider`, or propose the edit to `io.toml` — an ordinary write,
shown as a diff, gated by the session's policy, and approved by the operator or
not.

Do not say the session has switched. The provider in force is the one the
configuration named when the turn started; a change reaches the next turn, and
only after the operator has approved the write.
