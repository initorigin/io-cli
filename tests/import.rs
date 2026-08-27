//! `/import` — what another agent tool already knows, brought across once.
//!
//! Every fixture here is a temporary directory standing in for an operator's
//! home, so nothing in this file can read the machine it runs on. That is the
//! same property `src/import.rs` is built for — the roots are parameters, `$HOME`
//! is never consulted — and it is what makes the credential assertions below mean
//! anything: a test that reached the real `~/.codex` could not prove a secret was
//! never copied, because there would be a second copy of it already.
//!
//! No clocks and no sleeps. Every assertion is about bytes on disk or a value
//! parsed out of them.

use std::path::{Path, PathBuf};

use io_cli::import::{apply, detect, files, plan, Destination, Kind, Source};
use io_harness::config::Scope;

/// The value that must never reach a file io-cli writes.
const SECRET: &str = "sk-live-this-must-never-be-copied";

/// A variable no machine sets, so `${env:…}` for it is unresolvable.
const UNSET: &str = "IO_CLI_IMPORT_TEST_TOKEN";

fn put(path: &Path, text: &str) {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).expect("a fixture directory");
    }
    std::fs::write(path, text).expect("a fixture file");
}

/// Every file under `dir`, as `(relative path, bytes)`, sorted.
fn contents(dir: &Path) -> Vec<(PathBuf, Vec<u8>)> {
    fn walk(dir: &Path, base: &Path, out: &mut Vec<(PathBuf, Vec<u8>)>) {
        let Ok(entries) = std::fs::read_dir(dir) else {
            return;
        };
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_dir() {
                walk(&path, base, out);
            } else if let Ok(bytes) = std::fs::read(&path) {
                let relative = path.strip_prefix(base).unwrap_or(&path).to_path_buf();
                out.push((relative, bytes));
            }
        }
    }
    let mut out = Vec::new();
    walk(dir, dir, &mut out);
    out.sort();
    out
}

/// One number for a whole directory tree, through the crate's own digest.
fn fingerprint(dir: &Path) -> u64 {
    let mut bytes = Vec::new();
    for (path, content) in contents(dir) {
        bytes.extend_from_slice(path.to_string_lossy().as_bytes());
        bytes.push(0);
        bytes.extend_from_slice(&content);
        bytes.push(0);
    }
    io_cli::skills::digest(&bytes)
}

/// Whether any file under `dir` holds `needle`.
fn holds(dir: &Path, needle: &str) -> Option<PathBuf> {
    contents(dir).into_iter().find_map(|(path, bytes)| {
        String::from_utf8_lossy(&bytes)
            .contains(needle)
            .then_some(path)
    })
}

/// The `[[mcp]]` entries of a written configuration, as io-harness's own type.
///
/// **Deserialised, never string-matched.** The question this file has to answer
/// is "will io-harness read back what io-cli wrote", and a substring assertion
/// answers a different one — it would pass on a body that spelled every key right
/// and left off `transport`, which is the one field `#[serde(flatten)]` makes
/// mandatory and easy to forget.
fn written_servers(path: &Path) -> Vec<io_harness::McpServer> {
    let text = std::fs::read_to_string(path).expect("the configuration was written");
    let document: toml::Value = toml::from_str(&text).expect("it parses");
    let Some(entries) = document.get("mcp").and_then(toml::Value::as_array) else {
        return Vec::new();
    };
    entries
        .iter()
        .map(|entry| -> io_harness::McpServer {
            entry
                .clone()
                .try_into()
                .expect("every entry is an `io_harness::McpServer`")
        })
        .collect()
}

fn stdio(server: &io_harness::McpServer) -> (String, Vec<String>, Vec<String>) {
    match &server.transport {
        io_harness::McpTransport::Stdio { command, args, env } => (
            command.clone(),
            args.clone(),
            env.keys().cloned().collect::<Vec<_>>(),
        ),
        other => panic!("expected a stdio server, got {other:?}"),
    }
}

// ---------------------------------------------------------------------------
// A. Detection
// ---------------------------------------------------------------------------

