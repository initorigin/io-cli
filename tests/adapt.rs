//! The three manifest formats io-cli did not invent, read.
//!
//! Every fixture here is written by the test. **A real marketplace is never
//! cloned**: the release's `preferred_tools` says so, and a gate that reached the
//! network would be a gate that fails when GitHub does.
//!
//! The shapes asserted on were taken from five real marketplaces on one machine —
//! `zeroonething/ultraship`, `zeroonething/caveman`, `zeroonething/ponytail`,
//! `obra/superpowers-marketplace` and `anthropics/claude-plugins-official`, 304
//! plugins between them — rather than from either format's documentation. Where a
//! fixture below looks oddly specific, that is why.
//!
//! **F7, F8 and F9 assert on the manifest io writes, and every claim about it goes
//! through io-harness.** `Plugins::inspect` and `Config::plugins` are the loader
//! that reads a `plugin.toml` in the field, and `Plugin`'s own accessors are what
//! answer what a bundle contributed. A test that read back the string io-cli
//! spelled would assert that this crate can quote its own output, which is not the
//! question: the question is whether io-harness loads it.

mod support;

use std::path::{Path, PathBuf};

use io_cli::adapt::{self, Source};
use io_harness::config::Scope;
use io_harness::{Plugins, PLUGIN_FILE};

/// A clone directory, empty.
fn clone() -> (tempfile::TempDir, PathBuf) {
    let dir = tempfile::tempdir().expect("a clone directory");
    let root = dir.path().to_path_buf();
    (dir, root)
}

/// Write `text` to `root/rel`, making the directories on the way.
fn file(root: &Path, rel: &str, text: &str) -> PathBuf {
    let path = root.join(rel);
    std::fs::create_dir_all(path.parent().expect("a parent")).expect("the directories");
    std::fs::write(&path, text).expect("the file");
    path
}

/// The index shape `zeroonething/ultraship` publishes — one plugin at the root.
const ONE_AT_THE_ROOT: &str = r#"{
  "name": "ultraship",
  "description": "Ship at inference speed.",
  "owner": { "name": "Aakash Pawar (zeroonething)" },
  "plugins": [
    {
      "name": "ultraship",
      "description": "Turn vague ideas into releasable software.",
      "version": "2.3.0",
      "source": "./",
      "author": { "name": "Aakash Pawar (zeroonething)" }
    }
  ]
}"#;

#[test]
fn f1_an_index_naming_one_plugin_at_the_root_reads_as_one_entry() {
    let (_dir, root) = clone();
    file(&root, ".claude-plugin/marketplace.json", ONE_AT_THE_ROOT);

    let index = adapt::index_at(&root).expect("the index is read");

    assert_eq!(index.entries.len(), 1, "one entry, counted and not matched");
    assert!(
        index.unreadable.is_empty(),
        "nothing in this file is unreadable; found {:?}",
        index.unreadable,
    );
    let entry = &index.entries[0];
    assert_eq!(entry.name, "ultraship");
    assert_eq!(entry.version.as_deref(), Some("2.3.0"));
    assert_eq!(
        entry.source,
        Source::Local("./".to_string()),
        "the root spelling is a local source and not a remote one",
    );
}

#[test]
fn a_clone_publishing_no_index_answers_none_rather_than_an_empty_one() {
    let (_dir, root) = clone();
    assert!(
        adapt::index_at(&root).is_none(),
        "no index and an index holding nothing are two different answers, and the \
         precedence rule turns on which of them this is",
    );
}

