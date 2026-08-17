//! Resuming and forking a conversation — F3, F4 and F5.
//!
//! These three are the release's only acceptance criteria whose wiring lives in
//! `src/main.rs`, which an integration test cannot link against: the driver owns
//! the `Session` handle, and `/resume` and `/fork` are two of its match arms. What
//! those arms actually *do*, though, is not in the driver at all — it is
//! [`io_harness::Session::reopen`] and [`io_harness::Session::branch_from`], and
//! that is a library seam this file can drive directly. So the behaviour is pinned
//! here at the level it exists, and the live test that needs an API key is left to
//! prove only the wiring.
//!
//! Every fixture is a **real turn** driven by the scripted provider in
//! `tests/support`. That is not thoroughness for its own sake: `Transcript` and
//! `TranscriptTurn` are both `#[non_exhaustive]`, so a conversation tree cannot be
//! forged from outside io-harness even field by field. The turns asserted over are
//! the rows the harness itself wrote.
//!
//! **Everything is read back from the store, never from the handle that wrote
//! it.** A `Session` caches its own head, so an assertion made against the value
//! in hand can agree with a mistake the database never saw — which is exactly the
//! failure a resume has, since the whole question is what a *second* process finds
//! when it picks the conversation up. `Store::session_turn` is therefore the
//! source for every parentage claim below.
//!
//! **No clock.** Nothing here sleeps, measures, or asks what time it is; ordering
//! comes from the ids the store minted, which is the same order it returns them
//! in. `tests/timing.rs` enforces that, and a branching test is a tempting place
//! to break it because "which turn came first" sounds like a question about time.

mod support;

use io_cli::theme::DARK;
use io_cli::transcript;
use io_harness::{ApproveAll, Policy, Session, Store};
use support::Scripted;

/// The prompts. Named rather than inlined because several assertions below are
/// about *which* prompt ended up where, and a bare string literal repeated in two
/// places is one edit away from being two different fixtures.
const PLAN: &str = "draft a migration plan";
const ABANDONED: &str = "do it with a blue-green cutover";
const KEPT: &str = "do it with a read-only window instead";

/// The words `src/transcript.rs` labels a branched-away turn with, spelled out
/// here rather than imported. Importing the constant would assert that the
/// renderer agrees with itself; what matters is that the label is a phrase a
/// reader understands without knowing this product's vocabulary.
const LABEL: &str = "left behind by a branch";

/// Take one turn and hand back the id of the turn it recorded.
///
/// The provider writes no files: an empty script is one completion of plain text,
/// which is a turn in the tree exactly like any other. Nothing in this file is
/// about what a run put on disk — `tests/rewind.rs` owns that — and a fixture that
/// wrote files would only add a way for these tests to fail for a reason they are
/// not about.
async fn say(session: &mut Session, store: &Store, prompt: &str) -> i64 {
    session
        .turn(
            prompt,
            &Scripted::writing(&[]),
            store,
            &Policy::permissive(),
            &ApproveAll,
        )
        .await
        .expect("a scripted turn cannot fail")
        .turn_id
}

/// A temporary workspace, an in-memory store, and a session over the two.
///
/// The directory is returned alongside because dropping a `TempDir` deletes it,
/// and a session whose root has been removed underneath it is a fixture that
/// fails for the wrong reason.
fn fixture() -> (tempfile::TempDir, Store, Session) {
    let dir = tempfile::tempdir().expect("a temp dir");
    let store = Store::memory().expect("an in-memory store");
    let session = Session::open(&store, dir.path()).expect("a session");
    (dir, store, session)
}

/// The turn `id` as the store holds it, which is the only place worth asking.
fn stored(store: &Store, id: i64) -> io_harness::Turn {
    store
        .session_turn(id)
        .expect("a turn is readable")
        .unwrap_or_else(|| panic!("turn {id} was recorded, so it must be readable"))
}

/// Every rendered line as its own string, spans concatenated. The renderer's unit
/// is the line, so an assertion about what one line holds must not be able to
/// pass because two adjacent lines held the halves.
fn rows(lines: &[ratatui::text::Line<'_>]) -> Vec<String> {
    lines
        .iter()
        .map(|line| {
            line.spans
                .iter()
                .map(|span| span.content.as_ref())
                .collect::<String>()
        })
        .collect()
}

/// The one row holding `needle`, or a failure naming what was rendered instead.
fn row_with<'a>(rows: &'a [String], needle: &str) -> &'a String {
    rows.iter()
        .find(|row| row.contains(needle))
        .unwrap_or_else(|| panic!("nothing rendered for {needle:?}; got {rows:#?}"))
}

