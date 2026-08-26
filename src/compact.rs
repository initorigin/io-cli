//! `/compact` — asking io-harness to fold the conversation, and reporting the
//! fold rather than the request.
//!
//! **The whole module exists for one sentence in io-harness's own documentation**,
//! on [`Steer::fold`](io_harness::Steer::fold): an interface "must not report a
//! fold on the strength of having sent one — read the `Compacted` event instead."
//! Everything below is that sentence made structural. A request is one thing
//! ([`Said::asked`]); a fold is another ([`Said::folded`]), and it can only be
//! built out of an [`EventKind::Compacted`] that actually arrived. There is no
//! path from the first to the second, which is what stops the obvious bug: an
//! interface that says *folded* the moment `Steer::fold()` returns `Ok`, and is
//! then wrong on every conversation too short to have anything to fold.
//!
//! **Two triggers, one word.** io-harness reads a fold request in two places and
//! io-cli reaches both from the same `/compact`:
//!
//! - Mid-turn, `Steer::fold()` lands at the turn's **next step boundary** — a tool
//!   call in flight is not a safe place to change the conversation out from under
//!   the model.
//! - At an idle prompt there is no turn to steer, so the request rides the next
//!   turn's contract as `TaskContract::fold_now`, which io-harness reads once at
//!   that turn's **first step**, before it assembles its first request, and
//!   consumes with `std::mem::take`. io-cli builds a fresh contract for every
//!   turn, so the flag is naturally per-turn and nothing has to remember to clear
//!   it.
//!
//! **Four documented ways a request that was accepted still folds nothing, and
//! this module can predict exactly two of them.** The split is the reason
//! [`Said`] has the variants it has:
//!
//! | condition | predicted, or observed |
//! |---|---|
//! | it is not immediate — the next boundary, not now | **predicted**: a property of the mechanism, so [`Said::Sent`] and [`Said::Armed`] say when rather than whether |
//! | it does not override an off setting | **predicted**, from the contract's own [`Compaction::enabled`] — nothing is sent at all, and [`Said::Off`] is said before anything leaves this process |
//! | it loses to an interrupt sent before the same boundary | **observed**: nothing here knows what the operator will press next. [`Said::Unfolded`] carries whether the turn was stopped, which is the one thing the driver does know afterwards |
//! | it does nothing when there is nothing to fold | **observed, and only by absence**: a conversation shorter than [`Compaction::keep_recent`] has no prefix a paragraph could stand in for, and the only evidence is that no `Compacted` event ever arrived. **The request is spent either way**, which the sentence says, because an operator who is not told that will wait for a fold that is never coming |
//!
//! A fifth condition is io-harness's and not this interface's problem: a fold does
//! not reach a spawned child, because a child's ledger is its own work with no
//! conversation seeded into it. Nothing here claims otherwise — the sentences talk
//! about *the turn*, never about the fleet.
//!
//! **What the summary replaced is recoverable, and it is the useful half of the
//! report.** `EventKind::Compacted` carries a step and two token figures and no
//! text; `Store::summaries` carries the paragraph and, in
//! [`Summary::folded`](io_harness::Summary::folded), how many observations from
//! the front of the ledger it stands in for. A fold deletes nothing from the
//! store, so those observations are still there to be read — which is why the
//! sentence can say what was replaced without any of it having been lost.

use io_harness::{Compaction, EventKind, RunEvent, Store};

use crate::status::format_tokens;

/// Everything `/compact` puts on the screen, and the one place a fold is claimed.
///
/// **One type for the request and for the outcome, deliberately**, because the
/// bug this module exists to prevent is exactly a request wearing an outcome's
/// words. With both in one enum the difference is a variant rather than a turn of
/// phrase, [`Said::is_fold`] is the whole of the honesty rule in one line, and
/// `tests/compact.rs` can assert it without matching on prose.
///
/// [`Said::Folded`] is the only variant that says a fold happened, and
/// [`Said::folded`] is the only constructor for it — it takes a [`RunEvent`] and
/// returns nothing at all unless that event is an [`EventKind::Compacted`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Said {
    /// Folding is off for this session, so nothing was sent.
    ///
    /// The one condition that is known *before* the request exists, and the
    /// reason it is worth knowing: `Compaction { at_share: 1.0, .. }` never folds
    /// and no trigger reverses that, so a request sent under it would be spent for
    /// nothing and reported as pending forever.
    Off,
    /// The request went to the turn that is running. It folds at the next step
    /// boundary.
    Sent,
    /// There is no turn to steer, so the next turn's contract carries it and folds
    /// at that turn's first step.
    Armed,
    /// `EventKind::Compacted` arrived: a fold happened. **The only variant that
    /// says so.**
    Folded {
        /// The step whose assembly folded, from the event.
        through_step: u32,
        /// Estimated tokens the observation section held before the fold.
        before: u64,
        /// What it holds after it. A fold that bought nothing is visible here as
        /// two numbers that barely differ, which is a truth worth showing rather
        /// than rounding away.
        after: u64,
        /// How many observations from the front of the ledger the summary stands
        /// in for, from the run's own `summaries` row — `None` where the row is
        /// not readable, in which case the fold is still reported and the count
        /// simply is not claimed.
        replaced: Option<u32>,
    },
    /// The turn ended and no `Compacted` event ever came.
    ///
    /// Not a failure and not an error: it is the honest report of the two
    /// conditions nothing here can predict. `interrupted` separates them as far as
    /// they can be separated — a turn the operator stopped lost the fold to the
    /// stop, and a turn that ran to the end simply had nothing to fold.
    Unfolded {
        /// Whether the operator stopped this turn.
        interrupted: bool,
        /// How many of the newest observations a fold would have kept whole —
        /// [`Compaction::keep`], floored at one, rather than the raw
        /// `keep_recent`, because that floor is what io-harness actually applies.
        keep_recent: usize,
    },
}

