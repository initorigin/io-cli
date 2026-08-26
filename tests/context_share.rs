//! F10 — `ctx N%` is true for an operator who configured their window.
//!
//! **The field this covers was wrong from the release it was added in, and it was
//! wrong in the way that costs the most: silently.** Through 0.16.0 the status
//! line divided the assembled section by
//! `ContextBudget::default().effective_tokens(None)` — a flat `24_000`, identical
//! on every session anybody has ever run — under a comment in `src/app.rs`
//! asserting that "the denominator is io-harness's own declared budget, asked of
//! the harness rather than copied here". It was the *crate's* default budget. An
//! operator who wrote a `[run.context]` table was shown a share of a window they
//! did not have, and nothing on the screen could contradict it: a percentage is
//! plausible at any value.
//!
//! So every assertion below is made against a contract built from **configuration
//! text**, through `io_cli::contract::configured` and `Config::from_toml`, and
//! never against a `TaskContract` this file assembled by hand. A test that set
//! `ContextBudget` on a builder would prove that `effective_tokens` divides, which
//! io-harness already proves; the property here is that an operator's *file*
//! reaches the number on the line. The three windows below are 8k, 20k and 24k and
//! they are deliberately all different, so restoring any one of the two wrong
//! denominators moves a number this file names.
//!
//! The second half of the criterion is when the number arrives. `EventKind::
//! Compacted` fires when a fold happens and never otherwise, so a session whose
//! context never filled showed no `ctx` field at all — blank for exactly the
//! period in which it would have been worth reading. The numerator now comes off
//! `ContextEvent::assembled`, which io-harness has recorded per step in its own
//! store the whole time.
//!
//! Nothing here reads a clock or sleeps: every fact is a row in an in-memory store
//! or a field on a struct.

use std::time::Duration;

use io_cli::app::App;
use io_cli::status::{Budgets, Status};
use io_cli::theme::DARK;
use io_harness::{Config, ContextBudget, ContextEvent, EventKind, RunEvent, Store, TaskContract};

// ---------------------------------------------------------------------------
// Fixtures
// ---------------------------------------------------------------------------

/// A contract as an operator's `io.toml` produces one.
///
/// **`Config::from_toml` and not a builder call.** The whole feature is that a
/// file reaches the status line, and `[run.context]` is a sub-table of `[run]`
/// rather than a table of its own — `Config::apply_to` returns early on a file
/// with no `[run]` table at all, so a fixture that wrote a top-level `[context]`
/// would be rejected by `deny_unknown_fields` and a fixture that wrote
/// `[run.context]` alone still works, because TOML creates `run` on the way past.
fn from_config(toml: &str) -> TaskContract {
    let config = Config::from_toml(toml).expect("the fixture's io.toml parses");
    io_cli::contract::configured(
        "summarise the module",
        std::path::PathBuf::from("/tmp/io-cli-context-share"),
        &config,
    )
}

/// `[run.context]` set: an 8k ceiling taking a quarter of a 40k run budget.
///
/// Both keys, because both are wrong in the shipped code — the ceiling was ignored
/// in favour of `24_000`, and the share was computed against `None` instead of
/// against what `[run] max_tokens` leaves. `min(8_000, max(40_000 * 0.25, floor))`
/// is `8_000`: the ceiling wins here, which is what makes this window distinct
/// from the one below.
fn tight() -> TaskContract {
    from_config(
        "[run]\n\
         max_tokens = 40000\n\
         \n\
         [run.context]\n\
         max_tokens = 8000\n\
         share = 0.25\n",
    )
}

/// No `[run.context]`, but a `[run] max_tokens` the default budget takes half of.
///
/// `min(24_000, max(40_000 * 0.5, floor))` is `20_000` — *not* `24_000`. This is
/// the window the second half of the defect throws away by passing `None`, and it
/// is why this fixture and [`uncapped`] must report different shares even though
/// neither one configures a context budget.
fn roomy() -> TaskContract {
    from_config("[run]\nmax_tokens = 40000\n")
}

