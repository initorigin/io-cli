//! F5, F6 and F9 — the `/mcp` panel's three states, the writes it makes, and the
//! two counts it keeps apart.

use std::collections::BTreeMap;

use io_cli::configure::Decided;
use io_cli::servers::{self, At, Observed, Reached, Server};
use io_harness::config::{Config, Scope};
use io_harness::{EventKind, McpServer, McpTransport};

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
    let at = list
        .iter()
        .position(|s| s.id == id)
        .expect("a configured id");
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
    let after = io_cli::edit::apply(
        OPERATORS,
        &[servers::add(&McpServer::stdio("search", "mcp-search"))],
    )
    .unwrap();

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
    // The position is looked up by id rather than typed, which is the whole of
    // the `At` contract: the second entry is `search`, and nothing here spells a
    // `1` that could have been a row number instead.
    let at = At::of(Scope::User, TWO, "search").expect("the fixture names `search`");
    assert_eq!(at.index(), 1);
    let edit = servers::edit(&at, "command", &servers::quoted("mcp-find"))
        .expect("`command` is a key an `[[mcp]]` entry carries");
    let after = io_cli::edit::apply(TWO, &[edit]).unwrap();

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
    let at = At::of(Scope::User, TWO, "docs").expect("the fixture names `docs`");
    let after = io_cli::edit::apply(TWO, &[servers::remove(&at)]).unwrap();

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
    let at = At::of(Scope::User, EXTRA, "docs").expect("the fixture names `docs`");
    let edit = servers::edit(&at, "command", &servers::quoted("mcp-docs-2")).expect("a real key");
    let after = io_cli::edit::apply(EXTRA, &[edit]).unwrap();

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
    let after =
        io_cli::edit::apply("", &[servers::add(&McpServer::stdio("we\"ird", "cmd"))]).unwrap();
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

// --- F6: the write half, complete and addressable ------------------------------

/// Two servers and nothing else, for the position tests.
const TWO_SERVERS: &str = r#"[[mcp]]
id = "docs"
transport = "stdio"
command = "mcp-docs"

[[mcp]]
id = "search"
transport = "stdio"
command = "mcp-search"
"#;

/// A file an operator wrote: a comment above the array, an inline comment on a
/// value, a blank-line rhythm, an array value, a section io-cli has no type for
/// (`[[agent]]`) and another (`[instructions]`).
const HAND_WRITTEN: &str = r#"# The servers this workspace talks to.

[[mcp]]
id = "docs"
transport = "stdio"
command = "mcp-docs"

[[mcp]]
id = "search"
transport = "stdio"
command = "mcp-search" # the fast one
args = ["--index", "./idx"]

# The roster, which io-cli has no type for anywhere.
[[agent]]
name = "scout"
role = "find the file and say where it is"
model = "anthropic/claude-haiku-4"

[instructions]
files = ["AGENTS.md"]
"#;

/// Exactly the bytes the first `[[mcp]]` entry occupies, its trailing blank line
/// included — an entry owns the whitespace down to the next header.
const DOCS_ENTRY: &str = r#"[[mcp]]
id = "docs"
transport = "stdio"
command = "mcp-docs"

"#;

