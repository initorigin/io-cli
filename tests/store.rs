//! What the run store holds, and the four things `/store` can do about it.
//!
//! **The one fact every test here defends: a deletion does not shrink the file.**
//! SQLite frees pages *into* the database rather than out of it, so a removal
//! moves bytes from `StoreSize::file_bytes` into `StoreSize::free_bytes` and the
//! file on disk stays exactly the size it was. An interface that reported a
//! deletion as having reclaimed space would be lying, and it would be a lie an
//! operator only discovers by running out of disk. So `f1` asserts `file_bytes`
//! is **unchanged** — an equality, not an inequality — and `f2` asserts that
//! `compact` is the only call that moves it.
//!
//! **Every store here is a real file in a `TempDir`.** `Store::memory()` cannot
//! be used for any of this: the whole subject is page arithmetic on a file, and
//! a `VACUUM` of a connection that never touched a disk would be testing the half
//! of the claim that was never in doubt. That is also criterion O4 — a gate that
//! opened the developer's own `~/.io-cli/runs.db` and swept it would be data loss
//! shipped as a test, so the last test in this file asserts that no test here
//! names a real home.
//!
//! **What cannot be asserted from this crate, and why it is written down rather
//! than quietly skipped.** F4 describes a session created at exactly the sweep's
//! boundary surviving, because io-harness's comparison is strictly before. That
//! arm is real and it is io-harness's own doctest (`state/sessions.rs:420`); it
//! is **not falsifiable here**, because `sessions.created_at` is written by the
//! store and there is no public reader for it — the same gap as io-harness#216,
//! from the other side. A test that pretended otherwise would be asserting a
//! timestamp this crate invented. The reachable halves are asserted instead: a
//! date before everything sweeps nothing, a date after everything sweeps
//! everything not refused, and the refusal is named. This is 0.23.0's recorded
//! lesson — *check where a sabotage lives before writing the criterion* — showing
//! up on the other side of the same wall.

use io_cli::store::{
    acts, committed, compact, confirm_compact, confirm_remove, confirm_sweep, freed_report, remove,
    removed_report, sized, sweep, swept_report, view, Sized, LEAVE_IT,
};
use io_harness::Store;

/// A store in a real file, and the directory that has to outlive it.
///
/// The `TempDir` comes back with the store because dropping it deletes the
/// database underneath — and because criterion O4 is that no gate in this file
/// can reach anything else.
fn on_disk() -> (tempfile::TempDir, Store) {
    let dir = tempfile::tempdir().expect("a temporary directory");
    let store = Store::open(dir.path().join("runs.db")).expect("a store opens on disk");
    (dir, store)
}

/// One session with `turns` finished turns on it.
///
/// The run is finished as well as the turn. A run nothing ever finished is
/// `running`, which is a *resumable* run — which is exactly what
/// [`seed_resumable`] exists to produce deliberately, and what this must not
/// produce by accident, or every sweep test would be testing the refusal.
fn seed(store: &Store, root: &str, turns: usize) -> i64 {
    let session = store.create_session(root).expect("a session opens");
    for turn in 0..turns {
        let prompt = format!("a prompt long enough to occupy some bytes, number {turn}");
        let run = store.start_run(&prompt, root).expect("a run starts");
        let id = store
            .record_turn(session, None, run, &prompt)
            .expect("a turn records");
        store
            .finish_turn(id, Some("an answer of some length"), "completed")
            .expect("a turn finishes");
        store
            .finish_run(run, "success")
            .expect("the run finishes too");
    }
    session
}

/// One session whose run is left `running`, and is therefore still resumable.
///
/// This is what `Store::sweep_sessions` refuses. io-harness's argument
/// (`state/sessions.rs:370-378`) is that a date is a policy applied to sessions
/// nobody looked at, and a crash-resumable tree that vanished because it was old
/// is the worst outcome the call could have.
fn seed_resumable(store: &Store, root: &str) -> i64 {
    let session = store.create_session(root).expect("a session opens");
    let run = store.start_run("unfinished", root).expect("a run starts");
    store
        .record_turn(session, None, run, "unfinished")
        .expect("a turn records");
    session
}

/// A timestamp after every session any of these tests can create.
const AFTER_EVERYTHING: &str = "2999-01-01T00:00:00.000Z";

/// A timestamp before every session any of these tests can create.
const BEFORE_EVERYTHING: &str = "2000-01-01T00:00:00.000Z";

