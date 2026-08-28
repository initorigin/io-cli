//! F5 — a step that committed says so, once, from the typed call.
//! F7 — the identity a commit will carry is read, never chosen.
//!
//! Both criteria are asserted over values built here rather than over a `Store`.
//! `AssistantTurn` and `ToolCall` are public and constructible, so a run's rows
//! can be spelled directly — which keeps the assertion about the shape io-cli
//! reads instead of about SQLite, and keeps the one property F5 actually turns on
//! visible in the test source: **the message is a `serde_json::Value` field, and
//! never a substring of a display string.**
//!
//! That is why every message below that carries meaning carries a colon. The
//! naive reading of a commit — split `StepRecord::tool_call`, whose
//! `name:json` joining is an internal display convention — survives a corpus of
//! messages that happen to have no `:` in them, and dies on `fix: …`, which is
//! the first line of most commits ever written. A test suite whose messages are
//! all colonless would pass under that reading and prove nothing.

use io_cli::commit::{self, Made};
use io_harness::{AssistantTurn, Defaults, Effect, Identity, Policy, ToolCall};

/// A `git_commit` call carrying `message`.
fn commits(message: &str) -> ToolCall {
    ToolCall {
        name: commit::TOOL.to_string(),
        arguments: serde_json::json!({ "message": message }),
    }
}

/// A turn at `step` that made exactly the calls given, and wrote nothing.
fn turn(step: u32, calls: Vec<ToolCall>) -> AssistantTurn {
    AssistantTurn::new(step, None::<String>, calls)
}

/// The message every colon-sensitive assertion uses.
///
/// A conventional-commit subject, because that is the shape the sabotage breaks
/// on and the shape this product's own history is written in.
const COLON: &str = "fix: handle the colon case";

#[test]
fn f5_the_message_comes_back_whole_including_its_colon() {
    let made = commit::made_in(&[turn(1, vec![commits(COLON)])]);

    assert_eq!(
        made,
        vec![Made {
            step: 1,
            message: COLON.to_string(),
        }],
        "the message is the `message` argument of the typed call, verbatim. \
         Anything less than the whole string means it was reconstructed from a \
         display form rather than read.",
    );
}

#[test]
fn f5_a_colon_message_survives_the_block_and_the_subject() {
    // The same string again, through the two surfaces that render it. A reading
    // that truncated at the colon would leave `made_in` correct and still put
    // `handle the colon case` on screen, so the property is asserted where the
    // operator actually sees it and not only at the boundary.
    let made = Made {
        step: 4,
        message: COLON.to_string(),
    };

    assert_eq!(made.subject(), COLON);
    assert!(
        commit::block(&made, Some("feat/0.25.0"), None)
            .iter()
            .any(|line| line.contains(COLON)),
        "the committed block carries the message as written",
    );
}

#[test]
fn f5_only_git_commit_calls_in_a_turn_are_read() {
    // A real committing step reads, writes and then commits, all in one turn.
    // The name is checked per call, so the neighbours contribute nothing — and a
    // `message` argument on a *different* tool is not a commit message, which is
    // the mistake a per-turn check would make.
    let made = commit::made_in(&[turn(
        7,
        vec![
            ToolCall {
                name: "read_file".to_string(),
                arguments: serde_json::json!({ "path": "src/commit.rs" }),
            },
            commits(COLON),
            ToolCall {
                name: "write_file".to_string(),
                arguments: serde_json::json!({ "message": "not a commit: a note" }),
            },
        ],
    )]);

    assert_eq!(
        made,
        vec![Made {
            step: 7,
            message: COLON.to_string(),
        }],
    );
}

#[test]
fn f5_commits_come_back_oldest_first() {
    // Two turns, and two calls inside the second, because both orderings have to
    // hold: `step_turns` returns turns oldest first and a turn's `calls` are in
    // the order the model made them. Distinct messages rather than distinct
    // steps, so a sort on `step` alone cannot pass this.
    let made = commit::made_in(&[
        turn(2, vec![commits("feat: the first thing")]),
        turn(
            5,
            vec![commits("fix: the second thing"), commits("docs: the third")],
        ),
    ]);

    assert_eq!(
        made.iter().map(Made::subject).collect::<Vec<_>>(),
        vec![
            "feat: the first thing",
            "fix: the second thing",
            "docs: the third",
        ],
    );
    assert_eq!(
        made.iter().map(|m| m.step).collect::<Vec<_>>(),
        vec![2, 5, 5]
    );
}

