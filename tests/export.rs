//! Taking the work out of the terminal.
//!
//! **The single assertion this file exists for is a byte comparison.** A run's
//! canonical trace is valuable only because it is canonical: io-harness excludes
//! wall-clock stamps, measured durations, an argv's ephemeral tempdir and
//! `AUTOINCREMENT` ids from it so that two runs of one case can be compared, and
//! its own documentation says each exclusion is a promise rather than a
//! convenience. A trace io-cli pretty-printed would compare against nothing.
//! It is pipe-delimited text and not JSON, which this release's plan had wrong
//! until a sabotage that killed nothing exposed it — see `US-IO-CLI-0.27.0-I03`.
//!
//! So `f8_the_trace_is_written_byte_for_byte` compares **strings**, not parsed
//! values. Parsing both sides and comparing them is the test that looks more
//! rigorous and is the one that passes on the defect: a reformatted document
//! parses to the same values, which is exactly the thing that must fail.
//!
//! The markdown half is asserted on content and order rather than on formatting,
//! because nothing reads it back — it is for a person and for a diff, it carries
//! no schema version, and this product parses it nowhere.

use io_cli::export::{
    confirm, conversation, conversation_path, occupied, report, trace, trace_path, write, Refused,
};
use io_harness::tools::{Workspace, Wrote};
use io_harness::Store;

/// A store in a real file, plus a workspace to write into.
///
/// One `TempDir` for both, because an export writes into the workspace the
/// session is running over and the point of the last test here is that it cannot
/// write anywhere else.
fn fixture() -> (tempfile::TempDir, Store, Workspace) {
    let dir = tempfile::tempdir().expect("a temporary directory");
    let store = Store::open(dir.path().join("runs.db")).expect("a store opens on disk");
    let workspace = Workspace::new(dir.path());
    (dir, store, workspace)
}

/// One session with a finished turn per prompt, and the run ids in order.
///
/// **Every run records two steps, and that is not decoration.** The first
/// version of this fixture recorded none, and `Store::canonical_trace` is built
/// entirely from `steps` and `context_events` (`state/trace.rs:1019-1036`) — so
/// it returned the empty string, and the byte-for-byte gate below was comparing
/// `""` against `""`. It passed, and it would have gone on passing over any
/// defect whatsoever.
///
/// It was found because the named sabotage — pretty-print the trace — killed
/// **nothing**, which is the whole reason a sabotage is executed rather than
/// described. A gate that cannot fail is worse than a missing one, because it
/// reads on the page as coverage.
fn seed(store: &Store, prompts: &[(&str, Option<&str>)]) -> (i64, Vec<i64>) {
    let session = store.create_session("/tmp/work").expect("a session opens");
    let mut runs = Vec::new();
    for (prompt, reply) in prompts {
        let run = store.start_run(prompt, "/tmp/work").expect("a run starts");
        for step in 1..=2u32 {
            store
                .record(
                    run,
                    &io_harness::StepRecord {
                        step,
                        decision: format!("write_file ok on step {step}"),
                        result: format!("wrote src/thing-{step}.rs"),
                        prompt: prompt.to_string(),
                        tool_call: format!("write_file:{{\"path\":\"src/thing-{step}.rs\"}}"),
                        tokens: 128 * u64::from(step),
                    },
                )
                .expect("a step records");
        }
        let turn = store
            .record_turn(session, None, run, prompt)
            .expect("a turn records");
        // A `None` reply is a turn that did not finish, and it is written as one.
        // The turn is still closed, because an unfinished *reply* and an
        // unfinished *turn* are different states and only the first is on trial.
        store
            .finish_turn(turn, *reply, "completed")
            .expect("a turn finishes");
        store.finish_run(run, "success").expect("the run finishes");
        runs.push(run);
    }
    (session, runs)
}

// ---------------------------------------------------------------------------
// F8 — the conversation, in order
// ---------------------------------------------------------------------------

