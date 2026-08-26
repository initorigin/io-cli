//! F5, F6 and F9 — the `/mcp` panel's three states, the writes it makes, and the
//! two counts it keeps apart.

use io_cli::servers::{self, Observed, Reached};
use io_harness::config::Config;
use io_harness::{EventKind, McpTransport};

/// A configuration naming two servers, neither of them reached yet.
fn configured() -> Config {
    Config::from_toml(
        "\
[[mcp]]
id = \"docs\"
transport = \"stdio\"
command = \"mcp-docs\"

[[mcp]]
id = \"search\"
transport = \"stdio\"
command = \"mcp-search\"
",
    )
    .expect("the fixture parses")
}

/// The server reaching the run: an `Mcp` event with no `tool`. Since io-harness
/// 0.68.0 that event states how many tools the server offered, which is the one
/// form carrying the count.
fn reached(server: &str) -> EventKind {
    reached_offering(server, 3)
}

/// The same event with the offered count named.
fn reached_offering(server: &str, tools: u32) -> EventKind {
    EventKind::Mcp {
        server: server.into(),
        tool: None,
        ok: None,
        millis: None,
        tools: Some(tools),
    }
}

/// One call, and whether it worked. A call carries no count.
fn called(server: &str, tool: &str, ok: Option<bool>) -> EventKind {
    EventKind::Mcp {
        server: server.into(),
        tool: Some(tool.into()),
        ok,
        millis: Some(12),
        tools: None,
    }
}

#[test]
fn f5_a_server_the_session_has_not_reached_is_not_a_failure() {
    // The state EVERY server is in at session start, before a turn has run. A
    // panel that drew it as a failure would tell an operator their configuration
    // is broken at the moment it is most likely to be fine.
    let config = configured();
    let nothing = Observed::default();
    let list = servers::servers(&config, &nothing);

    assert_eq!(list.len(), 2);
    for server in &list {
        assert_eq!(server.state, Reached::NotYet, "{}", server.id);
        assert_eq!(server.state.word(), "not reached");
    }

    // And it reads as its own thing rather than as an absence.
    let rows = servers::rows(&list);
    assert!(
        rows[0]
            .detail
            .as_deref()
            .is_some_and(|d| d.contains("not reached this session")),
        "{:?}",
        rows[0].detail
    );
}

#[test]
fn f5_three_states_are_distinguishable_at_once() {
    let config = configured();
    let mut observed = Observed::default();
    // `docs` answers two calls; `search` is asked once and fails.
    observed.event(&reached("docs"));
    observed.event(&called("docs", "search_docs", Some(true)));
    observed.event(&called("docs", "get_page", Some(true)));
    observed.event(&called("search", "query", Some(false)));

    let list = servers::servers(&config, &observed);
    let docs = list.iter().find(|s| s.id == "docs").unwrap();
    let search = list.iter().find(|s| s.id == "search").unwrap();

    assert_eq!(
        docs.state,
        Reached::Answered {
            tools: 2,
            offered: Some(3)
        }
    );
    assert_eq!(
        search.state,
        Reached::Failed {
            tool: "query".into()
        }
    );

    // Three words, three states, no two the same.
    let mut words: Vec<&str> = vec![
        docs.state.word(),
        search.state.word(),
        Reached::NotYet.word(),
    ];
    words.sort();
    words.dedup();
    assert_eq!(words.len(), 3);
}

#[test]
fn f5_the_count_is_distinct_tools_and_never_calls() {
    // The number is a LOWER BOUND on what the server offers, and it counts
    // distinct tool names rather than calls — asking one tool five times is one
    // tool. A count of calls would grow without the server offering anything
    // more, which is the number io-cli's status line has been drawing since
    // 0.10.0 under a label that says "tools".
    let config = configured();
    let mut observed = Observed::default();
    for _ in 0..5 {
        observed.event(&called("docs", "search_docs", Some(true)));
    }
    let list = servers::servers(&config, &observed);
    let docs = list.iter().find(|s| s.id == "docs").unwrap();
    assert_eq!(
        docs.state,
        Reached::Answered {
            tools: 1,
            offered: None
        },
        "five calls is one tool"
    );
}