#[test]
fn every_source_on_the_machine_is_found_and_named() {
    let home_root = tempfile::tempdir().expect("a fake home");
    let workspace = tempfile::tempdir().expect("a workspace");

    put(
        &home_root.path().join(".claude/CLAUDE.md"),
        "- be careful\n",
    );
    put(&home_root.path().join(".claude.json"), "{}");
    put(&home_root.path().join(".codex/AGENTS.md"), "- be brief\n");
    put(&home_root.path().join(".gemini/GEMINI.md"), "- hello\n");
    put(&workspace.path().join(".cursorrules"), "no tabs\n");
    put(&workspace.path().join("CONVENTIONS.md"), "# conventions\n");

    let found = detect(home_root.path(), workspace.path());
    let sources: Vec<Source> = found.iter().map(|one| one.source).collect();
    assert_eq!(
        sources,
        vec![
            Source::Claude,
            Source::Codex,
            Source::Gemini,
            Source::Cursor,
            Source::Conventions,
        ],
        "every source with a file on disk is reported, in the order a surface lists them",
    );
    assert!(
        found.iter().all(|one| !one.empty()),
        "none of these fixtures is empty: {:?}",
        found.iter().map(|one| one.bytes).collect::<Vec<_>>(),
    );
}

#[test]
fn a_source_with_nothing_in_its_files_is_found_and_reported_empty() {
    let home_root = tempfile::tempdir().expect("a fake home");
    let workspace = tempfile::tempdir().expect("a workspace");

    // The state Gemini is actually in on the machine this module was written
    // against: three files, all of them zero bytes.
    put(&home_root.path().join(".gemini/GEMINI.md"), "");
    put(
        &home_root.path().join(".gemini/antigravity/mcp_config.json"),
        "",
    );

    let found = detect(home_root.path(), workspace.path());
    assert_eq!(found.len(), 1, "Gemini is there: {found:?}");
    assert_eq!(found[0].source, Source::Gemini);
    assert_eq!(found[0].paths.len(), 2, "both files are named");
    assert!(
        found[0].empty(),
        "found-but-empty is a state of its own, not the same answer as absent",
    );
    assert!(
        found[0].says().contains("nothing to bring across"),
        "the sentence says so: {}",
        found[0].says(),
    );

    assert!(
        plan(&found, home_root.path(), Scope::Project).is_empty(),
        "an empty file offers the operator nothing to accept",
    );
}

#[test]
fn a_source_that_is_not_installed_is_not_found_at_all() {
    let home_root = tempfile::tempdir().expect("a fake home");
    let workspace = tempfile::tempdir().expect("a workspace");
    put(&home_root.path().join(".codex/AGENTS.md"), "- be brief\n");

    let found = detect(home_root.path(), workspace.path());
    assert_eq!(
        found.iter().map(|one| one.source).collect::<Vec<_>>(),
        vec![Source::Codex],
        "a source with no files is absent from the answer rather than present and empty",
    );
}

// ---------------------------------------------------------------------------
// The plan is a plan
// ---------------------------------------------------------------------------

#[test]
fn a_plan_that_is_declined_writes_nothing_and_leaves_the_home_byte_identical() {
    let home_root = tempfile::tempdir().expect("a fake home");
    let workspace = tempfile::tempdir().expect("a workspace");
    let home = tempfile::tempdir().expect("io's home");

    put(
        &home_root.path().join(".claude/CLAUDE.md"),
        "- be careful\n",
    );
    put(
        &home_root.path().join(".claude.json"),
        r#"{"mcpServers":{"semlith":{"command":"semlith","args":["mcp"]}}}"#,
    );
    put(
        &home_root.path().join(".claude/skills/thing/SKILL.md"),
        "# thing\n",
    );
    put(&home.path().join("io.toml"), "");
    put(&home.path().join("skills/.gitkeep"), "");

    let found = detect(home_root.path(), workspace.path());
    let items = plan(&found, home.path(), Scope::Project);
    assert!(
        items.len() >= 3,
        "there is a real plan to decline: {items:?}",
    );

    let before = (fingerprint(home.path()), fingerprint(workspace.path()));

    // The operator said no to all of it. What a surface hands back is the
    // ACCEPTED subset, and that subset is empty.
    let report = apply(&[], workspace.path());

    assert!(report.written.is_empty(), "{report:?}");
    assert_eq!(report.lines(), vec!["nothing was imported".to_string()]);
    assert_eq!(
        (fingerprint(home.path()), fingerprint(workspace.path())),
        before,
        "building a plan is a read; declining it must not have moved a byte",
    );
}

// ---------------------------------------------------------------------------
// C. MCP round trips
// ---------------------------------------------------------------------------

