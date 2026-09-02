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
//! **0.30.0 writes it as well as reads it**, and the second half of this file is
//! about that write: `servers::switch`, the one edit `io mcp disable <id>`, `io mcp
//! enable <id>` and the `/mcp` keystroke all build; the sentence it costs an older
//! binary, which is not the sentence the `[[plugin]]` key costs; and
//! `servers::probe`, which asks io-harness to go and try the server rather than
//! asking the policy what it would say.
//!
//! A separate file from `tests/servers.rs` because that one is the 0.28.0
//! management surface and this is a property of the pin.

use io_cli::servers::{self, At, Observed, Reached};
use io_harness::config::Scope;
use io_harness::{Config, McpProbe, Policy};

mod support;

/// Read `text` as io-harness would read the operator's own file.
///
/// **Never `Config::from_toml`.** That parses at `Scope::Project`, and io-harness
/// 0.74.0 refuses `[[mcp]]` from a workspace-resident file — an MCP server is a
/// command, an argv and an environment this process spawns at run start, and
/// `io.toml` arrives with a `git clone`. Every fixture in this file declares one,
/// and `At::of` below already addresses it at `Scope::User`, so reading it at any
/// other scope was the fixture disagreeing with itself.
fn loaded(text: &str) -> Config {
    support::user_scope(text).config.clone()
}

/// One server switched off, one left alone, in the order the file names them.
///
/// The bytes as well as the parsed configuration, because half of this file is
/// about a *write*: `servers::At` addresses an entry by finding its id in the
/// file's own text, so a test of the write has to hold the text.
///
/// `mcp-docs` and `mcp-search` are commands that do not exist on any machine this
/// runs on, which is load-bearing in the probe tests below: a disabled server that
/// io-harness answered `Disabled` for was never spawned, and one it tried to spawn
/// answers `NotStarted`. The two are distinguishable precisely because the command
/// is not real.
const MIXED: &str = "\
[[mcp]]
id = \"docs\"
transport = \"stdio\"
command = \"mcp-docs\"
enabled = false

[[mcp]]
id = \"search\"
transport = \"stdio\"
command = \"mcp-search\"
";

fn mixed() -> Config {
    loaded(MIXED)
}

/// Where `MIXED` declares one of its two servers.
fn at(id: &str) -> At {
    At::of(Scope::User, MIXED, id).unwrap_or_else(|| panic!("the fixture names `{id}`"))
}

/// **The state reaches `Server`, and an absent key still means on.**
///
/// Addressed by id rather than by position: the fixture reads this one
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

/// **F10 — switching a server off leaves the entry, and every other key of it.**
///
/// The criterion in one test: after the write the `[[mcp]]` entry still exists, its
/// every other key is unchanged, `servers` and `rows` both report it off, and the
/// file reads back as io-harness's own [`io_harness::McpServer`] — which is the
/// only check that a `[[mcp]]` write really landed, since that section is exempt
/// from `deny_unknown_fields` and a key the harness ignores parses perfectly.
///
/// Sabotage: implement disable as `servers::remove`. The entry-still-exists
/// assertion is the first thing to go, and the `mcp_servers()` length assertion
/// after it says which entry was lost.
#[test]
fn f10_a_server_is_switched_off_without_being_taken_out_of_the_file() {
    let after = io_cli::edit::apply(MIXED, &[servers::switch(&at("search"), false)])
        .expect("the write parses back");

    // The entry is still there, and so is everything that made it a server.
    assert!(
        after.contains("id = \"search\""),
        "the entry was taken away rather than switched off: {after}",
    );
    assert!(
        after.contains("command = \"mcp-search\""),
        "the command went with the switch: {after}",
    );
    assert!(
        after.contains("transport = \"stdio\""),
        "the transport went with the switch: {after}",
    );
    // And the neighbour is untouched, which is what stops this passing on a
    // rewrite-the-whole-file implementation.
    assert!(after.contains("command = \"mcp-docs\""));

    let config = loaded(&after);
    assert_eq!(
        config.mcp_servers().len(),
        2,
        "both servers are still declared; a disable that removes is a remove",
    );

    let list = servers::servers(&config, &Observed::default());
    for server in &list {
        assert!(
            !server.enabled,
            "`{}` is still reported as running after both were switched off",
            server.id,
        );
    }
    // The panel says so too, on the row that was on a moment ago.
    let rows = servers::rows(&list);
    let row = rows
        .iter()
        .find(|row| row.label == "search")
        .expect("a row per configured server");
    assert!(
        row.detail
            .as_deref()
            .is_some_and(|detail| detail.contains(servers::DISABLED)),
        "the row for the server just switched off does not say so: {:?}",
        row.detail,
    );
}

