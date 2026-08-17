//! F1 — `--plain` runs a full session and animates nothing.
//!
//! The criterion has four halves and they are asserted separately, because a
//! single "plain mode works" test is one that passes for whichever half happens
//! to be implemented.
//!
//! **Nothing turns.** Asserted twice over, at two different distances from the
//! screen. [`io_cli::status::Status::indicator`] is `None` at every paint, which
//! is the direct reading of the criterion and the one gate the mode passes
//! through; and the recorded byte stream carries no frame of either spinner set,
//! which is the reading that survives somebody deciding the indicator is not the
//! only way a frame could reach a terminal. The second is not redundant with the
//! first: a mode threaded to every surface except the indicator is exactly the
//! sabotage the contract names, and it is the byte stream that catches a frame
//! arriving by any other route.
//!
//! **The state field carries no prefix.** The status line's own words are
//! `working` and `ready`, and in plain mode they stand alone. This is the
//! assertion that catches every frame including the ASCII pipe — which is
//! indistinguishable from the ASCII separator once it has been rendered into a
//! row, so a row-level search for `| working` would be green whatever happened.
//!
//! **Every state change is in the scrollback.** The default interface says
//! *whether a turn is running* only in the viewport, with a word that repaints
//! and a spinner that moves; plain mode commits that transition as text. Every
//! other state a run produces is a `RunEvent`, and
//! [`io_cli::events::Events::event`] already commits a line for every one of the
//! fifty kinds — so plain mode is a second *consumer* of that stream and adds
//! exactly one thing to it, rather than being a second renderer of the same run.
//!
//! **The file and the flag are one mode.** `[app.io-cli] plain = true` and
//! `--plain` meet in [`io_cli::settings::plain`], a pure function in the library
//! precisely so that this file can drive it — `src/main.rs` cannot be linked by
//! an integration test, so a decision written there is one nothing can check.
//!
//! Nothing here sleeps, measures or reads a clock: the session's age is handed to
//! `App::tick` and `App::event` by the driver, so a test states the ages it wants
//! and `tests/timing.rs` stays true over this file too.

mod support;

use std::time::Duration;

use io_cli::app::App;
use io_cli::glyphs::{Glyphs, ASCII_SPINNER};
use io_cli::settings::CliSettings;
use io_cli::status::SPINNER;
use io_cli::theme::{Theme, DARK};
use io_harness::{ApproveAll, Policy, Session, Steer, Store};
use ratatui::text::Line;
use support::Scripted;

/// The prompt the scripted turn is driven with.
const GOAL: &str = "write the note";

/// The file the script writes, by a relative path.
///
/// Relative on purpose: the turn runs in a temporary directory whose name is
/// different every time, and a fixture that put an absolute path into the
/// transcript would make the "the file and the flag produce the same session"
/// comparison fail for a reason that has nothing to do with plain mode.
const NOTE: &str = "notes.txt";

/// The ASCII frames a byte stream can be searched for without ambiguity.
///
/// `|` is left out, and that omission is the reason this constant exists rather
/// than [`ASCII_SPINNER`] being used directly. The ASCII separator is `" | "`, so
/// a status line reading `model | working | 0s` contains the byte sequence
/// `"| working"` whether or not anything is spinning: a search for it is green
/// against a still session and would stay green against a turning one. The other
/// three frames appear nowhere else, and a spinner that turns shows all four —
/// so three of them catch it just as certainly and nothing false can pass.
const UNAMBIGUOUS_ASCII: [char; 3] = ['/', '-', '\\'];

/// What one turn, driven to completion and painted, left behind.
struct Driven {
    /// Every byte written to the terminal.
    bytes: String,
    /// What `Status::indicator` answered at each paint.
    indicators: Vec<Option<char>>,
    /// The status line's state field at each paint, as text.
    states: Vec<String>,
    /// The last row of the viewport at each paint.
    rows: Vec<String>,
    /// Everything committed to the terminal's scrollback, one string per line.
    committed: Vec<String>,
}

impl Driven {
    /// Every committed line joined, for the assertions about what is *in* the
    /// scrollback rather than about where in it.
    fn scrollback(&self) -> String {
        self.committed.join("\n")
    }
}

/// A line's text, with the styling dropped.
fn text_of(line: &Line<'_>) -> String {
    line.spans
        .iter()
        .map(|span| span.content.as_ref())
        .collect()
}