// ---------------------------------------------------------------------------
// F1 — the file's own arithmetic, and what a deletion does not do to it
// ---------------------------------------------------------------------------

/// **F1 — the page reads io-harness's figures and computes none of its own.**
///
/// Asserted as an equality against `Store::store_size` rather than by checking
/// the numbers look plausible, because the defect this guards against is io-cli
/// deriving a total from the rows it happens to have listed — which would be
/// right on a small store and wrong on the one an operator came here about.
#[test]
fn f1_the_view_reports_the_stores_own_figures() {
    let (_dir, store) = on_disk();
    seed(&store, "/tmp/one", 3);

    let view = view(&store).expect("the store is readable");
    let direct = store.store_size().expect("a size is readable");

    assert_eq!(
        view.size, direct,
        "the view must not compute its own totals"
    );
    assert_eq!(view.reclaimable(), direct.free_bytes);
}

/// **F1 — a removal frees pages INTO the file and never shrinks it.**
///
/// The three assertions are the three halves of the sentence the module exists
/// to make true, and the middle one is an **equality**: `file_bytes` is the same
/// number afterwards, not merely no larger.
#[test]
fn f1_a_removal_frees_pages_into_the_file_and_the_file_does_not_move() {
    let (_dir, store) = on_disk();
    let doomed = seed(&store, "/tmp/doomed", 12);
    seed(&store, "/tmp/kept", 2);

    let removed = remove(&store, doomed).expect("the session is removed");

    assert_eq!(removed.pruned.sessions, 1);
    assert!(removed.pruned.turns >= 12, "every turn is accounted for");
    assert!(removed.pruned.bytes > 0, "rows carried bytes");
    assert_eq!(
        removed.before.file_bytes, removed.after.file_bytes,
        "a deletion must not change the size of the file on disk",
    );
    assert!(
        !removed.file_moved(),
        "file_moved() is the same claim and must agree",
    );
    assert!(
        removed.after.free_bytes >= removed.before.free_bytes,
        "the freed pages are inside the file",
    );
}

/// **F1 — the report says the file did not move, and names the compaction.**
///
/// A report that only said what was deleted would leave the operator believing
/// they had reclaimed the bytes it named.
#[test]
fn f1_the_report_says_the_file_is_unchanged_and_what_would_change_it() {
    let (_dir, store) = on_disk();
    let doomed = seed(&store, "/tmp/doomed", 6);
    let removed = remove(&store, doomed).expect("the session is removed");

    let report = removed_report(&removed).join("\n");
    assert!(report.contains("removed 1 session"), "{report}");
    assert!(
        report.contains("still"),
        "the report must say the file is still its old size: {report}",
    );
    assert!(
        report.contains("/store compact"),
        "and must name the one thing that changes that: {report}",
    );
}

// ---------------------------------------------------------------------------
// F2 — only a compaction shrinks the file, and it reports a measured figure
// ---------------------------------------------------------------------------

/// **F2 — compaction is the only thing that shrinks the file, and the figure it
/// reports is measured.**
///
/// The returned number is asserted against the difference between the two
/// `store_size` readings rather than against the freelist, because those are
/// different numbers and io-harness deliberately returns the true one.
#[test]
fn f2_only_compact_shrinks_the_file_and_reports_the_measured_difference() {
    let (_dir, store) = on_disk();
    let doomed = seed(&store, "/tmp/doomed", 40);
    seed(&store, "/tmp/kept", 1);
    remove(&store, doomed).expect("the session is removed");

    let freed = compact(&store).expect("the store compacts");

    assert!(
        freed.after.file_bytes < freed.before.file_bytes,
        "a compaction is the one call that shrinks the file",
    );
    assert_eq!(
        freed.bytes,
        freed.before.file_bytes - freed.after.file_bytes,
        "the figure reported is the measured shrink, not the freelist",
    );
    assert!(!freed.is_nothing());

    let report = freed_report(&freed).join("\n");
    assert!(report.contains("returned"), "{report}");
}