#[test]
fn f6_add_edit_and_remove_change_one_entry_and_no_other_byte() {
    // F1 as a BYTE property, on a file an operator wrote. A writer that
    // re-serialised the array from io-cli's model would still produce a file
    // that parses and still carry the value it was asked for, which is what
    // makes that mistake invisible to any test that only re-parses. So this
    // compares bytes, three times.

    // Adding appends, so the whole original is still the prefix, untouched.
    let added = io_cli::edit::apply(
        HAND_WRITTEN,
        &[servers::add(&McpServer::stdio("notes", "mcp-notes"))],
    )
    .unwrap();
    assert_eq!(
        &added[..HAND_WRITTEN.len()],
        HAND_WRITTEN,
        "adding a server rewrote bytes above it",
    );
    assert_eq!(
        &added[HAND_WRITTEN.len()..],
        "\n[[mcp]]\nid = \"notes\"\ntransport = \"stdio\"\ncommand = \"mcp-notes\"\n",
    );

    // Editing replaces the value's own bytes — the inline comment sharing that
    // line included, which is the span property `edit.rs` measurement 1 states.
    let at = At::of(Scope::User, HAND_WRITTEN, "search").expect("the fixture names `search`");
    let edited = io_cli::edit::apply(
        HAND_WRITTEN,
        &[servers::edit(&at, "command", &servers::quoted("mcp-find")).expect("a real key")],
    )
    .unwrap();
    assert_eq!(
        edited,
        HAND_WRITTEN.replace("\"mcp-search\"", "\"mcp-find\""),
        "an edit reached past the value it named",
    );
    assert!(
        edited.contains("command = \"mcp-find\" # the fast one"),
        "the inline comment on the edited line was eaten",
    );

    // Removing takes that entry's block and leaves every other byte in place.
    let at = At::of(Scope::User, HAND_WRITTEN, "docs").expect("the fixture names `docs`");
    let removed = io_cli::edit::apply(HAND_WRITTEN, &[servers::remove(&at)]).unwrap();
    assert_eq!(removed, HAND_WRITTEN.replace(DOCS_ENTRY, ""));
    assert!(!removed.contains("mcp-docs"), "the entry left bytes behind");
    assert!(
        removed.contains("# The servers this workspace talks to."),
        "the comment above the array went with the entry below it",
    );

    // And all three results are still the harness's schema, roster and
    // instructions intact.
    for text in [&added, &edited, &removed] {
        Config::from_toml(text).expect("the written file loads");
        assert!(text.contains("name = \"scout\""), "the roster was dropped");
        assert!(
            text.contains("files = [\"AGENTS.md\"]"),
            "a section io-cli has no type for was dropped",
        );
    }
}

#[test]
fn f6_a_server_with_args_and_env_round_trips_as_the_harness_s_own_type() {
    // **Asserted by deserialising, never by looking for a string.** `[[mcp]]` is
    // one of only two sections io-harness exempts from `deny_unknown_fields`
    // (`config.rs:86`), so a key this writer got wrong is accepted by the parser
    // and ignored by the harness: a test that found `args = [...]` somewhere in
    // the file would pass over a write the server never sees. Reading it back as
    // an `McpServer` and comparing values is the only assertion that cannot.
    let mut env = BTreeMap::new();
    env.insert(
        "GITHUB_TOKEN".to_string(),
        "ghp-not-a-real-token".to_string(),
    );
    // A key and a value a `format!("\"{}\"")` would spell wrong: the backslashes
    // in a Windows path open escapes, and `\U` is a parse error rather than a
    // different value.
    env.insert("WIN\"PATH".to_string(), "C:\\Users\\me".to_string());
    let server = McpServer {
        id: "github".into(),
        transport: McpTransport::Stdio {
            command: "github-mcp-server".into(),
            args: vec!["stdio".into(), "--read-only".into()],
            env,
        },
        timeout_secs: 30,
    };

    // ONE edit, therefore one `configure::write`: a server whose `args` arrived
    // in a second call would be a second discover-and-roll-back round trip over
    // a file that, in between, declared a server without the arguments that make
    // it work.
    let edits = [servers::add(&server)];
    assert_eq!(edits.len(), 1, "a whole server is one write");
    let after = io_cli::edit::apply("", &edits).unwrap();

    let config = Config::from_toml(&after).expect("the written file loads");
    assert_eq!(
        config.mcp_servers(),
        [server].as_slice(),
        "the server read back is not the server written",
    );
}

#[test]
fn f6_an_http_server_round_trips_with_its_headers() {
    let mut headers = BTreeMap::new();
    headers.insert(
        "Authorization".to_string(),
        "Bearer not-a-token".to_string(),
    );
    let server = McpServer {
        id: "remote".into(),
        transport: McpTransport::Http {
            url: "https://example.com/mcp".into(),
            headers,
        },
        timeout_secs: 90,
    };

    let after = io_cli::edit::apply("", &[servers::add(&server)]).unwrap();
    let config = Config::from_toml(&after).expect("the written file loads");
    assert_eq!(config.mcp_servers(), [server].as_slice());
    // The transport tag, not a `command` borrowed from the other arm.
    assert!(
        !after.contains("command"),
        "an http entry named a command: {after}"
    );
}