/// Drive one scripted turn to completion in the given mode, painting throughout.
///
/// The shape is `src/main.rs`'s `turn` reproduced: start, then for every event a
/// few ticks and then the event, then finish — each followed by the same
/// commit-then-draw the driver does. It is reproduced rather than called because
/// the binary cannot be linked from here, and the property under test is about
/// the pair: a `Status` that answers correctly and a driver that paints a frame
/// anyway would still put a spinner on the terminal.
///
/// The events are collected first and replayed second. `bridge::channel` is
/// unbounded — deliberately, so that a slow interface can never stall a run — so
/// draining it after the turn returns loses nothing and gives every event the
/// same treatment, which is what makes two runs comparable.
async fn drive(plain: bool) -> Driven {
    let dir = tempfile::tempdir().expect("a workspace");
    let store = Store::memory().expect("an in-memory store");
    let mut session = Session::open(&store, dir.path()).expect("a session");

    let (_steer, inbox) = Steer::channel();
    let (observer, mut events) = io_cli::bridge::channel();
    session
        .turn_steered(
            GOAL,
            &Scripted::writing(&[(NOTE, "hello\n")]),
            &store,
            &Policy::permissive(),
            &ApproveAll,
            &observer,
            &inbox,
        )
        .await
        .expect("a scripted turn cannot fail");

    let mut collected = Vec::new();
    while let Ok(event) = events.try_recv() {
        collected.push(event);
    }
    assert!(
        !collected.is_empty(),
        "the scripted turn emitted no events, so this test would assert nothing",
    );

    // Exactly what `run` does: the set is resolved from the mode, and the theme is
    // handed the set rather than deriving one. `DARK` rather than a resolved theme
    // so that the run is fully coloured — `Status::indicator` already returns
    // `None` on an uncoloured theme, and a test whose theme was `MONO` would pass
    // without plain mode having done anything at all.
    let glyphs = Glyphs::resolve(plain, true, None);
    let theme: Theme = DARK.with_glyphs(glyphs);

    let (mut screen, recorder) = support::screen(80, 24);
    let mut app = App::new(theme, "anthropic/claude-sonnet-4");
    app.set_plain(plain);

    let mut out = Driven {
        bytes: String::new(),
        indicators: Vec::new(),
        states: Vec::new(),
        rows: Vec::new(),
        committed: Vec::new(),
    };

    app.started();
    paint(&mut screen, &mut app, &mut out);

    let mut age = Duration::ZERO;
    for event in &collected {
        // Three ticks between events, so the frame counter is driven well past
        // the length of either set and a spinner that turned would have shown
        // every frame it has rather than only its first.
        for _ in 0..3 {
            age += Duration::from_millis(100);
            if app.tick(age) {
                paint(&mut screen, &mut app, &mut out);
            }
        }
        app.status.elapsed = age;
        app.event(event, age);
        paint(&mut screen, &mut app, &mut out);
    }

    app.finished();
    paint(&mut screen, &mut app, &mut out);

    out.bytes = recorder.text();
    out
}

/// One paint, recording what the session was saying as it happened.
fn paint(screen: &mut io_cli::term::Screen<support::Fixed>, app: &mut App, out: &mut Driven) {
    let pending = app.take_pending();
    if !pending.is_empty() {
        out.committed.extend(pending.iter().map(text_of));
        screen.commit(&pending).expect("commit");
    }
    out.indicators.push(app.status.indicator(&app.theme));
    out.states.push(state_field(app));
    screen
        .draw(|frame| app.render(frame, frame.area()))
        .expect("frame");
    out.rows.push(
        screen
            .viewport_text()
            .lines()
            .next_back()
            .unwrap_or_default()
            .to_string(),
    );
}

/// The status line's state field, as the line renders it.
///
/// Read off `Status::fields` rather than off the drawn row, because the ASCII
/// separator and the ASCII spinner's first frame are the same character: once a
/// row has been assembled there is no way to tell `working` behind a separator
/// from `working` behind a frame, and the assertion that matters is exactly that
/// distinction.
fn state_field(app: &App) -> String {
    app.status
        .fields(&app.theme)
        .into_iter()
        .find(|field| field.text.ends_with("working") || field.text == "ready")
        .map(|field| field.text)
        .expect("the status line always carries a state field")
}

#[tokio::test]
async fn f1_a_plain_turn_emits_no_spinner_frame_in_the_byte_stream() {
    let plain = drive(true).await;

    // The braille set, searched for bare. Nothing else in this product draws a
    // braille code point, so a single one of these in the stream is proof of both
    // failures at once: the animation ran, and the set `--plain` forces was not
    // the set the session drew with.
    for frame in SPINNER {
        assert!(
            !plain.bytes.contains(frame),
            "a plain session wrote the braille spinner frame {frame:?} to the terminal",
        );
    }
    // The ASCII set, searched for as the prefix it would be — and searched for in
    // the RENDERED ROW rather than in the byte stream, which is a distinction the
    // control below paid for. The frames are ordinary punctuation that appears
    // legitimately all over a byte stream, so the only safe thing to look for is
    // the one position a frame can occupy: immediately in front of the state word
    // it is evidence for. But that pair is never on the wire together. ratatui
    // writes the cells that CHANGED, and across a turn the frame changes while
    // `working` does not — so the glyph is written alone and a byte-stream search
    // for the pair is green whatever the spinner did. The composed row is where
    // the two are adjacent, and a row is what a reader sees.
    for frame in UNAMBIGUOUS_ASCII {
        let prefixed = format!("{frame} working");
        assert!(
            !plain.rows.iter().any(|row| row.contains(&prefixed)),
            "a plain session drew {prefixed:?} in the status row",
        );
    }
    // Not vacuous: the state the spinner was evidence for did reach the terminal,
    // in words. A run that painted nothing at all would pass every loop above.
    assert!(
        plain.bytes.contains("working"),
        "a plain session never said it was working, so the assertions above are empty",
    );
}