/// **F8 — every prompt and every reply appears, in the store's own order.**
///
/// Order is asserted by position in the rendered string rather than by counting
/// occurrences, because a document that carried all six strings shuffled would
/// satisfy a count and would be useless to read.
#[test]
fn f8_every_prompt_and_reply_appears_in_the_stores_order() {
    let (_dir, store, _ws) = fixture();
    let (session, _runs) = seed(
        &store,
        &[
            ("the first question", Some("the first answer")),
            ("the second question", Some("the second answer")),
            ("the third question", Some("the third answer")),
        ],
    );

    let markdown = conversation(&store, session)
        .expect("the conversation is readable")
        .expect("there is one");

    let mut at = 0;
    for needle in [
        "the first question",
        "the first answer",
        "the second question",
        "the second answer",
        "the third question",
        "the third answer",
    ] {
        let found = markdown[at..]
            .find(needle)
            .unwrap_or_else(|| panic!("`{needle}` is missing or out of order:\n{markdown}"));
        at += found + needle.len();
    }
}

/// **F8 — a turn with no reply is written as a turn with no reply.**
///
/// `Turn::reply` is `Option<String>` and `None` means the turn did not finish.
/// An empty section would make an interrupted conversation read as one the agent
/// had nothing to say to.
#[test]
fn f8_a_turn_that_never_answered_says_so_rather_than_showing_an_empty_reply() {
    let (_dir, store, _ws) = fixture();
    let (session, _runs) = seed(
        &store,
        &[("answered", Some("here you are")), ("never answered", None)],
    );

    let markdown = conversation(&store, session)
        .expect("the conversation is readable")
        .expect("there is one");

    assert!(markdown.contains("here you are"));
    assert!(
        markdown.contains("did not finish"),
        "an unanswered turn is named, not blank:\n{markdown}",
    );
}

/// A session with no turns is nothing to export, and says so.
///
/// A file asserting the session was empty is worse than no file: the session may
/// simply not have started.
#[test]
fn a_session_with_no_turns_is_refused_rather_than_written_empty() {
    let (_dir, store, _ws) = fixture();
    let session = store.create_session("/tmp/work").expect("a session opens");

    assert_eq!(
        conversation(&store, session).expect("readable"),
        None,
        "there is no conversation to write",
    );
    assert!(Refused::Nothing.said().contains("nothing to export"));
}

// ---------------------------------------------------------------------------
// F8 — the trace, byte for byte
// ---------------------------------------------------------------------------

/// **F8 — the trace file is byte-identical to `Store::canonical_trace`.**
///
/// A string comparison, deliberately. See the module note: parsing both sides
/// and comparing values is the test that passes on the defect.
#[test]
fn f8_the_trace_is_written_byte_for_byte() {
    let (_dir, store, workspace) = fixture();
    let (_session, runs) = seed(&store, &[("do the thing", Some("done"))]);
    let run = runs[0];

    let ours = trace(&store, run).expect("a trace is readable");
    let harness = store.canonical_trace(run).expect("a trace is readable");
    // **The gate that stops this one going vacuous again.** A run with no steps
    // has an empty canonical trace, and every assertion below is satisfied by
    // two empty strings. Assert there is something to compare before comparing
    // it — 0.26.0's lesson that a gate can go vacuous without going red.
    assert!(
        !harness.is_empty(),
        "the fixture must produce a trace, or every assertion here is `\"\" == \"\"`",
    );
    assert!(harness.contains("step 1"), "and a real one: {harness}");
    assert_eq!(ours, harness, "the trace is passed through untouched");

    let path = trace_path(run);
    let written = write(&workspace, &path, &ours).expect("the trace is written");
    assert_eq!(written.wrote, Wrote::Created);

    let back = std::fs::read_to_string(workspace.root().join(&path)).expect("the file is there");
    assert_eq!(
        back, harness,
        "the file on disk is io-harness's own string, byte for byte",
    );
}

