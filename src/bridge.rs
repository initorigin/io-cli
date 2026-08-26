//! The one seam between io-harness and the screen.
//!
//! `Observer::event` is synchronous and runs **on the run's own task**, in order,
//! for every event. Rendering inside it would put terminal I/O on the path of the
//! agent loop: a slow write would slow the run down, and a write that blocked
//! would stop it. So the observer does one thing — hand the event on — and the
//! interface draws whenever it next gets to.
//!
//! The channel is unbounded on purpose. A bounded one has two ways to behave when
//! it fills, and both are wrong here: blocking stalls the run, and dropping loses
//! an event, which is the failure F8 exists to prevent. The bound that matters is
//! that the interface drains continuously, which it does.

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

use io_harness::{Flow, Observer, RunEvent};
use tokio::sync::mpsc::{unbounded_channel, UnboundedReceiver, UnboundedSender};

/// The observer handed to `Session::turn_bounded_steered` and to
/// `Session::turn_contained_bounded_steered`.
pub struct Bridge {
    events: UnboundedSender<RunEvent>,
    /// Set to end a turn, contained or not. **The whole of how `Ctrl+C` stops a
    /// run**, and the only mechanism this product will use for it.
    ///
    /// Since 0.17.0 both turn shapes also carry a `SteerInbox`, so
    /// `Steer::interrupt` is a second way to reach the same
    /// `RunOutcome::Cancelled` at the same step boundary — and the stop key is
    /// deliberately not moved onto it. The two are recorded by different code in
    /// io-harness, an operator cannot tell them apart from the screen, and this
    /// is the one key no configuration file may rebind: a mechanism swap here
    /// costs a rewrite of `tests/interrupt.rs` and buys nothing anybody can see.
    /// The inbox carries the operator's *words*; this flag carries their stop.
    ///
    /// io-harness honours [`Flow::Cancel`] at the next step boundary — on a
    /// contained turn, the next one at which no child is in flight, which is a
    /// real wait and is disclosed as one rather than presented as an immediate
    /// stop.
    ///
    /// Shared rather than checked through a channel because `event` must not
    /// block: it runs on the run's own task, in order, for every event.
    cancel: Arc<AtomicBool>,
}

/// A bridge and the receiver the interface drains.
pub fn channel() -> (Bridge, UnboundedReceiver<RunEvent>) {
    let (events, rx) = unbounded_channel();
    (
        Bridge {
            events,
            cancel: Arc::new(AtomicBool::new(false)),
        },
        rx,
    )
}

impl Bridge {
    /// The switch that ends a turn, for the driver to hold.
    ///
    /// Taken once per turn, contained or not, and set by the stop key and by
    /// nothing else — which is what keeps `Ctrl+C` on the path it has had since
    /// 0.1.0 now that a turn can also be spoken to.
    pub fn canceller(&self) -> Arc<AtomicBool> {
        Arc::clone(&self.cancel)
    }
}

impl Observer for Bridge {
    fn event(&self, event: &RunEvent) -> Flow {
        // A send error means the interface is gone — the process is exiting, or
        // the receiver was dropped. Nothing to do about it here.
        let _ = self.events.send(event.clone());
        // Read, never assumed: a bridge that answered `Cancel` because a turn was
        // running would cancel every turn the moment its first event arrived.
        // This is the one place either kind of turn is stopped from — the steer
        // inbox both of them now carry is for the operator's words, not their
        // stop key.
        if self.cancel.load(Ordering::Relaxed) {
            return Flow::Cancel;
        }
        Flow::Continue
    }
}
