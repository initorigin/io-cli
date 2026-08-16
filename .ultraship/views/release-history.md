<!--
Generated from canonical UltraShip state. Do not edit directly.
Run `ultraship views` to regenerate.
-->
# Release history

## io-cli

| Version | Released | Mode | Delivered |
| --- | --- | --- | --- |
| 0.1.1 | 2026-08-16T18:47:29Z | published | The session stops looking frozen while it works. A turn in flight advances its own clock and turns its own indicator, and neither waits for an event to arrive. The screenshot this release put in the README is the proof and the case: forty seconds into a slow first token, no answer on screen yet, the status line reading `deepseek/deepseek-v4-flash-0731 · ⠨ working · 40s` with the indicator turning. Under 0.1.0 that same moment was a still word beside a still clock, which is what a hung process looks like. The mechanism is one function. `App::tick(age) -> bool` takes the session's age as an argument rather than reading a clock, and answers whether a frame is owed rather than drawing one. That is what makes liveness assertable without a timing assertion: a test advances the clock by hand and asks. It also decides the half that matters more — an idle session is told no, so this renderer still repaints only when something changed, which is the whole of its differentiation from the alternate-screen products. A step now reads as a step: the decision, the tool it called with its target, the result, and then the token count and the step number as trailing muted detail. The result is stated in both directions, `changed files` or `no change`, so a transcript is skimmed down one column instead of parsed. Nothing else moved. No key, no command, no event surface, no setting this release starts reading, and the eight dependencies are the same eight. A spinner is a handful of characters and a modulo. |
| 0.1.0 | 2026-08-16T17:35:00Z | published | A terminal interface over io-harness that renders run events, edits a prompt and reads a keyboard — and contains no agent loop, provider client, tool, sandbox, policy engine or session store of its own. 3,458 lines of source across 16 files, against the archived product's 1,199,453 across 88 crates. That ratio is what this release existed to establish. The renderer is the release. Finished content is committed into the terminal's own scrollback and a fixed few rows at the bottom hold the composer and the status line; a streaming answer commits each line as it finishes, so a two-hundred-line reply leaves the viewport exactly the size it started. The alternate screen is never entered and the mouse is never captured — asserted over the bytes the process writes, so no later release can lose it quietly. A first-run wizard takes a developer from no configuration to a working agent: provider, a credential verified against the live endpoint before anything is written, a model from that provider's own catalogue, a theme previewed live, and a permission posture that is an io_harness::Policy rather than a flag of io-cli's own. Nothing reaches disk before the confirmation screen; the credential never reaches the screen at all, and is not written when the provider's environment variable already carries it. Distribution is four cross-compiled artifacts and a SHA256SUMS on the GitHub Release, installed by one script per platform that verifies before it unpacks and needs no administrator rights. |

### 0.1.1 known limitations

- **An idle session's clock is still frozen, and that is F2 rather than a defect.** Between turns the elapsed time advances only when a key arrives. Making it tick when nothing is happening is exactly the repaint-forever behaviour this renderer exists not to have, so the frozen idle clock is the price of the property and is deliberate.

- **The tick's cost while text is streaming quickly was not measured.** The contract raised it as an open question to record rather than to gate, and it was recorded rather than measured: token events already drive repaints, so a timer on top of them is redundant while text is arriving and useful only while it is not. The interval is 100 ms, chosen for how motion reads rather than from a measurement. If a later release measures it, the number belongs there.

- **A long tool target in a step line wraps rather than being fitted.** The picker has a fitting rule that could be reused; the contract left the decision to the first rendering against real content, and the real content in the F3 run had no long target in it. Left to wrap, deliberately, rather than designing against a case not yet seen.

- **The indicator is braille and there is no ASCII fallback.** Every frame is one cell wide, which is why braille was chosen, but a terminal without the glyphs shows a replacement character where the animation should be. The word beside it is unaffected, and `NO_COLOR` removes the animation entirely. A proper plain mode is 0.6.0.

- **The README screenshot shows the liveness fix and not the step fix.** The turn it captures was in its first step, so no step line had committed yet. The picture proves the clock and the indicator, which is what the release is named for; F5 is proved by tests asserting the rendered order rather than by the image.