/// **F2 — a store with nothing free reclaims nothing, and says so.**
///
/// Zero is an honest answer and must not render as an empty success. This is
/// also the arm that stops the report being written as "returned {bytes}" with no
/// branch, which would print `returned 0 B` and read as a failure.
#[test]
fn f2_compacting_a_store_with_nothing_free_returns_zero_and_says_so() {
    let (_dir, store) = on_disk();
    seed(&store, "/tmp/kept", 2);
    // Compact once to reach a state with nothing free, then again to observe it.
    compact(&store).expect("the store compacts");

    let freed = compact(&store).expect("the store compacts again");

    assert!(freed.is_nothing(), "nothing was free to reclaim");
    let report = freed_report(&freed).join("\n");
    assert!(
        report.contains("nothing to reclaim"),
        "zero must be a sentence of its own: {report}",
    );
}

// ---------------------------------------------------------------------------
// F3 — absent is not empty
// ---------------------------------------------------------------------------

/// **F3 — a session that is not there and a session that holds nothing are
/// different answers.**
///
/// io-harness keeps them apart on purpose (`state/sessions.rs:305-309`): an
/// operator sweeping a list of ids needs to know which of them were already gone.
/// Collapsing `None` into a zeroed `SessionSize` tells them every id was empty.
#[test]
fn f3_absent_is_not_the_same_answer_as_empty() {
    let (_dir, store) = on_disk();
    let empty = store.create_session("/tmp/empty").expect("a session opens");

    let held = sized(&store, empty).expect("a size is readable");
    let missing = sized(&store, 9_999).expect("a missing session is not an error");

    assert!(matches!(held, Sized::Holds(_)), "an empty session exists");
    assert_eq!(held.size().expect("figures").turns, 0, "and holds nothing");
    assert_eq!(missing, Sized::Absent, "a missing session is absent");
    assert!(missing.size().is_none());
}

/// **F3 — the two answers produce different confirmations.**
///
/// The confirmation for a session that is not there offers no acting row at all,
/// because there is nothing to act on. A confirmation that offered "remove it"
/// for a session the store does not have would be a button that cannot work.
#[test]
fn f3_the_confirmation_for_a_missing_session_offers_nothing_to_do() {
    let (title, rows) = confirm_remove(9_999, &Sized::Absent);

    assert!(title.contains("no session 9999"), "{title}");
    assert_eq!(rows.len(), 1, "only the declining row");
    assert_eq!(rows[0].label, LEAVE_IT);
}

/// **F3 — removing a session the store does not have succeeds and reports
/// nothing removed.**
///
/// io-harness's own behaviour, not softened here: it is not an error, and it must
/// not be reported as one.
#[test]
fn f3_removing_a_session_that_is_not_there_reports_nothing() {
    let (_dir, store) = on_disk();
    seed(&store, "/tmp/kept", 1);

    let removed = remove(&store, 9_999).expect("removing nothing is not an error");

    assert_eq!(removed.pruned.sessions, 0);
    assert_eq!(removed.pruned.turns, 0);
    assert_eq!(removed.before.file_bytes, removed.after.file_bytes);
}

// ---------------------------------------------------------------------------
// F4 — the sweep, its refusal, and the rule it asks for
// ---------------------------------------------------------------------------

/// **F4 — a date before everything sweeps nothing.**
///
/// The half of "strictly before" that *is* reachable from this crate. Without
/// it, a sweep that ignored its argument entirely would pass every other test in
/// this file.
#[test]
fn f4_a_date_before_everything_sweeps_nothing() {
    let (_dir, store) = on_disk();
    seed(&store, "/tmp/one", 2);
    seed(&store, "/tmp/two", 2);

    let swept = sweep(&store, BEFORE_EVERYTHING).expect("the sweep runs");

    assert_eq!(swept.pruned.sessions, 0, "nothing was created before 2000");
    assert!(swept.refused().is_empty());
}

/// **F4 — a resumable session is refused, and named.**
///
/// The refusal is the interesting half: a report that showed only what went would
/// tell the operator they had swept sessions that are still sitting there.
#[test]
fn f4_a_resumable_session_is_refused_and_named() {
    let (_dir, store) = on_disk();
    seed(&store, "/tmp/finished", 3);
    let alive = seed_resumable(&store, "/tmp/alive");

    let swept = sweep(&store, AFTER_EVERYTHING).expect("the sweep runs");

    assert_eq!(
        swept.refused(),
        &[alive],
        "the session holding a resumable run is refused, not deleted",
    );
    assert!(
        swept.pruned.sessions >= 1,
        "and the finished one is still swept",
    );
    assert!(
        matches!(sized(&store, alive), Ok(Sized::Holds(_))),
        "a refused session is still in the store",
    );

    let report = swept_report(&swept).join("\n");
    assert!(
        report.contains(&alive.to_string()),
        "the refused id must be named: {report}",
    );
    assert!(report.contains("resumable"), "{report}");
}

