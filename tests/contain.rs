//! F1 — containment is opt-in, and its absence changes nothing.
//! F2 — a contained turn is the one that reaches the spawn loop.
//! F7 — `Ctrl+C` ends a contained turn through the observer, and says so.
//!
//! The mode is a property of the *turn*, not of the interface, and that is what
//! these assert. io-harness offers no session entry point that takes a caller's
//! containment and a steer inbox together, so a session either fans out or is
//! steerable; every claim below is about which of those this session is in and
//! what it told the operator about it.

mod support;

use std::sync::atomic::Ordering;

use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use io_cli::app::{App, Command};
use io_cli::bridge;
use io_cli::commands::{self, Action};
use io_cli::settings;
use io_cli::theme::DARK;
use io_harness::{Config, Containment, EventKind, Flow, Observer, RunEvent};

fn ctrl(code: char) -> KeyEvent {
    KeyEvent::new(KeyCode::Char(code), KeyModifiers::CONTROL)
}

/// Everything the app has queued for the scrollback, as one string.
fn said(app: &mut App) -> String {
    app.take_pending()
        .iter()
        .flat_map(|line| line.spans.iter())
        .map(|span| span.content.to_string())
        .collect::<Vec<_>>()
        .join(" ")
}

/// F1 — the four caps come out of `[app.io-cli.containment]` as io-harness's own
/// type, including through the alias the crate keeps for the pre-0.32.0 name.
#[test]
fn f1_the_containment_table_is_read_as_the_harness_type() {
    let config = Config::from_toml(
        r#"
[app.io-cli.containment]
max_total_agents = 12
max_concurrent = 4
max_depth = 2
max_total_tokens = 200000
"#,
    )
    .expect("the file parses");
    let (stored, complaint) = settings::stored(&config);
    assert_eq!(complaint, None);
    let caps = settings::containment(stored.as_ref()).expect("the table is there");
    assert_eq!(caps.max_total_agents, 12);
    // The alias is the point of this line: a file written before 0.32.0 spells
    // the concurrency cap `max_concurrent`, and io-harness still reads it.
    assert_eq!(caps.max_concurrent_agents, 4);
    assert_eq!(caps.max_depth, 2);
    assert_eq!(caps.max_total_tokens, 200_000);
    // Documented inert by the crate: there is no price telemetry, so nothing
    // here may render a spend as money.
    assert_eq!(caps.max_total_cost, None);
}

/// F1 — no table means no fan-out, and the session says nothing about a mode it
/// is not in.
#[test]
fn f1_a_configuration_without_the_table_has_no_containment() {
    let config = Config::from_toml("[app.io-cli]\ntheme = \"dark\"\n").expect("the file parses");
    let (stored, complaint) = settings::stored(&config);
    assert_eq!(complaint, None);
    assert!(settings::containment(stored.as_ref()).is_none());
    assert!(settings::containment(None).is_none());
}

/// F1 — a malformed table is disclosed with io-harness's own message rather than
/// silently defaulted.
///
/// The shape 0.6.0 paid for once with `Config::app`'s `unwrap_or_default`: a
/// section that fails to read must not quietly revert every setting in it. Here
/// it matters more than it did there, because the default is *no fan-out* and an
/// operator who set caps would be running turns that cannot spawn with nothing
/// said.
#[test]
fn f1_a_malformed_table_is_disclosed_and_not_defaulted() {
    let config = Config::from_toml(
        r#"
[app.io-cli.containment]
max_total_agents = "twelve"
max_concurrent_agents = 4
max_depth = 2
max_total_tokens = 200000
"#,
    )
    .expect("the file parses as TOML; it is the section that does not");
    let (stored, complaint) = settings::stored(&config);
    let complaint = complaint.expect("a section that cannot be read is a complaint");
    assert!(
        complaint.contains("default settings"),
        "the notice says the session is running on defaults: {complaint}"
    );
    assert!(settings::containment(stored.as_ref()).is_none());
}

/// F1 — the disclosure names the caps and everything a contained turn gives up.
#[test]
fn f1_entering_contained_mode_says_what_it_costs() {
    let caps = Containment::new(12, 4, 2, 200_000);
    let notice = settings::contained_notice(&caps, "-");
    for expected in [
        "12 agents",
        "4 at once",
        "2 deep",
        "200000 tokens",
        "cannot be steered",
        "[run] budget",
        "[sandbox]",
        "Ctrl+C still ends it",
    ] {
        assert!(
            notice.contains(expected),
            "the disclosure should name {expected:?}: {notice}"
        );
    }
}

/// F1 — `/contain` reports, and never guesses.
#[test]
fn f1_contain_parses_as_a_question_or_an_answer() {
    let keys = io_cli::keys::Keys::default();
    assert_eq!(
        commands::parse("contain", &keys, &DARK),
        Action::Contain(None)
    );
    assert_eq!(
        commands::parse("contain on", &keys, &DARK),
        Action::Contain(Some(true))
    );
    assert_eq!(
        commands::parse("contain off", &keys, &DARK),
        Action::Contain(Some(false))
    );
    // The word an operator who has read the configuration key would reach for.
    assert_eq!(
        commands::parse("containment on", &keys, &DARK),
        Action::Contain(Some(true))
    );
}

/// F7 — a steered turn is never cancelled through the observer.
///
/// The negative half, and the one the sabotage arm attacks: a bridge that
/// answered `Cancel` because a turn was running would cancel every steered turn
/// the moment its first event arrived.
#[test]
fn f7_the_bridge_continues_until_it_is_told_to_cancel() {
    let (observer, mut events) = bridge::channel();
    let canceller = observer.canceller();
    let event = RunEvent::new(1, 1, EventKind::Stalled);

    assert_eq!(observer.event(&event), Flow::Continue);
    assert!(events.try_recv().is_ok(), "the event still reaches the interface");

    canceller.store(true, Ordering::Relaxed);
    assert_eq!(observer.event(&event), Flow::Cancel);
    // Cancelling does not stop reporting: the events between the flag and the
    // boundary io-harness honours it at are exactly the ones showing a fleet
    // draining, and dropping them would blank the screen at the moment an
    // operator is waiting to see something happen.
    assert!(events.try_recv().is_ok(), "events keep flowing while it ends");
}

/// F7 — the sentence `Ctrl+C` prints depends on which kind of turn is running.
#[test]
fn f7_the_interrupt_says_where_the_turn_will_stop() {
    let mut app = App::new(DARK, "a-model");
    app.started();
    assert_eq!(app.key(ctrl('c')), Command::Interrupt);
    let steered = said(&mut app);
    assert!(
        steered.contains("next step boundary"),
        "a steered turn stops at a step boundary: {steered}"
    );

    let mut app = App::new(DARK, "a-model");
    app.started();
    app.contained = true;
    assert_eq!(app.key(ctrl('c')), Command::Interrupt);
    let contained = said(&mut app);
    assert!(
        contained.contains("no child is in flight"),
        "a contained turn stops where no child is in flight: {contained}"
    );
}

/// F2 — the mode is a fact about the turn, and it does not outlive it.
#[test]
fn f2_the_contained_flag_is_cleared_with_the_turn() {
    let mut app = App::new(DARK, "a-model");
    app.started();
    app.contained = true;
    app.finished();
    assert!(
        !app.contained,
        "an idle session describes no turn, contained or otherwise"
    );
}