#[test]
fn f6_an_entry_states_nothing_that_is_already_the_default() {
    // Every one of these is `#[serde(default)]`, so writing them changes nothing
    // except how much of an operator's file is io-cli's opinion. `timeout_secs`
    // is the one worth naming: the default is io-harness's, and a `60` written
    // here is a number that stops tracking it the day it changes.
    let plain = McpServer::stdio("plain", "cmd");
    let after = io_cli::edit::apply("", &[servers::add(&plain)]).unwrap();

    for absent in ["args", "env", "timeout_secs", "url", "headers"] {
        assert!(
            !after.contains(absent),
            "`{absent}` was written for a server that has none: {after}",
        );
    }
    // And the round trip is still exact, which is what makes the absence safe.
    let config = Config::from_toml(&after).expect("the written file loads");
    assert_eq!(config.mcp_servers(), [plain].as_slice());
}

#[test]
fn f6_a_key_no_mcp_entry_carries_is_refused_rather_than_written() {
    // THE SABOTAGE, and it is the quietest write in this crate. `comand = "…"`
    // would be spliced in, accepted by io-harness on `configure::write`'s round
    // trip, reported to the operator as written — and ignored by every turn
    // after it. There is no later failure to catch it, so the refusal has to
    // happen here.
    let at = At::of(Scope::User, TWO_SERVERS, "docs").expect("the fixture names `docs`");
    assert!(servers::edit(&at, "comand", "\"mcp-find\"").is_none());
    assert!(
        servers::edit(&at, "enabled", "false").is_none(),
        "there is no `enabled` key; the module docs say why the verb is not offered",
    );
    assert!(servers::edit(&at, "command", "\"mcp-find\"").is_some());
    for key in servers::KEYS {
        assert!(
            servers::edit(&at, key, "\"x\"").is_some(),
            "`{key}` is listed and refused",
        );
    }

    // The control: the file really would have taken the misspelling, so the
    // refusal above is load-bearing rather than belt-and-braces.
    let smuggled = io_cli::edit::apply(
        TWO_SERVERS,
        &[io_cli::edit::Edit::set("mcp[0].comand", "\"mcp-find\"")],
    )
    .unwrap();
    assert!(
        Config::from_toml(&smuggled).is_ok(),
        "io-harness rejected a misspelled `[[mcp]]` key, so the exemption this \
         refusal exists for is gone and the const can go with it",
    );
}

#[test]
fn f6_a_position_is_found_by_id_and_can_never_be_a_row_number() {
    // The 0.20.0 defect, closed by construction: `pluginview::rows` drew two
    // lists in an order no file shares, and a row number handed to a remover
    // deletes whichever entry happens to sit there. Nothing outside the module
    // can build an `At`, so there is no way left to spell that call.
    assert_eq!(
        At::of(Scope::User, TWO_SERVERS, "docs").map(|a| a.index()),
        Some(0),
    );
    assert_eq!(
        At::of(Scope::User, TWO_SERVERS, "search").map(|a| a.index()),
        Some(1),
    );
    assert!(
        At::of(Scope::User, TWO_SERVERS, "never-configured").is_none(),
        "a position was invented for a server no file names",
    );
    assert!(At::of(Scope::User, "", "docs").is_none());

    // An id spelled with an escape is the same id, and the comparison is made
    // against both spellings because this module may not parse TOML to settle it.
    let escaped = io_cli::edit::apply("", &[servers::add(&McpServer::stdio("we\"ird", "cmd"))])
        .expect("the entry is written");
    assert_eq!(
        At::of(Scope::User, &escaped, "we\"ird").map(|a| a.index()),
        Some(0),
    );

    // The scope rides along, because an index means nothing without the file it
    // counts in — and it is the scope `configure::write` has to be handed.
    assert_eq!(
        At::of(Scope::Local, TWO_SERVERS, "docs")
            .expect("`docs`")
            .scope,
        Scope::Local,
    );
}

