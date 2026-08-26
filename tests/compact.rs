//! F8 — `/compact` folds, and reports the fold rather than the request.
//!
//! **What this file can and cannot prove, first, because the gap is the shape of
//! the feature.**
//!
//! io-harness documents four conditions under which a fold request that was
//! accepted folds nothing — it is not immediate, it does not override an off
//! setting, it loses to an interrupt at the same boundary, and it does nothing
//! when there is nothing to fold — and it says what an interface may do about
//! them: it "must not report a fold on the strength of having sent one — read the
//! `Compacted` event instead."
//!
//! So `Steer::fold()` returning `Ok` proves nothing, exactly as `Steer::say`
//! returning `Ok` proves nothing for `/steer`. What is provable here is that
//! io-cli never mistakes the one for the other:
//!
//! 1. **A fold is claimed only from the event.** `Said::Folded` has one
//!    constructor and it takes a `RunEvent`; every other arm — sent, armed, off,
//!    and the turn that ended with no event — answers `is_fold()` with `false`.
//! 2. **The short conversation reports nothing to fold.** This is the arm the
//!    sabotage lands on, and the test named below is the one that kills it.
//! 3. **Folding off is decided from the configuration, before anything is sent.**
//!    The only condition of the four that can be predicted, and the reason
//!    `Said::asked` takes a `Compaction` at all.
//! 4. **The summary names what it replaced**, out of the run's own `summaries`
//!    row — a count the `Compacted` event does not carry and the store does.
//!
//! **Not provable without a network, and owed to the live run:** that a real turn
//! folds when asked. That evidence is one `EventKind::Compacted` in a real run's
//! trace with a `summaries` row beside it, and it comes from io-harness rather
//! than from anything asserted here.

use io_cli::commands::{self, Action, COMMANDS};
use io_cli::compact::Said;
use io_cli::keys::Keys;
use io_cli::theme::DARK;
use io_harness::{Compaction, EventKind, RunEvent, Store};

/// The separator the sentences are formatted with. Any of them would do — the
/// assertions below are about the words, not the glyph.
fn dash() -> &'static str {
    DARK.glyphs.dash
}

/// Folding on, at io-harness's own defaults: `at_share` 0.8, `keep_recent` 8.
fn folding() -> Compaction {
    Compaction::default()
}

/// Folding off, the way io-harness documents off: a threshold the ledger cannot
/// reach, which is a setting rather than an absence.
fn never() -> Compaction {
    Compaction {
        at_share: 1.0,
        ..Compaction::default()
    }
}

fn compacted(run_id: i64, through_step: u32, before: u64, after: u64) -> RunEvent {
    RunEvent::new(
        run_id,
        through_step,
        EventKind::Compacted {
            through_step,
            before_tokens: before,
            after_tokens: after,
        },
    )
}

fn store() -> Store {
    Store::memory().expect("an in-memory store")
}

/// `src/main.rs` as text. Nothing under `tests/` links the binary, so a decision
/// made in the driver is one no test can drive; this reads the driver instead.
fn driver() -> String {
    std::fs::read_to_string(std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("src/main.rs"))
        .expect("the driver")
}

// --- F8: a fold is claimed only from the event -------------------------------

/// **The asymmetry that is the whole feature.** A request can be built out of a
/// setting and a boolean; a fold cannot be built out of anything but an event.
#[test]
fn f8_only_the_event_makes_a_fold() {
    assert!(
        !Said::asked(folding(), true).is_fold(),
        "a request sent into a running turn is a request",
    );
    assert!(
        !Said::asked(folding(), false).is_fold(),
        "a request armed on the next turn's contract is a request",
    );
    assert!(!Said::asked(never(), true).is_fold());
    assert!(!Said::unfolded(folding(), false).is_fold());
    assert!(!Said::unfolded(folding(), true).is_fold());

    // And the constructor that can say so refuses every other kind of event: a
    // fold is announced by one variant and nothing else in the stream stands in
    // for it.
    let store = store();
    let run_id = store.start_run("fold it", "openrouter").expect("a run");
    assert_eq!(
        Said::folded(
            &store,
            &RunEvent::new(
                run_id,
                3,
                EventKind::CacheMarked {
                    through_step: 3,
                    prefix_bytes: 8_412,
                },
            )
        ),
        None,
        "only `Compacted` says a fold happened",
    );

    let said = Said::folded(&store, &compacted(run_id, 12, 19_400, 5_100)).expect("the fold");
    assert!(said.is_fold());
    let line = said.line(dash());
    assert!(line.contains("folded"), "{line}");
    // The two token figures are the point of the event — a fold that bought
    // nothing must be visible as such, so both ends are on the line.
    assert!(line.contains("19.4k"), "{line}");
    assert!(line.contains("5.1k"), "{line}");
}

