//! Who is allowed to read the terminal right now.
//!
//! A process has one terminal and this binary starts one reader over it, so the
//! lock is a `static` rather than a handle threaded through five signatures that
//! have nothing else to do with it.
//!
//! Two callers want it and they want it for opposite reasons:
//!
//! - The keyboard reader holds it around every poll, because crossterm's `poll`
//!   consumes bytes into its own parser: a reader that only locked around `read`
//!   would still swallow whatever arrived while it was polling.
//! - The renderer holds it around anything that asks the terminal where its
//!   cursor is (`ESC[6n`), because the answer arrives on stdin. A reader still
//!   running takes that answer first, the query times out after two seconds, and
//!   the program appears to hang — or, on the path where the query is not
//!   optional, refuses to start. [`crate::term::Screen`] takes this itself at
//!   every one of those sites rather than asking its callers to; a caller may
//!   still hold one around a larger operation, which is why a placement nests.
//!
//! **The lock alone is not enough, and 0.13.1 is what that cost.** A reader that
//! releases the lock at the bottom of its loop and takes it again at the top is
//! releasing it for the length of one instruction, and `std::sync::Mutex` makes
//! no fairness promise — on macOS it is `os_unfair_lock` and the name is not
//! decoration. The reader wins the re-acquisition essentially every time, so a
//! placement waiting behind it waits for a scheduling accident: measured against
//! the real binary, the keystroke that grew the composer to a third row froze the
//! session for 5.7 seconds and it answered nothing afterwards. `/clear` takes the
//! same lock. So does a paste expanded back to its full text, which is the worst
//! of the three because it floods the reader with the events that keep it busy.
//!
//! So the reader asks before it takes: [`next_event`] stands aside while
//! [`placing`] is waiting, and the placement is served within one poll interval.
//! That is the whole mechanism — one flag, two stores and a load — and it is
//! here, in the library, because no integration test links a binary.

use std::cell::Cell;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Mutex, MutexGuard};
use std::time::Duration;

use crossterm::event::Event;

/// How long the reader polls before coming back for another look.
///
/// It bounds two things at once. A `stop` flag is seen within one interval, and
/// — since 0.13.1 — so is a placement waiting for the lock, because the reader
/// cannot be interrupted mid-poll and this is the only lever on the hand-over.
/// Ten milliseconds is a hundred wake-ups a second at an idle prompt, each one a
/// timed `select`, which is the price of a composer row that appears when it is
/// asked for. It was forty, chosen when nothing waited on it.
pub const POLL: Duration = Duration::from_millis(10);

/// How long the reader waits before asking again, having been told to stand
/// aside.
///
/// A placement is a handful of milliseconds — an escape sequence, a query and a
/// reply — so this is short enough to be invisible and long enough that the
/// reader is not asking a hundred thousand times a second while it happens.
const STAND_ASIDE: Duration = Duration::from_millis(1);

/// The lock itself. Held around a poll-and-read, or around a placement.
static READING: Mutex<()> = Mutex::new(());

/// Set while a placement wants the terminal — from the moment it asks until the
/// moment it is finished with it.
///
/// Read by the reader before it takes the lock, which is what turns an unfair
/// mutex into a fair enough one for the only two callers there are. It covers
/// the holding as well as the waiting on purpose: a reader that queued on the
/// mutex during a placement would be holding the terminal again the instant the
/// placement let go, and the next placement would be behind it.
static WANTED: AtomicBool = AtomicBool::new(false);

thread_local! {
    /// Whether *this* thread is the one holding [`READING`] for a placement.
    ///
    /// `std::sync::Mutex` is not reentrant, so a thread that took the lock and
    /// then took it again would wait for itself forever — and the whole session
    /// with it, because the thread that places viewports is also the thread that
    /// draws them. Since 0.18.0 that nesting is ordinary rather than exotic:
    /// `Screen` takes the lock at every site that queries the cursor, and `main`
    /// still takes one around the larger operations — re-placing the viewport,
    /// rewinding a turn — that are built out of several of those sites.
    ///
    /// A thread-local rather than a stored `ThreadId`, because that is the whole
    /// question being asked and it needs no comparison to answer it. It cannot be
    /// read for another thread, which is also correct: [`Placing`] holds a
    /// [`MutexGuard`] and so cannot leave the thread that made it.
    ///
    /// A depth count would be the other spelling and it is the worse one: it is
    /// only right if guards are dropped in the order they were taken, and nothing
    /// here can promise that. What is recorded instead is which guard *is* the
    /// holder, so the bookkeeping is order-independent.
    static HOLDING: Cell<bool> = const { Cell::new(false) };
}

