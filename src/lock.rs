//! One `io` at a time on one session, and a sentence when there are two.
//!
//! This product keeps one store for the whole machine, so two terminals in one
//! repository is the ordinary case rather than the exotic one. Through 0.22.0
//! nothing guarded it: both processes opened the same `runs.db`, both advanced
//! the same session head, and the loser of that race paid for a completed turn
//! and was shown `run 7 is held by another owner until` — in which 7 is a
//! *session* id, "run" is the wrong noun, and the expiry is empty, because a
//! head conflict populates neither field.
//!
//! The guard is an advisory whole-file lock and nothing else.
//! [`std::fs::File::try_lock`] is stable on this crate's MSRV; it is `flock` on
//! unix and `LockFileEx` on Windows, and the kernel releases it on exit, on
//! panic and on `kill -9`. So on a local filesystem there is no such thing as a
//! stale lock to reap, and none of the usual pid-file machinery — a liveness
//! probe, a timestamp, a staleness threshold — is needed for the ordinary case.
//!
//! **Naming the holder cannot mean naming the operating system's process.**
//! `tests/dependencies.rs` asserts the direct dependency set in both directions
//! and forbids `nix` outright, and `std::process::Command` is permitted only in
//! `crate::shell`, which may not name a `Store` and is callable only from the
//! driver. So there is no `kill(pid, 0)`, no `ps` and no `tasklist`, and
//! `/proc` is one platform's answer to a three-platform question. What this
//! module can state truthfully is what it itself wrote beside the lock: the
//! pid, the workspace root, the `io` version, and the instant the holder
//! started. That record is a separate file, because a Windows byte-range lock
//! is mandatory and a refused reader could not open the locked file to read it.
//!
//! The lease and its lapse exist only for the case the kernel cannot cover — a
//! network filesystem, where an advisory lock is the kernel's business and not
//! this program's. There the owner record's own timestamp is all there is, and
//! the module says so rather than implying a guarantee it does not have.
//!
//! No clock is read here. `tests/timing.rs` permits [`std::time::SystemTime`]
//! readings in the driver and nowhere else, so every instant this module
//! records or ages arrives as an argument — the same seam [`crate::sessions`]
//! documents for its own stamps.
