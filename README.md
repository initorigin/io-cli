# IO CLI

A terminal agent that shows you what it is allowed to do, what it is spending,
and what it refused — while it works.

`io` is an interface. The agent loop, the providers, the tools, the sandbox, the
permission boundary and the session store are all
[io-harness](https://github.com/initorigin/io-harness), and none of them are
reimplemented here.

## It never takes your terminal

`io` does not enter the alternate screen and does not capture the mouse, in any
mode, behind any flag. Every finished message, tool call and system line is
committed into the terminal's own scrollback; only a few lines at the bottom
hold the composer and the status line, and only those repaint.

So when the session ends the whole conversation is still there. Your terminal's
search finds it, tmux copy-mode scrolls it, and a mouse drag selects it — none of
which is implemented here. It works because it was never taken away.

## Install

See the release notes. Distribution is prebuilt binaries on the GitHub Release,
installed by one script per platform; there is no package registry and no
`cargo install`.

## Licence

Apache-2.0. Copyright 2026 Aakash Pawar (InitOrigin). See `LICENSE` and `NOTICE`.
