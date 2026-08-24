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
//!
//! The last four blocks are about the two rendering functions, and need no store:
//! they are pure functions over this module's own types, so their fixtures are
//! literals. What they catch:
//!
//! * **The disclosure block** is the second half of F9, and it exists because the
//!   first half cannot be fixed here. io-harness restores from the snapshot taken
//!   before the run's first write and never compares it against what is on disk,
//!   so a hand edit made after the turn is overwritten silently — and io-cli cannot
//!   see it coming. Disclosure is therefore the whole of this product's answer.
//!   Sabotage it by deleting the clause about hand edits: nothing else in this file
//!   fails, which is exactly why the clause is asserted on its own rather than
//!   folded into the assertion about quoting the prompt.
//!
//! * **The no-count block** is F11. It asserts the armed line contains no digit at
//!   all, for a prompt that has none. Sabotage it by adding "puts 3 files back":
//!   the number can only have come from listing the workspace, because the set of
//!   paths a run snapshotted is not readable from this crate — so a plausible
//!   number is a fabricated one, and this is the assertion that says so.
//!
//! * **The order block** asserts by position, never by membership. Sabotage it by
//!   pushing the restored line before the declined one and every `contains`
//!   assertion stays green while the report now reads as a success with a
//!   footnote — which is how an operator misses that the agent's version of a file
//!   is still on disk.
//!
//! * **The closing-line block** covers both heads. Sabotage it by omitting the
//!   head line for `None` and only the `None` assertion fails: silence about a
//!   conversation that is back to having said nothing is indistinguishable from a
//!   conversation that carried on.

mod support;

use io_cli::glyphs::UNICODE;
use io_cli::rewind;
use io_cli::theme::Tone;
use io_harness::{ApproveAll, MemoryKind, Policy, Session, Store};
use support::Scripted;

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

/// A `Preview` of a turn that said `prompt`. The ids are arbitrary — nothing the
/// two rendering functions do depends on them, which is the point of their being
/// pure functions over this crate's own types.
fn preview_of(prompt: &str) -> rewind::Preview {
    rewind::Preview {
        turn_id: 7,
        run_id: 11,
        prompt: prompt.to_string(),
    }
}

/// An `Undone` carrying whatever the test is about. Everything not named is the
/// quiet case, so each assertion below is about exactly one thing.
fn undone_with(restored: &[&str], declined: &[(&str, &str)], head: Option<i64>) -> rewind::Undone {
    rewind::Undone {
        prompt: "rewrite everything".to_string(),
        restored: restored.iter().map(|path| path.to_string()).collect(),
        declined: declined
            .iter()
            .map(|(path, why)| (path.to_string(), why.to_string()))
            .collect(),
        memory_restored: 0,
        memory_removed: 0,
        queue_cleared: 0,
        head,
    }
}

#[test]
fn the_armed_line_discloses_that_a_hand_edit_since_the_turn_is_lost() {
    let line = rewind::armed_line(&preview_of("tidy the notes"), &UNICODE);

    // Quoting the turn. A confirmation of a keystroke is not a confirmation.
    assert!(
        line.contains("tidy the notes"),
        "the armed line must quote the turn it is about: {line}",
    );

    // The disclosure, asserted on its own words and separately from the quoting —
    // this is the half F9's first test cannot cover, because io-harness overwrites
    // a hand edit without reporting it and io-cli cannot detect that it did.
    assert!(
        line.contains("BEFORE that turn"),
        "the armed line must say the files go back to before the turn, not to \
         before the last write: {line}",
    );
    assert!(
        line.contains("by hand") && line.contains("lost"),
        "the armed line must say an edit made by hand since the turn is lost: {line}",
    );
}

#[test]
fn the_armed_line_names_the_turn_and_invents_no_file_count() {
    // A prompt with no digit of its own, so any digit in the result was produced
    // by the renderer rather than quoted from the operator.
    let line = rewind::armed_line(&preview_of("tidy the notes and add a summary"), &UNICODE);

    assert!(
        line.contains("tidy the notes and add a summary"),
        "the turn is named: {line}",
    );
    // F11. The set of paths a run recorded a restore point for is behind
    // io-harness's crate-private snapshot queries, so a count here could only have
    // come from listing the workspace — a number that is true whether or not the
    // rewind will do anything. No digit at all is the assertion that pins it,
    // because it fails for any invented number rather than for one particular
    // wording of one.
    assert!(
        !line.chars().any(char::is_numeric),
        "the armed line must state no count of anything: {line}",
    );
}

#[test]
fn the_report_leads_with_what_was_declined_and_tones_it_differently() {
    let lines = rewind::undone_lines(
        &undone_with(&["notes.md"], &[("logo.bin", "not valid UTF-8")], Some(4)),
        &UNICODE,
    );

    // By position, not by membership. A decline mentioned after a success reads as
    // a footnote, and a `contains` assertion is exactly as green for that order.
    assert!(
        lines[0].1.contains("logo.bin") && lines[0].1.contains("not valid UTF-8"),
        "the first line must be the decline, with its reason: {:?}",
        lines,
    );
    assert!(
        lines[1].1.contains("notes.md"),
        "the restoration comes after it: {:?}",
        lines,
    );
    // And the two are told apart without colour being the only carrier: the tones
    // differ, and each line also says in words what happened to its file.
    assert_ne!(
        lines[0].0, lines[1].0,
        "a decline and a restoration must not share a tone",
    );
    assert_eq!(
        lines[0].0,
        Tone::Warning,
        "a decline is something to act on"
    );
}