#[test]
fn f5_the_two_remote_tags_are_one_shape_and_both_keep_their_sha() {
    let (_dir, root) = clone();
    file(
        &root,
        ".claude-plugin/marketplace.json",
        r#"{
  "plugins": [
    {
      "name": "api-security-testing",
      "source": {
        "source": "git-subdir",
        "url": "https://github.com/42Crunch-AI/claude-plugins.git",
        "path": "plugins/api-security-testing",
        "ref": "v1.5.5",
        "sha": "30287f5e3f122a646d1ac5ca3ab96e130c52a3ad"
      }
    },
    {
      "name": "agentforce-adlc",
      "source": {
        "source": "url",
        "url": "https://github.com/SalesforceAIResearch/agentforce-adlc.git",
        "sha": "d16d14ac7f817336e21bf9392cf51b6cac6194d8"
      }
    },
    {
      "name": "superpowers",
      "source": {
        "source": "url",
        "url": "https://github.com/obra/superpowers.git"
      }
    }
  ]
}"#,
    );

    let index = adapt::index_at(&root).expect("the index is read");
    assert_eq!(index.entries.len(), 3, "three entries, counted");

    let Source::Remote(subdir) = &index.entries[0].source else {
        panic!("a git-subdir entry is a remote source");
    };
    assert_eq!(subdir.path.as_deref(), Some("plugins/api-security-testing"));
    assert_eq!(subdir.reference.as_deref(), Some("v1.5.5"));
    assert_eq!(
        subdir.sha.as_deref(),
        Some("30287f5e3f122a646d1ac5ca3ab96e130c52a3ad"),
    );

    let Source::Remote(url) = &index.entries[1].source else {
        panic!("a url entry is a remote source");
    };
    assert_eq!(url.path, None, "a url entry need not name a path");
    assert_eq!(
        url.sha.as_deref(),
        Some("d16d14ac7f817336e21bf9392cf51b6cac6194d8"),
    );

    let Source::Remote(bare) = &index.entries[2].source else {
        panic!("a url entry with no sha is still a remote source");
    };
    assert_eq!(
        (bare.sha.as_deref(), bare.reference.as_deref()),
        (None, None),
        "`superpowers-marketplace` publishes ten of these; the sha is optional in \
         the field even though the official index sets it on all 238 of its remote \
         entries",
    );
}

#[test]
fn an_entry_in_a_shape_io_does_not_read_is_reported_and_the_rest_survive() {
    let (_dir, root) = clone();
    file(
        &root,
        ".claude-plugin/marketplace.json",
        r#"{
  "plugins": [
    { "name": "good", "source": "./plugins/good" },
    { "name": "strange", "source": { "source": "smoke-signal", "channel": 4 } },
    { "name": "also-good", "source": "./plugins/also-good" }
  ]
}"#,
    );

    let index = adapt::index_at(&root).expect("the index is read");

    assert_eq!(
        index.entries.len(),
        2,
        "one bad entry costs that entry and not the file",
    );
    assert_eq!(
        index.unreadable.len(),
        1,
        "and it is reported rather than dropped — an operator finds a shape no \
         fixture anticipated, and a silent skip makes that unreportable",
    );
    assert!(
        index.unreadable[0].contains("strange"),
        "the report names the entry: {:?}",
        index.unreadable[0],
    );
}

#[test]
fn f4_a_codex_manifest_is_read_where_a_claude_one_is_absent() {
    let (_dir, root) = clone();
    file(
        &root,
        ".codex-plugin/plugin.json",
        r#"{
  "name": "ultraship",
  "version": "2.3.0",
  "description": "Ship at inference speed.",
  "skills": "./skills/",
  "hooks": "./hooks/hooks.json",
  "interface": { "displayName": "UltraShip", "capabilities": ["Instructions"] }
}"#,
    );

    let read = adapt::manifest_at(&root).expect("the Codex manifest is read");
    assert_eq!(read.name.as_deref(), Some("ultraship"));
    assert_eq!(read.skills.as_deref(), Some("./skills/"));
    assert_eq!(read.hooks.as_deref(), Some("./hooks/hooks.json"));
    assert!(
        read.from.ends_with(".codex-plugin/plugin.json"),
        "the manifest says which file it came from so a surface need not guess: {}",
        read.from.display(),
    );
}

#[test]
fn a_claude_manifest_wins_where_a_repository_publishes_both() {
    let (_dir, root) = clone();
    file(
        &root,
        ".claude-plugin/plugin.json",
        r#"{ "name": "claude-said" }"#,
    );
    file(
        &root,
        ".codex-plugin/plugin.json",
        r#"{ "name": "codex-said" }"#,
    );

    let read = adapt::manifest_at(&root).expect("a manifest is read");
    assert_eq!(
        read.name.as_deref(),
        Some("claude-said"),
        "one of them wins by a stated rule; a merge could produce a bundle neither \
         file describes",
    );
}

