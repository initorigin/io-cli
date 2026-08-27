//! One `io` at a time on one session, and a sentence when there are two.
//!
//! **The lock is keyed on the session id, because a conversation is the only
//! thing two processes can collide over.** `Session::open` creates a new row
//! every time it is called, so two terminals started in one repository are two
//! separate conversations that share a directory name and nothing else; a lock
//! keyed on the workspace would refuse the second one for a conflict that does
//! not exist. The real collision is narrower and arrives from one place:
//! `/resume` and `io resume` take an id that already exists, and two processes
//! holding one id advance one head. Through 0.22.0 nothing guarded that — both
//! opened the same `runs.db`, both advanced the same head, and the loser of the
//! race paid for a completed turn and was shown `run 7 is held by another owner
//! until`, in which 7 is a *session* id, "run" is the wrong noun, and the expiry
//! is empty, because a head conflict populates neither field.
//!
//! **So the acquisition at startup is not a contention at all — it is a
//! publication.** A freshly opened session is unique to the process that opened
//! it, and taking its lock always succeeds. What the acquisition buys is the
//! owner record written beside it, so that a later process asked to enter *that*
//! conversation can be refused and can name who holds it.
//!
//! **What this does not cover, said plainly.** Two `io` in one workspace on two
//! different sessions are not in conflict and are not refused. Nothing here sees
//! a process that reaches the store without coming through this module either.
//! The head compare-and-swap in the harness stays the guard of last resort for
//! everything the lock misses; this module exists so the ordinary case is
//! answered with a sentence instead of a paid-for turn lost to a race.
//!
//! The guard is an advisory whole-file lock and nothing else.
//! [`std::fs::File::try_lock`] is stable on this crate's MSRV; it is `flock` on
//! unix and `LockFileEx` on Windows, and the kernel releases it on exit, on
//! panic and on `kill -9`. So on a local filesystem there is no such thing as a
//! stale lock to reap, and none of the usual pid-file machinery — a liveness
//! probe, a timestamp, a staleness threshold — is needed for the ordinary case.
//!
//! Both of those are held per open file description or per handle rather than
//! per process, which is what makes two `io` on one session contend and what
//! lets `tests/lock.rs` prove it without a second process. Solaris is the one
//! target where the standard library reaches for `fcntl(F_SETLK)` instead, whose
//! record locks are per process; io-cli ships no Solaris artifact, and the day
//! it does this module needs reading again rather than trusting.
//!
//! **Naming the holder cannot mean naming the operating system's process.**
//! `tests/dependencies.rs` asserts the direct dependency set in both directions
//! and forbids `nix` outright, and a process spawn is permitted only in
//! `crate::shell`, which may not name a `Store` and is callable only from the
//! driver. (Spelled that way round deliberately: that gate sweeps the raw text
//! of every source file for the type's full name, comments included, so a module
//! that wrote it out to say it was *not* spawning would be listed as a module
//! that spawns.) So there is no `kill(pid, 0)`, no `ps` and no `tasklist`, and
//! `/proc` is one platform's answer to a three-platform question. What this
//! module can state truthfully is what it itself wrote beside the lock: the
//! pid, the workspace root, the `io` version, and the instant the holder
//! started. That record is a separate file, because a Windows byte-range lock
//! is mandatory and a refused reader could not open the locked file to read it.
//!
//! **The record carries no host, and on a shared home that matters.** The pid is
//! compared against this process's own to recognise a lock this process already
//! holds, which is sound wherever the home is local: a live holder on this
//! machine cannot have our pid. With `~/.io-cli` on a network filesystem shared
//! between machines, two hosts can carry the same pid, and a genuine second
//! process would then be admitted to a session another `io` is holding. That is
//! the same configuration the lease below exists for, and the same one where an
//! advisory lock is not this program's business either.
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
//!
//! **The home is an argument too, and for the same kind of reason.**
//! [`crate::home::path`] answers `None` where the operator has no home
//! directory, and a module that resolved it for itself would have to invent an
//! answer or panic in the one place a lock must not do either. Handing the
//! directory in leaves that `None` where it belongs — with the caller, which is
//! also what makes every function here reachable from a test on a temporary
//! directory rather than only from the driver.

use std::fs::File;
use std::io;
use std::path::{Path, PathBuf};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

/// What both files are named after, so a directory listing sorts them together
/// and an operator can see at a glance that they belong to this product's own
/// bookkeeping rather than to their configuration.
const STEM: &str = "session-";

/// The file that is only ever locked, and is never opened for its contents.
///
/// **It holds no bytes and is never removed.** Unlinking a file another process
/// holds a lock on does not release anything: the next opener creates a fresh
/// inode, locks that, and two processes are then holding two different locks
/// while both believe they are alone. So the file stays, one per session, and
/// the emptiness is the point — a lock file with a payload is a lock file
/// somebody eventually parses, and on Windows the holder's own mandatory lock
/// would stop them.
const LOCK: &str = "lock";

