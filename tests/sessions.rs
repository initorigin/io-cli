//! The session list behind `/resume`.
//!
//! Two facts are on trial here, and they are the ones a resume picker gets wrong
//! quietly rather than loudly.
//!
//! **F1 — the list comes out of the store, newest first, and io-cli writes
//! nothing down to produce it.** io-harness has no "enumerate sessions" call, so
//! the module walks `Store::runs` (newest run id first) back to the sessions those
//! runs served. The tempting alternative — an index of session ids kept in
//! io-cli's own config — would be a second answer to a question the store already
//! answers, and would be wrong the moment a second copy of the binary opened a
//! session. The tests below therefore seed only the store and assert only what the
//! store can produce; nothing here writes a file, and the last test asserts the
//! module itself cannot.
//!
//! **F2 — the scan is bounded, and the picker SAYS when the bound bit.** A
//! truncated list that does not announce itself reads as a complete one, and an
//! operator who cannot see their session concludes it is gone rather than that it
//! is off the end. Both halves are asserted: the cut case, and — the half that
//! actually has teeth — the *uncut* case, because a function that always claimed
//! it had cut would sail through a test that only checked the cut.
//!
//! **No clock.** `Recent::at` is the store's own stamp, sliced. It is not a
//! relative age, precisely so that neither the module nor this file has to read a
//! clock — which `tests/timing.rs` forbids outright, and which a resume picker is
//! the most tempting surface yet shipped to break.

use io_cli::glyphs::UNICODE;
use io_cli::sessions::{cut_note, recent, rows, MAX_RUNS_SCANNED, MAX_SESSIONS};
use io_harness::Store;

/// A workspace path longer than eighty columns, whose identifying segment is at
/// the very end — which is the shape every real workspace on one machine has.
const DEEP_ROOT: &str =
    "/Users/someone/Documents/Projects/archive/2026/quarter-three/checkouts/io-cli-resume";

/// The last segment of [`DEEP_ROOT`]: the part that tells one workspace from
/// another, and therefore the part a shortened path has to keep.
const DEEP_TAIL: &str = "io-cli-resume";

/// One turn on an existing session, chained onto whatever its newest turn is.
///
/// A run per turn, because that is the shape the product writes: `Session::turn`
/// starts a run and records the turn under it, and the run id is the only thing
/// the two halves both know. Fixtures that skipped the run would be testing a
/// store no version of io-cli can produce.
fn add_turn(store: &Store, session: i64, prompt: &str) -> i64 {
    let parent = store
        .session_turns(session)
        .expect("a session's turns are readable")
        .last()
        .map(|turn| turn.id);
    add_turn_from(store, session, parent, prompt)
}

/// One turn on an existing session, hanging off a caller-chosen parent — how a
/// branch is written, and the only reason this is separate from [`add_turn`].
fn add_turn_from(store: &Store, session: i64, parent: Option<i64>, prompt: &str) -> i64 {
    let run = store.start_run(prompt, "io-cli").expect("a run starts");
    let turn = store
        .record_turn(session, parent, run, prompt)
        .expect("a turn records");
    store
        .finish_turn(turn, Some("done"), "completed")
        .expect("a turn finishes");
    store
        .set_session_head(session, Some(turn))
        .expect("the head moves to the newest turn");
    turn
}

/// A session over `root` that has already run one turn per prompt, in order.
fn seed(store: &Store, root: &str, prompts: &[&str]) -> i64 {
    let session = store.create_session(root).expect("a session opens");
    for prompt in prompts {
        add_turn(store, session, prompt);
    }
    session
}

fn store() -> Store {
    Store::memory().expect("an in-memory store opens")
}

#[test]
fn f1_a_session_that_never_ran_a_turn_is_not_offered() {
    let store = store();
    let ran = seed(&store, "/work/one", &["fix the parser"]);
    let also_ran = seed(&store, "/work/two", &["write the changelog"]);
    // Opened and abandoned: `create_session` and nothing else. This is asserted
    // deliberately rather than left to chance — there is nothing in an empty
    // session to resume, so offering it would be offering a row that does
    // nothing when chosen, which is worse than not offering it at all.
    let never_ran = store
        .create_session("/work/three")
        .expect("a session opens");

    let (list, cut) = recent(&store).expect("the list reads");
    let ids: Vec<i64> = list.iter().map(|session| session.id).collect();

    assert_eq!(
        ids.len(),
        2,
        "only the sessions that ran a turn belong in the list: {list:?}",
    );
    assert!(ids.contains(&ran) && ids.contains(&also_ran), "got {ids:?}");
    assert!(
        !ids.contains(&never_ran),
        "a session with no turn has nothing to resume, so it must not be listed: {ids:?}",
    );
    assert!(!cut, "three sessions is not a cut list");
}