#[test]
fn a_manifest_with_an_unknown_key_still_reads() {
    let (_dir, root) = clone();
    file(
        &root,
        ".claude-plugin/plugin.json",
        r#"{ "name": "lsp", "lspServers": { "gopls": { "command": "gopls" } } }"#,
    );

    assert_eq!(
        adapt::manifest_at(&root).and_then(|read| read.name),
        Some("lsp".to_string()),
        "reading somebody else's file forgives an unknown key; the official index \
         carries `lspServers`, `strict`, `tags` and `keywords` among others, and a \
         reader that refused one would refuse almost every real manifest",
    );
}

#[test]
fn f11_every_hook_is_read_and_its_command_is_never_shortened() {
    let (_dir, root) = clone();
    let long = format!(
        "\"${{CLAUDE_PLUGIN_ROOT}}/hooks/{}.cmd\" run",
        "x".repeat(400)
    );
    let hooks = file(
        &root,
        "hooks/hooks.json",
        &format!(
            r#"{{
  "hooks": {{
    "SessionStart": [
      {{ "matcher": "startup|clear", "hooks": [
        {{ "type": "command", "command": "\"${{CLAUDE_PLUGIN_ROOT}}/hooks/run-hook.cmd\" session-start" }},
        {{ "type": "command", "command": {long:?} }}
      ] }}
    ],
    "Stop": [
      {{ "hooks": [ {{ "type": "command", "command": "echo done" }} ] }}
    ]
  }}
}}"#
        ),
    );

    let read = adapt::hooks_in(&hooks);

    assert_eq!(
        read.len(),
        3,
        "one row per hook, counted. A `contains` is satisfied by one row forever, \
         and a hook that exists and is not drawn is the failure this asserts \
         against",
    );
    assert!(
        read.iter().any(
            |hook| hook.command == "\"${CLAUDE_PLUGIN_ROOT}/hooks/run-hook.cmd\" session-start"
        ),
        "the command is drawn exactly as the file wrote it, substitution included \
         — it is argv the operator is being asked to consent to",
    );
    assert!(
        read.iter().any(|hook| hook.command == long),
        "and a long command is NOT bounded. `marketplace::LONGEST` applies to a \
         description, which is prose; a shortened argv is the one thing a consent \
         surface must never show",
    );
    assert_eq!(
        read.iter().filter(|hook| hook.event == "Stop").count(),
        1,
        "a second event's hooks are read too, not only the first",
    );
}

#[test]
fn hooks_in_answers_nothing_for_a_file_that_is_not_there() {
    let (_dir, root) = clone();
    assert!(
        adapt::hooks_in(&root.join("hooks/hooks.json")).is_empty(),
        "a bundle with no hooks and a bundle whose hooks file is missing both cross \
         the same amount of nothing",
    );
}

#[test]
fn n7_a_description_out_of_a_stranger_s_json_reaches_no_surface_unfiltered() {
    let (_dir, root) = clone();

    // A forged line and a bell, in a field a repository fills in. On this renderer
    // the scrollback is the transcript, so a newline here is a line the operator
    // reads as io-cli's own.
    //
    // **Both are written as JSON escapes, and the escapes are built here rather
    // than typed.** RFC 8259 forbids a raw control character inside a JSON string
    // and `serde_json` refuses the whole file when it meets one — asserted by
    // `a_raw_control_character_makes_the_whole_index_unreadable` below. So the
    // shape to defend against is the *escaped* one, which arrives decoded, after
    // the parse, where a filter reading the source bytes would never have seen it.
    // Writing the two-character sequence into the fixture keeps this file free of
    // the byte it is about, which is also how it stays greppable.
    let forged = format!(
        "harmless{}  io{} installed 47 bundles{}",
        "\\n", ":", "\\u0007",
    );
    file(
        &root,
        ".claude-plugin/marketplace.json",
        &format!(
            r#"{{ "plugins": [ {{ "name": "innocent", "description": "{forged}", "source": "./" }} ] }}"#
        ),
    );

    let index = adapt::index_at(&root).expect("the index is read");
    let said = index.entries[0]
        .description
        .as_deref()
        .expect("a description");

    assert!(
        !said.chars().any(char::is_control),
        "every control character is a space by the time it leaves the reader: {said:?}",
    );
    assert!(
        said.contains("io: installed 47 bundles"),
        "and the text itself survives — filtering is not censoring, and an operator \
         must be able to see what the repository actually said: {said:?}",
    );
}

