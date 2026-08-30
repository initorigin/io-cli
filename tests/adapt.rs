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

use std::path::{Path, PathBuf};

use io_cli::adapt::{self, Source};

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