/// No context budget and no token budget: io-harness's own `24_000`, flat.
///
/// This is the sabotage's value arrived at legitimately. A session that really
/// does run under the defaults should read `24_000`, and the shipped code read
/// `24_000` for *every* session — so this fixture is the one case the old
/// arithmetic got right, and it exists here to prove the new arithmetic still
/// gets it right rather than over-correcting.
fn uncapped() -> TaskContract {
    from_config("[run]\nmax_steps = 20\n")
}

/// A status line that has been handed a contract, as the driver hands it one.
fn under(contract: &TaskContract) -> Status {
    let mut status = Status::new("anthropic/claude-sonnet-4.5");
    status.budgets = Budgets::in_force(contract);
    status
}

/// A store with one run, and the assembly the harness recorded for a step of it.
fn assembled(step: u32, est_tokens: u64) -> (Store, i64) {
    let store = Store::memory().expect("an in-memory store");
    let run_id = store
        .start_run("summarise the module", "openrouter")
        .expect("a run");
    store
        .record_context_event(
            run_id,
            &ContextEvent::assembled(step, "carried=3 stubbed=0 reread=0", est_tokens),
        )
        .expect("the trace records an assembly");
    (store, run_id)
}

/// The event io-harness emits once a step has been committed to the store.
fn step(run_id: i64, step: u32) -> RunEvent {
    RunEvent::new(
        run_id,
        step,
        EventKind::Step {
            decision: "read the module".into(),
            tool_call: "read".into(),
            tokens: 900,
            changed: false,
        },
    )
}

/// The one denominator this feature must never use again.
///
/// Named as a function rather than as `24_000` so that the sabotage is *this
/// expression* — the exact call the shipped code made — and not a constant that
/// happens to equal it today.
fn the_defect() -> u64 {
    ContextBudget::default().effective_tokens(None)
}

// ---------------------------------------------------------------------------
// The denominator
// ---------------------------------------------------------------------------

/// **The test the sabotage kills.**
///
/// Restore `ContextBudget::default().effective_tokens(None)` as the denominator
/// and `tight` reports `17%` instead of `50%` here, on a fixture whose file plainly
/// says the window is 8k. Nothing else in the suite moves, which is the shape the
/// criterion names: it fails alone, and in the shipped code it failed silently.
#[test]
fn the_share_is_taken_against_the_configured_window_and_not_the_crate_default() {
    let contract = tight();
    assert_eq!(
        Budgets::in_force(&contract).window,
        Some(8_000),
        "`[run.context] max_tokens = 8000` is the window this turn assembles inside",
    );

    let mut status = under(&contract);
    status.note_context(4_000);
    assert_eq!(
        status.context,
        Some(50),
        "4k of an 8k window is half of it, whatever io-harness's own default is",
    );

    // And the number the defect produced, spelled out, so the diff between them is
    // on the record rather than left to arithmetic in a reader's head.
    let wrong = (4_000f64 / the_defect() as f64 * 100.0).round() as u8;
    assert_eq!(wrong, 17, "the shipped denominator was a flat 24k");
    assert_ne!(
        status.context,
        Some(wrong),
        "a share of the crate default is a share of a window this operator does not have",
    );
}