#[tokio::test]
async fn f3_reopening_continues_the_conversation_while_opening_starts_a_second_one() {
    // Sabotage: replace `Session::reopen` with `Session::open` in the `/resume`
    // arm, under which only F3 fails — the second turn lands in a brand-new
    // session with no parent, and every other test in this file is untouched
    // because none of them resume anything.
    let (dir, store, mut first) = fixture();

    let opening = say(&mut first, &store, PLAN).await;
    let conversation = first.id();
    // The handle is dropped the way the process would end, so what comes next has
    // nothing but the id to go on — which is the whole of what `/resume` is given.
    drop(first);

    let mut resumed =
        io_cli::sessions::resume(&store, conversation).expect("the session reopens by id");
    assert_eq!(
        resumed.head(),
        Some(opening),
        "F3: a reopened session must pick up on the turn the first one stopped on, \
         not at the root of the conversation",
    );

    let continued = say(&mut resumed, &store, KEPT).await;
    let continued = stored(&store, continued);
    assert_eq!(
        continued.parent_turn_id,
        Some(opening),
        "F3: the turn taken after a resume must answer from the turn the \
         conversation stopped on",
    );
    assert_eq!(
        continued.session_id, conversation,
        "F3: a resumed turn belongs to the conversation that was resumed",
    );

    // The contrast that makes the assertion above mean anything. Opening a second
    // session against the SAME directory is what `io` does every time it starts
    // without `/resume`, and it must produce a different conversation — otherwise
    // "resume" would be indistinguishable from "run here again", and the test
    // above would pass against an implementation that only ever opens.
    let mut second = Session::open(&store, dir.path()).expect("a second session, same root");
    let elsewhere = say(&mut second, &store, ABANDONED).await;
    let elsewhere = stored(&store, elsewhere);
    assert_ne!(
        elsewhere.session_id, conversation,
        "F3: a session opened against a directory that already has one is a new \
         conversation, so same-directory is not same-conversation",
    );
    assert_eq!(
        elsewhere.parent_turn_id, None,
        "F3: the first turn of a freshly opened session answers from nothing, \
         however many turns the directory has already seen",
    );
}

#[tokio::test]
async fn f4_forking_moves_the_head_back_and_destroys_none_of_what_came_after() {
    // Sabotage: implement `/fork` with `Store::set_session_head` plus a delete of
    // the turns after the chosen one — the "clean up the dead branch" version.
    // Every assertion about the head and the next turn's parent still passes; only
    // the two assertions about `session_turns` below fail, which is why they are
    // here and why they name the turns rather than counting them.
    let (_dir, store, mut session) = fixture();

    let one = say(&mut session, &store, PLAN).await;
    let two = say(&mut session, &store, ABANDONED).await;
    let three = say(&mut session, &store, "and roll it out on friday").await;
    let four = say(&mut session, &store, "and tell the on-call team").await;

    session
        .branch_from(&store, two)
        .expect("turn two belongs to this session, so it is forkable");
    assert_eq!(
        session.head(),
        Some(two),
        "F4: a fork puts the head on the chosen turn",
    );

    let five = say(&mut session, &store, KEPT).await;
    assert_eq!(
        stored(&store, five).parent_turn_id,
        Some(two),
        "F4: the turn taken after a fork answers from the turn that was forked \
         at, not from the newest turn in the session",
    );

    let path: Vec<i64> = session
        .history(&store)
        .expect("the path reads")
        .iter()
        .map(|turn| turn.id)
        .collect();
    assert_eq!(
        path,
        vec![one, two, five],
        "F4: the conversation the model sees runs through the forked-at turn and \
         stops there — the turns after it are on another branch",
    );

    let tree = store
        .session_turns(session.id())
        .expect("the whole tree reads");
    let ids: Vec<i64> = tree.iter().map(|turn| turn.id).collect();
    // The count AND the names. A count alone is green for an implementation that
    // deleted turns three and four and left two other rows behind it, which is
    // precisely the shape a "tidy up the branch" bug has.
    assert_eq!(
        ids.len(),
        5,
        "F4: five turns were taken and a fork writes nothing away: {ids:?}",
    );
    assert!(
        ids.contains(&three) && ids.contains(&four),
        "F4: the two turns the fork left behind must still be in the store — a \
         branch is a move, not a deletion: {ids:?}",
    );
}

#[tokio::test]
async fn f4_a_turn_from_another_conversation_cannot_be_forked_from() {
    // Sabotage: have io-cli check the turn's session itself before calling
    // `branch_from`, then let that check drift. This test is what says io-cli does
    // not need such a check — the guard is the harness's, it is the one that
    // actually runs, and `/fork` is allowed to rely on it. Delete the guard from
    // io-harness and only this test fails.
    let (_dir, store, mut mine) = fixture();
    let ours = say(&mut mine, &store, PLAN).await;

    let elsewhere = tempfile::tempdir().expect("a second temp dir");
    let mut theirs = Session::open(&store, elsewhere.path()).expect("a second session");
    let stranger = say(&mut theirs, &store, ABANDONED).await;

    let refused = mine.branch_from(&store, stranger);
    assert!(
        refused.is_err(),
        "F4: a turn belonging to another session is not a turn this conversation \
         can be forked at, whatever its id looks like",
    );
    assert_eq!(
        mine.head(),
        Some(ours),
        "F4: a refused fork leaves the conversation exactly where it was, so the \
         operator's next turn still answers from their own last one",
    );
}

