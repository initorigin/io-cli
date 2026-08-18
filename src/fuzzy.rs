//! The subsequence matcher: one ranking for every list the product filters.
//!
//! The picker filters with it, and the slash palette and the template rows filter
//! with it too, because they *are* pickers. Writing it once is the reason a query
//! typed at one of them orders its rows the same way as a query typed at another —
//! three matchers would be three different answers to "why is that row first?",
//! and the operator would have to learn each of them.
//!
//! It is a subsequence match rather than a substring match: the characters have to
//! appear in order, but not next to each other, so `oai` finds `openai/gpt-4o` in a
//! four-hundred-row catalogue without anybody having to remember where the slashes
//! go. The scoring exists so that when several rows match — and in a catalogue that
//! size several always do — the one the operator meant is at the top rather than
//! merely present somewhere in the list.
//!
//! Deliberately not a crate. `tests/dependencies.rs` asserts this crate's exact
//! dependency list in both directions, and a fuzzy finder is not something worth
//! spending an entry on: the whole of the ranking below is forty lines, and the
//! rows it ranks are a handful of labels typed against by a human, so nothing here
//! is on a path where an optimised implementation would be noticeable.

/// What a match adds for landing directly after the previous one.
///
/// The largest of the three because a consecutive run is the strongest evidence
/// that the operator is typing the row's actual name rather than picking letters
/// out of it.
const CONSECUTIVE: i32 = 8;

/// What a match adds for starting a word.
///
/// A word is anything after a non-alphanumeric character, which covers the
/// separators these labels actually use — the space in `Any OpenAI-compatible
/// endpoint`, the hyphen in `claude-opus`, the slash in `openai/gpt-4o`.
const BOUNDARY: i32 = 6;

/// The most a single gap can cost.
///
/// Capped, so a match late in a long label is worse than a match early in it
/// without being worse than no match at all. Uncapped, one four-hundred-character
/// row could score below a row that does not match, and the ordering would stop
/// meaning anything.
const GAP_CAP: usize = 3;

/// What an exact match adds, over and above the prefix bonus it also earns.
const EXACT: i32 = 200;

/// What a prefix match adds.
///
/// Larger than any run of per-character bonuses a scattered match can accumulate,
/// which is what makes the three tiers of the contract's ordering — exact, then
/// prefix, then scattered — hold for every needle rather than for the short ones.
const PREFIX: i32 = 100;

/// Score `needle` against `haystack`, case-insensitively.
///
/// `None` when the needle is not a subsequence of the haystack; `Some(score)` when
/// it is, and a higher score is a better match. An empty needle matches everything
/// with the same score, which is what makes an empty query mean "no filter"
/// without the caller having to special-case it.
///
/// The walk is greedy and leftmost: each needle character takes the first
/// occurrence after the one before it. That is not the highest-scoring assignment
/// in general — `oo` against `octopus zoo` would score better taking the pair at
/// the end — but finding the best one costs a table, and the leftmost walk is what
/// a reader predicts from watching the highlight move as they type.
pub fn score(haystack: &str, needle: &str) -> Option<i32> {
    let needle: Vec<char> = needle.chars().flat_map(char::to_lowercase).collect();
    if needle.is_empty() {
        return Some(0);
    }
    let hay: Vec<char> = haystack.chars().flat_map(char::to_lowercase).collect();

    let mut total = 0;
    let mut from = 0;
    let mut previous: Option<usize> = None;
    for wanted in &needle {
        let at = from + hay[from..].iter().position(|found| found == wanted)?;
        total += 1;
        if previous.is_some_and(|before| before + 1 == at) {
            total += CONSECUTIVE;
        } else if at == 0 || !hay[at - 1].is_alphanumeric() {
            total += BOUNDARY;
        }
        // The distance skipped to reach this character, measured from the previous
        // match or from the start of the label for the first one. Scattered hits
        // pay for their scatter; a run pays nothing.
        let skipped = match previous {
            Some(before) => at - before - 1,
            None => at,
        };
        total -= skipped.min(GAP_CAP) as i32;
        previous = Some(at);
        from = at + 1;
    }

    if hay.starts_with(&needle) {
        total += PREFIX;
    }
    if hay == needle {
        total += EXACT;
    }
    Some(total)
}

/// The indices of every haystack the needle matches, best first.
///
/// The result addresses the *input* — index 3 means the fourth haystack handed in,
/// whatever position it ends up in here. Every caller of this reads the index back
/// against its own list, so a rank that renumbered would be a rank that pointed at
/// the wrong row.
///
/// **The sort is stable and the input order is the tie-break.** Rows scoring the
/// same keep the order they arrived in, so the row under the marker does not jump
/// between two equally good candidates as the next character is typed — which is
/// the difference between a filter and a shuffle, and matters most on exactly the
/// keystroke where the operator is about to press Enter.
pub fn rank<'a>(haystacks: impl IntoIterator<Item = &'a str>, needle: &str) -> Vec<usize> {
    let mut scored: Vec<(usize, i32)> = haystacks
        .into_iter()
        .enumerate()
        .filter_map(|(index, haystack)| score(haystack, needle).map(|score| (index, score)))
        .collect();
    // `sort_by_key` rather than a comparator, and stable either way: equal scores
    // keep the order they arrived in, which is the tie-break the doc above promises.
    scored.sort_by_key(|scored| std::cmp::Reverse(scored.1));
    scored.into_iter().map(|(index, _)| index).collect()
}