/// The other half of the same defect: `None` was passed for the run's allowance,
/// so `[run] max_tokens` never narrowed anything.
///
/// `roomy` and `uncapped` configure no context budget at all. They differ only in
/// whether the file sets `[run] max_tokens`, and under the shipped code they were
/// therefore indistinguishable — both `24_000`.
#[test]
fn a_run_token_budget_narrows_the_window_by_the_share_the_budget_declares() {
    assert_eq!(
        Budgets::in_force(&roomy()).window,
        Some(20_000),
        "the default budget takes its `share` of what `[run] max_tokens` leaves",
    );
    assert_eq!(
        Budgets::in_force(&uncapped()).window,
        Some(the_defect()),
        "with no run budget the ceiling is `max_tokens` flat, which is the one case \
         the shipped code got right",
    );

    let mut narrowed = under(&roomy());
    narrowed.note_context(4_000);
    let mut wide = under(&uncapped());
    wide.note_context(4_000);
    assert_eq!(narrowed.context, Some(20));
    assert_eq!(wide.context, Some(17));
    assert_ne!(
        narrowed.context, wide.context,
        "passing `None` for the remaining allowance collapses these two files into one",
    );
}

/// Three files, three windows, three shares of the same section.
///
/// The point is the spread. A denominator that ignores configuration makes all
/// three of these equal, and one assertion catching that is easier to argue away
/// than a table where every row moves.
#[test]
fn the_same_section_reads_differently_under_three_different_files() {
    let shares: Vec<Option<u8>> = [tight(), roomy(), uncapped()]
        .iter()
        .map(|contract| {
            let mut status = under(contract);
            status.note_context(4_000);
            status.context
        })
        .collect();
    assert_eq!(shares, vec![Some(50), Some(20), Some(17)], "{shares:?}");
}

/// No contract, no claim.
///
/// `Budgets::default()` is what a `Status` carries before the driver has built a
/// turn, and its window is `None` — *io-cli has not been told*, which is a
/// different thing from *there is no window*. The field is left alone rather than
/// falling back to anything: losing `ctx` from the line is a failure somebody
/// notices, and a share of an invented denominator is the failure this whole
/// release is undoing.
#[test]
fn a_status_that_has_never_seen_a_contract_says_nothing_at_all() {
    let mut status = Status::new("anthropic/claude-sonnet-4.5");
    assert_eq!(status.budgets.window, None);
    status.note_context(4_000);
    assert_eq!(
        status.context, None,
        "with no window in hand the honest answer is silence, not a default",
    );
    assert!(
        !status.line(200, &DARK).to_string().contains("ctx"),
        "and the line draws nothing for it",
    );
}

/// A section larger than the window it was assembled against is ordinary — the
/// budget is what the assembler aims at and the fold is what enforces it — so the
/// share saturates rather than reporting `ctx 150%`.
#[test]
fn an_oversized_section_saturates_instead_of_reading_as_a_bug_in_the_line() {
    let mut status = under(&tight());
    status.note_context(12_000);
    assert_eq!(status.context, Some(100));
}

// ---------------------------------------------------------------------------
// The numerator, and when it arrives
// ---------------------------------------------------------------------------

/// **The half no arithmetic could have fixed.** The field is filled from the first
/// step of the first turn, with no fold anywhere in the run.
#[test]
fn the_share_is_there_from_the_first_step_of_the_first_turn() {
    let (store, run_id) = assembled(1, 4_000);
    let mut status = under(&tight());
    assert_eq!(
        status.context, None,
        "before a step has landed there is nothing assembled to report",
    );

    status.note_context_from(&store, &step(run_id, 1));

    assert_eq!(
        status.context,
        Some(50),
        "one step, no fold, and the share is on the line",
    );
    assert!(
        status.line(200, &DARK).to_string().contains("ctx 50%"),
        "{:?}",
        status.line(200, &DARK).to_string(),
    );
}

/// The trace is read once a step has landed and at no other event.
///
/// `EventKind::ToolCall` is emitted *before* the result is known, so a read there
/// is a read of a row that may not exist yet — the same anchor `main.rs`'s
/// `commit_edits` documents, and the same reason.
#[test]
fn only_a_committed_step_makes_the_line_read_the_trace() {
    let (store, run_id) = assembled(1, 4_000);
    let mut status = under(&tight());

    for kind in [
        EventKind::ToolCall {
            name: "read".into(),
            target: "src/lib.rs".into(),
        },
        EventKind::Stalled,
    ] {
        status.note_context_from(&store, &RunEvent::new(run_id, 1, kind));
        assert_eq!(
            status.context, None,
            "only `Step` is documented as emitted after the row is written",
        );
    }

    status.note_context_from(&store, &step(run_id, 1));
    assert_eq!(status.context, Some(50));
}

