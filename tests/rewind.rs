//! Undoing the last turn — F8, F9 and F10.
//!
//! Every fixture here is a **real turn**. A scripted provider issues `write_file`
//! calls into an in-memory store and a temporary workspace, and the harness's own
//! agent loop records the restore points as it goes. That is not thoroughness for
//! its own sake: `Store`'s snapshot writer is crate-private, so a restore point
//! cannot be forged from outside io-harness at all. The only way to assert that
//! `io_cli::rewind` puts a file back is to have a run put it there first.
//!
//! What each block would catch, stated so a green run means something:
//!
//! * **The two-file block** catches a rewind whose report is recounted from the
//!   filesystem instead of read off the returned `Rewound` — F8. Sabotage it by
//!   replacing `restored` with a directory listing: exactly one of the two files
//!   still exists afterwards, because the other was deleted, so the recount says
//!   one where the truth is two. It also catches a rewind that restores the edited
//!   file and forgets the created one, which is what a per-path undo looks like
//!   when it is applied to only the first snapshot.
//!
//! * **The memory block** catches an undo that puts the files back and leaves what
//!   the run learned. Sabotage it by dropping the `rewind_run` call and restoring
//!   files by hand: the fact the run got wrong is still readable through
//!   `Store::memory_get`, and it is read into the *next* prompt, so the agent
//!   repeats the mistake it was just corrected on.
//!
//! * **The declined block** catches a report that says "restored" about a file
//!   nothing touched — F9. Sabotage it by folding `Rewind::NotKept` into
//!   `restored`: the operator is told their file came back while the agent's
//!   version is still on disk, and so never reaches for their own backup. It also
//!   catches an undo that gives up on the whole run when one path cannot be put
//!   back, because the two restorable files in the same fixture must still come
//!   back.
//!
//! * **The two-turn block** catches an undo that moves files without moving the
//!   conversation. Sabotage it by deleting the `set_session_head` call: the head
//!   still points at the undone turn, so the next turn is assembled from a history
//!   containing a prompt whose work no longer exists.
//!
//! * **The only-turn block** is F10 and the one that matters most. Sabotage it by
//!   reaching for `Session::branch_from` instead of `set_session_head` — the whole
//!   plausible-looking implementation — and it cannot even be written, because
//!   there is no parent turn to branch from. Sabotage it by keeping the session
//!   value instead of re-opening it and `session.head()` answers with the undone
//!   turn while the store says `NULL`. Both sabotages pass the two-turn block.
//!
//! * **The empty-session block** catches an undo that errors, or panics on an
//!   `unwrap`, when an operator presses it on a fresh session.

use std::collections::VecDeque;
use std::sync::Mutex;

use io_cli::rewind;
use io_harness::provider::{CompletionRequest, CompletionResponse, ToolCall};
use io_harness::tools::WRITE_FILE_TOOL;
use io_harness::{ApproveAll, MemoryKind, Policy, Provider, Session, Store};

/// A provider that plays a script of tool-call batches and then stops talking.
///
/// One batch per completion, in order; once the script is exhausted every later
/// completion is plain text with no calls, which is how the agent loop is told the
/// turn is over. A provider that returned its calls forever would never end a
/// turn, and a test that ended one with a step ceiling would be asserting the
/// ceiling rather than the work.
struct Scripted {
    batches: Mutex<VecDeque<Vec<ToolCall>>>,
}

impl Scripted {
    /// One batch that writes each `(path, content)` in the order given.
    fn writing(files: &[(&str, &str)]) -> Self {
        let batch = files
            .iter()
            .map(|(path, content)| write_call(path, content))
            .collect();
        Self {
            batches: Mutex::new(VecDeque::from(vec![batch])),
        }
    }
}

impl Provider for Scripted {
    async fn complete(
        &self,
        _request: CompletionRequest,
    ) -> io_harness::Result<CompletionResponse> {
        let calls: Vec<ToolCall> = self
            .batches
            .lock()
            .expect("the script is not poisoned")
            .pop_front()
            .unwrap_or_default();
        Ok(CompletionResponse {
            // Text only once there is nothing left to do, so the loop has exactly
            // one reason to stop and it is the ordinary one.
            text: calls.is_empty().then(|| "done".to_string()),
            tool_calls: calls,
            ..Default::default()
        })
    }
}