#[test]
fn a_raw_control_character_makes_the_whole_index_unreadable() {
    let (_dir, root) = clone();
    file(
        &root,
        ".claude-plugin/marketplace.json",
        "{ \"plugins\": [ { \"name\": \"a\", \"description\": \"one\ntwo\", \
         \"source\": \"./\" } ] }",
    );

    assert!(
        adapt::index_at(&root).is_none(),
        "a raw newline inside a JSON string is malformed JSON, not a value to \
         filter — RFC 8259 forbids it and the parser refuses the file. Asserted so \
         the filtering test above cannot be mistaken for covering this case, which \
         is how it was written the first time",
    );
}

#[test]
fn n7_a_value_longer_than_the_bound_is_cut_before_it_leaves_the_reader() {
    let (_dir, root) = clone();
    let huge = "d".repeat(5_000);
    file(
        &root,
        ".claude-plugin/marketplace.json",
        &format!(
            r#"{{ "plugins": [ {{ "name": "big", "description": {huge:?}, "source": "./" }} ] }}"#
        ),
    );

    let index = adapt::index_at(&root).expect("the index is read");
    let said = index.entries[0]
        .description
        .as_deref()
        .expect("a description");

    assert!(
        said.chars().count() < 300,
        "a description is one finished line of a picker row, and nothing stops a \
         repository putting a megabyte on it; got {} characters",
        said.chars().count(),
    );
}

// ---------------------------------------------------------------------------
// F7, F8, F9 — the manifest io writes
// ---------------------------------------------------------------------------

/// The agent frontmatter shape a real bundle publishes: a name, a folded
/// `description`, a model that is one vendor's family alias, and a tool list.
///
/// Taken from `zeroonething/caveman`'s `agents/cavecrew-builder.md`. Three of the
/// four keys have no slot in an io-harness agent definition, which is the whole
/// point of the fixture — one is translated and three are disclosed.
const REVIEWER: &str = "---\n\
                        name: reviewer\n\
                        description: >\n\
                        \x20 Reads a diff and says what is wrong with it.\n\
                        model: sonnet\n\
                        tools: Read, Grep\n\
                        ---\n\
                        \n\
                        You look for what is missing.\n";

/// A stdio server, which is what every local `.mcp.json` entry is: no `type` key,
/// a `command`, and arguments.
const STDIO_SERVER: &str =
    r#"{ "mcpServers": { "github": { "command": "github-mcp-server", "args": ["stdio"] } } }"#;

/// A bundle carrying one of each kind that maps, and nothing that does not.
fn four_kinds(root: &Path) -> PathBuf {
    let bundle = root.join("bundle");
    file(
        &bundle,
        ".claude-plugin/plugin.json",
        r#"{ "name": "rust-review", "description": "Everything our Rust reviews need." }"#,
    );
    file(
        &bundle,
        "skills/review/SKILL.md",
        "# review\n\nHow we review.\n",
    );
    file(&bundle, "commands/ship.md", "Ship $ARGUMENTS.\n");
    file(&bundle, "agents/reviewer.md", REVIEWER);
    file(&bundle, ".mcp.json", STDIO_SERVER);
    bundle
}

/// The adapter at `into`, loaded the way a configuration loads it.
///
/// **The user-scope file, and an adapter directory outside the workspace — which
/// is where a real adapter is and which file really names it.** io-harness 0.74.0
/// decides what a manifest may contribute by where the manifest sits: a
/// `plugin.toml` inside the discovery root may not carry an `[[mcp]]`, whatever
/// declared it, because the run's own agent writes paths inside the root. An
/// adapter is written under `home::adapters()` — `~/.io-cli/adapters` — which is
/// outside every workspace, and `manage`'s `plugin add` resolves an unstated scope
/// to the user's. So the fixture discovers against an empty workspace of its own
/// and names the adapter from `$IO_CONFIG`, which is the only arrangement in which
/// a generated manifest's `[[mcp]]` survives the load.
///
/// The path is spelled through `edit::spell` rather than by a format string: it is
/// absolute, and an absolute Windows path is full of backslashes that a `"{path}"`
/// would turn into escapes.
fn loaded(into: &Path) -> Plugins {
    let declared = io_cli::edit::spell(&into.display().to_string());
    support::user_scope(&format!("[[plugin]]\npath = {declared}\n"))
        .config
        .plugins()
}

