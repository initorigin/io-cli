//! Answering a run that paused, instead of abandoning it.
//!
//! Since 0.10.0 this interface has been able to ask the operator a question and
//! to propose a plan, and since 0.20.0 a tool call can be interrupted in a way
//! io-harness records but cannot judge. In every one of those cases the run
//! stops, the pending row is written to the store, and through 0.22.0 io-cli
//! walked away from it: [`crate::exec`] printed that carrying on was "not in
//! this release" and exited 4, and `/resume` — one line, `Session::reopen` —
//! swapped the session handle without ever asking what the last run was waiting
//! on.
//!
//! Everything needed to do better has been public in io-harness the whole time.
//! This module reads the pending rows back, drives the matching resume entry
//! point, and then does the session bookkeeping that entry point does not.
//!
//! # The four kinds, which look alike and are not
//!
//! A question, a plan, an interrupted tool call and a run whose process merely
//! died each have their own reader, their own resume function and their own
//! flat-versus-tree twin. They are kept apart on purpose: collapsing them into
//! one generic switch over an integer is how an operator's answer is delivered
//! into somebody else's run, so each pending kind is its own type carrying its
//! own id and no bare integer crosses a surface.
//!
//! # What a free resume does not do
//!
//! `Session::drive` is the only thing in io-harness that stitches a run to a
//! turn, and it is private. The free resume functions know nothing about turns,
//! so after one of them the `session_turns` row still reads `awaiting_answer`
//! with an empty reply and the session head has not moved. `Store::turn_for_run`,
//! `Store::finish_turn` and `Store::set_session_head_if` are all public, so this
//! module closes the turn itself — with the **compare-and-swap** head write and
//! never the unconditional one, which is the defect [`crate::rewind`] carried
//! until this release.
//!
//! The reply is taken off the last assistant turn in the store rather than
//! reconstructed. io-harness's own extraction is a private function splitting on
//! a `pub(crate)` sentinel, so reproducing it would mean hardcoding a literal
//! this crate cannot see change.
//!
//! # The one pause that cannot be answered
//!
//! A turn the operator interrupted is finished, not paused. `Ctrl+C` sets the
//! flag that returns `Flow::Cancel`, io-harness records the outcome `cancelled`,
//! `finish_run` maps that to a *completed* status, and every resume entry point
//! short-circuits on a completed run and returns the original outcome without
//! driving. So the most common way an io-cli turn stops is the one way it cannot
//! be continued. This module reports such a run as ended by the operator and
//! points at `/fork` from the turn before it, which is the honest neighbouring
//! answer.
//!
//! The published io-harness documentation disagrees — `Steer::interrupt` says
//! such a turn "stays resumable" — and it is wrong, contradicted by the run loop
//! in the same crate. Reported upstream; not worked around here.
