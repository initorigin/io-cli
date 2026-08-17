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

/// The text of a rendered cell, one line per line, as a reader would see it.
fn rendered(edit: &Edit) -> String {
    cell(edit, &DARK)
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
        cell(&edit, &DARK).len(),
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
    let lines = cell(edit, &DARK);
    let line = lines
        .iter()
        .find(|line| {
            line.spans
                .iter()
                .map(|span| span.content.as_ref())
                .collect::<String>()
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
        1,
        "an unpaired line is one span",
    );
}
