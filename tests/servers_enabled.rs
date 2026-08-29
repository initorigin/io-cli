//! A server the file switched off is a state this interface can see.
//!
//! io-harness 0.70.0 gave `[[mcp]]` an `enabled` key defaulting to true and
//! honours it at the earliest possible point: a disabled server is never spawned,
//! never dialled, and never even checked against the policy (`mcp.rs:356`), and
//! `probe_mcp` answers `McpProbe::Disabled` (`mcp.rs:747`).
//!
//! io-cli 0.29.0 took that pin and, for one review cycle, consumed exactly half of
//! the field: `manage` set it on an added server and no reader anywhere could
//! report it. That is the same defect this release fixed for `[[plugin]]` in
//! `pluginview::view`, left standing on the sibling surface in the same release —
//! and `src/servers.rs`'s own module docs argued it could not happen, in three
//! clauses that io-harness 0.70.0 had already falsified.
//!
//! Nothing in this crate writes `enabled` into an `[[mcp]]` table. What reaches
//! this state is a file somebody hand-edited or a repository they cloned, which is
//! exactly the case a listing exists to explain: every turn runs without those
//! tools, and without this the operator has nowhere to look.
//!
//! A separate file from `tests/servers.rs` because that one is the 0.28.0
//! management surface and this is a property of the pin.

use io_cli::servers::{self, Observed, Reached};
use io_harness::Config;

/// One server switched off, one left alone, in the order the file names them.
fn mixed() -> Config {
    Config::from_toml(
        "\
[[mcp]]
id = \"docs\"
transport = \"stdio\"
command = \"mcp-docs\"
enabled = false

[[mcp]]
id = \"search\"
transport = \"stdio\"
command = \"mcp-search\"
",
    )
    .expect("the fixture parses")
}

/// **The state reaches `Server`, and an absent key still means on.**
///
/// Addressed by id rather than by position: `Config::from_toml` reads this one
/// string, but the rule this file follows is `tests/marketplace.rs`'s — a length
/// or an index is an assertion about somebody else's machine as much as about the
/// criterion.
///
/// Sabotage: drop `enabled: server.enabled` from `servers::servers` and take the
/// field's default. Every assertion below about `docs` fails; the one about
/// `search` does not, which is what makes this test about the *field* rather than
/// about the struct having grown one.
#[test]
fn a_server_switched_off_in_the_file_is_read_as_switched_off() {
    let list = servers::servers(&mixed(), &Observed::default());

    let docs = list
        .iter()
        .find(|server| server.id == "docs")
        .expect("the fixture names `docs`");
    assert!(
        !docs.enabled,
        "`docs` is declared `enabled = false` and io-harness will not start it; a \
         reader that cannot say so leaves an operator whose turns lost their tools \
         with nothing to look at",
    );

    let search = list
        .iter()
        .find(|server| server.id == "search")
        .expect("the fixture names `search`");
    assert!(
        search.enabled,
        "`search` names no `enabled` key, and io-harness's own default for it is \
         true — an entry written before the key existed means what it always did",
    );
}

/// **The panel says the instruction, not a state the session never reached.**
///
/// `Reached::NotYet` renders "not reached this session", which is *true* of a
/// switched-off server and reads as one that simply has not been called yet. That
/// is the one wrong answer available on this row, so the instruction replaces the
/// state rather than sitting beside it.
///
/// Sabotage: draw the `Reached` detail for a disabled server as well. The row then
/// carries "not reached this session" and this test fails on it by name.
#[test]
fn the_panel_draws_the_instruction_over_a_state_that_was_never_reached() {
    let list = servers::servers(&mixed(), &Observed::default());
    let rows = servers::rows(&list);

    let off = rows
        .iter()
        .find(|row| row.label == "docs")
        .expect("a row per configured server");
    let detail = off.detail.as_deref().expect("the row carries a detail");
    assert!(
        detail.contains(servers::DISABLED),
        "the row for a switched-off server reads {detail:?}, which does not say so",
    );
    assert!(
        !detail.contains("not reached"),
        "the row for a switched-off server reads {detail:?} — true, and the reading \
         an operator will take is that it has not been called yet",
    );

    // And the server that is on is untouched by any of it: without this the
    // assertions above are satisfied by a row builder that says `disabled` over
    // everything.
    let on = rows
        .iter()
        .find(|row| row.label == "search")
        .expect("a row per configured server");
    let detail = on.detail.as_deref().expect("the row carries a detail");
    assert!(
        !detail.contains(servers::DISABLED),
        "the row for a server that is on reads {detail:?}",
    );
    assert!(
        detail.contains("not reached"),
        "the row for a server that is on and unreached reads {detail:?}",
    );
}

/// **A switched-off server is still editable and still removable.**
///
/// `declared_at` locates by id in the file that declares it, so nothing about the
/// state changes which entry a write lands on. Without this a bundle an operator
/// could see would be one they could not act on, which is half a surface.
#[test]
fn a_switched_off_server_is_still_located_in_the_file_that_declares_it() {
    let list = servers::servers(&mixed(), &Observed::default());
    let docs = list
        .iter()
        .find(|server| server.id == "docs")
        .expect("the fixture names `docs`");

    assert!(!docs.enabled);
    assert_eq!(
        docs.state,
        Reached::NotYet,
        "a switched-off server reaches nothing, and the row's own state says so \
         separately from the file's instruction",
    );
}
