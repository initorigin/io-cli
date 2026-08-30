//! **F1.** Every event kind has a disposition, and an unknown one is quiet.
//!
//! The table in `src/triage.rs` is what replaced the wildcard arm that committed
//! a Rust variant name in front of an operator for thirty-seven kinds. A table
//! the compiler cannot check — `EventKind` is `#[non_exhaustive]`, so no match
//! over it is ever provably exhaustive — needs something else to keep it true,
//! and this file is that something: it reads `pub enum EventKind` out of the
//! io-harness source this crate is locked to and fails by name when the two sets
//! differ.
//!
//! The second half is the agreement between the table and the renderer. A kind
//! marked `Line` with no arm behind it would be silent while the table says it
//! speaks, which is precisely the failure the old `STYLED` list had — a name
//! could be moved into it and change nothing at all.

mod support;

use std::time::Duration;

use io_cli::events::Events;
use io_cli::theme::DARK;
use io_cli::triage::{self, Disposition, TRIAGE};
use io_harness::{EventKind, RunEvent};

fn event(kind: EventKind) -> RunEvent {
    RunEvent::new(1, 1, kind)
}

/// The variant spelling of a snake-case kind name, as `EventKind` writes it.
fn variant(name: &str) -> String {
    name.split('_')
        .map(|word| {
            let mut chars = word.chars();
            match chars.next() {
                Some(first) => first.to_ascii_uppercase().to_string() + chars.as_str(),
                None => String::new(),
            }
        })
        .collect()
}

#[test]
fn the_table_names_every_kind_the_locked_harness_declares() {
    let declared = support::harness_event_kinds();
    let triaged: Vec<&str> = TRIAGE.iter().map(|(name, ..)| *name).collect();

    // **The two name sets are asserted before the count, and that order is the
    // point (0.33.0).** The count stood first until this release, so a pin that
    // grew a kind failed here saying a number had moved and stopped — the list
    // naming *which* kind was new never ran. That is the least useful half of
    // this test failing in place of the most useful one, and it is exactly what
    // happened when io-harness 0.72.0 added `QuestionsAsked`.
    let untriaged: Vec<&String> = declared
        .iter()
        .filter(|name| !triaged.contains(&name.as_str()))
        .collect();
    assert!(
        untriaged.is_empty(),
        "io-harness emits kinds with no disposition: {untriaged:?}. Decide for each whether it is \
         a line, a status field or silent, and add it to `triage::TRIAGE` with the route its fact \
         takes.",
    );

    let gone: Vec<&&str> = triaged
        .iter()
        .filter(|name| !declared.contains(&(**name).to_string()))
        .collect();
    assert!(
        gone.is_empty(),
        "these names are no longer io-harness event kinds: {gone:?}",
    );

    // And the count last, which now catches only what the two sets above cannot:
    // a harness that declared the same names twice, or a parser that stopped
    // reading the enum and handed back a shorter list that happens to be a subset.
    assert_eq!(
        declared.len(),
        52,
        "the locked io-harness declares fifty-two event kinds; found {}",
        declared.len(),
    );
}

/// **0.33.0 — the batched ask is a line, and a `Silent` row here would route its
/// only fact to nothing.**
///
/// `crate::intent::Answerer` implements `Responder::answer` and nothing else, so
/// io-harness's `answer_all` walks a batch and the overlay draws one question at a
/// time — exactly as it does for `question_asked`. "These arrived together" is the
/// whole content of this variant and no surface in this product says it.
///
/// Sabotage: copy `question_asked`'s `Silent` onto the row. That is the plausible
/// wrong answer, it is the one a reader of the table above would reach for, and it
/// fails here by name rather than as a sentence missing from a transcript.
#[test]
fn a_batched_ask_is_a_line_because_no_overlay_carries_the_batch() {
    assert_eq!(
        triage::disposition("questions_asked"),
        Some(Disposition::Line),
        "a batched ask reaches no surface unless the transcript draws it",
    );
    let route = triage::route("questions_asked").expect("the questions_asked row");
    assert!(
        route.contains("numbering"),
        "the route does not say what makes the batch legible as one: {route}",
    );
}