/// The plain-text record beside the lock, saying who holds it.
///
/// **A second file rather than the lock's own contents**, because a Windows
/// byte-range lock is mandatory: the process that is being refused could not
/// read the locked file at all, and a refusal that cannot say anything about the
/// holder is the sentence this release exists to replace. It is never locked, so
/// a reader always gets *something* — possibly a half-written line, which
/// [`Owner::parse`] is written to survive.
const OWNER: &str = "owner";

/// How long a lease runs before [`Owner::lapsed`] will call it lapsed.
///
/// **This is a fallback for one case and a weaker guarantee than the lock.** On
/// a local filesystem the kernel releases the lock on exit, on panic and on
/// `kill -9`, so nothing here is ever consulted. On a network filesystem an
/// advisory lock is the kernel's business and not this program's, and the
/// record's own timestamp is the only evidence there is.
///
/// Twelve hours because the instant recorded is when the holder *started*, not
/// when it last did anything: a session left open across a working day is doing
/// nothing wrong, and a threshold that reads it as abandoned would offer to
/// take over a live conversation. The caller still confirms with the operator —
/// this constant only decides when the question is worth asking.
///
// ponytail: the record is written once at acquisition. If a tighter lease is
// ever wanted, rewrite it per turn rather than shortening this number.
pub const LEASE: Duration = Duration::from_secs(12 * 60 * 60);

/// The lock file and the owner record for one session, in that order.
///
/// **Named after the session id, and there is nothing to hash.** A
/// `Session::id()` is an `i64` the store issued, already unique and already
/// stable across processes and across toolchains — which is what a name two
/// separately-built `io` binaries have to agree on needs to be, and what the
/// workspace root was never able to be without inventing a hash to stand in for
/// it. The id is also the thing both processes know at the moment they can
/// actually contend, because contending means one of them resumed the other's
/// conversation.
#[must_use]
pub fn paths(home: &Path, session: i64) -> (PathBuf, PathBuf) {
    (
        home.join(format!("{STEM}{session}.{LOCK}")),
        home.join(format!("{STEM}{session}.{OWNER}")),
    )
}

/// What `io` wrote about itself beside the lock it holds.
///
/// **Every field is optional, and none of them may be invented.** The record can
/// be missing entirely, half-written, unreadable, or written by a version of
/// this product that recorded a different set of fields — four ordinary
/// outcomes, and a refusal that answered any of them with a guess would be
/// naming a process that does not exist.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Owner {
    /// The holder's process id, as it wrote it.
    ///
    /// **Never [`std::process::id`] of whoever is reading.** That value is
    /// always available and always wrong here, and it is the one substitution
    /// that would make a refusal look complete while telling the operator to go
    /// and kill themselves.
    pub pid: Option<u32>,
    /// The workspace the holder is working in.
    ///
    /// **Recorded, not keyed on.** The lock is the session's ([`paths`]); this
    /// field is here because "another `io` is working in `<root>`" is the clause
    /// that tells an operator with several terminals open which one to go to.
    ///
    /// Written through `to_string_lossy`, so a root that is not valid UTF-8
    /// arrives with the unrepresentable parts replaced. It is a sentence's
    /// worth of identification and never a path anything opens.
    pub root: Option<PathBuf>,
    /// The `io` version that took the lock.
    pub version: Option<String>,
    /// When the holder started, as it was handed that instant.
    ///
    /// Whole seconds since the unix epoch. The lease is measured in hours, so
    /// anything finer would be precision the decision cannot use.
    pub started: Option<SystemTime>,
}

/// A key and its value from one line of the record, or nothing.
fn field(line: &str) -> Option<(&str, &str)> {
    let (name, value) = line.split_once('=')?;
    let value = value.trim();
    if value.is_empty() {
        return None;
    }
    Some((name.trim(), value))
}

impl Owner {
    /// This process's own claim, for the record it is about to write.
    ///
    /// `started` is handed in rather than read: see the module note.
    #[must_use]
    pub fn claiming(root: &Path, started: SystemTime) -> Self {
        Self {
            pid: Some(std::process::id()),
            root: Some(root.to_path_buf()),
            version: Some(env!("CARGO_PKG_VERSION").to_string()),
            started: Some(started),
        }
    }

