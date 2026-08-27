//! How the work has gone: `/stats`, and the four places it refuses to average.
//!
//! **Every figure on this page is a `Store` call io-cli had never made.** The
//! aggregates have been in io-harness since 0.30.0 — `runs_by_outcome`,
//! `runs_by_day`, `first_try`, `gate_failures_by_phase`, `recovery` and
//! `store_size` — and the interface over them reported none of it. Reading them is
//! the easy half; the hard half is the four questions the harness deliberately
//! refuses to answer, because each of them is a place where a page can quietly
//! answer a different question from the one its heading asks.
//!
//! `FirstTry` is three counts and not a rate, because the denominator is a
//! choice. `gate_failures_by_phase` covers one of two gate mechanisms, whose phase
//! vocabularies do not overlap. `ttft_ms` is `None` for a call nothing measured,
//! never zero. And `StoreSize::tables` is empty on a SQLite build without
//! `dbstat`, which is a build that cannot answer rather than a database whose
//! tables are all empty. Every one of those has an obvious wrong rendering that
//! reads perfectly well, and this file is mostly about those four.
//!
//! The store is real and driven through io-harness's own recording calls. A page
//! asserted against hand-built aggregate structs would be asserting its own format
//! strings; here the numbers come out of SQL that ran.

use io_harness::{
    CheckpointEvent, ContextEvent, GateOutcome, ProviderCall, SandboxEvent, Store, Usage,
};

use io_cli::glyphs::ASCII;
use io_cli::stats::{self, RECENT_RUNS};
use io_cli::theme::{Theme, DARK};

// ---------------------------------------------------------------------------
// Fixtures
// ---------------------------------------------------------------------------

/// Wide enough that nothing folds, so a row lookup finds a whole row.
const ROOMY: u16 = 200;

fn ascii() -> Theme {
    DARK.with_glyphs(ASCII)
}

fn call(model: &str, latency_ms: u64, ttft_ms: Option<u64>) -> ProviderCall {
    ProviderCall {
        step: 1,
        provider: "anthropic".into(),
        model: Some(model.to_string()),
        usage: Some(Usage {
            prompt_tokens: 900,
            completion_tokens: 100,
            total_tokens: 1_000,
            ..Default::default()
        }),
        latency_ms,
        ttft_ms,
        ..Default::default()
    }
}

/// A store with a history: runs that ended four ways, both gate mechanisms
/// having failed, all three recovery mechanisms having fired, and provider calls
/// with a mixture of measured and unmeasured first tokens.
///
/// Everything is recorded through the call io-harness's own documentation shows,
/// so the aggregates below are the aggregates a real run would produce rather
/// than rows this file inserted a shape it invented.
fn history() -> Store {
    let store = Store::memory().expect("an in-memory store");

    // Three runs that succeeded, one of which failed a gate on the way, and one
    // that stalled. `first_try` is finished AND successful AND carrying no
    // `gate_phase_failed` event, so this fixture is 5 finished, 3 succeeded,
    // 2 first try — three different numbers, which is what makes a page that
    // picked the wrong one visible.
    for outcome in ["success", "success", "stalled", "step_cap_reached"] {
        let run = store
            .start_run("summarise the module", "/repo")
            .expect("a run");
        store.finish_run(run, outcome).expect("the run finishes");
    }

    let retried = store.start_run("port the parser", "/repo").expect("a run");
    // **The sandbox gate's vocabulary.** Four phases, none of which overlaps the
    // review/command/contains set below.
    store
        .record_sandbox_event(&SandboxEvent::gate_phase_failed(retried, 2, "test-run"))
        .expect("the phase failure is recorded");
    store
        .record_sandbox_event(&SandboxEvent::gate_phase_failed(
            retried,
            4,
            "criterion-compile",
        ))
        .expect("the phase failure is recorded");
    // **The other gate's vocabulary, recorded in another table entirely.** These
    // must not reach the phase list: they are a different mechanism, and merging
    // them would produce a list whose categories mean two things.
    for phase in ["review", "command", "contains"] {
        store
            .put_gate_attempt(retried, 3, phase, GateOutcome::Failed, "did not hold")
            .expect("the gate attempt is recorded");
    }
    store.finish_run(retried, "success").expect("it finishes");

    // All three recovery mechanisms, one each, so a page that reported the wrong
    // counter cannot pass by coincidence.
    store
        .record_context_event(retried, &ContextEvent::served(1, "anthropic"))
        .expect("the fallback is recorded");
    store
        .record_context_event(retried, &ContextEvent::replan(3, "no progress"))
        .expect("the replan is recorded");
    store
        .record_checkpoint_event(&CheckpointEvent::resume(retried, 4, "after a crash"))
        .expect("the resume is recorded");

    // Five calls, three of which measured a first token. The two that did not are
    // the whole of the `None`-is-not-zero case.
    for (model, latency, ttft) in [
        ("claude-sonnet-4.5", 1_200u64, Some(300u64)),
        ("claude-sonnet-4.5", 4_213, Some(900)),
        ("claude-opus-4.1", 800, Some(100)),
        ("claude-opus-4.1", 2_500, None),
        ("claude-haiku-4.5", 150, None),
    ] {
        store
            .record_provider_call(retried, &call(model, latency, ttft))
            .expect("the call is recorded");
    }

    store
}