#[test]
fn f5_a_call_is_not_counted_as_a_second_server() {
    // `Mcp` is the one event that means two things. Counting a call as a server
    // would multiply one server into as many things as it was asked to do.
    let config = configured();
    let mut observed = Observed::default();
    observed.event(&reached("docs"));
    observed.event(&called("docs", "a", Some(true)));
    observed.event(&called("docs", "b", Some(true)));

    let list = servers::servers(&config, &observed);
    assert_eq!(list.len(), 2, "the panel is the CONFIGURED set, always");
    assert_eq!(
        list.iter().find(|s| s.id == "docs").unwrap().state,
        Reached::Answered {
            tools: 2,
            offered: Some(3)
        }
    );
}

#[test]
fn f5_an_outcome_the_event_did_not_carry_is_not_a_failure() {
    // `ok` is `Option<bool>`. `None` is a call whose outcome the event did not
    // report, which is not the same as a call that failed and must not be drawn
    // as one.
    let config = configured();
    let mut observed = Observed::default();
    observed.event(&called("docs", "a", None));

    let list = servers::servers(&config, &observed);
    assert_eq!(
        list.iter().find(|s| s.id == "docs").unwrap().state,
        Reached::Answered {
            tools: 1,
            offered: None
        },
        "a call with no reported outcome was drawn as a failure"
    );
}

#[test]
fn f5_forgetting_a_run_clears_every_server() {
    // The hole `Status::forget_run` exists to close, at the other place that now
    // accumulates per-run state. 0.8.0 shipped `Fleet::forget` with no caller.
    let config = configured();
    let mut observed = Observed::default();
    observed.event(&called("docs", "a", Some(false)));
    assert!(matches!(observed.of("docs"), Reached::Failed { .. }));

    observed.forget();
    assert_eq!(observed.of("docs"), Reached::NotYet);
    let list = servers::servers(&config, &observed);
    assert!(list.iter().all(|s| s.state == Reached::NotYet));
}

#[test]
fn f5_the_panel_reads_the_configuration_and_never_the_events_for_its_rows() {
    // A server that answered but is not in the file is not a row: the panel is
    // about the configuration an operator can act on.
    let config = configured();
    let mut observed = Observed::default();
    observed.event(&reached("a-server-nobody-configured"));

    let list = servers::servers(&config, &observed);
    let ids: Vec<&str> = list.iter().map(|s| s.id.as_str()).collect();
    assert_eq!(ids, vec!["docs", "search"]);
}

// --- F9: offered, beside asked-for --------------------------------------------

/// The row a server drew, by id.
fn detail(config: &Config, observed: &Observed, id: &str) -> String {
    let list = servers::servers(config, observed);
    let rows = servers::rows(&list);
    let at = list.iter().position(|s| s.id == id).expect("a configured id");
    rows[at].detail.clone().expect("every row carries a detail")
}

#[test]
fn f9_offered_and_asked_for_are_two_different_numbers() {
    // The whole of F9: a server that announced ten tools and has been called
    // twice has TWO facts about it, and neither answers for the other. Ten with
    // two calls and ten with two hundred are different sessions; two asked for
    // out of ten and two out of two are different servers.
    let config = configured();
    let mut observed = Observed::default();
    observed.event(&reached_offering("docs", 10));
    observed.event(&called("docs", "search_docs", Some(true)));
    observed.event(&called("docs", "get_page", Some(true)));

    assert_eq!(
        observed.of("docs"),
        Reached::Answered {
            tools: 2,
            offered: Some(10)
        }
    );

    // And both reach the screen, as two numbers rather than one.
    let drawn = detail(&config, &observed, "docs");
    assert!(drawn.contains("10 offered"), "{drawn:?}");
    assert!(drawn.contains("2 used"), "{drawn:?}");
}