/// **F7.** A bundle carrying skills, commands, an agent and an MCP server becomes
/// a manifest io-harness loads, and what it reports is what the bundle carried.
///
/// Sabotage: write the `skills` key as the relative `"skills"` the bundle's own
/// directory is called. Nothing else changes — the manifest still parses, the
/// bundle still loads, `dropped()` is still empty, and `contributions()` still
/// says `skills` — and every skill in the bundle is silently absent, because
/// `Plugin::skills_dir` is `self.root.join(d)` and the root is io's adapter
/// directory rather than the clone. Only the equality below fails.
#[test]
fn f7_a_four_kind_bundle_becomes_a_manifest_io_harness_loads_with_nothing_dropped() {
    let (_dir, root) = clone();
    let bundle = four_kinds(&root);
    let into = adapt::at(
        &root.join("adapters"),
        "zeroonething",
        "rust-review",
        "rust-review",
    );

    let written = adapt::generate(&bundle, "rust-review", &into).expect("the adapter is written");

    assert_eq!(
        written.manifest,
        root.join("adapters/zeroonething/rust-review/rust-review")
            .join(PLUGIN_FILE),
        "the destination is `<adapters>/<owner>/<repo>/<name>/plugin.toml`, and nothing \
         is written inside the clone — a file io wrote in a stranger's checkout is a \
         dirty tree at their next `git pull`",
    );
    assert!(
        !bundle.join(PLUGIN_FILE).exists(),
        "and the clone is untouched",
    );

    let plugins = loaded(&into);
    assert!(
        plugins.dropped().is_empty(),
        "io-harness refused the manifest io wrote: {:?}",
        plugins.dropped(),
    );
    let plugin = plugins
        .get("rust-review")
        .expect("loaded under the id io declared");

    // Every assertion below is io-harness's own accessor on the file it has just
    // read. A test that compared the generated text against a string would be
    // asserting that this crate can quote its own output.
    assert_eq!(
        plugin.skills_dir(),
        Some(std::fs::canonicalize(bundle.join("skills")).expect("the skills directory")),
        "the manifest is in io's home and the directory is in the clone, so an absolute \
         path is the only spelling that reaches it: `skills_dir` is `root.join(d)`, and \
         a join of an absolute path discards the root",
    );
    assert_eq!(
        plugin.templates_dir(),
        Some(std::fs::canonicalize(bundle.join("commands")).expect("the commands directory")),
        "`commands/` and a templates directory are one file in two vocabularies — \
         markdown, optional frontmatter, `$ARGUMENTS` — so the directory is named and \
         nothing is copied",
    );
    assert_eq!(
        plugin.agents().len(),
        1,
        "one agent, counted. A `contains` over the manifest text is satisfied by one \
         `[[agent]]` forever, and an agent that exists and is not declared is the \
         failure this asserts against",
    );
    assert_eq!(plugin.agents()[0].name, "rust-review__reviewer");
    assert_eq!(
        plugin.agents()[0].role.as_deref(),
        Some("You look for what is missing."),
        "the markdown after the closing fence IS the child's system prompt, which is \
         what a role is prepended to",
    );
    assert_eq!(plugin.mcp_servers().len(), 1, "one MCP server, counted");
    assert_eq!(plugin.mcp_servers()[0].id, "rust-review__github");

    assert_eq!(
        written.contributes,
        ["skills", "templates", "agents", "mcp"],
        "and what io reports is `Plugin::contributions()` on the file it wrote, in \
         io-harness's own order — never io-cli's account of what it spelled",
    );
    assert_eq!(
        written.disclosed.len(),
        3,
        "`description`, `model` and `tools` have no slot in an io-harness agent \
         definition, and each is reported so a surface can name it rather than being \
         dropped in silence: {:?}",
        written.disclosed,
    );
    assert!(
        written
            .disclosed
            .iter()
            .any(|said| said.contains("`model`")),
        "`sonnet` is one vendor's family alias and `AgentDef::model` is a model id a \
         provider published; translating one into the other invents a fact the run pays \
         for: {:?}",
        written.disclosed,
    );
}

