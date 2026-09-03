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
//! the unit TOML itself enforces.
//!
//! **Both fields are nonetheless optional, and a half rule is refused rather than
//! rejected.** Making them required looked like the stricter choice and was the
//! more damaging one: a required field is a *deserialization* failure, so a
//! threshold with no model did not fail the rule, it failed `CliSettings` — and
//! `crate::settings::stored` then answered `None` for the whole `[app.io-cli]`
//! section, taking the theme, the keys, the ceilings, the capabilities and the
//! **verification gate** with it, silently. [`routing`] refuses the half rule by
//! name instead, which is the shape [`crate::gates::Settings::criterion`] already
//! uses, and [`notice`] is what says so on screen.
//!
//! The same function refuses the three values that are writable and disastrous:
//! `failures = 0`, which io-harness reads as "escalate before anything has
//! failed" and which therefore pins every run to the escalation model from its
//! first request; `bytes = 0`, which can never be true; and a model named as the
//! empty string, which would send every request of the run with no model id.
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
//!
//! # io-harness grew a `[routing]` table of its own in 0.76.0
//!
//! `Config::apply_to` now merges a **user-scope** `[routing]` onto the contract
//! key by key (`config.rs:2076`): `escalate_after`/`escalate_to`,
//! `downshift_under`/`downshift_to`, `require_primary`, and `mechanical` — which
//! names the model that reads the whole transcript when a fold summarises it, and
//! for which io-cli offers no key at all. io-cli builds its own section *after*
//! that merge (`contract.rs:356`) through `TaskContract::with_routing`, whose body
//! is `self.routing = Some(routing)` (`contract.rs:1292`). It replaces. So a
//! `[app.io-cli.routing]` that names a rule takes the whole of `[routing]` back
//! off the contract, `mechanical` included, and until this release it did so in
//! silence.
//!
//! **io-cli does not merge the two, and that is the decision rather than the
//! omission.** Merging would make this crate choose precedence between its own
//! section and the dependency's — which key of which table wins when both name an
//! escalation — and that is a second opinion about what a `Routing` means, which
//! is the shape `tests/dependencies.rs` exists to keep out of this crate. The
//! behaviour is therefore unchanged: io-cli's section wins. What changes is that
//! [`native_notice`] says so.
//!
//! The collision can only be written in the user scope, which is why the test for
//! it needs a user-scoped fixture rather than `Config::from_toml`. `routing` is in
//! io-harness's `REFUSED_SECTIONS` (`config.rs:2394`), so `io.toml`,
//! `io.local.toml` and a `[profile]` body may not declare it; the match is
//! `contains_key("routing")` against the **top-level** table (`config.rs:2600`),
//! so `[app.io-cli.routing]` — nested under `app` — is untouched by that rule in
//! every scope.

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
/// checked. Inside each table a typo is likewise a key nobody reads, and the half
/// rule it leaves behind is refused by [`routing`] by name rather than by serde —
/// see the module documentation for why the stricter-looking choice, making those
/// keys required, was the more damaging one.
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
    ///
    /// **Optional, and that is a correction rather than a loosening.** It was
    /// required, and a required field is a *deserialization* failure — so half a
    /// rule did not fail the rule, it failed `CliSettings` entirely, and
    /// `crate::settings::stored` then answered `None` for the whole `[app.io-cli]`
    /// section. An operator who wrote a threshold and forgot the model silently
    /// lost their theme, their keys, their ceilings, their capabilities and — worst
    /// of all — their **verification gate**, because `contract::criterion_for`
    /// gives up on the same `None`. A gate that stops gating without saying so is
    /// the most expensive failure this crate has. Half a rule now parses and is
    /// refused by name at [`routing`], which is the shape
    /// [`crate::gates::Settings::criterion`] already uses.
    #[serde(default)]
    pub failures: Option<u32>,
    /// The model asked from then on.
    #[serde(default)]
    pub model: Option<String>,
}

/// The downward rule: `[app.io-cli.routing.downshift_under]`.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Downshift {
    /// The byte total below which the cheaper model is asked.
    ///
    /// Measured on what the run has already written to disk, not on what it
    /// planned to write — again io-harness's definition (`contract.rs:1759`).
    ///
    /// Optional for the reason [`Escalation::failures`] gives at length.
    #[serde(default)]
    pub bytes: Option<u64>,
    /// The model asked while the run is under that total.
    #[serde(default)]
    pub model: Option<String>,
}