/// The `/stats` page as a reader sees it: every row, spans concatenated.
fn page(store: &Store) -> Vec<String> {
    stats::committed(store, &ascii(), ROOMY)
        .expect("the page draws")
        .iter()
        .map(|line| {
            line.spans
                .iter()
                .map(|span| span.content.as_ref())
                .collect::<String>()
        })
        .collect()
}

fn text(rows: &[String]) -> String {
    rows.join("\n")
}

/// The rows of one section: everything after its heading, up to the blank row
/// that separates it from the next.
///
/// **Sections are the whole point of `Row::Heading` existing**, and a test that
/// searched the whole page for a word could not tell a figure under the right
/// heading from the same figure under the wrong one — which is exactly the
/// failure the two gate vocabularies would produce.
fn section<'a>(rows: &'a [String], heading: &str) -> &'a [String] {
    let at = rows
        .iter()
        .position(|row| row == heading)
        .unwrap_or_else(|| panic!("no `{heading}` heading on the page:\n{}", text(rows)));
    let rest = &rows[at + 1..];
    let end = rest
        .iter()
        .position(|row| row.is_empty())
        .unwrap_or(rest.len());
    &rest[..end]
}

/// The value of the row reading `label: …` inside `rows`.
fn field<'a>(rows: &'a [String], label: &str) -> &'a str {
    let prefix = format!("{label}: ");
    rows.iter()
        .find_map(|row| row.trim_start().strip_prefix(&prefix))
        .unwrap_or_else(|| panic!("no `{label}` row here:\n{}", text(rows)))
}

/// Every heading the page draws, in the order it draws them.
///
/// A heading is indented to zero and every other row to two, which is what makes
/// this readable off the rendered rows rather than off an internal structure — and
/// it is also the property a reader in `--plain` depends on.
fn headings(rows: &[String]) -> Vec<&str> {
    rows.iter()
        .skip(1)
        .take(rows.len().saturating_sub(2))
        .filter(|row| !row.is_empty() && !row.starts_with(' '))
        .map(String::as_str)
        .collect()
}

// ---------------------------------------------------------------------------
// The page
// ---------------------------------------------------------------------------