#[test]
fn f5_a_call_with_no_usable_message_commits_nothing() {
    // Each of these is a call the model made wrongly and the harness refused.
    // None of them is somebody committing an empty message, and rendering a
    // block for one would be a scrollback claiming a commit that does not exist.
    let unusable = [
        serde_json::json!({}),
        serde_json::json!({ "msg": "fix: wrong argument name" }),
        serde_json::json!({ "message": 42 }),
        serde_json::json!({ "message": null }),
        serde_json::json!({ "message": ["fix: a list is not a message"] }),
        serde_json::json!({ "message": "" }),
        serde_json::json!({ "message": "   \n\t  " }),
        // Not an object at all — a model that produced a bare string where the
        // schema asked for arguments. `get` on it answers `None`, and the point
        // of asserting it is that it must not panic.
        serde_json::json!("fix: arguments that are not an object"),
    ];

    for arguments in unusable {
        let made = commit::made_in(&[turn(
            1,
            vec![ToolCall {
                name: commit::TOOL.to_string(),
                arguments: arguments.clone(),
            }],
        )]);
        assert!(
            made.is_empty(),
            "arguments {arguments} yielded a commit; a call the harness refused \
             must commit no block",
        );
    }
}

#[test]
fn f5_no_turns_and_no_commits_are_the_same_silence() {
    assert!(commit::made_in(&[]).is_empty());
    assert!(commit::made_in(&[turn(1, Vec::new())]).is_empty());
}

#[test]
fn f5_the_block_carries_the_message_the_branch_and_what_git_touched() {
    let made = Made {
        step: 3,
        message: COLON.to_string(),
    };

    assert_eq!(
        commit::block(&made, Some("feat/0.25.0"), Some("2 files changed")),
        vec![
            "committed on feat/0.25.0".to_string(),
            format!("  {COLON}"),
            "  2 files changed".to_string(),
        ],
        "one block, carrying all three things the criterion names",
    );
}

#[test]
fn f5_an_unknown_branch_or_report_leaves_out_a_line_rather_than_inventing_one() {
    let made = Made {
        step: 3,
        message: COLON.to_string(),
    };

    assert_eq!(
        commit::block(&made, None, None),
        vec!["committed".to_string(), format!("  {COLON}")],
    );
    // Present but blank is an unknown wearing a value's clothes, and is treated
    // as the unknown it is rather than drawn as `committed on `.
    assert_eq!(
        commit::block(&made, Some("  "), Some("\n")),
        commit::block(&made, None, None),
    );
}

#[test]
fn f5_a_body_is_kept_whole_and_the_subject_is_only_its_first_line() {
    let made = Made {
        step: 9,
        message: format!("{COLON}\n\nThe body: it explains why.\nAnd a second line."),
    };

    assert_eq!(
        made.subject(),
        COLON,
        "a status row shows the subject alone"
    );
    assert_eq!(
        commit::block(&made, Some("develop"), None),
        vec![
            "committed on develop".to_string(),
            format!("  {COLON}"),
            // The blank separator stays blank. Indenting nothing would put two
            // trailing spaces into everything that copies this block back out.
            String::new(),
            "  The body: it explains why.".to_string(),
            "  And a second line.".to_string(),
        ],
    );
}

#[test]
fn f5_a_subject_below_a_leading_blank_line_is_still_the_subject() {
    // A model that opens with a newline has still written a subject. Taking
    // `lines().next()` here answers `""` — a status row gone blank for a commit
    // that has one.
    let made = Made {
        step: 1,
        message: format!("\n{COLON}\n\nbody"),
    };
    assert_eq!(made.subject(), COLON);
}

#[test]
fn f5_the_subject_of_a_message_with_nothing_in_it_is_empty() {
    // `made_in` never builds one of these — every such call is skipped above —
    // so this pins the direct-construction path: empty, not a panic, and not the
    // body of some later line promoted into a subject it never was.
    for message in ["", "\n", "   ", "\n\n\n"] {
        assert_eq!(
            Made {
                step: 1,
                message: message.to_string(),
            }
            .subject(),
            "",
            "message {message:?}",
        );
    }
}

#[test]
fn f5_the_prompt_asks_the_agent_to_write_the_message() {
    let prompt = commit::prompt();
    assert!(
        prompt.contains("Commit the work from this turn"),
        "the command is a prompt to the agent, not a git invocation io-cli makes",
    );
    assert!(
        !prompt.is_empty() && !prompt.contains("  "),
        "a prompt continued across source lines keeps single spaces: {prompt:?}",
    );
}