#[test]
fn the_report_says_in_words_where_the_conversation_now_is() {
    // The only-turn case, which `Session::branch_from` cannot even express and
    // which nobody tries by hand.
    let emptied = rewind::undone_lines(&undone_with(&["started.md"], &[], None), &UNICODE);
    let last = emptied.last().expect("the report is never empty");
    assert!(
        last.1.contains("back to having said nothing"),
        "an emptied conversation must say so rather than be inferred from \
         silence: {last:?}",
    );

    let continued = rewind::undone_lines(&undone_with(&["notes.md"], &[], Some(4)), &UNICODE);
    let last = continued.last().expect("the report is never empty");
    assert!(
        last.1.contains("continues from the turn before"),
        "a conversation that carried on must say where from: {last:?}",
    );
}

#[test]
fn a_long_prompt_is_what_gets_cut_and_never_the_disclosure() {
    // Far longer than any row, and with no digits of its own so the F11 assertion
    // above is not accidentally what this one is testing.
    let long = "tidy the notes and then rewrite the whole of the migration plan \
                so that it reads as one document rather than as a pile of \
                fragments that nobody has looked at since the cutover, and \
                while you are there fold the appendix into the body, drop the \
                section that describes the read-only window twice, and make the \
                summary at the top match what the rest of it actually says";
    let line = rewind::armed_line(&preview_of(long), &UNICODE);

    // Bounded whatever the prompt does: only the quoted prompt is shortened, and
    // the sentence around it is fixed. Two wrapped rows on an eighty-column
    // terminal, with headroom for a wording change — not a count to maintain.
    assert!(
        line.chars().count() < 200,
        "the armed line must stay bounded; got {} characters: {line}",
        line.chars().count(),
    );
    assert!(
        line.chars().count() < long.chars().count(),
        "the prompt is the part that gets shortened: {line}",
    );

    // And the half that was cut is the quotation, never the warning. Two releases
    // have shipped a row whose important half was the half that went.
    assert!(
        line.contains("BEFORE that turn") && line.contains("by hand") && line.contains("lost"),
        "the disclosure survives any prompt length: {line}",
    );
    assert!(
        line.contains('…'),
        "and the shortening is visible, so a reader knows the quotation is partial: {line}",
    );
}

// ---------------------------------------------------------------------------
// F11 — the arming state machine itself.
//
// These exist because a sabotage found nothing. Rewiring `App::key` to return
// `Command::Rewind` on the FIRST `Esc` — deleting the confirmation outright from
// the only key in this product that changes the operator's files on its own
// initiative — failed no test in the suite. The tests above cover `armed_line`,
// which is the sentence; nothing covered the state machine that decides whether
// the sentence is shown or the files are changed. A sabotage that fails nothing
// is a finding, and this is the finding.
// ---------------------------------------------------------------------------

use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use io_cli::app::{App, Command};
use io_cli::theme::DARK;

fn app() -> App {
    App::new(DARK, "test-model".to_string())
}

fn press(app: &mut App, code: KeyCode) -> Command {
    app.key(KeyEvent::new(code, KeyModifiers::NONE))
}

#[test]
fn f11_the_first_escape_arms_and_the_second_performs() {
    let mut app = app();
    assert!(!app.armed(), "a session does not begin armed");

    assert_eq!(press(&mut app, KeyCode::Esc), Command::ArmRewind);
    assert!(app.armed(), "the first press must arm rather than act");

    assert_eq!(press(&mut app, KeyCode::Esc), Command::Rewind);
    assert!(
        !app.armed(),
        "performing must disarm, or a third press would undo a second turn",
    );
}

#[test]
fn f11_any_other_key_disarms_and_the_next_escape_arms_again() {
    let mut app = app();
    assert_eq!(press(&mut app, KeyCode::Esc), Command::ArmRewind);

    // A keystroke in between. The operator went to type something and changed
    // their mind; the arming must not survive it.
    press(&mut app, KeyCode::Char('h'));
    assert!(!app.armed(), "any other key cancels the arming");

    // And the composer now holds text, so `Esc` is no longer the rewind key at
    // all — clear it and check the arming restarts from the beginning rather than
    // firing, which is what a stale flag would do.
    press(&mut app, KeyCode::Backspace);
    assert_eq!(
        press(&mut app, KeyCode::Esc),
        Command::ArmRewind,
        "after a cancel the next Esc must arm, never perform",
    );
}

#[test]
fn f11_escape_with_text_in_the_composer_is_not_a_rewind() {
    let mut app = app();
    press(&mut app, KeyCode::Char('n'));
    press(&mut app, KeyCode::Char('o'));

    let what = press(&mut app, KeyCode::Esc);
    assert_ne!(
        what,
        Command::ArmRewind,
        "a typed prompt is not an empty one"
    );
    assert_ne!(what, Command::Rewind);
    assert!(!app.armed());
}

#[test]
fn f11_nothing_arms_while_a_turn_is_running() {
    // F12's half of this: a rewind moves the conversation head the running turn
    // is about to write to, so it is refused with a sentence rather than queued.
    let mut app = app();
    app.started();
    // A turn with a step behind it. A turn that has done nothing is taken back
    // whole on the first press since 0.13.1 — see `App::undoable` — and what F11
    // is about is the turn that has work in it.
    app.status.steps = Some(1);

    // **`Esc` stops the turn now**, which is what every other agent in this
    // field does with it and what an operator presses it for. What F11 is about
    // is unchanged and is the assertion under it: nothing is ARMED, so the
    // second press cannot rewind a conversation head the turn is writing to.
    assert_eq!(press(&mut app, KeyCode::Esc), Command::Interrupt);
    assert!(!app.armed(), "a turn in flight must leave nothing armed");
    assert_eq!(
        press(&mut app, KeyCode::Esc),
        Command::Abandon,
        "and pressing it twice mid-turn stops the turn now, never rewinds",
    );
}