/// A routing section that cannot be obeyed, and why.
///
/// The sibling of [`crate::gates::Refusal`] and it exists for the same reason: a
/// mistake TOML itself cannot express has to be caught where the operator can be
/// told about it, rather than reaching io-harness and changing every request of
/// every run.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Refusal {
    /// A threshold with no model, or a model with no threshold.
    HalfARule {
        /// `escalate_after` or `downshift_under`.
        rule: &'static str,
        /// The key that is missing.
        missing: &'static str,
    },
    /// `failures = 0`, which escalates before anything has failed.
    ///
    /// io-harness compares `consecutive_gate_failures >= failures`
    /// (`contract.rs:1813`), so zero is true at the first request of every run —
    /// the stronger model is used unconditionally, from the start, and
    /// `downshift_under` is never reached because escalation is checked first. An
    /// operator writing it means "escalate readily" and gets "never use the model
    /// I configured".
    EscalatesBeforeAnythingFailed,
    /// `bytes = 0`, which can never be true.
    ///
    /// `written < 0` is false for every run, so the rule is inert. Unlike the
    /// above it costs nothing, and it is still refused rather than ignored: a rule
    /// that silently does nothing is what the operator will not find when they
    /// wonder why the cheap model never appears.
    NeverDownshifts,
    /// A model named as the empty string.
    ///
    /// `apply_routing` sets `request.model = Some("")` and every request of the run
    /// goes to the vendor with an empty model id.
    NoModel {
        /// Which rule named it.
        rule: &'static str,
    },
}

impl std::fmt::Display for Refusal {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::HalfARule { rule, missing } => write!(
                f,
                "[app.io-cli.routing.{rule}] names no `{missing}`, and a threshold \
                 without a model is half a rule — this turn is not routed"
            ),
            Self::EscalatesBeforeAnythingFailed => write!(
                f,
                "[app.io-cli.routing.escalate_after] has `failures = 0`, which is \
                 true before anything has failed — every run would start on the \
                 escalation model and never reach the downshift. This turn is not \
                 routed."
            ),
            Self::NeverDownshifts => write!(
                f,
                "[app.io-cli.routing.downshift_under] has `bytes = 0`, and a run \
                 that has written fewer than zero bytes does not exist — the rule \
                 could never fire. This turn is not routed."
            ),
            Self::NoModel { rule } => write!(
                f,
                "[app.io-cli.routing.{rule}] names an empty model, which would send \
                 every request of the run with no model id — this turn is not routed"
            ),
        }
    }
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
/// **The two thresholds are checked rather than trusted**, because io-harness
/// obeys them literally and each has a value that is both writable and disastrous.
/// `failures = 0` satisfies `consecutive_gate_failures >= 0` at the first request
/// of every run, so the escalation model is used unconditionally and the downshift
/// — checked second — never runs at all. `bytes = 0` can never be true. An empty
/// model sends every request with no model id. None of the three is a shape TOML
/// can refuse, and all three are one keystroke from a plausible file.
pub fn routing(settings: &Settings) -> Result<Option<io_harness::Routing>, Refusal> {
    let mut routing = io_harness::Routing::new();
    let mut asked = false;
    if let Some(escalation) = &settings.escalate_after {
        let failures = escalation.failures.ok_or(Refusal::HalfARule {
            rule: "escalate_after",
            missing: "failures",
        })?;
        let model = escalation.model.as_deref().ok_or(Refusal::HalfARule {
            rule: "escalate_after",
            missing: "model",
        })?;
        if failures == 0 {
            return Err(Refusal::EscalatesBeforeAnythingFailed);
        }
        if model.trim().is_empty() {
            return Err(Refusal::NoModel {
                rule: "escalate_after",
            });
        }
        routing = routing.escalate_after(failures, model.to_string());
        asked = true;
    }
    if let Some(downshift) = &settings.downshift_under {
        let bytes = downshift.bytes.ok_or(Refusal::HalfARule {
            rule: "downshift_under",
            missing: "bytes",
        })?;
        let model = downshift.model.as_deref().ok_or(Refusal::HalfARule {
            rule: "downshift_under",
            missing: "model",
        })?;
        if bytes == 0 {
            return Err(Refusal::NeverDownshifts);
        }
        if model.trim().is_empty() {
            return Err(Refusal::NoModel {
                rule: "downshift_under",
            });
        }
        routing = routing.downshift_under(bytes, model.to_string());
        asked = true;
    }
    Ok(asked.then_some(routing))
}