/// **Every section draws, over a store with a real history in it.**
///
/// Eight lists on one page, and each is a separate `Store` read that can fail on
/// its own. A page that lost one would still render, still be edged, and still
/// answer seven questions — so the assertion is on the whole list of headings in
/// order rather than on any one of them, because that is the only form that
/// notices a section quietly leaving.
///
/// The sample bound is in the heading and not merely applied. `/cost` could not
/// bound its grouped reads honestly and says so in a comment; this page can,
/// because `Store::runs` returns ids and io-cli takes the tail itself — so the
/// sample really is the last N runs and the heading says which N. That is the
/// difference between a bound and a truncation: "the last 200 runs" is a true
/// statement about a subset where "by model" over a truncated read would be a
/// false statement about the whole.
///
/// Sabotage: drop any one `rows.push(Row::heading(…))` — under which the figures
/// beneath it join the section above and read as answers to its question.
#[test]
fn every_section_of_the_page_draws_with_its_scope_in_the_heading() {
    let store = history();
    let rows = page(&store);

    assert_eq!(
        headings(&rows),
        [
            "runs by outcome",
            "runs by day",
            "finished on the first try",
            "gate failures by phase",
            "recovery",
            // Five runs in the store, all of them inside the bound.
            "slowest calls, of the last 5 runs",
            "time to first token",
            "what the store holds",
        ],
        "a section left the page, or arrived under a name that does not say its \
         scope:\n{}",
        text(&rows),
    );

    // The bound the heading names is the module's own constant, so a release that
    // moves it moves both together and this stays a claim about honesty rather
    // than about the number two hundred.
    assert_eq!(RECENT_RUNS, 200);

    // Outcomes are the raw strings io-harness recorded, not a success/failure
    // collapse: "stalled" and "step_cap_reached" are different endings and the
    // distinction is the reason an operator is reading this page.
    let outcomes = section(&rows, "runs by outcome");
    assert_eq!(field(outcomes, "success"), "3");
    assert_eq!(field(outcomes, "stalled"), "1");
    assert_eq!(field(outcomes, "step_cap_reached"), "1");

    // Recovery: three counters, one each, so a page reading the wrong one cannot
    // pass by coincidence.
    let recovery = section(&rows, "recovery");
    assert_eq!(field(recovery, "fallbacks"), "1");
    assert_eq!(field(recovery, "replans"), "1");
    assert_eq!(field(recovery, "resumes"), "1");

    // The slowest call is named by its model and its latency, in that order,
    // because the question is which model is slow.
    let slowest = section(&rows, "slowest calls, of the last 5 runs");
    assert_eq!(field(slowest, "claude-sonnet-4.5"), "4213 ms");
    assert!(
        slowest.len() <= 5,
        "the slow list is unbounded: {slowest:?}",
    );
}

/// **The first-try share names its denominator in the row, rather than printing a
/// bare percentage.**
///
/// io-harness returns three counts and deliberately not a rate, and it says why:
/// *first_try / succeeded* is "when we got there, how often first time" and
/// *first_try / runs* is "how often does this work at all", and both are
/// legitimate questions with different answers. Returning one number would be
/// picking for the reader and hiding which was picked.
///
/// So the page picks one and says which. A row reading `73%` is a number two
/// readers reconstruct differently — one of them will read it as a share of every
/// run, including the ones still going — and they will disagree about whether the
/// agent got better this week.
///
/// The fixture makes the three counts three different numbers on purpose. A
/// fixture where every run succeeded first time would give the same percentage
/// under either denominator, and the test would pass against the wrong one.
///
/// Sabotage: `format!("{}%", first.first_try * 100 / first.runs)` with the label
/// left as it is — under which the row says "share of successful runs" and shows
/// a share of every run, which is the failure a bare percentage makes invisible.
#[test]
fn the_first_try_share_names_its_denominator_rather_than_printing_a_bare_percentage() {
    let store = history();
    let rows = page(&store);
    let first = section(&rows, "finished on the first try");

    // The three counts io-harness gives, each under a label that says what it
    // counts. Five runs finished, three of them succeeded, two of those had no
    // gate failure — three different numbers, from one fixture.
    assert_eq!(field(first, "runs finished"), "5");
    assert_eq!(field(first, "of those, succeeded"), "3");
    assert_eq!(field(first, "of those, with no gate failure"), "2");

    // The share, and the label carries the denominator rather than the reader
    // having to supply one. Two of three is sixty-six percent; two of five would
    // be forty, which is the number the wrong denominator produces.
    assert_eq!(
        field(first, "first-try share of successful runs"),
        "66%",
        "the share is not two of the three runs that succeeded:\n{}",
        text(&rows),
    );
    assert!(
        !first.iter().any(|row| row.trim() == "66%"),
        "the percentage is drawn without a label saying what it is a share of: {first:?}",
    );

    // A store where nothing has succeeded yet has no share to draw, and draws
    // none rather than dividing by zero or reporting a confident nought percent.
    let empty = Store::memory().expect("an in-memory store");
    let run = empty.start_run("summarise", "/repo").expect("a run");
    empty.finish_run(run, "stalled").expect("it finishes");
    let rows = page(&empty);
    let first = section(&rows, "finished on the first try");
    assert_eq!(field(first, "of those, succeeded"), "0");
    assert!(
        !first.iter().any(|row| row.contains('%')),
        "a share was drawn over a denominator of zero: {first:?}",
    );
}

