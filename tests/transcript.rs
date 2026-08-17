//! The whole conversation, committed to scrollback.
//!
//! The discriminating assertion is the first one: a session that branched holds a
//! turn `Session::history` no longer returns, and this renderer is the only
//! surface in the product that can show it. A renderer built from the path passes
//! every other assertion here and fails that one — silently, because a turn that
//! is missing looks exactly like a turn that never happened.
//!
//! `io_harness::Transcript` and `io_harness::TranscriptTurn` are both
//! `#[non_exhaustive]`, so neither can be built by a struct literal from outside
//! the harness — not even field by field. The transcript under test is therefore a
//! real one: a scripted provider drives real turns into an in-memory store, and
//! `Session::branch_from` puts one of them off the path the same way an operator
//! would. That also means these assertions are about what the harness actually
//! records, rather than about a hand-made value that agrees with the renderer by
//! construction.

use io_cli::theme::DARK;
use io_cli::transcript;
use io_harness::provider::{CompletionRequest, CompletionResponse};
use io_harness::{ApproveAll, Policy, Provider, Session, Store};

/// The three prompts the branched session is built from. The second is the one
/// the branch leaves behind.
const PLAN: &str = "draft a migration plan";
const BRANCHED: &str = "do it with a blue-green cutover";
const KEPT: &str = "do it with a read-only window instead";

/// The words `src/transcript.rs` labels a branched-away turn with, spelled out
/// here rather than imported. Importing the constant would assert that the
/// renderer agrees with itself; what matters is that the label is a phrase a
/// reader understands without knowing this product's vocabulary.
const LABEL: &str = "left behind by a branch";

/// Answers every turn with one line and never calls anything.
struct Talker;

impl Provider for Talker {
    async fn complete(
        &self,
        _request: CompletionRequest,
    ) -> io_harness::Result<CompletionResponse> {
        Ok(CompletionResponse {
            text: Some("here is the plan".into()),
            ..Default::default()
        })
    }
}

/// Every rendered line as its own string, spans concatenated. The renderer's unit
/// is the line, and every assertion below is about what a single line holds and in
/// what order — joining them all into one blob would make "on the same line" and
/// "somewhere in the output" indistinguishable.
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

async fn say(session: &mut Session, store: &Store, prompt: &str) -> i64 {
    session
        .turn(prompt, &Talker, store, &Policy::permissive(), &ApproveAll)
        .await
        .expect("a scripted turn cannot fail")
        .turn_id
}

/// A plan, an answer the operator changed their mind about, and the answer they
/// took instead — so the middle turn is off the path and the other two are on it.
async fn branched() -> (tempfile::TempDir, io_harness::Transcript) {
    let dir = tempfile::tempdir().expect("a temp dir");
    let store = Store::memory().expect("an in-memory store");
    let mut session = Session::open(&store, dir.path()).expect("a session");

    let first = say(&mut session, &store, PLAN).await;
    say(&mut session, &store, BRANCHED).await;
    session
        .branch_from(&store, first)
        .expect("the plan turn is branchable");
    say(&mut session, &store, KEPT).await;

    let transcript = session.transcript(&store).expect("a transcript");
    (dir, transcript)
}

#[tokio::test]
async fn the_turn_a_branch_left_behind_is_rendered_and_labelled_in_words() {
    let (_dir, transcript) = branched().await;
    assert_eq!(
        session_path_len(&transcript),
        2,
        "the fixture is only meaningful if one of the three turns is off the path",
    );

    let rendered = rows(&transcript::lines(&transcript, &DARK));

    // Rendered at all. `Session::history` would not have returned this turn.
    let branched = row_with(&rendered, BRANCHED);
    // And labelled in words on its own line, not merely toned: a colour is gone
    // under `NO_COLOR` and gone again when the line is copied out of the terminal.
    assert!(
        branched.contains(LABEL),
        "the branched-away turn must say so in words: {branched}",
    );

    let kept = row_with(&rendered, KEPT);
    assert!(
        !kept.contains(LABEL),
        "a turn the model can still see must not be labelled: {kept}",
    );
}

#[tokio::test]
async fn every_turn_that_was_asked_appears() {
    let (_dir, transcript) = branched().await;
    let rendered = rows(&transcript::lines(&transcript, &DARK));

    for turn in &transcript.turns {
        row_with(&rendered, &turn.prompt);
    }
    assert_eq!(transcript.turns.len(), 3, "three turns were taken");
}

#[tokio::test]
async fn the_prompt_comes_before_the_turn_id_on_its_line() {
    let (_dir, transcript) = branched().await;
    let rendered = rows(&transcript::lines(&transcript, &DARK));

    for turn in &transcript.turns {
        let row = row_with(&rendered, &turn.prompt);
        let id = format!("turn {}", turn.turn_id);
        let prompt_at = row.find(turn.prompt.as_str()).expect("the prompt is here");
        let id_at = row
            .find(id.as_str())
            .unwrap_or_else(|| panic!("no {id} on the line holding its prompt: {row}"));
        // By position, not by membership: a line that reads "turn 2 · draft a
        // migration plan" contains both and is inside out, and a `contains`
        // assertion is exactly as green for it.
        assert!(
            prompt_at < id_at,
            "content before metadata: {row} has the id at {id_at} and the prompt at {prompt_at}",
        );
    }
}

#[test]
fn an_empty_transcript_renders_a_sentence() {
    let dir = tempfile::tempdir().expect("a temp dir");
    let store = Store::memory().expect("an in-memory store");
    let session = Session::open(&store, dir.path()).expect("a session");
    let transcript = session.transcript(&store).expect("a transcript");
    assert!(transcript.turns.is_empty(), "nothing has been asked yet");

    let rendered = rows(&transcript::lines(&transcript, &DARK));
    let sentence = row_with(&rendered, "no turns");
    assert!(
        sentence.ends_with('.'),
        "a sentence, so the reader knows the key worked: {sentence}",
    );
}

/// How many turns the model can still see. Read through the transcript itself so
/// the fixture's premise is checked with the same data the renderer is given.
fn session_path_len(transcript: &io_harness::Transcript) -> usize {
    transcript.turns.iter().filter(|turn| turn.on_path).count()
}
