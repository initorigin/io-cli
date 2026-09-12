//! What may be done without asking, and what this install has written down.
//!
//! **The surface for `[[policy.layers]]`, and the other half of the `always`
//! answer.** 0.41.0 let an approval be answered "allow and write it down", which
//! appends a rule to the operator's own configuration — and a grant that can be
//! written from one surface but only un-written from another is a trap. This is
//! where it comes back off.
//!
//! # io un-writes only what io wrote
//!
//! Every rule this module offers to revoke sits in the layer
//! [`crate::approval::REMEMBERED_LAYER`], which nothing but an `always` answer
//! puts a rule into. A rule in any other layer was written by a person — in an
//! editor, deliberately, very possibly to *deny* something — and taking one away
//! on a keystroke would let a permission surface delete a permission boundary.
//!
//! The refusal names the file instead, because an operator who wants that rule
//! gone is one `$EDITOR` away and is better served by being told where it lives
//! than by being offered a verb that declines to explain itself.
//!
//! # This module decides nothing about what is permitted
//!
//! It reads `Config::policy()` and writes TOML. Every verdict is io-harness's,
//! before and after — `workspace.yaml` puts the policy engine there and this
//! crate holds none, which is the same line [`crate::approval::asking`] is held
//! to on the other side of the same feature.

use io_harness::config::Config;

use crate::approval::REMEMBERED_LAYER;

/// One `[[policy.layers]]` rule in force, as `/policy list` draws it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Written {
    /// The layer it belongs to, which is what decides whether io may remove it.
    pub layer: String,
    /// `read`, `write`, `exec` or `net`, as the file spells it.
    pub act: String,
    /// `allow`, `ask` or `deny`.
    pub effect: String,
    /// The glob the rule matches on.
    pub pattern: String,
}

impl Written {
    /// Whether io put this rule here, and may therefore take it away.
    ///
    /// The layer name and nothing else. Not "does it look like one io writes" —
    /// an operator is perfectly entitled to write `{ act = "net", effect =
    /// "allow", pattern = "example.com" }` by hand, and a predicate matching on
    /// the rule's shape would offer to delete it.
    #[must_use]
    pub fn revocable(&self) -> bool {
        self.layer == REMEMBERED_LAYER
    }

    /// The rule as one line, in the order a sentence reads: what, to what, and
    /// then where it came from.
    #[must_use]
    pub fn line(&self) -> String {
        format!("{} {} {}", self.effect, self.act, self.pattern)
    }
}

/// Every rule the configuration in force declares, in the order it declares them.
///
/// **Read through `Config::policy()` rather than out of the file.** The file is
/// one scope; what is in force is the merge of three, and a listing that showed
/// only the nearest would tell an operator a rule had gone when a wider scope
/// still carried it. The order is io-harness's stacking order, which is the order
/// the verdicts are decided in.
#[must_use]
pub fn written(config: &Config) -> Vec<Written> {
    let Some(policy) = config.policy() else {
        return Vec::new();
    };
    policy
        .layers
        .iter()
        .flat_map(|layer| {
            layer.rules.iter().map(|rule| Written {
                layer: layer.name.clone(),
                act: crate::approval::act_key(rule.act).to_string(),
                effect: rule.effect.as_str().to_string(),
                pattern: rule.pattern.clone(),
            })
        })
        .collect()
}

/// The rules `/policy revoke` may offer, which is io's own and no others.
#[must_use]
pub fn revocable(config: &Config) -> Vec<Written> {
    written(config)
        .into_iter()
        .filter(Written::revocable)
        .collect()
}

/// Why a rule may not be revoked from here, or `None` when it may.
///
/// The sentence names the layer, because that is the thing an operator has to go
/// and find. It does not name a file: a layer can be declared in any scope and
/// `Config::policy()` has merged them by the time this is asked, so naming one
/// would be a guess — `/config` is the surface that answers which file decided
/// what.
#[must_use]
pub fn why_kept(rule: &Written) -> Option<String> {
    match rule.revocable() {
        true => None,
        false => Some(format!(
            "`{}` is in the `{}` layer, which io did not write — it is yours, and \
             very possibly a deny. Edit the file that declares it",
            rule.line(),
            rule.layer,
        )),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn rule(layer: &str, effect: &str) -> Written {
        Written {
            layer: layer.to_string(),
            act: "net".to_string(),
            effect: effect.to_string(),
            pattern: "example.com".to_string(),
        }
    }

    /// io offers to remove its own rules and refuses to remove anybody else's.
    ///
    /// The negative is the half that matters: a permission surface that can delete
    /// a `deny` somebody wrote by hand is a permission surface that can remove a
    /// boundary, and the operator would have asked for the opposite.
    #[test]
    fn only_the_layer_io_writes_is_revocable() {
        assert!(rule(REMEMBERED_LAYER, "allow").revocable());
        assert!(why_kept(&rule(REMEMBERED_LAYER, "allow")).is_none());

        // Somebody's own layer, and a deny in it — the case where getting this
        // wrong is worst.
        let theirs = rule("ops-baseline", "deny");
        assert!(!theirs.revocable());
        let why = why_kept(&theirs).expect("a kept rule says why");
        assert!(
            why.contains("ops-baseline"),
            "the refusal does not name the layer an operator has to go and find: {why}",
        );

        // And a rule that merely LOOKS like one io writes is still not io's. The
        // predicate is the layer and never the shape.
        let lookalike = rule("mine", "allow");
        assert!(
            !lookalike.revocable(),
            "a hand-written allow of the same shape was offered for deletion",
        );
    }
}
