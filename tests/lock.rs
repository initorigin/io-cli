//! F9 — one `io` at a time on one workspace, and a refusal that says who has it.
//!
//! **Every test here runs in one process, and that is not a compromise.** On
//! unix `flock` is held per open file description and on Windows `LockFileEx` is
//! held per handle, so two `File::open` calls on one path inside a single
//! process contend exactly as two processes would. That is what makes this file
//! possible at all: N1 forbids a test that sleeps, and a fixture that spawned a
//! second `io` and waited for it to reach the lock would have to.
//!
//! **That premise is a fact about the three platforms this product releases
//! for, and it is stated rather than assumed.** The standard library reaches for
//! `flock` on Linux, on Apple platforms and on the BSDs, and for `LockFileEx` on
//! Windows; both are held per description or per handle. Solaris alone uses
//! `fcntl(F_SETLK)`, whose record locks are held per *process* — two opens there
//! would not conflict, and neither this file nor `src/lock.rs` would be right
//! about it. io-cli ships no Solaris artifact.
//!
//! No clock is read either. Every instant below is built from the unix epoch and
//! a `Duration`, and the lapse decision takes the `now` it is aged against as an
//! argument — which is the whole reason `Owner::lapsed` has that shape.
//!
//! The sabotage this file is aimed at is a refusal that reports the *reader's*
//! own process id instead of the holder's. It always looks right, because in the
//! ordinary fixture the two are the same number. So the record is overwritten
//! with a pid no process on any of the three platforms can have, and the refusal
//! is made to name it.

use std::path::{Path, PathBuf};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use io_cli::lock::{self, Guard, Owner, Taken, LEASE};

/// An io home that does not exist yet, so `acquire` is made to create it — which
/// is the same `home::create` the `0700` assertion at the bottom of this file
/// depends on.
fn home(dir: &Path) -> PathBuf {
    dir.join(".io-cli")
}

/// A fixed instant, so nothing in this file has to ask what time it is.
fn at(seconds: u64) -> SystemTime {
    UNIX_EPOCH + Duration::from_secs(seconds)
}

/// The instant every acquisition below records unless it says otherwise.
fn started() -> SystemTime {
    at(1_700_000_000)
}

fn guard(taken: Taken) -> Guard {
    match taken {
        Taken::Held(guard) => guard,
        Taken::Refused(owner) => {
            panic!(
                "the lock should have been free; it was refused: {}",
                owner.sentence()
            )
        }
    }
}

fn refusal(taken: Taken) -> Owner {
    match taken {
        Taken::Refused(owner) => owner,
        Taken::Held(_) => panic!("two `io` processes were let into one session"),
    }
}

/// **F9.** A second `io` in the same workspace is refused while the first is
/// alive, and let in the moment it is not.
///
/// Both halves in one test on purpose: an implementation that refuses everything
/// passes the first assertion and is useless, and one that refuses nothing
/// passes the second and is the defect.
#[test]
fn f9_a_second_io_in_one_workspace_is_refused_while_the_first_holds_the_lock() {
    let dir = tempfile::tempdir().expect("a temporary directory");
    let home = home(dir.path());
    let root = dir.path().join("repository");

    let first = guard(lock::acquire(&home, &root, started()).expect("the lock is free"));
    let second = lock::acquire(&home, &root, started()).expect("asking is not an error");
    let owner = refusal(second);
    assert!(
        owner.sentence().contains("another `io` holds this session"),
        "the refusal has to say what happened: {}",
        owner.sentence(),
    );

    drop(first);

    let third = guard(lock::acquire(&home, &root, started()).expect("the lock is free again"));
    let (lock_path, owner_path) = lock::paths(&home, &root);
    assert!(
        lock_path.is_file(),
        "the lock file itself is never removed; unlinking one another process holds \
         would give the next opener a fresh inode and two `io` would both think they \
         were alone",
    );
    drop(third);

    assert!(
        !owner_path.exists(),
        "the owner record is taken away with the lock, so a later reader is not told \
         about a process that has gone",
    );
    assert!(lock_path.is_file(), "and the lock file is still there");
}

