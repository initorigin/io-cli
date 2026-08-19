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

/// The observer handed to `Session::turn_steered` and to
/// `Session::turn_contained_bounded_observed`.
pub struct Bridge {
    events: UnboundedSender<RunEvent>,
    /// Set to end a *contained* turn, and never set for a steered one.
    ///
    /// A contained turn takes no `SteerInbox` — the entry point that reaches the
    /// spawn loop has no parameter for one — so the only way an interface can
    /// stop it is the return value of this method. io-harness honours
    /// [`Flow::Cancel`] at the next step boundary at which no child is in flight,
    /// which is a real wait and is disclosed as one rather than presented as an
    /// immediate stop.
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
    /// The switch that ends a contained turn, for the driver to hold.
    ///
    /// Handed out only where a contained turn is started. A steered turn leaves
    /// it untouched for its whole life, which is what keeps `Ctrl+C` on the path
    /// it has had since 0.1.0 for every session that configures no caps.
    pub fn canceller(&self) -> Arc<AtomicBool> {
        Arc::clone(&self.cancel)
    }
}

impl Observer for Bridge {
    fn event(&self, event: &RunEvent) -> Flow {
        // A send error means the interface is gone — the process is exiting, or
        // the receiver was dropped. Nothing to do about it here.
        let _ = self.events.send(event.clone());
        // A steered turn is stopped through `Steer::interrupt`, which ends it at
        // a step boundary and leaves it resumable, and nothing sets this flag on
        // that path. A contained turn has no inbox to interrupt, so this is the
        // one place it can be stopped from — and it is read, never assumed: a
        // bridge that answered `Cancel` unconditionally would cancel every
        // steered turn the moment its first event arrived.
        if self.cancel.load(Ordering::Relaxed) {
            return Flow::Cancel;
        }
        Flow::Continue
    }
}
