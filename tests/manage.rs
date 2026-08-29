//! F6, F7 and F8 — the one parse both entry paths go through, what `--` ends,
//! and the transport a form decides.
//!
//! Every test here drives `io_cli::manage` the way both doors drive it: a token
//! slice starting at the surface word. `io mcp add …` leaves exactly that after
//! the binary name, and `manage::tokens` makes exactly that of `/mcp add …`, so a
//! test written against one is a test of both — which is the property the module
//! exists to have and the reason the parse is in the library rather than in
//! `src/main.rs`, which nothing under `tests/` can link.

use std::path::Path;

use io_cli::manage::{self, ConfigVerb, McpVerb, PluginVerb, Request};
use io_harness::config::Scope;
use io_harness::McpTransport;

/// The token slice a shell hands `io`, spelled the way a test can read.
fn argv(words: &[&str]) -> Vec<String> {
    words.iter().map(|word| word.to_string()).collect()
}

/// The server a request declares, for a test that is about the transport.
fn server(request: &Request) -> &io_harness::McpServer {
    match request {
        Request::Mcp(McpVerb::Add { server, .. }) => server,
        other => panic!("expected an `mcp add`, got {other:?}"),
    }
}

/// The bytes a request writes into an empty file.
///
/// The whole point of F6 is that this is the comparison rather than a comparison
/// of parsed structs: two orderings could agree on every field of an `McpServer`
/// and still be written by two code paths that spell an entry differently.
fn written(root: &Path, request: &Request) -> String {
    let plan = manage::plan(root, request)
        .expect("the request plans")
        .expect("a write, not a read");
    io_cli::edit::apply("", &plan.edits).expect("the edits apply to an empty file")
}

// --- F7: `--` ends io's own arguments ------------------------------------------

#[test]
fn f7_a_command_after_the_dashes_is_taken_verbatim() {
    // The line from the module docs, end to end. `--store` is not a flag io has,
    // but that is not why it survives: nothing after the `--` is looked at at all.
    let request = manage::parse(&argv(&[
        "mcp",
        "add",
        "semlith",
        "--",
        "semlith",
        "--store",
        "/path/to/.semlith",
        "mcp",
    ]))
    .expect("the line parses");

    match &server(&request).transport {
        McpTransport::Stdio { command, args, env } => {
            assert_eq!(command, "semlith");
            // Verbatim, in order, with nothing consumed and nothing reordered.
            assert_eq!(args, &["--store", "/path/to/.semlith", "mcp"]);
            assert!(env.is_empty());
        }
        other => panic!("a command after `--` is a stdio server, got {other:?}"),
    }
    assert_eq!(server(&request).id, "semlith");
}

#[test]
fn f7_a_flag_after_the_dashes_belongs_to_the_server_and_not_to_io() {
    // `--plain` IS one of io's own flags, which is what makes this the arm worth
    // asserting: a scan that kept looking for its own flags past the `--` would
    // swallow it, start a server missing an argument its author wrote down, and
    // report success. The scan stops dead instead.
    let request = manage::parse(&argv(&[
        "mcp",
        "add",
        "web",
        "--",
        "web-server",
        "--plain",
        "--json",
    ]))
    .expect("a server's own flags after `--` are not io's");

    match &server(&request).transport {
        McpTransport::Stdio { command, args, .. } => {
            assert_eq!(command, "web-server");
            assert_eq!(args, &["--plain", "--json"]);
        }
        other => panic!("expected stdio, got {other:?}"),
    }
}

#[test]
fn f7_the_slash_form_and_the_argv_form_are_one_token_slice() {
    // The two doors, meeting before the parse. The shell removes the quotes on
    // its way to `io`, so the composer's tokeniser has to as well: a slash form
    // that kept them would write `"\"all green\""` — the same words, a different
    // value, and a difference nothing but a byte comparison would find.
    let typed = manage::tokens("/config set app.io-cli.gates.contains \"all green\"");
    assert_eq!(
        typed,
        argv(&["config", "set", "app.io-cli.gates.contains", "all green"])
    );

    let from_slash = manage::parse(&typed).expect("the slash form parses");
    let from_shell = manage::parse(&argv(&[
        "config",
        "set",
        "app.io-cli.gates.contains",
        "all green",
    ]))
    .expect("the argv form parses");
    assert_eq!(from_slash, from_shell);
}

// --- F8: the transport is decided by the form ----------------------------------