/// **F9, and the sabotage.** The refusal carries what the *holder* wrote, and
/// specifically a process id that is not the reader's own.
///
/// The first half asserts the record `acquire` writes is true of the holder. The
/// second overwrites it with four facts that are deliberately none of this
/// process's — a pid no operating system issues, another workspace, a version
/// this binary is not, and an instant a decade off — and requires every one of
/// them to come back. A refusal assembled from `std::process::id()`,
/// `env!("CARGO_PKG_VERSION")` and the root it was handed would pass the first
/// half and fail here, which is the point of writing both.
#[test]
fn f9_the_refusal_names_the_holder_that_wrote_the_record_and_never_the_reader() {
    let dir = tempfile::tempdir().expect("a temporary directory");
    let home = home(dir.path());
    let root = dir.path().join("repository");

    let held = guard(lock::acquire(&home, &root, started()).expect("the lock is free"));
    let mine = refusal(lock::acquire(&home, &root, started()).expect("asking is not an error"));

    assert_eq!(mine.pid, Some(std::process::id()));
    assert_eq!(mine.root.as_deref(), Some(root.as_path()));
    assert_eq!(mine.version.as_deref(), Some(env!("CARGO_PKG_VERSION")));
    assert_eq!(
        mine.started,
        Some(started()),
        "the instant recorded is the one the driver handed in, not one this crate read",
    );

    // A pid larger than any of the three platforms issues, so it cannot be a
    // process and it certainly cannot be this one.
    let planted = Owner {
        pid: Some(u32::MAX),
        root: Some(PathBuf::from("/some/other/workspace")),
        version: Some("0.0.1-not-this-binary".to_string()),
        started: Some(at(1_234_567_890)),
    };
    let (_, owner_path) = lock::paths(&home, &root);
    std::fs::write(&owner_path, planted.render()).expect("the record is a plain file");

    let read = refusal(lock::acquire(&home, &root, started()).expect("asking is not an error"));
    assert_ne!(
        planted.pid,
        Some(std::process::id()),
        "the fixture has to be a pid this process cannot have, or it proves nothing",
    );
    assert_eq!(read, planted, "every field came from the file");

    let said = read.sentence();
    assert!(
        said.is_ascii(),
        "the refusal is drawn on a terminal that may have no font: {said}"
    );
    assert!(
        said.contains(&u32::MAX.to_string()),
        "the holder's pid is named: {said}"
    );
    assert!(
        said.contains("/some/other/workspace"),
        "and its workspace: {said}"
    );
    assert!(
        said.contains("0.0.1-not-this-binary"),
        "and its version: {said}"
    );

    drop(held);
}

/// **F9.** A record that is missing, truncated, or written by a version that
/// recorded different fields still produces a refusal, and it claims only what
/// it can read.
///
/// Four shapes, one property: nothing panics and nothing is invented. The
/// sentence is asserted for what it does *not* say as well — a refusal that
/// printed `unknown` four times would satisfy "does not claim facts it does not
/// have" and read as a broken product.
#[test]
fn f9_a_record_it_cannot_read_produces_a_refusal_that_claims_nothing_it_does_not_know() {
    let dir = tempfile::tempdir().expect("a temporary directory");
    let home = home(dir.path());
    let root = dir.path().join("repository");
    let (_, owner_path) = lock::paths(&home, &root);

    let held = guard(lock::acquire(&home, &root, started()).expect("the lock is free"));

    // Missing: removed out from under the holder, which is what a `tmpwatch` or
    // an operator tidying their home directory does.
    std::fs::remove_file(&owner_path).expect("the record is an ordinary file");
    let bare = refusal(lock::acquire(&home, &root, started()).expect("asking is not an error"));
    assert_eq!(
        bare,
        Owner::default(),
        "nothing was read, so nothing is claimed"
    );
    assert_eq!(
        bare.sentence(),
        "another `io` holds this session",
        "with nothing known the sentence is the bare claim and no placeholders",
    );
    for absent in ["unknown", "None", "?"] {
        assert!(
            !bare.sentence().contains(absent),
            "the sentence pads a missing fact with {absent:?}: {}",
            bare.sentence(),
        );
    }

    // Truncated: the write was cut off mid-line. What landed is kept and the
    // half-line is not guessed at.
    std::fs::write(&owner_path, "pid = 4242\nroot = /half/a/pa").expect("the record");
    let cut = refusal(lock::acquire(&home, &root, started()).expect("asking is not an error"));
    assert_eq!(cut.pid, Some(4242), "the line that landed whole is read");
    assert_eq!(
        cut.root.as_deref(),
        Some(Path::new("/half/a/pa")),
        "a final line with a value is taken at its word; there is nothing else to do",
    );
    assert_eq!(cut.version, None);
    assert_eq!(cut.started, None);
    assert!(cut.sentence().contains("4242"));

    // Truncated inside a name, so the last line has no `=` at all.
    std::fs::write(&owner_path, "pid = 4242\nver").expect("the record");
    let mid = refusal(lock::acquire(&home, &root, started()).expect("asking is not an error"));
    assert_eq!(mid.pid, Some(4242));
    assert_eq!(mid.version, None, "half a field name is not a version");

    // Another version's record: fields this one has never heard of, one it has,
    // and one whose value will not parse.
    std::fs::write(
        &owner_path,
        "pid = not-a-number\nhostname = build-07\nlease_seconds = 900\nversion = 0.99.0\n",
    )
    .expect("the record");
    let other = refusal(lock::acquire(&home, &root, started()).expect("asking is not an error"));
    assert_eq!(
        other.version.as_deref(),
        Some("0.99.0"),
        "the field it does know is read"
    );
    assert_eq!(
        other.pid, None,
        "a value that will not parse leaves its own field empty"
    );
    assert_eq!(other.root, None);
    assert_eq!(other.started, None);
    assert!(
        !other.sentence().contains("build-07"),
        "a field this version does not model is not repeated back as if it were understood: {}",
        other.sentence(),
    );

    // Not text at all.
    std::fs::write(&owner_path, [0xff, 0xfe, 0x00, 0x80]).expect("the record");
    let binary = refusal(lock::acquire(&home, &root, started()).expect("asking is not an error"));
    assert_eq!(
        binary,
        Owner::default(),
        "bytes that are not text are an absence, not a panic"
    );

    drop(held);
}