/// The newest assembly wins, and it is the estimate for the section rather than
/// what the provider billed for the whole request.
#[test]
fn the_share_follows_the_latest_assembly_as_the_run_goes_on() {
    let (store, run_id) = assembled(1, 2_000);
    let mut status = under(&tight());
    status.note_context_from(&store, &step(run_id, 1));
    assert_eq!(status.context, Some(25));

    store
        .record_context_event(
            run_id,
            &ContextEvent::assembled(2, "carried=9 stubbed=1 reread=0", 6_000),
        )
        .expect("a second assembly");
    status.note_context_from(&store, &step(run_id, 2));
    assert_eq!(
        status.context,
        Some(75),
        "the window is not filling up at a rate the last fold could have told us",
    );
}

/// A trace with no assembly recorded — a run that failed before it composed
/// anything — leaves the field exactly as it was.
#[test]
fn a_run_with_nothing_assembled_yet_changes_nothing() {
    let store = Store::memory().expect("an in-memory store");
    let run_id = store.start_run("summarise", "openrouter").expect("a run");
    let mut status = under(&tight());
    status.note_context(4_000);

    status.note_context_from(&store, &step(run_id, 1));

    assert_eq!(
        status.context,
        Some(50),
        "an absent row is not a report of zero tokens assembled",
    );
}

// ---------------------------------------------------------------------------
// The fold, which is still the better answer at the moment it happens
// ---------------------------------------------------------------------------

/// `EventKind::Compacted` survives, and now divides by the same window.
///
/// It reports the section's new size the instant it shrinks, before any step has
/// assembled against it, so it is genuinely earlier than the trace read — and the
/// two agree, because `after_tokens` and `ContextEvent::est_tokens` are both the
/// assembler's estimate of the observation section.
#[test]
fn a_fold_reports_against_the_configured_window_too() {
    let mut app = App::new(DARK, "anthropic/claude-sonnet-4.5");
    app.status.budgets = Budgets::in_force(&tight());
    app.event(
        &RunEvent::new(
            1,
            4,
            EventKind::Compacted {
                through_step: 4,
                before_tokens: 7_600,
                after_tokens: 2_000,
            },
        ),
        Duration::ZERO,
    );
    assert_eq!(
        app.status.context,
        Some(25),
        "2k of the 8k window this file configured, not of io-harness's 24k",
    );
}

/// The fold and the trace cannot disagree: fed the same number, they produce the
/// same share. One quantity, one denominator, two arrival times.
#[test]
fn the_fold_and_the_trace_are_the_same_arithmetic() {
    let (store, run_id) = assembled(4, 2_000);
    let mut from_trace = under(&tight());
    from_trace.note_context_from(&store, &step(run_id, 4));

    let mut app = App::new(DARK, "anthropic/claude-sonnet-4.5");
    app.status.budgets = Budgets::in_force(&tight());
    app.event(
        &RunEvent::new(
            1,
            4,
            EventKind::Compacted {
                through_step: 4,
                before_tokens: 7_600,
                after_tokens: 2_000,
            },
        ),
        Duration::ZERO,
    );

    assert_eq!(app.status.context, from_trace.context);
}

// ---------------------------------------------------------------------------
// What survives a change of conversation
// ---------------------------------------------------------------------------