/// The reader could not be asked. Its thread has nothing left to do.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Broken;

/// Take the terminal for a placement — anything that writes a query to the
/// terminal and reads the answer off stdin — waiting for the reader's current
/// poll to end.
///
/// A caller must hold this for the whole placement. Ignores a previous holder's
/// panic: a poisoned lock here means the reader thread died mid-poll, and there
/// is no state behind this mutex to be left inconsistent — only the terminal, and
/// a dead reader must not also cost the session its viewport.
///
/// **Reentrant.** A thread that already holds it gets a token that does nothing,
/// so the terminal stays taken for as long as the outermost caller wants it and
/// nothing waits for itself. The `HOLDING` thread-local below carries why that
/// is the ordinary case rather than a defensive one — it is named in prose
/// rather than linked because it is private and this is not.
pub fn placing() -> Placing {
    if HOLDING.get() {
        return Placing(None);
    }
    WANTED.store(true, Ordering::SeqCst);
    let held = READING
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    HOLDING.set(true);
    Placing(Some(held))
}

/// The terminal, held for a placement — or, when this thread was already holding
/// it, a token that says so and does nothing.
///
/// The flag is cleared when this is dropped rather than when the lock is taken,
/// so the reader stays out of the way for the whole placement — the escape
/// sequence, the query, the reply and the re-attach — instead of queueing on the
/// mutex behind it. A reader queued there is a reader holding the terminal again
/// the moment the placement finishes, which is the thing this module exists to
/// stop.
pub struct Placing(Option<MutexGuard<'static, ()>>);

impl Drop for Placing {
    fn drop(&mut self) {
        // Only the guard that actually took the lock gives it back. An inner one
        // dropping must not tell the reader the terminal is free while the outer
        // one is still using it, and that stays true whichever order the two are
        // dropped in.
        if self.0.is_some() {
            HOLDING.set(false);
            WANTED.store(false, Ordering::SeqCst);
        }
    }
}

/// Whether a placement is queued for the terminal right now.
///
/// The flag, readable. It exists for the tests, which have to know that a
/// placement has *reached* the lock before they can assert what the reader does
/// about it — and the alternative is a sleep, which `tests/timing.rs` forbids
/// every test in this repository for reasons this release proved again.
pub fn placement_waiting() -> bool {
    WANTED.load(Ordering::SeqCst)
}

/// Run one reader's turn at the terminal, or stand aside because a placement
/// wants it.
///
/// `None` is "come back later", and it is the whole fairness rule: the reader
/// asks before it takes, so a placement waiting for the lock waits for the turn
/// already in flight and never for the next one as well.
///
/// Taking the guard around `work` rather than handing it out is what keeps a
/// reader's poll and its read inside one critical section. They cannot be split
/// across two acquisitions by a caller, because there is no acquisition to hand
/// out — and a lock released between the poll and the read would let the reader
/// swallow the cursor-position reply a placement is waiting for, which is the
/// defect `Keyboard::start`'s signature was written to prevent.
///
/// This is the seam a test drives: `work` needs no terminal, and the property
/// under test is who gets the lock rather than what is typed.
///
/// Not reentrant, and it does not need to be: the stand-aside above is checked
/// before the lock is taken, and a thread that is holding a [`Placing`] has
/// already said so through [`placement_waiting`] — so it declines here rather
/// than waiting for itself.
pub fn reading<T>(work: impl FnOnce() -> T) -> Option<T> {
    if placement_waiting() {
        // A short pause rather than a spin: the reader has nothing to do until
        // the placement is done with the terminal, and a loop that asked as fast
        // as it could would burn a core for the few milliseconds that takes.
        std::thread::sleep(STAND_ASIDE);
        return None;
    }
    let _held = READING
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    Some(work())
}

/// Poll the terminal for one event, under the lock, standing aside for a
/// placement.
///
/// `Ok(None)` means the interval passed with nothing typed — or that a placement
/// wanted the lock, which from the reader's point of view is the same instruction:
/// come back later. `Err(Broken)` means stdin itself failed, which is the end of
/// the reader.
pub fn next_event() -> Result<Option<Event>, Broken> {
    reading(|| match crossterm::event::poll(POLL) {
        Ok(true) => crossterm::event::read().map(Some).map_err(|_| Broken),
        Ok(false) => Ok(None),
        Err(_) => Err(Broken),
    })
    .unwrap_or(Ok(None))
}
