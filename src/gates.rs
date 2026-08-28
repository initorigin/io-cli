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

use std::fmt;
use std::path::{Path, PathBuf};

use io_harness::{Config, GateAttempt, GateOutcome, SandboxEvent, Verification};
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

impl Settings {
    /// The one criterion this section resolves to, or why it cannot.
    ///
    /// `Ok(None)` is the answer for a section that is entirely absent — every key
    /// unset. That is not the same as [`Refusal::Empty`], which is a section an
    /// operator plainly meant to be a gate (`retries` is set, or `contains`, or a
    /// `reviewer`) and which names no kind: the first is "no gate was asked for"
    /// and the second is "a gate was asked for and half-written", and answering
    /// both with silence would let a typo turn a gate off without a word.
    ///
    /// `turn_model` is the model the session will actually run with, passed in
    /// rather than read here, so this function stays a pure decision over its
    /// arguments and the tests need no configuration on disk. An empty string
    /// means the caller does not yet know which model will run — the wizard, a
    /// fresh home directory — and the self-review refusal cannot fire, because
    /// there is nothing to compare the reviewer against. It is not defaulted to
    /// "no clash": the caller that knows the model is the caller that must say it.
    ///
    /// The order of the checks is the order an operator can act on. Ambiguity is
    /// answered before emptiness because the two cannot both be true; a missing
    /// reviewer is answered before self-review because a reviewer that was never
    /// named cannot be compared to anything.
    pub fn criterion(&self, turn_model: &str) -> Result<Option<Criterion>, Refusal> {
        let kinds = u8::from(self.command.is_some())
            + u8::from(self.file.is_some())
            + u8::from(self.rubric.is_some());
        if kinds > 1 {
            return Err(Refusal::Ambiguous);
        }
        if kinds == 0 {
            // Compared against the default rather than against a hand-written
            // list of the remaining keys: a key added to `Settings` later joins
            // this check by existing, instead of by somebody remembering to add
            // it here and shipping a silent hole when they do not.
            return if self == &Settings::default() {
                Ok(None)
            } else {
                Err(Refusal::Empty)
            };
        }

        if let Some(argv) = &self.command {
            return Ok(Some(Criterion::Command {
                argv: argv.clone(),
                expect_exit: self.expect_exit.unwrap_or(0),
            }));
        }
        if let Some(file) = &self.file {
            return Ok(Some(Criterion::File {
                file: file.clone(),
                contains: self.contains.clone(),
            }));
        }

        let rubric = self
            .rubric
            .clone()
            .expect("one kind is set and it is the rubric");
        let Some(reviewer) = self.reviewer.clone() else {
            return Err(Refusal::ReviewerMissing);
        };
        let allow_self_review = self.allow_self_review.unwrap_or(false);
        if !allow_self_review && !turn_model.is_empty() && reviewer == turn_model {
            return Err(Refusal::SelfReview { model: reviewer });
        }
        Ok(Some(Criterion::Review {
            rubric,
            reviewer,
            allow_self_review,
        }))
    }

    /// How many further turns a failing gate may buy, with the default applied.
    ///
    /// One, not zero and not three. Zero would make the whole retry half of this
    /// release opt-in, and an operator who configured a gate has already said they
    /// want the work checked; more than one spends real money on a model that has
    /// already been told once what it got wrong.
    pub fn retries(&self) -> u8 {
        self.retries.unwrap_or(1)
    }
}

impl Criterion {
    /// The criterion as the dependency's own type, ready for `with_verification`.
    ///
    /// Three of the four mappings are direct. The fourth — a file criterion with
    /// no needle — is the one that has no honest counterpart, and it is worth
    /// saying exactly why rather than picking the nearest variant:
    ///
    /// `Verification::WorkspaceFileContains` reads its file with
    /// `read_to_string(..).unwrap_or_default()`, so a file that is not there is
    /// the empty string, and every string contains the empty needle. Mapping
    /// existence to an empty needle would therefore produce a gate that passes on
    /// a file nobody ever wrote — a criterion that can never fail, which is worse
    /// than no criterion at all, because a run reports `Success` on it.
    /// `DocumentContains` does error on a file it cannot read, but it errors on
    /// nearly every file: it reads four office formats by extension and refuses
    /// the rest, so it would report a criterion that could not be evaluated rather
    /// than one that failed. Nothing else in the enum reads a path.
    ///
    /// So existence is answered by [`Criterion::satisfied_in`], here, with a
    /// reader that tells a missing file from an empty one — and this returns
    /// `Verification::None`, which is the truthful statement that io-harness was
    /// asked to check nothing. A caller must therefore ask
    /// [`Criterion::checked_here`] before treating a run's own outcome as the
    /// whole verdict; that is what the flag is for.
    pub fn verification(&self) -> Verification {
        match self {
            Criterion::Command { argv, expect_exit } => Verification::Command {
                argv: argv.clone(),
                expect_exit: *expect_exit,
            },
            Criterion::File {
                file,
                contains: Some(needle),
            } => Verification::WorkspaceFileContains {
                file: file.clone(),
                needle: needle.clone(),
            },
            Criterion::File { contains: None, .. } => Verification::None,
            Criterion::Review {
                rubric,
                allow_self_review,
                ..
            } => Verification::Review {
                rubric: rubric.clone(),
                allow_self_review: *allow_self_review,
            },
        }
    }

