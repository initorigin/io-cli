# From an editor

`io acp` serves the [Agent Client Protocol](https://agentclientprotocol.com) on stdin and stdout,
so an editor that speaks ACP runs io against your own configuration, your own provider chain and
your own bundles — without a terminal.

It is not a command you run. An ACP client spawns `io acp` as a child process and speaks
newline-delimited JSON-RPC 2.0 at it.

## Pointing an editor at it

The client needs the path to the `io` binary and the argument `acp`. In Zed, that is an entry in
`settings.json`:

```json
{
  "agent_servers": {
    "io": {
      "command": "io",
      "args": ["acp"]
    }
  }
}
```

Global flags reach the subcommand, so `["acp", "-C", "/path/to/repo"]` points a session at a
workspace other than the one the editor launched io from, and `["acp", "-m", "<model>"]` picks the
model for that session.

Configuration is read exactly as every other door reads it: `$IO_CONFIG`, else
`$IO_CONFIG_HOME/io.toml`, else `~/.config/io/io.toml`. There is no editor-specific settings file
and no second way to be configured — the adapter adds a protocol, not a second product.

## What the editor sees

| ACP update | What io sends |
| --- | --- |
| `agent_message_chunk` | The answer, streamed as the provider returns it |
| `agent_thought_chunk` | Reasoning, where the provider returns it separately, so your editor can fold it |
| `tool_call` | A tool starting, with its ACP kind — a read, an edit, a search, an execution |
| `tool_call_update` | That call finishing, failing, or being refused by the policy |
| `plan` | The agent's plan changed |

A turn answers with ACP's own `stopReason`. A run that finished is `end_turn`, one you cancelled
is `cancelled`, one the policy refused is `refusal`, and a budget or step ceiling is
`max_turn_requests` or `max_tokens`.

**A run that stopped to ask you something also answers `end_turn`, not `refusal`.** It has
refused nothing — it stopped with work outstanding. `io resume` carries it on from the terminal;
see [When a run stops for you](resume.md).

`cancelled` is reachable only where a cancel arrived before the turn began — see
[Cancelling](#cancelling).

## Permissions

**An editor session asks you about every action the policy puts in the grey tier.** io sends
`session/request_permission` naming the tool call, its act and its target, and your answer decides
the call. Through 0.37.0 it asked nobody and refused them all; that is what 0.38.0 changed.

The request carries three options — allow once, allow for this session, deny — and not ACP's
four. There is no *reject always*, because io-harness records a remembered **approval** and has
nowhere to record a remembered refusal: a later matching action would ask again, and an option
that quietly means something narrower than its name is worse than one that is absent.

*Allow for this session* remembers exactly that act on exactly that target for the rest of the
run — not the act on its own, which would allow every write once you had allowed one. It is the
same rule the terminal's own approval overlay writes, from one shared function, so the two
surfaces cannot come to mean different things by the same word.

**If your client never answers, the action is denied rather than left hanging.** There is no
timer: when the connection ends, every question still outstanding becomes a denial. A timeout
would have been a number invented here — a minute is too short for someone reading a diff, and an
hour is indistinguishable from a hang. An option id io never offered, a cancellation, and a
protocol error are denials too; there is one safe direction to be wrong in.

Where nobody answered, the agent is told the interface could not route the approval, never that
you denied it. Where you were asked and said no, it is told you refused, because that is what
happened.

You can still configure the posture you want rather than being asked at all — see
[Configuration](configuration.md). A `workspace` posture lets the agent write inside the workspace
without an approval; `read-only` refuses writes outright. Nothing is granted that you did not
grant, and nothing happens silently.

## Cancelling

`session/cancel` is served, and in 0.36.0 it takes effect **between** turns rather than during
one: a turn in flight runs to its own end. Stopping a turn mid-flight is what the terminal's
`Ctrl+C` does; see [While it works](the-session.md).

## What is not carried

**Your editor's unsaved buffers.** ACP lets an agent read and write files *through* the client,
which is how it sees edits you have not saved. io does not: io-harness owns the filesystem inside
its own sandbox and publishes no seam to route a read through somebody else. io reads what is on
disk. Save before you ask about a file.

**A terminal the client owns.** Same reason — io runs programs inside its own sandbox, and handing
a spawn to the editor would put the run outside the boundary io exists to show you.

**More than one session per process.** A second `session/new` is refused with a sentence. Run a
second `io acp` for a second conversation.

**Loading an earlier conversation.** `session/load` is not served and `loadSession` is declared
unsupported, so a conforming client will not offer it. Runs are still in the store and `io resume`
reaches them.

## When something looks wrong

Diagnostics go to **stderr**, which most clients surface as a log for the agent process. stdout
carries the protocol and nothing else — a stray byte there is a frame boundary in the wrong place,
and every message after it is read at the wrong offset.

If the session goes silent, check the agent log for the sentence io printed. A missing provider
credential, a configuration file the harness refused, and a workspace io was not pointed at all
report there.