/// The trace is not reformatted, and this is the assertion that says so
/// independently of the equality above.
///
/// A pretty-printer is the specific defect: it would change the whitespace and
/// nothing else, so a test that only compared parsed values would not see it.
#[test]
fn the_trace_is_not_reformatted() {
    let (_dir, store, _ws) = fixture();
    let (_session, runs) = seed(&store, &[("do the thing", Some("done"))]);

    let ours = trace(&store, runs[0]).expect("a trace is readable");
    let harness = store.canonical_trace(runs[0]).expect("a trace is readable");

    assert!(!harness.is_empty(), "there is something to reformat");
    assert_eq!(
        ours.len(),
        harness.len(),
        "a reformatting changes the length even when it changes no value",
    );
    assert_eq!(
        ours.matches('\n').count(),
        harness.matches('\n').count(),
        "and it changes the line count, which is what a pretty-printer does",
    );
}

// ---------------------------------------------------------------------------
// F8 — the write goes through the workspace
// ---------------------------------------------------------------------------

/// **F8 — a path outside the workspace is refused rather than written.**
///
/// The refusal is io-harness's, through `Workspace::write_file`, and is asserted
/// by the file's absence as well as by the error — an error that had already
/// written the file would be worse than no error at all.
#[test]
fn f8_a_path_outside_the_workspace_is_refused() {
    let (_dir, store, workspace) = fixture();
    let (session, _runs) = seed(&store, &[("a question", Some("an answer"))]);
    let markdown = conversation(&store, session)
        .expect("readable")
        .expect("there is one");

    let escape = "../escaped.md";
    let refused = write(&workspace, escape, &markdown);

    assert!(refused.is_err(), "a path above the root is refused");
    assert!(
        !workspace.root().join("..").join("escaped.md").exists(),
        "and nothing is written outside the workspace",
    );
}

/// An existing file is refused rather than overwritten.
///
/// An export is a snapshot; the next one is a different snapshot. A command that
/// silently replaced the first would destroy what the operator was about to
/// compare against.
#[test]
fn an_existing_file_is_refused_rather_than_overwritten() {
    let (_dir, store, workspace) = fixture();
    let (session, _runs) = seed(&store, &[("a question", Some("an answer"))]);
    let path = conversation_path(session);
    let markdown = conversation(&store, session)
        .expect("readable")
        .expect("there is one");

    assert!(
        !occupied(&workspace, &path).expect("askable"),
        "nothing yet"
    );
    write(&workspace, &path, &markdown).expect("the first export is written");
    assert!(
        occupied(&workspace, &path).expect("askable"),
        "and now something is there",
    );

    let said = Refused::Exists(path.clone()).said();
    assert!(said.contains(&path), "the refusal names the file: {said}");
    assert!(said.contains("snapshot"), "and says why: {said}");
}

/// The proposed paths are distinct per session and per run, so two exports do
/// not collide before anyone has been asked anything.
#[test]
fn the_proposed_paths_name_what_they_hold() {
    assert_ne!(conversation_path(1), conversation_path(2));
    assert_ne!(trace_path(1), trace_path(2));
    assert!(conversation_path(7).ends_with(".md"));
    assert!(trace_path(7).ends_with(".txt"));
}

/// The confirmation declines at row 0, like every other confirmation this
/// release adds, and names the path it is about.
#[test]
fn the_confirmation_declines_at_row_zero_and_names_the_path() {
    let (title, rows) = confirm("io-session-3.md", "conversation");

    assert!(title.contains("io-session-3.md"), "{title}");
    assert_eq!(rows[0].label, io_cli::store::LEAVE_IT);
    assert!(!io_cli::store::acts(0));
    assert!(io_cli::store::acts(1));
    assert!(rows[1].label.contains("io-session-3.md"));
}

/// The report says what was written and how much of it.
#[test]
fn the_report_names_the_file_and_its_size() {
    let (_dir, store, workspace) = fixture();
    let (session, _runs) = seed(&store, &[("a question", Some("an answer"))]);
    let markdown = conversation(&store, session)
        .expect("readable")
        .expect("there is one");
    let written =
        write(&workspace, &conversation_path(session), &markdown).expect("the export is written");

    let said = report(&written);
    assert!(said.contains(&conversation_path(session)), "{said}");
    assert!(said.contains(&written.bytes.to_string()), "{said}");
    assert_eq!(written.bytes, markdown.len());
}