#[test]
fn f1_recency_is_by_newest_run_not_by_session_id() {
    let store = store();
    let alpha = seed(&store, "/work/alpha", &["start alpha"]);
    let beta = seed(&store, "/work/beta", &["start beta"]);

    let (list, _) = recent(&store).expect("the list reads");
    assert_eq!(
        list.iter().map(|s| s.id).collect::<Vec<_>>(),
        vec![beta, alpha],
        "the session that ran last comes first",
    );

    // The assertion that actually pins the behaviour. Ordering by session id
    // would also have produced [beta, alpha] above, so that first check alone
    // proves nothing. Running one more turn in the OLDER session moves it to the
    // front only if the order comes from the runs — which is what `Store::runs`
    // returning newest-first buys, and why this module needs no sort.
    add_turn(&store, alpha, "carry on with alpha");

    let (list, _) = recent(&store).expect("the list reads");
    assert_eq!(
        list.iter().map(|s| s.id).collect::<Vec<_>>(),
        vec![alpha, beta],
        "the session touched most recently must lead, whatever its id",
    );
}

#[test]
fn f1_a_row_carries_the_root_the_turn_count_and_the_first_prompt() {
    let store = store();
    let session = seed(
        &store,
        "/work/parser",
        &["fix the tokenizer", "now fix the lexer", "and the tests"],
    );

    let (list, _) = recent(&store).expect("the list reads");
    let row = &list[0];

    assert_eq!(row.id, session, "the id is what `Session::reopen` takes");
    assert_eq!(
        row.root, "/work/parser",
        "the workspace comes from the store"
    );
    assert_eq!(row.turns, 3);
    assert_eq!(
        row.prompt, "fix the tokenizer",
        "the FIRST prompt, not the last: what a session was opened to do is what \
         identifies it, and the newest prompt is usually a follow-up that means \
         nothing on its own",
    );

    // The stamp is the store's string, sliced to the minute — not a relative
    // age. "two minutes ago" would need the current time, and this module is not
    // allowed to ask for it.
    assert_eq!(
        row.at.chars().count(),
        16,
        "a stamp cut to the minute: {row:?}"
    );
    assert!(
        !row.at.contains("ago"),
        "a relative age would mean a clock read: {row:?}",
    );
    assert!(
        row.at.chars().nth(10) == Some(' ') && !row.at.contains('T'),
        "the stored `T` separator is replaced by a space for reading: {row:?}",
    );
}

#[test]
fn f1_the_turn_count_includes_the_turns_a_branch_left_behind() {
    let store = store();
    let session = store
        .create_session("/work/branch")
        .expect("a session opens");
    let first = add_turn_from(&store, session, None, "the question");
    add_turn_from(&store, session, Some(first), "the follow-up");
    // Branching: the head goes back to the first turn, so the second is no
    // longer on the path the conversation would replay.
    store
        .set_session_head(session, Some(first))
        .expect("the head moves back");

    let (list, _) = recent(&store).expect("the list reads");
    assert_eq!(
        list[0].turns, 2,
        "the count is every turn in the session's tree, not the path through it \
         — `Session::history` would say 1 here, and a picker that said 1 would be \
         telling the operator work had disappeared when it had only been branched \
         away from",
    );
}

#[test]
fn f2_the_list_stops_at_the_bound_and_admits_it() {
    let store = store();
    let mut seeded = Vec::new();
    for n in 0..MAX_SESSIONS + 5 {
        let prompt = format!("task {n}");
        seeded.push(seed(&store, &format!("/work/{n}"), &[prompt.as_str()]));
    }

    let (list, cut) = recent(&store).expect("the list reads");
    assert_eq!(
        list.len(),
        MAX_SESSIONS,
        "the walk stops at the bound rather than handing the picker a list nobody \
         can arrow to the bottom of",
    );
    assert!(cut, "a list that stopped at the bound must say so");

    // Newest first means the ones that fell off are the OLDEST, which is the only
    // acceptable direction for a bound like this.
    assert_eq!(list[0].id, *seeded.last().expect("seeded"), "got {list:?}");
    assert!(
        !list.iter().any(|s| s.id == seeded[0]),
        "the oldest session is the one dropped, not a newer one",
    );
}