    /// The record's text: one `name = value` line per field that is known.
    ///
    /// A field that is `None` is left out rather than written as an empty value,
    /// so the file says only what is true and [`Owner::parse`] reads back what
    /// this wrote. Plain text and not TOML — `tests/dependencies.rs` permits
    /// `toml::from_str` in `src/edit.rs` alone, and a lock record is four
    /// scalars, which is not a configuration format's worth of problem.
    #[must_use]
    pub fn render(&self) -> String {
        let mut out = String::new();
        if let Some(pid) = self.pid {
            out.push_str(&format!("pid = {pid}\n"));
        }
        if let Some(root) = &self.root {
            out.push_str(&format!("root = {}\n", root.to_string_lossy()));
        }
        if let Some(version) = &self.version {
            out.push_str(&format!("version = {version}\n"));
        }
        if let Some(seconds) = self
            .started
            .and_then(|started| started.duration_since(UNIX_EPOCH).ok())
        {
            out.push_str(&format!("started = {}\n", seconds.as_secs()));
        }
        out
    }

    /// Read a record back, believing nothing.
    ///
    /// **Every way this can go wrong ends in a field that is `None`.** A line
    /// with no `=` is skipped, which is what a truncated write leaves; a name
    /// this version has never heard of is skipped, which is what an older or
    /// newer `io` leaves; a value that will not parse leaves its own field alone
    /// and no other. Nothing here panics and nothing here fails: a record is
    /// evidence, and evidence that cannot be read is an absence rather than an
    /// error.
    #[must_use]
    pub fn parse(text: &str) -> Self {
        let mut owner = Self::default();
        for line in text.lines() {
            let Some((name, value)) = field(line) else {
                continue;
            };
            match name {
                "pid" => owner.pid = value.parse().ok(),
                "root" => owner.root = Some(PathBuf::from(value)),
                "version" => owner.version = Some(value.to_string()),
                "started" => {
                    owner.started = value
                        .parse()
                        .ok()
                        .map(|seconds| UNIX_EPOCH + Duration::from_secs(seconds));
                }
                _ => {}
            }
        }
        owner
    }

    /// Whether the holder's lease has run out, or nothing if it never said when
    /// it started.
    ///
    /// **A fact, not a policy.** `Some(true)` means the record is older than
    /// `lease` and nothing more: on a local filesystem the lock itself is still
    /// the answer, and even on a network filesystem a lapsed lease is a reason
    /// to *ask* the operator whether to take over, never a reason to do it. The
    /// caller confirms; this only says whether there is anything to confirm.
    ///
    /// `None` where the record carries no instant, which is honest about the
    /// only two ways that happens — a missing or truncated record, and one
    /// written by a version that did not record the field. A missing timestamp
    /// read as "old enough" would make every unreadable record a takeover.
    ///
    /// A record stamped in the future — two machines on one network filesystem
    /// disagreeing about the time — is `Some(false)`: clock skew is not
    /// evidence of an abandoned session.
    #[must_use]
    pub fn lapsed(&self, now: SystemTime, lease: Duration) -> Option<bool> {
        let started = self.started?;
        Some(now.duration_since(started).is_ok_and(|age| age > lease))
    }

    /// The refusal, in one line an operator can act on.
    ///
    /// ASCII throughout: `--plain` and `NO_COLOR` are the terminals this product
    /// promises to be readable on, and a sentence that reaches for a dash the
    /// font cannot draw is one this crate's own glyph sweep would fail.
    ///
    /// **It degrades by saying less, never by saying `unknown`.** With nothing
    /// in the record it is the bare claim, which is still true and still tells
    /// the operator what happened; each field that *is* there adds its own
    /// clause and no others. Four `unknown`s in a row would read as a broken
    /// product rather than as a missing file.
    ///
    /// The instant is deliberately not in it. A count of seconds since 1970 is
    /// not a fact anybody can act on at a prompt, and its actual use is
    /// [`Owner::lapsed`], where the caller has a `now` to age it against.
    #[must_use]
    pub fn sentence(&self) -> String {
        let mut said = String::from("another `io` holds this session");
        if let Some(root) = &self.root {
            said.push_str(&format!(" in {}", root.to_string_lossy()));
        }
        let mut named = Vec::new();
        if let Some(pid) = self.pid {
            named.push(format!("pid {pid}"));
        }
        if let Some(version) = &self.version {
            named.push(format!("io {version}"));
        }
        if !named.is_empty() {
            said.push_str(&format!(" ({})", named.join(", ")));
        }
        said
    }
}

/// The lock, for as long as this value is alive.
///
/// **The kernel is the guarantee and [`Drop`] is only the tidying.** `flock` and
/// `LockFileEx` are both released when the file handle closes, which happens on
/// a clean exit, on a panic, and on `kill -9` where no destructor runs at all.
/// So this implementation exists to take the owner record away promptly, not to
/// make the lock correct — a `mem::forget` here would leak the record and still
/// free the lock when the process ended.
#[derive(Debug)]
pub struct Guard {
    /// Held open because closing it is what releases the lock.
    file: File,
    /// The record to take away, and whether this guard is the one that wrote it.
    owner: PathBuf,
    /// **False where the record could not be written**, and the reason `Drop`
    /// consults it: removing a file this process did not create would delete
    /// somebody else's evidence, and the whole point of the record is that it
    /// says who is really there.
    wrote: bool,
}