#[test]
fn f7_the_identity_shown_is_the_one_the_contract_carries() {
    let configured = Identity {
        name: "release bot".to_string(),
        email: "bot@example.com".to_string(),
    };
    assert_eq!(
        commit::authored_as(&configured),
        "authored as release bot <bot@example.com>",
        "`[run.commit_identity]` is read and printed, both halves, unchanged",
    );
}

#[test]
fn f7_an_unconfigured_repository_is_told_the_harness_default() {
    // `TaskContract::commit_identity` is an `Identity`, not an `Option<Identity>`,
    // and holds exactly this when no `[run.commit_identity]` was written. So the
    // absent-section case is this value, and what io-cli must show for it is the
    // value io-harness will hand git — read from `Identity::default()` itself
    // rather than copied into a literal here, because a copy is a second place
    // for the string to live and the wrong one would still pass.
    let default = Identity::default();
    let shown = commit::authored_as(&default);

    assert_eq!(
        shown,
        format!("authored as {} <{}>", default.name, default.email),
    );
    assert!(
        shown.contains("io-harness"),
        "the default belongs to the harness and says so: {shown:?}",
    );
    assert!(
        !shown.contains("io-cli"),
        "a name this crate chose would be written into the operator's history by \
         a tool that was only asked to describe it, and there is nowhere to \
         correct it afterwards: {shown:?}",
    );
}

#[test]
fn f7_no_identity_is_substituted_for_one_that_merely_looks_like_a_default() {
    // The shape of the F7 sabotage: recognise the default and swap in something
    // of io-cli's own. An identity is read and printed, so a name that reads like
    // a placeholder is still printed as written.
    for name in ["io-harness agent", "", "unknown"] {
        let identity = Identity {
            name: name.to_string(),
            email: "somebody@example.com".to_string(),
        };
        assert_eq!(
            commit::authored_as(&identity),
            format!("authored as {name} <somebody@example.com>"),
        );
    }
}

// F2's first half — the check that happens BEFORE a turn is bought.
//
// `asked` is the whole of the pre-turn decision, and it lives here rather than in
// the driver for the reason every decision in this release does: nothing under
// `tests/` links `src/main.rs`, so a refusal written there could be neither
// asserted nor sabotaged. The driver holds the wiring — build the policy in
// force, ask, and either say why not or submit — and this is where the answer is
// pinned.

/// A policy whose `exec` default is what a posture would set it to.
fn posture(exec: Effect) -> Policy {
    let mut policy = Policy::permissive();
    policy.defaults = Defaults {
        read: Effect::Allow,
        write: Effect::Allow,
        exec,
        net: Effect::Deny,
    };
    policy
}

#[test]
fn f2_a_posture_that_allows_git_buys_the_turn() {
    assert_eq!(
        commit::asked(&posture(Effect::Allow), false),
        Ok(()),
        "nothing refuses git, so there is nothing to say and the turn is bought",
    );
}

#[test]
fn f2_an_asking_posture_is_refused_before_the_turn_is_paid_for() {
    let refusal = commit::asked(&posture(Effect::Ask), true)
        .expect_err("an asking posture refuses git at the spawn, so /commit must not spend a turn");
    assert!(
        refusal.contains("refused rather than asked"),
        "the sentence has to name what the posture did, because the posture's own \
         name says it will ask and the harness does not: {refusal:?}",
    );
    assert!(
        refusal.contains("git"),
        "and it has to name the one binary the allowance covers: {refusal:?}",
    );
}

#[test]
fn f2_a_denying_posture_is_refused_without_being_offered_a_rule() {
    // The asymmetry found while building F1, and the reason `asked` takes the
    // second argument at all. A rule is matched BEFORE a default, so the same
    // allowance that lifts an asking default would also lift a denying one — and
    // a keystroke that defeats the one posture whose name is a promise is not a
    // convenience. Under a deny the answer is to change posture.
    let refusal = commit::asked(&posture(Effect::Deny), false)
        .expect_err("a denying posture cannot run a command, so it cannot commit");
    assert!(
        !refusal.contains("Allow `git`"),
        "a denying posture must NOT be offered the allowance: {refusal:?}",
    );
    assert!(
        refusal.contains("posture"),
        "it is told what to change instead: {refusal:?}",
    );
}

#[test]
fn f2_the_two_refusals_are_not_the_same_sentence() {
    let asking = commit::asked(&posture(Effect::Ask), true).unwrap_err();
    let denying = commit::asked(&posture(Effect::Deny), false).unwrap_err();
    assert_ne!(
        asking, denying,
        "an operator who asked to be asked and one who asked for nothing to run \
         are in different situations, and one of them has a one-keystroke fix",
    );
}
