//! F5, F10 and F11 — the three surfaces that answer "show me more" and "give me
//! this text", asserted at the boundary each of them actually crosses.
//!
//! The three share one premise and it is the reason they are in one file: **the
//! viewport is four rows and never grows**. Everything that shows more of
//! something is therefore written *upward*, into the terminal's own scrollback,
//! where the terminal's search, selection and copy-mode already work — and
//! everything that hands text to the reader's own machine goes out as an escape
//! sequence, which is neither scrollback nor viewport. A regression in any of the
//! three looks the same from inside a rendered buffer: nothing. So the assertions
//! below are made against the bytes that leave the process and against the
//! viewport buffer at rest, never against a value the renderer handed itself.
//!
//! Order matters on a rendered line and is asserted by position rather than by
//! membership. A line that reads `turn 2 · left behind by a branch · do it with a
//! blue-green cutover` contains every substring an inside-out line contains, and
//! a `contains` assertion is exactly as green for it.

mod support;

use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use io_cli::app::{App, Command};
use io_cli::clipboard;
use io_cli::commands::{parse, Action, Copied};
use io_cli::diff::MAX_BODY_LINES;
use io_cli::keys::Keys;
use io_cli::term::VIEWPORT_HEIGHT;
use io_cli::theme::DARK;
use io_cli::transcript::{self, BRANCHED_AWAY};
use io_harness::provider::{CompletionRequest, CompletionResponse};
use io_harness::{ApproveAll, Edit, Policy, Provider, Session, Store};
use ratatui::text::Line;
use ratatui::widgets::Paragraph;

// ---------------------------------------------------------------------------
// F5 — expansion commits upward, and the viewport never grows.
// ---------------------------------------------------------------------------

/// Far more rows than the terminal under test has, so a commit that tried to
/// make room by growing the viewport would have to grow it past the screen.
const COMMITTED_ROWS: usize = 200;

/// What the viewport draws while the commit is happening. Deliberately shares no
/// word with the committed lines, so "the viewport holds only this" is a real
/// assertion rather than a coincidence of vocabulary.
const COMPOSER: &str = "> the prompt being typed";

#[test]
fn f5_a_large_commit_reaches_the_terminal_and_leaves_the_viewport_at_its_fixed_height() {
    // 80x24 on purpose: the smallest supported terminal, where a viewport that
    // grew by two hundred rows could not even be drawn. The failure this prevents
    // is a "show me more" path that opens a pane instead of committing — which on
    // this renderer means the transcript stops being the terminal's own buffer and
    // starts being something this process has to redraw and can lose.
    let (mut screen, recorder) = support::screen(80, 24);
    let before = screen.terminal_mut().get_frame().area().height;
    assert_eq!(
        before, VIEWPORT_HEIGHT,
        "the fixture is only meaningful if the viewport starts at its fixed height",
    );

    let body: Vec<Line<'static>> = (0..COMMITTED_ROWS)
        .map(|row| Line::from(format!("expanded detail row {row}")))
        .collect();
    screen.commit(&body).expect("a commit of two hundred rows");
    screen
        .draw(|frame| {
            let area = frame.area();
            frame.render_widget(Paragraph::new(COMPOSER), area);
        })
        .expect("a frame after the commit");

    assert_eq!(
        screen.terminal_mut().get_frame().area().height,
        before,
        "the viewport grew to hold committed content; it is fixed at {VIEWPORT_HEIGHT} rows \
         and everything that shows more has to go above it",
    );

    // The content reached the terminal — which is what "in the scrollback" means
    // for a renderer that never owns the rows above its viewport.
    let text = recorder.text();
    for row in [0, COMMITTED_ROWS / 2, COMMITTED_ROWS - 1] {
        let needle = format!("expanded detail row {row}");
        assert!(
            text.contains(&needle),
            "{needle:?} never left the process, so it is in no scrollback",
        );
    }

    // And none of it is in the viewport, which holds what the frame drew and
    // nothing else. This is the half that fails when expansion is rendered into
    // the viewport instead of committed above it.
    let viewport = screen.viewport_text();
    assert!(
        viewport.contains(COMPOSER),
        "the viewport should hold the composer, but it holds {viewport:?}",
    );
    assert!(
        !viewport.contains("expanded detail row"),
        "committed content is still in the live viewport: {viewport:?}",
    );
}

