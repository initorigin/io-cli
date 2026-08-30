---
name: io-mcp
description: Add, change or remove an MCP server for io by proposing an edit to the [[mcp]] array in io.toml, in the scope that suits it.
---

Use this when the operator wants a tool server: "add the GitHub MCP server",
"point io at our internal docs server", "drop the one that keeps failing",
"why is the issues server not answering".

## What one looks like

`[[mcp]]` is an array of tables. A stdio server is a program io launches and
talks to over a pipe; an HTTP server is a URL it dials.

```toml
[[mcp]]
id = "docs"
transport = "stdio"
command = "mcp-server-docs"
args = []

[[mcp]]
id = "issues"
transport = "http"
url = "https://mcp.example.com/v1"
```

`id` is how everything else refers to the server, so it has to be unique. There
is a second array, `[[app.io-cli.mcp]]`, which is merged with the top-level one
and wins a collision of ids — use it only when the operator wants a server for
the terminal session and not for `io exec`, and say so if you do.

## Which file

The user file `~/.io-cli/io.toml` for a server that belongs to this machine; the
project's `io.toml` for one the whole team should have. **A credential never goes
into a committed file**: write `${env:GITHUB_TOKEN}` and let the environment
carry the secret. io-harness substitutes `${env:…}`, `${file:…}` and `${cmd:…}`
and nothing else; `${cmd:…}` is refused in the project scope, because that file
travels with a clone, so it belongs in `io.local.toml` or the user file.

## The surface

`/mcp` shows what is configured, which servers this session actually reached, how
many distinct tools each answered with, and the last failure. A server the
session has not tried yet says so rather than showing as broken — worth checking
before proposing to remove one. The same surface adds, edits and removes entries,
so the operator may prefer to be sent there.

A server's tools reach a turn through io-harness. Its network traffic is the
server's own and is not what the `net` rule in `[policy.defaults]` governs, so do
not tell the operator that a policy change will contain it.

## What to do

Name the server, the transport, the id, and the file you would put it in. Then
either send the operator to `/mcp`, or propose the edit — writing `io.toml` is an
ordinary write, shown as a diff and gated by the session's policy, and it is the
operator who approves it.

The server is not configured because you described it. Say what you are
proposing; do not say it is connected, and do not say it works until `/mcp` shows
it answering.