#[test]
fn a_claude_mcp_entry_round_trips_into_an_mcp_server() {
    let home_root = tempfile::tempdir().expect("a fake home");
    let workspace = tempfile::tempdir().expect("a workspace");
    let home = tempfile::tempdir().expect("io's home");

    // Where they actually are: under `projects["<absolute path>"]`, not beside
    // `settings.json` and not under `~/.claude/` at all.
    put(
        &home_root.path().join(".claude.json"),
        r#"{
          "mcpServers": {},
          "projects": {
            "/somewhere/else": {
              "mcpServers": {
                "semlith": {
                  "type": "stdio",
                  "command": "semlith",
                  "args": ["--store", "/s", "mcp"]
                }
              }
            }
          }
        }"#,
    );

    let found = detect(home_root.path(), workspace.path());
    let items: Vec<_> = plan(&found, home.path(), Scope::Project)
        .into_iter()
        .filter(|item| item.kind == Kind::Mcp)
        .collect();
    assert_eq!(items.len(), 1, "{items:?}");

    let report = apply(&items, workspace.path());
    assert!(report.refused.is_empty(), "{report:?}");
    assert_eq!(report.written.len(), 1, "{report:?}");

    let servers = written_servers(&workspace.path().join("io.toml"));
    assert_eq!(servers.len(), 1);
    assert_eq!(servers[0].id, "semlith");
    let (command, args, env) = stdio(&servers[0]);
    assert_eq!(command, "semlith");
    assert_eq!(args, vec!["--store", "/s", "mcp"]);
    assert!(env.is_empty(), "this entry declared no environment");
    assert!(
        servers[0].timeout_secs > 0,
        "the timeout is io-harness's own default, not a zero this crate invented",
    );
}

#[test]
fn a_codex_mcp_table_round_trips_into_an_mcp_server() {
    let home_root = tempfile::tempdir().expect("a fake home");
    let workspace = tempfile::tempdir().expect("a workspace");
    let home = tempfile::tempdir().expect("io's home");

    put(
        &home_root.path().join(".codex/config.toml"),
        "model = \"gpt-5\"\n\
         \n\
         [mcp_servers.semlith]\n\
         command = \"semlith\"\n\
         args = [\"--store\", \"/s\", \"mcp\"]\n\
         startup_timeout_sec = 30\n",
    );

    let found = detect(home_root.path(), workspace.path());
    let all = plan(&found, home.path(), Scope::Project);

    let model: Vec<_> = all.iter().filter(|item| item.kind == Kind::Model).collect();
    assert_eq!(model.len(), 1, "{all:?}");
    assert_eq!(model[0].model(), Some("gpt-5"));
    assert_eq!(
        model[0].to,
        Destination::Nowhere,
        "a model id does not name a vendor, so nothing is written for it here",
    );
    assert!(
        model[0]
            .provider_edit(io_cli::providers::Endpoint::OpenAi)
            .is_some(),
        "the caller that HAS chosen a vendor gets the edit",
    );

    let items: Vec<_> = all
        .into_iter()
        .filter(|item| item.kind == Kind::Mcp)
        .collect();
    assert_eq!(items.len(), 1);

    let report = apply(&items, workspace.path());
    assert!(report.refused.is_empty(), "{report:?}");

    let servers = written_servers(&workspace.path().join("io.toml"));
    assert_eq!(servers.len(), 1);
    assert_eq!(servers[0].id, "semlith");
    let (command, args, env) = stdio(&servers[0]);
    assert_eq!(command, "semlith");
    assert_eq!(
        args,
        vec!["--store", "/s", "mcp"],
        "the argument list survives the TOML source it was quoted out of",
    );
    assert!(env.is_empty());
}

// ---------------------------------------------------------------------------
// D. Credentials
// ---------------------------------------------------------------------------