#[test]
fn f8_a_url_is_http_and_a_command_is_stdio() {
    let dialled = manage::parse(&argv(&[
        "mcp",
        "add",
        "linear",
        "--url",
        "https://mcp.linear.app/mcp",
    ]))
    .expect("a URL is a form");
    assert!(matches!(
        server(&dialled).transport,
        McpTransport::Http { .. }
    ));

    let started = manage::parse(&argv(&["mcp", "add", "docs", "--", "mcp-docs"]))
        .expect("a command is a form");
    assert!(matches!(
        server(&started).transport,
        McpTransport::Stdio { .. }
    ));
}

#[test]
fn f8_a_transport_that_disagrees_with_the_form_is_refused_by_name() {
    // Refused, never resolved. A precedence rule would silently discard half of
    // what the operator wrote, and which half depends on a rule nobody can see —
    // so the sentence names the flag AND the form it contradicts, because the
    // operator has to know which of the two to delete.
    let refusal = manage::parse(&argv(&[
        "mcp",
        "add",
        "linear",
        "--transport",
        "stdio",
        "--url",
        "https://mcp.linear.app/mcp",
    ]))
    .expect_err("a stdio server has no URL");
    assert!(refusal.contains("--transport stdio"), "{refusal}");
    assert!(refusal.contains("URL"), "{refusal}");

    let other = manage::parse(&argv(&[
        "mcp",
        "add",
        "docs",
        "--transport",
        "http",
        "--",
        "mcp-docs",
    ]))
    .expect_err("an HTTP server is dialled, not started");
    assert!(other.contains("--transport http"), "{other}");
    assert!(other.contains("`--`"), "{other}");
}

#[test]
fn f8_neither_form_is_refused_with_both_shapes_shown() {
    // The refusal an operator meets first, so it is the one that has to teach the
    // grammar rather than report its violation.
    let refusal =
        manage::parse(&argv(&["mcp", "add", "somewhere"])).expect_err("no form was given");
    assert!(refusal.contains("--url"), "{refusal}");
    assert!(refusal.contains("--"), "{refusal}");
    assert!(refusal.contains("command"), "{refusal}");
    assert!(refusal.contains("somewhere"), "{refusal}");
}

#[test]
fn f6_both_accepted_orderings_write_byte_identical_configuration() {
    // The criterion, and the sabotage it is aimed at: giving the foreign ordering
    // its own branch. Two branches can agree on every field of an `McpServer` and
    // still write an entry differently, so what is compared is the applied text.
    let root = tempfile::tempdir().expect("a temporary directory");

    let ours = manage::parse(&argv(&[
        "mcp",
        "add",
        "linear-server",
        "--url",
        "https://mcp.linear.app/mcp",
    ]))
    .expect("io's own ordering");
    let theirs = manage::parse(&argv(&[
        "mcp",
        "add",
        "--transport",
        "http",
        "linear-server",
        "https://mcp.linear.app/mcp",
    ]))
    .expect("the ordering another harness teaches");

    assert_eq!(
        written(root.path(), &ours),
        written(root.path(), &theirs),
        "two orderings, two different files"
    );
    // And the file they agree on is one io-harness reads back as the server that
    // was asked for — a byte comparison of two identically wrong entries would
    // pass without this.
    let text = written(root.path(), &ours);
    let config = io_harness::config::Config::from_toml(&text).expect("the entry loads");
    assert_eq!(config.mcp_servers().len(), 1);
    assert_eq!(config.mcp_servers()[0].id, "linear-server");
    assert!(matches!(
        config.mcp_servers()[0].transport,
        McpTransport::Http { .. }
    ));
}

#[test]
fn f8_the_two_orderings_share_one_parse_and_one_construction() {
    // The structural half of the criterion above, because a byte comparison would
    // go on passing over two branches that were kept in step by hand — until the
    // release where one of them gained a flag. Nothing under `tests/` can link a
    // driver, so this reads the module as text, the instrument `tests/servers.rs`
    // already uses on `src/main.rs`.
    let source =
        std::fs::read_to_string(Path::new(env!("CARGO_MANIFEST_DIR")).join("src/manage.rs"))
            .expect("the module");
    let flat: String = source.chars().filter(|c| !c.is_whitespace()).collect();

    assert_eq!(
        flat.matches("pubfnparse(").count(),
        1,
        "there is exactly one entry point, so neither door can grow a reading of \
         its own",
    );
    assert_eq!(
        flat.matches("McpTransport::Http{").count(),
        1,
        "an HTTP server is constructed in one place; a second is the branch the \
         foreign ordering is not allowed to have",
    );
    assert_eq!(
        flat.matches("McpTransport::Stdio{").count(),
        1,
        "a stdio server is constructed in one place",
    );
}

