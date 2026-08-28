//! F2, F3, F4 and F5 — what `[app.io-cli.gates]` resolves to, what it refuses,
//! what the repository proposes for itself, and what a file criterion actually
//! asserts.
//!
//! Every assertion here is written against the way the implementation would be
//! wrong rather than the way it is right, because the shapes this module gets
//! wrong all *pass* a naive test:
//!
//! * an existence criterion built on a reader that answers `Ok("")` for a file
//!   that is not there passes on a file nobody wrote, and a test that only checks
//!   the happy path — the file exists, the gate says yes — never notices;
//! * a command criterion that hardcodes exit zero satisfies every test written
//!   with the default;
//! * a toolchain proposal that falls back to a Rust command is correct in this
//!   repository and wrong in every other one, so it has to be asserted against a
//!   directory that is not this repository;
//! * a refusal that is silently `Ok(None)` looks exactly like a section nobody
//!   wrote.
//!
//! So the file criterion is asserted against a *missing* file and against an
//! *empty* one separately, the exit status is asserted at a value the default is
//! not, and the toolchain proposal is asserted against four temporary directories
//! of which one holds nothing at all.

use std::path::{Path, PathBuf};

use io_cli::gates::{self, Criterion, Refusal, Settings};
use io_harness::{Config, GateAttempt, GateOutcome, SandboxEvent, Verification};

/// The model the session would run with. Only the self-review refusal compares
/// against it, and only by equality.
const TURN_MODEL: &str = "vendor/worker-model";

/// A section with one key set, everything else absent.
fn section() -> Settings {
    Settings::default()
}

/// One recorded gate evaluation. `at` is a stored string this crate never parses
/// and never compares, so a fixture can leave it empty.
fn attempt(step: u32, phase: &str, outcome: GateOutcome) -> GateAttempt {
    GateAttempt {
        id: 1,
        step,
        phase: phase.into(),
        outcome,
        detail: String::new(),
        at: String::new(),
    }
}

/// An empty configuration: no `[toolchain.*]` overrides, so a proposal is exactly
/// what the repository said about itself.
fn plain() -> Config {
    Config::from_toml("").expect("an empty configuration parses")
}

// ---------------------------------------------------------------------------
// Resolving the section
// ---------------------------------------------------------------------------

/// A section nobody wrote is not a refusal, and a section somebody started is.
///
/// The pair is the test. Answering both with `Ok(None)` would be the natural
/// implementation — "no kind is named, so there is no gate" — and it turns a
/// half-written gate into a silently ungated session.
#[test]
fn f2_an_absent_section_resolves_to_nothing_and_a_half_written_one_is_refused() {
    assert_eq!(
        section().criterion(TURN_MODEL),
        Ok(None),
        "a section with every key unset asked for no gate"
    );

    for started in [
        Settings {
            retries: Some(2),
            ..section()
        },
        Settings {
            contains: Some("ok".into()),
            ..section()
        },
        Settings {
            reviewer: Some("vendor/judge".into()),
            ..section()
        },
        Settings {
            expect_exit: Some(1),
            ..section()
        },
        Settings {
            allow_self_review: Some(true),
            ..section()
        },
    ] {
        assert_eq!(
            started.criterion(TURN_MODEL),
            Err(Refusal::Empty),
            "{started:?} names no kind but is plainly meant to be a gate, and \
             resolving it to nothing would gate the session on nothing"
        );
    }
}

/// Two kinds is a refusal and never a precedence rule.
///
/// Asserted for every pair and for all three, because a precedence rule written
/// as an `if` chain refuses none of them and quietly gates the turn on whichever
/// branch happened to come first.
#[test]
fn f2_more_than_one_kind_is_refused_rather_than_ordered() {
    let command = Some(vec!["cargo".into(), "test".into()]);
    let file = Some(PathBuf::from("REPORT.md"));
    let rubric = Some("is it done".into());

    for both in [
        Settings {
            command: command.clone(),
            file: file.clone(),
            ..section()
        },
        Settings {
            command: command.clone(),
            rubric: rubric.clone(),
            reviewer: Some("vendor/judge".into()),
            ..section()
        },
        Settings {
            file: file.clone(),
            rubric: rubric.clone(),
            reviewer: Some("vendor/judge".into()),
            ..section()
        },
        Settings {
            command,
            file,
            rubric,
            reviewer: Some("vendor/judge".into()),
            ..section()
        },
    ] {
        assert_eq!(
            both.criterion(TURN_MODEL),
            Err(Refusal::Ambiguous),
            "{both:?} names more than one criterion and there is no right answer"
        );
    }
}