#[test]
fn the_table_has_no_duplicate_and_every_row_records_a_route() {
    let mut seen: Vec<&str> = Vec::new();
    for (name, _, route) in TRIAGE {
        assert!(
            !seen.contains(name),
            "{name} has two dispositions, and the first one found wins silently",
        );
        seen.push(name);
        assert!(
            !route.trim().is_empty(),
            "{name} has no route recorded, so nobody can check whether its fact reaches anyone",
        );
    }
    assert_eq!(seen.len(), 52);
}

/// A `Line` kind with no arm behind it is the old defect wearing the new table's
/// clothes: the disposition claims a designed line and the renderer commits
/// nothing.
#[test]
fn every_line_kind_has_an_arm_in_the_renderer() {
    let source = std::fs::read_to_string("src/events.rs")
        .expect("this crate's source is readable")
        .replace("\r\n", "\n");

    // **The renderer's match, sliced out before anything is searched — and the
    // twelve spaces alone are not enough to find it.**
    //
    // Two assertions live in this indentation. A bare `contains` was satisfied by
    // the variant's name appearing in a doc comment, which is how this test went
    // green through the whole of 0.14.0's sabotage pass with the `Dialed` arm
    // deleted: the `Sandbox` arm's prose names `EventKind::Dialed` to explain why
    // it draws nothing itself, and that mention alone answered the question. The
    // indentation closed that one. It did not close the second: `Events::commit`
    // opens with a *different* match on `&event.kind`, the one that decides
    // `self.thinking`, and its arms sit at the same twelve spaces — so it names
    // `EventKind::Reasoning` and `EventKind::Token` whatever the renderer below it
    // does, and deleting either arm from the renderer left this test green. The
    // comment that used to stand here claimed the indentation was unique to the
    // renderer's arms. It never was.
    //
    // So the renderer is identified by the statement immediately above it, which
    // belongs to no other match in the file, and the slice ends at the first
    // top-level item after `Events::commit`. What is searched is the renderer and
    // nothing else.
    const OPENS: &str = "let dash = theme.glyphs.dash;\n        match &event.kind {";
    const CLOSES: &str = "\nfn leader(";
    let from = source
        .find(OPENS)
        .expect("`Events::commit` opens its renderer match under `let dash`")
        + OPENS.len();
    let to = source[from..]
        .find(CLOSES)
        .map(|at| from + at)
        .expect("`fn leader` follows the impl `Events::commit` belongs to");
    let renderer = &source[from..to];

    // The slice starts *after* the thinking match, and that is the whole of what
    // makes the search honest. `self.thinking = true` is written in that match and
    // nowhere else, so finding it here would mean the slice had swallowed it and
    // every assertion below had gone back to being satisfiable from two arms it
    // does not care about.
    assert!(
        !renderer.contains("self.thinking = true"),
        "the slice reaches back into the match that decides `thinking`, whose arms \
         name `EventKind::Reasoning` and `EventKind::Token` at the same indentation",
    );
    assert!(
        renderer.contains("\n            EventKind::"),
        "the slice holds no match arms at all, so it is not the renderer",
    );

    for (name, disposition, _) in TRIAGE {
        if *disposition != Disposition::Line {
            continue;
        }
        // At the match arm's own indentation, within the renderer's own match: a
        // name with no arm behind it is the defect this test was written in 0.11.0
        // to close, and a mention in a comment is not an arm.
        let arm = format!("EventKind::{}", variant(name));
        let declared = format!("\n            {arm}");
        assert!(
            renderer.contains(&declared),
            "{name} is triaged as a line and `{arm}` has no arm in the renderer in \
             src/events.rs, so it commits nothing at all — a mention in a comment is not an \
             arm, and neither is the arm in the match that decides `thinking`",
        );
    }
}

