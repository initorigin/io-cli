//! F1 — an edit renders as the harness's own stored hunk.
//! F2 — an absent hunk reads as absent, never as an empty diff.
//!
//! The subject of both is `io_cli::diff`, which takes an `io_harness::Edit` and
//! returns lines. It is deliberately a function of a value rather than of a
//! store: the store read belongs to the driver, which is the only thing that
//! holds a `Store`, and keeping it out of here is what lets these tests state a
//! hunk by hand instead of standing up a database to hold one.

mod support;

use io_cli::diff::cell;
use io_cli::theme::DARK;
use io_harness::Edit;

/// A terminal wide enough that word-level emphasis applies. Every test that is
/// not about the narrow form states it, so none of them depends on the floor by
/// accident.
const WIDE: u16 = 120;

/// The text of a rendered cell, one line per line, as a reader would see it.
fn rendered(edit: &Edit) -> String {
    cell(edit, &DARK, WIDE)
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

/// A hunk as io-harness renders one: a body, no `---`/`+++` headers, and `@@`
/// line numbers that are the *file's* rather than the hunk's own.
///
/// The numbers here are the load-bearing part of F1. They say the change is at
/// line 12 of a file whose contents this test never supplies, which is precisely
/// the fact a diff computed inside io-cli from two strings could not produce.
const HUNK: &str = "\
@@ -12,5 +12,5 @@
 fn one() {}

-    Tone::Muted => Style::default().fg(GREY),
+    Tone::Muted => Style::default().fg(self.muted),

 fn two() {}
";

fn edit_with_hunk() -> Edit {
    Edit {
        step: 3,
        tool: "edit_file".to_string(),
        path: "src/theme.rs".to_string(),
        lines_added: 1,
        lines_removed: 1,
        hunk: Some(HUNK.to_string()),
    }
}

#[test]
fn f1_the_hunk_is_the_harness_s_own_text() {
    let text = rendered(&edit_with_hunk());

    // Every body line of the stored hunk survives, in order, with its marker.
    for line in HUNK.lines() {
        assert!(
            text.contains(line),
            "the rendered cell dropped {line:?}\n\n{text}",
        );
    }
}

#[test]
fn f1_the_line_numbers_are_the_file_s_and_are_not_recomputed() {
    let text = rendered(&edit_with_hunk());

    // The whole of F1. A diff io-cli computed from the two texts it was given
    // would number this hunk from 1, because it has never seen the eleven lines
    // above it. The header is passed through, character for character.
    assert!(
        text.contains("@@ -12,5 +12,5 @@"),
        "the file's own line numbers are not in the cell:\n\n{text}",
    );
    assert!(
        !text.contains("@@ -1,5 +1,5 @@"),
        "the hunk was renumbered, which means it was recomputed:\n\n{text}",
    );
}

#[test]
fn f1_the_path_comes_before_the_counts() {
    let text = rendered(&edit_with_hunk());
    let first = text.lines().next().expect("a header line");

    let path = first
        .find("src/theme.rs")
        .expect("the path is on the header");
    let counts = first.find("+1").expect("the counts are on the header");
    // Content before metadata, asserted by position rather than by presence —
    // 0.1.1 paid for the lesson that a `contains` assertion is just as green
    // when the sentence is inside out.
    assert!(
        path < counts,
        "the counts came before the path, which is metadata before content: {first:?}",
    );
}

#[test]
fn f2_an_absent_hunk_says_so_and_keeps_its_counts() {
    // `Edit.hunk` is `None` for three reasons and not one of them is "nothing
    // changed": the row predates the harness release that added hunks, the
    // file's previous contents were not kept, or the rendered diff would have
    // exceeded the store's snapshot cap. The counts are still there in every one
    // of those cases, and they are what says the file did change.
    let edit = Edit {
        step: 2,
        tool: "write_file".to_string(),
        path: "src/big.rs".to_string(),
        lines_added: 812,
        lines_removed: 3,
        hunk: None,
    };

    let text = rendered(&edit);

    assert!(text.contains("src/big.rs"), "{text}");
    assert!(text.contains("+812"), "the additions are lost:\n\n{text}");
    assert!(text.contains("−3") || text.contains("-3"), "{text}");
    assert!(
        text.contains("no diff stored"),
        "an absent hunk has to say it is absent:\n\n{text}",
    );
}

#[test]
fn f2_an_absent_hunk_never_renders_as_an_empty_diff() {
    let edit = Edit {
        step: 2,
        tool: "write_file".to_string(),
        path: "src/big.rs".to_string(),
        lines_added: 812,
        lines_removed: 3,
        hunk: None,
    };

    let text = rendered(&edit);

    // The version that would ship by accident: `None` treated as an empty patch,
    // which draws a cell that says the file was untouched while the counts
    // beside it say it was not.
    assert!(
        !text.contains("@@"),
        "an absent hunk drew a diff body:\n\n{text}",
    );
    // Counted over the rendered lines and NOT over `str::lines()` of the joined
    // text. That is not a stylistic preference: `"header\n".lines()` yields one
    // element, so a text-level count cannot see the trailing blank line an
    // `unwrap_or_default()` leaves behind — which is precisely the empty-patch
    // shape this assertion exists to catch. The sabotage arm found it.
    assert_eq!(
        cell(&edit, &DARK, WIDE).len(),
        1,
        "an absent hunk is one line and nothing else:\n\n{text}",
    );
}

#[test]
fn f1_only_this_step_s_edits_are_drawn_at_this_step() {
    // `Store::edits` answers for the whole run. A caller that draws what it
    // returns re-renders every earlier edit at every later step, so the same
    // diff appears once, then twice, then three times — a transcript that grows
    // quadratically in the length of the turn.
    let edits = vec![
        Edit {
            step: 2,
            path: "first.rs".to_string(),
            ..Default::default()
        },
        Edit {
            step: 3,
            path: "second.rs".to_string(),
            ..Default::default()
        },
        Edit {
            step: 3,
            path: "third.rs".to_string(),
            ..Default::default()
        },
    ];

    let mine = io_cli::diff::for_step(edits, 3);

    let paths: Vec<&str> = mine.iter().map(|edit| edit.path.as_str()).collect();
    assert_eq!(paths, ["second.rs", "third.rs"]);
}

/// The read this release adds, against a real store rather than against values.
///
/// It is here because this is the first time io-cli reads the durable trace at
/// all: everything before 0.3.0 rendered events as they streamed past. The
/// round trip is what proves the hunk survives being written and read back,
/// which is the whole premise of F1 — that the diff on screen is the harness's
/// text and not io-cli's.
#[test]
fn f1_a_hunk_survives_the_store_round_trip() {
    let directory = tempfile::tempdir().expect("a temporary directory");
    let store = io_harness::Store::open(directory.path().join("io.db")).expect("a store");
    let run_id = 1;

    store
        .record_edit(run_id, &edit_with_hunk())
        .expect("the edit is recorded");

    let read = store.edits(run_id).expect("the edits are read back");
    let mine = io_cli::diff::for_step(read, 3);
    assert_eq!(mine.len(), 1, "one edit was recorded at step 3");
    assert_eq!(
        mine[0].hunk.as_deref(),
        Some(HUNK),
        "the hunk came back changed",
    );

    let text = rendered(&mine[0]);
    assert!(text.contains("@@ -12,5 +12,5 @@"), "{text}");
}

#[test]
fn f2_a_hunk_that_is_present_is_not_confused_for_an_absent_one() {
    // The other direction, so the F2 sabotage arm cannot pass by rendering the
    // absence sentence on everything.
    let text = rendered(&edit_with_hunk());
    assert!(!text.contains("no diff stored"), "{text}");
}

// ---------------------------------------------------------------------------
// F3 — word-level emphasis inside a changed line.
// ---------------------------------------------------------------------------

/// The spans of the one line in a rendered cell that starts with `marker`,
/// as (text, emphasised) pairs.
///
/// Emphasis is read off the style rather than off the text, because the whole
/// point of F3 is *which part of the line* is marked — a test that looked at the
/// text alone could not tell a line emphasised in one piece from a line
/// emphasised in three.
fn spans_of(edit: &Edit, marker: char) -> Vec<(String, bool)> {
    let lines = cell(edit, &DARK, WIDE);
    let line = lines
        .iter()
        .find(|line| {
            line.spans
                .iter()
                .map(|span| span.content.as_ref())
                .collect::<String>()
                // Past the line-number gutter, which every body row carries
                // since 0.11.0: a diff a reader cannot go to is a change they
                // can see and not find.
                .trim_start()
                .trim_start_matches(|c: char| c.is_ascii_digit())
                .trim_start()
                .starts_with(marker)
        })
        .unwrap_or_else(|| panic!("no line starting with {marker:?}"));
    line.spans
        .iter()
        .map(|span| {
            (
                span.content.to_string(),
                span.style
                    .add_modifier
                    .contains(ratatui::style::Modifier::BOLD),
            )
        })
        .collect()
}

#[test]
fn f3_only_the_changed_words_are_emphasised() {
    let removed = spans_of(&edit_with_hunk(), '-');
    let added = spans_of(&edit_with_hunk(), '+');

    let emphasised: Vec<&str> = removed
        .iter()
        .filter(|(_, on)| *on)
        .map(|(text, _)| text.as_str())
        .collect();
    assert_eq!(
        emphasised,
        ["GREY"],
        "the removed side should emphasise exactly what went: {removed:?}",
    );

    let emphasised: Vec<&str> = added
        .iter()
        .filter(|(_, on)| *on)
        .map(|(text, _)| text.as_str())
        .collect();
    assert_eq!(
        emphasised,
        ["self.muted"],
        "the added side should emphasise exactly what arrived: {added:?}",
    );
}

#[test]
fn f3_the_emphasis_sits_between_the_common_head_and_the_common_tail() {
    let removed = spans_of(&edit_with_hunk(), '-');

    // Asserted by POSITION: head, then the change, then tail. A membership
    // assertion is just as green when the line is inside out, which 0.1.1 paid
    // for and 0.2.0 paid for again.
    let at = removed
        .iter()
        .position(|(_, on)| *on)
        .expect("something is emphasised");
    assert!(
        at > 0,
        "there is a common head before the change: {removed:?}"
    );
    assert!(
        at + 1 < removed.len(),
        "there is a common tail after the change: {removed:?}",
    );
    assert!(
        removed[at - 1].0.ends_with(".fg("),
        "the head stops where the lines stop agreeing: {removed:?}",
    );
    assert_eq!(
        removed[at + 1].0,
        "),",
        "the tail starts where they agree again: {removed:?}",
    );
}

#[test]
fn f3_an_unpaired_line_is_emphasised_at_the_line_and_not_within_it() {
    // Two lines out and one in. There is no honest pairing here — the harness
    // renders a `write_file` that rewrote two distant regions as ONE hunk
    // spanning both, so a rule that paired by position would emphasise the
    // difference between lines that have nothing to do with each other.
    let edit = Edit {
        step: 1,
        tool: "write_file".to_string(),
        path: "a.rs".to_string(),
        lines_added: 1,
        lines_removed: 2,
        hunk: Some(
            "@@ -1,3 +1,2 @@\n-let a = 1;\n-let b = 2;\n+let everything = 0;\n ok\n".to_string(),
        ),
    };

    for (text, emphasised) in spans_of(&edit, '-') {
        assert!(
            !emphasised,
            "an unpaired removal was emphasised within the line: {text:?}",
        );
    }
    assert_eq!(
        spans_of(&edit, '-').len(),
        2,
        "an unpaired line is the gutter and one span of text",
    );
}

// ---------------------------------------------------------------------------
// F4 — highlighting is drawn in io-cli's own tokens and disappears under
// NO_COLOR. N2 — the dependency set grows by exactly one name, with its
// features pinned. N4 — the syntax set is never loaded on the startup path.
// ---------------------------------------------------------------------------

/// A hunk of Rust with a keyword, a string and a number in it, so each of the
/// three syntax tokens has something to colour.
fn edit_with_code() -> Edit {
    Edit {
        step: 1,
        tool: "edit_file".to_string(),
        path: "src/lib.rs".to_string(),
        lines_added: 1,
        lines_removed: 1,
        hunk: Some(
            "@@ -1,3 +1,3 @@\n \
             // a comment\n\
             -let name = \"old\";\n\
             +let name = \"new\";\n \
             const N: u32 = 42;\n"
                .to_string(),
        ),
    }
}

#[test]
fn f4_the_colours_come_from_io_cli_s_own_tokens() {
    let edit = edit_with_code();
    let coloured: Vec<(String, Option<ratatui::style::Color>)> = cell(&edit, &DARK, WIDE)
        .iter()
        .flat_map(|line| {
            line.spans
                .iter()
                .map(|span| (span.content.to_string(), span.style.fg))
                .collect::<Vec<_>>()
        })
        .collect();
    let used: Vec<ratatui::style::Color> = coloured.iter().filter_map(|(_, fg)| *fg).collect();

    // Every colour on a context line is one this theme declares. A colour from
    // syntect's own theme set would be an RGB value that appears nowhere in
    // `theme.rs` — which is the whole reason `default-themes` is off.
    let mine = [
        DARK.foreground,
        DARK.muted,
        DARK.accent,
        DARK.syntax_keyword,
        DARK.syntax_string,
        DARK.syntax_literal,
        DARK.diff_add,
        DARK.diff_delete,
    ];
    for colour in &used {
        assert!(
            mine.contains(colour),
            "{colour:?} is not one of io-cli's tokens: {coloured:?}",
        );
    }
    assert!(
        !used.is_empty(),
        "nothing was coloured at all, so this asserts nothing: {coloured:?}",
    );
}

#[test]
fn f4_a_keyword_a_string_and_a_number_each_get_their_own_token() {
    let edit = edit_with_code();
    let all: Vec<(String, Option<ratatui::style::Color>)> = cell(&edit, &DARK, WIDE)
        .iter()
        .flat_map(|line| {
            line.spans
                .iter()
                .map(|span| (span.content.to_string(), span.style.fg))
                .collect::<Vec<_>>()
        })
        .collect();

    let has = |text: &str, colour: ratatui::style::Color| {
        all.iter()
            .any(|(content, fg)| content.contains(text) && *fg == Some(colour))
    };

    // `let` and `const` are `storage.*` in Sublime's scope vocabulary rather
    // than `keyword.*`. Asserting on them rather than on an operator is what
    // caught the missing `storage` entry in the scope table.
    assert!(has("let", DARK.syntax_keyword), "let is a keyword: {all:?}");
    assert!(
        has("const", DARK.syntax_keyword),
        "const is a keyword: {all:?}"
    );
    assert!(
        has("old", DARK.diff_delete),
        "the changed word keeps the diff colour: {all:?}"
    );
    assert!(has("42", DARK.syntax_literal), "literal: {all:?}");
    assert!(
        has("a comment", DARK.muted),
        "comment reads as muted: {all:?}"
    );
}

#[test]
fn f4_no_color_leaves_the_markers_carrying_the_meaning() {
    use io_cli::theme::MONO;

    let lines = cell(&edit_with_code(), &MONO, WIDE);
    for line in &lines {
        for span in &line.spans {
            assert_eq!(
                span.style.fg, None,
                "NO_COLOR emitted a colour: {:?}",
                span.content,
            );
            assert!(
                span.style.add_modifier.is_empty(),
                "NO_COLOR emitted a modifier, which is still a presentation-only \
                 carrier: {:?}",
                span.content,
            );
        }
    }

    // And the meaning survives, because the markers are text.
    let text: String = lines
        .iter()
        .map(|line| {
            line.spans
                .iter()
                .map(|span| span.content.as_ref())
                .collect::<String>()
        })
        .collect::<Vec<_>>()
        .join("\n");
    assert!(text.contains("-let name = \"old\";"), "{text}");
    assert!(text.contains("+let name = \"new\";"), "{text}");
}

#[test]
fn f4_a_file_with_no_grammar_still_renders() {
    // `None` from `find_syntax_by_extension` is not a failure — it is what a
    // file syntect has never heard of should look like.
    let edit = Edit {
        step: 1,
        tool: "write_file".to_string(),
        path: "notes.zzz".to_string(),
        lines_added: 1,
        lines_removed: 0,
        hunk: Some("@@ -0,0 +1 @@\n+hello\n".to_string()),
    };
    let text = rendered(&edit);
    assert!(text.contains("+hello"), "{text}");
}

// ---------------------------------------------------------------------------
// F6 — `minimal` shows changed lines only, and comes from the harness's own
// configuration.
// ---------------------------------------------------------------------------

#[test]
fn f6_minimal_keeps_the_changed_lines_and_the_header_and_drops_the_context() {
    use io_cli::diff::cell_styled;
    use io_cli::settings::DiffStyle;

    let edit = edit_with_hunk();
    let text = |style| {
        cell_styled(&edit, &DARK, WIDE, style)
            .iter()
            .map(|line| {
                line.spans
                    .iter()
                    .map(|span| span.content.as_ref())
                    .collect::<String>()
            })
            .collect::<Vec<_>>()
            .join("\n")
    };

    let unified = text(DiffStyle::Unified);
    let minimal = text(DiffStyle::Minimal);

    // What changed survives both.
    for style in [&unified, &minimal] {
        assert!(
            style.contains("-    Tone::Muted => Style::default().fg(GREY),"),
            "{style}"
        );
        assert!(
            style.contains("+    Tone::Muted => Style::default().fg(self.muted),"),
            "{style}"
        );
    }

    // The context is what `minimal` drops, and only that.
    assert!(unified.contains("fn one() {}"), "{unified}");
    assert!(
        !minimal.contains("fn one() {}"),
        "minimal kept a context line:\n{minimal}"
    );
    assert!(!minimal.contains("fn two() {}"), "{minimal}");

    // The `@@` header stays. A change with no line numbers is a change that does
    // not say where in the file it is, which is not a smaller diff — it is a
    // worse one.
    assert!(
        minimal.contains("@@ -12,5 +12,5 @@"),
        "minimal dropped the line numbers:\n{minimal}",
    );
    assert!(minimal.contains("src/theme.rs"), "{minimal}");
}

#[test]
fn f6_the_style_is_read_from_the_setting_and_an_unknown_value_is_unified() {
    use io_cli::settings::DiffStyle;

    assert_eq!(DiffStyle::from_setting(Some("minimal")), DiffStyle::Minimal);
    assert_eq!(DiffStyle::from_setting(Some("unified")), DiffStyle::Unified);
    // Absent means unified, which is what every file written before 0.3.0 means.
    assert_eq!(DiffStyle::from_setting(None), DiffStyle::Unified);
    // `[app.io-cli]` is the one section io-harness deliberately does not
    // validate, so a typo in a cosmetic key must not stop a session starting.
    assert_eq!(DiffStyle::from_setting(Some("minmal")), DiffStyle::Unified);
    assert_eq!(DiffStyle::default(), DiffStyle::Unified);
}