/// A rubric with no reviewer is refused here, not at run start.
///
/// io-harness answers this with `Error::Config` before the first billed call, so
/// an implementation that resolves it to a `Review` criterion has moved the
/// failure from a keystroke the operator is looking at to every turn afterwards.
#[test]
fn f5_a_rubric_with_no_reviewer_is_refused() {
    let written = Settings {
        rubric: Some("the change is covered by a test".into()),
        ..section()
    };
    assert_eq!(written.criterion(TURN_MODEL), Err(Refusal::ReviewerMissing));

    // And the refusal survives `allow_self_review`, which is about *which* model
    // judges and cannot supply one.
    let permissive = Settings {
        rubric: Some("the change is covered by a test".into()),
        allow_self_review: Some(true),
        ..section()
    };
    assert_eq!(
        permissive.criterion(TURN_MODEL),
        Err(Refusal::ReviewerMissing),
        "allowing self-review does not name a reviewer"
    );
}

/// A reviewer that is the model doing the work is refused unless it was asked for.
///
/// The three cases together are the test: refused by default, allowed when the
/// operator said so, and untouched when the reviewer is somebody else. An
/// implementation that never fires the refusal passes the second and third alone.
#[test]
fn f5_a_reviewer_equal_to_the_turn_model_is_refused_unless_allowed() {
    let judging_itself = Settings {
        rubric: Some("the change is covered by a test".into()),
        reviewer: Some(TURN_MODEL.into()),
        ..section()
    };
    assert_eq!(
        judging_itself.criterion(TURN_MODEL),
        Err(Refusal::SelfReview {
            model: TURN_MODEL.into(),
        }),
        "a model marking its own paper is a decision, not a default"
    );

    let asked_for = Settings {
        allow_self_review: Some(true),
        ..judging_itself.clone()
    };
    assert_eq!(
        asked_for.criterion(TURN_MODEL),
        Ok(Some(Criterion::Review {
            rubric: "the change is covered by a test".into(),
            reviewer: TURN_MODEL.into(),
            allow_self_review: true,
        })),
        "the operator said so, and the flag has to reach the criterion"
    );

    let second_model = Settings {
        reviewer: Some("vendor/judge-model".into()),
        ..judging_itself.clone()
    };
    assert_eq!(
        second_model.criterion(TURN_MODEL),
        Ok(Some(Criterion::Review {
            rubric: "the change is covered by a test".into(),
            reviewer: "vendor/judge-model".into(),
            allow_self_review: false,
        }))
    );

    // A caller that does not yet know which model will run cannot be told there
    // is a clash, and must not be told there is none either — it is told nothing,
    // which is what an empty model means.
    assert!(
        judging_itself.criterion("").is_ok(),
        "with no turn model there is nothing to compare the reviewer against"
    );
}

// ---------------------------------------------------------------------------
// Mapping a criterion onto the dependency's type
// ---------------------------------------------------------------------------

/// The exit status the operator wrote is the exit status the gate carries.
///
/// Asserted at one rather than at zero, because zero is the default and a mapping
/// that ignores the key entirely is indistinguishable from a correct one at that
/// value. The default is asserted separately, in the same test, so neither half
/// can pass on its own.
#[test]
fn f2_a_command_criterion_carries_the_exit_status_it_was_given() {
    let lint_found_nothing = Settings {
        command: Some(vec!["npm".into(), "run".into(), "lint".into()]),
        expect_exit: Some(1),
        ..section()
    };
    let criterion = lint_found_nothing
        .criterion(TURN_MODEL)
        .expect("a command criterion resolves")
        .expect("and it is a criterion");
    assert_eq!(
        criterion,
        Criterion::Command {
            argv: vec!["npm".into(), "run".into(), "lint".into()],
            expect_exit: 1,
        }
    );

    let Verification::Command { argv, expect_exit } = criterion.verification() else {
        panic!("a command criterion maps to a command verification");
    };
    assert_eq!(argv, ["npm", "run", "lint"]);
    assert_eq!(expect_exit, 1, "a hardcoded zero would pass every other test");

    let defaulted = Settings {
        command: Some(vec!["make".into(), "check".into()]),
        ..section()
    };
    assert_eq!(
        defaulted
            .criterion(TURN_MODEL)
            .expect("it resolves")
            .expect("to a criterion"),
        Criterion::Command {
            argv: vec!["make".into(), "check".into()],
            expect_exit: 0,
        },
        "an unwritten exit status is zero"
    );

    // A command gate is io-harness's to run, whatever else this crate can check.
    assert!(!criterion.checked_here());
    assert_eq!(criterion.satisfied_in(Path::new("/nonexistent")), None);
}