/// 0.14.0 F7, F8 and F9 — the three rows this release promoted, and the one
/// sabotage all three of them take.
///
/// **None of these kinds was ever untriaged**, whatever the contract said before
/// `US-IO-CLI-0.14.0-I01` corrected it. All three were in the table from 0.11.0
/// and all three were deliberately `Silent`, so `Status::unknown` never moved for
/// any of them and could not have: `Events::undesigned` increments only for a
/// name the table does not hold. Restoring `Disposition::Silent` on any one row
/// is therefore the sabotage each criterion actually has, and this is where it
/// fails first — before the renderer's own tests, and naming the row rather than
/// the sentence that went missing because of it.
#[test]
fn the_three_kinds_this_release_draws_are_lines_rather_than_silent() {
    for name in ["dialed", "sandbox", "stalled"] {
        assert_eq!(
            triage::disposition(name),
            Some(Disposition::Line),
            "{name} is drawn by this release, so a `Silent` row here is a line gone from the \
             scrollback with the unknown counter still at zero and nothing else saying so",
        );
    }
}

/// **0.24.0.** The `sandbox` route said `gate_phase_failed` and `gate_output`
/// "belong to a verification gate no session has until 0.24.0". This is that
/// release.
///
/// The route column is the only evidence anybody has that a fact reaches its
/// reader, so a sentence in it that has stopped being true is worse than a blank
/// one: a reviewer checking whether a kind is drawn reads this and stops. The
/// seam was left open here deliberately, and closing it means closing the
/// sentence that described it as open.
///
/// Sabotage: restore either retired phrase to the row. Only this fails, and it
/// fails on the claim rather than on the sentence around it — which is the part
/// an author rewrites while leaving the claim standing.
#[test]
fn the_route_column_no_longer_says_a_session_has_no_verification_gate() {
    let sandbox = triage::route("sandbox").expect("the sandbox row");
    for retired in ["no session has", "until 0.24.0"] {
        assert!(
            !sandbox.contains(retired),
            "the sandbox route still says {retired:?}, which stopped being true in this \
             release: {sandbox}",
        );
    }
    // And says where the two of them go now, which is the whole job of this
    // column: a `Line` disposition covers seven kinds at once, so the row is the
    // only place that can record which of them the arm actually draws.
    for kind in ["gate_phase_failed", "gate_output"] {
        assert!(
            sandbox.contains(kind),
            "{kind} is drawn from this release and the route does not say so: {sandbox}",
        );
    }
    // `dial` is the one kind of the seven that still reaches no line, so the row
    // has to go on naming the event that draws it instead.
    assert!(sandbox.contains("dialed"), "{sandbox}");

    // The asymmetry a reader of this table cannot work out from the enum: a
    // review that never happened emits nothing at all, so a missing verdict line
    // is a criterion that did not answer rather than one that said yes.
    let reviewed = triage::route("reviewed").expect("the reviewed row");
    assert!(
        reviewed.contains("Errored"),
        "the reviewed route does not record that a review which never ran emits \
         nothing here: {reviewed}",
    );
}