#[test]
fn f2_a_full_list_with_a_session_that_ran_twice_is_not_a_cut_list() {
    let store = store();
    let mut seeded = Vec::new();
    for n in 0..MAX_SESSIONS {
        let prompt = format!("task {n}");
        seeded.push(seed(&store, &format!("/work/{n}"), &[prompt.as_str()]));
    }
    // The store now holds MORE runs than sessions, which is the ordinary state of
    // any workspace somebody came back to.
    add_turn(&store, seeded[0], "come back to the first one");

    let (list, cut) = recent(&store).expect("the list reads");

    // A regression test, and it is the one this pair got wrong first time.
    // Charging the bound at the top of the walk made this exact store — the list
    // full, and one more run belonging to a session already in it — report that
    // older sessions were hidden when every session in the store was on screen.
    // Nothing on the screen distinguishes a wrongly-cut list from a rightly-cut
    // one, so the defect was invisible to everything except this assertion.
    assert_eq!(
        list.len(),
        MAX_SESSIONS,
        "every session is listed: {list:?}"
    );
    assert!(
        !cut,
        "the whole store is on screen, so the walk must not claim it stopped short",
    );
    assert_eq!(cut_note(cut, list.len()), None, "and therefore no note");
    // The extra run also has to have moved its session to the front, which is the
    // same fact F1 pins — asserted here because it is what makes the run
    // reachable a second time at all.
    assert_eq!(list[0].id, seeded[0], "the session touched last leads");
}

#[test]
fn f2_a_list_that_fits_does_not_claim_it_was_cut() {
    let store = store();
    for n in 0..3 {
        let prompt = format!("task {n}");
        seed(&store, &format!("/work/{n}"), &[prompt.as_str()]);
    }

    let (list, cut) = recent(&store).expect("the list reads");
    assert_eq!(list.len(), 3);
    // The important half of the pair. A `cut` flag that is always true would pass
    // the test above and still be useless: every list would carry a note saying
    // older sessions exist, and the note would stop meaning anything the first
    // time it appeared under a complete list.
    assert!(
        !cut,
        "three sessions is the whole store, so nothing was cut: {list:?}",
    );
    assert_eq!(
        cut_note(cut, list.len()),
        None,
        "no note under a complete list"
    );
}

#[test]
fn n4_the_walk_gives_up_after_five_hundred_runs() {
    let store = store();

    // Seeded FIRST, so it is the oldest and therefore the last thing the walk
    // would reach — `Store::runs` is newest-first, which is what makes the walk
    // recency-ordered and also what puts this session out past the scan window.
    let buried = seed(&store, "/work/buried", &["the session nobody reaches"]);

    // One session, and the run bound is not the session bound: a single
    // conversation can put hundreds of runs between the walk's start and the next
    // session down. `add_turn_from` with the parent carried in a local, rather
    // than `add_turn`, because `add_turn` re-reads the whole session's turns to
    // find the parent — five hundred times that is a quadratic fixture, and the
    // cost being asserted here belongs to the production walk, not to the setup.
    let busy = store.create_session("/work/busy").expect("a session opens");
    let mut parent = None;
    for n in 0..MAX_RUNS_SCANNED + 2 {
        parent = Some(add_turn_from(&store, busy, parent, &format!("turn {n}")));
    }

    let (list, cut) = recent(&store).expect("the list reads");

    // The bound is a promise about cost: at most five hundred `turn_for_run`
    // lookups, whatever the store holds. A constant nothing checks is a constant
    // that stops being true the first time someone raises the session bound and
    // assumes this one scales with it.
    assert_eq!(
        list.len(),
        1,
        "only the session inside the scan window is listed: {list:?}",
    );
    assert_eq!(list[0].id, busy);
    assert!(
        !list.iter().any(|s| s.id == buried),
        "a session more than five hundred runs back is beyond the walk",
    );
    assert!(
        cut,
        "giving up on the scan is a cut list even though the session bound was \
         never reached — this is the half of the bound that has nothing to do \
         with how many sessions were found",
    );
    assert!(
        cut_note(cut, list.len()).is_some(),
        "and the operator is told, or the buried session reads as deleted",
    );
}

#[test]
fn f2_the_cut_note_names_how_many_are_shown_not_the_bound() {
    let note = cut_note(true, MAX_SESSIONS).expect("a cut list carries a note");
    assert!(
        note.contains(&MAX_SESSIONS.to_string()),
        "the note has to say HOW MANY are shown, or it is an apology rather than \
         information the operator can act on: {note:?}",
    );
    // And the number is the number of rows, never the constant. There are two
    // bounds and they cut to different sizes: a store where one workspace ran
    // five hundred turns trips the RUN bound and leaves a single row, so a note
    // quoting MAX_SESSIONS would tell that operator they were looking at twenty
    // sessions while one was on screen. The note is the whole of this
    // behaviour's honesty, so it may not be the part that is wrong.
    let one = cut_note(true, 1).expect("a run-bound cut carries a note too");
    assert!(
        one.contains('1') && !one.contains(&MAX_SESSIONS.to_string()),
        "a one-row cut list must say one, not twenty: {one:?}",
    );
    assert_eq!(
        cut_note(false, 3),
        None,
        "a complete list must carry no note at all — a permanent footnote is a \
         footnote nobody reads",
    );
}

