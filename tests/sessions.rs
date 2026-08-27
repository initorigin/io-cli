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
//! **F10 — there is one bound left, and it is not a list length.** The twenty-row
//! session cap existed only because a picker that could not be typed at was a list
//! nobody could reach the bottom of; 0.7.0 filters, so the cap is gone and
//! `/resume` offers what the walk found. `MAX_RUNS_SCANNED` stays, because it
//! bounds the *cost* of the walk rather than the length of its result — a ceiling
//! of five hundred runs that leaves a single session on screen, which is exactly
//! why `cut_note` takes its number from what is drawn and never from a constant.
//!
//! **The index the picker returns is resolved in the library, and tested here.**
//! `/resume` and `/fork` used to read their id lists back inside `src/main.rs`,
//! which is `[[bin]] name = "io"` and which nothing under `tests/` can link — so
//! the one lookup that decides which session reopens was unsabotageable. It is
//! `sessions::pick` and `sessions::pick_turn` now, driven below through a real
//! `Picker` with a query typed into it, because the index is only interesting once
//! the drawn order and the caller's order have been made to disagree.
//!
//! **F1, the half added this release — every row says what its session stopped
//! on.** The list is chosen from, and until now every row looked alike: the
//! session holding an unanswered question was indistinguishable from the twenty
//! that finished, and the operator learned which was which by opening them. The
//! state comes off the session's newest run through `io_cli::resume::pending_for`
//! and is drawn on `Row::mark` — **not** in the label, which is the only field the
//! fuzzy matcher ranks, and **not** in the detail, which is the first thing a
//! narrow terminal drops. The tests below assert the mark is present *and* that
//! the same string is absent from both of the other two fields, because "it is on
//! the row somewhere" is what 0.16.0 asserted about a template and a skill, and
//! the distinction vanished at exactly the width that needed it.
//!
//! **F7 — the one pause that is reported and must never be offered.** A turn the
//! operator interrupted is recorded `cancelled`, which `finish_run` maps to a
//! *completed* status, and every io-harness resume entry point short-circuits on
//! it. So the most common way a turn stops is the one way it cannot be continued.
//! It gets a mark and a sentence naming `/fork` from the turn before, and the test
//! asserts the sentence does not offer to resume.
//!
//! **The stores here are real files.** These fixtures reach for `Store::open` on a
//! `TempDir` rather than `Store::memory`, because what is being read back is what
//! a run loop leaves in a database another process wrote — and where io-harness
//! exposes no way to *reach* a state, the fixture writes the row through the
//! public writer the run loop itself calls, so the row is the one production
//! produces rather than one shaped to suit the assertion.
//!
//! **No clock.** `Recent::at` is the store's own stamp, sliced. It is not a
//! relative age, precisely so that neither the module nor this file has to read a
//! clock — which `tests/timing.rs` forbids outright, and which a resume picker is
//! the most tempting surface yet shipped to break.

use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use io_cli::glyphs::{ASCII, UNICODE};
use io_cli::picker::{Outcome, Picker, Row};
use io_cli::sessions::{
    cut_note, mark, note, pick, pick_turn, recent, rows, turn_rows, DIED_MARK, ENDED_MARK,
    MAX_RUNS_SCANNED, PLAN_MARK, QUESTION_MARK, RECOVERY_MARK,
};
use io_harness::{Plan, PlanStep, Question, StepRecord, Store, ToolRecovery};

/// More sessions than the twenty-row cap 0.7.0 removed, so a list of this length
/// is itself the evidence the cap is gone. A literal rather than a constant: the
/// constant it used to be derived from is what these tests exist to prove absent,
/// and a fixture sized off the surviving bound would be sized off a run count.
const PAST_THE_OLD_CAP: usize = 25;

fn key(code: KeyCode) -> KeyEvent {
    KeyEvent::new(code, KeyModifiers::NONE)
}