/// A file criterion with a needle is the dependency's contains criterion.
#[test]
fn f4_a_file_with_a_needle_maps_to_the_contains_criterion() {
    let written = Settings {
        file: Some(PathBuf::from("docs/REPORT.md")),
        contains: Some("## Findings".into()),
        ..section()
    };
    let criterion = written
        .criterion(TURN_MODEL)
        .expect("it resolves")
        .expect("to a criterion");

    let Verification::WorkspaceFileContains { file, needle } = criterion.verification() else {
        panic!("a file criterion with a needle maps to the contains verification");
    };
    assert_eq!(file, PathBuf::from("docs/REPORT.md"));
    assert_eq!(needle, "## Findings");

    assert!(
        !criterion.checked_here(),
        "the dependency can express this one, so it runs it"
    );
    assert_eq!(criterion.satisfied_in(Path::new("/nonexistent")), None);
}

/// A file criterion with no needle is never a contains criterion with no text.
///
/// This is the whole of F4's sabotage. The dependency reads its file with a
/// reader that answers the empty string for a file that is not there, and every
/// string contains the empty needle — so the obvious mapping is a gate that can
/// never fail, on a file that need never exist.
#[test]
fn f4_existence_is_never_an_empty_needle() {
    let written = Settings {
        file: Some(PathBuf::from("REPORT.md")),
        ..section()
    };
    let criterion = written
        .criterion(TURN_MODEL)
        .expect("it resolves")
        .expect("to a criterion");
    assert_eq!(
        criterion,
        Criterion::File {
            file: PathBuf::from("REPORT.md"),
            contains: None,
        }
    );

    if let Verification::WorkspaceFileContains { needle, .. } = criterion.verification() {
        panic!(
            "existence was mapped to a contains criterion with needle {needle:?}, \
             which a file that does not exist satisfies"
        );
    }
    assert!(
        criterion.checked_here(),
        "the dependency cannot express existence, so this crate must own it, and \
         a caller has to be able to ask"
    );
}

/// Existence tells a missing file from an empty one.
///
/// The empty file is the assertion that matters. Every reader in the dependency
/// that answers `Ok` for a missing file answers `Ok` for an empty one too, so a
/// test that only checks a file with text in it passes on both readers and
/// distinguishes nothing.
#[test]
fn f4_existence_tells_a_missing_file_from_an_empty_one() {
    let root = tempfile::tempdir().expect("a temporary directory");
    let criterion = Criterion::File {
        file: PathBuf::from("REPORT.md"),
        contains: None,
    };

    assert_eq!(
        criterion.satisfied_in(root.path()),
        Some(false),
        "nobody wrote the file, so the gate has to say no"
    );

    std::fs::write(root.path().join("REPORT.md"), "").expect("an empty file is written");
    assert_eq!(
        criterion.satisfied_in(root.path()),
        Some(true),
        "the file exists; a reader that could not tell this from the case above \
         would have passed the assertion before it too"
    );

    // A directory at the path is not a file that was written.
    let directory = Criterion::File {
        file: PathBuf::from("docs"),
        contains: None,
    };
    std::fs::create_dir(root.path().join("docs")).expect("a directory is created");
    assert_eq!(
        directory.satisfied_in(root.path()),
        Some(false),
        "an operator who named a file did not mean a directory"
    );

    // And the criterion cannot be used to read outside the workspace.
    let escaping = Criterion::File {
        file: PathBuf::from("../../etc/hosts"),
        contains: None,
    };
    assert_eq!(
        escaping.satisfied_in(root.path()),
        Some(false),
        "a path that climbs out of the workspace is refused, not followed"
    );
}