#[test]
fn n5_at_eighty_columns_a_long_path_keeps_its_end() {
    let store = store();
    seed(
        &store,
        DEEP_ROOT,
        &[
            "rework the whole session picker so that it reads the store instead of \
           keeping an index of its own, which is a single line far wider than any \
           terminal",
        ],
    );

    let (list, _) = recent(&store).expect("the list reads");
    let fitted = rows(&list, 80, &UNICODE);
    let row = &fitted[0];
    let detail = row.detail.as_deref().expect("a session row has a detail");

    assert!(
        row.label.chars().count() <= 40,
        "the label takes at most half of eighty columns: {:?}",
        row.label,
    );
    assert!(
        row.label.ends_with('…'),
        "a long prompt is shortened from the RIGHT — the start of a prompt is what \
         identifies it: {:?}",
        row.label,
    );
    // Not eighty exactly: the picker fits the detail again at render time against
    // whatever the label left over. This is the bound that says the module handed
    // over something row-shaped rather than the raw path.
    assert!(
        detail.chars().count() <= 80,
        "the detail is fitted before it reaches the picker: {detail:?}",
    );

    assert!(
        detail.contains(DEEP_TAIL),
        "the end of the path is what tells one workspace from another and must \
         survive: {detail:?}",
    );
    assert!(
        detail.starts_with('…'),
        "a path is shortened from the LEFT: every workspace on one machine shares \
         its first segments, so shortening from the right would leave every row \
         reading `/Users/someone/Doc…`: {detail:?}",
    );
    assert!(
        !detail.contains("/Users/someone"),
        "the shared prefix is the part that goes: {detail:?}",
    );
}

#[test]
fn n5_the_detail_leads_with_the_workspace() {
    let store = store();
    seed(&store, DEEP_ROOT, &["one", "two"]);

    let (list, _) = recent(&store).expect("the list reads");
    let stamp = list[0].at.clone();
    let detail = rows(&list, 80, &UNICODE)[0]
        .detail
        .clone()
        .expect("a session row has a detail");

    let path_at = detail.find(DEEP_TAIL).expect("the path is in the detail");
    let turns_at = detail.find("turn").expect("the count is in the detail");
    let stamp_at = detail.find(&stamp).expect("the stamp is in the detail");

    // By POSITION, never by `contains` alone. All three facts are present in any
    // order, so a membership assertion would pass on a detail that had put the
    // timestamp first — and the picker shortens a detail from the RIGHT, so
    // whatever leads is the fact that survives a narrow terminal. Two sessions
    // over different workspaces are told apart by the path; nobody resumes by
    // timestamp.
    assert!(
        path_at < turns_at,
        "the workspace must come before the turn count: {detail:?}",
    );
    assert!(
        turns_at < stamp_at,
        "the stamp is the least load-bearing fact and goes last: {detail:?}",
    );
    assert!(
        detail.contains("2 turns"),
        "the count is pluralised for a session with more than one turn: {detail:?}",
    );
}

#[test]
fn the_module_reads_no_clock_and_cannot_reach_the_filesystem() {
    let path = concat!(env!("CARGO_MANIFEST_DIR"), "/src/sessions.rs");
    let source = std::fs::read_to_string(path).expect("src/sessions.rs is readable");

    // A scan that silently read nothing would pass for the wrong reason, which is
    // the failure mode a source-scanning test has to be defended against before
    // it is trusted to defend anything else.
    assert!(
        source.len() > 2000,
        "the scan read only {} bytes, which means it is not reading the module",
        source.len(),
    );
    assert!(
        source.contains("pub fn recent"),
        "the scan read something, but not this module",
    );

    // The needles are assembled from fragments at run time so that this file does
    // not match itself and needs no self-exemption. A scanner that has to skip its
    // own path has a hole in it exactly the size of the thing it skips — and
    // `tests/timing.rs` scans this file too.
    let forbidden = [
        (
            format!("std::{}", "fs"),
            "the session list is a read of the store and nothing else",
        ),
        (
            format!("{}::", "File"),
            "an index of session ids on disk is the design this module exists to refuse",
        ),
        (
            format!("{}::now", "SystemTime"),
            "the stamp is the store's string sliced, never a clock read",
        ),
        (
            format!("{}::now", "Instant"),
            "only the driver may read a clock",
        ),
        (
            format!(".{}()", "elapsed"),
            "a relative age would need the current time",
        ),
    ];

    let mut violations = Vec::new();
    for (needle, why) in &forbidden {
        for (number, line) in source.lines().enumerate() {
            if line.contains(needle) {
                violations.push(format!("src/sessions.rs:{}: {needle} — {why}", number + 1));
            }
        }
    }

    assert!(
        violations.is_empty(),
        "the session module grew a clock or a file:\n{}",
        violations.join("\n"),
    );
}
