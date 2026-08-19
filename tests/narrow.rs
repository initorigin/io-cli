//! F9 — 80x24 is a supported terminal size, not a degraded one.
//!
//! Every surface this release renders gets an assertion here as it lands. The
//! renderer's own obligations are the two below: the viewport survives content
//! taller than the screen, and nothing it writes is cut mid-character.

mod support;

use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use ratatui::text::Line;

const WIDTH: u16 = 80;
const HEIGHT: u16 = 24;

/// Both glyph sets, as the themes the surfaces are actually drawn with.
///
/// The arithmetic differs between them — the ellipsis is one character in Unicode
/// and three in ASCII — so a fitter that reserved the wrong number of cells clips
/// every shortened row of every surface at once, and only in one set.
fn themes() -> [io_cli::theme::Theme; 2] {
    [
        io_cli::theme::DARK.with_glyphs(io_cli::glyphs::UNICODE),
        io_cli::theme::DARK.with_glyphs(io_cli::glyphs::ASCII),
    ]
}

/// No row of `drawn` is wider than the terminal.
///
/// A bound and nothing more, which is the whole reason it is never the only
/// assertion in a test below. `Screen::viewport_text` right-trims every row, so a
/// row clipped at eighty and a row fitted to eighty are the same length here; this
/// catches a wrap, and the `ends_with(ellipsis)` assertions beside it are what
/// catch a clip.
fn within_eighty(set: &str, drawn: &str) {
    for line in drawn.lines() {
        assert!(
            line.chars().count() <= WIDTH as usize,
            "a row overflowed eighty columns ({set}): {line:?}",
        );
    }
}

/// Type at a picker, one character at a time, exactly as an operator would.
fn type_at(picker: &mut io_cli::picker::Picker, text: &str) {
    for character in text.chars() {
        picker.key(KeyEvent::new(KeyCode::Char(character), KeyModifiers::NONE));
    }
}

/// Draw a picker in the session's own four-row viewport and read the real render
/// buffer back.
///
/// Four rows because that is the only size a picker is ever drawn at in a session:
/// `paint_picker` hands it `frame.area()` of the inline viewport, whose height is
/// fixed at attach and does not grow for an overlay. A test that gave it twelve
/// rows would be auditing a screen this product does not have.
fn drawn(picker: &mut io_cli::picker::Picker, theme: &io_cli::theme::Theme) -> String {
    let (mut screen, _recorder) = support::screen(WIDTH, HEIGHT);
    screen
        .draw(|frame| picker.render(frame, frame.area(), theme))
        .expect("frame");
    screen.viewport_text().to_string()
}