/// One `write_file` call, with its arguments built as JSON text and parsed.
///
/// `ToolCall::arguments` is a `serde_json::Value`, and `serde_json` is not a
/// dependency of this crate — io-harness carries it and does not re-export it, so
/// the type cannot be named here at all. It can still be *produced*: `Value`
/// implements `FromStr`, so `str::parse` builds one with the target type inferred
/// from the field it is assigned to. That is the whole reason this helper writes
/// its arguments as text rather than with a builder.
fn write_call(path: &str, content: &str) -> ToolCall {
    ToolCall {
        name: WRITE_FILE_TOOL.to_string(),
        arguments: format!(
            "{{\"path\":{},\"content\":{}}}",
            quoted(path),
            quoted(content)
        )
        .parse()
        .expect("the arguments were assembled as JSON and must parse as JSON"),
    }
}

/// `text` as a JSON string literal.
///
/// Hand-written because the crate has no JSON encoder to reach for. It escapes
/// only what the fixtures below actually contain — quotes, backslashes and
/// newlines — and would produce invalid JSON for a control character, which is
/// why `write_call` asserts that the result parses rather than trusting it.
fn quoted(text: &str) -> String {
    let mut out = String::with_capacity(text.len() + 2);
    out.push('"');
    for character in text.chars() {
        match character {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            other => out.push(other),
        }
    }
    out.push('"');
    out
}

/// A temporary workspace, an in-memory store, and a session over the two.
struct Fixture {
    dir: tempfile::TempDir,
    store: Store,
    session: Session,
}

impl Fixture {
    fn new() -> Self {
        let dir = tempfile::tempdir().expect("a temp dir");
        let store = Store::memory().expect("an in-memory store");
        let session = Session::open(&store, dir.path()).expect("a session");
        Self {
            dir,
            store,
            session,
        }
    }

    /// Take a turn in which the agent writes every one of `files`.
    async fn turn_writing(&mut self, prompt: &str, files: &[(&str, &str)]) {
        self.session
            .turn(
                prompt,
                &Scripted::writing(files),
                &self.store,
                &Policy::permissive(),
                &ApproveAll,
            )
            .await
            .expect("a scripted turn cannot fail");
    }

    /// What is on disk at `path`, as bytes, so a comparison cannot be fooled by a
    /// lossy decode.
    fn bytes(&self, path: &str) -> Vec<u8> {
        std::fs::read(self.dir.path().join(path))
            .unwrap_or_else(|error| panic!("{path} is readable: {error}"))
    }

    fn exists(&self, path: &str) -> bool {
        self.dir.path().join(path).exists()
    }

    /// The workspace key memory is stored under — the session root, spelled the
    /// way the harness spells it.
    fn workspace(&self) -> String {
        self.session.root().display().to_string()
    }
}

const BEFORE: &str = "what was there first\n";
const AFTER: &str = "what the agent wrote instead\n";
const INVENTED: &str = "a file the agent made up\n";

#[tokio::test]
async fn an_edited_file_comes_back_and_a_created_file_goes_and_both_are_reported() {
    let mut fixture = Fixture::new();
    std::fs::write(fixture.dir.path().join("notes.md"), BEFORE).expect("the file is writable");

    fixture
        .turn_writing(
            "tidy the notes and add a summary",
            &[("notes.md", AFTER), ("summary.md", INVENTED)],
        )
        .await;
    // The fixture is only meaningful if the run actually landed both writes.
    assert_eq!(fixture.bytes("notes.md"), AFTER.as_bytes());
    assert_eq!(fixture.bytes("summary.md"), INVENTED.as_bytes());

    let undone = rewind::last_turn(&mut fixture.session, &fixture.store)
        .expect("the rewind cannot fail")
        .expect("a turn was taken, so there is one to undo");

    assert_eq!(
        fixture.bytes("notes.md"),
        BEFORE.as_bytes(),
        "the edited file must hold what it held before the run's first write to it",
    );
    assert!(
        !fixture.exists("summary.md"),
        "the way a created file was is not existing, so it must be gone",
    );

    // F8. Two paths are reported while exactly one file remains on disk, so a
    // report recounted from the workspace afterwards cannot produce this number —
    // which is the whole reason the counts come from the returned `Rewound`.
    assert_eq!(
        undone.restored,
        vec!["notes.md".to_string(), "summary.md".to_string()],
        "both paths, in the order the run first wrote them",
    );
    assert!(
        undone.declined.is_empty(),
        "nothing was declined: {:?}",
        undone.declined,
    );
    assert_eq!(
        undone.prompt, "tidy the notes and add a summary",
        "the report names the turn in the operator's own words",
    );
}