#[tokio::test]
async fn the_byte_stream_carries_frames_when_the_mode_is_off() {
    // The control for the test above, and the reason it is worth anything. If a
    // spinner frame could not reach the recorded bytes in the first place, then
    // "no frame in the byte stream" is a claim about this harness rather than
    // about plain mode. It is asserted in both of the shapes the test above uses,
    // so neither of them is resting on an unexamined assumption about when
    // ratatui rewrites a whole row.
    let lively = drive(false).await;

    assert!(
        SPINNER.iter().any(|frame| lively.bytes.contains(*frame)),
        "no spinner frame reached the terminal even with the mode off; the \
         byte-stream assertion in this file cannot see one",
    );
    // The second shape, in the rows rather than in the bytes — and **this
    // assertion is why the pair is asserted there**. Written against the byte
    // stream it failed, and it was right to: ratatui writes only the cells that
    // changed, so across a turn the spinner glyph goes out on its own while the
    // unchanged `working` beside it does not. The pair never appears on the wire,
    // which made the sibling test's prefix loop green for a reason that had
    // nothing to do with plain mode. A control that cannot fail is decoration;
    // this one failed and moved the assertion it guards.
    assert!(
        lively.rows.iter().any(|row| SPINNER
            .iter()
            .any(|frame| row.contains(&format!("{frame} working")))),
        "no frame was ever drawn in front of the state word, so the prefix \
         assertion in this file cannot see one either",
    );
}

#[tokio::test]
async fn f1_the_indicator_is_none_throughout_a_plain_turn() {
    let plain = drive(true).await;

    assert!(
        plain.indicators.iter().all(Option::is_none),
        "the indicator turned during a plain turn: {:?}",
        plain
            .indicators
            .iter()
            .flatten()
            .copied()
            .collect::<Vec<char>>(),
    );
    // And the same control, in the same shape: with the mode off the indicator
    // does turn, so `all is_none` above is a property of the mode rather than of
    // a session that never ran.
    let lively = drive(false).await;
    assert!(
        lively.indicators.iter().any(Option::is_some),
        "the indicator never turned with the mode off, so `None` proves nothing",
    );
}

#[tokio::test]
async fn f1_the_state_field_stands_alone_in_plain_mode() {
    let plain = drive(true).await;

    for state in &plain.states {
        assert!(
            state == "working" || state == "ready",
            "the status line's state field carried a prefix in plain mode: {state:?}",
        );
    }
    assert!(
        plain.states.iter().any(|state| state == "working"),
        "no paint happened while the turn was running: {:?}",
        plain.states,
    );
    assert!(
        plain.states.iter().any(|state| state == "ready"),
        "the session never came back to rest: {:?}",
        plain.states,
    );
}

#[tokio::test]
async fn f1_every_state_change_the_run_produced_is_in_the_scrollback() {
    let plain = drive(true).await;
    let scrollback = plain.scrollback();

    // io-cli's own state, which is the one the default interface says only in the
    // viewport. `started` and `finished` bracket the turn, so both transitions
    // are here whatever the harness did or did not emit.
    assert!(
        plain.committed.iter().any(|line| line == "working"),
        "the turn started and the scrollback never said so:\n{scrollback}",
    );
    assert!(
        plain.committed.iter().any(|line| line == "ready"),
        "the turn ended and the scrollback never said so:\n{scrollback}",
    );
    assert_eq!(
        plain.committed.first().map(String::as_str),
        Some("working"),
        "the first thing committed was not the turn starting:\n{scrollback}",
    );
    assert_eq!(
        plain.committed.last().map(String::as_str),
        Some("ready"),
        "the last thing committed was not the turn ending:\n{scrollback}",
    );

    // And the run's own states, which are `RunEvent`s and were already committed
    // before this release. Asserted here so that a plain mode which announced its
    // two words and dropped the stream underneath them fails: what the criterion
    // asks for is every state change, not two of them.
    assert!(
        scrollback.contains(GOAL),
        "the goal the turn was given is not in the scrollback:\n{scrollback}",
    );
    assert!(
        scrollback.contains(NOTE),
        "the tool call the run made is not in the scrollback:\n{scrollback}",
    );
    assert!(
        scrollback.contains("finished"),
        "the run's outcome is not in the scrollback:\n{scrollback}",
    );
}

