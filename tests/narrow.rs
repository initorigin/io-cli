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
    let rows = io_cli::sessions::rows(&sessions, WIDTH);
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
    let rows = io_cli::sessions::turn_rows(&turns, WIDTH);
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
    let line = io_cli::rewind::armed_line(&about);
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