/// A review criterion carries the rubric and the flag, and not the reviewer.
///
/// The reviewer is attached to the contract separately, through
/// `TaskContract::with_reviewer`, because the dependency's `Review` variant has
/// no field for it. A mapping that dropped `allow_self_review` would re-refuse at
/// run start the configuration this module just accepted.
#[test]
fn f5_a_review_criterion_carries_the_rubric_and_the_flag() {
    let criterion = Criterion::Review {
        rubric: "no public item lost its documentation".into(),
        reviewer: "vendor/judge-model".into(),
        allow_self_review: true,
    };
    let Verification::Review {
        rubric,
        allow_self_review,
    } = criterion.verification()
    else {
        panic!("a review criterion maps to a review verification");
    };
    assert_eq!(rubric, "no public item lost its documentation");
    assert!(
        allow_self_review,
        "dropping the flag would make the run refuse what this module accepted"
    );
    assert!(!criterion.checked_here());
}

/// Every criterion describes itself, including the one with no verification.
#[test]
fn f7_every_criterion_says_what_it_asks() {
    let existence = Criterion::File {
        file: PathBuf::from("REPORT.md"),
        contains: None,
    };
    let described = existence.describe();
    assert!(
        described.contains("REPORT.md") && described.contains("exist"),
        "an existence criterion has no verification to delegate to and must still \
         be able to tell a retried turn what it wants: {described:?}"
    );
    assert!(
        !described.contains("no automated check"),
        "delegating this one to the dependency would describe it as ungated"
    );

    let command = Criterion::Command {
        argv: vec!["make".into(), "check".into()],
        expect_exit: 0,
    };
    assert!(
        command.describe().contains("make check"),
        "the words a retried turn reads are the words it was judged by"
    );
}

// ---------------------------------------------------------------------------
// Retries
// ---------------------------------------------------------------------------

/// The retry budget defaults to one and zero is honoured as zero.
///
/// Zero is the half that needs asserting: an implementation reaching for
/// `unwrap_or` on a count and then treating a falsy value as unset turns "do not
/// retry" into "retry once", which spends a turn against a real model the
/// operator explicitly declined.
#[test]
fn f7_the_retry_budget_defaults_to_one_and_zero_means_zero() {
    assert_eq!(section().retries(), 1);
    assert_eq!(
        Settings {
            retries: Some(0),
            ..section()
        }
        .retries(),
        0,
        "an operator who wrote zero asked for no retry at all"
    );
    assert_eq!(
        Settings {
            retries: Some(3),
            ..section()
        }
        .retries(),
        3
    );

    let failed = [attempt(1, "command", GateOutcome::Failed)];
    assert!(
        !gates::may_retry(&failed, 0),
        "zero retries must not buy one"
    );
    assert!(gates::may_retry(&failed, 1), "one retry buys the second turn");

    let twice = [
        attempt(1, "command", GateOutcome::Failed),
        attempt(2, "command", GateOutcome::Failed),
    ];
    assert!(
        !gates::may_retry(&twice, 1),
        "one retry buys one further turn, not one further failure"
    );

    assert!(
        !gates::may_retry(&[attempt(1, "command", GateOutcome::Passed)], 3),
        "a gate that passed is not retried however much budget is left"
    );
    assert!(
        gates::may_retry(&[attempt(1, "review", GateOutcome::Errored)], 1),
        "a gate that could not be evaluated has judged nothing"
    );
    assert!(
        !gates::may_retry(&[], 3),
        "nothing has failed, so there is nothing to retry"
    );
}

/// The standing is the last attempt, counted from one.
#[test]
fn f6_the_standing_is_the_last_attempt_and_its_number() {
    assert_eq!(gates::standing(&[]), None, "an ungated run has no standing");

    let attempts = [
        attempt(1, "command", GateOutcome::Failed),
        attempt(2, "command", GateOutcome::Passed),
    ];
    let standing = gates::standing(&attempts).expect("two attempts have a standing");
    assert_eq!(standing.outcome, GateOutcome::Passed, "the last one, not the first");
    assert_eq!(standing.attempt, 2, "counted from one, so two attempts is two");
    assert_eq!(standing.phase, "command");
}