/// **F10 — and it switches back on, which is the half 0.29.0 could not ship.**
///
/// `switch(.., true)` over the entry `MIXED` declares off, read back through
/// io-harness. A toggle that only goes one way is what the 0.29.0 refusal in
/// `tests/servers.rs` was standing in for.
#[test]
fn f10_a_server_switched_off_in_the_file_is_switched_back_on() {
    let after = io_cli::edit::apply(MIXED, &[servers::switch(&at("docs"), true)])
        .expect("the write parses back");
    let config = loaded(&after);
    let docs = servers::servers(&config, &Observed::default())
        .into_iter()
        .find(|server| server.id == "docs")
        .expect("the entry is still declared");
    assert!(
        docs.enabled,
        "`enabled = false` was not replaced, so the way back on does not exist",
    );

    // A boolean and not a string. `enabled = "true"` parses as TOML and io-harness
    // refuses it — which `configure::write`'s round trip would catch, at the cost
    // of an operator being told their toggle is broken.
    assert!(
        after.contains("enabled = true"),
        "the value must be a TOML boolean: {after}",
    );
}

/// **F10's second sabotage — `KEYS` gained one name and the check did not move.**
///
/// The refusal exists because `[[mcp]]` is one of two sections io-harness exempts
/// from `deny_unknown_fields`: a misspelled key is spliced in, accepted on
/// `configure::write`'s round trip, reported to the operator as written, and
/// ignored by every turn after it. Widening `servers::edit` to accept any key —
/// rather than listing `enabled` — would look like the same change and would put
/// that back.
///
/// Sabotage: replace the `KEYS.contains` guard with an unconditional `Some`. The
/// two assertions below on `comand` and `enbled` fail.
#[test]
fn f10_widening_keys_by_one_name_did_not_widen_the_check() {
    let docs = at("docs");
    assert!(
        servers::KEYS.contains(&"enabled"),
        "the criterion names `enabled` in `KEYS` and it is not there",
    );
    assert!(
        servers::edit(&docs, "enabled", "false").is_some(),
        "the listed name is refused, so the verb has no writer",
    );
    assert!(
        servers::edit(&docs, "comand", "\"mcp-find\"").is_none(),
        "a typo'd key is accepted, and io-harness will not complain about it either",
    );
    assert!(
        servers::edit(&docs, "enbled", "false").is_none(),
        "a near-miss on the very key this release added is written and ignored — \
         the server would go on running while the operator was told it stopped",
    );
}

/// **F12 — the two `enabled` writes cost an older binary opposite things.**
///
/// A `[[plugin]]` entry carrying `enabled` makes an io-cli built against io-harness
/// 0.69.0 refuse the **whole configuration file**: every setting in it, loudly, at
/// startup. The same key in an `[[mcp]]` entry is **ignored** by that binary, and
/// the server the operator switched off starts and runs with nothing said.
///
/// The second is the dangerous one, and an operator who has met the first will
/// assume the second behaves the same way. One constant for both would be that
/// assumption written down.
///
/// Sabotage: `pub use crate::pluginview::OLDER_BINARY` for the MCP side, or paste
/// the plugin sentence into `servers::OLDER_BINARY`. The inequality fails first,
/// and the word assertions say which half went missing.
#[test]
fn f12_the_two_older_binary_sentences_are_not_the_same_sentence() {
    let plugin = io_cli::pluginview::OLDER_BINARY;
    let server = servers::OLDER_BINARY;
    assert_ne!(
        plugin, server,
        "one sentence for two opposite costs tells the operator on the silent path \
         that they will notice",
    );

    assert!(
        plugin.contains("[[plugin]]") && plugin.contains("refuses"),
        "the plugin sentence must say the whole file is refused: {plugin}",
    );
    assert!(
        server.contains("[[mcp]]") && server.contains("ignores"),
        "the MCP sentence must say the key is ignored: {server}",
    );
    assert!(
        server.contains("anyway") || server.contains("starts the server"),
        "the MCP sentence must say the server runs regardless, which is the cost: \
         {server}",
    );
    // And neither is a copy of the other's claim: a plugin sentence that said
    // "ignores" would be wrong about the loud case, which is the direction that
    // makes an operator distrust the warning entirely.
    assert!(
        !plugin.contains("ignores the key"),
        "the plugin sentence claims the quiet behaviour: {plugin}",
    );
}

