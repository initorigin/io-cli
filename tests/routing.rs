//! F3 and F4 — what `[app.io-cli.routing]` puts on the contract, and what a
//! contained session is told about it.
//!
//! F3 is the section reaching an `io_harness::Routing`: the keys an operator
//! types, the two rules they name, and the run's behaviour under them. F4 is the
//! disclosure that those rules do not fire for a contained turn.
//!
//! Two things here are asserted the way the implementation would be wrong rather
//! than the way it is right.
//!
//! **The behaviour is asserted through `io_harness::Routing::model_for` and never
//! re-implemented.** The precedence rule — escalating beats downshifting when
//! both conditions hold — belongs to io-harness, and a test that computed the
//! expected model from io-cli's own reading of the rules would pass against an
//! io-cli that had quietly disagreed with the harness about which model a run
//! asks. Asking the harness's own method is the only assertion that catches that.
//!
//! **The empty cases are asserted separately from the absent ones.** A section
//! that is present and names no rule and a section that was never written must
//! both leave the contract's routing unset; building a `Routing::default()` for
//! the first is the mistake that looks harmless, so it gets its own test.

use io_cli::routing::{self, Downshift, Escalation, Settings};

/// The models the fixtures route between. Names, not real models: nothing here
/// connects to anything, and a name that looks like a real one invites a reader
/// to think it matters which.
const STRONGER: &str = "vendor/stronger-model";
const CHEAPER: &str = "vendor/cheaper-model";

/// A section naming both rules.
fn both() -> Settings {
    Settings {
        escalate_after: Some(Escalation {
            failures: Some(3),
            model: Some(STRONGER.to_string()),
        }),
        downshift_under: Some(Downshift {
            bytes: Some(2_000),
            model: Some(CHEAPER.to_string()),
        }),
    }
}

// ---------------------------------------------------------------------------
// F3 — the section becomes a Routing
// ---------------------------------------------------------------------------

#[test]
fn a_section_naming_both_rules_carries_both_exactly_as_written() {
    let routing = routing::routing(&both())
        .expect("two rules are obeyable")
        .expect("two rules are a routing");
    assert_eq!(routing.escalate_after, Some((3, STRONGER.to_string())));
    assert_eq!(routing.downshift_under, Some((2_000, CHEAPER.to_string())));
}

#[test]
fn a_section_that_names_no_rule_puts_no_routing_on_the_contract() {
    assert_eq!(routing::routing(&Settings::default()), Ok(None));
}

#[test]
fn a_present_but_empty_section_puts_no_routing_on_the_contract() {
    let empty: Settings = toml::from_str("").expect("an empty routing section parses");
    assert_eq!(empty, Settings::default());
    assert_eq!(
        routing::routing(&empty),
        Ok(None),
        "an empty Routing is a value where there was none, and it changes the contract"
    );
}

#[test]
fn a_section_naming_only_escalation_carries_only_escalation() {
    let settings = Settings {
        downshift_under: None,
        ..both()
    };
    let routing = routing::routing(&settings)
        .expect("one rule is obeyable")
        .expect("one rule is a routing");
    assert_eq!(routing.escalate_after, Some((3, STRONGER.to_string())));
    assert_eq!(routing.downshift_under, None);
}

#[test]
fn a_section_naming_only_downshift_carries_only_downshift() {
    let settings = Settings {
        escalate_after: None,
        ..both()
    };
    let routing = routing::routing(&settings)
        .expect("one rule is obeyable")
        .expect("one rule is a routing");
    assert_eq!(routing.escalate_after, None);
    assert_eq!(routing.downshift_under, Some((2_000, CHEAPER.to_string())));
}

// ---------------------------------------------------------------------------
// F3 — what the harness then does with it
// ---------------------------------------------------------------------------

#[test]
fn under_the_failure_threshold_a_small_run_is_asked_of_the_cheaper_model() {
    let routing = routing::routing(&both())
        .expect("two rules are obeyable")
        .expect("two rules are a routing");
    assert_eq!(routing.model_for(0, 0), Some(CHEAPER));
    assert_eq!(routing.model_for(2, 1_999), Some(CHEAPER));
}

