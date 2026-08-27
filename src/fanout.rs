//! One `&dyn Observer` for a turn that has more than one watcher.
//!
//! io-harness takes exactly one observer per run, and 0.69.0 ships nothing that
//! combines two: the whole crate declares three implementations — `Hooks`,
//! `Ignore`, and `Broadcast`, which is a store-writing decorator over one inner
//! observer, not a tee. Until 0.20.0 that was enough, because io-cli had one
//! watcher: [`Bridge`](crate::bridge::Bridge), feeding the screen. Now the
//! operator's `[[hook]]` tables run as well, and those are driven by io-harness's
//! own `Hooks`, which is itself an `Observer`. Two observers, one slot. **The
//! combinator has to live here, because the crate that owns the trait does not
//! provide one** — not because io-cli wants its own abstraction over observing.
//!
//! # Cancel wins, whatever the order
//!
//! [`Flow`] folds one way: any `Cancel` makes the whole fan-out `Cancel`. A hook
//! that asked to stop the turn must not be overruled by whichever observer
//! happened to be registered ahead of it, and a watcher that returns `Continue`
//! — which is what a watcher always returns — must never be able to veto a stop.
//! Registration order is a call-site detail; whether the turn ends is not. So the
//! fold is a logical OR over `Cancel`, which is commutative, and the order the
//! observers were passed in cannot change the answer.
//!
//! # Every observer is called, always
//!
//! The loop never short-circuits on the first `Cancel`. An observer that was
//! registered for an event gets told about it even when an earlier one has
//! already asked to stop, because observers have side effects: a `[[hook]]` that
//! appends to an audit log has to record *the event that cancelled the turn*,
//! which is exactly the event a short-circuiting fold would hide from it. Cheap
//! to get right once here; impossible to debug later from a log with a hole in it.
//!
//! # What it costs
//!
//! [`Observer::event`] is synchronous and runs on the run's own task, in order,
//! for every event — so this loop is on the run's critical path, and so is every
//! observer in it. A slow observer slows the run; **a panic in one takes the run's
//! future with it and leaves the run's row `running`**, with no outcome ever
//! written. The fan-out adds nothing but a slice walk; the observers inside it
//! are where that cost is spent, and each is responsible for its own.

use io_harness::{Flow, Observer, RunEvent};

/// Several observers behind the single `&dyn Observer` a turn accepts.
///
/// Borrows rather than owns, to match the entry points: `turn_*_steered` takes
/// `&dyn Observer` for the length of one turn, so the observers already outlive
/// the call and there is nothing for this to own.
pub struct Fanout<'a> {
    observers: Vec<&'a dyn Observer>,
}

impl<'a> Fanout<'a> {
    /// Fan every event out to `observers`, in the given order.
    ///
    /// Order decides who is called first and nothing else — see the module docs.
    /// An empty fan-out is legal and behaves as `Ignore`.
    pub fn new(observers: Vec<&'a dyn Observer>) -> Self {
        Self { observers }
    }
}

impl Observer for Fanout<'_> {
    fn event(&self, event: &RunEvent) -> Flow {
        // Deliberately not `any(|o| o.event(event).is_cancel())`: that
        // short-circuits, and an observer past the first `Cancel` would never
        // see the event that stopped the turn.
        let mut flow = Flow::Continue;
        for observer in &self.observers {
            if observer.event(event).is_cancel() {
                flow = Flow::Cancel;
            }
        }
        flow
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use io_harness::EventKind;
    use std::sync::atomic::{AtomicUsize, Ordering};

    /// Counts what it was told and answers with a fixed [`Flow`].
    struct Spy {
        seen: AtomicUsize,
        answer: Flow,
    }

    impl Spy {
        fn new(answer: Flow) -> Self {
            Self {
                seen: AtomicUsize::new(0),
                answer,
            }
        }

        fn seen(&self) -> usize {
            self.seen.load(Ordering::Relaxed)
        }
    }

    impl Observer for Spy {
        fn event(&self, _event: &RunEvent) -> Flow {
            self.seen.fetch_add(1, Ordering::Relaxed);
            self.answer
        }
    }

    fn an_event() -> RunEvent {
        RunEvent::new(1, 1, EventKind::Stalled)
    }

    #[test]
    fn every_observer_is_told_even_after_one_cancels() {
        let first = Spy::new(Flow::Continue);
        let stopper = Spy::new(Flow::Cancel);
        let last = Spy::new(Flow::Continue);

        let flow = Fanout::new(vec![&first, &stopper, &last]).event(&an_event());

        assert_eq!(flow, Flow::Cancel);
        // The one that matters: an audit hook registered after the hook that
        // stopped the turn still has to record the event that stopped it.
        assert_eq!(last.seen(), 1, "the fold short-circuited on Cancel");
        assert_eq!(first.seen(), 1);
        assert_eq!(stopper.seen(), 1);
    }

    #[test]
    fn cancel_wins_from_either_end() {
        for cancel_first in [true, false] {
            let canceller = Spy::new(Flow::Cancel);
            let watcher = Spy::new(Flow::Continue);
            let order: Vec<&dyn Observer> = if cancel_first {
                vec![&canceller, &watcher]
            } else {
                vec![&watcher, &canceller]
            };

            let flow = Fanout::new(order).event(&an_event());

            assert_eq!(
                flow,
                Flow::Cancel,
                "registration order decided the turn (cancel_first={cancel_first})"
            );
            assert_eq!(canceller.seen(), 1);
            assert_eq!(watcher.seen(), 1);
        }
    }

    #[test]
    fn all_continue_is_continue() {
        let a = Spy::new(Flow::Continue);
        let b = Spy::new(Flow::Continue);

        assert_eq!(
            Fanout::new(vec![&a, &b]).event(&an_event()),
            Flow::Continue,
            "a fan-out must not invent a cancel nobody asked for"
        );
    }

    #[test]
    fn an_empty_fanout_watches_nothing() {
        assert_eq!(Fanout::new(Vec::new()).event(&an_event()), Flow::Continue);
    }

    #[test]
    fn the_fanout_is_send_and_sync() {
        // `Observer: Send + Sync`, so a type that is not cannot be one — and this
        // fails at compile time, before any assertion runs.
        fn require<T: Send + Sync>() {}
        require::<Fanout<'_>>();
    }
}