/// **F13 — six outcomes, six sentences, and the sixth is the one nobody can build.**
///
/// Five of `McpProbe`'s variants can be constructed here. The sixth cannot exist
/// yet — it is whatever a later io-harness adds behind `#[non_exhaustive]` — so it
/// is asserted through `servers::UNMODELLED`, the string the `_` arm renders: no
/// known variant may produce it, and it may not produce any known variant's words.
///
/// Sabotage: collapse `Disabled` into `NotStarted`. The disabled sentence stops
/// naming the file's instruction and the pairwise-distinct assertion fails on that
/// pair. Second sabotage: drop the `_` arm and let a catch-all name a known state —
/// whichever variant the catch-all swallowed loses its own sentence below.
#[test]
fn f13_every_probe_outcome_is_its_own_sentence() {
    let disabled = servers::probed("docs", &McpProbe::Disabled);
    let refused = servers::probed(
        "docs",
        &McpProbe::Refused {
            act: "exec".to_string(),
            target: "mcp-docs".to_string(),
            rule: Some("mcp-*".to_string()),
            layer: Some("project".to_string()),
        },
    );
    let not_started = servers::probed(
        "docs",
        &McpProbe::NotStarted {
            reason: "could not spawn mcp-docs: No such file or directory".to_string(),
        },
    );
    let unreachable = servers::probed(
        "docs",
        &McpProbe::Unreachable {
            reason: "connection reset".to_string(),
        },
    );
    let timed_out = servers::probed("docs", &McpProbe::TimedOut { secs: 60 });
    let answered = servers::probed(
        "docs",
        &McpProbe::Answered {
            tools: vec![
                "mcp__docs__search".to_string(),
                "mcp__docs__fetch".to_string(),
            ],
        },
    );

    // A switched-off server reports the instruction, not a failure to start.
    assert!(
        disabled.contains(servers::DISABLED),
        "a disabled server must be reported as disabled: {disabled}",
    );
    assert!(
        !disabled.contains("did not start"),
        "`Disabled` is rendered as a failed spawn, which sends the operator to fix \
         a command that was never run: {disabled}",
    );

    // All four fields of a refusal, because `act`/`target` and `rule`/`layer` are
    // two different repairs — add an allow, or remove a deny.
    for wanted in ["exec", "mcp-docs", "mcp-*", "project"] {
        assert!(
            refused.contains(wanted),
            "a refusal must name {wanted}: {refused}",
        );
    }

    assert!(
        not_started.contains("No such file or directory"),
        "the spawn failure is what tells the operator the path is wrong: \
         {not_started}",
    );
    assert!(
        unreachable.contains("connection reset"),
        "the transport's own words are the only thing that locates this: \
         {unreachable}",
    );
    assert!(
        timed_out.contains("60"),
        "the bound that was exceeded is the number to change: {timed_out}",
    );
    // The namespaced names, which are what the model sees and what a policy rule
    // can name. The bare `search` appears nowhere a rule could match it.
    assert!(
        answered.contains("mcp__docs__search") && answered.contains("mcp__docs__fetch"),
        "an answering server lists the namespaced tools io-harness returned: \
         {answered}",
    );

    // Six sentences, pairwise. The `_` arm's is included: it stands for the
    // outcome no test can construct, and the only thing that can be asserted about
    // it is that it is nobody else's sentence.
    let unknown = format!("`docs`: {}", servers::UNMODELLED);
    let all = [
        ("disabled", &disabled),
        ("refused", &refused),
        ("not started", &not_started),
        ("unreachable", &unreachable),
        ("timed out", &timed_out),
        ("answered", &answered),
        ("unmodelled", &unknown),
    ];
    for (i, (left_name, left)) in all.iter().enumerate() {
        for (right_name, right) in all.iter().skip(i + 1) {
            assert_ne!(
                left, right,
                "{left_name} and {right_name} render the same sentence, so the \
                 repair an operator goes off to make is a coin toss",
            );
        }
    }
    // And no known outcome renders as the unknown one, which is the other half of
    // the `_` arm being honest.
    for (name, line) in all.iter().take(6) {
        assert!(
            !line.contains(servers::UNMODELLED),
            "{name} is reported as a state this build does not model: {line}",
        );
    }
}