#[test]
fn f9_a_server_that_announced_no_tools_offers_zero_and_not_unknown() {
    // `Some(0)` is a STATEMENT. A server that came up with an empty catalogue is
    // a configuration an operator wants to see named, and drawing it the same as
    // a server whose count was never heard would hide the one thing wrong with it.
    let config = configured();
    let mut observed = Observed::default();
    observed.event(&reached_offering("docs", 0));

    assert_eq!(
        observed.of("docs"),
        Reached::Answered {
            tools: 0,
            offered: Some(0)
        }
    );
    let drawn = detail(&config, &observed, "docs");
    assert!(drawn.contains("0 offered"), "{drawn:?}");
}

#[test]
fn f9_a_server_that_has_only_been_called_does_not_report_offering_nothing() {
    // THE SABOTAGE. Read a missing count as zero — `unwrap_or(0)` anywhere on the
    // path — and this server, which has demonstrably answered two different tools,
    // reports offering none. Every event here carries `tools: None`, because the
    // count rides only the announcing event and this session never folded one in.
    let config = configured();
    let mut observed = Observed::default();
    observed.event(&called("docs", "search_docs", Some(true)));
    observed.event(&called("docs", "get_page", Some(true)));

    assert_eq!(
        observed.of("docs"),
        Reached::Answered {
            tools: 2,
            offered: None
        },
        "a missing count was read as a stated zero"
    );
    let drawn = detail(&config, &observed, "docs");
    assert!(
        !drawn.contains("offered"),
        "the panel claimed an offered count it never heard: {drawn:?}"
    );
    assert!(drawn.contains("2 tools used"), "{drawn:?}");
}

#[test]
fn f9_a_later_event_without_the_count_does_not_erase_the_one_that_had_it() {
    // The same sabotage from the other side: the announcing event states the
    // count, then every call that follows carries `None`. Folding a `None` in as
    // an assignment would delete a fact the session was told.
    let mut observed = Observed::default();
    observed.event(&reached_offering("docs", 7));
    observed.event(&called("docs", "a", Some(true)));

    assert_eq!(
        observed.of("docs"),
        Reached::Answered {
            tools: 1,
            offered: Some(7)
        }
    );
}

#[test]
fn f9_a_server_never_reached_shows_neither_number() {
    // Not reached is not "offered nothing, used nothing". It is the third state,
    // and F5's whole point is that it stays its own thing.
    let config = configured();
    let observed = Observed::default();

    assert_eq!(observed.of("docs"), Reached::NotYet);
    let drawn = detail(&config, &observed, "docs");
    assert!(!drawn.contains("offered"), "{drawn:?}");
    assert!(!drawn.contains("used"), "{drawn:?}");
    assert!(drawn.contains("not reached this session"), "{drawn:?}");
}

// --- F6: the writes -----------------------------------------------------------