/// A failing gate's output is read per step, so a retry carries its own failure.
#[test]
fn f7_the_recorded_output_is_the_one_for_the_step_asked_about() {
    let events = [
        SandboxEvent::gate_output(7, 1, "first failure"),
        SandboxEvent::exec(7, 1, "macos-sandbox-exec", "cargo test"),
        SandboxEvent::gate_output(7, 2, "second failure"),
        SandboxEvent::gate_output(7, 2, "and its second phase"),
    ];

    assert_eq!(
        gates::output(&events, 2).as_deref(),
        Some("second failure\nand its second phase"),
        "the retry is told what is wrong now, and no phase of it is dropped"
    );
    assert_eq!(gates::output(&events, 1).as_deref(), Some("first failure"));
    assert_eq!(
        gates::output(&events, 3),
        None,
        "a step that printed nothing is nothing, not an empty string"
    );
    assert_eq!(gates::output(&[], 1), None);
}

// ---------------------------------------------------------------------------
// What the repository proposes for itself
// ---------------------------------------------------------------------------

/// The proposal is the repository's own, for four different repositories.
///
/// The bare directory is the assertion the other three cannot make: a fallback to
/// a Rust command is correct in this repository and in the first case below, and
/// wrong everywhere else, so only a directory that says nothing about itself
/// catches it.
#[test]
fn f3_the_test_command_is_proposed_by_the_repository_and_never_defaulted() {
    let rust = tempfile::tempdir().expect("a temporary directory");
    std::fs::write(rust.path().join("Cargo.toml"), "[package]\nname = \"x\"\n")
        .expect("the marker is written");
    assert_eq!(
        gates::proposed_command(rust.path(), &plain()),
        Some(vec!["cargo".into(), "test".into()])
    );

    let node = tempfile::tempdir().expect("a temporary directory");
    std::fs::write(node.path().join("package.json"), "{}").expect("the marker is written");
    std::fs::write(node.path().join("pnpm-lock.yaml"), "").expect("the lockfile is written");
    assert_eq!(
        gates::proposed_command(node.path(), &plain()),
        Some(vec!["pnpm".into(), "test".into()]),
        "the lockfile beside the marker chooses the manager, and this crate holds \
         no opinion about which"
    );

    let python = tempfile::tempdir().expect("a temporary directory");
    std::fs::write(python.path().join("pyproject.toml"), "[project]\nname = \"x\"\n")
        .expect("the marker is written");
    let proposed = gates::proposed_command(python.path(), &plain())
        .expect("a Python project proposes a test command");
    assert!(
        proposed.first().is_some_and(|program| program != "cargo"),
        "a Rust command was offered in a repository with no Rust in it: {proposed:?}"
    );
    assert_eq!(proposed, ["python", "-m", "pytest"]);

    let bare = tempfile::tempdir().expect("a temporary directory");
    assert_eq!(
        gates::proposed_command(bare.path(), &plain()),
        None,
        "a repository that said nothing about itself gets no proposal; a fallback \
         here is a wrong command an operator accepts with one keystroke"
    );
}