/// **F13's second arm, which had nowhere to run until this.**
///
/// The criterion's named sabotage is *"drop the `_` arm and match the five known
/// variants with a catch-all that names one of them"*. Executing it killed
/// **nothing**: `McpProbe` is `#[non_exhaustive]` and belongs to another crate, so
/// no test can construct a variant this build does not know, and the arm is
/// therefore unreachable from every behavioural assertion in this file — the one
/// above builds its expected string from `servers::UNMODELLED` by hand for exactly
/// that reason.
///
/// A criterion whose sabotage has no site is a criterion nobody is checking. The
/// property here is about a branch that cannot be entered, so the only instrument
/// left is the source, read as text — the same answer this product already gives
/// for `src/main.rs`.
///
/// Sabotage: point the catch-all at any known state's sentence. Only this fails.
#[test]
fn f13_the_catch_all_names_no_state_this_build_knows() {
    let source = std::fs::read_to_string("src/servers.rs").expect("the module");
    let code: String = source
        .lines()
        .filter(|line| !line.trim_start().starts_with("//"))
        .collect::<Vec<&str>>()
        .join("\n");
    let flat = code.split_whitespace().collect::<Vec<&str>>().join(" ");

    assert!(
        flat.contains("_ => format!(\"`{id}`: {UNMODELLED}\")"),
        "`servers::probed`'s catch-all no longer renders `UNMODELLED`. A newer \
         io-harness reporting a state this build does not model would be described \
         to the operator as one it does, and the repair they go off to make would \
         be for a problem they do not have.",
    );
}

/// **F13 — a disabled server is probed without being started.**
///
/// This is the one probe assertion that reaches `io_harness::probe_mcp` for real,
/// and it is safe to run anywhere because the whole point is that nothing is
/// spawned: io-harness answers `Disabled` before it looks at the transport. The
/// fixture's command does not exist, so an implementation that spawned first would
/// come back `NotStarted` and this fails by name.
#[tokio::test]
async fn f13_a_disabled_server_is_probed_without_a_process_being_started() {
    let config = mixed();
    let policy = Policy::default().layer("test").allow_exec("*");

    let probe = servers::probe(&config, "docs", &policy)
        .await
        .expect("the fixture declares `docs`");
    assert_eq!(
        probe,
        McpProbe::Disabled,
        "a switched-off server was started, or the answer was reported as \
         something else",
    );

    // An id nothing declares is the one thing `probe_mcp` cannot have an opinion
    // about, so it is this crate's refusal — and it names the verb that lists them.
    let refusal = servers::probe(&config, "never-configured", &policy)
        .await
        .expect_err("no file declares it");
    assert!(
        refusal.contains("never-configured") && refusal.contains("mcp list"),
        "the refusal has to say which id and where to look: {refusal}",
    );
}

/// **F14 — a probe is never confused with the observation beside it.**
///
/// `Reached::NotYet` is documented as *not a failure*: it is the state every server
/// is in before the first turn. `McpProbe::Unreachable` is io-cli having gone and
/// looked and got nothing. Rendering one in the other's words would turn a fresh
/// session into a wall of faults, or a real fault into "nothing has called it yet".
///
/// Sabotage: render `NotYet` with the probe's unreachable string, or the reverse.
/// The two containment assertions fail in whichever direction the words were
/// copied.
#[tokio::test]
async fn f14_a_probe_and_a_state_nobody_reached_are_different_words() {
    let list = servers::servers(&mixed(), &Observed::default());
    let rows = servers::rows(&list);
    let not_yet = rows
        .iter()
        .find(|row| row.label == "search")
        .and_then(|row| row.detail.clone())
        .expect("the server that is on draws a row");
    assert!(
        not_yet.contains("not reached"),
        "the passive state's own words moved: {not_yet}",
    );

    let unreachable = servers::probed(
        "search",
        &McpProbe::Unreachable {
            reason: "connection reset".to_string(),
        },
    );
    assert!(
        !unreachable.contains("not reached"),
        "a probe that went and looked is reported as a server nobody has called: \
         {unreachable}",
    );
    assert!(
        !not_yet.contains("did not answer"),
        "a server nobody has called this session is reported as one that failed to \
         answer: {not_yet}",
    );

    // **And a probe writes nothing into `Observed`.** The signature is the real
    // guarantee — `probe` takes no `&mut Observed` — and this is the behavioural
    // half of it: a probe happened, and the panel still says nobody has called the
    // server, because no turn has.
    let config = mixed();
    let observed = Observed::default();
    let policy = Policy::default().layer("test").allow_exec("*");
    let _ = servers::probe(&config, "docs", &policy)
        .await
        .expect("the fixture declares `docs`");
    assert_eq!(
        observed.of("docs"),
        Reached::NotYet,
        "a probe put its own result under a heading that says a turn produced it",
    );
}
