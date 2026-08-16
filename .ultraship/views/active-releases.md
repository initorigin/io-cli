<!--
Generated from canonical UltraShip state. Do not edit directly.
Run `ultraship views` to regenerate.
-->
# Active releases

## io-cli 0.1.0

**Execution state:** DEVELOPING
**Release fit:** probable
**Target mode:** release-ready
**Outcome:** A developer clones nothing, configures nothing, and runs `io` in a repository. Because no configuration exists, a guided first run starts: it asks which provider to use — the choices being the variants io-harness's own `ProviderSpec` defines, not a hardcoded list — takes an API key without echoing it, and **verifies that key against the live endpoint before continuing**, so a bad key is caught in the wizard rather than on the first real prompt. It then offers the provider's own model catalogue, lets the developer arrow through the shipped themes with a sample transcript re-rendering live behind the picker, and asks for a default permission posture in plain words. It shows exactly what will be written and where, and only then writes io-harness's configuration file with mode `0600`.

The session that follows is the product's core claim made real. Every finished message, tool cell and system line is committed into **the terminal's own scrollback**; a viewport of a few lines at the bottom holds the composer and a status line, and is the only region that repaints. The developer types a task, watches the reply stream token by token without strobing, watches the agent edit a file inside io-harness's sandbox, presses `Ctrl+C` to interrupt a turn that is going wrong, types another, and exits with `Ctrl+D`.

Afterwards the terminal contains the whole conversation. `Cmd+F` finds text in it. tmux copy-mode scrolls it. A mouse drag selects it. None of that was implemented — it works because io-cli never entered the alternate screen and never captured the mouse.



_Canonical sources: products/<id>/execution/active.yaml, products/<id>/releases/<version>.yaml_
