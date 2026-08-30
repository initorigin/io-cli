//! How the work has gone: `/stats`.
//!
//! **The other half of the question `/cost` asks.** `/cost` says what the work
//! cost; this says whether it worked. They are the field's own two names and they
//! answer different questions, which is why every agent that has both keeps them
//! apart — and why `/usage` is an alias for `/cost` here rather than a third
//! screen, since `/usage` means plan and rate-limit status everywhere it exists
//! and this product has no plan to report.
//!
//! Every figure is a `Store` call io-cli has never made: `runs_by_outcome`,
//! `runs_by_day`, `first_try`, `gate_failures_by_phase`, `recovery` and
//! `store_size`, plus the latencies read off the provider calls themselves.
//!
//! # First-try is io-harness's definition, not one derived here
//!
//! It is tempting to count successful runs and call it a first-try rate. That
//! would count a run which failed three gates and then passed as a first-try
//! success. io-harness's `first_try` is finished **and** successful **and**
//! carrying no `gate_phase_failed` event, which is the question an operator is
//! actually asking, and it is a `Store` method precisely so nobody re-derives it
//! wrongly.
//!
//! `FirstTry` is three counts and deliberately not a rate, because the denominator
//! is a choice: first-try over every run counts runs that are still going, and
//! first-try over successful runs answers a different question again. This page
//! picks one and **names it in the row**, rather than printing a percentage whose
//! meaning a reader has to guess.
//!
//! # The two gate vocabularies stay apart
//!
//! io-harness records gate failures twice, in two tables, with two phase
//! vocabularies that do not overlap. `SandboxEvent` phases are `subject-compile`,
//! `criterion-compile`, `test-run` and `subject-emptied`; `GateAttempt` phases are
//! `review`, `command` and `contains`. They are different mechanisms and merging
//! them into one list would produce a chart whose categories mean two things. So
//! they get two headings.
//!
//! # Nothing here compacts anything
//!
//! `Store::compact` is a full `VACUUM`: it needs free disk roughly equal to the
//! database file, rewrites the whole thing, and is not quick. A page that reported
//! free space and then reclaimed it on its own would be a page that surprised
//! somebody mid-session. The free figure is reported and the reclaiming is not
//! offered here.

use io_harness::Store;

use crate::page::{self, Row};
use crate::theme::Theme;

/// How many of the most recent runs the latency sections read.
///
/// **A bound this module can actually keep, unlike the one `/cost` could not.**
/// `Store::runs` returns run ids newest first, and io-cli takes the head of that
/// list itself, so the sample is genuinely the last `N` runs and the page says so
/// in the heading. That is the difference between a bound and a truncation: a
/// heading reading "slowest calls of the last 200 runs" is a true statement about
/// a subset, where "by model" over a truncated read would have been a false
/// statement about the whole.
///
/// The same shape and the same reason as `crate::sessions::MAX_RUNS_SCANNED`.
pub const RECENT_RUNS: usize = 200;

/// How many slow calls to name.
const SLOWEST: usize = 5;

/// How many tables to break the file down by.
///
/// A named constant since 0.32.0, where it was a bare `5` in the loop — and, more
/// to the point, a cap that said nothing. **This is not a viewport problem at
/// all**: `/stats` is a committed page with unlimited rows, so a database with
/// nine tables was having four of them dropped on a page that had room for every
/// one. The cap stays, because a page that lists forty tables is not a summary;
/// what changes is that it says what it did not draw.
const TABLES: usize = 5;