/// The share is a run fact and the window is a session fact, and `forget_run`
/// draws the line between them.
///
/// The share was an observation the undone run made about its own ledger: a rewind
/// or a `/resume` puts a different conversation on screen, whose section that
/// number never measured. The window is not cleared because nothing about it
/// changed — `io.toml` does not move while a session runs, which is the same
/// argument `Status::budgets` as a whole already rests on.
#[test]
fn a_rewind_forgets_the_share_and_keeps_the_window() {
    let mut status = under(&tight());
    status.note_context(4_000);
    assert_eq!(status.context, Some(50));

    status.forget_run();

    assert_eq!(
        status.context, None,
        "the section belonged to the undone run"
    );
    assert_eq!(
        status.budgets.window,
        Some(8_000),
        "the file did not change, so the next turn's window has not either",
    );

    // And the very next step of the next turn fills it again, with no contract
    // rebuild in between.
    let (store, run_id) = assembled(1, 6_000);
    status.note_context_from(&store, &step(run_id, 1));
    assert_eq!(status.context, Some(75));
}

// ---------------------------------------------------------------------------
// The page
// ---------------------------------------------------------------------------

/// `/status` names the window even before a turn has run.
///
/// The old row said `not known until the context has been folded once`, which was a
/// true description of a defect rather than of a session. The window is knowable
/// from the moment a contract exists, and it is the half an operator checking
/// whether their `[run.context]` table took effect is actually looking for.
#[test]
fn the_status_page_names_the_window_before_anything_has_been_assembled() {
    let dir = tempfile::tempdir().expect("a workspace");
    let store = Store::memory().expect("an in-memory store");
    let session = io_harness::Session::open(&store, dir.path()).expect("a session");
    let policy = io_harness::Policy::permissive();
    let contract = tight();

    let page = |status: &Status| {
        io_cli::status::committed(status, &session, &policy, &contract, None, &DARK, 100)
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

    let fresh = page(&under(&contract));
    assert!(
        fresh.contains("context: nothing assembled yet") && fresh.contains("8.0k"),
        "the window is knowable before the first turn and the page says so: {fresh}",
    );

    let mut status = under(&contract);
    status.note_context(4_000);
    let filled = page(&status);
    assert!(
        filled.contains("context: 50% of a 8.0k window"),
        "the page names the denominator it divided by: {filled}",
    );
    assert!(
        !filled.contains("24.0k"),
        "no surface may still be quoting io-harness's crate default: {filled}",
    );
}

// ---------------------------------------------------------------------------
// The driver
// ---------------------------------------------------------------------------

/// The trace read has to be wired into the event loop, and nothing under `tests/`
/// links the binary — so this reads the driver as text, the way `tests/compact.rs`
/// already does for the fold it reports.
///
/// **Both drain paths, and that is the whole of the assertion.** `src/main.rs`
/// runs the events twice — once on the select's drain half while the turn future
/// is suspended, and once after the turn has returned, because the last step of a
/// turn is exactly the one whose event the select loses to the turn's own return.
/// `commit_edits` is called from both for that reason. A share wired into one of
/// them would be a field that is correct all through a turn and stale on its final
/// step, which is the reading an operator trusts least.
#[test]
fn the_driver_reads_the_assembly_on_both_event_paths() {
    let driver = std::fs::read_to_string(
        std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("src/main.rs"),
    )
    .expect("the driver");

    // `note_context` and not `note_context_from`: a live run found the field and
    // `/context` disagreeing about the same turn — 0% against 4,363 of 24,000 —
    // because one measured the observation section and the other the request. The
    // driver now goes through one helper that prefers the request and falls back
    // to the trace, so what this counts is that helper.
    let called = driver.matches("note_context(app, store,").count();
    let edits = driver.matches("commit_edits(app, store,").count();
    assert!(
        called >= edits && edits > 0,
        "the share is read on {called} of the {edits} paths that already read per \
         step; a field filled on one drain half is stale on the last step of every \
         turn",
    );
    // And the fallback is still reachable, because a step lands before the
    // completion call after it is snapshotted: on the very first step the trace is
    // the only number there is.
    assert!(
        driver.contains("note_context_from(store, event)"),
        "the request is preferred, but the trace is what answers before one exists",
    );
}