/// **F9.** A record this guard did not write is left alone.
///
/// The fixture puts a directory where the record goes, which is the cheapest
/// thing on all three platforms that a write cannot succeed against — so the
/// guard is acquired with no record of its own. Dropping it must then remove
/// nothing. A `Drop` that unlinked the path unconditionally would take away
/// whatever was actually there, and the one time that path holds something this
/// process did not write is the one time it matters.
#[test]
fn f9_a_guard_that_could_not_write_a_record_removes_nothing_when_it_goes() {
    let dir = tempfile::tempdir().expect("a temporary directory");
    let home = home(dir.path());
    let root = dir.path().join("repository");
    let (_, owner_path) = lock::paths(&home, &root);

    std::fs::create_dir_all(&owner_path).expect("something else is at the record's path");
    let marker = owner_path.join("not-ours");
    std::fs::write(&marker, "somebody else's").expect("the marker");

    let held = guard(lock::acquire(&home, &root, started()).expect("the record is not the lock"));
    let unreadable =
        refusal(lock::acquire(&home, &root, started()).expect("asking is not an error"));
    assert_eq!(
        unreadable,
        Owner::default(),
        "a record that cannot be read is an absence; the lock is still refused",
    );

    drop(held);

    assert!(
        marker.is_file(),
        "the guard removed a record it did not write"
    );
}

/// **F9.** The lapse is computed from a `now` that is handed in, and it is
/// asserted on both sides of the threshold with no clock and nothing sleeping.
///
/// It is a *fact* and not a policy: the caller confirms a takeover with the
/// operator, and this only says whether there is anything to confirm. So the
/// interesting cases are the two boundaries, the record that never said when it
/// started, and the clock that runs backwards.
#[test]
fn f9_the_lease_lapses_against_a_now_that_is_handed_in_and_never_read() {
    let began = at(1_700_000_000);
    let owner = Owner {
        started: Some(began),
        ..Owner::default()
    };

    assert_eq!(
        owner.lapsed(began, LEASE),
        Some(false),
        "a lease that has just started has not lapsed",
    );
    assert_eq!(
        owner.lapsed(began + LEASE, LEASE),
        Some(false),
        "the threshold itself is inside the lease; a takeover offered at the exact \
         second is one offered to a session still doing its work",
    );
    assert_eq!(
        owner.lapsed(began + LEASE + Duration::from_secs(1), LEASE),
        Some(true),
        "and one second past it has lapsed",
    );

    assert_eq!(
        owner.lapsed(began - Duration::from_secs(3_600), LEASE),
        Some(false),
        "a record stamped in the future is two machines disagreeing about the time, \
         which is not evidence that anybody abandoned anything",
    );

    assert_eq!(
        Owner::default().lapsed(began + LEASE * 100, LEASE),
        None,
        "a record with no instant has no opinion; reading its absence as `old enough` \
         would make every unreadable record a takeover",
    );

    // The lease is a knob, not a constant the decision hides inside itself.
    assert_eq!(
        owner.lapsed(began + Duration::from_secs(60), Duration::from_secs(30)),
        Some(true),
    );
    assert!(
        LEASE > Duration::from_secs(60 * 60),
        "the instant recorded is when the holder STARTED, so a lease shorter than a \
         working session would offer to take over a live conversation",
    );
}