// --- F8: the short conversation ------------------------------------------------

/// **The test that kills the sabotage.**
///
/// Sabotage: report a fold as soon as the request is accepted — `Said::asked`
/// answering `Folded` instead of `Sent`, or its sentence borrowing the past
/// tense. On every long conversation that is indistinguishable from working code,
/// because the event arrives a step later and says the same thing. On a
/// conversation shorter than `Compaction::keep_recent` there is no prefix a
/// paragraph could stand in for, io-harness folds nothing, no `Compacted` event
/// is ever emitted — and the sabotaged interface has announced a fold that the
/// harness's own event never claimed.
///
/// So the arm is asserted end to end: the request is accepted, nothing is
/// observed, and what the operator reads says there was nothing to fold and that
/// the request is gone. **The request is spent either way**, which is why the
/// sentence has to say so — an operator who thinks it is still queued waits for a
/// fold that is never coming.
#[test]
fn f8_a_short_conversation_reports_nothing_to_fold() {
    let accepted = Said::asked(folding(), true);
    assert_eq!(accepted, Said::Sent);
    assert!(
        !accepted.is_fold(),
        "accepting a request is not observing a fold",
    );
    let asked = accepted.line(dash());
    assert!(
        !asked.contains("folded"),
        "the request's own line claims a fold: {asked}",
    );

    // The turn ran to its end and the event never came.
    let said = Said::unfolded(folding(), false);
    assert!(!said.is_fold());
    let line = said.line(dash());
    assert!(line.contains("nothing to fold"), "{line}");
    assert!(!line.contains("folded"), "{line}");
    assert!(
        line.contains("spent"),
        "the operator is not told the request is gone: {line}",
    );
    // The number a fold keeps whole, so the sentence says why nothing folded
    // rather than only that nothing did.
    assert!(line.contains('8'), "{line}");
}

/// The other condition nothing can predict: a fold that lost to the interrupt
/// sent before the same step boundary. Reported as what it is, and not as a fold.
#[test]
fn f8_an_interrupted_turn_says_the_request_went_with_it() {
    let said = Said::unfolded(folding(), true);
    assert!(!said.is_fold());
    let line = said.line(dash());
    assert!(line.contains("stopped"), "{line}");
    assert!(!line.contains("folded"), "{line}");
}

// --- F8: folding off -----------------------------------------------------------

/// **The one condition of the four that is known in advance**, and the only one
/// under which nothing is sent at all.
///
/// `Compaction { at_share: 1.0, .. }` never folds and no trigger reverses that —
/// off is a setting rather than an absence — so a request made under it would be
/// spent for nothing and reported as pending forever. Predicted from the
/// contract's own setting, before anything leaves the process, on both arms:
/// whether a turn is running does not change what an off setting does.
#[test]
fn f8_folding_off_is_said_before_anything_is_sent() {
    assert_eq!(Said::asked(never(), true), Said::Off);
    assert_eq!(Said::asked(never(), false), Said::Off);

    let line = Said::Off.line(dash());
    assert!(line.contains("off"), "{line}");
    assert!(
        line.contains("nothing was sent"),
        "the operator is not told the request never left: {line}",
    );
    assert!(!line.contains("folded"), "{line}");

    // A threshold a config file could produce by accident rather than on purpose
    // is off too, and is answered the same way: io-harness reads a non-finite or
    // negative share as never folding, and an interface that read it as "fold
    // always" would send a request into a run that cannot honour it.
    assert_eq!(
        Said::asked(
            Compaction {
                at_share: f32::NAN,
                ..Compaction::default()
            },
            true
        ),
        Said::Off,
    );
}

