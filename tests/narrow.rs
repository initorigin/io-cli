//! F9 — 80x24 is a supported terminal size, not a degraded one.
//!
//! Every surface this release renders gets an assertion here as it lands. The
//! renderer's own obligations are the two below: the viewport survives content
//! taller than the screen, and nothing it writes is cut mid-character.

mod support;

use ratatui::text::Line;

const WIDTH: u16 = 80;
const HEIGHT: u16 = 24;

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
