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
    assert_eq!(
        declared.len(),
        51,
        "the locked io-harness declares fifty-one event kinds; found {}",
        declared.len(),
    );

    let triaged: Vec<&str> = TRIAGE.iter().map(|(name, ..)| *name).collect();

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
    assert_eq!(seen.len(), 51);
}

/// A `Line` kind with no arm behind it is the old defect wearing the new table's
/// clothes: the disposition claims a designed line and the renderer commits
/// nothing.
#[test]
fn every_line_kind_has_an_arm_in_the_renderer() {
    let source = std::fs::read_to_string("src/events.rs")
        .expect("this crate's source is readable")
        .replace("\r\n", "\n");
    for (name, disposition, _) in TRIAGE {
        if *disposition != Disposition::Line {
            continue;
        }
        // **At the match arm's own indentation, and not anywhere in the file.**
        // A bare `contains` was satisfied by the variant's name appearing in a
        // doc comment, which is how this test went green through the whole of
        // 0.14.0's sabotage pass with the `Dialed` arm deleted: the `Sandbox`
        // arm's prose names `EventKind::Dialed` to explain why it draws nothing
        // itself, and that mention alone answered the question. That is the same
        // defect this test was written in 0.11.0 to close, wearing the new
        // table's clothes exactly as the comment above says — a name with no arm
        // behind it. `tests/glyphs.rs` already reads arms this way, by the twelve
        // spaces every match arm in that file sits at and no `use`, doc line or
        // expression does.
        let arm = format!("EventKind::{}", variant(name));
        let declared = format!("\n            {arm}");
        assert!(
            source.contains(&declared),
            "{name} is triaged as a line and `{arm}` has no arm in src/events.rs, so it commits \
             nothing at all — a mention in a comment is not an arm",
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
        EventKind::Speculated {
            started: 2,
            used: 1,
            discarded: 1,
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
        51,
        "every kind is exactly one of the three: {lines} lines, {status} status, {silent} silent",
    );
    // The release's own claim, and the reason it exists: most kinds are not
    // worth a line, and until 0.11.0 thirty-seven of them got one anyway.
    assert!(
        status + silent >= 20,
        "if almost everything is a line, nothing was triaged: {status} status, {silent} silent",
    );
}
