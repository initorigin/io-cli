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

use io_harness::{Flow, Observer, RunEvent};
use tokio::sync::mpsc::{unbounded_channel, UnboundedReceiver, UnboundedSender};

/// The observer handed to `Session::turn_steered`.
pub struct Bridge {
    events: UnboundedSender<RunEvent>,
}

/// A bridge and the receiver the interface drains.
pub fn channel() -> (Bridge, UnboundedReceiver<RunEvent>) {
    let (events, rx) = unbounded_channel();
    (Bridge { events }, rx)
}

impl Observer for Bridge {
    fn event(&self, event: &RunEvent) -> Flow {
        // A send error means the interface is gone — the process is exiting, or
        // the receiver was dropped. Nothing to do about it here, and cancelling
        // the run from an observer is not this product's way of stopping a turn:
        // `Ctrl+C` goes through `Steer::interrupt`, which ends the turn at a step
        // boundary and leaves it resumable.
        let _ = self.events.send(event.clone());
        Flow::Continue
    }
}