/// The other direction, in behaviour rather than in source: a kind whose fact
/// belongs to a status field or to another event commits no line.
///
/// Every kind here is constructed and driven through the real renderer. The two
/// that are not — `todo_wrote` and the rest of the `Line` set — are covered by
/// `tests/events.rs`, which asserts what they say rather than that they say
/// something.
#[test]
fn a_status_or_silent_kind_commits_nothing() {
    let quiet = vec![
        EventKind::ToolCall {
            name: "read_file".into(),
            target: "notes.txt".into(),
        },
        EventKind::SpendDraw {
            tokens: 12,
            remaining: Some(400),
        },
        EventKind::Fleet {
            tier: 1,
            working: 2,
            queued: 0,
            done: 1,
        },
        EventKind::PlanProposed {
            plan_id: 4,
            steps: Vec::new(),
        },
        EventKind::Mcp {
            server: "docs".into(),
            tool: None,
            ok: None,
            millis: None,
            tools: Some(4),
        },
        EventKind::HandlePolled {
            handle: 3,
            bytes: 0,
        },
        EventKind::Routed {
            from: "a".into(),
            to: "b".into(),
            why: "cheaper".into(),
        },
        EventKind::PluginLoaded {
            plugin: "acme".into(),
            contributions: vec!["skills".into()],
        },
        EventKind::LspStarted {
            server: "rust-analyzer".into(),
            root: "/w".into(),
            ready_ms: 900,
        },
        EventKind::BrowserStarted {
            binary: "chrome".into(),
            headless: true,
            ready_ms: 120,
        },
        EventKind::BrowserNavigated {
            host: "example.com:443".into(),
            permitted: true,
        },
        EventKind::PluginDropped {
            plugin: "acme".into(),
            why: "manifest unreadable".into(),
        },
        EventKind::Rewound {
            files: 2,
            memory: 0,
            queued: 0,
        },
        EventKind::Reverted {
            undid_step: 3,
            files: 1,
        },
        EventKind::Answered { turn_id: 7 },
        EventKind::Compacted {
            through_step: 12,
            before_tokens: 19_400,
            after_tokens: 5_100,
        },
        EventKind::CacheMarked {
            through_step: 13,
            prefix_bytes: 8_412,
        },
        EventKind::PromptComposed {
            family: "anthropic".into(),
            bytes: 2_140,
            source: "builtin".into(),
            boundary: true,
            instructions: false,
        },
        EventKind::Contained {
            mode: "workspace-write".into(),
            backend: "macos-sandbox-exec".into(),
            roots: 1,
        },
    ];

    let mut events = Events::new(DARK);
    for kind in quiet {
        let name = io_cli::events::kind_name(&kind);
        let disposition = triage::disposition(&name)
            .unwrap_or_else(|| panic!("{name} is not in the triage table"));
        assert_ne!(
            disposition,
            Disposition::Line,
            "{name} is triaged as a line; this test is for the ones that are not",
        );
        let lines = events.event(&event(kind), Duration::ZERO);
        assert!(
            lines.is_empty(),
            "{name} committed {} line(s) while its disposition says its fact goes elsewhere: {}",
            lines.len(),
            triage::route(&name).unwrap_or_default(),
        );
    }
    assert_eq!(
        events.unknown(),
        0,
        "every kind above is in the table, so none of them may be counted as unknown",
    );
}

/// The branch that cannot be reached with a constructed event, because a kind
/// with no disposition is by definition one the locked harness does not declare.
#[test]
fn a_kind_with_no_disposition_is_counted_rather_than_printed() {
    let mut events = Events::new(DARK);

    let lines = events.undesigned("a_kind_io_harness_has_not_invented_yet");
    assert!(
        lines.is_empty(),
        "an unknown kind must commit nothing: {lines:?}",
    );
    assert_eq!(events.unknown(), 1);

    // A known kind arriving here — a `Status` one, or a `Line` one whose arm
    // declined this particular event — is not an unknown and must not be counted
    // as one.
    let lines = events.undesigned("mcp");
    assert!(lines.is_empty());
    assert_eq!(
        events.unknown(),
        1,
        "a kind with a disposition was counted as one this release has never seen",
    );
}

#[test]
fn the_dispositions_are_the_three_the_contract_names() {
    let lines = TRIAGE
        .iter()
        .filter(|(_, d, _)| *d == Disposition::Line)
        .count();
    let status = TRIAGE
        .iter()
        .filter(|(_, d, _)| *d == Disposition::Status)
        .count();
    let silent = TRIAGE
        .iter()
        .filter(|(_, d, _)| *d == Disposition::Silent)
        .count();
    assert_eq!(
        lines + status + silent,
        52,
        "every kind is exactly one of the three: {lines} lines, {status} status, {silent} silent",
    );
    // The release's own claim, and the reason it exists: most kinds are not
    // worth a line, and until 0.11.0 thirty-seven of them got one anyway.
    assert!(
        status + silent >= 20,
        "if almost everything is a line, nothing was triaged: {status} status, {silent} silent",
    );
}