/// The operator's own override wins over what was detected.
#[test]
fn f3_an_operator_override_replaces_the_detected_command() {
    let rust = tempfile::tempdir().expect("a temporary directory");
    std::fs::write(rust.path().join("Cargo.toml"), "[package]\nname = \"x\"\n")
        .expect("the marker is written");

    let tuned = Config::from_toml(
        "\
[toolchain.cargo]
test = [\"cargo\", \"nextest\", \"run\"]
",
    )
    .expect("the fixture parses");

    assert_eq!(
        gates::proposed_command(rust.path(), &tuned),
        Some(vec!["cargo".into(), "nextest".into(), "run".into()]),
        "detection is layered under the file, not instead of it"
    );

    let silenced = Config::from_toml(
        "\
[toolchain.cargo]
test = []
",
    )
    .expect("the fixture parses");
    assert_eq!(
        gates::proposed_command(rust.path(), &silenced),
        None,
        "an empty override is an operator saying there is no test step here, and \
         an empty argv is a criterion with no program in it"
    );
}

/// The module holds no list of marker files and no list of test commands.
///
/// The sweep is over the *comment-stripped* source: this crate argues its design
/// in prose, and prose naming what it deliberately does not hold is a module
/// explaining itself rather than a module hiding a list. Code is what is checked.
///
/// A marker name appearing here is the copy of the dependency's table that drifts
/// — and the way it drifts is by proposing an ecosystem's command in a repository
/// that stopped being that ecosystem two commits ago.
#[test]
fn f3_the_module_names_no_marker_file_and_no_test_command() {
    let path = Path::new(env!("CARGO_MANIFEST_DIR")).join("src/gates.rs");
    let source = std::fs::read_to_string(&path).expect("the module is readable");
    let code = strip_comments(&source);

    for marker in [
        "Cargo.toml",
        "package.json",
        "pyproject.toml",
        "requirements.txt",
        "go.mod",
        "deno.json",
        "Gemfile",
        "pom.xml",
        "build.gradle",
        "mix.exs",
        "composer.json",
        "Package.swift",
        "CMakeLists.txt",
        "Makefile",
        ".csproj",
    ] {
        assert!(
            !code.contains(marker),
            "src/gates.rs names the marker file {marker:?} in code; detection \
             belongs to io_harness::toolchain, and a second copy of that table is \
             a table that drifts"
        );
    }

    for command in [
        "cargo\", \"test",
        "npm\", \"test",
        "pnpm\", \"test",
        "go\", \"test",
        "pytest",
        "dotnet",
    ] {
        assert!(
            !code.contains(command),
            "src/gates.rs spells the test command {command:?} itself, which is the \
             fallback F3 forbids"
        );
    }

    // The stripper has to actually strip, or the assertions above are vacuous for
    // any module that mentions a marker in prose.
    assert!(
        strip_comments("let x = 1; // Cargo.toml\n").trim() == "let x = 1;",
        "the stripper removes a trailing line comment"
    );
    assert!(
        !strip_comments("//! about Cargo.toml\ncode\n").contains("Cargo.toml"),
        "the stripper removes a module doc line"
    );
}

/// Everything from a `//` to the end of the line, and every block comment.
///
/// Deliberately naive about a `//` inside a string literal: the only way that
/// errs is by stripping more than it should, which can weaken this sweep and can
/// never make it fail on a module that is correct. A stripper that tried to parse
/// Rust strings here would be a parser nobody maintains guarding a one-line rule.
fn strip_comments(source: &str) -> String {
    let mut out = String::with_capacity(source.len());
    let mut rest = source;
    // Block comments first, so a `//` inside one cannot terminate a line early.
    while let Some(start) = rest.find("/*") {
        out.push_str(&rest[..start]);
        match rest[start..].find("*/") {
            Some(end) => rest = &rest[start + end + 2..],
            None => rest = "",
        }
    }
    out.push_str(rest);

    out.lines()
        .map(|line| match line.find("//") {
            Some(at) => &line[..at],
            None => line,
        })
        .collect::<Vec<_>>()
        .join("\n")
}

// ---------------------------------------------------------------------------
// What the operator is told
// ---------------------------------------------------------------------------

/// Every refusal has a sentence, it is ASCII, and it names the key to change.
///
/// ASCII because these render through the plain renderer, under `NO_COLOR` and
/// through the ASCII glyph set — a refusal that arrives as a replacement
/// character is a refusal nobody reads. Naming the key because the operator is
/// looking at the file when they get it.
#[test]
fn f5_every_refusal_has_an_ascii_sentence_naming_the_key_to_change() {
    let sentences = [
        (Refusal::Empty, vec!["command", "file", "rubric"]),
        (Refusal::Ambiguous, vec!["command", "file", "rubric"]),
        (Refusal::ReviewerMissing, vec!["reviewer", "rubric"]),
        (
            Refusal::SelfReview {
                model: TURN_MODEL.into(),
            },
            vec![TURN_MODEL, "allow_self_review"],
        ),
    ];

    for (refusal, keys) in sentences {
        let text = refusal.to_string();
        assert!(
            text.is_ascii(),
            "{refusal:?} renders as {text:?}, which is not ASCII and will not \
             survive the ASCII glyph set"
        );
        assert!(
            !text.is_empty() && text.len() > 20,
            "{refusal:?} renders as {text:?}, which tells an operator nothing"
        );
        for key in keys {
            assert!(
                text.contains(key),
                "{refusal:?} renders as {text:?} without naming {key:?}, and the \
                 operator is looking at the file"
            );
        }
    }
}