/// Why the configured routing is not routing this run, if it is not.
///
/// The sibling of [`crate::contract::gate_notice`], and it exists for the reason
/// that one does: a refusal is not a `Routing`, so the function that answers with a
/// value cannot also be the one that explains itself.
#[must_use]
pub fn notice(settings: &Settings) -> Option<String> {
    routing(settings).err().map(|refusal| refusal.to_string())
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
    // **"asks for" and not "is in force", because this sentence is printed beside
    // one that may contradict it.** `/config` records this and then, for a
    // contained session, `inert_under_containment`'s warning that the rules will
    // not fire — so a prefix asserting the rules are active made two adjacent
    // lines disagree. What this function knows is what the operator *wrote*;
    // whether it fires is the other function's subject. `/config` is also typed at
    // an idle prompt, so "this run" named nothing.
    let mut sentence = String::from("the routing rules ask for ");
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
    // A half rule is refused by `routing` before any caller reaches a sentence, so
    // the fallbacks below describe a rule that cannot reach the contract. They are
    // written rather than unwrapped because a surface that panics on a
    // configuration file is worse than one that says "unset".
    let failures = escalation.failures.unwrap_or_default();
    format!(
        "{} after {} consecutive failed gate attempt{}",
        escalation.model.as_deref().unwrap_or("an unnamed model"),
        failures,
        if failures == 1 { "" } else { "s" },
    )
}

/// The downward rule as a clause.
fn downshift_clause(downshift: &Downshift) -> String {
    format!(
        "{} while it has written fewer than {} bytes",
        downshift.model.as_deref().unwrap_or("an unnamed model"),
        downshift.bytes.unwrap_or_default(),
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

/// What a harness-native `[routing]` table means for this operator, or `None`.
///
/// The disclosure the 0.76.0 pin owes: io-harness merges `[routing]` onto the
/// contract and io-cli then replaces it, and neither half announces itself. See
/// the module documentation for the mechanism and for why the answer is a sentence
/// rather than a merge.
///
/// **Two sentences rather than one, because the two situations need different
/// things said.** A `[routing]` with nothing beside it loses nothing: io-harness
/// merged it and io-cli left the contract alone, so what the operator is missing is
/// not their rules but the fact that no surface here will ever show them —
/// `/config` lists four keys and every one of them is `app.io-cli.routing.*`, and
/// [`describe`], [`notice`] and [`inert_under_containment`] all read that section
/// alone. A `[routing]` with a section beside it that reaches the contract is the
/// silent loss, and that sentence names both tables and says which one won.
///
/// **`settings` is asked through [`routing`] rather than for its own presence**,
/// and that is the correctness of the split rather than a shortcut. A section that
/// is present and empty, and a section refused for a half rule or a threshold that
/// could only misfire, both leave `contract.rs:356` on its `None` arm — the
/// contract keeps the merged `[routing]` and nothing was overwritten. Keying on
/// `settings.is_some()` would tell those operators their table had been dropped
/// when it is the one in effect, which is worse than saying nothing.
///
/// The table is found through [`io_harness::Config::origins`], the dependency's own
/// record of every key a file set, rather than by reading the file again: this
/// crate parses TOML in `src/edit.rs` alone, and a second reading of a table
/// io-harness has already read and validated is a second opinion about it. It also
/// catches the spellings a header scan would miss — `routing.mechanical = "…"`
/// written flat at the top level is the same table with no `[routing]` line in the
/// file at all.
///
/// ASCII throughout, as [`inert_under_containment`]'s sentence is: this renders on
/// the plain renderer, under `NO_COLOR` and through the ASCII glyph set.
#[must_use]
pub fn native_notice(config: &io_harness::Config, settings: Option<&Settings>) -> Option<String> {
    // The harness's table is top-level, so `app.io-cli.routing.*` — recorded by
    // `origins` under `app` — matches neither arm. The bare key is tested as well
    // as the dotted prefix because `origins` records leaves: a `routing` that is
    // not a table is one key rather than a prefix, and a rule spelled to catch only
    // the prefix would be a rule with a hole exactly where a malformed file is.
    if !config
        .origins()
        .any(|(key, _)| key == "routing" || key.starts_with("routing."))
    {
        return None;
    }
    if settings
        .and_then(|settings| routing(settings).ok().flatten())
        .is_some()
    {
        return Some(
            "[routing] and [app.io-cli.routing] are both configured, and only \
             [app.io-cli.routing] reaches the contract: io-harness merges [routing] onto it \
             key by key, io-cli then sets its own section through with_routing, and that \
             builder replaces rather than merges, so every key of [routing] is dropped, \
             mechanical included. Write the rules in one section or the other."
                .to_string(),
        );
    }
    Some(
        "[routing] is configured, and no io-cli surface lists it: /config edits \
         [app.io-cli.routing] only, and every routing notice this product prints describes \
         that section alone. io-harness merges [routing] onto the contract itself, so it \
         reaches the run through the dependency rather than through anything here."
            .to_string(),
    )
}