/// **0.27.0 — the one silence that had no route, and now has a line.**
///
/// `speculated` routed to `io exec --json` and the durable trace, and both of
/// those are places somebody goes deliberately, afterwards, already suspecting
/// something. A route is meant to name where the fact reaches an operator who
/// was *not* looking for it. The other eight silences reviewed in the same pass
/// keep theirs, because they are better arguments than drawing would be — see
/// `US-IO-CLI-0.27.0-I04`.
#[test]
fn f9_a_discarded_read_is_drawn_and_a_perfect_one_is_not() {
    let mut events = Events::new(DARK);

    assert_eq!(
        triage::disposition("speculated"),
        Some(Disposition::Line),
        "speculated is a line since 0.27.0",
    );

    let lines = events.event(
        &event(EventKind::Speculated {
            started: 3,
            used: 1,
            discarded: 2,
        }),
        Duration::ZERO,
    );
    let text = lines
        .iter()
        .flat_map(|line| line.spans.iter().map(|span| span.content.as_ref()))
        .collect::<String>();
    assert!(text.contains("read ahead"), "{text}");
    assert!(text.contains('3'), "started is on the line: {text}");
    assert!(text.contains('2'), "and so is what was thrown away: {text}");

    // **A step that speculated perfectly has nothing to report.** io-harness
    // emits this whenever `started > 0`, so without the guard every transcript
    // would carry a line per step saying nothing went wrong — and a line that is
    // always there is a line nobody reads.
    let quiet = events.event(
        &event(EventKind::Speculated {
            started: 4,
            used: 4,
            discarded: 0,
        }),
        Duration::ZERO,
    );
    assert!(
        quiet.is_empty(),
        "nothing was discarded, so there is nothing to say: {quiet:?}",
    );
    assert_eq!(
        events.unknown(),
        0,
        "declining an event is not the same as not knowing the kind",
    );
}

/// **Every `Silent` route names a surface this product actually ships.**
///
/// A route is the whole justification for a silence: the fact is not on screen
/// *because it reaches the operator another way*. Until this release that claim
/// was prose nobody checked, and `speculated` is what a route looks like when it
/// is wrong — it named two places a person only goes deliberately, afterwards.
///
/// So every route must name a command this build registers, or one of the two
/// machine surfaces. A route naming a command that does not exist is a silence
/// with nothing behind it.
#[test]
fn f9_every_silent_route_names_a_surface_that_exists() {
    let commands: Vec<&str> = io_cli::commands::COMMANDS
        .iter()
        .map(|(name, _)| *name)
        .collect();

    for (name, disposition, route) in triage::TRIAGE {
        if *disposition != Disposition::Silent {
            continue;
        }
        assert!(
            !route.is_empty(),
            "{name} is silent and says nothing about why"
        );

        // A route may name a command, one of the two machine surfaces, or a
        // surface of this interface that is not a slash command — the overlay, a
        // committed summary, the answer itself. The first is the one that can go
        // stale, so it is the one that is checked: any `/word` a route names must
        // be a command this build registers.
        for word in route.split_whitespace() {
            let candidate = word.trim_matches(|c: char| !c.is_ascii_alphanumeric() && c != '/');
            if let Some(slash) = candidate.strip_prefix('/').map(|rest| format!("/{rest}")) {
                if slash.len() > 1 {
                    assert!(
                        commands.contains(&slash.as_str()),
                        "{name} routes to {slash}, which this build does not register",
                    );
                }
            }
        }
    }
}