/// **F7, the control.** A bundle carrying none of the four kinds contributes none
/// of them.
///
/// Without this, every count in F7 is satisfied by a generator that wrote a `name`
/// and stopped: `dropped()` would still be empty, `get(id)` would still find the
/// plugin, and the four assertions would be measuring a fixture rather than a
/// translation. This fixture differs from F7's in exactly the four directories.
#[test]
fn f7_a_bundle_carrying_none_of_the_four_kinds_contributes_nothing() {
    let (_dir, root) = clone();
    let bundle = root.join("bundle");
    file(
        &bundle,
        ".claude-plugin/plugin.json",
        r#"{ "name": "bare" }"#,
    );
    let into = root.join("adapters/bare");

    adapt::generate(&bundle, "bare", &into).expect("a bundle carrying nothing still adapts");

    let plugins = loaded(&into);
    let plugin = plugins
        .get("bare")
        .expect("loaded under the id io declared");
    assert!(
        plugin.contributions().is_empty(),
        "the four kinds are read off the disk, not written unconditionally: {:?}",
        plugin.contributions(),
    );
    assert_eq!(plugin.skills_dir(), None);
    assert_eq!(plugin.templates_dir(), None);
    assert!(plugin.agents().is_empty());
    assert!(plugin.mcp_servers().is_empty());
}

/// **F8.** The generated manifest goes back through io-harness's own parser, and a
/// key io-cli invents fails the build rather than one operator's configuration.
///
/// Sabotage: add a `commands` key to the generator beside `templates`. Under it
/// this test fails and nothing else does — and in the field the cost is not one
/// setting that does nothing, it is `Plugins::dropped` carrying the whole bundle
/// with an "unknown field" the operator cannot act on, in a file io-cli wrote and
/// they have never opened.
#[test]
fn f8_the_generated_manifest_round_trips_and_an_invented_key_refuses_the_bundle() {
    let (_dir, root) = clone();
    let bundle = four_kinds(&root);
    let into = root.join("adapters/rust-review");
    adapt::generate(&bundle, "rust-review", &into).expect("the adapter is written");
    let manifest = into.join(PLUGIN_FILE);
    let written = std::fs::read_to_string(&manifest).expect("the manifest io wrote");

    // `inspect` is the loader `Config::plugins` runs, reached without a
    // `[[plugin]]` entry — every check, on the file that is on disk, with none of
    // a configuration's other scopes in the way.
    let plugin = Plugins::inspect(Scope::User, &into).expect("the manifest io wrote parses");
    assert_eq!(plugin.id(), "rust-review");

    // **The half that makes the assertion above worth making.** io-harness parses a
    // manifest under `deny_unknown_fields`, and this proves it on the exact bytes
    // io writes. `commands` is the near miss worth choosing: it is the foreign
    // format's own word for what io-harness calls `templates`, so it is the key
    // this generator is most likely to grow by accident.
    //
    // **Prepended, not appended, and that is load-bearing.** The written manifest
    // ends in an `[[mcp]]` block, and a bare key after an array-of-tables header
    // belongs to that table rather than to the root — where `McpServer`'s
    // `#[serde(flatten)]` absorbs it in silence. A test that appended would pass
    // for a reason that has nothing to do with the rule it is about.
    std::fs::write(&manifest, format!("commands = \"commands\"\n{written}"))
        .expect("the manifest with one invented key");

    let refused = Plugins::inspect(Scope::User, &into)
        .expect_err("a key io-harness has no field for refuses the manifest and is not ignored");
    assert!(
        refused.to_string().contains("commands"),
        "and the refusal names the key, which is what makes it actionable: {refused}",
    );
}