    /// Whether this crate evaluates the criterion itself rather than the run loop.
    ///
    /// True for exactly one shape — a file criterion with no needle — and the
    /// reason it is a question a surface can ask without a workspace root is that
    /// "the run finished and its gate passed" is not the whole verdict for such a
    /// criterion. A caller that reads [`Criterion::verification`] and nothing else
    /// would see `Verification::None` and report an ungated run.
    pub fn checked_here(&self) -> bool {
        matches!(self, Criterion::File { contains: None, .. })
    }

    /// The verdict for a criterion this crate owns, or `None` when io-harness does.
    ///
    /// The read goes through `io_harness::tools::Workspace::read_bytes`, and the
    /// choice of reader is the whole check: `Workspace::read_file` answers `Ok("")`
    /// for a file that is not there, so a criterion written on top of it passes on
    /// a file that was never created. `read_bytes` is the one that errors. Going
    /// through `Workspace` rather than `std::fs` also keeps the path resolution
    /// the rest of this crate uses, so a `file` that climbs out of the workspace
    /// is refused here exactly as it is everywhere else.
    ///
    /// A directory at that path is not a satisfied criterion: reading it fails,
    /// which is the answer an operator who wrote a file name wanted.
    pub fn satisfied_in(&self, root: &Path) -> Option<bool> {
        let Criterion::File {
            file,
            contains: None,
        } = self
        else {
            return None;
        };
        // Separators normalised the way io-harness normalises them before it
        // resolves a workspace-relative path, so a path an operator typed with
        // backslashes means the same file on either platform.
        let relative = file.to_string_lossy().replace('\\', "/");
        Some(
            io_harness::tools::Workspace::new(root)
                .read_bytes(&relative)
                .is_ok(),
        )
    }

    /// What the criterion asks, in one sentence, for a prompt or a surface.
    ///
    /// Delegates to the dependency for everything it can express, so the words a
    /// retried turn reads are the same words the first turn was judged by. The
    /// existence criterion has no `Verification` to delegate to and says so here.
    pub fn describe(&self) -> String {
        if let Criterion::File {
            file,
            contains: None,
        } = self
        {
            return format!("the file {} must exist in the workspace", file.display());
        }
        self.verification().describe()
    }
}

/// The gate's standing after a turn, as a surface draws it.
///
/// Read off the stored rows rather than remembered, because a resumed session did
/// not run the turn whose gate it has to report, and a field that is only correct
/// when this process happened to watch the run is a field that lies after every
/// `/resume`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Standing {
    /// Which criterion ran, as io-harness recorded it — one of `command`,
    /// `review`, `compiles`, `document`, `contains`, `none`. Taken from the row
    /// rather than from the configured [`Criterion`]: the row is what actually
    /// ran, and a configuration edited mid-session would otherwise relabel a gate
    /// that was never it.
    pub phase: String,
    /// How the last attempt ended.
    pub outcome: GateOutcome,
    /// Which attempt that was, counting from one.
    pub attempt: usize,
}

/// The gate's standing for a run, or `None` when nothing has been gated.
///
/// Pure over the rows the caller already read, so nothing here opens a store, and
/// nothing here asks what time it is — the attempt number is a count of rows, not
/// an interval. `attempts` is expected in the ascending row order
/// `Store::gate_attempts` returns; the last row is the standing.
pub fn standing(attempts: &[GateAttempt]) -> Option<Standing> {
    let last = attempts.last()?;
    Some(Standing {
        phase: last.phase.clone(),
        outcome: last.outcome,
        attempt: attempts.len(),
    })
}