// --- the flags a transport admits ----------------------------------------------

#[test]
fn f8_env_on_an_http_server_and_a_header_on_a_stdio_one_are_refused_by_name() {
    // Neither is a value io-harness would reject: `McpTransport` is
    // `#[serde(tag = …)]` and the wrong key simply is not on the variant, so a
    // parse that accepted it would drop it on the floor and start a server
    // missing the credential it was given.
    let no_env = manage::parse(&argv(&[
        "mcp",
        "add",
        "linear",
        "--url",
        "https://mcp.linear.app/mcp",
        "--env",
        "TOKEN=abc",
    ]))
    .expect_err("an HTTP server has no child process");
    assert!(no_env.contains("--env"), "{no_env}");
    assert!(no_env.contains("--header"), "{no_env}");

    let no_header = manage::parse(&argv(&[
        "mcp",
        "add",
        "docs",
        "--header",
        "Authorization=Bearer x",
        "--",
        "mcp-docs",
    ]))
    .expect_err("a started server sends no headers");
    assert!(no_header.contains("--header"), "{no_header}");
    assert!(no_header.contains("--env"), "{no_header}");
}

#[test]
fn f7_repeated_env_and_header_accumulate() {
    // Repeatable, and the sabotage is a map that keeps the last: a server given
    // two variables would start with one, which no test of a single pair sees.
    let started = manage::parse(&argv(&[
        "mcp",
        "add",
        "docs",
        "--env",
        "TOKEN=abc",
        "--env",
        "HOME=/tmp",
        "--timeout-secs",
        "30",
        "--",
        "mcp-docs",
    ]))
    .expect("two variables");
    match &server(&started).transport {
        McpTransport::Stdio { env, .. } => {
            assert_eq!(env.len(), 2);
            assert_eq!(env["TOKEN"], "abc");
            assert_eq!(env["HOME"], "/tmp");
        }
        other => panic!("expected stdio, got {other:?}"),
    }
    assert_eq!(server(&started).timeout_secs, 30);

    let dialled = manage::parse(&argv(&[
        "mcp",
        "add",
        "linear",
        "--url",
        "https://mcp.linear.app/mcp",
        "--header",
        "Authorization=Bearer x",
        "--header",
        "X-Trace=1",
    ]))
    .expect("two headers");
    match &server(&dialled).transport {
        McpTransport::Http { headers, .. } => {
            assert_eq!(headers.len(), 2);
            // Split at the FIRST `=`, so a value carrying one survives whole.
            assert_eq!(headers["Authorization"], "Bearer x");
            assert_eq!(headers["X-Trace"], "1");
        }
        other => panic!("expected http, got {other:?}"),
    }
}

// --- config --------------------------------------------------------------------

#[test]
fn f6_config_set_takes_a_value_the_kind_admits_and_names_the_options_for_one_it_does_not() {
    // Three kinds, because they fail differently: a flag has two words, a choice
    // has a set the dependency owns, and a number has a shape. Until this release
    // every key was typed blind, and a value the schema rejects reached the file.
    let root = tempfile::tempdir().expect("a temporary directory");

    for (key, value, written_as) in [
        ("sandbox.allow_network", "true", "true"),
        ("sandbox.mode", "read-only", "\"read-only\""),
        ("run.max_steps", "30", "30"),
    ] {
        let request =
            manage::parse(&argv(&["config", "set", key, value])).expect("a value the kind admits");
        assert_eq!(
            request,
            Request::Config(ConfigVerb::Set {
                key: key.to_string(),
                // TOML source: a choice is a quoted string and a number and a
                // flag are bare, and a parse that quoted all three would write
                // `max_steps = "30"`, which io-harness cannot read back.
                value: written_as.to_string(),
                // `None`, not `Scope::User`: no `--scope` was given, and the file
                // already deciding the key is resolved by `plan` rather than
                // guessed at parse time. Collapsing the two is F13's own sabotage.
                scope: None,
            })
        );
        let plan = manage::plan(root.path(), &request)
            .expect("it plans")
            .expect("a write");
        assert_eq!(plan.scope, Scope::User);
    }

    let flag = manage::parse(&argv(&["config", "set", "sandbox.allow_network", "yes"]))
        .expect_err("`yes` is not a boolean");
    assert!(flag.contains("true"), "{flag}");
    assert!(flag.contains("false"), "{flag}");

    let choice = manage::parse(&argv(&["config", "set", "sandbox.mode", "full"]))
        .expect_err("`full` is not a mode");
    for option in ["read-only", "workspace-write", "full-access"] {
        assert!(choice.contains(option), "{option} missing from {choice}");
    }

    let number = manage::parse(&argv(&["config", "set", "run.max_steps", "lots"]))
        .expect_err("`lots` is not a number");
    assert!(number.contains("whole number"), "{number}");
    let negative = manage::parse(&argv(&["config", "set", "run.max_steps", "-1"]))
        .expect_err("a step count cannot be negative");
    assert!(negative.contains("negative"), "{negative}");
}

