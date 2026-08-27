//! The shape of a committed page, and the folding four surfaces now share.
//!
//! **This module is a de-duplication, which is the most dangerous kind of change
//! to test.** `status::committed` and `context::committed` each carried their own
//! copy of the folding until 0.22.0 — twenty lines under two names, `folded` and
//! `wrapped`, differing only in that one took its indents as arguments and the
//! other hard-coded the same two numbers. Both had shipped and both were correct.
//! A shared replacement that is merely *plausible* passes every test those two
//! surfaces already have, because none of them asserts a fold at a width where
//! the two implementations could disagree.
//!
//! So the last test here is not a property test at all. It carries a verbatim
//! copy of each of the two functions as 0.21.0 shipped them and runs the new one
//! against both, over inputs chosen to reach the arm that splits a word — which
//! is the arm a rewrite gets wrong and the arm no existing test reaches. The
//! copies are dead code that exists to disagree.

use io_cli::glyphs::ASCII;
use io_cli::page::{self, Row};
use io_cli::theme::{Theme, Tone, DARK};
use ratatui::text::Line;

/// The theme the sweeps draw in: the ordinary palette, ASCII glyph set, so a
/// failure message is readable in a terminal that cannot draw the Unicode rule.
fn ascii() -> Theme {
    DARK.with_glyphs(ASCII)
}

/// One rendered line as a reader sees it.
fn row(line: &Line<'_>) -> String {
    line.spans.iter().map(|s| s.content.as_ref()).collect()
}