#[test]
fn a_secret_in_a_servers_environment_reaches_no_file_io_cli_writes() {
    let home_root = tempfile::tempdir().expect("a fake home");
    let workspace = tempfile::tempdir().expect("a workspace");
    let home = tempfile::tempdir().expect("io's home");

    put(
        &home_root.path().join(".codex/config.toml"),
        &format!(
            "[mcp_servers.paid]\n\
             command = \"paid-mcp\"\n\
             \n\
             [mcp_servers.paid.env]\n\
             {UNSET} = \"{SECRET}\"\n"
        ),
    );
    put(
        &home_root.path().join(".claude.json"),
        &format!(
            r#"{{"mcpServers":{{"other":{{"command":"other-mcp","env":{{"{UNSET}":"{SECRET}"}}}}}}}}"#
        ),
    );

    let found = detect(home_root.path(), workspace.path());
    let items = plan(&found, home.path(), Scope::Project);
    let mcp: Vec<_> = items
        .iter()
        .filter(|item| item.kind == Kind::Mcp)
        .cloned()
        .collect();
    assert_eq!(mcp.len(), 2, "{items:?}");

    for item in &mcp {
        let form = item.form.as_deref().expect("a translated body");
        assert!(
            form.contains(&format!("${{env:{UNSET}}}")),
            "the NAME points at itself: {form}",
        );
        assert!(
            !form.contains(SECRET),
            "the value was read and written: {form}",
        );
        assert!(
            !item.says.contains(SECRET),
            "the value reached the sentence an operator sees: {}",
            item.says,
        );
    }

    let report = apply(&mcp, workspace.path());

    assert_eq!(
        holds(workspace.path(), SECRET),
        None,
        "the secret reached the workspace",
    );
    assert_eq!(
        holds(home.path(), SECRET),
        None,
        "the secret reached io's home",
    );

    // **And the unresolvable reference is refused at import time rather than at
    // the operator's next session.** io-harness treats an unset `${env:…}` as a
    // hard parse error, and `configure::write` round-trips through
    // `Config::discover` and rolls the file back when it does — so a server whose
    // secret has not been exported yet costs a sentence here instead of a dead
    // session later. Guarded on the variable genuinely being unset, because that
    // is the premise rather than the claim.
    if std::env::var_os(UNSET).is_none() {
        assert!(
            report.written.is_empty(),
            "an unresolvable `${{env:}}` must not be left in the file: {report:?}",
        );
        assert_eq!(report.refused.len(), 2, "{report:?}");
        let at = workspace.path().join("io.toml");
        assert!(
            !at.exists() || written_servers(&at).is_empty(),
            "the rollback left a `[[mcp]]` entry behind",
        );
    }
}