impl Said {
    /// What `/compact` does with the request, decided before anything is sent.
    ///
    /// `running` is whether a turn is in flight, which is the only thing that
    /// chooses between the two triggers — and it is passed in rather than looked
    /// up because this module has no session to ask.
    ///
    /// The off check comes first and is the reason this function takes a
    /// [`Compaction`] at all. It is read off the contract that is about to run
    /// (or that just ran), never recomposed from the configuration, for the reason
    /// the status line reads its budgets off the contract: the setting in force is
    /// the one the turn carries, and a second answer assembled here would be a
    /// second opinion about a value io-cli does not own.
    pub fn asked(compaction: Compaction, running: bool) -> Self {
        if !compaction.enabled() {
            Said::Off
        } else if running {
            Said::Sent
        } else {
            Said::Armed
        }
    }

    /// The fold, read out of the event that announced it and the row that holds
    /// the paragraph. `None` for every other kind of event.
    ///
    /// **This is the only way a [`Said::Folded`] is made**, and it is why the
    /// driver's arm can be one call: an interface that wanted to report a fold
    /// early would have to fabricate an event, which is a thing somebody would
    /// notice writing.
    ///
    /// The summary row is looked up by the event's own `through_step`, which is
    /// the field io-harness documents the two as agreeing on, and the *newest*
    /// match wins because a run whose fold was corrected reads the correction. A
    /// missing row is not a missing fold: the event is the fact, the row is the
    /// detail, and losing the second is a sentence with one fewer number in it.
    pub fn folded(store: &Store, event: &RunEvent) -> Option<Self> {
        let EventKind::Compacted {
            through_step,
            before_tokens,
            after_tokens,
        } = &event.kind
        else {
            return None;
        };
        let replaced = store
            .summaries(event.run_id)
            .ok()
            .and_then(|rows| {
                rows.into_iter()
                    .rev()
                    .find(|row| row.through_step == *through_step)
            })
            .map(|row| row.folded);
        Some(Said::Folded {
            through_step: *through_step,
            before: *before_tokens,
            after: *after_tokens,
            replaced,
        })
    }

    /// What to say when the turn is over and the event never arrived.
    ///
    /// Takes the [`Compaction`] rather than a number so the sentence names
    /// [`Compaction::keep`] — the floored value io-harness actually keeps — rather
    /// than a `keep_recent` of zero that would read as "a fold keeps nothing".
    pub fn unfolded(compaction: Compaction, interrupted: bool) -> Self {
        Said::Unfolded {
            interrupted,
            keep_recent: compaction.keep(),
        }
    }

    /// Whether this line tells the operator that a fold **happened**.
    ///
    /// True for exactly one variant. This is the honesty rule as a predicate, and
    /// `tests/compact.rs` reads it on every arm — including the two where a
    /// request was accepted and nothing folded, which is where the sabotage lands.
    pub fn is_fold(&self) -> bool {
        matches!(self, Said::Folded { .. })
    }

    /// The line itself.
    ///
    /// `dash` is the session's own separator glyph, passed in the way every other
    /// sentence in the driver takes it — a module that reached for the Unicode em
    /// dash directly would be the one surface that ignores `--plain` and
    /// `NO_COLOR`'s ASCII glyph set.
    pub fn line(&self, dash: &str) -> String {
        match self {
            // Says *nothing was sent*, and says it before anything is: this is the
            // one condition worth knowing in advance, and the value of knowing it
            // is that the operator is not left waiting on a request that could
            // never have been honoured.
            Said::Off => format!(
                "folding is off for this session {dash} nothing was sent, and nothing would fold"
            ),
            // *When*, never *whether*. The step boundary is a property of the
            // mechanism and safe to promise; the fold is not, and is not mentioned
            // in the past tense anywhere on this arm.
            Said::Sent => format!(
                "asked {dash} the turn folds at its next step, and this says so when it does"
            ),
            Said::Armed => format!(
                "asked {dash} the next turn folds at its first step, and this says so when it does"
            ),
            Said::Folded {
                through_step,
                before,
                after,
                replaced,
            } => {
                let cost = format!(
                    "{} tokens to {}",
                    format_tokens(*before),
                    format_tokens(*after)
                );
                match replaced {
                    // What the paragraph stands in for, which is the half of the
                    // report the event alone cannot give. Nothing was deleted to
                    // make it: the observations are still in the run's store.
                    Some(count) => format!(
                        "folded {dash} {count} observations through step {through_step} are now \
                         one paragraph; {cost}"
                    ),
                    None => format!("folded {dash} through step {through_step}; {cost}"),
                }
            }
            // Neither of these claims to know which of the two unpredictable
            // conditions happened beyond what the driver actually observed, and
            // both say the request is gone — because an operator who thinks it is
            // still queued will wait instead of asking again.
            Said::Unfolded {
                interrupted: true, ..
            } => format!("stopped before the fold {dash} the request went with the turn"),
            Said::Unfolded {
                interrupted: false,
                keep_recent,
            } => format!(
                "nothing to fold {dash} no fold was reported; the newest {keep_recent} \
                 observations are kept whole, and the request is spent"
            ),
        }
    }
}