/// A server that is a script inside the bundle — named the only way a bundle can
/// name one, because the bundle does not know where it will be cloned to.
const PLUGIN_ROOT_SERVER: &str = r#"{
  "mcpServers": {
    "local": {
      "command": "node",
      "args": ["${CLAUDE_PLUGIN_ROOT}/server/index.js"],
      "env": { "ROOT": "${CLAUDE_PLUGIN_ROOT}" }
    }
  }
}"#;

/// **F9.** `${CLAUDE_PLUGIN_ROOT}` comes out as an absolute path, and the
/// generated bytes carry no `${` at all.
///
/// Sabotage: pass the value through unchanged. The manifest still looks right to
/// anyone reading it, and io-harness refuses it — and the refusal takes the whole
/// bundle in every scope, so the operator loses every skill, template and agent
/// the bundle carries over one argument in a file they did not write.
#[test]
fn f9_the_one_expandable_substitution_is_written_out_and_no_dollar_brace_survives() {
    let (_dir, root) = clone();
    let bundle = root.join("bundle");
    file(&bundle, ".mcp.json", PLUGIN_ROOT_SERVER);
    let into = root.join("adapters/local");

    adapt::generate(&bundle, "local", &into).expect("the adapter is written");

    // Scanned in the bytes, because the bytes are what io-harness refuses. What
    // the generator meant to expand is not the question.
    let text = std::fs::read_to_string(into.join(PLUGIN_FILE)).expect("the manifest");
    assert!(
        !text.contains("${"),
        "io-harness refuses a substitution inside a plugin.toml in every scope and the \
         refusal takes the whole bundle, so the generated bytes carry none: {text}",
    );

    // And the path is the one io-harness will actually reach, asked of io-harness.
    let plugin = Plugins::inspect(Scope::User, &into).expect("the manifest parses");
    assert_eq!(plugin.mcp_servers().len(), 1, "one server, counted");
    let io_harness::McpTransport::Stdio { args, env, .. } = &plugin.mcp_servers()[0].transport
    else {
        panic!("a server naming a `command` is a stdio server");
    };
    assert_eq!(args.len(), 1, "one argument, counted");
    assert!(
        Path::new(&args[0]).is_absolute(),
        "the substitution means the bundle's root, and a manifest in io's own home \
         reaches the clone only by an absolute path: {}",
        args[0],
    );
    assert!(
        args[0].ends_with("server/index.js") || args[0].ends_with("server\\index.js"),
        "and the rest of the argument survives the expansion rather than being replaced \
         by it: {}",
        args[0],
    );
    let expected = std::fs::canonicalize(&bundle)
        .expect("the bundle")
        .to_string_lossy()
        .into_owned();
    assert_eq!(
        env.get("ROOT"),
        Some(&expected),
        "every field is expanded and not only the one this fixture was written for",
    );
}

/// **F9, the other half.** A substitution io cannot honestly expand is a refusal
/// naming the server, and nothing is left on disk.
#[test]
fn f9_a_substitution_io_cannot_expand_refuses_and_names_the_server() {
    let (_dir, root) = clone();
    let bundle = root.join("bundle");
    file(
        &bundle,
        ".mcp.json",
        r#"{ "mcpServers": { "github": {
             "command": "github-mcp-server",
             "env": { "GITHUB_TOKEN": "${GITHUB_TOKEN}" } } } }"#,
    );
    let into = root.join("adapters/github");

    let refused = adapt::generate(&bundle, "github", &into)
        .expect_err("a substitution io cannot honestly expand is never passed through");

    assert!(
        refused.contains("server `github`"),
        "the refusal names the server rather than the bundle: a person fixing this has \
         to know which entry of the file it is about, and `github-mcp-server` would \
         satisfy a looser check forever: {refused}",
    );
    assert!(
        refused.contains("env.GITHUB_TOKEN"),
        "and the field it is in: {refused}",
    );
    assert!(
        !into.join(PLUGIN_FILE).exists(),
        "and nothing is left behind. A manifest io-harness would refuse must never reach \
         a directory an operator could go on to declare — a broken adapter on disk reads \
         exactly like a working one until something loads it",
    );
}