#[tokio::test]
async fn the_state_words_are_plain_mode_s_own_and_the_default_does_not_commit_them() {
    // The difference has to be real in both directions. A default session says
    // `working` and `ready` in the status line, where they repaint; committing
    // them there as well would put two lines into every transcript for a state
    // that was already on screen.
    let lively = drive(false).await;
    assert!(
        !lively
            .committed
            .iter()
            .any(|line| line == "working" || line == "ready"),
        "the default session committed a status word to the scrollback: {:?}",
        lively.committed,
    );
}

#[tokio::test]
async fn f1_plain_mode_draws_with_the_ascii_set() {
    let plain = drive(true).await;

    // The frames this file searches the byte stream for are still frames. Spelled
    // out because `UNAMBIGUOUS_ASCII` is a hand-picked subset: a release that
    // respells the ASCII spinner would otherwise leave the byte-stream assertion
    // above searching for characters no spinner draws, and it would stay green
    // for exactly that reason.
    for frame in UNAMBIGUOUS_ASCII {
        assert!(
            ASCII_SPINNER.contains(&frame),
            "{frame:?} is no longer a frame of the ASCII spinner",
        );
    }

    // The separator is the mark every row of the status line is assembled from,
    // so it is the cheapest evidence that the set the session actually drew with
    // is the one `--plain` forces.
    assert!(
        plain.rows.iter().all(|row| !row.contains(" · ")),
        "a plain session drew the Unicode separator: {:?}",
        plain.rows,
    );
    assert!(
        plain.rows.iter().any(|row| row.contains(" | ")),
        "a plain session drew no ASCII separator at all: {:?}",
        plain.rows,
    );
}

#[test]
fn f1_the_flag_is_accepted_on_both_sides_of_a_subcommand() {
    use clap::Parser;
    use io_cli::cli::Cli;

    for argv in [
        vec!["io", "--plain"],
        vec!["io", "--plain", "exec", GOAL],
        vec!["io", "exec", "--plain", GOAL],
        vec!["io", "--plain", "setup"],
        vec!["io", "setup", "--plain"],
    ] {
        let cli = Cli::try_parse_from(argv.iter().copied())
            .unwrap_or_else(|error| panic!("`{}` was refused: {error}", argv.join(" ")));
        assert!(cli.plain, "`{}` did not set the mode", argv.join(" "));
    }

    let quiet = Cli::try_parse_from(["io"]).expect("a bare invocation parses");
    assert!(
        !quiet.plain,
        "the mode was on without anybody asking for it"
    );
}

#[test]
fn f1_the_flag_and_the_file_key_resolve_to_one_mode() {
    use io_cli::settings::plain;

    let asked = CliSettings {
        plain: Some(true),
        ..CliSettings::default()
    };
    let declined = CliSettings {
        plain: Some(false),
        ..CliSettings::default()
    };
    let silent = CliSettings::default();

    // The file on its own.
    assert!(
        plain(false, Some(&asked)),
        "`plain = true` was not honoured"
    );
    assert!(!plain(false, Some(&declined)));
    assert!(!plain(false, Some(&silent)), "an absent key is not a mode");
    assert!(!plain(false, None), "no file at all is not a mode");

    // The flag wins over the file, which is the whole reason this is a function
    // rather than an `unwrap_or`. It only has teeth in one direction — there is
    // no `--no-plain` — and that is the direction that matters: a mode somebody
    // switched on for accessibility must not be losable to a command line.
    assert!(plain(true, Some(&declined)), "the flag lost to the file");
    assert!(plain(true, None));
    assert!(plain(true, Some(&asked)));
}

#[tokio::test]
async fn f1_the_file_key_produces_the_same_session_as_the_flag() {
    let by_flag = io_cli::settings::plain(true, None);
    let by_file = io_cli::settings::plain(
        false,
        Some(&CliSettings {
            plain: Some(true),
            ..CliSettings::default()
        }),
    );
    assert_eq!(
        by_flag, by_file,
        "the two routes disagree before anything runs"
    );

    // Then driven, because equal booleans are not the criterion — the criterion
    // is that the same session comes out. Two runs of the same scripted turn
    // differ in nothing a transcript can see: the ages are stated by this file,
    // the script is fixed, and the workspace path never reaches a line.
    let flagged = drive(by_flag).await;
    let filed = drive(by_file).await;

    assert_eq!(
        flagged.committed, filed.committed,
        "the flag and the file committed different transcripts",
    );
    assert_eq!(
        flagged.states, filed.states,
        "the flag and the file put different words on the status line",
    );
    assert_eq!(
        flagged.rows, filed.rows,
        "the flag and the file drew different viewports",
    );
}