/// **The two gate vocabularies stay under separate headings, because they are two
/// mechanisms and not two spellings.**
///
/// io-harness records gate failures twice, in two tables, with two phase
/// vocabularies that do not overlap. `SandboxEvent` phases are `subject-compile`,
/// `criterion-compile`, `test-run` and `subject-emptied`; `GateAttempt` phases are
/// `review`, `command` and `contains`. Merged into one list they would produce a
/// chart whose categories mean two different things, and the operator reading
/// "gate failures by phase" would be adding a compile failure to a review
/// disagreement.
///
/// The fixture has both, failing, at the same time — which is the only way to
/// state that the page keeps them apart rather than that the page has only ever
/// seen one of them. The note is the other half: a reader who came looking for
/// their failing `review` gate has to be told where it is not, or they will read
/// its absence as "the review gate has never failed".
///
/// Sabotage: `UNION ALL` the two tables behind `gate_failures_by_phase`, or add a
/// second `tallies` call over the gate attempts under the same heading — under
/// which the counts still add up, the page still renders, and this is the only
/// test that fails.
#[test]
fn the_two_gate_vocabularies_stay_under_separate_headings() {
    let store = history();
    let rows = page(&store);
    let gates = section(&rows, "gate failures by phase");

    assert_eq!(field(gates, "test-run"), "1");
    assert_eq!(field(gates, "criterion-compile"), "1");

    // The other mechanism's phases are not rows here. Checked as rows rather than
    // as words, because the note below deliberately names all three in prose.
    for phase in ["review", "command", "contains"] {
        assert!(
            gates
                .iter()
                .all(|row| !row.trim_start().starts_with(format!("{phase}: ").as_str())),
            "`{phase}` is a gate attempt and is counted under the sandbox gate's \
             phases: {gates:?}",
        );
    }

    // And the reader is told where it went, in the section itself rather than in
    // a footnote somewhere else — because the question is asked while looking at
    // this list.
    let note = text(gates);
    assert!(
        note.contains("review, command and contains"),
        "the section does not say which gate's phases these are not:\n{note}",
    );
    assert!(
        note.contains("not merged into this list"),
        "the section does not say the other gate records separately:\n{note}",
    );
}

/// **An unmeasured time to first token is `None`, and it is counted apart rather
/// than averaged in as a zero.**
///
/// A zero would drag every figure on the section toward an instant nothing ever
/// took, and it would do it worst on exactly the paths that do not stream — which
/// are the paths an operator is most likely to be diagnosing when they open this
/// page. So the unmeasured calls are excluded from the statistics and the count of
/// what was measured is drawn beside them, which is what makes the median
/// interpretable at all.
///
/// The median is computed here from the seeded latencies rather than read off the
/// page and asserted against itself.
///
/// Sabotage: `call.ttft_ms.unwrap_or(0)` in the collector — under which the median
/// of this fixture drops from 300 ms to 100 ms, the slowest is unchanged, and
/// nothing says two of the five calls were never measured.
#[test]
fn an_unmeasured_time_to_first_token_is_counted_apart_and_never_averaged_as_zero() {
    let store = history();
    let rows = page(&store);
    let ttft = section(&rows, "time to first token");

    // The three measured values, sorted, and the median io-harness's own data
    // would give: the middle of what was measured, not the middle of what was
    // called.
    let measured = [100u64, 300, 900];
    assert_eq!(
        field(ttft, "median"),
        format!("{} ms", measured[measured.len() / 2]),
        "the median counted the calls that measured nothing:\n{}",
        text(&rows),
    );
    assert_eq!(
        field(ttft, "slowest"),
        format!("{} ms", measured[measured.len() - 1]),
    );
    assert_eq!(
        field(ttft, "measured on"),
        "3 of 5 calls",
        "the page does not say how much of the sample it is speaking for",
    );
    // **A whole figure of `0 ms`, not the substring.** `text(ttft).contains("0 ms")`
    // was the first spelling of this and it is a false positive on every measured
    // value that happens to end in a zero: `300 ms` and `100 ms` both contain it.
    // The claim is that no *figure* is zero, so the test is on the value of a row.
    assert!(
        !ttft.iter().any(|row| row.trim_end().ends_with(": 0 ms")),
        "an unmeasured first token reached the figures as an instant one:\n{}",
        text(ttft),
    );

    // A store whose calls measured nothing says so in words rather than drawing a
    // median of nought.
    let store = Store::memory().expect("an in-memory store");
    let run = store.start_run("summarise", "/repo").expect("a run");
    store
        .record_provider_call(run, &call("claude-sonnet-4.5", 900, None))
        .expect("recorded");
    let rows = page(&store);
    let ttft = section(&rows, "time to first token");
    assert!(
        text(ttft).contains("unmeasured rather than instant"),
        "a sample that measured nothing drew a figure anyway: {ttft:?}",
    );
    assert!(
        !text(ttft).contains("median"),
        "a median was drawn over nothing: {ttft:?}",
    );
}

