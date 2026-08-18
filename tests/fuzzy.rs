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
fn f8_the_row_that_contains_the_needle_whole_is_the_row_on_top() {
    // The defect this exists for, in the shape it actually shipped in. Every row of
    // a real catalogue opens with a vendor prefix, so every row donates an `o` to
    // `o4` at index 0; a walk that takes that first `o` and never reconsiders finds
    // the `4` of `openai/o4-mini` seven characters later and buried inside a word,
    // and scores it *below* `openai/gpt-4o`, whose `4` at least starts one. The row
    // holding the needle contiguously sat under the row that merely holds its
    // letters, and Enter took the wrong model.
    let run = score("openai/o4-mini", "o4").expect("the run matches");
    let scattered = score("openai/gpt-4o", "o4").expect("the scatter matches");
    assert!(
        run > scattered,
        "the row containing `o4` must outrank the row that only spells it: \
         openai/o4-mini {run} vs openai/gpt-4o {scattered}",
    );

    let catalogue = [
        "openai/gpt-4o",
        "anthropic/claude-opus-4",
        "openai/o4-mini",
        "google/gemini-2.5-pro",
    ];
    assert_eq!(
        rank(catalogue, "o4").first().copied(),
        Some(2),
        "typing `o4` at a catalogue must put `openai/o4-mini` under the marker",
    );
}

#[test]
fn f8_a_run_outranks_a_scatter_wherever_the_run_starts() {
    // Not just when the run happens to start a word. `zzzab` opens its run in the
    // middle of one and pays the full gap for getting there, while `x-a-b` spends
    // both its letters on word starts — and the run still has to win, because a run
    // is what the operator typing a name produces and a scatter is not.
    let buried = score("zzzab", "ab").expect("the buried run matches");
    let boundaries = score("x-a-b", "ab").expect("the boundary scatter matches");
    assert!(
        buried > boundaries,
        "a run must beat a scatter from anywhere: zzzab {buried} vs x-a-b {boundaries}",
    );
}

#[test]
fn f8_a_run_beats_a_scatter_over_every_short_label_there_is() {
    // The claim above is an ordering rule, not one example, so it is asserted over
    // every label the alphabet can spell rather than over the two that happen to
    // have caught the defect. Three characters — two letters and the separator that
    // makes word boundaries — and every label up to six of them: about a thousand,
    // which is a millisecond's work and the only proof that the run bonus is large
    // enough to cover a run that starts late.
    let labels = every_label_up_to(6);
    for needle in ["ab", "ba", "aab", "aba", "abb", "bab"] {
        let (worst_run, worst_label) = labels
            .iter()
            .filter(|label| label.contains(needle))
            .filter_map(|label| score(label, needle).map(|score| (score, label)))
            .min_by_key(|(score, _)| *score)
            .expect("some label contains the needle");
        let (best_scatter, best_label) = labels
            .iter()
            .filter(|label| !label.contains(needle))
            .filter_map(|label| score(label, needle).map(|score| (score, label)))
            .max_by_key(|(score, _)| *score)
            .expect("some label spells the needle without containing it");
        assert!(
            worst_run > best_scatter,
            "`{needle}`: the worst label containing it ({worst_label} at {worst_run}) \
             must still beat the best label merely spelling it \
             ({best_label} at {best_scatter})",
        );
    }
}

/// Every string over `a`, `b` and `-` up to `longest` characters.
fn every_label_up_to(longest: usize) -> Vec<String> {
    let mut all = Vec::new();
    let mut shorter = vec![String::new()];
    for _ in 0..longest {
        let mut longer = Vec::new();
        for label in &shorter {
            for letter in ['a', 'b', '-'] {
                longer.push(format!("{label}{letter}"));
            }
        }
        all.extend_from_slice(&longer);
        shorter = longer;
    }
    all
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