#[test]
fn f6_a_server_is_added_as_a_whole_entry_and_the_file_survives() {
    const OPERATORS: &str = "\
# my servers
[[mcp]]
id = \"docs\"
transport = \"stdio\"
command = \"mcp-docs\"

[instructions]
files = [\"AGENTS.md\"]
";
    let after = io_cli::edit::apply(OPERATORS, &[servers::add("search", "mcp-search")]).unwrap();

    assert!(after.contains("id = \"search\""));
    assert!(after.contains("command = \"mcp-search\""));
    // Everything that was there is still there — including the section io-cli
    // does not model and the comment above the first entry.
    for line in OPERATORS.lines().filter(|l| !l.trim().is_empty()) {
        assert!(after.contains(line), "line lost: {line:?}");
    }
    // And the result is still the harness's schema.
    let config = Config::from_toml(&after).expect("the written file loads");
    assert_eq!(config.mcp_servers().len(), 2);
}

#[test]
fn f6_an_edit_reaches_the_entry_it_names_and_no_other() {
    const TWO: &str = "\
[[mcp]]
id = \"docs\"
transport = \"stdio\"
command = \"mcp-docs\"

[[mcp]]
id = \"search\"
transport = \"stdio\"
command = \"mcp-search\"
";
    let after = io_cli::edit::apply(TWO, &[servers::edit(1, "command", "\"mcp-find\"")]).unwrap();

    assert!(after.contains("command = \"mcp-find\""));
    assert!(
        after.contains("command = \"mcp-docs\""),
        "the first entry was edited instead of the second"
    );
    let config = Config::from_toml(&after).unwrap();
    assert_eq!(config.mcp_servers().len(), 2);
}

#[test]
fn f6_a_removal_takes_the_whole_entry_and_leaves_its_siblings() {
    const TWO: &str = "\
[[mcp]]
id = \"docs\"
transport = \"stdio\"
command = \"mcp-docs\"

[[mcp]]
id = \"search\"
transport = \"stdio\"
command = \"mcp-search\"

[run]
max_steps = 30
";
    let after = io_cli::edit::apply(TWO, &[servers::remove(0)]).unwrap();

    let config = Config::from_toml(&after).expect("the written file loads");
    let left: Vec<&str> = config.mcp_servers().iter().map(|s| s.id.as_str()).collect();
    assert_eq!(left, vec!["search"]);
    assert!(
        after.contains("max_steps = 30"),
        "a later section was taken with it"
    );
}

#[test]
fn f6_a_key_io_cli_does_not_model_survives_a_write_to_its_neighbour() {
    // The reason F1 is a byte property, proved on the one table where the loss
    // would be silent: `McpServer` is `#[serde(flatten)]`-based, so an unknown
    // key inside an `[[mcp]]` table is NOT rejected at load — a writer that
    // re-serialised the array from io-cli's model would delete it without a word.
    const EXTRA: &str = "\
[[mcp]]
id = \"docs\"
transport = \"stdio\"
command = \"mcp-docs\"
env = { TOKEN = \"abc\" }
args = [\"--verbose\"]
";
    let after =
        io_cli::edit::apply(EXTRA, &[servers::edit(0, "command", "\"mcp-docs-2\"")]).unwrap();

    assert!(
        after.contains("env = { TOKEN = \"abc\" }"),
        "an unmodelled key vanished"
    );
    assert!(
        after.contains("args = [\"--verbose\"]"),
        "an unmodelled key vanished"
    );
    assert!(after.contains("command = \"mcp-docs-2\""));
}

#[test]
fn f6_a_server_id_with_a_quote_in_it_is_escaped_rather_than_breaking_the_file() {
    let after = io_cli::edit::apply("", &[servers::add("we\"ird", "cmd")]).unwrap();
    let config = Config::from_toml(&after).expect("the written file still parses");
    assert_eq!(config.mcp_servers()[0].id, "we\"ird");
}

#[test]
fn f6_the_transport_is_shown_as_the_thing_that_reaches_it() {
    let config = Config::from_toml(
        "\
[[mcp]]
id = \"local\"
transport = \"stdio\"
command = \"mcp-docs\"

[[mcp]]
id = \"remote\"
transport = \"http\"
url = \"https://example.com/mcp\"
",
    )
    .unwrap();
    let list = servers::servers(&config, &Observed::default());

    let by = |id: &str| list.iter().find(|s| s.id == id).unwrap().transport.clone();
    assert_eq!(by("local"), "mcp-docs");
    assert_eq!(by("remote"), "https://example.com/mcp");

    // And the fixture really did produce two different transports, so the test
    // is not passing on one arm twice.
    assert!(matches!(
        config.mcp_servers()[0].transport,
        McpTransport::Stdio { .. }
    ));
}