#[test]
fn f6_config_unset_deletes_the_line_and_not_the_section() {
    // `unset` and `remove` read as the same call at a call site and are the
    // difference between clearing one setting and deleting an operator's whole
    // `[run]` block. The verb is settled by which constructor was written, so
    // that is what is asserted — and then proved on bytes.
    const FILE: &str = "\
[run]
max_steps = 30
max_retries = 2
";
    let root = tempfile::tempdir().expect("a temporary directory");
    let request =
        manage::parse(&argv(&["config", "unset", "run.max_steps"])).expect("unset parses");
    let plan = manage::plan(root.path(), &request)
        .expect("it plans")
        .expect("a write");

    assert_eq!(plan.edits, vec![io_cli::edit::Edit::unset("run.max_steps")]);
    assert_ne!(plan.edits, vec![io_cli::edit::Edit::remove("run")]);

    let after = io_cli::edit::apply(FILE, &plan.edits).expect("the line goes");
    assert!(!after.contains("max_steps"), "{after}");
    assert!(
        after.contains("[run]"),
        "the section went with the key: {after}"
    );
    assert!(after.contains("max_retries = 2"), "a sibling went: {after}");
}

#[test]
fn f4_a_widening_in_a_project_file_is_reported_rather_than_written() {
    // io-harness's `refuse_widening` runs before deserialization, so this is not
    // a rejected setting — it is a configuration file that no longer parses at
    // all. `configure::write` would roll it back and quote the harness; saying it
    // here means the file is never written.
    let refusal = manage::parse(&argv(&[
        "config",
        "set",
        "sandbox.mode",
        "full-access",
        "--scope",
        "project",
    ]))
    .expect_err("a committed file may not widen what a clone may do");
    assert!(refusal.contains("sandbox.mode"), "{refusal}");
    assert!(refusal.contains("full-access"), "{refusal}");
    assert!(refusal.contains("--scope local"), "{refusal}");

    // The same value in a file that is not shared is the operator's own business.
    let allowed = manage::parse(&argv(&[
        "config",
        "set",
        "sandbox.mode",
        "full-access",
        "--scope",
        "local",
    ]))
    .expect("a local file may say it");
    assert_eq!(
        allowed,
        Request::Config(ConfigVerb::Set {
            key: "sandbox.mode".to_string(),
            value: "\"full-access\"".to_string(),
            scope: Some(Scope::Local),
        })
    );
}

// --- the existing writers, reached rather than re-implemented -------------------