/// **F9.** Two different workspaces do not contend.
///
/// The lock is scoped by the workspace root, so a second terminal in another
/// repository is not a second `io` on this session. A single global lock would
/// pass every other test in this file and make the product unusable for anyone
/// with two checkouts.
#[test]
fn f9_two_different_workspaces_do_not_contend() {
    let dir = tempfile::tempdir().expect("a temporary directory");
    let home = home(dir.path());
    let one = dir.path().join("repository-one");
    let two = dir.path().join("repository-two");

    let first = guard(lock::acquire(&home, &one, started()).expect("the lock is free"));
    let second = guard(lock::acquire(&home, &two, started()).expect("another workspace is free"));

    assert_ne!(
        lock::paths(&home, &one),
        lock::paths(&home, &two),
        "two workspaces are two names, or they are one lock wearing two",
    );
    // The same root, spelled again rather than the same binding read twice: the
    // claim is that the name is a function of the path's bytes, which is what two
    // separately-built `io` binaries have to agree on before either can see the
    // other at all.
    let again = dir.path().join("repository-one");
    assert_eq!(
        lock::paths(&home, &one),
        lock::paths(&home, &again),
        "one workspace is always one name, or two `io` would take two locks and \
         neither would ever notice the other",
    );

    drop(first);
    drop(second);
}

/// The record survives its own text, which is what makes the refusal readable by
/// a version of `io` that is not the one that wrote it.
#[test]
fn the_owner_record_round_trips_through_the_plain_text_it_is_stored_as() {
    let full = Owner {
        pid: Some(4242),
        root: Some(PathBuf::from("/Users/someone/work/io-cli")),
        version: Some("0.23.0".to_string()),
        started: Some(at(1_700_000_000)),
    };
    assert_eq!(Owner::parse(&full.render()), full);

    // Every field absent writes an empty record rather than four empty values,
    // so what comes back is an absence and not four fields that say nothing.
    let empty = Owner::default();
    assert!(empty.render().is_empty());
    assert_eq!(Owner::parse(&empty.render()), empty);

    // And a partial one, which is what an older version's record looks like.
    let partial = Owner {
        pid: Some(9),
        ..Owner::default()
    };
    assert_eq!(Owner::parse(&partial.render()), partial);

    for line in full.render().lines() {
        assert!(
            line.contains(" = "),
            "each fact is one `name = value` line: {line:?}"
        );
    }
}

/// **N3.** The lock and the record it names belong to the operator alone.
///
/// The record carries a workspace path, and it sits in the directory
/// `home::create` already makes `0700` around a file holding a credential. The
/// mode is set at creation, the way `settings::write` sets it, so there is no
/// window in which either file is on disk world-readable.
#[cfg(unix)]
#[test]
fn n3_the_lock_and_its_record_are_readable_by_their_owner_alone() {
    use std::os::unix::fs::PermissionsExt;

    let dir = tempfile::tempdir().expect("a temporary directory");
    let home = home(dir.path());
    let root = dir.path().join("repository");

    let held = guard(lock::acquire(&home, &root, started()).expect("the lock is free"));
    let (lock_path, owner_path) = lock::paths(&home, &root);

    for path in [&lock_path, &owner_path] {
        let mode = std::fs::metadata(path)
            .unwrap_or_else(|error| panic!("{} exists: {error}", path.display()))
            .permissions()
            .mode();
        assert_eq!(
            mode & 0o777,
            0o600,
            "{} is not the operator's alone",
            path.display(),
        );
    }

    assert_eq!(
        std::fs::metadata(&home)
            .expect("the home")
            .permissions()
            .mode()
            & 0o777,
        0o700,
        "and `acquire` made the home with `home::create`, rather than with a bare \
         `create_dir_all` that would leave it world-readable",
    );

    drop(held);
}