/// What a failing gate printed on `step`, or `None` if it printed nothing.
///
/// The output of a gate command is not in the gate row — `GateAttempt::detail`
/// carries a verdict's reasons — it arrives as sandbox events of kind
/// `gate_output`, already bounded in size by io-harness. Filtered by step so that
/// a retried gate's prompt carries the failure that caused the retry rather than
/// the first one, which is the difference between telling the model what is wrong
/// now and telling it what was wrong two turns ago.
///
/// More than one row for a step is joined in order: io-harness records one per
/// failing phase, and dropping all but the last would drop the half that explains
/// the other.
pub fn output(events: &[SandboxEvent], step: u32) -> Option<String> {
    let text: Vec<&str> = events
        .iter()
        .filter(|event| event.kind == "gate_output" && event.step == step)
        .filter_map(|event| event.detail.as_deref())
        .collect();
    if text.is_empty() {
        None
    } else {
        Some(text.join("\n"))
    }
}

/// Whether a failing gate has a retry left under `retries`.
///
/// **Deliberately not `GateOutcome::is_retryable`, which answers a different
/// question.** That method asks whether re-running the *same* criterion over the
/// *same* tree could honestly say something else, and for `Failed` the answer is
/// no. This asks whether it is worth driving *another turn* — after which the
/// tree is not the same, because the agent has been handed the failure and sent
/// back to work. `Errored` retries too: a gate that could not be evaluated has
/// judged nothing.
///
/// No attempts at all is not a retry: nothing has failed yet.
pub fn may_retry(attempts: &[GateAttempt], retries: u8) -> bool {
    let Some(standing) = standing(attempts) else {
        return false;
    };
    standing.outcome != GateOutcome::Passed && standing.attempt <= usize::from(retries)
}

/// The test command this repository proposes for itself, if it has one.
///
/// **This crate holds no list of marker filenames and no list of test commands,
/// and that is the point of the function.** Both lists live in
/// `io_harness::toolchain`, which is where the ecosystems are added; a copy here
/// would be a second list that drifts, and the way it drifts is by offering a
/// Rust command in a repository with no Rust in it. `Config::toolchain` then
/// layers whatever the operator wrote in their own `[toolchain.*]` section over
/// the detected answer, so an operator who runs a different test runner is offered
/// theirs rather than the ecosystem's default.
///
/// `None` means the repository said nothing about itself, and nothing is proposed.
/// There is deliberately no fallback: a proposal is a suggestion an operator
/// accepts with a keystroke, and one that is wrong is worse than one that is
/// absent, because it is accepted just as easily.
pub fn proposed_command(root: &Path, config: &Config) -> Option<Vec<String>> {
    let tuned = config.toolchain(io_harness::toolchain::detect(root)?);
    // An override can name an empty command, which is an operator saying this
    // ecosystem has no test step here. Proposing an empty argv would produce a
    // criterion with no program in it.
    (!tuned.test.is_empty()).then_some(tuned.test)
}

// **The reviewer is built in `crate::provider` and not here, and the gate that
// says so is `tests/provider.rs`.** Every vendor type must be constructed in
// exactly one place outside the wizard's handshake and the `/provider` panel, so
// that the interactive and the headless entry points cannot drift apart — and a
// reviewer built here would have been a second site with its own key resolution,
// which is precisely the drift that gate exists to catch. It caught this release
// writing one. See `crate::provider::reviewer`.

/// One sentence an operator can act on, for each way a section is refused.
///
/// ASCII throughout — no quotation marks that are not the typewriter kind, no
/// arrows, no ellipsis character. These render on the plain renderer, under
/// `NO_COLOR`, and through the ASCII glyph set, and a refusal that arrives as a
/// replacement character is a refusal nobody reads.
///
/// Each one names the key to change, because the operator is looking at the file.
impl fmt::Display for Refusal {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Refusal::Empty => f.write_str(
                "this gates section sets no criterion: name a command, a file or a \
                 rubric, or delete the section",
            ),
            Refusal::Ambiguous => f.write_str(
                "this gates section names more than one criterion: keep exactly one \
                 of command, file and rubric",
            ),
            Refusal::ReviewerMissing => f.write_str(
                "a rubric needs a model to answer it: set reviewer, or delete the \
                 rubric",
            ),
            Refusal::SelfReview { model } => write!(
                f,
                "the reviewer {model} is the model doing the work: name a different \
                 reviewer, or set allow_self_review = true to accept a model \
                 marking its own paper"
            ),
        }
    }
}
