//! What "done" means for this repository, and whether the turn proved it.
//!
//! io-harness has carried a verification pillar since long before this interface
//! existed: a `TaskContract` holds one `Verification`, the run loop executes it
//! after the agent stops, and the run comes back as `RunOutcome::Success` when it
//! passed. Until this release io-cli supplied none, so every contract it built
//! carried `Verification::None` and every clean run returned `Finished` — which
//! `crate::exec` already documents as an honest report of a contract that asked
//! for nothing.
//!
//! This module is where the operator's answer lives. [`Settings`] is the
//! `[app.io-cli.gates]` section as written; [`Criterion`] is the one criterion
//! that section resolves to, once it has been checked for the two mistakes the
//! file itself cannot express.
//!
//! # One criterion, not a suite
//!
//! `TaskContract`'s verification is a single value rather than a list, so a
//! contract carries one criterion. That is the whole surface this module offers,
//! and the section is a table rather than an array so that a list, if the
//! dependency ever grows one, is an addition rather than a break.
//!
//! # Why the criterion is not run here
//!
//! io-harness executes it, inside the sandbox, with `argv[0]` checked against the
//! operator's `Act::Exec` policy. io-cli could run one itself — `Verification`
//! exposes the calls — but `ExecGuard::with_writable_roots` is `pub(crate)`, so a
//! criterion run from here cannot be given the cache directories a real run gets
//! from the detected toolchain, and a `cargo test` gate would fail on a registry
//! write the harness's own gate would have allowed. The criterion rides the
//! contract; the dependency runs it. That is also what keeps `crate::shell` the
//! only module in this crate that spawns anything.
//!
//! # The refusals happen when the file is written
//!
//! A review criterion with no reviewer, or one whose reviewer is the model doing
//! the work, is `Error::Config` at run start in io-harness — before the first
//! billed call, on every turn, with the failure disconnected on screen from the
//! keystroke that caused it. So [`Refusal`] exists to catch both while the
//! operator is still looking at the surface that wrote them.

use std::path::PathBuf;

use serde::{Deserialize, Serialize};

/// The `[app.io-cli.gates]` section exactly as an operator wrote it.
///
/// Flat rather than tagged, matching every other section this crate reads: TOML
/// spells a flat table far more naturally than an internally tagged enum, and a
/// key nobody set is simply absent. Exactly one of [`Settings::command`],
/// [`Settings::file`] and [`Settings::rubric`] decides the kind; naming none or
/// more than one is a [`Refusal`] rather than a silent precedence rule, because a
/// precedence rule here would quietly gate a turn on something the operator did
/// not choose.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Settings {
    /// How many further turns a failing gate may buy, defaulting to one.
    ///
    /// Zero means the gate reports and nothing is re-driven. It is a small
    /// number on purpose: a retry is a whole turn against a real model, and an
    /// operator who wants several says so.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub retries: Option<u8>,

    /// The argv of a command that must exit [`Settings::expect_exit`].
    ///
    /// An argv rather than a command line, because io-harness checks `argv[0]`
    /// against the policy and runs it without a shell. An empty vector means the
    /// command the repository's own toolchain proposes.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub command: Option<Vec<String>>,

    /// The exit status the command must report, defaulting to zero.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub expect_exit: Option<i32>,

    /// A workspace-relative file that must exist.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub file: Option<PathBuf>,

    /// Text [`Settings::file`] must contain. Absent asserts existence alone.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub contains: Option<String>,

    /// What a second model is asked to judge the work against.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub rubric: Option<String>,

    /// The model that answers [`Settings::rubric`].
    ///
    /// Required whenever a rubric is set, and never defaulted to the model doing
    /// the work — a judge marking its own paper is a decision the operator makes
    /// explicitly through [`Settings::allow_self_review`].
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub reviewer: Option<String>,

    /// Whether the reviewer may be the same model that did the work.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub allow_self_review: Option<bool>,
}

/// The one criterion a [`Settings`] resolves to.
///
/// Distinct from [`Settings`] because a section is what was written and a
/// criterion is what survived being checked. Everything downstream — the
/// contract, the status line, the retry, the exit code — takes this.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Criterion {
    /// A command that must exit a named status, run by io-harness in its sandbox.
    Command {
        /// The program and its arguments. Never a shell line.
        argv: Vec<String>,
        /// The status that counts as passing.
        expect_exit: i32,
    },
    /// A workspace-relative file that must exist, and optionally say something.
    File {
        /// The path, relative to the workspace root.
        file: PathBuf,
        /// Text the file must contain, or `None` to assert existence alone.
        contains: Option<String>,
    },
    /// A rubric a second model answers.
    Review {
        /// What the reviewer is asked to judge.
        rubric: String,
        /// The model that answers it.
        reviewer: String,
        /// Whether that model may be the one that did the work.
        allow_self_review: bool,
    },
}

/// Why a section cannot become a [`Criterion`].
///
/// Each of these is refused where the operator can still see what they typed.
/// The last two exist because io-harness answers them with `Error::Config` at run
/// start, which would turn writing a configuration file into a session that will
/// not start.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Refusal {
    /// The section names no criterion at all.
    Empty,
    /// The section names more than one kind, and there is no right answer.
    Ambiguous,
    /// A rubric was given with no model to answer it.
    ReviewerMissing,
    /// The reviewer is the model doing the work, and that was not asked for.
    SelfReview {
        /// The model named as both the worker and the judge.
        model: String,
    },
}