/// Every built line as a reader would see it, spans concatenated.
fn text_of(lines: &[Line<'static>]) -> Vec<String> {
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

#[test]
fn f9_a_commit_taller_than_the_screen_keeps_the_viewport() {
    let (mut screen, recorder) = support::screen(WIDTH, HEIGHT);

    // Forty lines into a twenty-four-line terminal. `insert_before` loops here,
    // drawing a screenful at a time, and it is the path where a viewport is most
    // easily scrolled off and lost.
    let lines: Vec<Line> = (0..40).map(|n| Line::from(format!("line {n}"))).collect();
    screen.commit(&lines).expect("tall commit");
    screen
        .draw(|frame| {
            frame.render_widget(ratatui::widgets::Paragraph::new("> ready"), frame.area());
        })
        .expect("frame");

    assert!(
        screen.viewport_text().contains("> ready"),
        "the viewport was lost behind a commit taller than the screen: {:?}",
        screen.viewport_text(),
    );
    assert!(
        recorder.contains("line 39"),
        "the last committed line never reached the terminal",
    );
}

#[test]
fn f9_a_line_wider_than_the_terminal_wraps_rather_than_truncating() {
    let (mut screen, recorder) = support::screen(WIDTH, HEIGHT);

    // Two hundred characters into eighty columns: three rows, and every character
    // has to survive. Truncation here would silently drop the end of a model's
    // answer, which is the content this whole renderer exists to preserve.
    let long = "abcdefghij".repeat(20);
    screen.commit(&[Line::from(long.clone())]).expect("commit");

    let written = recorder.text().matches("abcdefghij").count();
    assert_eq!(
        written, 20,
        "{written} of 20 repeats survived; the line was truncated at the terminal width",
    );
}

#[test]
fn f9_multibyte_text_is_not_split_across_a_wrap() {
    let (mut screen, recorder) = support::screen(WIDTH, HEIGHT);

    // Wide characters are two cells each, so a naive byte- or char-count wrap puts
    // half a character at the edge and produces mojibake on copy — the failure the
    // alternate-screen renderers are reported for.
    let wide = "日本語テキスト".repeat(10);
    screen.commit(&[Line::from(wide.clone())]).expect("commit");

    let text = recorder.text();
    assert!(
        !text.contains('\u{fffd}'),
        "the byte stream contains a replacement character, so a multi-byte character was split",
    );
    // Counted per character rather than per repeat: a wrap legitimately lands
    // between two characters, and crossterm emits a cursor move there, so the
    // seven-character substring is not contiguous in the stream. What must hold is
    // that no character was dropped.
    for glyph in "日本語テキスト".chars() {
        let written = text.matches(glyph).count();
        assert_eq!(
            written, 10,
            "{written} of 10 {glyph:?} survived a wrap at eighty columns",
        );
    }
}

/// **N5.** The approval overlay at eighty columns, with a long path and content
/// wider than the terminal. It is the widest thing this product draws, and it is
/// the one surface where a line that overflows costs somebody a decision they did
/// not mean to make.
#[tokio::test]
async fn n5_the_approval_overlay_fits_eighty_columns() {
    use io_cli::approval::{self, Approval};
    use io_cli::theme::DARK;
    use io_harness::{Act, ApprovalContext, Approver, Decision, Request};

    let target = "crates/some-rather-long-crate-name/src/subsystem/module/implementation.rs";
    let content = format!("{}\n{}\n", "x".repeat(200), "y".repeat(200));
    let (asker, mut asks) = approval::channel();
    let deciding = tokio::spawn(async move {
        let asker = asker;
        asker
            .decide_in_context(
                &Request::new(Act::Write, target).with_content(content),
                &ApprovalContext::new("tidy the parser").flagged_by(
                    Some("crates/**/src/**/*.rs".into()),
                    Some("ops-baseline".into()),
                ),
            )
            .await
    });
    let ask = asks.recv().await.expect("the question arrived");

    let approval = Approval::new(ask, std::path::Path::new(""));
    let (mut screen, _recorder) = support::screen(WIDTH, HEIGHT);
    screen
        .draw(|frame| approval.render(frame, frame.area(), &DARK))
        .expect("frame");

    let viewport = screen.viewport_text();
    for line in viewport.lines() {
        assert!(
            line.chars().count() <= WIDTH as usize,
            "the overlay overflowed eighty columns: {line:?}",
        );
    }
    // The answers are what the overlay exists for. A layout that fits by pushing
    // them off the bottom has fitted nothing.
    assert!(
        viewport.contains("allow once") && viewport.contains("deny"),
        "the answers must survive a narrow terminal: {viewport:?}",
    );
    // And so must the act and the target, in some form.
    assert!(viewport.contains("write"), "{viewport:?}");
    // Something was cut and the overlay said so. Two markers can carry that: `…`
    // when a single value was shortened to fit, and `⋯` when whole rows of a
    // change were left out. 0.3.0 made the second the usual one here — the
    // overlay's diff row leads with the counts rather than repeating the path, so
    // the path no longer needs shortening and the rows do.
    assert!(
        viewport.contains('…') || viewport.contains('⋯'),
        "something was cut and nothing said so: {viewport:?}",
    );
    // The two facts no other core records must survive the narrow form. A widget
    // that "fits" by letting ratatui clip the row has not fitted anything — it has
    // silently cut the half of the sentence this release exists to show.
    assert!(
        viewport.contains("ops-baseline"),
        "the layer was cut at eighty columns: {viewport:?}",
    );

    approval.answer(approval::Answer::Deny);
    let decision = deciding.await.expect("the approver did not panic");
    assert!(matches!(decision, Decision::Deny { .. }));
}

/// **N5.** A diff at eighty columns, and the emphasis floor below a hundred.
///
/// Nothing here is about truncation — a committed line wraps, which
/// `f9_a_line_wider_than_the_terminal_wraps_rather_than_truncating` already
/// proves for the renderer. What is asserted is that the cell's own facts survive
/// the narrow form, and that word-level emphasis gives way to the line rather
/// than leaving a bolded fragment somewhere in the middle of a three-row wrap.
mod diff {
    use io_cli::diff::{cell, EMPHASIS_FLOOR};
    use io_cli::theme::DARK;
    use io_harness::Edit;
    use ratatui::style::Modifier;

    /// A path longer than eighty columns on its own, and body lines wider still.
    fn wide_edit() -> Edit {
        Edit {
            step: 1,
            tool: "edit_file".to_string(),
            path: "crates/some-rather-long-crate-name/src/subsystem/module/implementation.rs"
                .to_string(),
            lines_added: 1,
            lines_removed: 1,
            hunk: Some(format!(
                "@@ -12,3 +12,3 @@\n \
                 fn one() {{}}\n\
                 -let value = {};\n\
                 +let value = {};\n",
                "x".repeat(200),
                "y".repeat(200),
            )),
        }
    }

    fn text(edit: &Edit, width: u16) -> String {
        cell(edit, &DARK, width)
            .iter()
            .map(|line| {
                line.spans
                    .iter()
                    .map(|span| span.content.as_ref())
                    .collect::<String>()
            })
            .collect::<Vec<_>>()
            .join("\n")
    }

    #[test]
    fn n5_the_header_keeps_its_facts_at_eighty_columns() {
        let edit = wide_edit();
        let rendered = text(&edit, super::WIDTH);

        // The path is longer than the terminal, so the counts sit past the wrap.
        // 0.2.0 paid for the lesson that a row can "fit" because ratatui clipped
        // the half of the sentence that mattered — here the facts must be in the
        // cell whatever the wrap does with them.
        assert!(rendered.contains(&edit.path), "{rendered}");
        assert!(
            rendered.contains("+1"),
            "the additions are gone: {rendered}"
        );
        assert!(rendered.contains("-1"), "the removals are gone: {rendered}");
        assert!(rendered.contains("edit_file"), "{rendered}");
    }

    #[test]
    fn n5_nothing_in_the_body_is_dropped_at_eighty_columns() {
        let rendered = text(&wide_edit(), super::WIDTH);
        assert_eq!(
            rendered.matches(&"x".repeat(200)).count(),
            1,
            "the removed line was cut rather than left to wrap: {rendered}",
        );
        assert_eq!(rendered.matches(&"y".repeat(200)).count(), 1, "{rendered}");
        assert!(rendered.contains("@@ -12,3 +12,3 @@"), "{rendered}");
    }

    #[test]
    fn n5_below_the_floor_the_emphasis_is_the_line_and_not_a_word() {
        let edit = wide_edit();

        let wide = cell(&edit, &DARK, EMPHASIS_FLOOR);
        let emphasised = |lines: &[ratatui::text::Line<'static>]| {
            lines
                .iter()
                .flat_map(|line| line.spans.iter())
                .filter(|span| span.style.add_modifier.contains(Modifier::BOLD))
                .count()
        };
        assert!(
            emphasised(&wide) > 0,
            "at the floor itself the word-level emphasis still applies",
        );

        let narrow = cell(&edit, &DARK, EMPHASIS_FLOOR - 1);
        assert_eq!(
            emphasised(&narrow),
            0,
            "below the floor a changed line takes the whole wash, so no fragment \
             of it is bolded inside a wrap",
        );

        // And the line still reads as changed: the wash is the carrier now.
        let removed = narrow
            .iter()
            .find(|line| {
                line.spans
                    .iter()
                    .map(|span| span.content.as_ref())
                    .collect::<String>()
                    .trim_start()
                    .starts_with('-')
            })
            .expect("a removed line");
        assert!(
            removed
                .spans
                .iter()
                .any(|s| s.style.fg == Some(DARK.diff_delete)),
            "a line with no emphasis must still be coloured as removed",
        );
    }
}

#[test]
fn n5_the_resume_picker_at_eighty_columns_keeps_the_end_of_the_path() {
    // The resume picker is the first surface in this product to put a path from
    // outside the current workspace on screen, and paths are exactly the content
    // that does not fit. Asserted through a real render buffer rather than over
    // the row strings, because what matters is what reaches the terminal: twice
    // now this product has shipped a row whose load-bearing half was the half
    // ratatui clipped, and a string assertion cannot see that happen.
    let (mut screen, _recorder) = support::screen(WIDTH, HEIGHT);

    let sessions = [io_cli::sessions::Recent {
        id: 7,
        root: "/Users/someone/work/very/deeply/nested/directories/that/go/on/io-cli".into(),
        turns: 6,
        prompt: "make the retry loop back off instead of hammering the endpoint".into(),
        at: "2026-08-17 02:31".into(),
    }];
    let rows = io_cli::sessions::rows(&sessions, WIDTH, &io_cli::theme::DARK.glyphs);
    let mut picker = io_cli::picker::Picker::new("Resume which session?", rows);

    screen
        .draw(|frame| picker.render(frame, frame.area(), &io_cli::theme::DARK))
        .expect("frame");
    let drawn = screen.viewport_text();

    assert!(
        drawn.contains("io-cli"),
        "the end of the path is what identifies the session and it must survive: {drawn:?}",
    );
    assert!(
        !drawn.contains("/Users/someone/work/very"),
        "the beginning of the path is the same on every row and is what should go: {drawn:?}",
    );
    assert!(
        drawn.contains("6 turns"),
        "the turn count is a load-bearing fact and must not be the part that is cut: {drawn:?}",
    );
    for line in drawn.lines() {
        assert!(
            line.chars().count() <= WIDTH as usize,
            "a row overflowed eighty columns: {line:?}",
        );
    }
}

#[test]
fn n5_the_fork_picker_at_eighty_columns_keeps_the_turn_number() {
    // The fork picker's rows are prompts, which are arbitrarily long, and its
    // load-bearing fact is which turn a row *is* — that is what the operator is
    // choosing. It sits in the detail, which the picker shortens from the right,
    // so this asserts the number is still there after the shortening.
    let (mut screen, _recorder) = support::screen(WIDTH, HEIGHT);

    let long = "explain in as much detail as you possibly can, at length, without \
                stopping, exactly why the retry loop hammers the endpoint";
    let turns = vec![turn(1, 11, "first, read the client"), turn(2, 12, long)];
    let rows = io_cli::sessions::turn_rows(&turns, WIDTH, &io_cli::theme::DARK.glyphs);
    let mut picker = io_cli::picker::Picker::new("Continue from which turn?", rows);

    screen
        .draw(|frame| picker.render(frame, frame.area(), &io_cli::theme::DARK))
        .expect("frame");
    let drawn = screen.viewport_text();

    assert!(
        drawn.contains("turn 1") && drawn.contains("turn 2"),
        "both turn numbers must survive the shortening: {drawn:?}",
    );
    for line in drawn.lines() {
        assert!(
            line.chars().count() <= WIDTH as usize,
            "a row overflowed eighty columns: {line:?}",
        );
    }
}

/// **F12 — the picker fits eighty columns, title and label included.**
///
/// The two surfaces this file could not see until 0.6.0 were the picker's title
/// and its row label: `Picker::render` passed only the *detail* through
/// `picker::fit`, and the other two went to a viewport `Paragraph` raw. `/resume`
/// and `/fork` escaped it only because `sessions::rows` happens to shorten a
/// prompt to a third of the width before the widget ever sees it — a property of
/// one caller, not of the widget. The wizard's model step, whose labels are model
/// ids straight out of a provider's catalogue, had no such caller.
///
/// **Every assertion here is on surviving content, and the load-bearing one is
/// `ends_with(ellipsis)`.** `Screen::viewport_text` right-trims each row
/// (`term.rs:292`), so a row clipped at eighty and a row fitted to eighty are the
/// same length and `chars().count() <= 80` cannot tell them apart — it fails only
/// on a wrap, which is not the failure this product keeps shipping. A *fitted* row
/// ends with the whole mark; a *clipped* row is the mark's last character short,
/// which is one cell in the Unicode set and one of three dots in the ASCII set.
/// That is exactly the contract's sabotage — one cell of overrun in the `used`
/// budget — and it is why the ellipsis is asserted at the end of the row rather
/// than merely somewhere in the frame.
///
/// Both glyph sets, because the arithmetic differs between them: the ellipsis is
/// one character in Unicode and three in ASCII, and a fitter that reserved the
/// wrong number of cells would clip every shortened row of every surface at once.
#[test]
fn f12_the_picker_fits_eighty_columns_with_a_long_title_and_a_long_label() {
    for glyphs in [io_cli::glyphs::UNICODE, io_cli::glyphs::ASCII] {
        let set = glyphs.name;
        let mut theme = io_cli::theme::DARK;
        theme.glyphs = glyphs;
        let mark = glyphs.ellipsis;

        let mut picker = io_cli::picker::Picker::new(
            "Which model should this session run against, of the ones this provider \
             is actually serving today?",
            vec![
                // A label longer than the terminal, which is what a model id from
                // a self-hosted catalogue looks like. It leaves no room for a
                // detail at all, which is correct: the label identifies the row.
                io_cli::picker::Row::with_detail(
                    "a-self-hosted-vendor/an-extremely-long-model-identifier-that-nobody-would-\
                     ever-type-by-hand-v2",
                    "128k context",
                ),
                // A short label with a long detail: the row whose width is decided
                // by the `used` budget, and therefore the row the contract's
                // one-cell sabotage lands on.
                io_cli::picker::Row::with_detail(
                    "gpt-5",
                    "served direct by the vendor, with the reference price list and no proxy \
                     in front of it",
                ),
            ],
        );

        let (mut screen, _recorder) = support::screen(WIDTH, HEIGHT);
        screen
            .draw(|frame| picker.render(frame, frame.area(), &theme))
            .expect("frame");
        let drawn = screen.viewport_text().to_string();
        let mut rows = drawn.lines();
        let title = rows.next().unwrap_or_default();
        let first = rows.next().unwrap_or_default();
        let second = rows.next().unwrap_or_default();

        assert!(
            title.starts_with("Which model should this session run against"),
            "the picker's title lost the beginning of its question ({set}): {drawn:?}",
        );
        assert!(
            title.ends_with(mark),
            "the picker's title was clipped rather than fitted ({set}): {title:?}",
        );

        assert!(
            first.starts_with(glyphs.marker),
            "the selection marker is the only thing that says which row Enter takes \
             and it did not survive ({set}): {drawn:?}",
        );
        assert!(
            first.contains("a-self-hosted-vendor/an-extremely-long-model-identifier"),
            "the identifying part of the selected row's label was cut ({set}): {drawn:?}",
        );
        assert!(
            first.ends_with(mark),
            "the selected row's label was clipped rather than fitted ({set}): {first:?}",
        );

        assert!(
            second.starts_with("  gpt-5"),
            "an unselected row must keep its label, in the marker's own column ({set}): {drawn:?}",
        );
        assert!(
            second.contains("served direct by the vendor"),
            "the beginning of the detail is the part that carries it ({set}): {drawn:?}",
        );
        assert!(
            second.ends_with(mark),
            "the row overran its budget and ratatui took the end of the mark off it \
             ({set}): {second:?}",
        );
    }
}

/// **F12 — every wizard step fits eighty columns.**
///
/// The wizard had no width test at any width before this one: every test in
/// `tests/wizard.rs` renders at a hundred columns, and `wizard::paragraph` does
/// not wrap. Eight-plus screens, each with its own layout, none of them ever drawn
/// at the size the contract calls supported.
///
/// The audit's finding was that the *questions* all fit — every fixed string the
/// wizard draws is under seventy characters — and the three things that do not are
/// the three that come from outside it: the provider's rejection message, the
/// configuration path, and a model id from a catalogue. Those three are fitted
/// now; the questions are asserted verbatim so that a step whose wording grows
/// past eighty columns fails here rather than losing its second half in silence.
///
/// `Step::Done` and `Step::Cancelled` are deliberately absent: they draw nothing,
/// so there is no row for a width to be wrong about.
#[test]
fn f12_every_wizard_step_fits_eighty_columns() {
    use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
    use io_cli::wizard::{Step, Wizard};

    // A configuration path longer than the sixty-four columns the confirmation
    // screen's lead text leaves for it. Fabricated rather than created: nothing is
    // written here, because the wizard hands `Progress::Write` back to a driver
    // this test never runs.
    std::env::set_var(
        "IO_CONFIG",
        "/Users/a-rather-long-account-name/Library/Preferences/io-cli/first-run/io.toml",
    );
    std::env::remove_var("IO_CONFIG_HOME");

    // Longer than the seventy-eight columns a marker leaves a picker row, so the
    // model step and the confirmation screen both have something to shorten.
    const MODEL: &str = "a-self-hosted-vendor/an-extremely-long-model-identifier-that-nobody-\
                         would-ever-type-by-hand-v2";
    // What a provider actually sends back when a key is wrong: its own prose, at
    // its own length, and none of this crate's business to shorten at the source.
    const REJECTION: &str = "401 Unauthorized: no auth credentials found for this request; \
                             check that the key is for this account and has not been revoked";

    let key = |code| KeyEvent::new(code, KeyModifiers::NONE);

    for glyphs in [io_cli::glyphs::UNICODE, io_cli::glyphs::ASCII] {
        let set = glyphs.name;
        let mark = glyphs.ellipsis;
        let mut theme = io_cli::theme::DARK;
        theme.glyphs = glyphs;

        let (mut screen, _recorder) =
            support::screen_of(WIDTH, HEIGHT, io_cli::term::WIZARD_VIEWPORT_HEIGHT);
        let mut wizard = Wizard::new(theme);

        // --- Welcome. ---
        let drawn = draw_step(&mut wizard, &mut screen);
        assert!(
            drawn.contains("Welcome. Four questions and you have a working agent."),
            "the welcome step's sentence did not survive ({set}): {drawn:?}",
        );
        assert!(
            drawn.contains("Enter to begin, Esc to leave."),
            "the welcome step's instruction did not survive ({set}): {drawn:?}",
        );

        // --- Provider. ---
        wizard.key(key(KeyCode::Enter));
        assert_eq!(wizard.step(), Step::Provider);
        let drawn = draw_step(&mut wizard, &mut screen);
        assert!(
            drawn.contains("Which provider?"),
            "the provider step lost its question ({set}): {drawn:?}",
        );
        for label in [
            "OpenRouter",
            "Anthropic",
            "OpenAI",
            "Any OpenAI-compatible endpoint",
        ] {
            assert!(
                drawn.contains(label),
                "the provider step lost the row {label:?} ({set}): {drawn:?}",
            );
        }
        assert!(
            drawn.contains("a base URL of your own"),
            "the longest row's detail lost the words that explain it ({set}): {drawn:?}",
        );

        // --- Credential, and the provider's own rejection on it. ---
        wizard.key(key(KeyCode::Enter));
        assert_eq!(wizard.step(), Step::Credential);
        let drawn = draw_step(&mut wizard, &mut screen);
        assert!(
            drawn.contains("Paste an API key"),
            "the credential step lost its question ({set}): {drawn:?}",
        );

        wizard.paste("sk-not-a-real-key");
        wizard.key(key(KeyCode::Enter));
        assert_eq!(wizard.step(), Step::Verifying);
        let drawn = draw_step(&mut wizard, &mut screen);
        assert!(
            drawn.contains("Checking the key against the provider"),
            "the verifying step lost the only sentence it draws ({set}): {drawn:?}",
        );

        wizard.rejected(REJECTION);
        assert_eq!(wizard.step(), Step::Credential);
        let drawn = draw_step(&mut wizard, &mut screen);
        let notice = drawn
            .lines()
            .find(|line| line.starts_with("error: "))
            .unwrap_or_else(|| panic!("the rejection is not on the screen ({set}): {drawn:?}"));
        assert!(
            notice.contains("401 Unauthorized: no auth credentials found"),
            "the beginning of the provider's message is the informative half and it \
             was cut ({set}): {notice:?}",
        );
        assert!(
            notice.ends_with(mark),
            "the rejection was clipped rather than fitted, so nothing on screen says \
             the message continues ({set}): {notice:?}",
        );
        assert!(
            drawn.contains("Paste an API key"),
            "the rejection pushed the question off the credential step ({set}): {drawn:?}",
        );

        // --- Model, from a catalogue whose ids are longer than the terminal. ---
        wizard.paste("sk-not-a-real-key");
        wizard.key(key(KeyCode::Enter));
        wizard.verified();
        wizard.catalogue(vec![MODEL.to_string()]);
        assert_eq!(wizard.step(), Step::Model);
        let drawn = draw_step(&mut wizard, &mut screen);
        assert!(
            drawn.contains("Which model?"),
            "the model step lost its question ({set}): {drawn:?}",
        );
        let row = drawn
            .lines()
            .find(|line| line.contains("a-self-hosted-vendor"))
            .unwrap_or_else(|| panic!("the model row is not on the screen ({set}): {drawn:?}"));
        assert!(
            row.starts_with(glyphs.marker),
            "the model row lost the marker that says Enter would take it ({set}): {drawn:?}",
        );
        assert!(
            row.ends_with(mark),
            "the model id was clipped rather than fitted ({set}): {row:?}",
        );

        // --- Theme, which is a picker with a sample transcript under it. ---
        wizard.key(key(KeyCode::Enter));
        assert_eq!(wizard.step(), Step::Theme);
        let drawn = draw_step(&mut wizard, &mut screen);
        assert!(
            drawn.contains("Which theme?"),
            "the theme step lost its question ({set}): {drawn:?}",
        );
        for name in ["dark", "light"] {
            assert!(
                drawn.contains(name),
                "the theme step lost the row {name:?} ({set}): {drawn:?}",
            );
        }
        assert!(
            drawn.contains("not your session"),
            "the sample's own label is what stops it reading as a real failure, and it \
             was cut ({set}): {drawn:?}",
        );
        assert!(
            drawn.contains("rule fs.deny, layer workspace"),
            "the sample's refusal lost the half that makes it worth previewing \
             ({set}): {drawn:?}",
        );

        // --- Posture. ---
        wizard.key(key(KeyCode::Enter));
        assert_eq!(wizard.step(), Step::Posture);
        let drawn = draw_step(&mut wizard, &mut screen);
        assert!(
            drawn.contains("How much should it be allowed to do?"),
            "the posture step lost its question ({set}): {drawn:?}",
        );
        for label in ["Sandboxed workspace", "Ask before writes", "Read only"] {
            assert!(
                drawn.contains(label),
                "the posture step lost the row {label:?}, which is what the operator is \
                 choosing between ({set}): {drawn:?}",
            );
        }
        assert!(
            drawn.contains("read, write and run inside this repository"),
            "the chosen posture's explanation lost the words that describe it \
             ({set}): {drawn:?}",
        );

        // --- Confirm: the screen that promises to name the exact path. ---
        wizard.key(key(KeyCode::Enter));
        assert_eq!(wizard.step(), Step::Confirm);
        let drawn = draw_step(&mut wizard, &mut screen);
        let written = drawn
            .lines()
            .find(|line| line.starts_with("This will write "))
            .unwrap_or_else(|| panic!("the path line is not on the screen ({set}): {drawn:?}"));
        assert!(
            written.ends_with("first-run/io.toml"),
            "the end of the path is what identifies the file and it must survive — a \
             confirmation that names a different file has broken the wizard's one \
             promise ({set}): {written:?}",
        );
        assert!(
            written.contains(mark),
            "the path was clipped rather than shortened from the left ({set}): {written:?}",
        );
        let model = drawn
            .lines()
            .find(|line| line.starts_with("  model "))
            .unwrap_or_else(|| panic!("the model line is not on the screen ({set}): {drawn:?}"));
        assert!(
            model.contains("a-self-hosted-vendor"),
            "the confirmation named a model the operator cannot recognise ({set}): {model:?}",
        );
        assert!(
            model.ends_with(mark),
            "the model id was clipped rather than fitted ({set}): {model:?}",
        );
        for fact in [
            "provider",
            "OpenRouter",
            "credential",
            "permission",
            "Sandboxed workspace",
            "theme",
            "Enter to write it, Esc to leave without writing.",
        ] {
            assert!(
                drawn.contains(fact),
                "the confirmation lost {fact:?}, which is one of the facts it exists to \
                 state ({set}): {drawn:?}",
            );
        }

        // `Step::Done` draws nothing at all, so there is no row here for eighty
        // columns to be wrong about — only that the walk arrived.
        wizard.key(key(KeyCode::Enter));
        assert_eq!(wizard.step(), Step::Done);

        // --- The two steps only a compatible endpoint reaches. ---
        let mut wizard = Wizard::new(theme);
        wizard.key(key(KeyCode::Enter));
        for _ in 0..3 {
            wizard.key(key(KeyCode::Down));
        }
        wizard.key(key(KeyCode::Enter));
        assert_eq!(wizard.step(), Step::BaseUrl);
        let drawn = draw_step(&mut wizard, &mut screen);
        assert!(
            drawn.contains("The base URL of the endpoint, for example http://localhost:11434/v1"),
            "the base URL step's example is the whole of its instruction and the end of \
             it was cut ({set}): {drawn:?}",
        );

        wizard.paste("http://localhost:11434/v1");
        wizard.key(key(KeyCode::Enter));
        wizard.paste("sk-not-a-real-key");
        wizard.key(key(KeyCode::Enter));
        wizard.verified();
        // No catalogue and no default to fall back on: the one route to the typed
        // model step.
        wizard.catalogue(Vec::new());
        assert_eq!(wizard.step(), Step::ModelText);
        let drawn = draw_step(&mut wizard, &mut screen);
        assert!(
            drawn.contains("No catalogue to offer. Type the model id this endpoint serves."),
            "the typed-model step lost its question ({set}): {drawn:?}",
        );
    }
}

/// Draw whichever step the wizard is on, and read the real render buffer back.
///
/// The buffer rather than the `Line`s the wizard built, because the whole subject
/// is what survives the last hop: a `Paragraph` in the viewport does not wrap, and
/// what it cannot fit it drops on the floor without telling anybody.
fn draw_step(
    wizard: &mut io_cli::wizard::Wizard,
    screen: &mut io_cli::term::Screen<support::Fixed>,
) -> String {
    screen
        .draw(|frame| wizard.render(frame, frame.area()))
        .expect("frame");
    screen.viewport_text().to_string()
}

/// A `Turn` built by hand.
///
/// `io_harness::Turn` is a plain struct of public fields, so a rendering test can
/// make one without a store — which is the point: what is under test here is the
/// row, not the walk that produced it.
fn turn(id: i64, run_id: i64, prompt: &str) -> io_harness::Turn {
    io_harness::Turn {
        id,
        session_id: 1,
        parent_turn_id: (id > 1).then_some(id - 1),
        run_id,
        prompt: prompt.to_string(),
        reply: Some("done".into()),
        outcome: Some("finished".into()),
        created_at: "2026-08-17T02:31:13.841Z".into(),
    }
}

#[test]
fn n5_the_armed_rewind_line_keeps_both_halves_at_eighty_columns() {
    // The armed line is long — a quoted prompt plus a disclosure — and the
    // instinct after two clipped rows in this product is to assume its tail is at
    // risk. It is not, and the distinction is what this test pins rather than
    // argues: the line is COMMITTED into the terminal's own scrollback, which
    // wraps, and the rows this product has lost halves of before were rows drawn
    // in the viewport, where there is no second line to wrap onto.
    //
    // If a later release ever draws this line in the viewport instead, this test
    // is what fails and says why.
    let (mut screen, recorder) = support::screen(WIDTH, HEIGHT);

    let about = io_cli::rewind::Preview {
        turn_id: 4,
        run_id: 9,
        prompt: "make the retry loop back off instead of hammering the endpoint on \
                 every single failure"
            .into(),
    };
    let line = io_cli::rewind::armed_line(&about, &io_cli::theme::DARK.glyphs);
    assert!(
        line.chars().count() > WIDTH as usize,
        "this test is only meaningful while the line is wider than the terminal",
    );

    screen
        .commit(&[Line::from(line)])
        .expect("commit the armed line");

    let written = recorder.text();
    assert!(
        written.contains("make the retry loop back off"),
        "the quoted prompt never reached the terminal: {written:?}",
    );
    assert!(
        written.contains("BEFORE that turn"),
        "the disclosure is the half that must not be lost, and it was: {written:?}",
    );
    assert!(
        written.contains("is lost"),
        "the consequence has to survive the wrap, not just the warning word: {written:?}",
    );
}

// ---------------------------------------------------------------------------
// F12 — the surfaces 0.7.0 added, at eighty columns, in both glyph sets.
//
// Two gates, and the first is why a length bound is never the only assertion
// below. ratatui clips a viewport row in silence: this product has lost a
// load-bearing half of a row three separate times, and each time the width test
// was green *because* the content was gone. `Screen::viewport_text` right-trims,
// so a clipped row and a fitted row are the same length — what tells them apart
// is that a fitted row carries the whole ellipsis and a clipped one is a
// character short of it. So every surface here is asserted for the fact that had
// to survive, and for `ends_with` the mark of the set it was drawn in.
//
// The second gate is the cursor, and it lives in `tests/cursor.rs` for the
// surfaces that are frames. The plan block and the `!` block are not: both are
// committed into the terminal's own scrollback, where there is no frame for a
// caret to be missing from.
//
// Nothing here uses a double-width character. Every width bound in this crate
// counts `chars()` rather than display cells, which is recorded as a limitation
// rather than asserted around.
// ---------------------------------------------------------------------------

/// A templates directory holding one template whose description io-harness
/// clamps.
///
/// The clamp is `DESCRIPTION_CAP` in `io_harness::template`, two hundred and
/// forty characters against an eighty-column terminal, so this is the longest
/// detail the palette can ever be handed and the row where a fitter that is one
/// cell out is visible.
fn templates_directory() -> tempfile::TempDir {
    let dir = tempfile::tempdir().expect("a temporary directory");
    std::fs::write(
        dir.path().join("triage.md"),
        format!(
            "---\nname: triage\ndescription: {}\n---\nSift the failures.\n",
            "sift the failures and say which one to look at first, ".repeat(8),
        ),
    )
    .expect("a template is written");
    dir
}

/// **F12 — the slash palette at eighty columns.**
///
/// The palette is a `Picker` over two inventories that behave differently at a
/// width: a command's detail is a sentence this repository wrote and can keep
/// short, and a template's is whatever an operator put in a file, arriving
/// already clamped to two hundred and forty characters by io-harness. The second
/// is the row under audit, and the `template: ` marker at the front of it is the
/// fact that has to survive — it is the only thing on screen that says a row is a
/// template rather than a command, and it rides at the front precisely because a
/// detail is fitted from the right.
#[test]
fn f12_the_slash_palette_fits_eighty_columns_in_both_glyph_sets() {
    use io_cli::commands::{self, TEMPLATE};
    use io_cli::picker::Picker;

    let dir = templates_directory();
    let templates = io_harness::Templates::discover(dir.path()).expect("the directory walks");
    let described = templates
        .iter()
        .next()
        .expect("one template was written")
        .description
        .chars()
        .count();
    assert!(
        described > 240,
        "the fixture has to really reach io-harness's own clamp, or the row under \
         audit is an ordinary one: {described} characters",
    );

    for theme in themes() {
        let set = theme.glyphs.name;
        let mark = theme.glyphs.ellipsis;
        let palette = |query: &str| {
            let mut picker = Picker::new("Which command?", commands::palette(&templates, &io_harness::Skills::none()));
            type_at(&mut picker, query);
            picker
        };

        // As the driver opens it: the whole inventory, nothing typed.
        let opened = drawn(&mut palette(""), &theme);
        within_eighty(set, &opened);
        let mut rows = opened.lines();
        assert_eq!(
            rows.next().unwrap_or_default(),
            "Which command?",
            "the title is what a picker with an empty query draws ({set}): {opened:?}",
        );
        let first = rows.next().unwrap_or_default();
        assert!(
            first.starts_with(theme.glyphs.marker),
            "the selection marker is the only thing that says which row Enter takes \
             ({set}): {opened:?}",
        );
        assert!(
            first.contains("help") && first.contains("this table"),
            "a command row carries its name and what it does ({set}): {first:?}",
        );

        // The longest description this release ships, reached by typing its name.
        // The head is the query rather than the title, which is where the query
        // is drawn; the row under it is the match, and the assertion is on the
        // whole of the description — a row that kept its first half would still
        // contain the word the query found it by.
        let expand = drawn(&mut palette("expand"), &theme);
        within_eighty(set, &expand);
        let mut rows = expand.lines();
        assert_eq!(
            rows.next().unwrap_or_default(),
            "expand",
            "the query is drawn in place of the title, not above it ({set}): {expand:?}",
        );
        let row = rows.next().unwrap_or_default();
        assert!(
            row.contains("commit the last step's full detail into the scrollback"),
            "the longest command description in the inventory was cut ({set}): {row:?}",
        );

        // The template row, whose detail is three times the terminal.
        let triage = drawn(&mut palette("triage"), &theme);
        within_eighty(set, &triage);
        let row = triage
            .lines()
            .find(|line| line.contains(TEMPLATE))
            .unwrap_or_else(|| panic!("the template row is not on the screen ({set}): {triage:?}"));
        assert!(
            row.contains("triage"),
            "a template row is labelled with the name `Templates::render` knows it \
             by ({set}): {row:?}",
        );
        assert!(
            row.contains(&format!("{TEMPLATE}sift the failures")),
            "the marker rides at the front of the detail so a narrow terminal cannot \
             be the thing that takes it off ({set}): {row:?}",
        );
        assert!(
            row.ends_with(mark),
            "the clamped description was clipped rather than fitted, so nothing on \
             the row says it continues ({set}): {row:?}",
        );
    }
}

/// **F12 — the picker's query line, and the no-match line under it.**
///
/// The query is drawn *in place of* the title rather than on a line of its own,
/// which is what keeps the row arithmetic the same whether or not anything has
/// been typed — so a query wider than the terminal must fit rather than push or
/// wrap, and the two lines must still be two lines.
///
/// The no-match line is new in 0.7.0 and had never been rendered at any width.
/// It carries the query back inside quotes, so it is longer than the query it
/// reports on and is the more easily clipped of the two.
#[test]
fn f12_the_pickers_query_line_and_its_no_match_line_fit_eighty_columns() {
    use io_cli::picker::Picker;

    // Wider than the terminal, and a subsequence of no label in the inventory, so
    // one set of keystrokes exercises both lines at once.
    const TYPED: &str = "why does the retry loop hammer the endpoint on every single \
                         failure instead of backing off";

    for theme in themes() {
        let set = theme.glyphs.name;
        let mark = theme.glyphs.ellipsis;
        let mut picker = Picker::new(
            "Which command?",
            io_cli::commands::palette(&io_harness::Templates::none(), &io_harness::Skills::none()),
        );
        type_at(&mut picker, TYPED);
        assert_eq!(
            picker.matching(),
            0,
            "the fixture must really admit nothing, or the line under audit is \
             never drawn ({set})",
        );

        let screen = drawn(&mut picker, &theme);
        within_eighty(set, &screen);
        let mut rows = screen.lines();

        let head = rows.next().unwrap_or_default();
        assert!(
            head.starts_with("why does the retry loop"),
            "the beginning of what was typed is what the operator is reading back \
             ({set}): {head:?}",
        );
        assert!(
            head.ends_with(mark),
            "the query was clipped rather than fitted ({set}): {head:?}",
        );

        let note = rows.next().unwrap_or_default();
        assert!(
            note.starts_with("No row matches"),
            "a query that admits nothing has to say so, or the screen is a query \
             over blank rows and reads as a picker that has broken ({set}): {screen:?}",
        );
        assert!(
            note.contains(theme.glyphs.quote_open),
            "the query comes back quoted, so what was typed is distinguishable from \
             the sentence around it ({set}): {note:?}",
        );
        assert!(
            note.ends_with(mark),
            "the no-match line was clipped rather than fitted ({set}): {note:?}",
        );

        assert_eq!(
            screen.lines().filter(|line| !line.is_empty()).count(),
            2,
            "the query is drawn in place of the title and neither line wraps, so a \
             picker with nothing to show is exactly two rows ({set}): {screen:?}",
        );
    }
}

/// A directory deep enough that its title is wider than the terminal.
///
/// Assembled a component at a time rather than joined from one slash-bearing
/// literal, which is a path on unix and something else on Windows. The
/// `/`-separated spelling below is the *listing* argument, which
/// `complete::entries` documents as relative to the root and `/`-separated
/// whatever the platform is.
const DEEP: &str = "crates/some-rather-long-crate-name/src/subsystem/module/implementation";

/// Longer than the seventy-eight columns a marker leaves a picker row.
const LONG_FILE: &str =
    "an-extremely-long-module-file-name-that-nobody-would-ever-type-by-hand-and-would-not-want-to.rs";

fn deep_workspace() -> tempfile::TempDir {
    let dir = tempfile::tempdir().expect("a temporary directory");
    let deep = dir
        .path()
        .join("crates")
        .join("some-rather-long-crate-name")
        .join("src")
        .join("subsystem")
        .join("module")
        .join("implementation");
    std::fs::create_dir_all(deep.join("generated")).expect("the directory tree");
    std::fs::write(deep.join(LONG_FILE), "\n").expect("a file");
    std::fs::write(deep.join("mod.rs"), "\n").expect("a file");
    dir
}

/// The rows the driver builds for one directory: the listing, then the note that
/// says the listing was cut.
///
/// The note is produced through `complete::cut_note` rather than by filling a
/// directory with two hundred files. What is under audit here is the row at a
/// width; that the bound is applied at all is `tests/complete.rs`'s claim, and
/// two hundred files would make this test slow for a fact it does not assert.
fn completion_rows(root: &std::path::Path) -> Vec<io_cli::picker::Row> {
    let (found, _cut) = io_cli::complete::entries(root, &io_harness::Policy::permissive(), DEEP)
        .expect("a listing");
    let mut rows = io_cli::complete::rows(&found);
    assert_eq!(
        rows.len(),
        3,
        "the fixture is a directory of three: {rows:?}"
    );
    rows.push(io_cli::picker::Row::new(
        io_cli::complete::cut_note(true, rows.len()).expect("a cut listing has a note"),
    ));
    rows
}

/// **F12 — the `@` completion picker at eighty columns.**
///
/// Its rows are last components rather than paths, so what is at risk is a file
/// name longer than a row, the trailing separator that is the only thing saying a
/// row is a directory, and the note that keeps a bounded listing from reading as
/// a complete one. The note is the last row of a list taller than the viewport,
/// so it is reached the way an operator reaches it.
#[test]
fn f12_the_completion_picker_fits_eighty_columns_in_both_glyph_sets() {
    use io_cli::picker::Picker;

    let workspace = deep_workspace();
    let rows = completion_rows(workspace.path());

    for theme in themes() {
        let set = theme.glyphs.name;
        let mark = theme.glyphs.ellipsis;
        let mut picker = Picker::new(io_cli::complete::title(DEEP, &theme.glyphs), rows.clone());

        let opened = drawn(&mut picker, &theme);
        within_eighty(set, &opened);

        let head = opened.lines().next().unwrap_or_default();
        assert!(
            head.starts_with("Which path under "),
            "the title says what is being listed ({set}): {opened:?}",
        );
        // The TAIL is what identifies a directory, and the rows are last
        // components, so the title is the only thing on screen telling
        // `app.rs` under `src` from `app.rs` under `tests`. A path too long
        // for the row is shortened from the left, keeping the end.
        assert!(
            head.contains("implementation?"),
            "the last component is what identifies the directory ({set}): {head:?}",
        );
        assert!(
            head.contains(mark),
            "the title was clipped rather than fitted ({set}): {head:?}",
        );

        let directory = opened
            .lines()
            .find(|line| line.contains("generated"))
            .unwrap_or_else(|| panic!("the directory row is not drawn ({set}): {opened:?}"));
        assert!(
            directory.ends_with('/'),
            "the trailing separator is the only thing on the row that says it is a \
             directory, and it is at the end, which is the end that gets cut \
             ({set}): {directory:?}",
        );

        let long = opened
            .lines()
            .find(|line| line.contains("an-extremely-long-module-file-name"))
            .unwrap_or_else(|| panic!("the long file row is not drawn ({set}): {opened:?}"));
        assert!(
            long.ends_with(mark),
            "the file name was clipped rather than fitted ({set}): {long:?}",
        );

        // The cut note sits past the viewport's three row slots, so `End` is what
        // puts it on the screen — and a scrolled list is where a row is most
        // easily drawn somewhere other than where it was measured.
        picker.key(KeyEvent::new(KeyCode::End, KeyModifiers::NONE));
        let scrolled = drawn(&mut picker, &theme);
        within_eighty(set, &scrolled);
        let note = scrolled
            .lines()
            .find(|line| line.contains("type to narrow it"))
            .unwrap_or_else(|| panic!("the cut note is not on the screen ({set}): {scrolled:?}"));
        assert!(
            note.contains("of a larger directory"),
            "the half of the note that says the listing was bounded is the half that \
             stops it reading as `the file is not there` ({set}): {note:?}",
        );
    }
}

/// A finding from this release's audit, and the fix that answered it.
///
/// `complete::title` is documented as load-bearing rather than decorative: the
/// rows are last components, so `app.rs` under `src` and `app.rs` under `tests`
/// are the same three rows of characters and the title is the only thing on
/// screen that tells them apart. `Picker::render` fits its head with
/// `picker::fit`, which keeps the *beginning* — so at eighty columns a directory
/// path longer than sixty-two characters loses exactly the last component that
/// identifies it, and every deep directory's title reads
/// `Which path under crates/some-rather-long-crate-name/src/subs…`.
///
/// The fix is in `complete::title`, which is the only caller that knows its head
/// is a path: it shortens the path itself with `picker::fit_left` before wrapping
/// it in the question, so what survives is the tail. `Picker` cannot do this — it
/// is handed a string and cannot know a path is inside it.
#[test]
fn f12_the_completion_title_keeps_the_directory_it_names() {
    use io_cli::picker::{Picker, Row};

    for theme in themes() {
        let set = theme.glyphs.name;
        let mut picker = Picker::new(
            io_cli::complete::title(DEEP, &theme.glyphs),
            vec![Row::new("mod.rs".to_string())],
        );
        let head = drawn(&mut picker, &theme)
            .lines()
            .next()
            .unwrap_or_default()
            .to_string();
        assert!(
            head.contains("implementation"),
            "the last component is what identifies the directory being listed, and \
             it is what the fit took off ({set}): {head:?}",
        );
    }
}

/// **F12 — the plan committed to scrollback, at eighty columns.**
///
/// `TODO_TEXT_CAP` is two hundred characters and the terminal this product is
/// audited at is eighty columns, so every item on a real plan is fitted. The
/// state word is what must survive that: it is the load-bearing fact — where the
/// agent says the item has got to — and the text is the part that may go. A row
/// whose budget were one cell out would take the state instead, and a row that
/// merely *contained* the word could have taken half of it, so the assertion is
/// that the row **ends** with it.
///
/// No cursor claim: the plan is committed into the terminal's own scrollback and
/// is never a frame, so there is nothing here for ratatui to hide a caret on.
#[test]
fn f12_a_committed_plan_fits_eighty_columns_and_keeps_its_state_word() {
    use io_harness::{EventKind, RunEvent, TodoItem, TodoState};

    // Longer than io-harness's own text cap, which the event is not subject to
    // either — the cap is the store's, and this list is the model's.
    let long = "port the error paths and then everything that reads them ".repeat(5);

    for theme in themes() {
        let set = theme.glyphs.name;
        let mut events = io_cli::events::Events::new(theme);
        let lines = events.event(
            &RunEvent::new(
                1,
                1,
                EventKind::TodoWrote {
                    items: vec![
                        TodoItem::new(long.clone(), TodoState::Done),
                        TodoItem::new("change it", TodoState::Active),
                        TodoItem::new(long.clone(), TodoState::Pending),
                    ],
                },
            ),
            std::time::Duration::ZERO,
        );

        let (mut screen, recorder) = support::screen(WIDTH, HEIGHT);
        screen.commit(&lines).expect("commit the plan");

        let rows = text_of(&lines);
        for row in &rows {
            assert!(
                row.chars().count() <= WIDTH as usize,
                "a plan row overran eighty columns ({set}): {row:?}",
            );
        }
        assert!(
            rows.iter()
                .any(|row| row.contains("1 of 3 done, by the agent's own account")),
            "the header is what says the count is the agent's claim rather than a \
             checked fact ({set}): {rows:?}",
        );

        for state in [TodoState::Done, TodoState::Active, TodoState::Pending] {
            let word = state.as_str();
            assert!(
                rows.iter().any(|row| row.ends_with(word)),
                "no row ends with {word:?}, so the fit took the state rather than \
                 the text it was supposed to shorten ({set}): {rows:?}",
            );
            assert!(
                recorder.contains(word),
                "{word:?} never reached the terminal ({set})",
            );
        }

        let fitted = rows
            .iter()
            .find(|row| row.contains("port the error paths"))
            .unwrap_or_else(|| panic!("the long item never reached the plan ({set}): {rows:?}"));
        assert!(
            fitted.contains(theme.glyphs.ellipsis),
            "a shortened item has to say it was shortened, or it reads as an item \
             the agent wrote that way ({set}): {fitted:?}",
        );
    }
}

/// **F12 — the `!` shell block, at eighty columns.**
///
/// Committed rather than drawn, so a line wider than the terminal wraps and the
/// question is not whether it fits but whether all of it arrived. The head and
/// the tail of the wide line are asserted separately and are placed clear of the
/// wrap columns on purpose: crossterm writes a cursor move where a row wraps, so
/// a needle straddling one is not contiguous in the byte stream.
///
/// Both sets, and the block draws no glyph in either — the leading `!` is the
/// character the operator pressed and the tone words are prose. That is the
/// finding rather than an omission: there is nothing here for a glyph set to
/// change, and this is what says so.
#[test]
fn f12_a_shell_block_reaches_the_terminal_whole_at_eighty_columns() {
    use io_cli::shell::{self, Ran};

    let wide = format!("HEAD{}TAIL", "x".repeat(180));

    for theme in themes() {
        let set = theme.glyphs.name;
        let (mut screen, recorder) = support::screen(WIDTH, HEIGHT);
        let lines = shell::lines(
            "git log --oneline -20",
            &Ran::Output {
                stdout: format!("{wide}\n"),
                stderr: "fatal: refname is ambiguous\n".to_string(),
                status: Some(3),
            },
            &theme,
        );
        screen.commit(&lines).expect("commit the block");

        let written = recorder.text();
        assert!(
            written.contains("! git log --oneline -20"),
            "the echoed command is what says which line produced this block, and it \
             never reached the terminal ({set})",
        );
        assert!(
            written.contains("HEAD"),
            "the beginning of a captured line never reached the terminal ({set})",
        );
        assert!(
            written.contains("TAIL"),
            "the end of a line wider than the terminal was truncated rather than \
             wrapped ({set})",
        );
        assert!(
            written.contains("fatal: refname is ambiguous"),
            "stderr is committed after stdout and must not be the half that goes \
             ({set})",
        );
        // The two halves separately, and that is not squeamishness: a notice is a
        // styled word and an unstyled sentence, so crossterm writes an SGR change
        // between them and the two are not contiguous in the byte stream.
        assert!(
            written.contains("warning:"),
            "the exit status carries its tone as a word, so it reads the same under \
             NO_COLOR and in a screen reader ({set})",
        );
        assert!(
            written.contains("exited 3"),
            "the exit status is the only thing on screen that says the command \
             failed ({set})",
        );
    }
}

/// **F12 — the paste placeholder at eighty columns.**
///
/// The placeholder stands for a block nobody can see, so half of one stands for
/// nothing: the ordinal keeps two pastes apart and the count is what the operator
/// checks against what they copied. Both are on the end of the line, which is the
/// end a narrow terminal takes.
#[test]
fn f12_the_paste_placeholder_is_whole_at_eighty_columns_in_both_glyph_sets() {
    use io_cli::app::App;
    use io_cli::composer::PASTE_THRESHOLD;

    let pasted = "x".repeat(PASTE_THRESHOLD + 1);
    let placeholder = format!("[pasted text #1, {} characters]", pasted.chars().count());

    for theme in themes() {
        let set = theme.glyphs.name;
        let mut app = App::new(theme, "opus-5");
        assert!(
            app.paste(&pasted, false),
            "nothing was open, so the paste had nowhere to go but the composer ({set})",
        );

        let (mut screen, _recorder) = support::screen(WIDTH, HEIGHT);
        screen
            .draw(|frame| app.render(frame, frame.area()))
            .expect("frame");
        let viewport = screen.viewport_text().to_string();
        within_eighty(set, &viewport);

        let row = viewport
            .lines()
            .find(|line| line.contains("pasted text"))
            .unwrap_or_else(|| {
                panic!("the placeholder is not on the screen ({set}): {viewport:?}")
            });
        assert!(
            row.contains(&placeholder),
            "the placeholder was cut, so the line no longer says which paste it is \
             or how large ({set}): {row:?}",
        );
    }
}

/// 0.8.0 F5 — the fleet view fits eighty columns in both glyph sets.
///
/// Two rows are audited together because they are cut by different rules: the
/// tier line is one string that grows with the depth of the tree, and a child row
/// is an identity that must survive with a goal that must not.
#[test]
fn f5_the_fleet_view_fits_eighty_columns_in_both_glyph_sets() {
    use io_cli::fleet::Fleet;
    use io_harness::{EventKind, RunEvent};

    let mut fleet = Fleet::new();
    // Four tiers, which is a tree deeper than anything a default containment
    // allows, so the summary is longer than the line it has to fit in.
    for tier in 0..4 {
        fleet.event(&RunEvent::at_depth(
            1,
            1,
            0,
            EventKind::Fleet {
                tier,
                working: 12,
                queued: 144,
                done: 36,
            },
        ));
    }
    fleet.event(&RunEvent::at_depth(
        1,
        1,
        0,
        EventKind::Spawned {
            child_run_id: 7,
            goal: "port the tokenizer, the error paths, and everything that reads either \
                   of them, one at a time, without changing behaviour"
                .to_string(),
        },
    ));

    for theme in themes() {
        let set = theme.glyphs.name;
        let mark = theme.glyphs.ellipsis;

        let (mut screen, _) = support::screen_of(WIDTH, HEIGHT, 4);
        let mut app = io_cli::app::App::new(theme, "a-model");
        app.fleet = fleet.clone();
        app.toggle_fleet();
        screen
            .draw(|frame| app.render(frame, frame.area()))
            .expect("frame");
        let drawn = screen.viewport_text().to_string();
        within_eighty(set, &drawn);
        assert!(
            drawn.contains("tier 0"),
            "{set}: the first tier is what a cut summary keeps: {drawn:?}",
        );

        let rows = fleet.rows(WIDTH, &theme.glyphs);
        let row = rows.first().expect("one child");
        assert!(
            row.chars().count() <= WIDTH as usize,
            "{set}: {row:?} is wider than the terminal",
        );
        assert!(
            row.contains("run 7") && row.contains("working"),
            "{set}: the identity survives the cut: {row:?}",
        );
        assert!(
            row.ends_with(mark),
            "{set}: the goal is what gets cut, and says so with the set's own \
             mark: {row:?}",
        );
    }
}