#[test]
fn escalating_beats_downshifting_at_and_above_the_threshold() {
    let routing = routing::routing(&both())
        .expect("two rules are obeyable")
        .expect("two rules are a routing");
    // Both conditions hold: three consecutive failures, and fewer bytes written
    // than the downshift bound. io-harness answers with the stronger model.
    assert_eq!(routing.model_for(3, 0), Some(STRONGER));
    assert_eq!(routing.model_for(9, 1_999), Some(STRONGER));
}

#[test]
fn neither_condition_met_leaves_the_requests_model_alone() {
    let routing = routing::routing(&both())
        .expect("two rules are obeyable")
        .expect("two rules are a routing");
    assert_eq!(routing.model_for(2, 2_000), None);
    assert_eq!(routing.model_for(0, 10_000), None);
}

// ---------------------------------------------------------------------------
// F4 — the containment disclosure
// ---------------------------------------------------------------------------

#[test]
fn a_contained_session_with_rules_is_told_they_will_not_fire() {
    let notice = routing::inert_under_containment(&both(), true)
        .expect("contained plus rules is the one case that discloses");
    assert!(
        notice.contains("[app.io-cli.containment]"),
        "the notice must name the section that caused it: {notice}"
    );
    assert!(
        notice.contains("/contain off"),
        "the notice must name the way out: {notice}"
    );
}

#[test]
fn an_uncontained_session_with_rules_is_told_nothing() {
    assert_eq!(
        routing::inert_under_containment(&both(), false),
        None,
        "the majority of operators have no containment, and their rules work"
    );
}

#[test]
fn a_contained_session_with_no_rules_is_told_nothing() {
    assert_eq!(
        routing::inert_under_containment(&Settings::default(), true),
        None
    );
}

#[test]
fn an_uncontained_session_with_no_rules_is_told_nothing() {
    assert_eq!(
        routing::inert_under_containment(&Settings::default(), false),
        None
    );
}

// ---------------------------------------------------------------------------
// F3 — the sentence a surface draws
// ---------------------------------------------------------------------------

#[test]
fn the_description_states_escalation_before_downshift_and_names_both_rules() {
    let sentence = routing::describe(&both()).expect("two rules describe as something");
    let escalation = sentence
        .find(STRONGER)
        .expect("the stronger model is named");
    let downshift = sentence.find(CHEAPER).expect("the cheaper model is named");
    assert!(
        escalation < downshift,
        "escalation is the rule that overrides the other and is stated first: {sentence}"
    );
    assert!(sentence.contains("3 consecutive failed gate attempts"));
    assert!(sentence.contains("2000 bytes"));
    assert!(
        sentence.contains("wins over downshifting"),
        "io-harness's precedence rule has to be in the sentence: {sentence}"
    );
    assert!(
        sentence.contains("happens once"),
        "escalation not coming back down is the other half: {sentence}"
    );
}

#[test]
fn a_single_failure_threshold_is_described_in_the_singular() {
    let settings = Settings {
        escalate_after: Some(Escalation {
            failures: Some(1),
            model: Some(STRONGER.to_string()),
        }),
        downshift_under: None,
    };
    let sentence = routing::describe(&settings).expect("one rule describes as something");
    assert!(
        sentence.contains("1 consecutive failed gate attempt"),
        "the count is stated: {sentence}"
    );
    assert!(
        !sentence.contains("attempts"),
        "one attempt is singular: {sentence}"
    );
}

#[test]
fn a_section_with_no_rules_describes_as_nothing() {
    assert_eq!(routing::describe(&Settings::default()), None);
}

// ---------------------------------------------------------------------------
// F3 — the keys an operator actually types
// ---------------------------------------------------------------------------

/// The spellings are pinned by a test because they are the operator's interface.
/// A field renamed in `src/routing.rs` compiles, passes every other test in this
/// file, and silently stops reading a file somebody already wrote.
#[test]
fn the_section_deserializes_from_the_toml_an_operator_writes() {
    let settings: Settings = toml::from_str(
        r#"
[escalate_after]
failures = 3
model = "vendor/stronger-model"

[downshift_under]
bytes = 2000
model = "vendor/cheaper-model"
"#,
    )
    .expect("the documented section parses");
    assert_eq!(settings, both());
}

