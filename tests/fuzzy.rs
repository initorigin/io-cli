//! The subsequence matcher. Every filtering surface in the product ranks with
//! this, so its ordering is asserted once here rather than through whichever
//! widget happens to be calling it.

use io_cli::fuzzy::{rank, score};

#[test]
fn f8_a_needle_matches_only_when_it_is_a_subsequence() {
    // In order, but not next to each other: this is the whole reason the filter
    // is a subsequence match and not a substring one.
    assert!(score("openai/gpt-4o", "oai").is_some());
    assert!(score("Anthropic", "atc").is_some());

    // Out of order is not a match, however many of the letters are present.
    assert_eq!(score("Anthropic", "ci"), None);
    // A letter that is not there at all is not a match either.
    assert_eq!(score("Anthropic", "z"), None);
}

#[test]
fn f8_case_is_ignored_in_both_directions() {
    assert!(score("OpenRouter", "openrouter").is_some());
    assert!(score("openrouter", "OPENROUTER").is_some());
    assert_eq!(
        score("OpenRouter", "opr"),
        score("openrouter", "OPR"),
        "the case of either side must not change the ranking",
    );
}

#[test]
fn f8_an_empty_needle_matches_everything_with_one_score() {
    // What makes an empty query mean "no filter" without the picker having to
    // special-case it: every row matches, and every row scores the same, so the
    // stable sort hands back the caller's own order untouched.
    assert_eq!(score("OpenRouter", ""), Some(0));
    assert_eq!(score("", ""), Some(0));
    assert_eq!(score("a very long label indeed", ""), Some(0));
    assert_eq!(rank(["c", "a", "b"], ""), vec![0, 1, 2]);
}

#[test]
fn f8_exact_beats_prefix_beats_scattered() {
    // The ordering the contract names, asserted as an ordering rather than
    // against three magic numbers, because the numbers are an implementation
    // detail and the ordering is the promise.
    let exact = score("dark", "dark").expect("an exact match matches");
    let prefix = score("darker", "dark").expect("a prefix match matches");
    let scattered = score("dust and rocks kept", "dark").expect("a scattered match matches");

    assert!(
        exact > prefix,
        "an exact match must outrank a prefix: {exact} vs {prefix}",
    );
    assert!(
        prefix > scattered,
        "a prefix must outrank a scattered subsequence: {prefix} vs {scattered}",
    );
}

#[test]
fn f8_a_consecutive_run_beats_the_same_letters_scattered() {
    let run = score("xab", "ab").expect("a run matches");
    let scattered = score("xayb", "ab").expect("a scattered pair matches");
    assert!(
        run > scattered,
        "consecutive hits must score better: {run} vs {scattered}",
    );
}

#[test]
fn f8_a_word_boundary_beats_a_hit_inside_a_word() {
    // The labels these rank are full of separators — `Any OpenAI-compatible
    // endpoint`, `claude-opus`, `openai/gpt-4o` — and a reader typing `c` for
    // `compatible` means the word, not the `c` buried in `Anthropic`.
    let boundary = score("any openai-compatible", "c").expect("a boundary hit matches");
    let inside = score("anthropic", "c").expect("a buried hit matches");
    assert!(
        boundary > inside,
        "a hit that starts a word must score better: {boundary} vs {inside}",
    );
}

#[test]
fn f8_equal_scores_keep_the_order_they_arrived_in() {
    // The defect this exists for: an unstable sort makes the row under the marker
    // swap between two equally good candidates on a keystroke that did not even
    // change the result, which is how Enter takes a row nobody chose.
    let rows = ["ant", "and", "any", "ash"];
    assert_eq!(
        rank(rows, "a"),
        vec![0, 1, 2, 3],
        "four identical scores must come back in the caller's order",
    );
    // And the tie-break survives a filter that drops a row out of the middle.
    assert_eq!(rank(rows, "an"), vec![0, 1, 2]);
}

#[test]
fn f8_rank_indexes_the_input_and_not_its_own_output() {
    // `rank` reorders; the numbers it returns must still address the list handed
    // in. A rank that renumbered would point every caller at the wrong row.
    let rows = ["zebra", "gpt-4o", "gpt-4o-mini"];
    let ordered = rank(rows, "gpt");
    assert_eq!(ordered.len(), 2, "zebra has no `gpt` in it: {ordered:?}");
    assert!(
        ordered.iter().all(|index| *index > 0),
        "index 0 is `zebra`, which did not match: {ordered:?}",
    );
    for index in ordered {
        assert!(rows[index].contains("gpt"));
    }
}
