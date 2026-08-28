//! Which model a run asks, once the run has told you something about itself.
//!
//! io-harness has carried the decision since 0.55.0: a `TaskContract` holds one
//! [`io_harness::Routing`], the flat run loop asks it after every step, and the
//! answer replaces the model on the next request. Until this release io-cli
//! supplied none, so every contract it built carried no routing and every run
//! asked one model from the first token to the last.
//!
//! This module is where the operator's answer lives, and it is deliberately thin:
//! [`Settings`] is the `[app.io-cli.routing]` section as written, [`routing`]
//! turns it into the dependency's own value, and the other two functions are what
//! a surface says about it. **No rule is evaluated here.** io-harness owns
//! `Routing::model_for` (`contract.rs:1811`), it is the only code that knows the
//! consecutive-failure count and the byte total, and a second implementation in
//! this crate would be a second answer that drifts from the one the run uses.
//!
//! # Why this section is not deserialized straight into the harness's type
//!
//! `[app.io-cli.containment]` is `Option<io_harness::Containment>` in
//! [`crate::settings::CliSettings`] because that type is `Serialize` and
//! `Deserialize` for exactly that purpose. [`io_harness::Routing`] is not: it
//! derives `Debug, Clone, Default, PartialEq, Eq` and nothing else
//! (`contract.rs:1745`), and it is `#[non_exhaustive]`, so this crate can neither
//! deserialize into it nor write a struct literal for it. Both halves of that
//! matter — a `serde` derive added upstream later would still leave the literal
//! refused, because `#[non_exhaustive]` is a promise about fields yet to exist.
//!
//! So io-cli defines its own `Deserialize` shapes below and builds through the
//! three builder methods, which are the supported way in and which keep working
//! when the harness grows a fourth field.
//!
//! # What an operator writes
//!
//! ```toml
//! [app.io-cli.routing.escalate_after]
//! failures = 3
//! model = "a-stronger-model"
//!
//! [app.io-cli.routing.downshift_under]
//! bytes = 2000
//! model = "a-cheaper-model"
//! ```
//!
//! Each rule is a sub-table rather than a pair of flat keys
//! (`escalate_after_failures`, `escalate_after_model`) because a rule is two
//! numbers that only mean anything together: a threshold with no model and a
//! model with no threshold are both half a rule, and a sub-table makes the pair
//! the unit TOML itself enforces. Neither field carries a `#[serde(default)]`, so
//! half a rule is a parse failure the operator hears about with the key named,
//! rather than a threshold silently defaulting to zero — which would escalate on
//! the very first gate attempt of every turn.
//!
//! # `require_primary` is not offered, and that is a decision
//!
//! [`io_harness::Routing`] has a third field: `require_primary`, which refuses to
//! start when the primary provider says it is unreachable. It is not exposed
//! here. It gates on `Provider::reachable`, a **defaulted** trait method whose
//! body is `async { Ok(true) }` (`provider/mod.rs:1745`), and **no provider in
//! io-harness 0.69 overrides it** — the only other mention of the name in that
//! crate is the doc example demonstrating how one *could*. A key for it would
//! therefore be advertised on a surface, accepted from a file, and permanently
//! inert: an operator would set it, believe an unattended overnight job now
//! refuses to start against a dead endpoint, and get exactly the behaviour they
//! had before. That is worse than the setting not existing. It goes in when a
//! provider answers the question.
//!
//! # Routing does not reach a contained turn
//!
//! `apply_routing` has exactly one call site in io-harness: `run/step.rs:1097`,
//! inside the flat workspace loop. The contained loop takes each agent's model
//! from that agent's own `AgentDef` (`run/tree.rs:638`), and the root is entered
//! with its identity passed as `None` (`run.rs:3959`), which every provider reads
//! as "the model you were built with". So for an operator who has configured
//! `[app.io-cli.containment]`, a routing section parses, reaches the contract,
//! and never fires.
//!
//! Nothing in this crate can fix that — the loop that would have to consult the
//! rules is the dependency's — so what this module owes the operator is the
//! disclosure, and [`inert_under_containment`] is it. It is deliberately narrow:
//! see that function for why warning every operator with a routing section would
//! be worse than warning none.