- **Two of 0.1.0's four interface limitations are untouched and stay where the roadmap put them.** The model picker still has no type-to-filter (0.7.0, decided with the owner at 0.1.0's release and widened there to every picker), and a prompt longer than two rows still scrolls within them rather than growing the viewport (0.7.0, because ratatui fixes an inline viewport's height at construction). This release fixed the other two.

- **An action that needs approval is still declined, and the configuration's `[sandbox]`, `[run]` and `[instructions]` sections are still not applied to a turn.** Both are 0.1.0 limitations carried forward unchanged; the approval overlay is 0.2.0 and the contract sections wait on a harness entry point that takes both a caller's contract and a steer inbox.

- **This record is sealed before the Release exists, which is the flow rather than a shortfall.** The sealed record and its lock pin ride the feature branch and merge with the code, so the seal necessarily precedes the tag that cuts the Release and the workflow that attaches the four artifacts and `SHA256SUMS`. `mode: published` describes the channel this product has. If the tag or the matrix fails after this point, the correction is a new version, not an edit.

- **The `fmt and clippy` job failed once on this branch and was re-run.** The failure was seven seconds into `dtolnay/rust-toolchain`, before any of this repository's own commands ran, on a SHA that had just gone green on `develop`. Recorded rather than smoothed over: a failure inside a toolchain download is a flake, and the same two commands exit 0 locally and in the re-run.


### 0.1.0 known limitations

- **The configuration's `[sandbox]`, `[run]` and `[instructions]` sections are read by io-harness and not applied to an io-cli turn.** `Session::turn_steered` builds its own `TaskContract`, and the harness has no entry point taking both a caller's contract and a steer inbox — so honouring them would mean giving up `Ctrl+C`, which F6 requires. The sandbox itself is on; what is missing is the configured ceilings, not the containment. Nothing in this contract promised those sections. Stated in the README and in `docs/config.example.toml`, and it is the natural first item for a harness release.

- **An action that needs approval is declined, and says so.** The overlay that asks a human, and the refusal surface that names the rule and the policy layer, are 0.2.0. A live run walked into the consequence: the ask-before-writes posture denied three actions, the agent asked for permission, and the turn ended `awaiting_answer`. That outcome now reads as a warning carrying the way out rather than as an unexplained error, but the posture still cannot do what its name suggests until 0.2.0.

- **The status line's working indicator is a word and not a spinner, and its elapsed time advances only when an event or a keystroke arrives.** Between events the clock is still. The owner raised both; neither blocks a session and both are remediation of what this release shipped, so they belong in a patch.

- **The model picker has no filter.** A provider catalogue of four hundred rows is walked with the arrow keys. The wizard's viewport was widened so a usable number of rows is visible, but type-to-filter is new capability and the roadmap puts fuzzy filtering in 0.7.0. Decided with the owner rather than added quietly.

- **A prompt longer than two rows scrolls within them rather than growing the session viewport.** ratatui fixes an inline viewport's height when the terminal is constructed, and rebuilding mid-session would re-query the cursor and risk shifting the scrollback this product exists to protect. The wizard gets its own taller viewport because the boundary between the two phases is safe to rebuild at; a session has no such boundary. The composer's own release is 0.7.0.

- **O5 is verified after this record is sealed.** The installers can only be exercised on clean machines against a Release that exists, which is by definition later than the tag that creates it. `install.sh` is proved end to end against a local release, including the corrupted-artifact refusal, and the cross-compile matrix is proved on a throwaway branch — what remains is the clean-machine, new-shell half. What this record claims is the state at seal time.

- **The `x86_64-apple-darwin` artifact is cross-compiled and not executed by CI**, because the runners are arm64. The other three rows run `io --version` on the machine that built them.

- **The F1 recording shows the theme step before its fix.** The picker was invisible in that run — the defect the run found — and the recording documents it. It is honest evidence of the criterion passing and a poor screenshot; a fresh capture for the README is a patch-release chore, not a correction.

- **This record was sealed before the Release existed, and that is the flow rather than a shortfall.** The sealed record and its lock pin ride the feature branch and merge with the code, so the seal necessarily precedes the tag that cuts the Release and the workflow that attaches the four artifacts and `SHA256SUMS`. `mode: published` describes the channel this product has — the GitHub Release, which is the whole of it, since there is no registry and never will be. If the tag or the matrix failed after this point, the correction is a new version, not an edit.

- **This record was amended and re-pinned after its first seal, before the merge into `main`, because the tree it described would not start.** Giving the wizard its own taller viewport means attaching the terminal twice, and the keyboard reader had been moved ahead of the first attach — placing an inline viewport asks the terminal for its cursor position and reads the answer off stdin, so the reader consumed it and the query timed out. The owner found it by running the binary; nothing in the suite reaches the driver's own startup ordering. The reader now polls so it can be stopped and joined, starts only after a screen is attached, and takes that screen as a witness argument so the ordering is a compile error to get wrong. Every gate below was re-measured on the corrected tree at 9349aa0.



_Canonical sources: products/<id>/releases/<version>.yaml_
