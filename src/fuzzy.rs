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
//! spending an entry on: the whole of the ranking below is sixty lines, and the
//! rows it ranks are a handful of labels typed against by a human, so nothing here
//! is on a path where an optimised implementation would be noticeable.

/// What a match adds for landing directly after the previous one.
///
/// The largest of the three because a consecutive run is the strongest evidence
/// that the operator is typing the row's actual name rather than picking letters
/// out of it.
///
/// Fifteen rather than eight, and the extra seven is load-bearing: it is what
/// makes a run beat a scatter *wherever the run starts*. Worst case, a run opens
/// mid-word and pays the full gap cap — `1 - GAP_CAP`, so −2 — while the scatter
/// it is up against opens at index 0 on a boundary, `1 + BOUNDARY`, so 7; and a
/// scatter, being a scatter, has at least one later character that is not
/// consecutive and so earns at most `1 + BOUNDARY - 1`, so 6, where the run earns
/// `1 + CONSECUTIVE`. Every other character is a wash, so the run wins by
/// `(1 + CONSECUTIVE) - (-2) - 7 - 6`, which is `CONSECUTIVE - 14`: positive from
/// fifteen up, and at eight it was negative, which is exactly how `openai/gpt-4o`
/// used to sit above `openai/o4-mini` for `o4`.
const CONSECUTIVE: i32 = 15;

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
/// It does not have to out-weigh what a scattered match accumulates, and a fixed
/// number never could once the needle is long enough. It does not have to because
/// a prefix *is* a contiguous run starting at index 0, which is the highest a walk
/// can score at all — `1 + BOUNDARY` for the first character and `1 + CONSECUTIVE`
/// for every one after it, with no gap to pay anywhere. So a prefix row already
/// out-walks a scattered row before this is added, and this and [`EXACT`] are
/// separators between tiers rather than the thing holding the tiers apart.
const PREFIX: i32 = 100;

/// Score `needle` against `haystack`, case-insensitively.
///
/// `None` when the needle is not a subsequence of the haystack; `Some(score)` when
/// it is, and a higher score is a better match. An empty needle matches everything
/// with the same score, which is what makes an empty query mean "no filter"
/// without the caller having to special-case it.
///
/// Each character after the first is still taken greedily — the first occurrence
/// after the one before it — but the *first* one is not: every position it could
/// occupy is walked and the best-scoring walk wins. Leftmost-only was wrong on the
/// case the filter exists for. Every row of a real catalogue opens with a vendor
/// prefix, so `o4` spent its `o` on the `o` of `openai/` in `openai/o4-mini` and
/// then found the `4` seven characters away, scoring below `openai/gpt-4o`, whose
/// `4` at least started a word — the row containing the needle *contiguously* sat
/// under the row that merely contained its letters. Only the second `o` can see
/// the `4` sitting next to it, and only a walk that starts there finds it.
pub fn score(haystack: &str, needle: &str) -> Option<i32> {
    let needle: Vec<char> = needle.chars().flat_map(char::to_lowercase).collect();
    if needle.is_empty() {
        return Some(0);
    }
    let hay: Vec<char> = haystack.chars().flat_map(char::to_lowercase).collect();

    // Every start the needle's first character could take, scored in full, best
    // kept. Bounded by how often that one character occurs, times the length of
    // the label: on a four-hundred-row catalogue of forty-character model ids that
    // is a few thousand character comparisons a keystroke, which is nothing. The
    // table that would find the genuinely optimal assignment is not worth it.
    let mut best: Option<i32> = None;
    for (start, found) in hay.iter().enumerate() {
        if *found != needle[0] {
            continue;
        }
        // `None` orders below every `Some`, so this is "keep the better of the
        // two" and the first successful walk with no special case around it.
        best = best.max(walk(&hay, &needle, start));
    }
    let mut total = best?;

    if hay.starts_with(&needle) {
        total += PREFIX;
    }
    if hay == needle {
        total += EXACT;
    }
    Some(total)
}

/// Score one assignment: the needle's first character pinned at `start`, every
/// character after it taken greedily. `None` when the rest of the needle does not
/// fit after `start`, which is a dead start rather than a failed match — another
/// start may still carry the whole needle.
fn walk(hay: &[char], needle: &[char], start: usize) -> Option<i32> {
    let mut total = 0;
    let mut from = start;
    let mut previous: Option<usize> = None;
    for wanted in needle {
        let at = from + hay[from..].iter().position(|found| found == wanted)?;
        total += 1;
        if previous.is_some_and(|before| before + 1 == at) {
            total += CONSECUTIVE;
        } else if at == 0 || !hay[at - 1].is_alphanumeric() {
            total += BOUNDARY;
        }
        // The distance skipped to reach this character, measured from the previous
        // match or from the start of the label for the first one. Scattered hits
        // pay for their scatter; a run pays nothing. The first character pays from
        // the start of the label even though the walk began at `start`, so that a
        // late start is still a worse match than an early one, all else equal.
        let skipped = match previous {
            Some(before) => at - before - 1,
            None => at,
        };
        total -= skipped.min(GAP_CAP) as i32;
        previous = Some(at);
        from = at + 1;
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