#[test]
fn f6_an_edit_and_a_removal_go_to_the_file_that_declares_the_server() {
    // The scope of a change to something that already exists is not the
    // operator's to choose: an index counted in one file's `[[mcp]]` array aimed
    // at another file's deletes a server nobody named.
    let root = tempfile::tempdir().expect("a temporary directory");
    std::fs::write(
        root.path().join(io_harness::config::PROJECT_FILE),
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
    .expect("the fixture is written");

    let request = manage::parse(&argv(&["mcp", "edit", "search", "--command", "mcp-find"]))
        .expect("one key at a time");
    let plan = manage::plan(root.path(), &request)
        .expect("the server is declared")
        .expect("a write");
    assert_eq!(plan.scope, Scope::Project);
    let text = std::fs::read_to_string(root.path().join(io_harness::config::PROJECT_FILE)).unwrap();
    let after = io_cli::edit::apply(&text, &plan.edits).expect("the edit applies");
    assert!(after.contains("command = \"mcp-find\""), "{after}");
    assert!(
        after.contains("command = \"mcp-docs\""),
        "the first entry was edited instead of the second: {after}"
    );

    // A `--scope` here is refused rather than honoured, by name.
    let aimed = manage::parse(&argv(&["mcp", "remove", "search", "--scope", "user"]))
        .expect_err("the scope is the file that declares it");
    assert!(aimed.contains("--scope"), "{aimed}");

    let removal = manage::parse(&argv(&["mcp", "remove", "search"])).expect("remove parses");
    let plan = manage::plan(root.path(), &removal)
        .expect("the server is declared")
        .expect("a write");
    assert_eq!(plan.scope, Scope::Project);
    let gone = io_cli::edit::apply(&text, &plan.edits).expect("the entry goes");
    let config = io_harness::config::Config::from_toml(&gone).expect("the file still loads");
    let left: Vec<&str> = config.mcp_servers().iter().map(|s| s.id.as_str()).collect();
    assert_eq!(left, vec!["docs"]);

    // A server no file in force declares is a refusal naming it, not a write to
    // a position that was guessed.
    let missing = manage::parse(&argv(&["mcp", "remove", "absent"])).expect("it parses");
    let refusal = manage::plan(root.path(), &missing).expect_err("nothing declares it");
    assert!(refusal.contains("absent"), "{refusal}");
}

#[test]
fn f6_a_reading_verb_plans_no_write_at_all() {
    // `Ok(None)` rather than an empty plan: an empty edit list handed to
    // `configure::write` creates a file, discovers the whole tree and reports
    // success for a question nobody asked to have answered in writing.
    let root = tempfile::tempdir().expect("a temporary directory");
    for line in [
        "mcp list",
        "mcp get docs",
        "plugin list",
        "config list",
        "config get run.max_steps",
    ] {
        let request = manage::parse(&manage::tokens(line)).expect(line);
        assert!(
            manage::plan(root.path(), &request)
                .expect("a read plans")
                .is_none(),
            "{line} planned a write"
        );
    }
}

#[test]
fn f6_a_plugin_directory_with_no_manifest_is_refused_before_anything_is_written() {
    // io-harness drops a `[[plugin]]` entry naming a directory with no manifest —
    // recorded and otherwise silently absent — so an entry written without this
    // check is a bundle an operator believes is loaded for a week. The refusal is
    // `pluginview`'s own sentence rather than a second one written here.
    let root = tempfile::tempdir().expect("a temporary directory");
    std::fs::create_dir(root.path().join("bundle")).expect("a directory that is not a bundle");

    let request = manage::parse(&argv(&["plugin", "add", "bundle"])).expect("it parses");
    assert_eq!(
        request,
        Request::Plugin(PluginVerb::Add {
            path: "bundle".into(),
            scope: Scope::User,
        })
    );
    let refusal = manage::plan(root.path(), &request).expect_err("it is not a bundle");
    assert!(refusal.contains(io_cli::pluginview::MANIFEST), "{refusal}");

    // And with a manifest it is declared as written, relative to the root, so a
    // committed entry works for everyone who clones it.
    std::fs::write(
        root.path()
            .join("bundle")
            .join(io_cli::pluginview::MANIFEST),
        "name = \"demo\"\n",
    )
    .expect("a manifest");
    let plan = manage::plan(root.path(), &request)
        .expect("it is a bundle now")
        .expect("a write");
    let after = io_cli::edit::apply("", &plan.edits).expect("the entry is written");
    assert!(after.contains("path = \"bundle\""), "{after}");
}

// --- refusals say what was wrong ------------------------------------------------

#[test]
fn every_refusal_names_what_was_wrong_and_what_is_accepted() {
    // No bare "invalid argument" anywhere: the operator is at a terminal with no
    // `--help` open, and a refusal that does not say what to type next costs them
    // a trip to documentation for something the parser already knew.
    for (line, expected) in [
        ("", "mcp"),
        ("mcp", "add"),
        ("mcp explode", "explode"),
        ("nothing list", "nothing"),
        ("mcp add web --url", "--url"),
        ("mcp add web --shout loud", "--shout"),
        ("mcp add web -x", "-x"),
        ("mcp add web --url http://x --url http://y", "--url"),
        // The commonest mistake there is: the `--` left out, so the command reads
        // as a URL that is not one. The refusal shows the line it should have been.
        ("mcp add web semlith", "semlith"),
        ("mcp add web --transport carrier -- x", "carrier"),
        ("mcp add web --timeout-secs soon -- x", "--timeout-secs"),
        ("mcp add web --env TOKEN -- x", "KEY=VALUE"),
        ("mcp add web --scope global -- x", "global"),
        ("mcp edit docs", "--command"),
        ("mcp edit docs --transport http", "--transport"),
        ("mcp list extra", "extra"),
        ("config set", "config list"),
        ("config set prices.as_of 2026-01-01", "prices.as_of"),
        ("config set run.max_steps 1 2", "quote"),
    ] {
        let refusal = match manage::parse(&manage::tokens(line)) {
            Err(refusal) => refusal,
            Ok(accepted) => panic!("`{line}` should have been refused, and became {accepted:?}"),
        };
        assert!(
            refusal.contains(expected),
            "`{line}` was refused with `{refusal}`, which does not name `{expected}`"
        );
        assert!(
            refusal.len() > 30,
            "`{line}` was refused with `{refusal}`, which is not a sentence"
        );
    }
}

/// F14 — the headless listing prints the origin column, asserted against the
/// driver as text.
///
/// **This gate exists because listing the criteria found F14's sabotage had no
/// site.** "Drop the origin column from the listing" is the arm the criterion
/// names, and the listing is rendered in `manage_main` inside `src/main.rs`, which
/// nothing under `tests/` can link. Every other criterion in this release has a
/// library gate; this one had none, and a criterion whose sabotage cannot be
/// executed is a criterion that is not being checked. That is the 0.23.0 shape —
/// three HIGH defects all in `src/main.rs`, invisible to 1,215 passing tests.
///
/// A weak instrument, and the only one available. It is the same one
/// `tests/providers.rs`'s ordering gate and `tests/context_share.rs` already use.
///
/// Sabotage: drop `setting.decided.word()` from the `ConfigVerb::List` arm — under
/// which this fails, and the headless answer starts disagreeing with `/config`
/// about which file decided a value.
#[test]
fn f14_the_headless_listing_prints_the_deciding_file() {
    // Normalised: a Windows checkout has `\r\n`, and a gate that sliced on `"\n"`
    // matched nothing and panicked on a green product in 0.19.0 and 0.23.0.
    let driver = std::fs::read_to_string("src/main.rs")
        .expect("the driver is beside the tests")
        .replace("\r\n", "\n");
    let at = driver
        .find("fn manage_main(")
        .expect("the argument forms are answered by `manage_main`");
    let body = &driver[at..];
    let end = body
        .find("\n/// The values one setting can take")
        .unwrap_or(body.len());
    let body = &body[..end];

    let list = body
        .find("ConfigVerb::List")
        .expect("`io config list` must be answered");
    let rest = &body[list..];
    // The arm ends at the next match arm.
    let arm_end = rest.find("_ => {}").unwrap_or(rest.len());
    let arm = &rest[..arm_end];
    assert!(
        arm.contains("decided.word()"),
        "`io config list` does not print the deciding file, so the headless answer \
         disagrees with `/config` about the same configuration"
    );
    assert!(
        arm.contains("configure::settings"),
        "the listing must come from the same reader `/config` draws, or the two \
         surfaces can list different keys"
    );

    // And the preflight disclosure goes to stderr, not stdout: a listing being
    // piped must not have prose in it. F14 says a preflight refusal is not a
    // failed operation, so it also must not change the exit status.
    let preflight = body
        .find("preflight::line")
        .expect("an MCP add reports the policy preflight");
    // The statement, not the line: `cargo fmt` puts the macro's arguments on
    // their own lines, so a one-line window sees the argument and never the
    // macro. The 200 bytes before the call are that statement and its comment.
    let statement = &body[preflight.saturating_sub(200)..preflight];
    assert!(
        statement.contains("eprintln!"),
        "the policy preflight must go to stderr, where it cannot contaminate a listing; \
         the 200 bytes before the call were: {statement:?}"
    );
    assert!(
        // `eprintln!` ends in `println!`, so the stdout macro has to be looked for
        // with the stderr one taken out first — a needle that matches its own
        // opposite is a gate that can never fail.
        !statement.replace("eprintln!", "").contains("println!("),
        "the policy preflight reached stdout, which puts prose in a pipe a script is reading"
    );
}