/// **A half rule parses and is refused, and the difference is the whole finding.**
///
/// These keys were required, which looked stricter and was far worse. A required
/// field is a *deserialization* failure, so `failures = 3` with no `model` did not
/// fail the rule — it failed `CliSettings`, and `settings::stored` then answered
/// `None` for the whole `[app.io-cli]` section. The theme, the keys, the ceilings,
/// the capabilities and the **verification gate** all silently reverted to their
/// defaults, because `contract::criterion_for` gives up on that same `None`. A gate
/// that stops gating without saying so is the most expensive failure this crate
/// has, and it was one missing line in a configuration file away.
///
/// Worse, `/config` writes exactly one key per invocation, so **every** path
/// through that surface to a routing rule passed through this state.
///
/// Found by the adversarial review. Sabotage: make either key required again —
/// under which this test fails at the `expect`, because the section no longer
/// parses at all.
#[test]
fn a_rule_missing_half_of_itself_parses_and_is_refused_by_name() {
    let settings: Settings = toml::from_str("[escalate_after]\nfailures = 3\n")
        .expect("half a rule must still parse, or it takes all of [app.io-cli] with it");

    assert_eq!(
        routing::routing(&settings),
        Err(routing::Refusal::HalfARule {
            rule: "escalate_after",
            missing: "model",
        }),
    );
    assert!(
        routing::notice(&settings).is_some_and(|said| said.contains("half a rule")),
        "the operator hears which key is missing",
    );
}

/// **The three values that are writable and disastrous.**
///
/// io-harness obeys the thresholds literally (`contract.rs:1811-1822`), and none
/// of these is a shape TOML can refuse:
///
/// * `failures = 0` satisfies `consecutive_gate_failures >= 0` at the first request
///   of every run, so the escalation model is used unconditionally and the
///   downshift — checked second — is never reached. An operator writing it means
///   "escalate readily" and gets "never use the model I configured".
/// * `bytes = 0` can never be true, so the rule is permanently inert.
/// * An empty model sends every request of the run with no model id.
///
/// Found by the adversarial review, which noted that this file already named the
/// empty-model outcome as the thing being avoided and the code then accepted it.
#[test]
fn a_threshold_that_could_only_misfire_is_refused() {
    let escalating_at_zero = Settings {
        escalate_after: Some(routing::Escalation {
            failures: Some(0),
            model: Some(STRONGER.to_string()),
        }),
        downshift_under: None,
    };
    assert_eq!(
        routing::routing(&escalating_at_zero),
        Err(routing::Refusal::EscalatesBeforeAnythingFailed),
    );

    let never_downshifts = Settings {
        escalate_after: None,
        downshift_under: Some(routing::Downshift {
            bytes: Some(0),
            model: Some(CHEAPER.to_string()),
        }),
    };
    assert_eq!(
        routing::routing(&never_downshifts),
        Err(routing::Refusal::NeverDownshifts),
    );

    let nameless = Settings {
        escalate_after: Some(routing::Escalation {
            failures: Some(3),
            model: Some("   ".to_string()),
        }),
        downshift_under: None,
    };
    assert_eq!(
        routing::routing(&nameless),
        Err(routing::Refusal::NoModel {
            rule: "escalate_after"
        }),
    );
}

/// A refusal leaves the run unrouted, and every refusal has a sentence.
///
/// The pair of `contract::gate_notice`: a section that is plainly in the
/// operator's file and is not doing anything has to say why, or the surface that
/// lists it is lying by omission.
#[test]
fn every_refusal_says_which_key_is_wrong_and_that_the_turn_is_not_routed() {
    for settings in [
        Settings {
            escalate_after: Some(routing::Escalation {
                failures: Some(0),
                model: Some(STRONGER.to_string()),
            }),
            downshift_under: None,
        },
        Settings {
            escalate_after: None,
            downshift_under: Some(routing::Downshift {
                bytes: Some(0),
                model: Some(CHEAPER.to_string()),
            }),
        },
        Settings {
            escalate_after: Some(routing::Escalation {
                failures: Some(3),
                model: None,
            }),
            downshift_under: None,
        },
    ] {
        let said = routing::notice(&settings).expect("a refused section says why");
        assert!(
            said.contains("app.io-cli.routing"),
            "a refusal names the section the operator has to open: {said}",
        );
        assert!(
            said.contains("not routed"),
            "a refusal says what the run does instead: {said}",
        );
    }

    assert_eq!(
        routing::notice(&both()),
        None,
        "an obeyable section has nothing to explain",
    );
}
