//! F5 — every glyph has an ASCII form and the set is chosen once.
//!
//! Two halves, and the second is the one that is easy to forget.
//!
//! **Nothing outside ASCII reaches the terminal under the ASCII set.** Asserted
//! by sweeping the rendered output of every surface for a character
//! `char::is_ascii` rejects, rather than by looking for the marks this release
//! happens to know about. A `contains` assertion over eleven known code points is
//! green the day somebody types a twelfth one into a new line, and a glyph nobody
//! remembered is exactly the failure mode: the terminal draws a replacement box,
//! the suite stays green, and the person who cannot read the row is the operator.
//!
//! **Meaning survives the substitution.** A set that mapped every mark to a space
//! would pass the sweep and destroy the product, so each class is also asserted
//! for what it is *for*: the marker still marks the selected row, the elision
//! still says how many lines it hid, the quotes still enclose the prompt, the
//! fitter still fits.
//!
//! The width arithmetic gets its own test, because the ASCII ellipsis is three
//! cells where the Unicode one is one and every fitter in the product reserves
//! room for it. This repository has shipped a clipped row three releases running,
//! and a fitter that reserves one cell then appends three is how it would have
//! happened a fourth time.
//!
//! The IO CLI mark in `io_cli::splash` is deliberately not swept. It is
//! *suppressed* rather than transliterated when it cannot be drawn — a wordmark
//! redrawn in `#` is a different and worse image wearing its name — and
//! `splash::visible` is where that decision already lives.

mod support;

use std::time::Duration;

use io_cli::glyphs::{self, Glyphs, ASCII, UNICODE};
use io_cli::picker::{fit, fit_left, Picker, Row};
use io_cli::status::Status;
use io_cli::theme::{Background, Theme, DARK, MONO};
use io_harness::{Edit, EventKind, RunEvent, TodoItem, TodoState};
use ratatui::text::Line;

/// The theme every sweep below renders in: the ordinary dark palette, fully
/// coloured, drawing in ASCII.
///
/// Coloured on purpose. The two axes are independent, and a test that reached for
/// `MONO` here would be asserting F5 against a theme that has less to draw.
fn ascii() -> Theme {
    DARK.with_glyphs(ASCII)
}

/// One rendered line as a reader would see it, spans concatenated.
fn row(line: &Line<'_>) -> String {
    line.spans
        .iter()
        .map(|span| span.content.as_ref())
        .collect()
}

