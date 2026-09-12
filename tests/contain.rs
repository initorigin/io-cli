//! F1 — containment is opt-in, and its absence changes nothing.
//! F2 — a contained turn is the one that reaches the spawn loop.
//! F7 — `Ctrl+C` ends a contained turn through the observer, and says so.
//!
//! The mode is a property of the *turn*, not of the interface, and that is what
//! these assert. Since 0.17.0 the switch decides the fan-out and nothing else:
//! `turn_contained_bounded_steered` and `turn_bounded_steered` both take a
//! caller's containment-or-not, a contract and a `SteerInbox`, so a contained
//! turn can be steered exactly as an ordinary one can. Every claim below is about
//! which of the two turns this session takes and what it told the operator about
//! it — never about what it gave up to get there, because it gives up nothing.

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
    // **The footer, since 0.13.1.** A sentence about stopping answers the key
    // that was just pressed and is not part of the conversation; it used to be
    // committed into the scrollback, where three of them stacked up over one
    // decision and stayed there for the life of the terminal.
    app.status
        .notice
        .as_ref()
        .map(|(_, text)| text.clone())
        .unwrap_or_default()
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

/// **F5 — the notice says what containment actually decides, and nothing else.**
///
/// The positive half is the caps, which are the operator's own numbers and the
/// reason the mode has a configuration key at all. The negative half is the two
/// claims 0.11.0 falsified and this notice went on making for a release: that the
/// mode is what grants skills, MCP, language servers and a browser, and that it
/// costs a mid-turn steer. Both turns carry a contract since 0.11.0 and both
/// carry a `SteerInbox` since 0.17.0, so the first was selling capabilities the
/// session already had and the second was charging for something a contained turn
/// keeps. The 0.11.0 phrase about steering stays in the absent list below for
/// that reason — it was wrong when it was charged for, and it is wrong twice over
/// now — and it is written there as a literal rather than here in prose, so that
/// `tests/docs.rs`'s comment sweep reads it as the assertion it is.
///
/// Sabotage: restore the 0.11.0 wording — under which only this test fails, and
/// it fails on the absences rather than the caps, which is the half that decides
/// whether an operator turns on a fan-out they did not want.
#[test]
fn f5_entering_contained_mode_says_what_it_decides() {
    let caps = Containment::new(12, 4, 2, 200_000);
    let notice = settings::contained_notice(&caps, "-");
    for expected in [
        "12 agents",
        "4 at once",
        "2 deep",
        "200000 tokens",
        "only turn that can fan out",
        "Ctrl+C ends either one",
    ] {
        assert!(
            notice.contains(expected),
            "the disclosure should name {expected:?}: {notice}"
        );
    }
    for gone in [
        "cannot be steered",
        "[run] budget",
        "[sandbox]",
        "the turn that carries a contract",
        "a plan is decided here",
        "questions are answered here",
    ] {
        assert!(
            !notice.contains(gone),
            "0.11.0 gave every turn a contract, so {gone:?} is no longer something this mode \
             decides: {notice}"
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

/// F7 — no turn is cancelled through the observer until the flag is set.
///
/// The negative half, and the one the sabotage arm attacks: a bridge that
/// answered `Cancel` because a turn was running would cancel every turn, contained
/// or not, the moment its first event arrived.
#[test]
fn f7_the_bridge_continues_until_it_is_told_to_cancel() {
    let (observer, mut events) = bridge::channel();
    let canceller = observer.canceller();
    let event = RunEvent::new(1, 1, EventKind::Stalled);

    assert_eq!(observer.event(&event), Flow::Continue);
    assert!(
        events.try_recv().is_ok(),
        "the event still reaches the interface"
    );

    canceller.store(true, Ordering::Relaxed);
    assert_eq!(observer.event(&event), Flow::Cancel);
    // Cancelling does not stop reporting: the events between the flag and the
    // boundary io-harness honours it at are exactly the ones showing a fleet
    // draining, and dropping them would blank the screen at the moment an
    // operator is waiting to see something happen.
    assert!(
        events.try_recv().is_ok(),
        "events keep flowing while it ends"
    );
}

/// F7 — the sentence `Ctrl+C` prints depends on which kind of turn is running.
#[test]
fn f7_the_interrupt_says_where_the_turn_will_stop() {
    let mut app = App::new(DARK, "a-model");
    app.started();
    // A step's worth of progress, because a turn that has done nothing is simply
    // undone now and says nothing at all — see `App::undoable`.
    app.status.steps = Some(1);
    assert_eq!(app.key(ctrl('c')), Command::Interrupt);
    // Both kinds of turn are steered since 0.17.0, so the word for this one is
    // *uncontained* — the sentence differs on where it can stop, not on whether
    // the operator can speak to it.
    let uncontained = said(&mut app);
    assert!(
        uncontained.contains("next step"),
        "an uncontained turn stops at a step boundary: {uncontained}"
    );

    let mut app = App::new(DARK, "a-model");
    app.started();
    app.status.steps = Some(1);
    app.contained = true;
    assert_eq!(app.key(ctrl('c')), Command::Interrupt);
    let contained = said(&mut app);
    assert!(
        contained.contains("no child is in flight"),
        "a contained turn stops where no child is in flight: {contained}"
    );
}

/// **F10 — `/contain on` with nothing configured offers a fan-out rather than
/// four key names.**
///
/// The arm used to name `max_total_agents`, `max_concurrent_agents`, `max_depth`
/// and `max_total_tokens` and stop. That is technically correct and it asks an
/// operator to choose a token ceiling for a mode they have not tried, out of a
/// documentation page they are not reading.
///
/// **Every number is on the row that acts**, spelled out rather than summarised.
/// This writes to their configuration file and turns on a mode that spends tokens
/// across a tree of agents; a row reading "write a default" would be asking them
/// to agree to figures they were never shown.
#[test]
fn f10_the_offer_shows_every_number_it_would_write() {
    let caps = io_cli::settings::offered_containment();
    let (title, rows) = io_cli::settings::containment_offer(&caps);

    assert!(
        title.contains("fan-out"),
        "the question has to name what is being turned on: {title}",
    );
    assert_eq!(
        rows[0].label,
        io_cli::store::LEAVE_IT,
        "every confirmation in this product declines at row 0",
    );
    assert!(!io_cli::store::acts(0));
    assert!(io_cli::store::acts(1));

    let acting = &rows[1].label;
    for number in [
        caps.max_total_agents.to_string(),
        caps.max_concurrent_agents.to_string(),
        caps.max_depth.to_string(),
        caps.max_total_tokens.to_string(),
    ] {
        assert!(
            acting.contains(&number),
            "the row that writes the section does not show `{number}`, so the \
             operator is agreeing to a figure they were never shown: {acting}",
        );
    }
}

/// **F10 — the offered caps are small, and the one that must exist does.**
///
/// A fan-out with no aggregate token ceiling is the one shape of this feature
/// that can spend without a bound anybody chose, so an offer without one would be
/// worse than no offer. The rest are small on purpose: enough for a fan-out to be
/// worth having, small enough that a mistake is legible.
///
/// The two io-harness leaves optional stay unset, and for different reasons — a
/// cost ceiling is documented as reserved and not enforced, so offering a number
/// that does nothing is worse than offering none; a duration is a real ceiling and
/// is a property of the work rather than something `io` can guess.
#[test]
fn f10_the_offered_caps_bound_the_spend_and_nothing_it_cannot_know() {
    let caps = io_cli::settings::offered_containment();

    assert!(
        caps.max_total_tokens > 0,
        "a fan-out with no aggregate token ceiling can spend without a bound \
         anybody chose, and this is the offer that would have set it",
    );
    assert!(caps.max_concurrent_agents >= 2, "or it is not a fan-out");
    assert!(
        caps.max_concurrent_agents <= caps.max_total_agents,
        "more at once than may exist at all is not a configuration, it is a typo",
    );
    assert!(
        caps.max_depth >= 1,
        "children are the whole point; zero depth is the mode switched off",
    );
    assert!(
        caps.max_depth <= 2,
        "depth is where a tree stops being something an operator can hold in \
         their head, and every tier multiplies how many may be working at once",
    );
    assert!(
        caps.max_total_cost.is_none(),
        "io-harness documents the cost ceiling as reserved and not enforced, so \
         a number here would be one that does nothing",
    );
    assert!(
        caps.max_total_duration.is_none(),
        "how long a fan-out may take is a property of the work",
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

/// **F10 — the two doors onto `[app.io-cli.containment]` write the same section.**
///
/// There are two, and they could not share an implementation. The `io setup`
/// wizard's fan-out step renders a whole file — `settings::render`, serializing
/// `CliSettings` — because at that point there is no file to splice into. `/contain
/// on` meets an operator who already has one, so it goes through
/// `edit::Edit::set`, which takes a value as text. A whole-file serializer and a
/// one-value splice is two spellings of the same four ceilings, and two spellings
/// drift: the day somebody adds a fifth cap to `offered_containment`, the door
/// that serializes picks it up for free and the door that formats a string does
/// not, and an operator who used the wrong door gets a fan-out with a cap they
/// were shown and never got.
///
/// So both are rendered here and parsed back, and the *parsed* sections are
/// compared rather than the text — the two forms are legitimately different text
/// (a whole file against an inline table) and identical data, which is exactly the
/// claim worth asserting.
#[test]
fn f10_both_doors_onto_the_containment_section_write_the_same_keys() {
    let caps = io_cli::settings::offered_containment();

    // The wizard's door: a whole file, with the step's answer carried through.
    let spec = io_harness::ProviderSpec::Anthropic {
        model: "claude-sonnet-4".into(),
        api_key: None,
    };
    let file = io_cli::settings::render(
        &spec,
        io_cli::settings::Posture::Workspace,
        "dark",
        Some(caps.clone()),
    )
    .expect("the wizard's file renders");
    let file: toml::Value = toml::from_str(&file).expect("and parses back");
    let wizard = file
        .get("app")
        .and_then(|app| app.get("io-cli"))
        .and_then(|ours| ours.get("containment"))
        .unwrap_or_else(|| panic!("the wizard accepted the caps and wrote no section: {file:#?}"));

    // `/contain on`'s door: one inline table, spliced into a file it did not write.
    let inline = io_cli::settings::containment_inline(&caps);
    let spliced: toml::Value =
        toml::from_str(&format!("containment = {inline}")).expect("the inline table parses");
    let command = spliced.get("containment").expect("it is the value written");

    assert_eq!(
        wizard, command,
        "the two doors onto [app.io-cli.containment] write different sections, so \
         which one an operator used decides what caps they got",
    );

    // And the section is the offer, not a subset of it: a door that wrote three of
    // the four would still agree with the other door that wrote three.
    let table = wizard.as_table().expect("a table");
    assert_eq!(
        table.len(),
        4,
        "the section is the four ceilings `offered_containment` sets and the two \
         io-harness leaves unset: {table:#?}",
    );
    for (key, value) in [
        ("max_total_agents", i64::from(caps.max_total_agents)),
        (
            "max_concurrent_agents",
            i64::from(caps.max_concurrent_agents),
        ),
        ("max_depth", i64::from(caps.max_depth)),
        ("max_total_tokens", caps.max_total_tokens as i64),
    ] {
        assert_eq!(
            table.get(key).and_then(toml::Value::as_integer),
            Some(value),
            "`{key}` is not what the offer showed: {table:#?}",
        );
    }
}

/// **F10 — declining the wizard's fan-out step writes no section at all.**
///
/// The default answer is the one that writes nothing, and that has to mean
/// *nothing*: a declined install must produce the file 0.40.0 produced, not one
/// carrying an empty `[app.io-cli.containment]` that reads as a fan-out configured
/// with no ceilings. An empty table is the worst of the three outcomes — it is the
/// only one that could turn the mode on without a bound anybody chose.
#[test]
fn f10_declining_the_fanout_step_writes_no_containment_at_all() {
    let spec = io_harness::ProviderSpec::Anthropic {
        model: "claude-sonnet-4".into(),
        api_key: None,
    };
    let file = io_cli::settings::render(&spec, io_cli::settings::Posture::Workspace, "dark", None)
        .expect("the declined file renders");

    assert!(
        !file.contains("containment"),
        "declining wrote a containment section, and an empty one is a fan-out with \
         no ceiling: {file}",
    );
}