#[test]
fn f6_the_only_entry_and_the_last_of_several_both_come_out_clean() {
    const ONLY: &str = r#"[[mcp]]
id = "docs"
transport = "stdio"
command = "mcp-docs"
"#;
    let at = At::of(Scope::User, ONLY, "docs").expect("the fixture names `docs`");
    let after = io_cli::edit::apply(ONLY, &[servers::remove(&at)]).unwrap();
    assert_eq!(after, "", "the last server left bytes behind");
    assert!(Config::from_toml(&after)
        .expect("a file with nothing in it is a configuration")
        .mcp_servers()
        .is_empty(),);

    const THREE_AND_A_SECTION: &str = r#"[[mcp]]
id = "a"
transport = "stdio"
command = "mcp-a"

[[mcp]]
id = "b"
transport = "stdio"
command = "mcp-b"

[[mcp]]
id = "c"
transport = "stdio"
command = "mcp-c"

[run]
max_steps = 30
"#;
    let at = At::of(Scope::User, THREE_AND_A_SECTION, "c").expect("the fixture names `c`");
    assert_eq!(at.index(), 2);
    let after = io_cli::edit::apply(THREE_AND_A_SECTION, &[servers::remove(&at)]).unwrap();

    let config = Config::from_toml(&after).expect("the written file loads");
    let left: Vec<&str> = config.mcp_servers().iter().map(|s| s.id.as_str()).collect();
    assert_eq!(left, vec!["a", "b"]);
    assert!(
        !after.contains("mcp-c"),
        "the removed entry left bytes behind"
    );
    assert!(
        after.contains("max_steps = 30"),
        "the section after the last entry was taken with it",
    );
}

#[test]
fn f6_the_file_a_row_names_is_the_file_its_position_is_read_from() {
    let dir = tempfile::tempdir().expect("a temporary directory");
    let path = dir.path().join(io_harness::config::PROJECT_FILE);
    std::fs::write(&path, TWO_SERVERS).expect("the fixture is written");

    // A row as the panel builds it, pointing at the file its origin column drew.
    let row = Server {
        id: "search".into(),
        transport: "mcp-search".into(),
        decided: Decided::File {
            scope: Scope::Project,
            path: path.clone(),
        },
        state: Reached::NotYet,
    };
    let at = servers::declared_at(&row).expect("the file names `search`");
    assert_eq!(at.index(), 1);
    assert_eq!(at.scope, Scope::Project);

    // A server io-harness's own default supplied has no file to edit, and this
    // says so rather than handing back position 0 of something.
    let defaulted = Server {
        decided: Decided::Default,
        ..row.clone()
    };
    assert!(servers::declared_at(&defaulted).is_none());

    // A row whose file no longer names it — an operator editing `io.toml` under
    // the session — is the same answer, and not a write to whatever is at 1 now.
    let stale = Server {
        id: "renamed-since".into(),
        ..row
    };
    assert!(servers::declared_at(&stale).is_none());
}

#[test]
fn f6_a_shadowed_server_is_not_offered_for_editing_because_it_is_not_running() {
    // `[[mcp]]` is NOT one of io-harness's appending keys (`config.rs:2052`):
    // the winning scope replaces the array WHOLE, "because a half-merged MCP
    // server definition is not a server". So an entry named only in the project
    // file, while a local file declares any `[[mcp]]` at all, is not
    // lower-priority — it is not in force. Editing it would change a file and
    // nothing an operator can observe, which is the worst kind of successful
    // write.
    let dir = tempfile::tempdir().expect("a temporary directory");
    std::fs::write(
        dir.path().join(io_harness::config::PROJECT_FILE),
        TWO_SERVERS,
    )
    .expect("the project file");
    std::fs::write(
        dir.path().join(io_harness::config::LOCAL_FILE),
        "[[mcp]]\nid = \"local-only\"\ntransport = \"stdio\"\ncommand = \"mcp-local\"\n",
    )
    .expect("the local file");

    let at = servers::declared_in(dir.path(), "local-only").expect("the local file names it");
    assert_eq!(at.scope, Scope::Local);
    assert_eq!(at.index(), 0);

    assert!(
        servers::declared_in(dir.path(), "docs").is_none(),
        "a shadowed entry was offered as editable: the local scope replaces the \
         array whole, so `docs` is not a server this workspace runs",
    );
}