/// Every rendered line joined, which is what the sweep runs over.
fn text(lines: &[Line<'_>]) -> String {
    lines.iter().map(row).collect::<Vec<_>>().join("\n")
}

/// Fail naming the surface, the offending character and its code point.
///
/// The code point is in the message because `⋅` and `·` are indistinguishable in
/// a test failure otherwise, and they are two different classes.
fn assert_ascii(surface: &str, drawn: &str) {
    if let Some(bad) = drawn.chars().find(|character| !character.is_ascii()) {
        panic!(
            "{surface} drew {bad:?} (U+{:04X}) under the ASCII set; \
             every glyph must have an ASCII form.\n{drawn}",
            bad as u32,
        );
    }
}

// ---------------------------------------------------------------------------
// The choice, and where it is made
// ---------------------------------------------------------------------------

#[test]
fn the_set_is_resolved_by_the_same_shape_the_theme_is() {
    // A flag beats everything, exactly as `NO_COLOR` does for the theme: an
    // accessibility escape hatch a configuration file can overrule is not one.
    assert_eq!(Glyphs::resolve(true, true, Some("unicode")), ASCII);

    // A named set beats the locale. Somebody who has looked at their terminal
    // knows more about its font than `LANG` does.
    assert_eq!(Glyphs::resolve(false, false, Some("unicode")), UNICODE);
    assert_eq!(Glyphs::resolve(false, true, Some("ascii")), ASCII);

    // And the locale decides when nothing else has an opinion.
    assert_eq!(Glyphs::resolve(false, true, None), UNICODE);
    assert_eq!(Glyphs::resolve(false, false, None), ASCII);

    // A name nothing answers to is ignored rather than fatal, which is the same
    // thing `Theme::by_name` does with an unknown theme in a configuration file.
    assert_eq!(Glyphs::resolve(false, true, Some("emoji")), UNICODE);
}

#[test]
fn a_locale_is_read_for_utf8_in_either_spelling_and_in_any_case() {
    assert!(glyphs::claims_utf8("en_US.UTF-8"));
    assert!(glyphs::claims_utf8("en_US.utf8"));
    assert!(glyphs::claims_utf8("C.UTF8"));
    // Case matters to nobody but the check, which is why it must not matter to
    // the check: macOS and glibc disagree about the spelling and both are common.
    assert!(glyphs::claims_utf8("ja_JP.Utf-8"));

    assert!(!glyphs::claims_utf8("C"));
    assert!(!glyphs::claims_utf8("POSIX"));
    assert!(!glyphs::claims_utf8("en_US.ISO8859-1"));
    assert!(!glyphs::claims_utf8(""));
}

#[test]
fn colour_and_glyphs_are_two_axes_and_neither_forces_the_other() {
    // `NO_COLOR` selects `MONO` and leaves the marks alone.
    let uncoloured = Theme::resolve(true, Background::Dark, Some("dark"), UNICODE);
    assert!(!uncoloured.coloured, "NO_COLOR still wins the colour axis");
    assert_eq!(
        uncoloured.glyphs, UNICODE,
        "an uncoloured terminal can still draw a middle dot",
    );

    // And the ASCII set arrives at a fully coloured theme.
    let plain = Theme::resolve(false, Background::Dark, Some("dark"), ASCII);
    assert!(
        plain.coloured,
        "asking for ASCII must not take the colour away",
    );
    assert_eq!(plain.glyphs, ASCII);
}

#[test]
fn a_theme_re_resolved_keeps_the_set_it_was_handed() {
    // The shape that makes "chosen once" true: `resolve` takes the set and cannot
    // derive one, so the three places a theme is rebuilt as a session runs —
    // `/theme`, the wizard's seed, the wizard's live preview — are obliged to hand
    // back the set startup chose rather than working out a new one.
    let chosen = ASCII;
    let first = Theme::resolve(false, Background::Dark, Some("dark"), chosen);
    let again = Theme::resolve(false, Background::Light, Some("light"), first.glyphs);
    assert_eq!(again.glyphs, ASCII, "a rebuilt theme keeps the chosen set");
    assert_eq!(
        again.name, "light",
        "and still changed the thing it was for"
    );
}

// ---------------------------------------------------------------------------
// Width arithmetic
// ---------------------------------------------------------------------------

#[test]
fn a_fitter_never_hands_back_more_than_the_room_it_was_given() {
    // Both sets, every room from nothing to past the length of the text. The
    // Unicode ellipsis is one cell and the ASCII one is three, and the whole
    // point of this test is that no caller has to know which.
    let long = "make the retry loop back off instead of hammering the endpoint";
    for set in [&UNICODE, &ASCII] {
        for room in 0..=long.chars().count() + 4 {
            let fitted = fit(long, room, set);
            assert!(
                fitted.chars().count() <= room,
                "{} fit {room} into {} characters: {fitted:?}",
                set.name,
                fitted.chars().count(),
            );
            let left = fit_left(long, room, set);
            assert!(
                left.chars().count() <= room,
                "{} fit_left {room} into {} characters: {left:?}",
                set.name,
                left.chars().count(),
            );
        }
    }
}

#[test]
fn a_shortened_string_still_says_that_it_was_shortened() {
    let long = "a prompt far longer than the room it is being given";
    let fitted = fit(long, 20, &ASCII);
    assert_eq!(fitted.chars().count(), 20);
    assert!(
        fitted.ends_with(ASCII.ellipsis),
        "the ASCII mark has to be on the end or the row lies about being whole: {fitted}",
    );
    assert!(
        fitted.starts_with("a prompt"),
        "and the beginning is what survives: {fitted}",
    );

    // From the left, for a path: the end is what identifies it.
    let path = "/Users/someone/work/deeply/nested/io-cli";
    let left = fit_left(path, 20, &ASCII);
    assert_eq!(left.chars().count(), 20);
    assert!(left.starts_with(ASCII.ellipsis), "{left}");
    assert!(left.ends_with("io-cli"), "{left}");
}

#[test]
fn the_two_sets_agree_on_every_width_the_layout_depends_on() {
    // The status line drops whole fields by counting the separator, the picker
    // places the terminal cursor just past the marker, and `sessions::rows`
    // budgets four cells for the marker and the gap. All three are arithmetic
    // over a constant, and they stay arithmetic over a constant only because
    // these two agree.
    assert_eq!(
        UNICODE.separator.chars().count(),
        ASCII.separator.chars().count(),
        "a separator of a different width moves every field-drop decision",
    );
    assert_eq!(
        UNICODE.marker.chars().count(),
        ASCII.marker.chars().count(),
        "a marker of a different width moves the cursor off the label",
    );
    assert_eq!(
        UNICODE.marker.chars().count(),
        2,
        "and both must match the two spaces an unmarked row is drawn with",
    );
}

#[test]
fn every_spinner_frame_is_exactly_one_cell_wide_in_both_sets() {
    for set in [&UNICODE, &ASCII] {
        assert!(!set.spinner.is_empty(), "{} has frames", set.name);
        for frame in set.spinner {
            assert_eq!(
                frame.to_string().chars().count(),
                1,
                "{}: {frame:?} is not one character; a spinner of uneven frames \
                 shifts the whole status line as it turns",
                set.name,
            );
        }
    }
    // Not the same frame repeated, in either set. An indicator that shows one
    // character forever looks like an indicator that has stopped.
    assert!(
        ASCII.spinner.windows(2).any(|pair| pair[0] != pair[1]),
        "the ASCII spinner has to actually turn",
    );
}

// ---------------------------------------------------------------------------
// The surfaces
// ---------------------------------------------------------------------------

#[test]
fn the_status_line_draws_in_ascii_and_still_says_everything() {
    let mut status = Status::new("anthropic/claude-opus-4");
    status.policy = Some("workspace".into());
    status.working = true;
    status.elapsed = Duration::from_secs(93);
    status.tokens = Some(12_400);
    status.context = Some(41);
    status.containment = Some("workspace-write/seatbelt".into());

    let theme = ascii();
    // Every frame of the indicator, so a set whose tenth frame is Unicode cannot
    // hide behind the first.
    for _ in 0..UNICODE.spinner.len().max(ASCII.spinner.len()) * 2 {
        let drawn = row(&status.line(120, &theme));
        assert_ascii("the status line", &drawn);
        assert!(drawn.contains("working"), "the word is the state: {drawn}");
        assert!(
            drawn.contains(ASCII.separator),
            "the fields are still separated: {drawn}",
        );
        status.advance();
    }

    // The indicator is a frame of the chosen set and of nothing else.
    let frame = status.indicator(&theme).expect("a running turn spins");
    assert!(
        ASCII.spinner.contains(&frame),
        "the indicator drew {frame:?}, which is not in the ASCII set",
    );

    // A narrow line still fits: the separator is counted from the set, so the
    // arithmetic that drops fields is fed the width it is actually drawing.
    let narrow = row(&status.line(24, &theme));
    assert_ascii("the narrow status line", &narrow);
    assert!(
        narrow.chars().count() <= 24,
        "the status line overran its width: {narrow:?}",
    );
}

#[test]
fn the_picker_marks_the_selected_row_in_ascii() {
    // Rendered through a real screen rather than by reading the spans, because
    // what F5 is about is what reaches the terminal — and because a marker that
    // is the wrong width is only visible once something has been laid out.
    let (mut screen, _recorder) = support::screen(80, 12);
    let mut picker = Picker::new(
        "Resume which session?",
        vec![
            Row::with_detail("draft a migration plan", "started 2026-08-17 02:31"),
            Row::with_detail(
                "make the retry loop back off instead of hammering the endpoint",
                "a detail long enough that the row has to be shortened to fit",
            ),
            Row::new("tidy the notes"),
        ],
    )
    .selecting(1);

    let theme = ascii();
    screen
        .draw(|frame| picker.render(frame, frame.area(), &theme))
        .expect("frame");
    let drawn = screen.viewport_text().to_string();

    assert_ascii("the picker", &drawn);

    // The marker still marks, and marks the row that is selected. This is the
    // assertion the contract's sabotage is aimed at: leaving `MARKER` Unicode
    // fails the sweep above, and marking the wrong row fails this.
    let marked: Vec<&str> = drawn
        .lines()
        .filter(|line| line.starts_with(ASCII.marker))
        .collect();
    assert_eq!(
        marked.len(),
        1,
        "exactly one row is marked; got {marked:?} from\n{drawn}",
    );
    assert!(
        marked[0].contains("make the retry loop"),
        "the marked row is the selected one: {}",
        marked[0],
    );
}

#[test]
fn a_diff_cell_draws_in_ascii_and_its_elision_still_counts() {
    let before: String = (1..=80).map(|n| format!("line {n}\n")).collect();
    let after = before.replace("line 3\n", "line three\n");
    let edit =
        Edit::measure(1, "edit_file", "src/notes.rs", &before, &after).with_hunk(&before, &after);

    let theme = ascii();
    let drawn = text(&io_cli::diff::cell(&edit, &theme, 120));
    assert_ascii("a diff cell", &drawn);
    assert!(
        drawn.contains("src/notes.rs") && drawn.contains(ASCII.separator),
        "the header still names the path and separates its fields: {drawn}",
    );

    // An absent hunk still reads as absent rather than as an empty diff.
    let bare = Edit::measure(1, "write_file", "src/new.rs", "", "one\n");
    let bare = text(&io_cli::diff::cell(&bare, &theme, 120));
    assert_ascii("a diff cell with no hunk", &bare);
    assert!(bare.contains("no diff stored"), "{bare}");
}

#[test]
fn an_elision_in_ascii_still_says_how_many_lines_went() {
    // A body far past the cell's own ceiling, so the elision line is drawn and
    // has a number on it. The number is the whole reason the class exists: an
    // elision that only said "more" would be a decoration.
    let before: String = (1..=400).map(|n| format!("line {n}\n")).collect();
    let after: String = (1..=400).map(|n| format!("changed {n}\n")).collect();
    let edit =
        Edit::measure(1, "write_file", "src/big.rs", &before, &after).with_hunk(&before, &after);

    let drawn = text(&io_cli::diff::cell(&edit, &ascii(), 120));
    assert_ascii("an elided diff cell", &drawn);

    let elided = drawn
        .lines()
        .find(|line| line.contains("more lines"))
        .unwrap_or_else(|| panic!("nothing was elided; the fixture is too small:\n{drawn}"));
    assert!(
        elided.contains(ASCII.elision),
        "the elision mark has to be there or the line reads as ordinary text: {elided}",
    );
    let count: usize = elided
        .split_whitespace()
        .find_map(|word| word.parse().ok())
        .unwrap_or_else(|| panic!("the elision states no number: {elided}"));
    assert!(count > 0, "an elision that hid nothing is not an elision");
}

#[test]
fn every_event_this_release_renders_draws_in_ascii() {
    let mut events = io_cli::events::Events::new(ascii());

    // An open call first, so `live()` has something to say and the `Step` below
    // closes a real cell rather than committing on its own.
    let call = RunEvent::new(
        1,
        1,
        EventKind::ToolCall {
            name: "read_file".into(),
            target: "src/main.rs".into(),
        },
    );
    assert!(
        events.event(&call, Duration::ZERO).is_empty(),
        "an announced call commits nothing until its step lands",
    );
    assert_ascii("the live row", &events.live());
    assert!(
        events.live().contains("read_file"),
        "the live row still names the tool: {}",
        events.live(),
    );

    let kinds = [
        EventKind::Started {
            goal: "make the failing test pass".into(),
            provider: "anthropic".into(),
        },
        EventKind::Token {
            text: "here is the plan\n".into(),
        },
        EventKind::Step {
            decision: "edited src/lib.rs".into(),
            tool_call: "read_file".into(),
            tokens: 812,
            changed: true,
        },
        EventKind::Refused {
            act: "write".into(),
            target: "/etc/hosts".into(),
            rule: Some("fs.deny".into()),
            layer: Some("workspace".into()),
        },
        EventKind::ApprovalRequested {
            act: "write".into(),
            target: "src/main.rs".into(),
        },
        EventKind::ApprovalDecided {
            act: "write".into(),
            target: "src/main.rs".into(),
            decision: "allow".into(),
        },
        EventKind::Mcp {
            server: "notes".into(),
            tool: Some("search".into()),
            ok: Some(true),
            millis: Some(42),
        },
        // The plan block, which 0.7.0 added and which this list did not have. Two
        // items on purpose: one short enough to draw whole, and one longer than
        // the eighty columns the block is fitted to, so the sweep covers the
        // ellipsis the fitter appends as well as the three state words.
        EventKind::TodoWrote {
            items: vec![
                TodoItem::new("read the current parser", TodoState::Done),
                TodoItem::new(
                    "port the tokenizer, the error paths, and everything that reads \
                     either of them, one at a time",
                    TodoState::Active,
                ),
            ],
        },
        EventKind::Finished {
            outcome: "success".into(),
            steps: 4,
            tokens: 8_912,
        },
        // 0.8.0 — styled with the pin bump to io-harness 0.65, which is what
        // made the pause exist. Its leader is a glyph like every other line's.
        EventKind::RecoveryPaused {
            attempt_id: 3,
            tool: "deploy".into(),
        },
        // 0.8.0 — the fleet. Each carries a leader and a dash, and the spawn and
        // the refusal carry an indent as well.
        EventKind::Spawned {
            child_run_id: 7,
            goal: "read every file under src/".into(),
        },
        EventKind::SpawnRefused {
            cap: "agents".into(),
        },
        EventKind::ChildCollected {
            text: "found three call sites".into(),
        },
        EventKind::ChildDetached {
            child_run_id: 11,
            after: Some(std::time::Duration::from_secs(30)),
        },
        // 0.9.0 — a background handle's whole life. Four arms that draw and one
        // that deliberately does not, and every one of them is swept: the four
        // carry a job id and a word, and `HandlePolled` is here for the same
        // reason `SpendDraw` is, because "it renders no row" is an answer this
        // list has to have checked rather than assumed.
        EventKind::HandleStarted {
            handle: 4,
            line: "npm run dev".into(),
        },
        EventKind::HandlePolled {
            handle: 4,
            bytes: 2_048,
        },
        EventKind::HandleExited {
            handle: 4,
            code: Some(1),
        },
        EventKind::HandleKilled { handle: 4 },
        EventKind::HandleOrphaned {
            handle: 4,
            reason: "the run finished".into(),
        },
        // An arm that commits nothing — the draw is a status field, not a row —
        // and it is swept anyway: the sweep's question is whether an arm can put
        // a glyph on a terminal that cannot draw it, and "it renders no row" is
        // an answer this list has to have checked rather than assumed.
        EventKind::SpendDraw {
            tokens: 21,
            remaining: Some(500),
        },
    ];

    // **The list above is checked against the renderer, not trusted.** It is
    // hand-written, and a hand-written list cannot notice that `src/events.rs`
    // grew an arm — which it did: `TodoWrote` landed in 0.7.0, was styled, was
    // committed, and was swept by nothing until this line was written.
    let mut covered: Vec<String> = kinds.iter().map(io_cli::events::kind_name).collect();
    covered.push(io_cli::events::kind_name(&call.kind));
    for kind in styled_kinds() {
        assert!(
            covered.contains(&kind),
            "src/events.rs draws a line of its own for {kind:?} and nothing here \
             renders it under the ASCII set, so a glyph in that arm reaches a \
             terminal that cannot draw it",
        );
    }

    for kind in kinds {
        let name = io_cli::events::kind_name(&kind);
        let drawn = text(&events.event(&RunEvent::new(1, 1, kind), Duration::from_millis(120)));
        assert_ascii(&format!("the {name} event"), &drawn);
    }
    assert_ascii("the flushed tail", &text(&events.flush()));
}

/// Every event kind `src/events.rs` writes a line of its own for, read out of the
/// renderer's own source.
///
/// The same shape [`support::harness_event_kinds`] uses on io-harness, and for
/// the same reason: a list copied into a test cannot notice that the thing it is
/// a list *of* has changed. The forty-one kinds that fall through to the
/// catch-all are not here and do not need to be — the catch-all draws
/// [`io_cli::events::kind_name`], which is ASCII by construction.
///
/// The match arms sit at exactly twelve spaces. Every other mention of the type
/// in that file is a `use`, a doc line or an expression, and none of them is
/// indented like this.
fn styled_kinds() -> Vec<String> {
    let source = std::fs::read_to_string(
        std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("src")
            .join("events.rs"),
    )
    .expect("the renderer's source is in this repository")
    .replace("\r\n", "\n");

    let mut kinds: Vec<String> = source
        .lines()
        .filter_map(|line| line.strip_prefix("            EventKind::"))
        .map(|rest| {
            let variant: String = rest
                .chars()
                .take_while(char::is_ascii_alphanumeric)
                .collect();
            let mut snake = String::new();
            for (index, character) in variant.char_indices() {
                if character.is_ascii_uppercase() && index > 0 {
                    snake.push('_');
                }
                snake.push(character.to_ascii_lowercase());
            }
            snake
        })
        .collect();
    kinds.sort();
    kinds.dedup();
    assert!(
        !kinds.is_empty(),
        "no match arm was found; the renderer moved and this check is now blind",
    );
    kinds
}

#[test]
fn the_resume_rows_draw_in_ascii_and_keep_the_facts_that_fit() {
    let sessions = [io_cli::sessions::Recent {
        id: 7,
        root: "/Users/someone/work/very/deeply/nested/directories/that/go/on/io-cli".into(),
        turns: 6,
        prompt: "make the retry loop back off instead of hammering the endpoint".into(),
        at: "2026-08-17 02:31".into(),
    }];
    let rows = io_cli::sessions::rows(&sessions, 80, &ASCII);
    let row = rows.first().expect("one session, one row");
    let detail = row.detail.clone().unwrap_or_default();

    assert_ascii("a resume row's label", &row.label);
    assert_ascii("a resume row's detail", &detail);
    assert!(
        detail.contains("io-cli"),
        "the path's end survives: {detail}"
    );
    assert!(
        detail.contains("6 turns"),
        "the turn count is whole: {detail}"
    );

    // Drawn as well as built: a row that is one cell over its budget is only
    // visible once the picker has laid it out.
    let (mut screen, _recorder) = support::screen(80, 8);
    let mut picker = Picker::new("Resume which session?", rows);
    let theme = ascii();
    screen
        .draw(|frame| picker.render(frame, frame.area(), &theme))
        .expect("frame");
    assert_ascii("the resume picker", screen.viewport_text());
}

#[test]
fn a_rewind_quotes_its_prompt_in_ascii_and_still_discloses_the_loss() {
    let armed = io_cli::rewind::armed_line(
        &io_cli::rewind::Preview {
            turn_id: 7,
            run_id: 11,
            prompt: "tidy the notes and add a summary of everything decided today".into(),
        },
        &ASCII,
    );
    assert_ascii("the armed rewind line", &armed);
    // The quotation is what tells the operator which turn is about to go, so it
    // has to still be a quotation and not two bare words in a sentence.
    assert!(
        armed.contains(&format!("{}tidy the notes", ASCII.quote_open)),
        "the prompt is still quoted: {armed}",
    );
    assert!(
        armed.contains("edited by hand"),
        "and the disclosure is untouched: {armed}",
    );

    let undone = io_cli::rewind::undone_lines(
        &io_cli::rewind::Undone {
            prompt: "rewrite everything".into(),
            restored: vec!["notes.md".into()],
            declined: vec![("kept.md".into(), "changed since the run".into())],
            memory_restored: 1,
            memory_removed: 0,
            queue_cleared: 0,
            head: Some(4),
        },
        &ASCII,
    );
    let drawn = undone
        .iter()
        .map(|(_, line)| line.as_str())
        .collect::<Vec<_>>()
        .join("\n");
    assert_ascii("the rewind report", &drawn);
    assert!(
        drawn.contains("kept.md") && drawn.contains("changed since the run"),
        "a decline still names the path and the harness's reason: {drawn}",
    );
}

#[test]
fn the_wizard_sample_draws_in_ascii() {
    let theme = ascii();
    let drawn = text(&io_cli::wizard::sample(&theme));
    assert_ascii("the wizard's sample transcript", &drawn);
    assert!(
        drawn.contains("preview"),
        "it still says what it is: {drawn}",
    );
    assert!(
        drawn.lines().any(|line| line.starts_with(ASCII.marker)),
        "the sample's prompt still carries the marker: {drawn}",
    );
    // The uncoloured theme is drawn with the same set, which is the pairing the
    // two-axes rule exists for: no colour and no Unicode at once is the worst
    // terminal this product supports, and it still has to be readable.
    assert_ascii(
        "the wizard's sample without colour",
        &text(&io_cli::wizard::sample(&MONO.with_glyphs(ASCII))),
    );
}

#[tokio::test]
async fn a_committed_transcript_draws_in_ascii() {
    use io_harness::provider::{CompletionRequest, CompletionResponse};
    use io_harness::{ApproveAll, Policy, Provider, Session, Store};

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

    let dir = tempfile::tempdir().expect("a temp dir");
    let store = Store::memory().expect("an in-memory store");
    let mut session = Session::open(&store, dir.path()).expect("a session");
    for prompt in ["draft a migration plan", "do it with a read-only window"] {
        session
            .turn(prompt, &Talker, &store, &Policy::permissive(), &ApproveAll)
            .await
            .expect("a scripted turn cannot fail");
    }

    let theme = ascii();
    let transcript = session.transcript(&store).expect("a transcript");
    let drawn = text(&io_cli::transcript::lines(&transcript, &theme));
    assert_ascii("a committed transcript", &drawn);

    // The rule still frames it. Without an edge the passage is indistinguishable
    // from whatever the shell printed before `io` started, which is the whole
    // reason the transcript has one.
    let rule = format!("{0}{0}{0}", ASCII.rule);
    assert!(
        drawn.starts_with(&rule) && drawn.lines().last().is_some_and(|l| l.starts_with(&rule)),
        "the transcript has no visible edges: {drawn}",
    );
    assert!(drawn.contains("2 turns"), "the count survives: {drawn}");
    assert!(
        drawn
            .lines()
            .any(|line| line.starts_with(ASCII.marker) && line.contains("draft a migration plan")),
        "a prompt still carries the marker: {drawn}",
    );

    // And the fork rows, which are the same turns through the other renderer.
    let history = session
        .history(&store)
        .expect("the conversation's own path");
    for turn in io_cli::sessions::turn_rows(&history, 80, &ASCII) {
        assert_ascii("a fork row's label", &turn.label);
        assert_ascii("a fork row's detail", &turn.detail.unwrap_or_default());
    }
}

#[tokio::test]
async fn the_approval_overlay_draws_in_ascii() {
    use io_cli::approval::{self, Approval};
    use io_harness::{Act, ApprovalContext, Approver, Decision, Request};

    let directory = tempfile::tempdir().expect("a temporary directory");
    let target = directory.path().join("parse.rs");
    let before: String = (1..=40).map(|n| format!("fn f{n}() {{}}\n")).collect();
    let after = before.replace("fn f3() {}\n", "fn f3(s: &str) {}\n");
    std::fs::write(&target, &before).expect("the file exists first");

    let (asker, mut asks) = approval::channel();
    let request =
        Request::new(Act::Write, target.to_string_lossy().to_string()).with_content(after.clone());
    let deciding = tokio::spawn(async move {
        let asker = asker;
        asker
            .decide_in_context(&request, &ApprovalContext::new("tidy the parser"))
            .await
    });
    let ask = asks.recv().await.expect("the question arrived");
    let approval = Approval::new(ask, std::path::Path::new(""));

    // Four rows, which is a session's viewport and the size at which the overlay
    // has to elide — so the sweep covers the elision, the marker on the answers
    // row and the ellipsis `fit_line` appends, all at once.
    let (mut screen, _recorder) = support::screen_of(80, 8, 4);
    let theme = ascii();
    screen
        .draw(|frame| approval.render(frame, frame.area(), &theme))
        .expect("frame");
    let drawn = screen.viewport_text().to_string();

    assert_ascii("the approval overlay", &drawn);
    assert!(
        drawn.contains("warning:"),
        "the act still carries its tone's word: {drawn}",
    );
    for line in drawn.lines() {
        assert!(
            line.chars().count() <= 80,
            "a row overran the terminal: {line:?}",
        );
    }

    approval.answer(approval::Answer::Deny);
    let decision = deciding.await.expect("the approver did not panic");
    assert!(matches!(decision, Decision::Deny { .. }));
}
