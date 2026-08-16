<!--
Generated from canonical UltraShip state. Do not edit directly.
Run `ultraship views` to regenerate.
-->
# Release history

## io-cli

| Version | Released | Mode | Delivered |
| --- | --- | --- | --- |
| 0.1.0 | 2026-08-16T17:05:00Z | published | A terminal interface over io-harness that renders run events, edits a prompt and reads a keyboard — and contains no agent loop, provider client, tool, sandbox, policy engine or session store of its own. 3,458 lines of source across 16 files, against the archived product's 1,199,453 across 88 crates. That ratio is what this release existed to establish. The renderer is the release. Finished content is committed into the terminal's own scrollback and a fixed few rows at the bottom hold the composer and the status line; a streaming answer commits each line as it finishes, so a two-hundred-line reply leaves the viewport exactly the size it started. The alternate screen is never entered and the mouse is never captured — asserted over the bytes the process writes, so no later release can lose it quietly. A first-run wizard takes a developer from no configuration to a working agent: provider, a credential verified against the live endpoint before anything is written, a model from that provider's own catalogue, a theme previewed live, and a permission posture that is an io_harness::Policy rather than a flag of io-cli's own. Nothing reaches disk before the confirmation screen; the credential never reaches the screen at all, and is not written when the provider's environment variable already carries it. Distribution is four cross-compiled artifacts and a SHA256SUMS on the GitHub Release, installed by one script per platform that verifies before it unpacks and needs no administrator rights. |

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



_Canonical sources: products/<id>/releases/<version>.yaml_