fn drawn(lines: &[Line<'_>]) -> Vec<String> {
    lines.iter().map(row).collect()
}

/// The texts every sweep runs over.
///
/// Chosen for the arms they reach rather than for realism, though every one of
/// them is a string this product actually draws: a workspace path with no spaces
/// in it, a policy layer's act list, a namespaced model id, a whole sentence, and
/// a single word longer than eighty columns — which is the one that separates
/// folding from fitting.
fn texts() -> Vec<String> {
    vec![
        String::new(),
        "workspace".to_string(),
        "policy: ops-baseline allow read src/* allow write out/* deny read .env".to_string(),
        "model: anthropic/claude-sonnet-4.5".to_string(),
        "/Users/somebody/Documents/work/monorepo/services/billing/src/domain/invoice.rs"
            .to_string(),
        "a".repeat(200),
        format!("prefix {} suffix", "x".repeat(120)),
        "the total above is a floor and not a total, because four calls used a model \
         with no rate in the price table"
            .to_string(),
        // Multi-byte, because `folded` slices a word by byte index after counting
        // its characters, and the two are the same number only in ASCII.
        "café ☕ ünïcödé ✱ ".repeat(12),
    ]
}

// ---------------------------------------------------------------------------
// `folded`
// ---------------------------------------------------------------------------

/// **No row is ever wider than the width it was given, at any width.**
///
/// The claim the module is named for. A committed page lands in the terminal's
/// own scrollback beside every earlier turn, so a row that runs past the edge is
/// not truncated — it is wrapped by the terminal at a column nothing here chose,
/// which puts the break in the middle of a path and the continuation hard against
/// the left margin where it reads as a new fact.
///
/// The companion claim is that nothing is lost. Folding is not fitting: the whole
/// argument for it is written on the function, and it is that a committed surface
/// owns as many rows as it needs, so there is no reason left to lose a character —
/// and the characters most likely to be lost are the tail of a path, the tail of
/// an act list and the tail of a model id, which is to say the answer. Compared on
/// the non-whitespace characters, because folding is allowed to move whitespace
/// around and is not allowed to drop anything else.
///
/// Sabotage: replace the split arm with a truncation, or use `picker::fit` here —
/// under which every width still holds and the character count fails on the two
/// long-word fixtures.
#[test]
fn folding_never_exceeds_the_width_and_never_loses_a_character() {
    for text in texts() {
        // From one column, which is narrower than either indent and is the case
        // the `.max(1)` in the loop exists for.
        for width in 1..=120usize {
            for (first, rest) in [(0usize, 0usize), (2, 4), (0, 2), (4, 2), (8, 8)] {
                let rows = page::folded(&text, width, first, rest);
                assert!(!rows.is_empty(), "folding produced no rows at all");

                // The general bound. A row is its indent plus its content, and
                // content is at least one character however narrow the terminal
                // is — the `.max(1)` that stops a width under the indent from
                // looping forever. So `indent + 1` is the floor of what a row can
                // be, and below it the terminal is narrower than a row can be
                // drawn at all.
                let bound = width.max(first.max(rest) + 1);
                for line in &rows {
                    assert!(
                        line.chars().count() <= bound,
                        "a row of {} characters was folded to {width}: {line:?}",
                        line.chars().count(),
                    );
                }
                // At every width this product supports, the indent fits and the
                // bound is the width itself.
                if width > first.max(rest) {
                    for line in &rows {
                        assert!(
                            line.chars().count() <= width,
                            "a row overflowed {width} columns: {line:?}",
                        );
                    }
                }

                let before: String = text.chars().filter(|c| !c.is_whitespace()).collect();
                let after: String = rows
                    .concat()
                    .chars()
                    .filter(|c| !c.is_whitespace())
                    .collect();
                assert_eq!(
                    before, after,
                    "folding lost or invented a character at {width} columns, indents {first}/{rest}",
                );
            }
        }
    }
}

/// **A word longer than the room it has is split, and a word that merely does not
/// fit *this* row is not.**
///
/// Two rules that look like one. A word is moved to a fresh row first and only
/// split when a row of its own cannot hold it — so an ordinary sentence never has
/// a word broken across rows, and a two-hundred-character path is broken rather
/// than allowed to run past the edge. A folder that split eagerly would hyphenate
/// the middle of every long word in every sentence; one that never split would put
/// the path past the margin.
///
/// Sabotage: delete the `if used > 0 { … continue; }` arm, which is the retry on a
/// fresh row — under which the sentence fixture starts splitting words at the end
/// of every row and this test fails while the width property above still holds.
#[test]
fn a_word_longer_than_its_room_is_split_and_one_that_merely_does_not_fit_is_not() {
    // Eighty columns, two-space indent: seventy-eight characters of room. A word
    // of a hundred and twenty cannot be held by any row and has to be cut.
    let long = "x".repeat(120);
    let rows = page::folded(&format!("prefix {long} suffix"), 80, 2, 4);
    assert!(
        rows.len() >= 3,
        "a hundred-and-twenty-character word fitted somewhere it could not: {rows:?}",
    );
    // Cut at exactly the room a continuation row has — eighty columns less the
    // four-space hanging indent — rather than at the room left on the row it
    // could not fit, which is what "retried whole on the fresh row" means.
    assert!(
        rows.iter().any(|line| line.trim() == "x".repeat(76)),
        "the word was cut somewhere other than the full width of a fresh row: {rows:?}",
    );

    // The same width, and a sentence of ordinary words. Every row's content is
    // whole words, so splitting `text` on whitespace and re-splitting each row
    // yields the same sequence of words.
    let sentence = "the total above is a floor and not a total, because four calls used a \
                    model with no rate in the price table";
    let rows = page::folded(sentence, 80, 2, 4);
    assert!(rows.len() > 1, "the fixture has to fold to say anything");
    let words: Vec<&str> = sentence.split_whitespace().collect();
    let folded_words: Vec<&str> = rows
        .iter()
        .flat_map(|line| line.split_whitespace())
        .collect();
    assert_eq!(
        words, folded_words,
        "a word was broken across two rows when a fresh row would have held it: {rows:?}",
    );
}

/// **The hanging indent starts on the second row, which is what says a row is a
/// continuation without aligning anything into a column.**
///
/// The page is deliberately not a table — the argument is written out at
/// `status::committed` and it is about eighty columns rather than about taste: a
/// column width is decided by the widest cell, and the widest cell on these pages
/// is a model id. So a continuation cannot be marked by aligning under a column
/// that does not exist, and it is marked by being indented further than the row
/// it continues.
///
/// Sabotage: set `indent = rest` before the first row rather than after it, and
/// every fact on every page gains two spaces while this is the only test that
/// notices — the width property still holds and the characters are all still
/// there.
#[test]
fn the_hanging_indent_marks_a_continuation_from_the_second_row() {
    let rows = page::folded(
        "policy: ops-baseline allow read src/* allow write out/* deny read .env deny net \
         ads.example.com",
        40,
        2,
        4,
    );
    assert!(rows.len() > 2, "the fixture has to fold more than once");

    assert!(
        rows[0].starts_with("  ") && !rows[0].starts_with("   "),
        "the first row does not carry the first indent: {:?}",
        rows[0],
    );
    for line in &rows[1..] {
        assert!(
            line.starts_with("    ") && !line.starts_with("     "),
            "a continuation row does not carry the hanging indent: {line:?}",
        );
    }

    // A heading folds at zero and continues at two, which is what makes it
    // distinguishable from a fact by POSITION as well as by colour — the whole of
    // what a reader under `NO_COLOR` or `--plain` has to go on.
    let heading = page::folded("slowest calls, of the last two hundred runs", 20, 0, 2);
    assert!(heading.len() > 1);
    assert!(
        !heading[0].starts_with(' '),
        "a heading is indented to zero"
    );
    assert!(heading[1].starts_with("  "));
}

/// **An empty text is one indented row, not no rows at all.**
///
/// A caller folding an empty value gets a row it can put on the page, which is
/// what keeps `Row::Fact("label", "")` from silently disappearing. The tail
/// condition is `used > 0 || rows.is_empty()`, and the second half of it is the
/// whole of this behaviour.
///
/// Sabotage: drop `|| rows.is_empty()`, under which an empty string folds to
/// nothing and a fact with an empty value vanishes from the page rather than
/// reading as empty.
#[test]
fn an_empty_text_folds_to_one_row_rather_than_to_none() {
    assert_eq!(page::folded("", 80, 2, 4), vec!["  ".to_string()]);
    assert_eq!(page::folded("   ", 80, 0, 0), vec![String::new()]);
}

// ---------------------------------------------------------------------------
// `commit`
// ---------------------------------------------------------------------------

/// **A committed page has edges, and both of them name it.**
///
/// It lands in the terminal's own scrollback beside every earlier turn and every
/// line of transcript, so a passage with no edges is one a reader cannot tell the
/// extent of — where the page started, and whether what follows is still part of
/// it. The closing rule carries `ends` for the same reason `transcript` does: two
/// identical rules would be two openings.
///
/// Sabotage: drop the closing line, under which a page runs into whatever the
/// next turn writes and nothing fails anywhere else.
#[test]
fn a_page_is_edged_at_both_ends_with_the_title_on_each() {
    let theme = ascii();
    let rule = theme.glyphs.rule;
    let lines = page::commit(
        "cost",
        &[Row::fact("calls", "12"), Row::note("nothing recorded")],
        &theme,
        80,
    );
    let rows = drawn(&lines);

    assert_eq!(rows[0], format!("{rule}{rule}{rule} cost"));
    assert_eq!(
        rows[rows.len() - 1],
        format!("{rule}{rule}{rule} cost ends"),
        "the closing rule does not say the page ended, so it reads as a second opening",
    );
    assert_eq!(
        rows.len(),
        4,
        "two rows between two rules, and nothing else: {rows:?}",
    );
}

/// **Every row kind reaches the page, and the two that qualify a figure do not
/// read the same as the two that state one.**
///
/// `Row::Note` and `Row::caveat` are the same variant with different tones, and
/// the difference is not decoration: io-harness's own pricing documentation calls
/// a renderer that draws a floor and a total identically "lying by omission". A
/// heading is separated by position *and* tone for the same reason, since a reader
/// under `NO_COLOR` has only the position.
///
/// Sabotage: make `Row::caveat` call `Row::note`, under which a total that is a
/// floor reads exactly like one that is not, and no test that reads text alone
/// can tell.
#[test]
fn every_row_kind_draws_and_a_caveat_does_not_read_like_a_note() {
    let theme = ascii();
    let lines = page::commit(
        "stats",
        &[
            Row::heading("runs by outcome"),
            Row::fact("success", "12"),
            Row::Blank,
            Row::note("nothing recorded"),
            Row::caveat("the cost above is a floor and not a total"),
        ],
        &theme,
        80,
    );
    let rows = drawn(&lines);

    assert_eq!(rows[1], "runs by outcome", "a heading is indented to zero");
    assert_eq!(rows[2], "  success: 12");
    assert_eq!(rows[3], "", "a blank row is blank");
    assert_eq!(rows[4], "  nothing recorded");
    assert_eq!(rows[5], "  the cost above is a floor and not a total");

    // The tones. Read off the theme rather than compared to a colour typed here,
    // so a palette change moves both sides together and only a collapse of the
    // two tones fails.
    let style_of = |index: usize| lines[index].spans[0].style;
    assert_eq!(style_of(1), theme.style(Tone::Accent), "the heading");
    assert_eq!(style_of(2), theme.style(Tone::Normal), "the fact");
    assert_eq!(style_of(4), theme.style(Tone::Muted), "the note");
    assert_eq!(style_of(5), theme.style(Tone::Warning), "the caveat");
    assert_ne!(
        style_of(4),
        style_of(5),
        "a qualification and an ordinary sentence draw identically, so a floor \
         reads as a total",
    );
}

/// **A page folds its rows rather than letting one run past the edge, at every
/// width the product supports.**
///
/// The integration of the two halves: `commit` hands each row to `folded` with
/// the indents that row kind takes, and a page whose facts were merely `format!`ed
/// straight into `Line`s would pass every content assertion above and overflow the
/// terminal on the one row that matters — which on `/cost` and `/stats` is the row
/// carrying a namespaced model id.
///
/// Sabotage: push `Line::from(format!("  {label}: {value}"))` in the `Fact` arm
/// instead of folding it, and this is the only test that fails.
#[test]
fn no_row_of_a_committed_page_runs_past_the_terminal() {
    let theme = ascii();
    let rows = [
        Row::heading("slowest calls, of the last 200 runs"),
        Row::fact(
            "anthropic/claude-sonnet-4.5-with-an-absurdly-long-deployment-suffix",
            "4213 ms",
        ),
        Row::fact(
            "workspace",
            "/Users/somebody/Documents/work/monorepo/services/billing",
        ),
        Row::caveat(
            "four calls used a model with no rate in the price table, so the cost above \
             is a floor and not a total",
        ),
        Row::Blank,
    ];

    for width in 20..=120u16 {
        let lines = page::commit("cost", &rows, &theme, width);
        // The first and last rows are the rules. They carry the title rather than
        // content and are deliberately not folded — a rule broken across two rows
        // would stop being an edge — so the claim is about everything between them.
        for line in &lines[1..lines.len() - 1] {
            let drawn: usize = line
                .spans
                .iter()
                .map(|span| span.content.chars().count())
                .sum();
            assert!(
                drawn <= width as usize,
                "a page row overflowed {width} columns: {line:?}",
            );
        }
    }
}

// ---------------------------------------------------------------------------
// The de-duplication itself
// ---------------------------------------------------------------------------

/// `status::folded` exactly as 0.21.0 shipped it, before the four surfaces shared
/// one.
///
/// Verbatim, comments and all. It is here to disagree, so it is not tidied, not
/// renamed and not improved — a copy that had been cleaned up would be a third
/// implementation and would prove nothing about the second.
fn folded_0_21_0(text: &str, width: usize, first: usize, rest: usize) -> Vec<String> {
    let mut rows: Vec<String> = Vec::new();
    let mut indent = first;
    let mut row = " ".repeat(indent);
    let mut used = 0usize;
    for word in text.split_whitespace() {
        let mut word = word;
        loop {
            let room = width.saturating_sub(indent).max(1);
            let space = usize::from(used > 0);
            let length = word.chars().count();
            if used + space + length <= room {
                if space == 1 {
                    row.push(' ');
                }
                row.push_str(word);
                used += space + length;
                break;
            }
            if used > 0 {
                rows.push(std::mem::take(&mut row));
                indent = rest;
                row = " ".repeat(indent);
                used = 0;
                continue;
            }
            let head: String = word.chars().take(room).collect();
            word = &word[head.len()..];
            row.push_str(&head);
            rows.push(std::mem::take(&mut row));
            indent = rest;
            row = " ".repeat(indent);
        }
    }
    if used > 0 || rows.is_empty() {
        rows.push(row);
    }
    rows
}

/// `context::wrapped` exactly as 0.21.0 shipped it.
///
/// The same twenty lines with the two indents written in rather than taken as
/// arguments, which is the whole of what separated the two copies. Kept whole for
/// the same reason as above: a paraphrase would be testing the paraphrase.
fn wrapped_0_21_0(text: &str, width: usize) -> Vec<String> {
    let mut rows: Vec<String> = Vec::new();
    let mut indent = 2usize;
    let mut row = " ".repeat(indent);
    let mut used = 0usize;
    for word in text.split_whitespace() {
        let mut word = word;
        loop {
            let room = width.saturating_sub(indent).max(1);
            let space = usize::from(used > 0);
            let length = word.chars().count();
            if used + space + length <= room {
                if space == 1 {
                    row.push(' ');
                }
                row.push_str(word);
                used += space + length;
                break;
            }
            if used > 0 {
                rows.push(std::mem::take(&mut row));
                indent = 4;
                row = " ".repeat(indent);
                used = 0;
                continue;
            }
            let head: String = word.chars().take(room).collect();
            word = &word[head.len()..];
            row.push_str(&head);
            rows.push(std::mem::take(&mut row));
            indent = 4;
            row = " ".repeat(indent);
        }
    }
    if used > 0 || rows.is_empty() {
        rows.push(row);
    }
    rows
}

/// **The shared folder is the two it replaced, row for row, at every width.**
///
/// A de-duplication is a claim about behaviour that no user-facing test states:
/// `/status` and `/context` each have a suite full of `contains` assertions, and
/// not one of them would notice a fold that broke one column earlier — the words
/// are all still on the page, in order, in the same rows plus or minus one. The
/// only way to state "nothing moved" is to keep the thing it moved from.
///
/// The sweep runs over the fixtures that reach the split arm as well as the ones
/// that do not, because that arm is the one a rewrite gets wrong: it is reached
/// only when a single word cannot fit a row of its own, which on a real `/status`
/// page happens at eighty columns for a deep workspace path and never in a test
/// written against a short one.
///
/// `wrapped` is compared at its own two indents, since that is the only shape it
/// had; anything else would be comparing the new function against a version of the
/// old one that never existed.
///
/// Sabotage: change `used + space + length <= room` to `<`, which is the classic
/// off-by-one and is invisible on any page whose longest row is not exactly full —
/// under which this test fails at a handful of widths per fixture and every other
/// test in the repository stays green.
#[test]
fn the_shared_folder_is_the_two_copies_it_replaced() {
    for text in texts() {
        for width in 1..=120usize {
            for (first, rest) in [(0usize, 0usize), (2, 4), (0, 2), (4, 2), (8, 8)] {
                assert_eq!(
                    page::folded(&text, width, first, rest),
                    folded_0_21_0(&text, width, first, rest),
                    "the shared folder differs from `status::folded` at {width} columns, \
                     indents {first}/{rest}, on {text:?}",
                );
            }
            assert_eq!(
                page::folded(&text, width, 2, 4),
                wrapped_0_21_0(&text, width),
                "the shared folder differs from `context::wrapped` at {width} columns \
                 on {text:?}",
            );
        }
    }
}