use serde::{Deserialize, Serialize};

/// The `[app.io-cli.routing]` section exactly as an operator wrote it.
///
/// Two optional rules and nothing else. There is no `enabled` key and no
/// `require_primary` key: a section that names a rule is a section that wants it,
/// and the third field of [`io_harness::Routing`] is inert in io-harness 0.69 for
/// the reasons in the module documentation.
///
/// Not `deny_unknown_fields`, unlike [`crate::gates::Settings`], and the
/// asymmetry is worth stating. That section's failure mode is a typo landing on
/// the default and silently producing no gate at all, which is why it refuses
/// names it does not know. Here the two names are the *tables*, and a mistyped
/// table is a table nobody reads — the section then names no rule, [`routing`]
/// answers `None`, and the run behaves exactly as it did before the operator
/// touched the file. Wrong, but not a run that reports success on work nobody
/// checked. Inside each table the keys are required, so a typo *there* is a parse
/// error with the key named, which is the loud half already.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Settings {
    /// Move up to a stronger model after this many consecutive failed gates.
    #[serde(default)]
    pub escalate_after: Option<Escalation>,
    /// Stay on a cheaper model while the run has written little.
    #[serde(default)]
    pub downshift_under: Option<Downshift>,
}

/// The upward rule: `[app.io-cli.routing.escalate_after]`.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Escalation {
    /// How many *consecutive* failed gate attempts trigger the change.
    ///
    /// Consecutive rather than cumulative, and that is io-harness's counting
    /// rather than a choice made here (`contract.rs:1748`): a run that fails,
    /// recovers, and fails again much later is a run doing hard work, not a run
    /// that needs a bigger model.
    pub failures: u32,
    /// The model asked from then on.
    pub model: String,
}

/// The downward rule: `[app.io-cli.routing.downshift_under]`.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Downshift {
    /// The byte total below which the cheaper model is asked.
    ///
    /// Measured on what the run has already written to disk, not on what it
    /// planned to write — again io-harness's definition (`contract.rs:1759`).
    pub bytes: u64,
    /// The model asked while the run is under that total.
    pub model: String,
}

/// The section as the dependency's own type, ready for the contract, or `None`.
///
/// **`None` for a section that names neither rule, including a section that is
/// present and empty, and that is not tidiness.** `io_harness::Routing::default()`
/// is a perfectly constructible value whose `model_for` always answers `None`, so
/// putting one on the contract looks harmless. It is not the same thing as
/// leaving the contract's routing unset: it is a value where there was absence,
/// it is what the run records and what a later reader of that run sees, and it
/// says the operator asked for routing when they asked for nothing. An operator
/// who writes `[app.io-cli.routing]` and no rule under it gets the contract they
/// would have had without the line.
///
/// Built through the builders rather than by literal because
/// [`io_harness::Routing`] is `#[non_exhaustive]` — see the module
/// documentation. The builders are `#[must_use]` and consuming, so the chain
/// below is the whole construction.
pub fn routing(settings: &Settings) -> Option<io_harness::Routing> {
    let mut routing = io_harness::Routing::new();
    let mut asked = false;
    if let Some(escalation) = &settings.escalate_after {
        routing = routing.escalate_after(escalation.failures, escalation.model.clone());
        asked = true;
    }
    if let Some(downshift) = &settings.downshift_under {
        routing = routing.downshift_under(downshift.bytes, downshift.model.clone());
        asked = true;
    }
    asked.then_some(routing)
}