/// The `@@` header plus this many additions, which is far past [`MAX_BODY_LINES`]
/// and is what makes the cap observable at all.
const HUNK_ADDITIONS: usize = 900;

/// A hunk taller than the cap, on a path syntect has a grammar for, so the cell
/// under test is the one a real edit produces rather than a shortest path through
/// the renderer.
fn tall_edit() -> Edit {
    let mut hunk = format!("@@ -1,{HUNK_ADDITIONS} +1,{HUNK_ADDITIONS} @@\n");
    for row in 0..HUNK_ADDITIONS {
        hunk.push_str(&format!("+row {row} of a very large change\n"));
    }
    Edit {
        step: 1,
        tool: "write_file".to_string(),
        path: "notes.txt".to_string(),
        lines_added: HUNK_ADDITIONS as u64,
        lines_removed: 0,
        hunk: Some(hunk),
    }
}

#[test]
fn f5_a_hunk_past_the_cap_is_bounded_and_says_how_many_lines_it_cut() {
    let edit = tall_edit();
    let body_rows = edit
        .hunk
        .as_deref()
        .expect("the fixture has a hunk")
        .lines()
        .count();
    assert!(
        body_rows > MAX_BODY_LINES,
        "the fixture only tests the cap if it exceeds it: {body_rows} rows against {MAX_BODY_LINES}",
    );

    let rendered = rows(&io_cli::diff::cell(&edit, &DARK, 120));

    // Bounded. The cap exists because highlighting runs on the loop that also
    // delivers keystrokes and because `insert_before` takes a `u16` height: an
    // uncapped cell makes `Ctrl+C` unreachable for as long as the parse takes.
    assert!(
        rendered.len() < body_rows,
        "the whole hunk was drawn — {} lines — so the cap did nothing",
        rendered.len(),
    );
    assert!(
        rendered.len() <= MAX_BODY_LINES + 3,
        "the cell is {} lines; the cap allows {MAX_BODY_LINES} body rows plus a header, \
         the notice and the trailing blank",
        rendered.len(),
    );

    // And it says what it cut. A cap that truncates silently is the defect: the
    // reader cannot tell a change that ended from a change that was cut off, and
    // the part they cannot see is the part they went looking for.
    let cut = body_rows - MAX_BODY_LINES;
    let counted = cut.to_string();
    let notice_at = rendered
        .iter()
        .rposition(|row| !row.trim().is_empty())
        .expect("a cell has at least a header");
    let notice = &rendered[notice_at];
    assert!(
        notice.contains(format!("{cut} more lines").as_str()),
        "the last line of a capped cell must count what it cut, and this one reads {notice:?}",
    );

    // By position within the line: the count comes before the words that say
    // where the rest of it went. A line reading "the whole of it is in the trace
    // — 401 more lines" contains both and answers the reader's question in the
    // wrong order.
    let count_at = notice
        .find(counted.as_str())
        .expect("the count is on the line");
    let trace_at = notice
        .find("trace")
        .expect("the notice says where the rest of the change is");
    assert!(
        count_at < trace_at,
        "the cut count must lead the line: {notice:?}",
    );

    // The notice is at the end of the body, under the rows it applies to, rather
    // than a header nobody reads in context.
    assert!(
        rendered[notice_at - 1].contains("row "),
        "the notice should sit under the last drawn row, but follows {:?}",
        rendered[notice_at - 1],
    );
}