/// **A SQLite build that cannot break the file down by table says so, rather than
/// drawing the breakdown as zeroes or omitting it in silence.**
///
/// `dbstat` is a virtual table over the b-tree pages and it is compiled in or it
/// is not. io-harness returns an empty table list on a build without it, and it
/// says why in a comment: that is a build that cannot answer the question, not a
/// database whose tables are all empty. The two are indistinguishable in the data
/// and completely different facts.
///
/// **Which branch runs here is a property of the SQLite this crate happens to
/// link, not of io-cli**, so the test asserts the mapping in both directions
/// rather than picking one and pretending the other cannot happen: a breakdown
/// present means no excuse drawn, and a breakdown absent means the excuse drawn.
/// Asserting one branch would go green on the developer's machine and say nothing
/// about CI — where the SQLite is built by a different job on a different
/// platform, which is exactly where this would differ.
///
/// The file's own figures stand either way, which is the other half of io-harness's
/// argument for not failing the whole size call over it.
///
/// Sabotage: draw the table rows unconditionally — under which a build without
/// `dbstat` shows a `what the store holds` section with the size, the free space
/// and nothing at all where the breakdown should be, and no sentence saying which
/// of "there are no tables" and "I cannot see the tables" is true.
#[test]
fn an_absent_table_breakdown_reads_as_cannot_break_down_rather_than_as_zeroes() {
    let store = history();
    let size = store.store_size().expect("the store reports its size");
    let rows = page(&store);
    let held = section(&rows, "what the store holds");
    let drawn = text(held);

    // The figures that do not depend on `dbstat` are there either way.
    assert_eq!(field(held, "sessions"), "0", "no session was opened");
    assert_eq!(field(held, "runs"), "5");
    assert!(
        !field(held, "file").starts_with("0 "),
        "a store with five runs in it reported no file at all: {drawn}",
    );

    let excuse = "cannot break the file down by table";
    if size.tables.is_empty() {
        assert!(
            drawn.contains(excuse),
            "this build has no `dbstat` and the page does not say so:\n{drawn}",
        );
        assert!(
            !held.iter().any(|row| row.contains("provider_calls")),
            "a breakdown was drawn from a table list that is empty: {held:?}",
        );
    } else {
        assert!(
            !drawn.contains(excuse),
            "this build has `dbstat` and the page claims it does not:\n{drawn}",
        );
        // Largest first, and no more than five, so the section stays a summary.
        let (largest, _) = &size.tables[0];
        assert!(
            drawn.contains(largest.as_str()),
            "the largest table is not in the breakdown:\n{drawn}",
        );
        assert!(
            size.tables
                .iter()
                .take(5)
                .all(|(name, _)| drawn.contains(name.as_str())),
            "the breakdown is shorter than the five rows it promises:\n{drawn}",
        );
    }
}

