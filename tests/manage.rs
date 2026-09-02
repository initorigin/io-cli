//! F6, F7 and F8 — the one parse both entry paths go through, what `--` ends,
//! and the transport a form decides.
//!
//! Every test here drives `io_cli::manage` the way both doors drive it: a token
//! slice starting at the surface word. `io mcp add …` leaves exactly that after
//! the binary name, and `manage::tokens` makes exactly that of `/mcp add …`, so a
//! test written against one is a test of both — which is the property the module
//! exists to have and the reason the parse is in the library rather than in
//! `src/main.rs`, which nothing under `tests/` can link.

use std::path::{Path, PathBuf};

use io_cli::manage::{self, ConfigVerb, McpVerb, PluginVerb, Request};
use io_harness::config::Scope;
use io_harness::McpTransport;

mod support;

/// The token slice a shell hands `io`, spelled the way a test can read.
fn argv(words: &[&str]) -> Vec<String> {
    words.iter().map(|word| word.to_string()).collect()
}

/// `Config::discover` reads `IO_CONFIG` at call time, so two tests setting it at
/// once would each see the other's file. The scope test sets it; the `plugin
/// remove` tests below resolve a real configuration and would see it set, so they
/// take the same lock. Every other test here parses rather than plans.
///
/// Delegated to [`support::env_lock`] rather than declared here: two different
/// mutexes in one binary exclude nothing from each other, and the `support`
/// fixtures take that one.
fn env_lock() -> std::sync::MutexGuard<'static, ()> {
    support::env_lock()
}

/// Read a written file the way io-harness would read the operator's own.
///
/// **Never `Config::from_toml` for an `[[mcp]]` file.** `from_toml` parses at
/// `Scope::Project`, and io-harness 0.74.0 refuses MCP servers there — an MCP
/// server is a command, an argv and an environment this process spawns at run
/// start, and `io.toml` arrives with a `git clone`. `mcp add` writes the user
/// scope by default (`decided_scope`'s `unwrap_or(Scope::User)`), so the user
/// scope is where the round trip belongs.
///
/// Takes [`env_lock`] for itself, so a caller must not already hold it.
fn loaded(text: &str) -> io_harness::Config {
    support::user_scope(text).config.clone()
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
    // No declared bundle: every request written through this helper is an `mcp` or
    // `config` one, and `plan` reads the set only for `plugin remove`.
    let plan = manage::plan(root, request, &[])
        .expect("the request plans")
        .expect("a write, not a read");
    io_cli::edit::apply("", &plan.edits).expect("the edits apply to an empty file")
}

/// **F13, planned rather than parsed — the half that had no test at all.**
///
/// Every other `plan` call in this file passes `scope: None` against an empty
/// temporary root, so only `decided_scope`'s `unwrap_or(Scope::User)` fallback
/// ever ran: replacing the whole function with `|_, _, _| Scope::User` left the
/// suite green, and `io config set x y --scope project` would have written the
/// user file. The parse carrying `Some(Scope::Local)` is asserted elsewhere and is
/// not the same claim.
///
/// Sabotage: drop the inheritance lookup — under which the first arm fails,
/// because a key the project file decides would be written to the user file.
#[test]
fn f13_a_planned_config_write_inherits_the_deciding_file() {
    let home = tempfile::tempdir().expect("a home outside the workspace");
    let root = tempfile::tempdir().expect("a workspace");
    // The user file is written OUTSIDE the workspace: a file inside it is
    // project-scoped whatever variable names it.
    std::fs::write(home.path().join("io.toml"), "[run]\nmax_steps = 10\n").unwrap();
    std::fs::write(
        root.path().join("io.toml"),
        "[app.io-cli]\ntheme = \"dark\"\n",
    )
    .unwrap();

    let guard = env_lock();
    std::env::set_var("IO_CONFIG", home.path().join("io.toml"));

    // A key the PROJECT file decides, with no `--scope`, goes back to it.
    let inherited = manage::plan(
        root.path(),
        &manage::parse(&argv(&["config", "set", "app.io-cli.theme", "light"])).expect("it parses"),
        &[],
    )
    .expect("it plans")
    .expect("a write");
    assert_eq!(
        inherited.scope,
        Scope::Project,
        "a write with no `--scope` must go into the file already deciding the key, or a personal \
         file silently shadows a committed one"
    );

    // A key the USER file decides goes back to that.
    let user = manage::plan(
        root.path(),
        &manage::parse(&argv(&["config", "set", "run.max_steps", "20"])).expect("it parses"),
        &[],
    )
    .expect("it plans")
    .expect("a write");
    assert_eq!(user.scope, Scope::User);

    // An explicit `--scope` overrides the inheritance and moves the key.
    let moved = manage::plan(
        root.path(),
        &manage::parse(&argv(&[
            "config",
            "set",
            "app.io-cli.theme",
            "light",
            "--scope",
            "local",
        ]))
        .expect("it parses"),
        &[],
    )
    .expect("it plans")
    .expect("a write");
    assert_eq!(moved.scope, Scope::Local);

    // And `unset` follows the same rule, since it is the same decision.
    let unset = manage::plan(
        root.path(),
        &manage::parse(&argv(&["config", "unset", "app.io-cli.theme"])).expect("it parses"),
        &[],
    )
    .expect("it plans")
    .expect("a write");
    assert_eq!(unset.scope, Scope::Project);

    std::env::remove_var("IO_CONFIG");
    drop(guard);
}