#[tokio::test]
async fn memory_the_run_changed_is_put_back_and_memory_it_invented_is_removed() {
    let mut fixture = Fixture::new();
    let workspace = fixture.workspace();

    // What an earlier run learned, and which the turn below is about to get wrong.
    let earlier = fixture
        .store
        .start_run("learn how flaky the suite is", &workspace)
        .expect("a run row");
    fixture
        .store
        .memory_write(&workspace, "retries", "three", earlier, 1, MemoryKind::Fact)
        .expect("the earlier fact is written");

    fixture.turn_writing("raise the retry count", &[]).await;
    let run = fixture
        .session
        .head()
        .and_then(|head| {
            fixture
                .store
                .session_turn(head)
                .expect("the turn is readable")
        })
        .expect("the turn exists")
        .run_id;

    // The turn corrects the earlier fact wrongly and invents a second note of its
    // own. Written against the turn's run so the rewind is about this turn — the
    // harness's memory tools write through exactly this call.
    fixture
        .store
        .memory_write(&workspace, "retries", "nine", run, 1, MemoryKind::Fact)
        .expect("the wrong fact is written");
    fixture
        .store
        .memory_write(&workspace, "flaky", "always", run, 2, MemoryKind::Fact)
        .expect("the invented fact is written");

    // The premise, checked: `memory_write` can refuse a write, and a refused one
    // would leave `retries` at "three" and `flaky` absent — which is exactly what
    // the assertions below look for. Without this the whole test is green for a
    // write that never happened.
    assert_eq!(
        fixture
            .store
            .memory_get(&workspace, "retries")
            .expect("memory is readable")
            .expect("the key exists")
            .value,
        "nine",
        "the turn's wrong value must actually be in the store before it is undone",
    );

    let undone = rewind::last_turn(&mut fixture.session, &fixture.store)
        .expect("the rewind cannot fail")
        .expect("a turn was taken");

    // Edited, so it comes back to the value the earlier run left.
    assert_eq!(
        fixture
            .store
            .memory_get(&workspace, "retries")
            .expect("memory is readable")
            .expect("the key still exists")
            .value,
        "three",
        "a key the run overwrote must hold what was there before it",
    );
    // Created, so it goes. Asserted as an absence: a key that is merely stale
    // still reaches the next prompt.
    assert!(
        fixture
            .store
            .memory_get(&workspace, "flaky")
            .expect("memory is readable")
            .is_none(),
        "a key the run invented must be gone, not merely old",
    );
    assert_eq!(undone.memory_restored, 1, "one key was put back");
    assert_eq!(undone.memory_removed, 1, "one key was removed");
}

#[tokio::test]
async fn a_file_whose_previous_contents_were_not_kept_is_declined_and_left_untouched() {
    let mut fixture = Fixture::new();
    std::fs::write(fixture.dir.path().join("notes.md"), BEFORE).expect("the file is writable");
    // Not valid UTF-8, so the harness records that this file's previous contents
    // were deliberately **not kept**. That is the state in which a rewind reports
    // `Rewind::NotKept` and changes nothing — the only verdict this product can
    // reach that means "the agent's version is still on disk".
    std::fs::write(
        fixture.dir.path().join("logo.bin"),
        [0xff_u8, 0xfe, 0x00, 0x01],
    )
    .expect("the file is writable");

    fixture
        .turn_writing(
            "rewrite everything",
            &[
                ("notes.md", AFTER),
                ("summary.md", INVENTED),
                ("logo.bin", AFTER),
            ],
        )
        .await;

    // And then the operator edits that file themselves, after the turn.
    let by_hand = b"what the operator typed instead\n";
    std::fs::write(fixture.dir.path().join("logo.bin"), by_hand).expect("the file is writable");

    let undone = rewind::last_turn(&mut fixture.session, &fixture.store)
        .expect("the rewind cannot fail")
        .expect("a turn was taken");

    // F9. The verdict says nothing was changed, so the path is a decline carrying
    // the harness's own reason — never a claim that it came back.
    assert_eq!(
        undone.declined.len(),
        1,
        "exactly one path was left alone: {:?}",
        undone.declined,
    );
    let (path, why) = &undone.declined[0];
    assert_eq!(path.as_str(), "logo.bin");
    assert!(
        why.contains("UTF-8"),
        "the decline must carry a reason an operator can act on, got {why:?}",
    );
    assert!(
        !undone.restored.contains(path),
        "a declined path must never also be reported as restored",
    );
    // Byte-identical to the hand edit: the rewind wrote nothing here.
    assert_eq!(
        fixture.bytes("logo.bin"),
        by_hand,
        "a file the rewind declined must be exactly as it was found",
    );

    // Two put back and a third declined, all three reported — one path that
    // cannot be undone does not abandon the rest of the run.
    assert_eq!(
        undone.restored,
        vec!["notes.md".to_string(), "summary.md".to_string()],
        "the two restorable paths still came back",
    );
    assert_eq!(fixture.bytes("notes.md"), BEFORE.as_bytes());
    assert!(!fixture.exists("summary.md"));
}