/// A plan, an answer the operator changed their mind about, and the answer they
/// took instead — so the middle turn is off the path and the other two are on it.
///
/// The abandoned turn sits *between* the two kept ones in the store's own order,
/// which is what makes the ordering assertion below worth making: a renderer that
/// appended the off-path turns at the end would still contain all three.
async fn forked() -> (tempfile::TempDir, io_harness::Transcript, i64) {
    let (dir, store, mut session) = fixture();

    let plan = say(&mut session, &store, PLAN).await;
    let abandoned = say(&mut session, &store, ABANDONED).await;
    session
        .branch_from(&store, plan)
        .expect("the plan turn is forkable");
    say(&mut session, &store, KEPT).await;

    let transcript = session.transcript(&store).expect("a transcript");
    (dir, transcript, abandoned)
}

#[tokio::test]
async fn f5_the_forked_away_turn_is_marked_off_the_path_and_the_others_are_on_it() {
    // Sabotage: build the transcript from `Session::history` instead of the whole
    // tree, under which the abandoned turn is simply absent. Only the assertions
    // about `on_path == false` can catch that — a turn that is missing looks
    // exactly like a turn that never happened.
    let (_dir, transcript, abandoned) = forked().await;

    let off: Vec<&str> = transcript
        .turns
        .iter()
        .filter(|turn| !turn.on_path)
        .map(|turn| turn.prompt.as_str())
        .collect();
    let on: Vec<&str> = transcript
        .turns
        .iter()
        .filter(|turn| turn.on_path)
        .map(|turn| turn.prompt.as_str())
        .collect();

    // Both directions, because a transcript that marked everything off the path
    // would satisfy the first assertion on its own and would be telling the
    // operator their whole conversation is gone.
    assert_eq!(
        off,
        vec![ABANDONED],
        "F5: exactly the turn the fork left behind is off the path: {off:?}",
    );
    assert_eq!(
        on,
        vec![PLAN, KEPT],
        "F5: the turns the model can still see stay on the path across a fork: {on:?}",
    );

    let abandoned = transcript
        .turns
        .iter()
        .find(|turn| turn.turn_id == abandoned)
        .expect("the abandoned turn is in the transcript");
    assert!(
        !abandoned.on_path,
        "F5: the turn identified by id — not merely by prompt — is the one marked \
         off the path",
    );
    assert!(
        abandoned.run_id > 0,
        "F5: a branched-away turn keeps the run that served it, so what it cost \
         and what it did is still reachable after the fork",
    );
}

#[tokio::test]
async fn f5_the_transcript_renders_the_forked_away_turn_in_place_and_labels_it() {
    // Sabotage: drop the `!turn.on_path` arm from `transcript::turn_lines`, or
    // filter the off-path turns out of `lines` entirely. The first loses the
    // label and the second loses the turn; the two assertions below fail one
    // each, and nothing else in this file notices either.
    let (_dir, transcript, _) = forked().await;
    let rendered = rows(&transcript::lines(&transcript, &DARK));

    // Rendered at all: `Session::history` would not have returned this turn, so
    // the transcript is the only surface in the product that can show it.
    let abandoned = row_with(&rendered, ABANDONED);
    assert!(
        abandoned.contains(LABEL),
        "F5: the forked-away turn must say so in words on its own line — a tone \
         is gone under NO_COLOR and gone again when the line is copied: {abandoned}",
    );
    let kept = row_with(&rendered, KEPT);
    assert!(
        !kept.contains(LABEL),
        "F5: a turn the model can still see must not carry the branch label: {kept}",
    );

    // By position, never by membership. A renderer that gathered the off-path
    // turns into a footnote after the conversation contains all three prompts and
    // is green for every `contains` assertion above, while reading as though the
    // operator asked for the cutover last — which is the opposite of what
    // happened, and is an order defect this product has already paid for once.
    let text = rendered.join("\n");
    let plan_at = text.find(PLAN).expect("the plan is rendered");
    let abandoned_at = text
        .find(ABANDONED)
        .expect("the abandoned turn is rendered");
    let kept_at = text.find(KEPT).expect("the kept turn is rendered");
    assert!(
        plan_at < abandoned_at && abandoned_at < kept_at,
        "F5: turns are rendered oldest first whichever branch they are on, so the \
         forked-away turn belongs between the two on the path — got plan at \
         {plan_at}, abandoned at {abandoned_at}, kept at {kept_at}",
    );
}