#[test]
fn the_codex_credential_file_is_never_listed_and_never_read() {
    let home_root = tempfile::tempdir().expect("a fake home");
    let workspace = tempfile::tempdir().expect("a workspace");
    let home = tempfile::tempdir().expect("io's home");

    put(
        &home_root.path().join(".codex/auth.json"),
        &format!(r#"{{"OPENAI_API_KEY":"{SECRET}"}}"#),
    );
    put(&home_root.path().join(".codex/AGENTS.md"), "- be brief\n");

    let listed = files(Source::Codex, home_root.path(), workspace.path());
    assert!(
        !listed.iter().any(|path| path.ends_with("auth.json")),
        "`auth.json` is in the list of files this module reads: {listed:?}",
    );

    let found = detect(home_root.path(), workspace.path());
    let items = plan(&found, home.path(), Scope::Project);
    assert!(
        items.iter().all(|item| !item.says.contains(SECRET)
            && item
                .form
                .as_deref()
                .is_none_or(|form| !form.contains(SECRET))),
        "a credential reached the plan: {items:?}",
    );

    apply(&items, workspace.path());
    assert_eq!(holds(workspace.path(), SECRET), None);
    assert_eq!(holds(home.path(), SECRET), None);
}

// ---------------------------------------------------------------------------
// Instructions
// ---------------------------------------------------------------------------

#[test]
fn instructions_land_somewhere_config_discover_reports() {
    let home_root = tempfile::tempdir().expect("a fake home");
    let workspace = tempfile::tempdir().expect("a workspace");
    let home = tempfile::tempdir().expect("io's home");

    put(
        &home_root.path().join(".claude/CLAUDE.md"),
        "# rules\n\nAlways run the linter before committing.\n",
    );

    let found = detect(home_root.path(), workspace.path());
    let items: Vec<_> = plan(&found, home.path(), Scope::Project)
        .into_iter()
        .filter(|item| item.kind == Kind::Instructions)
        .collect();
    assert_eq!(items.len(), 1, "{items:?}");
    assert_eq!(
        items[0].to,
        Destination::Instructions(Scope::Project),
        "the scope the caller asked for",
    );

    let report = apply(&items, workspace.path());
    assert!(report.refused.is_empty(), "{report:?}");
    assert_eq!(
        report.written.len(),
        1,
        "the report names the destination, which is what makes this undoable by hand: {report:?}",
    );
    assert_eq!(report.written[0].0, workspace.path().join("AGENTS.md"));

    // The oracle is io-harness's own discovery, not the file being on disk:
    // `AGENTS.md` is `DEFAULT_INSTRUCTIONS`, so a project with no configuration
    // at all reads it — and this asserts that it did.
    let config = io_harness::config::Config::discover(workspace.path())
        .expect("the workspace configuration is readable");
    let read = config.instructions().join("\n");
    assert!(
        read.contains("Always run the linter before committing."),
        "io-harness did not read the imported instructions back: {read}",
    );
    assert!(
        read.contains("imported from"),
        "the provenance line went with it: {read}",
    );
}

// ---------------------------------------------------------------------------
// Skills
// ---------------------------------------------------------------------------

#[test]
fn a_skill_nested_five_deep_arrives_flat_and_is_found_by_discovery() {
    let home_root = tempfile::tempdir().expect("a fake home");
    let workspace = tempfile::tempdir().expect("a workspace");
    let home = tempfile::tempdir().expect("io's home");
    std::fs::create_dir_all(home.path().join("skills")).expect("a skills directory");

    put(
        &home_root
            .path()
            .join(".claude/plugins/repo/marketplace/pack/skills/deepone/SKILL.md"),
        "# deepone\n\nDoes the deep thing.\n",
    );

    let found = detect(home_root.path(), workspace.path());
    let items = plan(&found, home.path(), Scope::Project);
    assert_eq!(items.len(), 1, "{items:?}");
    assert_eq!(items[0].kind, Kind::Skill);
    assert_eq!(
        items[0].to,
        Destination::File(home.path().join("skills/deepone/SKILL.md")),
        "five levels down at the source, one level down at the destination",
    );

    let report = apply(&items, workspace.path());
    assert!(report.refused.is_empty(), "{report:?}");

    let discovered =
        io_harness::Skills::discover(home.path().join("skills")).expect("the directory discovers");
    assert!(
        discovered.get("deepone").is_some(),
        "the flattened skill is not in io-harness's own catalogue",
    );
}

#[cfg(unix)]
#[test]
fn a_symlinked_skill_arrives_as_a_real_file() {
    let home_root = tempfile::tempdir().expect("a fake home");
    let workspace = tempfile::tempdir().expect("a workspace");
    let home = tempfile::tempdir().expect("io's home");
    let elsewhere = tempfile::tempdir().expect("the checkout the link points at");
    std::fs::create_dir_all(home.path().join("skills")).expect("a skills directory");

    put(
        &elsewhere.path().join("graphify/SKILL.md"),
        "# graphify\n\nBuilds a graph.\n",
    );
    std::fs::create_dir_all(home_root.path().join(".claude/skills")).expect("a skills directory");
    std::os::unix::fs::symlink(
        elsewhere.path().join("graphify"),
        home_root.path().join(".claude/skills/graphify"),
    )
    .expect("a symlink");

    let found = detect(home_root.path(), workspace.path());
    let items = plan(&found, home.path(), Scope::Project);
    assert_eq!(items.len(), 1, "the walk followed the link: {items:?}");

    let report = apply(&items, workspace.path());
    assert!(report.refused.is_empty(), "{report:?}");

    let landed = home.path().join("skills/graphify/SKILL.md");
    let kind = std::fs::symlink_metadata(&landed).expect("it is there");
    assert!(
        !kind.is_symlink(),
        "the import made a link rather than materialising the file, so deleting the \
         operator's checkout would empty their skills directory",
    );
    assert_eq!(
        std::fs::read_to_string(&landed).expect("readable"),
        "# graphify\n\nBuilds a graph.\n",
    );
    assert!(io_harness::Skills::discover(home.path().join("skills"))
        .expect("it discovers")
        .get("graphify")
        .is_some());
}

#[test]
fn a_skill_name_that_is_already_taken_is_refused_rather_than_overwritten() {
    let home_root = tempfile::tempdir().expect("a fake home");
    let workspace = tempfile::tempdir().expect("a workspace");
    let home = tempfile::tempdir().expect("io's home");

    let mine = home.path().join("skills/mine/SKILL.md");
    put(&mine, "# mine\n\nThe operator's own.\n");
    put(
        &home_root.path().join(".claude/skills/mine/SKILL.md"),
        "# mine\n\nSomebody else's.\n",
    );

    let found = detect(home_root.path(), workspace.path());
    let items = plan(&found, home.path(), Scope::Project);
    assert_eq!(items.len(), 1, "{items:?}");
    assert_eq!(items[0].kind, Kind::Skill);
    assert_eq!(
        items[0].to,
        Destination::Nowhere,
        "a claimed name is refused in the plan, before anything can be written",
    );
    assert!(items[0].form.is_none(), "and it carries nothing to write");
    assert!(
        items[0].says.contains("already answers"),
        "the operator is told why: {}",
        items[0].says,
    );

    let report = apply(&items, workspace.path());
    assert!(report.written.is_empty(), "{report:?}");
    assert_eq!(
        std::fs::read_to_string(&mine).expect("still there"),
        "# mine\n\nThe operator's own.\n",
        "the operator's skill was overwritten",
    );
}

#[test]
fn sixty_three_present_and_three_incoming_is_reported_and_nothing_is_written() {
    let home_root = tempfile::tempdir().expect("a fake home");
    let workspace = tempfile::tempdir().expect("a workspace");
    let home = tempfile::tempdir().expect("io's home");

    for n in 0..63 {
        put(
            &home.path().join(format!("skills/held{n:02}.md")),
            &format!("# held{n:02}\n"),
        );
    }
    for name in ["alpha", "beta", "gamma"] {
        put(
            &home_root
                .path()
                .join(format!(".claude/skills/{name}/SKILL.md")),
            &format!("# {name}\n"),
        );
    }

    let before = fingerprint(home.path());
    let found = detect(home_root.path(), workspace.path());
    let items = plan(&found, home.path(), Scope::Project);
    assert_eq!(items.len(), 3, "{items:?}");
    for item in &items {
        assert_eq!(
            item.to,
            Destination::Nowhere,
            "63 + 3 is 66, and the ceiling is {}",
            io_harness::skills::MAX_SKILLS,
        );
        assert!(
            item.says.contains("no skill is imported") && item.says.contains("63"),
            "the numbers are in the sentence: {}",
            item.says,
        );
    }

    let report = apply(&items, workspace.path());
    assert!(report.written.is_empty(), "{report:?}");
    assert_eq!(
        fingerprint(home.path()),
        before,
        "going over the ceiling rejects the WHOLE directory at run start, so an import \
         that would cross it writes nothing at all",
    );
    assert_eq!(
        io_harness::Skills::discover(home.path().join("skills"))
            .expect("it still discovers")
            .len(),
        63,
        "and the operator's session still starts",
    );
}

// ---------------------------------------------------------------------------
// The allowlist, which is described and never translated
// ---------------------------------------------------------------------------

#[test]
fn an_allowlist_is_reported_and_no_policy_is_written() {
    let home_root = tempfile::tempdir().expect("a fake home");
    let workspace = tempfile::tempdir().expect("a workspace");
    let home = tempfile::tempdir().expect("io's home");

    put(
        &home_root.path().join(".codex/rules/default.rules"),
        "# the operator's own allowlist\n\
         prefix_rule(pattern=[\"bun\", \"install\"], decision=\"allow\")\n\
         prefix_rule(pattern=[\"cargo\", \"test\"], decision=\"allow\")\n",
    );
    put(
        &home_root.path().join(".claude/settings.json"),
        r#"{"model":"opusplan","permissions":{"ask":["Bash(cargo yank *)"]}}"#,
    );
    // One server, so there IS a configuration file to assert the absence in.
    put(
        &home_root.path().join(".claude.json"),
        r#"{"mcpServers":{"semlith":{"command":"semlith","args":["mcp"]}}}"#,
    );

    let found = detect(home_root.path(), workspace.path());
    let items = plan(&found, home.path(), Scope::Project);

    let allow: Vec<_> = items
        .iter()
        .filter(|item| item.kind == Kind::Allowlist)
        .collect();
    assert_eq!(
        allow.len(),
        2,
        "one per source that declares one: {items:?}"
    );
    for item in &allow {
        assert_eq!(item.to, Destination::Nowhere);
        assert!(item.form.is_none(), "there is no translated form to accept");
        assert!(
            item.says.contains("BINARY NAME"),
            "the sentence states why it cannot be translated: {}",
            item.says,
        );
        assert!(!item.kind.writes(), "and a surface can draw it as such");
    }

    let report = apply(&items, workspace.path());
    assert!(report.refused.is_empty(), "{report:?}");
    assert_eq!(
        report.carried.len(),
        3,
        "two allowlists and a model: {report:?}"
    );

    // Asserted on the PARSED result, not on a substring: a `[policy]` written as
    // a dotted key would be invisible to a `contains("[policy]")`.
    let at = workspace.path().join("io.toml");
    let text = std::fs::read_to_string(&at).expect("the MCP entry made a file");
    let document: toml::Value = toml::from_str(&text).expect("it parses");
    assert!(
        document.get("policy").is_none(),
        "an allowlist was translated into a policy: {text}",
    );
    assert_eq!(
        written_servers(&at).len(),
        1,
        "and the rest of the import still landed",
    );
}