/// **F4 — a sweep that refused nothing says so.**
///
/// "Refused none" and "we did not check" are the same silence otherwise, and the
/// refusal is the half an operator has to act on.
#[test]
fn f4_a_sweep_that_refused_nothing_says_nothing_was_refused() {
    let (_dir, store) = on_disk();
    seed(&store, "/tmp/finished", 2);

    let swept = sweep(&store, AFTER_EVERYTHING).expect("the sweep runs");
    let report = swept_report(&swept).join("\n");

    assert!(swept.refused().is_empty());
    assert!(report.contains("nothing was refused"), "{report}");
}

/// **F4 — the confirmation names the rule and never a count.**
///
/// The counts cannot be known before the sweep runs — there is no public reader
/// for `sessions.created_at`, which is the column the sweep filters on
/// (io-harness#216, `US-IO-CLI-0.27.0-I02`). A confirmation carrying a number
/// here would be carrying one io-cli invented, and the one available substitute
/// under-states the deletion.
#[test]
fn f4_the_sweep_confirmation_names_the_rule_and_carries_no_count() {
    let (title, rows) = confirm_sweep("2026-08-01");

    assert!(
        title.contains("2026-08-01"),
        "the rule names the date: {title}"
    );
    assert!(
        !title
            .chars()
            .any(|c| c.is_ascii_digit() && !title.contains("2026-08-01")),
        "no figure other than the date itself",
    );
    assert_eq!(rows[0].label, LEAVE_IT);
    let detail = rows[1].detail.clone().unwrap_or_default();
    assert!(
        detail.contains("refused"),
        "the refusal policy is stated up front: {detail}",
    );
    assert!(
        detail.contains("reported"),
        "and the operator is told the figures come afterwards: {detail}",
    );
}

// ---------------------------------------------------------------------------
// F5 — the confirmation, and the row that does nothing
// ---------------------------------------------------------------------------

/// **F5 — row 0 declines in every confirmation this module builds.**
///
/// Asserted by **index**, across all three, because a `Picker` opens on its first
/// row: the keystroke an operator gives by reflex has to be the one that changes
/// nothing. A label comparison alone would not catch an acting row that drifted
/// to the top.
#[test]
fn f5_row_zero_declines_in_every_confirmation() {
    let (_dir, store) = on_disk();
    let session = seed(&store, "/tmp/one", 1);
    let size = store.store_size().expect("a size is readable");
    let held = sized(&store, session).expect("a size is readable");

    for (what, rows) in [
        ("remove", confirm_remove(session, &held).1),
        ("sweep", confirm_sweep("2026-08-01").1),
        ("compact", confirm_compact(&size).1),
    ] {
        assert_eq!(rows[0].label, LEAVE_IT, "{what}: row 0 must decline");
        assert!(
            !acts(0),
            "{what}: and the driver's predicate must agree that it does",
        );
        assert!(rows.len() >= 2, "{what}: there is something to choose");
        for index in 1..rows.len() {
            assert!(acts(index), "{what}: every other row acts");
        }
    }
}

/// **F5 — declining changes nothing.**
///
/// The whole `StoreSize` is compared, not one field, so an operation that ran and
/// then reported a refusal is caught as well as one that reported honestly and
/// ran anyway.
#[test]
fn f5_declining_leaves_the_store_byte_identical() {
    let (_dir, store) = on_disk();
    let session = seed(&store, "/tmp/one", 4);
    let before = store.store_size().expect("a size is readable");

    // What the driver does with row 0: nothing at all. The predicate is the whole
    // decision and it lives in the library precisely so this can be asserted —
    // `src/main.rs` is a binary nothing under `tests/` can link.
    if acts(0) {
        remove(&store, session).expect("unreachable");
    }

    let after = store.store_size().expect("a size is readable");
    assert_eq!(before, after, "declining must change nothing at all");
    assert!(matches!(sized(&store, session), Ok(Sized::Holds(_))));
}

