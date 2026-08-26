---
name: io-update
description: Find out whether a newer io has been released and propose the exact install.sh command for the operator to approve, never running an update behind them.
---

Use this when the operator asks about the version: "update io", "am I on the
latest?", "is there a newer version", "how do I upgrade this".

## What is running now

`io --version` says it, and so does the splash at the top of the session. Use
what the session already told you before proposing a command that finds out.

## What has been published

The newest release is whatever
`https://github.com/initorigin/io-cli/releases/latest` redirects to — the tag in
that redirect's URL is the version, which is why it can be read without a token
and without parsing JSON.

Reaching it is a network act and `net` is `deny` by default in
`[policy.defaults]`, so it may be refused. **A refusal is an answer, not an
obstacle**: say the check could not be made from inside this session and give the
operator the release page to look at, or the `io-permissions` skill if they want
to widen the rule. Do not work around a refusal.

## The command to propose

`install.sh` in this repository is the installer, and it is the only installer.
It resolves the target for this machine, downloads the artifact and the
`SHA256SUMS` beside it, **verifies the checksum before unpacking**, and moves the
binary into a directory the operator already owns — no `sudo`, nothing written
outside their own directories, nothing left behind if any step fails.

```sh
curl -fsSL https://raw.githubusercontent.com/initorigin/io-cli/main/install.sh | sh
```

Three environment variables change what it does, and nothing else does:

- `IO_VERSION` — install this version instead of the latest, e.g. `IO_VERSION=0.18.0`.
- `IO_INSTALL_DIR` — install here instead of `~/.local/bin`.
- `IO_BASE_URL` — download from here instead of the GitHub Release.

So a pinned install into a chosen directory is one line:

```sh
IO_VERSION=0.19.0 IO_INSTALL_DIR="$HOME/bin" sh -c 'curl -fsSL https://raw.githubusercontent.com/initorigin/io-cli/main/install.sh | sh'
```

**Do not re-implement any of that.** Downloading the tarball yourself, unpacking
it, computing a checksum by hand or moving a binary into place is a second
installer with none of the guarantees of the first, written in a session nobody
will read back. Propose the script.

## What to do

Say what is running, say what has been published if you could find out, and then
propose the command. Running it is a gated command: the operator sees the exact
argv before anything runs and approves it or does not.

Then stop. **Do not say io has been updated.** The binary in flight is the one
this process started with; a new one takes effect when the operator starts a new
session, and until they have approved the command and seen it finish, nothing has
changed at all.