/// **Nothing on this page reclaims anything, and the free figure says so.**
///
/// `Store::compact` is a full `VACUUM`: it needs free disk roughly equal to the
/// database file, it rewrites the whole thing, and it is not quick. A page that
/// reported free space and then reclaimed it on its own would be a page that
/// surprised somebody mid-session — and this one is opened while a turn may be in
/// flight.
///
/// The sentence is only drawn when there is something to reclaim, because a page
/// that explained a decision about nothing every time it was opened would be a
/// page nobody finishes reading.
///
/// Sabotage: call `Store::compact` from this page, or drop the sentence and leave
/// the figure — under which an operator reads a number with no idea whether it is
/// about to move.
#[test]
fn the_page_reports_free_space_and_does_not_offer_to_reclaim_it() {
    let store = history();
    let before = store.store_size().expect("a size");
    let rows = page(&store);
    let held = section(&rows, "what the store holds");

    // **Named units, and the disjunction that used to stand here was no test at
    // all**: `ends_with(..) || !is_empty()` is satisfied by the second half for
    // every non-empty string, so a figure rendered as a bare byte count would have
    // passed it. `stats::bytes` renders exactly four units and this asserts on
    // that set, so a figure that lost its suffix fails here rather than on a
    // reader's screen.
    let free = field(held, "free inside it");
    assert!(
        ["B", "KiB", "MiB", "GiB"]
            .iter()
            .any(|unit| free.ends_with(unit)),
        "the free figure has no unit: {free:?} in {held:?}",
    );
    if before.free_bytes > 0 {
        assert!(
            text(held).contains("is not done from this page"),
            "free space was reported with no word about what would reclaim it: {held:?}",
        );
    }

    // Reading the page changed nothing about the store. The figures a second read
    // gives are the figures the first one gave.
    let after = store.store_size().expect("a size");
    assert_eq!(
        (before.file_bytes, before.free_bytes, before.runs),
        (after.file_bytes, after.free_bytes, after.runs),
        "drawing the page moved the database it was reporting on",
    );
}

/// **An empty store draws every section and reports nothing, rather than a row of
/// zeroes.**
///
/// The distinction every counter in this product is held to: "no run has been
/// recorded" and "every run scored zero" are different facts, and a page that
/// drew the second when the first was true would be reporting a corpus that does
/// not exist. It is also the state a brand new install is in, which is to say the
/// first thing anybody sees.
///
/// The page still has all eight sections. An empty state that quietly dropped its
/// headings would leave a first-time reader with no idea what the page is for.
///
/// Sabotage: return `Vec::new()` from `tallies` on an empty list rather than a
/// note — under which the headings sit above nothing at all and read as sections
/// that failed rather than as questions with no answer yet.
#[test]
fn an_empty_store_says_nothing_is_recorded_rather_than_drawing_zeroes() {
    let store = Store::memory().expect("an in-memory store");
    let rows = page(&store);
    let drawn = text(&rows);

    assert_eq!(
        headings(&rows).len(),
        8,
        "an empty store lost a section:\n{drawn}",
    );
    // Zero runs, so the sample is zero runs and the heading says so in the
    // singular-aware form rather than reading "the last 0 run".
    assert!(
        headings(&rows).contains(&"slowest calls, of the last 0 runs"),
        "the sample heading does not survive an empty store: {:?}",
        headings(&rows),
    );

    for heading in ["runs by outcome", "runs by day"] {
        let empty = section(&rows, heading);
        assert_eq!(
            empty.len(),
            1,
            "`{heading}` drew more than a note: {empty:?}"
        );
        assert_eq!(
            empty[0], "  no runs recorded",
            "`{heading}` drew a zero where it has nothing to report",
        );
    }
    assert!(
        text(section(&rows, "gate failures by phase")).contains("no gate has failed"),
        "an empty store reported gate failures:\n{drawn}",
    );
    assert!(
        text(section(&rows, "slowest calls, of the last 0 runs"))
            .contains("no provider calls in those runs"),
        "an empty store drew a slow list:\n{drawn}",
    );

    // The counters that genuinely are zero are still drawn as zero: nobody has
    // fallen back, replanned or resumed, and that is a measured nought rather
    // than an absence.
    let recovery = section(&rows, "recovery");
    assert_eq!(field(recovery, "fallbacks"), "0");
    assert_eq!(field(recovery, "replans"), "0");
    assert_eq!(field(recovery, "resumes"), "0");
}
