//! What a bundle's contribution is called on screen, and what it is called on the
//! wire.
//!
//! **The separator is io-harness's and it is load-bearing.** `io_harness::NAMESPACE`
//! is `__`, and a bundle's skills, agents, MCP servers and policy layers are all
//! named `<bundle>__<name>` when the harness loads them. For a skill that name
//! reaches the *model*: `Skills::catalog` puts it in the system prompt and
//! `read_skill` resolves it by equality, so the model can only ask for the string
//! it was shown. Renaming the separator would change the system prompt, the tool
//! dispatch, every event `target`, both crates' test suites and every consent
//! surface at once.
//!
//! So nothing here renames anything. [`display`] is a translation applied at the
//! moment a name is drawn, and [`wire`] is its inverse, applied to a name an
//! operator typed. Between the two, `__` never reaches a human and `:` never
//! reaches io-harness.
//!
//! **First occurrence only, in both directions.** A bundle id cannot contain `__`
//! — io-harness builds the qualified name by joining on it — so the first `__` is
//! always the join. Anything after it belongs to the contribution's own name and
//! is left exactly as it was found, which is what keeps the round trip honest for
//! a skill that has a double underscore of its own.

use io_harness::NAMESPACE;

/// What the operator reads: `ultraship__brainstorm` becomes `ultraship:brainstorm`.
///
/// A name carrying no separator is returned unchanged, which is every name a
/// bundle did not contribute.
pub fn display(name: &str) -> String {
    match name.split_once(NAMESPACE) {
        Some((bundle, rest)) => format!("{bundle}:{rest}"),
        None => name.to_string(),
    }
}

/// What io-harness is given: `ultraship:brainstorm` becomes `ultraship__brainstorm`.
///
/// The inverse of [`display`], for a name an operator typed. A name carrying no
/// colon is returned unchanged — an unqualified skill is addressed by its own name
/// and always has been.
pub fn wire(name: &str) -> String {
    match name.split_once(':') {
        Some((bundle, rest)) => format!("{bundle}{NAMESPACE}{rest}"),
        None => name.to_string(),
    }
}

/// Whether a word an operator typed is asking for a bundle's contribution by its
/// displayed name.
///
/// Used by `commands::parse` to tell `/ultraship:brainstorm` from a mistyped
/// command. **No command in `COMMANDS` contains a colon**, so the shape is
/// unambiguous without the parse having to know what skills exist — which is what
/// keeps the skills list out of `commands::parse`'s signature and out of the
/// twenty tests that read it. Whether the name resolves to anything is the
/// driver's question, because the driver is what holds the live inventory.
pub fn is_qualified(word: &str) -> bool {
    match word.split_once(':') {
        Some((bundle, rest)) => !bundle.is_empty() && !rest.is_empty(),
        None => false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_namespaced_name_reads_with_a_colon_and_goes_back_with_the_separator() {
        assert_eq!(display("ultraship__brainstorm"), "ultraship:brainstorm");
        assert_eq!(wire("ultraship:brainstorm"), "ultraship__brainstorm");
    }

    #[test]
    fn the_round_trip_is_lossless_for_every_shape_that_reaches_it() {
        for wire_name in [
            "ultraship__brainstorm",
            "caveman__caveman_review",
            // A contribution whose own name carries the separator. Only the first
            // occurrence is the join, which is the whole reason this is
            // `split_once` and not a replace.
            "bundle__deep__nested",
        ] {
            assert_eq!(
                wire(&display(wire_name)),
                wire_name,
                "{wire_name} did not survive the round trip",
            );
        }
    }

    #[test]
    fn a_name_no_bundle_contributed_is_untouched_in_both_directions() {
        assert_eq!(display("brainstorm"), "brainstorm");
        assert_eq!(wire("brainstorm"), "brainstorm");
    }

    #[test]
    fn only_a_qualified_word_is_a_bundles_contribution() {
        assert!(is_qualified("ultraship:brainstorm"));
        assert!(!is_qualified("brainstorm"));
        // Neither half may be empty, or `/:` and `/x:` would be read as skills and
        // take the unknown-command notice away from a genuine typo.
        assert!(!is_qualified(":brainstorm"));
        assert!(!is_qualified("ultraship:"));
        assert!(!is_qualified(":"));
    }

    #[test]
    fn no_shipped_command_carries_a_colon() {
        // The property the parse rests on. If a command ever takes one, the
        // qualified-name arm has to start consulting `COMMANDS` first — and this
        // is where that is discovered, rather than in a session where `/x:y`
        // silently stopped being a command.
        for (name, _) in crate::commands::COMMANDS {
            assert!(
                !name.contains(':'),
                "{name} carries a colon, which the skill-name parse reads as a \
                 bundle qualifier",
            );
        }
    }
}