/// What the rules do, in one sentence, for `/config` and the startup notices.
///
/// `None` when no rule is named, so a caller writes nothing rather than a line
/// saying nothing happens.
///
/// Escalation is stated first because it is the rule that overrides the other,
/// and the two sentences an operator needs after that are both io-harness's
/// documented behaviour rather than io-cli's: escalation wins over downshifting
/// when both conditions hold (`contract.rs:1808`, and the ordering of the two
/// checks in `model_for` at `contract.rs:1812`), and it happens once and does not
/// come back down (`contract.rs:1753`) — because a run oscillating between two
/// models mid-flight is a behaviour nobody asked for. Both clauses are stated
/// only when they can bite: a section with no escalation rule cannot escalate,
/// and one with only an escalation rule has nothing for it to win over.
///
/// ASCII throughout, as [`crate::gates::Refusal`]'s sentences are: this renders
/// on the plain renderer, under `NO_COLOR`, and through the ASCII glyph set, and
/// a notice that arrives as a replacement character is a notice nobody reads.
pub fn describe(settings: &Settings) -> Option<String> {
    let mut sentence = String::from("routing is in force: this run asks ");
    match (&settings.escalate_after, &settings.downshift_under) {
        (None, None) => return None,
        (Some(escalation), None) => {
            sentence.push_str(&escalation_clause(escalation));
            sentence.push_str(". The change happens once and does not come back down.");
        }
        (None, Some(downshift)) => {
            sentence.push_str(&downshift_clause(downshift));
            sentence.push('.');
        }
        (Some(escalation), Some(downshift)) => {
            sentence.push_str(&escalation_clause(escalation));
            sentence.push_str(", and ");
            sentence.push_str(&downshift_clause(downshift));
            sentence.push_str(
                ". Escalating wins over downshifting when both apply, and it happens once and \
                 does not come back down.",
            );
        }
    }
    Some(sentence)
}

/// The upward rule as a clause, with the count read in English.
///
/// One attempt is singular. A notice that says "after 1 consecutive failed gate
/// attempts" is a notice that reads as unfinished, and the surfaces this appears
/// on are the ones an operator reads once and trusts.
fn escalation_clause(escalation: &Escalation) -> String {
    format!(
        "{} after {} consecutive failed gate attempt{}",
        escalation.model,
        escalation.failures,
        if escalation.failures == 1 { "" } else { "s" },
    )
}

/// The downward rule as a clause.
fn downshift_clause(downshift: &Downshift) -> String {
    format!(
        "{} while it has written fewer than {} bytes",
        downshift.model, downshift.bytes,
    )
}

/// The one thing a contained operator has to be told, or `None`.
///
/// Returned only when both halves are true: the session runs contained **and**
/// the section names at least one rule. Either alone is silence.
///
/// **The narrowness is the whole function, and it has a name.** Returning the
/// sentence whenever a routing section exists would warn every operator who
/// configured routing — the great majority of whom have no
/// `[app.io-cli.containment]` at all, whose runs go through the flat loop, and
/// whose rules therefore work exactly as written — about a limitation that does
/// not apply to them. A caveat attached to a working feature is how an operator
/// learns to stop reading the notices. And the mirror mistake is as bad the other
/// way: a contained session with no rules configured has nothing to disclose, and
/// a line about routing there is a line about a feature the operator never asked
/// for.
///
/// `contained` is passed in rather than read here, so this stays a pure decision
/// over its arguments and a test needs no configuration on disk — the same reason
/// [`crate::gates::Settings::criterion`] takes its model as an argument. The
/// caller knows whether *this* session is contained, which is not the same
/// question as whether caps are configured: `/contain off` is a real switch, and
/// a turn run with it takes the flat loop and routes normally. The sentence says
/// so, because the operator who reads it has a way out.
pub fn inert_under_containment(settings: &Settings, contained: bool) -> Option<String> {
    if !contained || (settings.escalate_after.is_none() && settings.downshift_under.is_none()) {
        return None;
    }
    Some(
        "these routing rules will not fire while [app.io-cli.containment] is configured: \
         io-harness applies routing in its flat workspace loop only, and a contained turn takes \
         each agent's model from that agent's own definition. A turn run with /contain off routes \
         normally."
            .to_string(),
    )
}