// ---------------------------------------------------------------------------
// F10 — Ctrl+T commits the whole conversation, branched-away turns included.
// ---------------------------------------------------------------------------

/// What is half-typed when the transcript key is pressed.
const HALF_TYPED: &str = "a thought i was in the middle of";

#[test]
fn f10_ctrl_t_asks_for_the_transcript_and_leaves_the_composer_holding_what_was_typed() {
    let mut app = App::new(DARK, "opus-5");
    for character in HALF_TYPED.chars() {
        app.key(KeyEvent::new(KeyCode::Char(character), KeyModifiers::NONE));
    }

    let command = app.key(KeyEvent::new(KeyCode::Char('t'), KeyModifiers::CONTROL));
    assert_eq!(
        command,
        Command::Transcript,
        "Ctrl+T is the one key that puts the conversation back into the scrollback",
    );

    // The failure this prevents: a key that reaches for the whole conversation and
    // eats the prompt on the way, so looking something up costs the sentence being
    // written. Nothing about committing the transcript touches the composer, and
    // this is what keeps it that way.
    assert_eq!(
        app.composer.text(),
        HALF_TYPED,
        "the composer lost what was typed when the transcript was asked for",
    );
}

/// The three prompts the branched session is built from. The second is the one
/// the branch leaves behind, and it is the only turn in the fixture that
/// `Session::history` would no longer return.
const PLAN: &str = "sketch the cutover";
const BRANCHED: &str = "take the whole service down for it";
const KEPT: &str = "keep it up and drain the queue instead";

/// Answers every turn with one line and calls nothing, so the turns are real and
/// their content is not.
struct Talker;

impl Provider for Talker {
    async fn complete(
        &self,
        _request: CompletionRequest,
    ) -> io_harness::Result<CompletionResponse> {
        Ok(CompletionResponse {
            text: Some("here is the sketch".into()),
            ..Default::default()
        })
    }
}

async fn say(session: &mut Session, store: &Store, prompt: &str) -> i64 {
    session
        .turn(prompt, &Talker, store, &Policy::permissive(), &ApproveAll)
        .await
        .expect("a scripted turn cannot fail")
        .turn_id
}

/// A real session of three turns with the middle one branched away.
///
/// Built rather than stated: `io_harness::Transcript` and `TranscriptTurn` are
/// both `#[non_exhaustive]`, so neither can be constructed by a struct literal
/// from outside the harness. A hand-made value would also agree with the renderer
/// by construction, which is the opposite of what F10 needs to know — the
/// question is whether the renderer agrees with what the *database* recorded.
async fn branched() -> (tempfile::TempDir, io_harness::Transcript) {
    let dir = tempfile::tempdir().expect("a temp dir");
    let store = Store::memory().expect("an in-memory store");
    let mut session = Session::open(&store, dir.path()).expect("a session");

    let first = say(&mut session, &store, PLAN).await;
    say(&mut session, &store, BRANCHED).await;
    session
        .branch_from(&store, first)
        .expect("the first turn is branchable");
    say(&mut session, &store, KEPT).await;

    let transcript = session.transcript(&store).expect("a transcript");
    (dir, transcript)
}