#[tokio::test]
async fn rewinding_the_second_of_two_turns_leaves_the_head_on_the_first() {
    let mut fixture = Fixture::new();
    fixture
        .turn_writing("draft a plan", &[("plan.md", BEFORE)])
        .await;
    let first = fixture.session.head().expect("the first turn is the head");
    fixture
        .turn_writing("now do it", &[("plan.md", AFTER)])
        .await;
    assert_ne!(
        fixture.session.head(),
        Some(first),
        "the fixture needs two distinct turns",
    );

    let undone = rewind::last_turn(&mut fixture.session, &fixture.store)
        .expect("the rewind cannot fail")
        .expect("a turn was taken");

    assert_eq!(
        undone.head,
        Some(first),
        "the report says where the conversation now is",
    );
    assert_eq!(
        fixture.session.head(),
        Some(first),
        "and the session agrees, because it was re-read from the store",
    );
    let history = fixture
        .session
        .history(&fixture.store)
        .expect("the history is readable");
    assert_eq!(
        history.len(),
        1,
        "the model must no longer see the undone turn, got {:?}",
        history.iter().map(|turn| &turn.prompt).collect::<Vec<_>>(),
    );
    assert_eq!(history[0].prompt, "draft a plan");
    // The second turn's write went back to what the first turn left, not to
    // nothing: the restore point is per run.
    assert_eq!(fixture.bytes("plan.md"), BEFORE.as_bytes());
}

#[tokio::test]
async fn rewinding_the_only_turn_leaves_the_session_with_no_head_at_all() {
    let mut fixture = Fixture::new();
    fixture
        .turn_writing("start something", &[("started.md", INVENTED)])
        .await;
    assert!(
        fixture.session.head().is_some(),
        "the fixture needs one turn",
    );

    let undone = rewind::last_turn(&mut fixture.session, &fixture.store)
        .expect("the rewind cannot fail")
        .expect("a turn was taken");

    // F10. `Session::branch_from` cannot produce this state — it needs a turn to
    // branch from, and there is none — so an implementation built on it either
    // fails here or leaves the head on the turn it just undid.
    assert_eq!(
        undone.head, None,
        "the report must say the conversation is back to having said nothing",
    );
    assert_eq!(
        fixture.session.head(),
        None,
        "the in-memory session must not still hold the undone turn",
    );
    assert!(
        fixture
            .session
            .history(&fixture.store)
            .expect("the history is readable")
            .is_empty(),
        "nothing is left for the next prompt to be assembled from",
    );
    assert!(
        !fixture.exists("started.md"),
        "and the file the turn created is gone with it",
    );

    // Nothing in the trace was deleted: the turn is still there to read, and the
    // undo is written down as a row of its own.
    let turns = fixture
        .store
        .session_turns(fixture.session.id())
        .expect("the tree is readable");
    assert_eq!(
        turns.len(),
        1,
        "the turn stays in the tree; an undo is not a deletion",
    );
    assert_eq!(
        fixture
            .store
            .rewinds(turns[0].run_id)
            .expect("the rewinds are readable")
            .len(),
        1,
        "the undo is itself recorded",
    );
}

#[test]
fn a_session_that_has_taken_no_turns_undoes_nothing_and_does_not_fail() {
    let mut fixture = Fixture::new();
    assert!(
        rewind::preview(&fixture.session, &fixture.store).is_none(),
        "there is nothing to arm a confirmation prompt about",
    );
    let undone = rewind::last_turn(&mut fixture.session, &fixture.store)
        .expect("an empty session is not an error");
    assert!(
        undone.is_none(),
        "pressing undo on a fresh session asks for nothing to happen",
    );
    assert_eq!(fixture.session.head(), None);
}

#[tokio::test]
async fn the_preview_names_the_turn_that_would_be_undone() {
    let mut fixture = Fixture::new();
    fixture.turn_writing("draft a plan", &[]).await;
    fixture.turn_writing("now do it", &[]).await;

    let preview = rewind::preview(&fixture.session, &fixture.store)
        .expect("two turns were taken, so one is undoable");
    assert_eq!(
        preview.prompt, "now do it",
        "the armed prompt quotes the last turn, not the first",
    );
    assert_eq!(
        Some(preview.turn_id),
        fixture.session.head(),
        "and it is the turn the head points at",
    );
}