/// `--flag=value` and `--flag value` are the same flag.
///
/// Untested anywhere until now, while the module's own comment argued that a parse
/// taking only one form "would be a parse that works on one surface" — a claim
/// carried by prose alone. Deleting the inline branch makes `--url=https://x` a
/// flag literally named `url=https://x`, refused as unknown.
#[test]
fn f8_a_flag_may_be_written_with_an_equals_sign() {
    let spaced = manage::parse(&argv(&[
        "mcp",
        "add",
        "linear",
        "--url",
        "https://mcp.linear.app/mcp",
    ]))
    .expect("the spaced form parses");
    let joined = manage::parse(&argv(&[
        "mcp",
        "add",
        "linear",
        "--url=https://mcp.linear.app/mcp",
    ]))
    .expect("the joined form parses");
    assert_eq!(spaced, joined, "the two spellings must be one flag");

    // A value that itself contains `=` keeps every character after the FIRST one,
    // which is what an `Authorization: Bearer …` header depends on.
    let request = manage::parse(&argv(&[
        "mcp",
        "add",
        "linear",
        "--url",
        "https://mcp.linear.app/mcp",
        "--header=Authorization=Bearer abc=123",
    ]))
    .expect("a joined header parses");
    match &server(&request).transport {
        io_harness::McpTransport::Http { headers, .. } => assert_eq!(
            headers.get("Authorization").map(String::as_str),
            Some("Bearer abc=123"),
            "a header value must split at the first `=` and keep the rest"
        ),
        other => panic!("expected an HTTP server, got {other:?}"),
    }

    // An empty flag name is refused rather than being read as a flag called "".
    assert!(manage::parse(&argv(&["mcp", "add", "linear", "--=x"])).is_err());
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

    // **This compared `parse(X)` with `parse(X)` and was a tautology**, because
    // the assertion above had already established that the two token slices are
    // the same slice. Comparing the parse against the VALUE it must produce is
    // what carries content: it fails if the tokeniser and the parser ever agree
    // with each other about something wrong.
    let from_slash = manage::parse(&typed).expect("the slash form parses");
    assert_eq!(
        from_slash,
        manage::Request::Config(manage::ConfigVerb::Set {
            key: "app.io-cli.gates.contains".to_string(),
            // The quotes are the shell's and the tokeniser's to remove; what
            // reaches the file is the TOML source for the words themselves.
            value: "\"all green\"".to_string(),
            scope: None,
        }),
        "the slash form must parse to the request the argv form does, and to this one"
    );
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
    // **`contains("--")` was here and could never fail**, because `"--url"`
    // contains it — so a refusal that stopped teaching the stdio form entirely
    // would have kept this test green while the criterion it serves says *both*
    // shapes are shown. The needle has to be the `--` that stands alone.
    // Taken from the refusal's real bytes and starting after the markup: the
    // sentence writes ``-- <command>`` inside backticks, so a needle of `"--"`
    // split on whitespace never matches the token it is aiming at. That is the
    // markup-inside-the-needle trap, and it is how a checker ends up checking
    // nothing.
    assert!(
        refusal.contains("-- <command>"),
        "the refusal must show the stdio form as its own shape, not only `--url`: {refusal}"
    );
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
    let config = loaded(&text);
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
        let plan = manage::plan(root.path(), &request, &[])
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
    let plan = manage::plan(root.path(), &request, &[])
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

/// **F4.** A widening is reported at parse time, for **both** files that live in
/// the workspace — and the advice names neither of them.
///
/// io-harness's `refuse_widening` runs before deserialization, so this is not a
/// rejected setting: it is a configuration file that no longer parses at all.
/// `configure::write` would roll it back and quote the harness; saying it here
/// means the file is never written.
///
/// **Both halves of this test asserted the defect until 0.35.0, and it was green.**
/// The guard read `Scope::Project` alone, so `--scope local` parsed cleanly and
/// this test asserted that it should — while the refusal on the other arm sent the
/// operator to `--scope local` as the remedy. io-harness 0.74.0 refuses
/// `io.local.toml` for the same reason it refuses `io.toml`: it is not committed,
/// but it sits in the workspace root a run's own agent can write to, so one
/// `write_file` of an unremarkable name was an escalation. io-cli was shipping
/// advice that fails, pinned by an assertion that it should.
#[test]
fn f4_a_widening_in_either_workspace_file_is_reported_rather_than_written() {
    for scope in ["project", "local"] {
        let refusal = manage::parse(&argv(&[
            "config",
            "set",
            "sandbox.mode",
            "full-access",
            "--scope",
            scope,
        ]))
        .unwrap_err();
        assert!(refusal.contains("sandbox.mode"), "{scope}: {refusal}");
        assert!(refusal.contains("full-access"), "{scope}: {refusal}");
        assert!(
            refusal.contains("--scope user"),
            "{scope}: the user scope is the only destination left, and a refusal that names a \
             file the harness also refuses is worse than one that names none: {refusal}"
        );
        assert!(
            !refusal.contains("--scope local"),
            "{scope}: `io.local.toml` is refused too — this is the sentence the release exists to \
             correct: {refusal}"
        );
    }

    // The user scope is the operator's own file and is not in a workspace, so the
    // same value parses there. The positive control matters: without it this test
    // passes against a guard that refuses every scope.
    let allowed = manage::parse(&argv(&[
        "config",
        "set",
        "sandbox.mode",
        "full-access",
        "--scope",
        "user",
    ]))
    .expect("the operator's own file may say it");
    assert_eq!(
        allowed,
        Request::Config(ConfigVerb::Set {
            key: "sandbox.mode".to_string(),
            value: "\"full-access\"".to_string(),
            scope: Some(Scope::User),
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
    let plan = manage::plan(root.path(), &request, &[])
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
    let plan = manage::plan(root.path(), &removal, &[])
        .expect("the server is declared")
        .expect("a write");
    assert_eq!(plan.scope, Scope::Project);
    let gone = io_cli::edit::apply(&text, &plan.edits).expect("the entry goes");
    // Read at the user scope, which is not the scope this file lives at: the
    // claim under test is that the removal took the right entry, and 0.74.0 would
    // refuse this text as a project file for declaring `[[mcp]]` at all — which is
    // a fact about the fixture's location, not about the edit.
    let config = loaded(&gone);
    let left: Vec<&str> = config.mcp_servers().iter().map(|s| s.id.as_str()).collect();
    assert_eq!(left, vec!["docs"]);

    // A server no file in force declares is a refusal naming it, not a write to
    // a position that was guessed.
    let missing = manage::parse(&argv(&["mcp", "remove", "absent"])).expect("it parses");
    let refusal = manage::plan(root.path(), &missing, &[]).expect_err("nothing declares it");
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
            manage::plan(root.path(), &request, &[])
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
    let refusal = manage::plan(root.path(), &request, &[]).expect_err("it is not a bundle");
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
    let plan = manage::plan(root.path(), &request, &[])
        .expect("it is a bundle now")
        .expect("a write");
    let after = io_cli::edit::apply("", &plan.edits).expect("the entry is written");
    assert!(after.contains("path = \"bundle\""), "{after}");
}

// --- `plugin remove <word>`: the path first, then the declared name -------------
//
// `io plugin add <name>` installs a bundle a marketplace holds and then tells the
// operator that `plugin remove <id>` takes it back out. Until this release that
// sentence was false: the verb resolved its word as a path only, so the id it had
// just printed found nothing and the refusal named a path nobody typed. The two
// readings now sit here in the order `marketplace::chosen` states — the disk is
// asked about the directory first, and nothing reads the *shape* of the word —
// and everything below is about that order, about never taking the first of two
// bundles sharing a name, and about the refusals saying which reading failed.
//
// Every fixture declares its bundles inside a fresh `tempfile` root and addresses
// entries by id and by path. No test here asserts a position in a
// `Config::discover` result: the user-scope file on the machine running the suite
// is discovered too, and six tests in an earlier release asserted indices over the
// developer's own `~/.io-cli/io.toml` and were green on CI alone.

/// A bundle directory at `at` whose manifest carries `name`.
fn manifest(root: &Path, at: &str, name: &str) -> PathBuf {
    let dir = root.join(at);
    std::fs::create_dir_all(&dir).expect("the bundle directory");
    std::fs::write(
        dir.join(io_cli::pluginview::MANIFEST),
        format!("name = \"{name}\"\n"),
    )
    .expect("the manifest");
    dir
}

/// A local-scope file declaring each `(path, enabled)` in order, and its bytes.
fn declaring(root: &Path, entries: &[(&str, bool)]) -> String {
    let text: String = entries
        .iter()
        .map(|(path, on)| {
            let off = if *on { "" } else { "enabled = false\n" };
            format!("[[plugin]]\npath = \"{path}\"\n{off}\n")
        })
        .collect();
    std::fs::write(root.join(io_harness::config::LOCAL_FILE), &text).expect("the configuration");
    text
}

/// The `(id, directory)` pairs a door hands `plan`, resolved the way both doors
/// resolve them — through `resolved::Resolved`, which is the one module permitted
/// to call `Config::plugins()`.
fn declared_bundles(root: &Path) -> Vec<(String, PathBuf)> {
    let config = io_harness::Config::discover(root).expect("the configuration discovers");
    io_cli::pluginview::ids(&io_cli::pluginview::view(
        io_cli::resolved::Resolved::load(&config).loaded(),
    ))
}

/// The file after the removal `line` plans, or the refusal it gave instead.
fn removing(root: &Path, before: &str, line: &str) -> Result<(Scope, String), String> {
    let request = manage::parse(&manage::tokens(line)).expect("the line parses");
    let plan = manage::plan(root, &request, &declared_bundles(root))?
        .expect("a removal is a write, not a read");
    Ok((
        plan.scope,
        io_cli::edit::apply(before, &plan.edits).expect("the edits apply to the file"),
    ))
}

/// **The path reading is asked first, and a word that is both a declared directory
/// and another bundle's name is the directory.**
///
/// This is `marketplace::chosen`'s rule for `plugin add`, kept for `plugin remove`:
/// the disk answers, and nothing keys on whether the word holds a `/`, a leading
/// `.` or an extension. `docs/guide/headless.md` documents
/// `io plugin remove ./bundles/rust-review`, and `tests/commands.rs` sends the same
/// spelling through the slash form.
///
/// Sabotage: match declared ids before asking the disk. Only this test fails —
/// and it fails by removing `bundles/elsewhere`, the entry the operator did not
/// name, which is the silent wrong delete this whole path is written against.
#[test]
fn f6_plugin_remove_reads_a_word_that_is_a_declared_directory_as_the_directory() {
    // Every test below resolves a real configuration, and `Config::discover` reads
    // `IO_CONFIG` at call time — so they take the same lock the scope test does.
    let _lock = env_lock();
    let dir = tempfile::tempdir().expect("a temporary directory");
    let root = dir.path();
    // A directory called `twin`, and a *different* directory whose manifest is
    // called `twin`. One word, two readings, and the disk decides.
    manifest(root, "twin", "other-bundle");
    manifest(root, "bundles/elsewhere", "twin");
    let before = declaring(root, &[("twin", true), ("bundles/elsewhere", true)]);

    let (scope, after) = removing(root, &before, "plugin remove twin").expect("the directory");
    assert_eq!(
        scope,
        Scope::Local,
        "the file that declared it is the file edited"
    );
    assert!(
        !after.contains("path = \"twin\""),
        "the directory named on the command line is still declared: {after}",
    );
    assert!(
        after.contains("path = \"bundles/elsewhere\""),
        "the bundle whose manifest is called `twin` was removed instead of the \
         directory the operator typed: {after}",
    );

    // The documented spelling resolves to the same entry it always has.
    let (_, after) =
        removing(root, &before, "plugin remove ./bundles/elsewhere").expect("the directory");
    assert!(
        !after.contains("path = \"bundles/elsewhere\""),
        "`./` in front of a declared directory stopped resolving, and that is the \
         spelling `docs/guide/headless.md` prints: {after}",
    );
    assert!(after.contains("path = \"twin\""), "{after}");
}

/// **A word no file declares as a directory is read as a declared bundle's name**,
/// which is what `plugin add <name>` tells the operator to type.
///
/// Sabotage: drop the id fallback and keep the path reading alone — the state this
/// release found. Only this test and the two below fail, and what ships is an
/// install that prints a removal command which cannot work.
#[test]
fn f6_plugin_remove_takes_the_name_a_declared_bundle_carries() {
    let _lock = env_lock();
    let dir = tempfile::tempdir().expect("a temporary directory");
    let root = dir.path();
    manifest(root, "bundles/first", "alpha-bundle");
    manifest(root, "bundles/second", "beta-bundle");
    let before = declaring(root, &[("bundles/first", true), ("bundles/second", true)]);

    let (scope, after) =
        removing(root, &before, "plugin remove beta-bundle").expect("the name resolves");
    assert_eq!(scope, Scope::Local);
    assert!(
        !after.contains("path = \"bundles/second\""),
        "the entry declaring the bundle called `beta-bundle` is still there: {after}",
    );
    assert!(
        after.contains("path = \"bundles/first\""),
        "the wrong entry was removed — a name resolved to a neighbour: {after}",
    );
}

/// **Two declared bundles of one name are refused, with both directories named.**
///
/// `Listed::id` is unique among the bundles io-harness *loaded*; two declared
/// `enabled = false` may share one, which is the `tools-v1`/`tools-v2` swap the
/// flag exists for. Taking the first of them deletes a `[[plugin]]` entry the
/// operator never pointed at, and nothing says so until a bundle's skills stop
/// being offered — so the refusal hands back the spelling that disambiguates,
/// which is the directory, and which is what the path reading above resolves.
///
/// Sabotage: return the first hit instead of collecting them. Only this test
/// fails, and it fails silently in the field.
#[test]
fn f6_plugin_remove_refuses_two_bundles_of_one_name_and_names_both_directories() {
    let _lock = env_lock();
    let dir = tempfile::tempdir().expect("a temporary directory");
    let root = dir.path();
    manifest(root, "bundles/one", "twinned");
    manifest(root, "bundles/two", "twinned");
    // Both switched off: io-harness reserves an id only for a bundle it switched
    // on, so this is the shape in which two entries genuinely share one.
    let before = declaring(root, &[("bundles/one", false), ("bundles/two", false)]);

    let refusal = removing(root, &before, "plugin remove twinned")
        .expect_err("one name, two bundles, no answer");
    for named in ["bundles/one", "bundles/two"] {
        assert!(
            refusal.contains(named),
            "the refusal must name every candidate's path, and `{named}` is not in \
             it: {refusal}",
        );
    }
    assert!(
        refusal.contains('2'),
        "the refusal should say how many bundles answer to the name: {refusal}",
    );

    // The fixture is sound: each of them is still removable by its directory, so
    // the refusal above is about the ambiguity and not about a broken file.
    let (_, after) = removing(root, &before, "plugin remove bundles/two").expect("the directory");
    assert!(!after.contains("path = \"bundles/two\""), "{after}");
    assert!(after.contains("path = \"bundles/one\""), "{after}");
}

/// **A word that is neither reading is refused as neither**, naming the directory
/// that was looked for and the name that was looked up.
///
/// The refusal before this release named only the path — a path the operator never
/// typed, because they had typed a name — and left them with no way to tell a
/// misspelt directory from a misspelt bundle.
///
/// Sabotage: drop the name half of the sentence and keep the old path-only
/// refusal. Only this test fails.
#[test]
fn f6_plugin_remove_refuses_a_word_that_is_neither_a_directory_nor_a_name() {
    let _lock = env_lock();
    let dir = tempfile::tempdir().expect("a temporary directory");
    let root = dir.path();
    manifest(root, "bundles/first", "alpha-bundle");
    let before = declaring(root, &[("bundles/first", true)]);

    let refusal = removing(root, &before, "plugin remove nowhere-at-all")
        .expect_err("neither a declared directory nor a declared name");
    assert!(
        refusal.contains(&root.join("nowhere-at-all").display().to_string()),
        "the directory reading is not reported, so an operator who mistyped a path \
         cannot see what was looked for: {refusal}",
    );
    assert!(
        refusal.contains("is called `nowhere-at-all`"),
        "the refusal names only the path, which is the sentence this release \
         replaced: an operator who typed a name is told about a path they never \
         wrote: {refusal}",
    );
}

/// **A bundle that did not load is removable by the name the listing shows**, and
/// it is the one an operator most wants gone.
///
/// A directory with no manifest is dropped by io-harness, listed under
/// `pluginview::Refused`, and cannot be repaired from a manifest that is not there.
/// Its id is the directory's own name — asserted here rather than assumed, so that
/// a word which is secretly a path could not be what makes the removal work.
///
/// Sabotage: build the pairs from `view.plugins` alone and leave `view.refused`
/// out. Only this test fails, and what ships is a broken entry an operator can see
/// on `/plugin` and cannot take out by the name it is listed under.
#[test]
fn f6_plugin_remove_takes_the_name_of_a_bundle_that_was_refused() {
    let _lock = env_lock();
    let dir = tempfile::tempdir().expect("a temporary directory");
    let root = dir.path();
    manifest(root, "bundles/good", "good-bundle");
    std::fs::create_dir_all(root.join("bundles/ghost-bundle")).expect("a directory, no manifest");
    let before = declaring(
        root,
        &[("bundles/good", true), ("bundles/ghost-bundle", true)],
    );

    let declared = declared_bundles(root);
    assert!(
        declared
            .iter()
            .any(|(id, at)| id == "ghost-bundle" && at.ends_with("ghost-bundle")),
        "a dropped bundle is identified by the directory's own name, and the pairs \
         handed to `plan` must carry it: {declared:?}",
    );

    let (scope, after) =
        removing(root, &before, "plugin remove ghost-bundle").expect("the refused bundle");
    assert_eq!(scope, Scope::Local);
    assert!(
        !after.contains("path = \"bundles/ghost-bundle\""),
        "the entry declaring the bundle that would not load is still there: {after}",
    );
    assert!(
        after.contains("path = \"bundles/good\""),
        "the bundle that loaded was removed instead: {after}",
    );
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

/// **Every surface `manage::parse` accepts is a subcommand `clap` will route.**
///
/// 0.30.0 shipped `io skill add` in the README, the CHANGELOG and
/// `manage::verbs`, with a working parse, a working plan and a working arm in the
/// session — and clap answering `error: unrecognized subcommand 'skill'`, because
/// the door is the `Subcommand` enum in `src/cli.rs` and nothing had added it
/// there. Every one of the 1,609 tests passed: they all entered through
/// `manage::parse`, which is downstream of the routing that was missing.
///
/// **Found by running the published binary, not by the suite.** This is the gate
/// that makes the next one cheaper — it asks clap itself what it will accept,
/// rather than reading `src/cli.rs` as text, so it cannot be satisfied by a
/// comment.
///
/// Sabotage: delete `Skill(Manage)` from `cli::Subcommand`. Only this fails.
#[test]
fn every_managed_surface_has_a_subcommand_clap_will_route() {
    use clap::CommandFactory as _;

    let command = io_cli::cli::Cli::command();
    let routed: Vec<String> = command
        .get_subcommands()
        .map(|sub| sub.get_name().to_string())
        .collect();

    for surface in ["mcp", "plugin", "config", "skill"] {
        assert!(
            routed.iter().any(|name| name == surface),
            "`manage::parse` accepts `{surface}` and clap does not route it, so \
             `io {surface} …` answers `unrecognized subcommand` for a verb the \
             product documents. clap routes: {routed:?}",
        );
        // And the surface really is one the parse accepts — otherwise this test
        // would pass by asserting a name nothing else knows.
        let refusal = manage::parse(&[surface.to_string()])
            .expect_err("a bare surface with no verb is refused");
        // **The needle is the refusal's real words.** The first version of this
        // assertion looked for "io does not manage" and the sentence says "is not
        // a surface io manages", so it passed over exactly the state it was written
        // to catch — clap routing a surface the parse then rejects. Taken from the
        // bytes, which is this repository's own rule about prose needles.
        assert!(
            !refusal.contains("is not a surface io manages"),
            "`{surface}` is routed by clap but is not a surface `manage::parse` \
             knows, so the door opens onto a refusal: {refusal}",
        );
        assert!(
            refusal.contains("needs a verb after it"),
            "a bare `{surface}` should ask for a verb: {refusal}",
        );
    }
}

/// **F3 — a mistyped verb names the verbs that surface does take.**
///
/// The bare-surface gate above covers `io skill` with nothing after it. This
/// covers `io skill bogus`, which took a different arm and was wrong there until
/// 0.30.2: the unknown-verb arm listed `mcp`, `plugin` and `config`, so a
/// mistyped verb on the fourth surface fell through to the unknown-*surface* arm
/// and answered "`skill` is not a surface io manages; they are `mcp`, `plugin`,
/// `skill` and `config`" — a sentence that denies and asserts the same fact, and
/// never names `add`, `list` or `remove`.
///
/// Two doors reach this parse and neither could see it: the sentence is
/// well-formed, the exit code is right, and only its *content* is wrong. Nothing
/// in the suite read a refusal's words for this family before this release.
///
/// Sabotage: remove `"skill"` from the unknown-verb arm in `src/manage.rs`. Only
/// this fails — and it fails on the `skill` row alone, which is why the loop
/// asserts per surface rather than over a joined string.
#[test]
fn f3_a_mistyped_verb_names_the_verbs_that_surface_takes() {
    // One distinctive verb per surface, chosen so no needle matches another
    // surface's list: `probe` is only mcp's, `marketplace` only plugin's, `unset`
    // only config's, and `<path>` only skill's argument spelling.
    let surfaces = [
        ("mcp", "probe"),
        ("plugin", "marketplace"),
        ("config", "unset"),
        ("skill", "<path>"),
    ];

    for (surface, distinctive) in surfaces {
        let refusal = manage::parse(&argv(&[surface, "definitely-not-a-verb"]))
            .expect_err("a verb no surface takes is refused");

        assert!(
            !refusal.contains("is not a surface io manages"),
            "`{surface} definitely-not-a-verb` was read as an unknown SURFACE \
             rather than an unknown verb, so the refusal denies that `{surface}` \
             is managed while listing it among the surfaces io manages: {refusal}",
        );
        assert!(
            refusal.contains("is not a verb"),
            "`{surface}` should refuse the word as a verb: {refusal}",
        );
        assert!(
            refusal.contains(distinctive),
            "the refusal for `{surface}` should name the verbs it takes, and \
             `{distinctive}` is the one only `{surface}` has: {refusal}",
        );
    }
}