#[tokio::test]
async fn f10_the_transcript_shows_the_branched_away_turn_and_labels_it_after_its_own_prompt() {
    let (_dir, transcript) = branched().await;
    assert_eq!(
        transcript.turns.iter().filter(|turn| turn.on_path).count(),
        2,
        "the fixture is only meaningful if one of the three turns is off the path",
    );

    let rendered = rows(&transcript::lines(&transcript, &DARK));

    // Rendered at all. This turn is the one thing in the whole product that only
    // this surface can show, and a turn that was dropped looks exactly like a turn
    // that never happened.
    let branched = row_with(&rendered, BRANCHED);
    assert!(
        branched.contains(BRANCHED_AWAY),
        "the branched-away turn must say so in words — a tone is gone under NO_COLOR \
         and gone again once the line is copied out of the terminal: {branched}",
    );

    // By position, not by membership: content, then the branch label, then the id.
    // A row carrying all three in any other order passes a `contains` and reads
    // inside out, with metadata in the column a reader skims for what was said.
    let prompt_at = branched.find(BRANCHED).expect("the prompt is on its row");
    let label_at = branched
        .find(BRANCHED_AWAY)
        .expect("the label is on its row");
    let id_at = branched
        .find("turn ")
        .expect("the turn id rides the same row");
    assert!(
        prompt_at < label_at && label_at < id_at,
        "content, then the label, then the id: {branched}",
    );

    // And the turn the model can still see carries no label. Without this half,
    // a renderer that labelled every row would pass everything above.
    let kept = row_with(&rendered, KEPT);
    assert!(
        !kept.contains(BRANCHED_AWAY),
        "a turn still on the path must not be labelled: {kept}",
    );
}

// ---------------------------------------------------------------------------
// F11 — /copy writes OSC 52 and claims nothing about the clipboard.
// ---------------------------------------------------------------------------

#[test]
fn f11_copy_resolves_to_the_answer_and_copy_diff_and_copy_patch_both_to_the_patch() {
    assert_eq!(
        parse("copy", &Keys::default(), &DARK),
        Action::Copy(Copied::Answer),
        "a bare /copy is the last answer, which is what a reader who just read one wants",
    );
    // Two spellings of one thing, because a reader who has just been shown a diff
    // types the word they were shown. A parser that knows only one of them fails
    // silently into "copy the answer", which puts the wrong text on the clipboard
    // rather than reporting that the command was not understood.
    for spelling in ["copy diff", "copy patch"] {
        assert_eq!(
            parse(spelling, &Keys::default(), &DARK),
            Action::Copy(Copied::Diff),
            "/{spelling} must reach the patch",
        );
    }
}

/// The text put on the clipboard. It holds spaces, so it cannot appear inside the
/// base64 form of itself — which is what makes "the plaintext never left the
/// process" an assertion rather than a hope.
const PAYLOAD: &str = "the answer the agent gave, verbatim";

#[test]
fn f11_the_bytes_that_leave_the_process_are_an_osc_52_sequence_carrying_the_payload() {
    let (mut screen, recorder) = support::screen(80, 24);
    screen
        .escape(&clipboard::sequence(PAYLOAD))
        .expect("the sequence is written");

    // Asserted over the bytes rather than over `sequence`'s return value: what is
    // under test is what the terminal emulator at the far end of an SSH connection
    // receives, and a renderer that built the right string and never flushed it
    // would satisfy every assertion made against the string.
    let bytes = recorder.bytes();
    let introducer = b"\x1b]52;c;";
    let start = find(&bytes, introducer)
        .unwrap_or_else(|| panic!("no OSC 52 introducer in {:?}", recorder.text()))
        + introducer.len();
    let end = start
        + bytes[start..]
            .iter()
            .position(|byte| *byte == 0x07)
            .expect("the sequence is terminated by BEL, which the whole xterm lineage accepts");

    let encoded = std::str::from_utf8(&bytes[start..end]).expect("base64 is ASCII");
    assert_eq!(
        decode(encoded),
        PAYLOAD.as_bytes(),
        "the payload does not survive the round trip, so the reader's paste is not what they copied",
    );

    // The encoding is not decoration: an OSC string ends at the first BEL or ST,
    // and a transcript can carry both. A payload written raw would terminate its
    // own sequence early and print the remainder of itself on screen.
    assert!(
        !encoded.contains(['\x07', '\x1b']),
        "the encoded middle carries a byte that ends an OSC string early",
    );
}