impl Drop for Guard {
    fn drop(&mut self) {
        if self.wrote {
            let _ = std::fs::remove_file(&self.owner);
        }
        // **The record goes first and the lock second, and the order is the
        // point.** Released the other way round, the next `io` could take the
        // lock and write its own record in the window before this one deleted
        // the file — and the deletion would then take the new holder's record
        // away, leaving a held lock with nothing beside it to name it.
        //
        // Best effort: an `unlock` that fails changes nothing, because closing
        // the handle a line later releases it anyway.
        let _ = self.file.unlock();
    }
}

/// What an attempt to take the lock found.
#[derive(Debug)]
pub enum Taken {
    /// It is this process's until the guard is dropped or the process ends.
    Held(Guard),
    /// Another `io` has it, and this is what that `io` said about itself.
    Refused(Owner),
}

/// Open a file this module owns, readable by its owner alone on unix.
///
/// The mode is set at CREATE time rather than afterwards, which is the rule
/// [`crate::settings::write`] already follows for the file holding a credential:
/// a mode applied after the fact leaves a window in which the file is on disk
/// with the wrong one. The owner record names a workspace path, and the
/// directory around both is already `0700`.
fn open(path: &Path, truncate: bool) -> io::Result<File> {
    let mut options = std::fs::OpenOptions::new();
    options
        .read(true)
        .write(true)
        .create(true)
        .truncate(truncate);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600);
    }
    options.open(path)
}

/// Take the lock for one session, or say who has it.
///
/// `session` is what the lock is named after; `root` is only written into the
/// record, for the refusal to name. `started` is the instant to record, handed
/// in by the driver — the only thing in this crate permitted to read a clock.
///
/// **On a session this process just opened it always succeeds**, because that id
/// has never existed anywhere else. The call is worth making anyway: succeeding
/// is what publishes the owner record, and the record is the whole of what a
/// later `io resume` of this id gets to say when it is refused.
///
/// **`Err` means the attempt could not be made, and never that the lock is
/// held.** [`std::fs::File::try_lock`] answers with
/// [`std::fs::TryLockError::WouldBlock`] for a lock somebody else holds and with
/// [`std::fs::TryLockError::Error`] for an I/O failure, and the standard library
/// guarantees it does not put a `WouldBlock` error inside the second. Collapsing
/// the two either way is the defect this function is written around: an I/O
/// error reported as "held" locks an operator out of their own repository with a
/// sentence about a process that was never there, and a held lock reported as an
/// error lets two `io` processes into one session, which is the whole thing this
/// module exists to stop.
pub fn acquire(home: &Path, session: i64, root: &Path, started: SystemTime) -> io::Result<Taken> {
    crate::home::create(home)?;
    let (lock, owner) = paths(home, session);

    // Not truncated: the file's contents are never read, and truncating one that
    // another process holds a lock on would be a write for no reason.
    let file = open(&lock, false)?;
    match file.try_lock() {
        Ok(()) => {}
        Err(std::fs::TryLockError::WouldBlock) => {
            return Ok(Taken::Refused(read_owner(&owner)));
        }
        Err(std::fs::TryLockError::Error(error)) => return Err(error),
    }

    // **The lock is already ours before the record is written**, so a record
    // that cannot be written costs the refusal its facts and nothing else. The
    // alternative — treating it as a failure and giving the lock back — would
    // turn a full disk into two `io` processes in one session.
    let wrote = write_owner(&owner, &Owner::claiming(root, started)).is_ok();
    Ok(Taken::Held(Guard { file, owner, wrote }))
}

/// The record as it is on disk, or an empty one.
///
/// A record that is missing, unreadable, or not valid UTF-8 is an [`Owner`] with
/// nothing in it — the same shape a truncated one produces, because from the
/// refused process's seat they are the same fact: nothing is known about the
/// holder.
fn read_owner(path: &Path) -> Owner {
    match std::fs::read_to_string(path) {
        Ok(text) => Owner::parse(&text),
        Err(_) => Owner::default(),
    }
}

/// Write the record, flushed before the guard is handed back.
///
/// `sync_all` for the same reason [`crate::settings::write`] calls it: the
/// process that reads this file is a different one, and a record still sitting
/// in the page cache when the machine loses power is a lock with no holder named
/// beside it.
fn write_owner(path: &Path, owner: &Owner) -> io::Result<()> {
    use std::io::Write as _;
    let mut file = open(path, true)?;
    file.write_all(owner.render().as_bytes())?;
    file.sync_all()
}
