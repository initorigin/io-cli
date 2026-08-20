//! The model answers in markdown, and the transcript renders it.
//!
//! Every case here came off a real answer. A model writes `## Layout`,
//! `**Binary**` and `` `src/main.rs` `` whether or not anything asked it to, and
//! a transcript that commits the text verbatim hands the reader the notation
//! instead of the thing — which is the same defect as printing `prompt_composed`
//! at them, one module over.
//!
//! What is asserted is the *text a reader sees* and the *weight it carries*,
//! separately: the notation is gone from the string, and the emphasis is on the
//! span. A renderer that stripped the asterisks and drew the words flat would
//! pass a string-only test and have thrown the meaning away.

use io_cli::markdown::Markdown;
use io_cli::theme::DARK;
use ratatui::style::Modifier;
use ratatui::text::Line;

/// The line as a reader sees it, notation and all.
fn text(line: &Line<'static>) -> String {
    line.spans
        .iter()
        .map(|span| span.content.as_ref())
        .collect()
}

/// The spans carrying `modifier`, as text.
fn carrying(line: &Line<'static>, modifier: Modifier) -> Vec<String> {
    line.spans
        .iter()
        .filter(|span| span.style.add_modifier.contains(modifier))
        .map(|span| span.content.to_string())
        .collect()
}

fn render(source: &str) -> Vec<Line<'static>> {
    let mut markdown = Markdown::default();
    source
        .lines()
        .map(|line| markdown.line(line, &DARK))
        .collect()
}

#[test]
fn a_heading_loses_its_hashes_and_keeps_its_weight() {
    for source in ["# Layout", "## Layout", "###### Layout"] {
        let line = &render(source)[0];
        assert_eq!(text(line), "Layout", "{source:?}");
        assert_eq!(carrying(line, Modifier::BOLD), vec!["Layout".to_string()]);
    }

    // Six is the last heading level, and a run of hashes with no space after it
    // is not a heading at all — `#!/bin/sh` and `#42` are text.
    for source in ["####### seven", "#nospace", "#"] {
        assert_eq!(text(&render(source)[0]), source, "{source:?}");
    }
}

#[test]
fn bold_and_italic_and_code_are_drawn_rather_than_spelled() {
    let line = &render("A **Binary** `src/main.rs` and _a name_.")[0];
    assert_eq!(text(line), "A Binary src/main.rs and a name.");
    assert_eq!(carrying(line, Modifier::BOLD), vec!["Binary".to_string()]);
    assert_eq!(carrying(line, Modifier::ITALIC), vec!["a name".to_string()]);
    // The code span is a tone rather than a modifier, and it is the only span on
    // the line wearing it.
    let literal = DARK.style(io_cli::theme::Tone::Literal);
    let code: Vec<String> = line
        .spans
        .iter()
        .filter(|span| span.style.fg == literal.fg)
        .map(|span| span.content.to_string())
        .collect();
    assert_eq!(code, vec!["src/main.rs".to_string()]);
}

/// **`**` must win over `*` at the same position.** A `.min()` over the marker
/// positions picks `*` because it sorts first, which left one asterisk on each
/// side of every bold word in a real answer.
#[test]
fn a_double_asterisk_is_not_read_as_two_single_ones() {
    let line = &render("**Structure:** src/")[0];
    assert_eq!(text(line), "Structure: src/");
    assert_eq!(
        carrying(line, Modifier::BOLD),
        vec!["Structure:".to_string()]
    );
}

/// Notation that never closes is not notation.
///
/// This is what keeps the renderer safe on a streaming line: a bold run that has
/// not finished arriving reads as asterisks for one frame rather than eating the
/// rest of the answer, and an underscore inside a name is part of the name.
#[test]
fn unclosed_or_word_internal_notation_is_left_alone() {
    for source in [
        "a **bold run that never closes",
        "the run_id field",
        "2 * 3 * 4",
        "an empty ** pair",
    ] {
        assert_eq!(text(&render(source)[0]), source, "{source:?}");
    }
}

#[test]
fn a_bullet_becomes_the_themes_bullet_at_its_own_depth() {
    for source in ["- one", "* one", "+ one"] {
        assert_eq!(text(&render(source)[0]), "⋅ one", "{source:?}");
    }
    assert_eq!(text(&render("    - nested")[0]), "    ⋅ nested");
}

/// Inside a fence the model's characters are code, not notation.
#[test]
fn a_fenced_block_is_left_exactly_as_the_model_wrote_it() {
    let lines = render("```rust\nlet x = *p; // **not bold**\n```\nafter **bold**");
    // The opening fence draws the language and the closing one draws nothing.
    assert_eq!(text(&lines[0]), "rust");
    assert_eq!(text(&lines[1]), "let x = *p; // **not bold**");
    assert!(carrying(&lines[1], Modifier::BOLD).is_empty());
    assert_eq!(text(&lines[2]), "");
    // And the fence closed, so the line after it is prose again.
    assert_eq!(text(&lines[3]), "after bold");
    assert_eq!(
        carrying(&lines[3], Modifier::BOLD),
        vec!["bold".to_string()]
    );
}

/// A fence left open by a turn does not swallow the next one.
#[test]
fn forgetting_closes_a_fence_nothing_else_did() {
    let mut markdown = Markdown::default();
    markdown.line("```", &DARK);
    markdown.forget();
    let line = markdown.line("**after**", &DARK);
    assert_eq!(text(&line), "after");
}

#[test]
fn a_rule_is_drawn_in_the_glyph_sets_own_character() {
    let line = &render("---")[0];
    assert!(text(line).starts_with("──"), "{:?}", text(line));
    assert!(!text(line).contains('-'), "{:?}", text(line));
}