#[test]
fn f11_the_message_states_a_size_and_never_claims_the_clipboard_was_set() {
    // Two and a half thousand bytes, so the size is a stated fact with a unit
    // rather than a number that could have come from anywhere.
    let payload = "x".repeat(2_500);
    let said = clipboard::describe(&payload);
    assert!(
        said.contains("2.5 kB"),
        "the message must state the size — it is the one fact that makes a paste that came \
         up empty diagnosable against a terminal's own limit: {said}",
    );

    // No terminal answers an OSC 52 write. tmux drops it without `set -g
    // set-clipboard on` and several emulators cap the payload silently, so a
    // success word here is a claim this process never observed, printed at exactly
    // the moment the reader goes to paste and finds the old contents.
    let lowered = said.to_lowercase();
    for claim in ["copied", "success", "done"] {
        assert!(
            !lowered.contains(claim),
            "{claim:?} claims an acknowledgement that does not exist on the wire: {said}",
        );
    }
}

#[test]
fn f11_the_clipboard_sequence_goes_to_the_terminal_and_not_into_the_scrollback() {
    let (mut screen, recorder) = support::screen(80, 24);
    screen
        .escape(&clipboard::sequence(PAYLOAD))
        .expect("the sequence is written");
    screen
        .draw(|frame| {
            let area = frame.area();
            frame.render_widget(Paragraph::new(COMPOSER), area);
        })
        .expect("a frame after the sequence");

    // An OSC 52 write is a message to the terminal emulator, not content for the
    // screen. Routed through `commit` it would become a line of the transcript the
    // reader scrolls past — and, because `commit` renders through a widget, the
    // escape bytes would be drawn as text rather than acted on. The payload's
    // plaintext holds spaces, which base64 never emits, so its absence here is
    // proof it was not committed.
    let text = recorder.text();
    assert!(
        !text.contains(PAYLOAD),
        "the payload reached the terminal as text, which means it was committed rather \
         than sent as an escape sequence",
    );
    assert!(
        text.contains("\x1b]52;c;"),
        "the sequence itself never left the process",
    );

    let viewport = screen.viewport_text();
    assert!(
        !viewport.contains(PAYLOAD) && !viewport.contains("52;c;"),
        "the clipboard sequence is in the viewport: {viewport:?}",
    );
}

// ---------------------------------------------------------------------------
// Helpers.
// ---------------------------------------------------------------------------

/// Every rendered line as its own string, spans concatenated.
///
/// The renderer's unit is the line and every assertion above is about what a
/// single line holds and in what order. Joining the whole output into one blob
/// would make "on the same line" and "somewhere in the output" the same question.
fn rows(lines: &[Line<'_>]) -> Vec<String> {
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

/// Where `needle` starts in `haystack`, over bytes rather than text — the byte
/// stream holds escape sequences that are not valid on a `char` boundary basis to
/// search through as a string.
fn find(haystack: &[u8], needle: &[u8]) -> Option<usize> {
    haystack
        .windows(needle.len())
        .position(|window| window == needle)
}

/// Standard base64, the `+/` alphabet of RFC 4648 — the one every terminal
/// emulator implements. Spelled out here rather than imported from the crate
/// under test: decoding with the encoder's own table would assert only that the
/// table is self-consistent, and a URL-safe `-_` payload decodes to different
/// bytes in a real terminal rather than failing loudly.
const ALPHABET: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";

/// Decode a base64 string back to the bytes it was made from.
fn decode(encoded: &str) -> Vec<u8> {
    let mut bits: u32 = 0;
    let mut held = 0;
    let mut out = Vec::new();
    for byte in encoded.bytes() {
        if byte == b'=' {
            break;
        }
        let index = ALPHABET
            .iter()
            .position(|character| *character == byte)
            .unwrap_or_else(|| {
                panic!(
                    "{:?} is not a standard base64 character, so a terminal would decode this \
                     sequence to something other than what was copied",
                    byte as char,
                )
            });
        bits = (bits << 6) | index as u32;
        held += 6;
        if held >= 8 {
            held -= 8;
            out.push((bits >> held) as u8);
        }
    }
    out
}