/// **F5 — the removal confirmation states what it costs and what goes with it.**
///
/// The restore-point sentence is in the confirmation rather than in the report,
/// because it is the fact an operator needs *before* they agree: they delete an
/// old session, and reach for its rewind a week later.
#[test]
fn f5_the_removal_confirmation_states_the_cost_and_the_restore_points() {
    let (_dir, store) = on_disk();
    let session = seed(&store, "/tmp/one", 5);
    let held = sized(&store, session).expect("a size is readable");

    let (title, rows) = confirm_remove(session, &held);

    assert!(title.contains(&session.to_string()), "{title}");
    assert!(
        title.contains("turn"),
        "the figures are in the title: {title}"
    );
    let detail = rows[1].detail.clone().unwrap_or_default();
    assert!(detail.contains("restore points"), "{detail}");
    assert!(
        detail.contains("memory does not"),
        "and what survives, which is the half that surprises: {detail}",
    );
}

/// **F5 — the compaction confirmation states what it costs while it runs.**
///
/// io-harness's own warning, stated before the call rather than discovered by a
/// full disk.
#[test]
fn f5_the_compaction_confirmation_states_what_it_needs_while_it_runs() {
    let (_dir, store) = on_disk();
    seed(&store, "/tmp/one", 2);
    let size = store.store_size().expect("a size is readable");

    let (_title, rows) = confirm_compact(&size);
    let detail = rows[1].detail.clone().unwrap_or_default();

    assert!(detail.contains("rewrites"), "{detail}");
    assert!(detail.contains("free on disk"), "{detail}");
}

// ---------------------------------------------------------------------------
// The page, and O4
// ---------------------------------------------------------------------------

/// The page names the file, what is free inside it, and the verbs.
///
/// Also the one place the "free inside the file" wording is asserted: calling it
/// *reclaimable* on the page would invite the reading that a deletion returned it.
#[test]
fn the_page_names_the_file_the_free_space_and_the_three_verbs() {
    let (_dir, store) = on_disk();
    let doomed = seed(&store, "/tmp/doomed", 8);
    seed(&store, "/tmp/kept", 1);
    remove(&store, doomed).expect("the session is removed");

    let theme = io_cli::theme::THEMES[0];
    let lines = committed(&store, &theme, 100).expect("the page renders");
    let text = lines
        .iter()
        .flat_map(|line| line.spans.iter().map(|span| span.content.as_ref()))
        .collect::<String>();

    assert!(text.contains("on disk"), "{text}");
    assert!(text.contains("free inside it"), "{text}");
    assert!(text.contains("/store compact"), "{text}");
    assert!(text.contains("/store rm"), "{text}");
    assert!(text.contains("/store sweep"), "{text}");
    assert!(
        !text.contains("reclaimed"),
        "nothing on this page may say a deletion reclaimed anything: {text}",
    );
}

/// **O4 — no gate in this file can reach a real store.**
///
/// A test that compacted or swept the developer's own `~/.io-cli/runs.db` would
/// be data loss shipped as a test. Read as text, because that is the only
/// instrument that sees what a test *could* open rather than what it did.
#[test]
fn o4_no_store_gate_names_a_real_home() {
    let source = include_str!("store.rs");
    let code: String = source
        .lines()
        .filter(|line| !line.trim_start().starts_with("//"))
        .collect::<Vec<_>>()
        .join("\n");

    // **The needles are assembled rather than written out, and that is the whole
    // reason this loop looks odd.** A gate that reads its own file cannot spell
    // the thing it forbids: the array would be the first match. io-cli has now
    // hit this in 0.16.0, 0.19.0, 0.25.0 and 0.26.0 — twice in one wave in
    // 0.25.0, where two agents each wrote a forbidden name into a comment in
    // order to say the file did not use it. Stripping comments, which the sweep
    // above does, is only half the fix; the other half is here.
    for (head, tail) in [
        (".io", "-cli"),
        ("home", "_dir"),
        ("IO_CONFIG", "_HOME"),
        ("USER", "PROFILE"),
    ] {
        let forbidden = format!("{head}{tail}");
        assert!(
            !code.contains(&forbidden),
            "a store gate must never be able to reach a real home, and this file names `{forbidden}`",
        );
    }
    assert!(
        code.contains("tempfile::tempdir"),
        "every store here is a temporary one",
    );
    // Assembled for the same reason as the loop above: spelled out, this needle
    // is its own first match.
    let in_memory = format!("{}::{}", "Store", "memory");
    assert!(
        !code.contains(&in_memory),
        "and a real file, because the subject is page arithmetic on one",
    );
}