/// The `/stats` page.
pub fn committed(
    store: &Store,
    theme: &Theme,
    width: u16,
) -> Result<Vec<ratatui::text::Line<'static>>, String> {
    let mut rows: Vec<Row> = Vec::new();

    rows.push(Row::heading("runs by outcome".to_string()));
    rows.extend(tallies(
        store.runs_by_outcome().map_err(|e| e.to_string())?,
        "no runs recorded",
    ));

    rows.push(Row::Blank);
    rows.push(Row::heading("runs by day".to_string()));
    rows.extend(tallies(
        store.runs_by_day().map_err(|e| e.to_string())?,
        "no runs recorded",
    ));

    rows.push(Row::Blank);
    rows.push(Row::heading("finished on the first try".to_string()));
    let first = store.first_try().map_err(|e| e.to_string())?;
    rows.push(Row::fact("runs finished", first.runs.to_string()));
    rows.push(Row::fact(
        "of those, succeeded",
        first.succeeded.to_string(),
    ));
    rows.push(Row::fact(
        "of those, with no gate failure",
        first.first_try.to_string(),
    ));
    // **The denominator is named in the row, not implied by a percent sign.**
    // io-harness gives three counts and refuses to pick one for exactly this
    // reason; a bare `73%` would be a number whose meaning the reader has to
    // reconstruct, and two readers would reconstruct it differently.
    if let Some(share) = (first.first_try * 100).checked_div(first.succeeded) {
        rows.push(Row::fact(
            "first-try share of successful runs",
            format!("{share}%"),
        ));
    }

    rows.push(Row::Blank);
    rows.push(Row::heading("gate failures by phase".to_string()));
    rows.extend(tallies(
        store.gate_failures_by_phase().map_err(|e| e.to_string())?,
        "no gate has failed",
    ));
    rows.push(Row::note(
        "these are the sandbox gate's phases; the review, command and contains \
         gates record separately and are not merged into this list"
            .to_string(),
    ));

    rows.push(Row::Blank);
    rows.push(Row::heading("recovery".to_string()));
    let recovery = store.recovery().map_err(|e| e.to_string())?;
    rows.push(Row::fact("fallbacks", recovery.fallbacks.to_string()));
    rows.push(Row::fact("replans", recovery.replans.to_string()));
    rows.push(Row::fact("resumes", recovery.resumes.to_string()));

    let ids = store.runs().map_err(|e| e.to_string())?;
    let scanned = ids.len().min(RECENT_RUNS);
    // **The head, because `Store::runs` is newest first.** Its own documentation
    // says so — `ORDER BY id DESC` — and taking the tail would have read the
    // *oldest* two hundred runs under a heading promising the most recent, which
    // is a false statement about a subset rather than a true one. That is the
    // whole difference this bound rests on.
    let recent = &ids[..scanned];
    let mut calls = Vec::new();
    for id in recent {
        calls.extend(store.provider_calls(*id).map_err(|e| e.to_string())?);
    }

    rows.push(Row::Blank);
    rows.push(Row::heading(format!(
        "slowest calls, of the last {scanned} run{}",
        if scanned == 1 { "" } else { "s" }
    )));
    let mut slow: Vec<(u64, String)> = calls
        .iter()
        .map(|call| {
            (
                call.latency_ms,
                call.model.clone().unwrap_or_else(|| call.provider.clone()),
            )
        })
        .collect();
    // Descending, so `take(SLOWEST)` below is the slowest and not the fastest.
    slow.sort_by_key(|(latency, _)| std::cmp::Reverse(*latency));
    if slow.is_empty() {
        rows.push(Row::note("no provider calls in those runs"));
    } else {
        for (ms, model) in slow.iter().take(SLOWEST) {
            rows.push(Row::fact(model.clone(), format!("{ms} ms")));
        }
    }

    rows.push(Row::Blank);
    rows.push(Row::heading("time to first token".to_string()));
    // **An unmeasured time to first token is `None`, never 0**, and it is counted
    // separately rather than averaged in as a zero — which would drag every mean
    // toward an instant nothing ever took.
    let measured: Vec<u64> = calls.iter().filter_map(|call| call.ttft_ms).collect();
    if measured.is_empty() {
        rows.push(Row::note(
            "no call in those runs reported one, which is unmeasured rather than instant",
        ));
    } else {
        let mut sorted = measured.clone();
        sorted.sort_unstable();
        rows.push(Row::fact(
            "median",
            format!("{} ms", sorted[sorted.len() / 2]),
        ));
        rows.push(Row::fact(
            "slowest",
            format!("{} ms", sorted[sorted.len() - 1]),
        ));
        rows.push(Row::fact(
            "measured on",
            format!("{} of {} calls", measured.len(), calls.len()),
        ));
    }

    rows.push(Row::Blank);
    rows.push(Row::heading("what the store holds".to_string()));
    let size = store.store_size().map_err(|e| e.to_string())?;
    rows.push(Row::fact("file", bytes(size.file_bytes)));
    rows.push(Row::fact("free inside it", bytes(size.free_bytes)));
    rows.push(Row::fact("sessions", size.sessions.to_string()));
    rows.push(Row::fact("runs", size.runs.to_string()));
    // **Absent rather than zeroed.** io-harness returns an empty table list when
    // the SQLite build lacks `dbstat`, which is a build that cannot answer the
    // question — not a database whose tables are all empty.
    if size.tables.is_empty() {
        rows.push(Row::note(
            "this SQLite build cannot break the file down by table",
        ));
    } else {
        for (name, count) in size.tables.iter().take(TABLES) {
            rows.push(Row::fact(name.clone(), bytes(*count)));
        }
        // What the cap held back. Silence here is indistinguishable from a
        // database with exactly five tables in it.
        if let Some(rest) = size
            .tables
            .len()
            .checked_sub(TABLES)
            .filter(|rest| *rest > 0)
        {
            rows.push(Row::note(format!(
                "{rest} smaller table{} not shown",
                if rest == 1 { "" } else { "s" }
            )));
        }
    }
    if size.free_bytes > 0 {
        rows.push(Row::note(format!(
            "{} could be returned to the filesystem, which is a full rewrite of the \
             database and is not done from this page",
            bytes(size.free_bytes)
        )));
    }

    Ok(page::commit("stats", &rows, theme, width))
}

/// Rows for a set of io-harness's own `Tally` groups.
fn tallies(tallies: Vec<io_harness::Tally>, empty: &str) -> Vec<Row> {
    if tallies.is_empty() {
        return vec![Row::note(empty.to_string())];
    }
    tallies
        .into_iter()
        .map(|tally| Row::fact(tally.key, tally.count.to_string()))
        .collect()
}

/// A byte count in the largest unit that leaves a whole number in front.
///
/// Powers of 1024 and named as such: this is a file on a disk, and a reader
/// comparing it against what `ls` said should not have to convert.
///
/// **Public since 0.27.0 so [`crate::store`] can reuse it rather than write a
/// second one.** Two spellings of the same quantity on two pages of one
/// interface is the shape 0.25.0 recorded when one fact acquired two holders —
/// and a store page reporting `8.2 MB` beside a stats page reporting `8.2 MiB`
/// is that defect in its most readable form.
pub fn bytes(count: u64) -> String {
    const UNITS: [(&str, u64); 4] = [
        ("GiB", 1024 * 1024 * 1024),
        ("MiB", 1024 * 1024),
        ("KiB", 1024),
        ("B", 1),
    ];
    for (name, size) in UNITS {
        if count >= size {
            if size == 1 {
                return format!("{count} B");
            }
            return format!("{}.{} {name}", count / size, (count % size) * 10 / size);
        }
    }
    "0 B".to_string()
}