/// Type `query` into `picker`, one character at a time, exactly as an operator
/// does — the filter is fed by keystrokes and there is no other way in.
fn typed(picker: &mut Picker, query: &str) {
    for character in query.chars() {
        picker.key(key(KeyCode::Char(character)));
    }
}

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
    // The RUN is closed as well as the turn, which this fixture did not do until
    // the state column existed. A run nothing ever finished is `running` in the
    // store — which is what a process that died mid-loop leaves behind — and it
    // reads as finished here only because it committed no step and so has nothing
    // to be resumed *from*. That is one accident away from every session in this
    // file drawing a `died` mark for reasons no test in it is about. The product
    // closes the run when the turn returns; so does this now, and the fixture
    // stops depending on a step count it never meant to assert.
    store
        .finish_run(run, "success")
        .expect("the run finishes with it");
    store
        .set_session_head(session, Some(turn))
        .expect("the head moves to the newest turn");
    turn
}

/// One more turn on `session`, with its run left **open** and its id returned.
///
/// The seam every state below is built through. A run is the thing a pause is
/// written under — the question, the plan and the tool journal all key on
/// `run_id`, none of them on a turn — so a fixture that could not name the newest
/// run could not put a session into any of these states at all.
///
/// The turn is deliberately not finished: a turn whose run is still waiting has
/// not returned anything, and finishing it here would seed a shape the product
/// never writes.
fn open_run(store: &Store, session: i64, prompt: &str) -> i64 {
    let parent = store
        .session_turns(session)
        .expect("a session's turns are readable")
        .last()
        .map(|turn| turn.id);
    let run = store.start_run(prompt, "io-cli").expect("a run starts");
    let turn = store
        .record_turn(session, parent, run, prompt)
        .expect("a turn records");
    store
        .set_session_head(session, Some(turn))
        .expect("the head moves to the newest turn");
    run
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

/// A store in a real file, and the directory that has to outlive it.
///
/// `Store::open` rather than `Store::memory` for the state tests: what they read
/// back is what a run loop committed to a database, and the writers those states
/// go through — `put_question`, `put_plan`, `open_attempt` — are the ones whose
/// whole purpose is surviving the process that wrote them. Reading them out of a
/// connection that never touched a disk would be testing the half of the claim
/// that was never in doubt.
///
/// The `TempDir` comes back with the store because dropping it deletes the
/// database underneath.
fn on_disk() -> (tempfile::TempDir, Store) {
    let dir = tempfile::tempdir().expect("a temporary directory");
    let store = Store::open(dir.path().join("runs.db")).expect("a store opens on disk");
    (dir, store)
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

/// Six sessions, one per state a newest run can be found in, seeded through the
/// writers the run loop itself calls.
///
/// Returned as pairs of session id and the mark that state calls for, in seeding
/// order, so the assertions read as a table rather than as six copies of one
/// paragraph. A state missing from this fixture is a state nothing watches — and
/// the way a state goes wrong is by being drawn as another state's mark, which
/// only a fixture holding all of them at once can catch.
fn seed_every_state(store: &Store) -> Vec<(i64, Option<&'static str>)> {
    // Waiting on an answer. `put_question` is the writer `run/dispatch.rs` calls
    // when nothing in the process could answer, and `awaiting_answer` is the
    // outcome the loop then finishes with. io-harness exposes no way to *reach*
    // this state without running a model, so the row is written through the
    // public writer that produces it — which is what makes it the row production
    // leaves rather than one shaped to suit the assertion.
    let asking = seed(store, "/work/importer", &["port the importer"]);
    let run = open_run(store, asking, "which database?");
    store
        .put_question(run, 4, &Question::new("postgres or sqlite?"))
        .expect("a question is written");
    store
        .finish_run(run, "awaiting_answer")
        .expect("and the run stops on it");

    // Waiting on a verdict about a plan. The plan row is written *before* the
    // gate is consulted, which is the whole of io-harness's durability claim for
    // it: a process that died between the proposal and the verdict leaves a row a
    // human can still answer, and that row is what this list has to see.
    let planning = seed(store, "/work/migration", &["rewrite the migration"]);
    let run = open_run(store, planning, "here is how I would do it");
    store
        .put_plan(
            run,
            2,
            &Plan::new([
                PlanStep::new("read the schema"),
                PlanStep::new("write the up and down"),
            ]),
        )
        .expect("a plan is written");
    store
        .finish_run(run, "awaiting_plan")
        .expect("and the run stops on it");

    // Waiting on a decision about a call io-harness cannot inspect. There is no
    // outcome to write and no status to set here: the pause *is* the open row in
    // the tool journal. `open_attempt` commits it outside every step transaction
    // precisely so it survives the death of the process making the call, and
    // `run/outcome.rs` reads exactly this back at each resume root.
    let recovering = seed(store, "/work/billing", &["settle the invoice"]);
    let run = open_run(store, recovering, "charge the customer");
    store
        .open_attempt(run, 6, "charge_card", ToolRecovery::Indeterminate)
        .expect("the journal takes the attempt")
        .expect("an indeterminate call is journalled at all");

    // Running, with a committed step and nothing pending: the process died
    // mid-loop and nothing ever closed the row. The step is written rather than
    // implied because it is what the run would resume *from* — a crashed run with
    // no committed work and one with six steps behind it are not the same offer.
    let crashed = seed(store, "/work/archive", &["reindex the archive"]);
    let run = open_run(store, crashed, "carry on with the reindex");
    store
        .record(run, &StepRecord::new(3, "wrote the index", "ok"))
        .expect("a step commits");

    // Finished, cleanly. The row that carries no mark at all.
    let finished = seed(store, "/work/changelog", &["write the changelog"]);

    // Ended by the operator. `Ctrl+C` sets the flag that returns `Flow::Cancel`,
    // io-harness records the outcome `cancelled`, and `finish_run` maps that to a
    // **completed** status — which is why every resume entry point short-circuits
    // on it, why it cannot be continued, and why it is reported rather than
    // offered.
    let ended = seed(store, "/work/columns", &["rename the columns"]);
    let run = open_run(store, ended, "and now the indexes");
    store
        .finish_run(run, "cancelled")
        .expect("an interrupted turn ends");

    vec![
        (asking, Some(QUESTION_MARK)),
        (planning, Some(PLAN_MARK)),
        (recovering, Some(RECOVERY_MARK)),
        (crashed, Some(DIED_MARK)),
        (finished, None),
        (ended, Some(ENDED_MARK)),
    ]
}

#[test]
fn f1_each_row_carries_the_mark_its_last_run_state_calls_for() {
    let (_dir, store) = on_disk();
    let expected = seed_every_state(&store);

    let (list, cut) = recent(&store).expect("the list reads");
    assert!(!cut, "six sessions is the whole store");
    assert_eq!(
        list.len(),
        expected.len(),
        "one row per session, whatever state its newest run stopped in — a state \
         this list cannot represent must not become a session it drops: {list:?}",
    );

    // Both glyph sets, because a mark is TEXT and not a glyph. It has to say the
    // same thing to a terminal that can draw nothing else, which is the whole
    // reason the state rides in `Row::mark` as a word rather than as a colour or a
    // symbol on the row.
    for glyphs in [UNICODE, ASCII] {
        let built = rows(&list, 100, &glyphs);
        for (id, want) in &expected {
            let index = list
                .iter()
                .position(|session| session.id == *id)
                .unwrap_or_else(|| panic!("session {id} is listed, under {}", glyphs.name));
            let row = &built[index];
            assert_eq!(
                row.mark, *want,
                "the row for session {id} carries the mark its state calls for, \
                 under {}: {row:?}",
                glyphs.name,
            );
            assert_eq!(
                mark(&list[index].pending),
                *want,
                "and the row is drawn from `mark`, so the two cannot drift apart",
            );
            // The completed session is the one asserted to carry NOTHING, and it
            // is the assertion with teeth: a mark saying *fine* would be on almost
            // every row of almost every list, and a mark that is nearly always
            // there is one nobody reads. The absence is the signal.
            assert_eq!(
                note(&list[index].pending).is_some(),
                want.is_some(),
                "the prose and the mark report the same set of states, so a \
                 surface with room for a sentence and one without agree: {:?}",
                list[index].pending,
            );
        }

        // **The criterion.** The mark is present in the one field that survives,
        // and absent from both of the others. In the label it would be ranked by
        // the fuzzy matcher — every waiting session sharing a first word, and a
        // query for `plan` answering with the sessions *stopped on* a plan rather
        // than the ones asked about one. In the detail it would be the first thing
        // dropped on a narrow terminal, which is where a row is hardest to tell
        // apart: 0.16.0 marked a template and a skill in the detail alone and the
        // distinction vanished at exactly the width that needed it.
        for row in &built {
            let Some(mark) = row.mark else { continue };
            assert!(
                !row.label.contains(mark),
                "the state must not be in the label, which is what the matcher \
                 ranks: {row:?}",
            );
            let detail = row.detail.as_deref().expect("a session row has a detail");
            assert!(
                !detail.contains(mark),
                "the state must not be in the detail, which is the first thing a \
                 narrow terminal drops: {row:?}",
            );
        }
    }

    // Every mark is ASCII, by the same rule the memory page's four are held to: a
    // mark is text rather than a colour, so it has to survive `NO_COLOR`,
    // `--plain` and a terminal whose locale does not claim UTF-8 unchanged.
    for (_, want) in &expected {
        let Some(mark) = want else { continue };
        assert!(
            mark.chars().all(|character| character.is_ascii_graphic()),
            "a mark that cannot be drawn is a state nobody is told about: {mark:?}",
        );
    }
}

#[test]
fn f7_a_turn_the_operator_ended_is_reported_and_never_offered_as_resumable() {
    let (_dir, store) = on_disk();
    let session = seed(&store, "/work/columns", &["draft the release notes"]);
    let run = open_run(&store, session, "and now rewrite the summary");
    store
        .finish_run(run, "cancelled")
        .expect("an interrupted turn ends");

    let (list, _) = recent(&store).expect("the list reads");
    let pending = &list[0].pending;

    assert_eq!(
        mark(pending),
        Some(ENDED_MARK),
        "an ended turn is REPORTED — the alternative is a row that looks finished \
         and a piece of work the operator believes they stopped and cannot find",
    );

    let note = note(pending).expect("an ended turn has something to say");
    assert!(
        note.contains("you ended"),
        "the sentence says who ended it: the operator did this, and a passive \
         sentence would read as something that went wrong: {note:?}",
    );
    assert!(
        note.contains("/fork"),
        "and it names the neighbouring answer that actually works, from the turn \
         before — otherwise the report is a dead end: {note:?}",
    );
    // The half with teeth. `cancelled` maps to a COMPLETED status in the store and
    // every io-harness resume entry point short-circuits on a completed run,
    // returning the original outcome without driving. So a sentence offering to
    // resume would be offering something that cannot happen, and the operator
    // would learn it by choosing the row and watching nothing occur.
    assert!(
        !note.to_lowercase().contains("resum"),
        "an ended turn must never read as an offer to resume: {note:?}",
    );
}

#[test]
fn f1_a_cut_list_still_marks_its_rows_and_its_note_still_stands_for_no_session() {
    let (_dir, store) = on_disk();

    // Seeded first, so it is the oldest and sits past the scan window.
    let buried = seed(&store, "/work/buried", &["the session nobody reaches"]);

    // Runs that served no turn — what a headless run leaves — between the buried
    // session and the one on screen. They are skipped by the walk and still
    // COUNTED by it, which is the cheap way to reach the bound: the ceiling
    // charges for looking at a run, not for finding a session behind it.
    for n in 0..MAX_RUNS_SCANNED {
        store
            .start_run(&format!("headless {n}"), "io-cli")
            .expect("a run starts");
    }

    // And the newest run of all is a session waiting on an answer, so the one row
    // that survives the bound is a MARKED one. A cut list is exactly when the
    // operator most needs to know what is on it.
    let waiting = seed(&store, "/work/importer", &["port the importer"]);
    let run = open_run(&store, waiting, "which database?");
    store
        .put_question(run, 4, &Question::new("postgres or sqlite?"))
        .expect("a question is written");
    store
        .finish_run(run, "awaiting_answer")
        .expect("and the run stops on it");

    let (list, cut) = recent(&store).expect("the list reads");
    let ids: Vec<i64> = list.iter().map(|session| session.id).collect();

    assert_eq!(
        list.len(),
        1,
        "only the session inside the window: {list:?}"
    );
    assert_eq!(list[0].id, waiting);
    assert!(
        !ids.contains(&buried),
        "a session past five hundred runs is beyond the walk",
    );
    assert!(cut, "the walk gave up, so the list is cut");

    let mut built = rows(&list, 80, &UNICODE);
    assert_eq!(
        built[0].mark,
        Some(QUESTION_MARK),
        "the surviving row still says what it is waiting on: {built:?}",
    );

    // The note is appended by the driver as an ordinary row, and it is not a
    // session: no mark, because it has no state, and no id, because there is
    // nothing behind it to reopen. The filter ranks it against the session rows
    // like any other row and can put it under the marker, so `pick` has to keep
    // answering `None` for it however the rows above it are now drawn.
    built.push(Row::new(
        cut_note(cut, built.len()).expect("a cut list carries a note"),
    ));
    assert_eq!(
        built[1].mark, None,
        "the note stands for no session, so it carries no session's state",
    );
    assert_eq!(
        pick(&ids, 1),
        None,
        "and it resolves to nothing, which the driver answers with a sentence",
    );
}

#[test]
fn f10_the_list_is_as_long_as_the_walk_found_not_twenty() {
    let store = store();
    let mut seeded = Vec::new();
    for n in 0..PAST_THE_OLD_CAP {
        let prompt = format!("task {n}");
        seeded.push(seed(&store, &format!("/work/{n}"), &[prompt.as_str()]));
    }

    let (list, cut) = recent(&store).expect("the list reads");

    // The whole of F10. Twenty rows here would mean the cap is back — and it would
    // look entirely reasonable on screen, because a truncated list with a note
    // under it is exactly what this surface used to be. The cap existed only
    // because a picker that could not be typed at was a list nobody could reach
    // the bottom of; the picker filters as of this release, so the reason is gone
    // and so is the bound.
    assert_eq!(
        list.len(),
        PAST_THE_OLD_CAP,
        "every session the walk found is offered, not the first twenty: {list:?}",
    );
    assert_eq!(list[0].id, *seeded.last().expect("seeded"), "newest first");
    assert!(
        list.iter().any(|session| session.id == seeded[0]),
        "the oldest session is reachable now: nothing cut it off",
    );
    // The half with teeth. A list of twenty-five rows that still claimed it had
    // stopped short would put a permanent note under a complete list, and a note
    // that is always there is a note nobody reads — which is the same failure the
    // uncut assertions below guard, reached from the direction the removed bound
    // used to cause.
    assert!(!cut, "nothing was cut, so nothing may say it was");
    assert_eq!(cut_note(cut, list.len()), None, "and therefore no note");
}

#[test]
fn f2_a_full_list_with_a_session_that_ran_twice_is_not_a_cut_list() {
    let store = store();
    let mut seeded = Vec::new();
    for n in 0..PAST_THE_OLD_CAP {
        let prompt = format!("task {n}");
        seeded.push(seed(&store, &format!("/work/{n}"), &[prompt.as_str()]));
    }
    // The store now holds MORE runs than sessions, which is the ordinary state of
    // any workspace somebody came back to.
    add_turn(&store, seeded[0], "come back to the first one");

    let (list, cut) = recent(&store).expect("the list reads");

    // A regression test, and it is the one this pair got wrong first time. Under
    // the session bound this exact store — the list full, and one more run
    // belonging to a session already in it — reported that older sessions were
    // hidden when every session was on screen. The bound is gone, so the arm that
    // was wrong cannot be reached; what is asserted now is the property that
    // outlives it, that more runs than sessions is the ordinary state of a store
    // and is not a cut. Nothing on the screen distinguishes a wrongly-cut list
    // from a rightly-cut one, so this stays the only thing watching for it.
    assert_eq!(
        list.len(),
        PAST_THE_OLD_CAP,
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
    // lookups, whatever the store holds. It is also the bound F10 KEEPS, and this
    // is the test that says why — the walk stopped, and it stopped with one row
    // on screen. A ceiling of five hundred that leaves a list of one is not a list
    // length by any reading, which is the whole argument for removing the session
    // cap and none of the argument for removing this.
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
        "giving up on the scan is a cut list however few sessions were found — \
         this is the bound that has nothing to do with how long the list is, and \
         it is the assertion that fails if it is lifted along with the session cap",
    );
    assert!(
        cut_note(cut, list.len()).is_some(),
        "and the operator is told, or the buried session reads as deleted",
    );
}

#[test]
fn f10_the_cut_note_row_stands_for_no_session() {
    let store = store();
    seed(&store, "/work/one", &["alpha the parser"]);
    seed(&store, "/work/two", &["beta the lexer"]);

    let (list, _) = recent(&store).expect("the list reads");
    let ids: Vec<i64> = list.iter().map(|session| session.id).collect();

    // Every row the walk produced resolves, in the order the walk produced them.
    // `ids.first()` written in place of the lookup passes nothing here, which is
    // the point of the function existing at all: this arm lived in `src/main.rs`
    // until 0.7.0, and `src/main.rs` is `[[bin]] name = "io"` — no integration
    // test links it, so a swap there failed nothing.
    assert_eq!(pick(&ids, 0), Some(ids[0]), "the first row is the first id");
    assert_eq!(pick(&ids, 1), Some(ids[1]), "the second row is the second");

    // And the row past the end is the cut note, which stands for nothing. The
    // driver answers `None` with a sentence rather than by closing in silence.
    assert_eq!(
        pick(&ids, ids.len()),
        None,
        "the note carries no id, and a resume that guessed one would reopen a \
         session the operator never chose",
    );
}

#[test]
fn f10_a_filtered_choice_resolves_the_row_the_operator_saw() {
    let store = store();
    seed(&store, "/work/alpha", &["alpha the parser"]);
    seed(&store, "/work/beta", &["beta the lexer"]);
    seed(&store, "/work/gamma", &["gamma the tests"]);

    let (list, cut) = recent(&store).expect("the list reads");
    let ids: Vec<i64> = list.iter().map(|session| session.id).collect();
    let mut built = rows(&list, 80, &UNICODE);
    assert_eq!(cut_note(cut, built.len()), None, "three sessions is no cut");

    // The picker the driver builds, keystroke for keystroke. Newest first, so
    // `alpha` was seeded first and is the LAST row — which is what makes this
    // assertion able to fail: a resolution that read the chosen index against the
    // filtered list would answer with `gamma`, the row that sits at position zero
    // of what is drawn, and would do it silently.
    let mut picker = Picker::new("Resume which session?", built.clone());
    typed(&mut picker, "alpha");
    assert_eq!(picker.matching(), 1, "one prompt is spelled that way");
    let Outcome::Chosen(index) = picker.key(key(KeyCode::Enter)) else {
        panic!("Enter over a matching row chooses it");
    };
    assert_eq!(
        pick(&ids, index),
        Some(ids[2]),
        "the row the operator was looking at is the session that reopens",
    );

    // The hole the filter opened, and the reason `pick` may not answer an index
    // it cannot honour. The note is appended as an ordinary row, so it is ranked
    // against the sessions like any other and a query can put it under the marker
    // — where before 0.7.0 it was last and effectively unreachable.
    built.push(Row::new(
        cut_note(true, built.len()).expect("a cut list carries a note"),
    ));
    let mut picker = Picker::new("Resume which session?", built);
    typed(&mut picker, "older");
    assert_eq!(
        picker.matching(),
        1,
        "no prompt here is spelled that way, so the note is what is left",
    );
    let Outcome::Chosen(index) = picker.key(key(KeyCode::Enter)) else {
        panic!("Enter over the note chooses it, because it is a row");
    };
    assert_eq!(index, ids.len(), "the note is the row past the last id");
    assert_eq!(
        pick(&ids, index),
        None,
        "and it resolves to nothing, which the driver has to SAY — a picker that \
         closed here and did nothing is indistinguishable from a resume that failed",
    );
}

#[test]
fn f10_a_forked_row_carries_the_turn_number_it_was_drawn_with() {
    let store = store();
    let session = seed(
        &store,
        "/work/fork",
        &["the first", "the second", "the third"],
    );
    let turns = store.session_turns(session).expect("the turns read");
    let ids: Vec<i64> = turns.iter().map(|turn| turn.id).collect();

    // The id and the number come back together because they are one index read
    // once. They were read twice until 0.7.0 — the id in the driver's match arm,
    // the number in the sentence that arm printed — and a sentence naming a turn
    // the branch did not take is a lie about what just happened.
    for (index, drawn) in turn_rows(&turns, 80, &UNICODE).iter().enumerate() {
        let (id, number) = pick_turn(&ids, index).expect("every fork row is a turn");
        assert_eq!(id, ids[index], "the row branches from the turn it names");
        let detail = drawn.detail.as_deref().expect("a turn row has a detail");
        assert!(
            detail.starts_with(&format!("turn {number}")),
            "the number the operator was told is the number on the row: {detail:?}",
        );
    }

    // Numbered from one, because a turn id is a database key and nobody counts in
    // database keys. Asserted outright rather than left to the loop, which would
    // pass just as happily on two copies of the same off-by-one.
    assert_eq!(pick_turn(&ids, 0).map(|(_, n)| n), Some(1));
    assert_eq!(
        pick_turn(&ids, ids.len()),
        None,
        "`/fork` has no note row, so this is unreachable rather than impossible — \
         and an index with no turn behind it must not branch from anything",
    );
}

#[test]
fn f10_a_filtered_fork_choice_resolves_the_turn_the_operator_saw() {
    let store = store();
    let session = seed(
        &store,
        "/work/fork",
        &[
            "describe the parser",
            "rename the lexer",
            "delete the tests",
        ],
    );
    let turns = store.session_turns(session).expect("the turns read");
    let ids: Vec<i64> = turns.iter().map(|turn| turn.id).collect();

    // `/fork` opens on the newest turn, which is what the driver's `selecting`
    // does — so the marker starts on row 2 and the query has to move it. A
    // resolution that read the filtered position would answer row 0 here, and row
    // 0 is `describe the parser`: branching from the wrong turn, silently, having
    // been asked for the one in the middle.
    let mut picker = Picker::new("Continue from which turn?", turn_rows(&turns, 80, &UNICODE))
        .selecting(ids.len() - 1);
    typed(&mut picker, "rename");
    assert_eq!(picker.matching(), 1, "one prompt is spelled that way");
    let Outcome::Chosen(index) = picker.key(key(KeyCode::Enter)) else {
        panic!("Enter over a matching row chooses it");
    };
    assert_eq!(
        pick_turn(&ids, index),
        Some((ids[1], 2)),
        "the middle turn, and the number the row was drawn with",
    );
}

#[test]
fn f10_the_cut_note_names_how_many_are_shown_not_the_bound() {
    let note = cut_note(true, 7).expect("a cut list carries a note");
    assert!(
        note.contains('7'),
        "the note has to say HOW MANY are shown, or it is an apology rather than \
         information the operator can act on: {note:?}",
    );
    // And the number is the number of rows, never a constant. Removing the session
    // bound did not make the surviving one safe to quote — it made it worse. The
    // run ceiling counts RUNS, so a store where one workspace ran five hundred
    // turns leaves a single row, and a note quoting the ceiling would tell that
    // operator they were looking at five hundred sessions while one was on screen.
    // 0.4.0 paid for this once with a run-bound cut that claimed twenty.
    let one = cut_note(true, 1).expect("a run-bound cut carries a note too");
    assert!(
        one.contains('1') && !one.contains(&MAX_RUNS_SCANNED.to_string()),
        "a one-row cut list must say one, not five hundred: {one:?}",
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