// --- F8: what the summary replaced ---------------------------------------------

/// The event carries a step and two token figures and no text; the run's own
/// `summaries` row carries how many observations from the front of the ledger the
/// paragraph stands in for. The report is both, because "folded" on its own does
/// not tell an operator what left the model's window.
///
/// **And nothing was destroyed to produce it.** A fold deletes nothing from the
/// store — the observations the row counts are still readable — which is what
/// makes naming them safe rather than a report of a loss.
#[test]
fn f8_the_summary_names_what_it_replaced() {
    let store = store();
    let run_id = store
        .start_run("port the parser", "openrouter")
        .expect("a run");
    store
        .put_summary(run_id, 12, 40, "Read the lexer, kept the token enum.", 11)
        .expect("a summary");

    let said = Said::folded(&store, &compacted(run_id, 12, 19_400, 5_100)).expect("the fold");
    assert_eq!(
        said,
        Said::Folded {
            through_step: 12,
            before: 19_400,
            after: 5_100,
            replaced: Some(40),
        },
    );
    let line = said.line(dash());
    assert!(line.contains("40 observations"), "{line}");
    assert!(line.contains("12"), "{line}");

    // A fold whose row is not readable is still a fold: the event is the fact and
    // the row is the detail, so what is lost is one number rather than the report.
    let other = Said::folded(&store, &compacted(run_id, 30, 21_000, 6_000)).expect("the fold");
    assert_eq!(
        other,
        Said::Folded {
            through_step: 30,
            before: 21_000,
            after: 6_000,
            replaced: None,
        },
    );
    assert!(other.line(dash()).contains("folded"));
}

// --- F8: the command -----------------------------------------------------------

/// It is listed, it is grouped with the other words said to a turn, and it
/// resolves to an action of its own rather than to the unknown-command print.
#[test]
fn f8_compact_is_a_command() {
    assert!(
        COMMANDS.iter().any(|(name, _)| *name == "/compact"),
        "a command nobody can find in `/help` or the palette",
    );
    assert_eq!(
        commands::group_of("/compact"),
        Some(commands::Group::Turn),
        "it changes what this turn does, so it belongs with `/steer`",
    );
    assert_eq!(
        commands::parse("compact", &Keys::default(), &DARK),
        Action::Compact,
    );
    // One spelling. `/fold` is io-harness's own word and nobody has typed it at a
    // prompt, and an alias that worked at an idle prompt while the driver's
    // mid-turn arm matched only `compact` would be worse than no alias at all.
    assert!(matches!(
        commands::parse("fold", &Keys::default(), &DARK),
        Action::Print(_)
    ));
}

// --- F8: the driver ------------------------------------------------------------

/// **Both triggers are actually wired, which nothing else here can see.**
///
/// Nothing under `tests/` links the binary, so the driver's decisions are ones no
/// test can drive — this file reads `src/main.rs` instead, the instrument
/// `tests/steer.rs`, `tests/contract.rs` and `tests/structure.rs` already use for
/// exactly that reason.
///
/// Two calls and they are not interchangeable: `Steer::fold` is read at a running
/// turn's next step boundary, and `TaskContract::fold_now` at a turn's first step
/// before it assembles its first request. An implementation with only the first
/// leaves `/compact` doing nothing at an idle prompt, which is where an operator
/// who has just finished a long thread actually types it.
#[test]
fn f8_the_driver_reaches_both_triggers() {
    let squashed: String = driver().chars().filter(|c| !c.is_whitespace()).collect();

    assert!(
        squashed.contains("steer.fold()"),
        "the mid-turn request is never sent, so `/compact` waits for a turn that has already \
         started",
    );
    assert!(
        squashed.contains("with_fold_now"),
        "the idle request never reaches a contract, so `/compact` at a prompt does nothing",
    );
}
