//! F1, N1 and N3 — the writer that replaces one value and copies the rest through.
//!
//! The property under test is a **byte** property, and that is the whole point of
//! the file. Two TOML documents that parse equal can differ in every comment, in
//! every blank line and in the order of every table, so an assertion that
//! re-parses the result and compares values is satisfied by the very rewrite this
//! module exists to replace — `settings::render`'s whole-file serialisation, which
//! is what io-cli did before 0.16.0 and which would erase an operator's file.
//!
//! So every assertion here compares the bytes before and after with the one
//! replaced span excised. A test that cannot tell those two implementations apart
//! is not a test of this module.

use io_cli::edit::{self, Edit};

/// A file with every shape an operator's real `io.toml` has and io-cli does not
/// model: a header comment, an inline comment on the very key being changed, an
/// array of tables, a section io-cli has no type for, and a blank-line rhythm.
const OPERATORS_FILE: &str = "\
# My io configuration.
# Two lines of it, because people write two lines.

[run]
max_steps = 30   # deliberately low while I am debugging
max_tokens = 100000

[[agent]]
name = \"scout\"
model = \"anthropic/claude-sonnet-4\"

[instructions]
files = [\"AGENTS.md\"]

[app.io-cli]
theme = \"dark\"
";

/// Everything outside `span` must be byte-identical.
///
/// Returns the text that replaced the span, so a caller can assert on the new
/// value separately from the preservation property.
fn assert_only_span_changed(before: &str, after: &str, key: &str) -> String {
    // Find the longest common prefix and suffix. Whatever sits between them is
    // the whole of what the writer touched; if the writer rewrote the document
    // the prefix and suffix collapse and the remainder is most of the file.
    let pre = before
        .bytes()
        .zip(after.bytes())
        .take_while(|(a, b)| a == b)
        .count();
    let suf = before[pre..]
        .bytes()
        .rev()
        .zip(after[pre..].bytes().rev())
        .take_while(|(a, b)| a == b)
        .count();

    let old = &before[pre..before.len() - suf];
    let new = &after[pre..after.len() - suf];

    assert!(
        !old.contains('\n') && !new.contains('\n'),
        "writing {key} changed more than one line's worth of bytes.\n  \
         removed: {old:?}\n  inserted: {new:?}\n\
         A whole-file re-serialisation looks exactly like this."
    );
    new.to_string()
}

#[test]
fn f1_a_scalar_is_replaced_and_every_other_byte_survives() {
    let after = edit::apply(OPERATORS_FILE, &[Edit::set("run.max_steps", "45")]).unwrap();

    let new = assert_only_span_changed(OPERATORS_FILE, &after, "run.max_steps");
    assert_eq!(new, "45");

    // The things a re-serialisation loses, named one at a time so a failure says
    // which kind of loss happened rather than only that the bytes differ.
    assert!(
        after.contains("# My io configuration."),
        "header comment lost"
    );
    assert!(
        after.contains("# deliberately low while I am debugging"),
        "the inline comment on the edited line itself was lost"
    );
    assert!(after.contains("[[agent]]"), "array of tables lost");
    assert!(after.contains("[instructions]"), "unmodelled section lost");
    assert!(after.contains("max_tokens = 100000"), "sibling key lost");
}

#[test]
fn f1_a_string_keeps_its_quotes_and_its_neighbours() {
    let after = edit::apply(
        OPERATORS_FILE,
        &[Edit::set("app.io-cli.theme", "\"light\"")],
    )
    .unwrap();

    assert_only_span_changed(OPERATORS_FILE, &after, "app.io-cli.theme");
    assert!(
        after.contains("theme = \"light\""),
        "the quotes did not survive the splice"
    );
    assert!(after.contains("name = \"scout\""), "a sibling string moved");
}

#[test]
fn f1_a_key_the_file_does_not_have_is_inserted_into_its_section() {
    let after = edit::apply(OPERATORS_FILE, &[Edit::set("run.max_retries", "3")]).unwrap();

    assert!(after.contains("max_retries = 3"));
    // Inserted into `[run]`, which means before `[[agent]]` rather than at the
    // end of the file — a key appended after the last section would silently
    // join whatever section happens to be last.
    let inserted = after.find("max_retries").unwrap();
    let agent = after.find("[[agent]]").unwrap();
    assert!(
        inserted < agent,
        "the key was appended to the file rather than inserted into [run]"
    );
    // Everything that was there before is still there.
    for line in OPERATORS_FILE.lines().filter(|l| !l.trim().is_empty()) {
        assert!(after.contains(line), "line lost on insert: {line:?}");
    }
}

#[test]
fn f1_a_section_the_file_does_not_have_is_appended_whole() {
    let after = edit::apply(OPERATORS_FILE, &[Edit::set("memory.max_rows", "500")]).unwrap();

    assert!(after.contains("[memory]"));
    assert!(after.contains("max_rows = 500"));
    for line in OPERATORS_FILE.lines().filter(|l| !l.trim().is_empty()) {
        assert!(after.contains(line), "line lost on new section: {line:?}");
    }
}

#[test]
fn f1_an_entry_in_an_array_of_tables_is_addressed_by_index() {
    const TWO: &str = "\
[[mcp]]
id = \"docs\"
command = \"mcp-docs\"

[[mcp]]
id = \"search\"
command = \"mcp-search\"
";
    let after = edit::apply(TWO, &[Edit::set("mcp[1].command", "\"mcp-find\"")]).unwrap();

    assert_only_span_changed(TWO, &after, "mcp[1].command");
    assert!(after.contains("command = \"mcp-find\""));
    assert!(
        after.contains("command = \"mcp-docs\""),
        "the first entry was edited instead of the second"
    );
}

#[test]
fn f1_a_multi_line_string_is_not_mistaken_for_a_table_header() {
    // A `[` at the start of a line inside a multi-line string is not a header,
    // and a scanner that splits on `^[` would cut the document in half here.
    const TRICKY: &str = "\
[instructions]
text = \"\"\"
[run]
this is prose, not a section
\"\"\"

[run]
max_steps = 10
";
    let after = edit::apply(TRICKY, &[Edit::set("run.max_steps", "12")]).unwrap();

    assert_only_span_changed(TRICKY, &after, "run.max_steps");
    assert!(after.contains("max_steps = 12"));
    assert!(
        after.contains("this is prose, not a section"),
        "the multi-line string was damaged"
    );
}

#[test]
fn f1_a_dotted_key_is_refused_rather_than_mis_spliced() {
    // `toml` reports the KEY's span for a dotted key, not the value's — proved
    // against toml 1.1.4 before this module was written. Splicing there would
    // replace the key with a value and produce a file that no longer parses, so
    // the writer refuses and says why instead.
    const DOTTED: &str = "run.max_steps = 30\n";
    let err = edit::apply(DOTTED, &[Edit::set("run.max_steps", "45")]).unwrap_err();
    assert!(
        err.contains("dotted"),
        "the refusal should name the shape it refused, got: {err}"
    );
}

#[test]
fn f1_the_result_is_always_still_the_harness_schema() {
    // Every edit this module makes is read back before it is allowed to land.
    // A value that is not valid TOML is refused rather than written.
    let err = edit::apply(
        OPERATORS_FILE,
        &[Edit::set("run.max_steps", "not a number")],
    )
    .unwrap_err();
    assert!(!err.is_empty(), "an unparseable result must be refused");
    // And the refusal is not a panic, which is the only other way this could go.
}

#[test]
fn f1_two_edits_in_one_pass_do_not_disturb_each_other_s_offsets() {
    // Splicing left to right invalidates every later span. Applying both and
    // getting both right is the property; doing it by two sequential calls
    // would hide an offset bug that only appears in one pass.
    let after = edit::apply(
        OPERATORS_FILE,
        &[
            Edit::set("run.max_steps", "45"),
            Edit::set("run.max_tokens", "250000"),
        ],
    )
    .unwrap();

    assert!(after.contains("max_steps = 45"));
    assert!(after.contains("max_tokens = 250000"));
    assert!(
        after.contains("# deliberately low while I am debugging"),
        "the inline comment beside the first edit was lost"
    );
    assert!(after.contains("[[agent]]"));
}

#[test]
fn n3_a_failed_write_leaves_the_previous_file_intact() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("io.toml");
    std::fs::write(&path, OPERATORS_FILE).unwrap();

    // An edit that cannot be applied must not touch the file at all.
    let err = edit::write(&path, &[Edit::set("run.max_steps", "not a number")]).unwrap_err();
    assert!(!err.is_empty());
    assert_eq!(
        std::fs::read_to_string(&path).unwrap(),
        OPERATORS_FILE,
        "a refused edit rewrote the operator's file"
    );
}

#[test]
fn n3_a_written_file_keeps_its_mode_and_lands_whole() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("io.toml");
    io_cli::settings::write(&path, OPERATORS_FILE).unwrap();

    edit::write(&path, &[Edit::set("run.max_steps", "45")]).unwrap();

    let after = std::fs::read_to_string(&path).unwrap();
    assert_eq!(
        assert_only_span_changed(OPERATORS_FILE, &after, "run.max_steps"),
        "45"
    );

    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let mode = std::fs::metadata(&path).unwrap().permissions().mode() & 0o777;
        assert_eq!(
            mode, 0o600,
            "the credential file's mode was widened by a write"
        );
    }

    // No temporary file is left behind next to it.
    let strays: Vec<_> = std::fs::read_dir(dir.path())
        .unwrap()
        .filter_map(|e| e.ok())
        .map(|e| e.file_name().to_string_lossy().into_owned())
        .filter(|n| n != "io.toml")
        .collect();
    assert!(strays.is_empty(), "temporary files left behind: {strays:?}");
}

#[test]
fn f1_an_array_of_tables_grows_by_a_whole_entry_and_shrinks_the_same_way() {
    // The shape `set` cannot express: `set` reaches a key inside an entry that
    // already exists, and an array of tables grows by gaining a block.
    let after = edit::apply(
        OPERATORS_FILE,
        &[Edit::append("mcp", "id = \"docs\"\ncommand = \"mcp-docs\"")],
    )
    .unwrap();

    assert!(after.contains("[[mcp]]"));
    assert!(after.contains("id = \"docs\""));
    for line in OPERATORS_FILE.lines().filter(|l| !l.trim().is_empty()) {
        assert!(after.contains(line), "line lost on append: {line:?}");
    }

    // And it comes back out, bytes and all.
    let removed = edit::apply(&after, &[Edit::remove("mcp")]).unwrap();
    assert!(!removed.contains("[[mcp]]"));
    assert!(!removed.contains("mcp-docs"));
    for line in OPERATORS_FILE.lines().filter(|l| !l.trim().is_empty()) {
        assert!(removed.contains(line), "line lost on remove: {line:?}");
    }
}

#[test]
fn f1_a_new_entry_goes_last_because_the_order_is_the_chain() {
    const ONE: &str = "[[provider]]\nkind = \"openrouter\"\nmodel = \"a\"\n";
    let after = edit::apply(
        ONE,
        &[Edit::append(
            "provider",
            "kind = \"anthropic\"\nmodel = \"b\"",
        )],
    )
    .unwrap();

    let first = after.find("openrouter").unwrap();
    let second = after.find("anthropic").unwrap();
    assert!(
        first < second,
        "a new provider was inserted ahead of an existing one, which silently \
         rearranges which provider a run uses"
    );
}

#[test]
fn f1_removing_the_second_entry_leaves_the_first() {
    const TWO: &str = "\
[[mcp]]
id = \"docs\"

[[mcp]]
id = \"search\"
";
    let after = edit::apply(TWO, &[Edit::remove("mcp[1]")]).unwrap();
    assert!(after.contains("docs"));
    assert!(!after.contains("search"), "the wrong entry was removed");
}

#[test]
fn f1_an_entry_moves_with_its_comments_and_its_unmodelled_keys() {
    // Order is meaning: for `[[provider]]` it is the fallback chain, so a move
    // has to carry the entry's own bytes rather than rewrite the array.
    const CHAIN: &str = "\
[[provider]]
kind = \"openrouter\"
model = \"a\"

# the cheap one, second on purpose
[[provider]]
kind = \"compatible\"
preset = \"groq\"
model = \"b\"

[run]
max_steps = 30
";
    let up = edit::apply(CHAIN, &[Edit::move_entry("provider", 1, 0)]).unwrap();

    // The second entry is now first.
    assert!(
        up.find("groq").unwrap() < up.find("openrouter").unwrap(),
        "the move did not reorder the chain:\n{up}"
    );
    // With its comment, its unmodelled key, and nothing else disturbed.
    assert!(
        up.contains("# the cheap one, second on purpose"),
        "comment lost"
    );
    assert!(up.contains("preset = \"groq\""), "key lost");
    assert!(up.contains("max_steps = 30"), "a later section moved");
    let config = io_harness::Config::from_toml(&up).expect("the moved file loads");
    assert_eq!(config.fallback_specs().len(), 1);

    // And moving it back is the identity, which is the property a one-way
    // implementation would fail.
    let back = edit::apply(&up, &[Edit::move_entry("provider", 0, 1)]).unwrap();
    assert_eq!(
        back.trim_end(),
        CHAIN.trim_end(),
        "a move is not reversible"
    );
}

/// **A dot inside a quoted key is not a path separator, and the two sides of this
/// module used to disagree about that.**
///
/// `split_path` split the caller's path on every `.` and normalised nothing;
/// `regions` split the file's header on every `.` and then stripped quotes with
/// `trim_matches`. Two different wrong answers, so `prices.models."gpt-4.1"` never
/// matched itself: the read half answered `None` and the write half fell through
/// to the append arm and emitted a SECOND `[prices.models."gpt-4.1"]` table, which
/// only the read-back caught, and only as "the edit would have produced a file
/// that does not parse" — for a perfectly legal TOML key.
///
/// Sabotage: put `path.split('.')` back in `split_path`, or
/// `.trim_matches('"').trim_matches('\'')` back in `regions`, or drop either
/// `spell` call from the two spelling sites, and this test fails.
#[test]
fn f1_a_dot_inside_a_quoted_key_is_one_segment_on_both_sides() {
    // Two quoted keys, and the second is the one `trim_matches` cannot ever get
    // right: a basic string takes the full escape set, so `"a\"b"` is a legal key
    // whose NAME is `a"b`. Trimming quotes off both ends of that yields `a\"b`,
    // which is a key no file holds.
    const QUOTED: &str = "\
[prices.models.\"gpt-4.1\"]
input = 3.0
output = 12.0

[prices.models.\"a\\\"b\"]
input = 1.0
";

    // 1. The header is cut into segments, decoded, and the escape is resolved.
    let sections = edit::sections(QUOTED);
    assert!(
        sections.contains(&vec![
            "prices".to_string(),
            "models".to_string(),
            "gpt-4.1".to_string()
        ]),
        "the quoted header was split on the dot inside it: {sections:?}"
    );
    assert!(
        sections.contains(&vec![
            "prices".to_string(),
            "models".to_string(),
            "a\"b".to_string()
        ]),
        "the escaped quote was not decoded: {sections:?}"
    );

    // 2. A caller's path is cut the same way, so the two sides meet.
    assert_eq!(
        edit::value_at(QUOTED, "prices.models.\"gpt-4.1\".input").as_deref(),
        Some("3.0"),
        "a dotted key showed a file with no value in it"
    );
    // A literal string spells the same key and takes no escapes at all.
    assert_eq!(
        edit::value_at(QUOTED, "prices.models.'gpt-4.1'.output").as_deref(),
        Some("12.0"),
        "a literal-string segment did not reach the key a basic string reaches"
    );
    assert_eq!(
        edit::value_at(QUOTED, "prices.models.\"a\\\"b\".input").as_deref(),
        Some("1.0"),
        "the escape was not resolved on the caller's side"
    );
    // Whitespace around a dot is legal TOML and an unquoted segment is trimmed.
    assert_eq!(
        edit::value_at(QUOTED, "prices . models . \"gpt-4.1\" . input").as_deref(),
        Some("3.0"),
        "whitespace around a dot became part of a segment name"
    );

    // 3. The write half reaches the same section, and reaches it exactly once.
    let after = edit::apply(
        QUOTED,
        &[Edit::set("prices.models.\"gpt-4.1\".input", "4.0")],
    )
    .unwrap();
    assert_eq!(
        assert_only_span_changed(QUOTED, &after, "prices.models.\"gpt-4.1\".input"),
        "4.0"
    );
    assert_eq!(
        after.matches("[prices.models.\"gpt-4.1\"]").count(),
        1,
        "the write fell through to the append arm and duplicated the table:\n{after}"
    );

    // 4. Because a segment is DECODED now, the two sites that spell one back into
    // a document have to re-quote anything that is not a bare key. A bare key is
    // `A-Za-z0-9_-` and nothing else, so both of these need quotes.
    let key = edit::apply(
        QUOTED,
        &[Edit::set(
            "prices.models.\"gpt-4.1\".\"cached input\"",
            "1.5",
        )],
    )
    .unwrap();
    assert!(
        key.contains("\"cached input\" = 1.5"),
        "the new key was spelled bare and the space in it would not parse:\n{key}"
    );

    let header = edit::apply(
        QUOTED,
        &[Edit::set("prices.models.\"gpt-4.5\".input", "5.0")],
    )
    .unwrap();
    assert!(
        header.contains("[prices.models.\"gpt-4.5\"]"),
        "the new header was spelled bare and the dot in it would name a third table:\n{header}"
    );
    // And the proof that both spellings are right is that the document still says
    // what it was asked to say when it is read back as TOML.
    let parsed: toml::Value = toml::from_str(&header).expect("the appended header parses");
    assert_eq!(
        parsed["prices"]["models"]["gpt-4.5"]["input"].as_float(),
        Some(5.0),
        "the appended section landed under a different key than the one asked for"
    );
}

/// **A section's trailing comment run belongs to the section BELOW it.**
///
/// A region's `body.end` is the first byte of the NEXT header, so splicing
/// `region.start..region.body.end` away deletes everything between this entry's
/// last key and that header — including the comment block an operator wrote to
/// document the next section. `Kind::Move` carried the same bytes away with it.
///
/// Sabotage: use `region.body.end` at either splice instead of `removal_end`, and
/// the comment naming the entry that survives is deleted along with the one that
/// was asked for.
#[test]
fn f1_removing_or_moving_an_entry_leaves_the_next_one_s_comment_behind() {
    const DOCUMENTED: &str = "\
[[mcp]]
id = \"docs\"

# search is the one the team actually uses; do not remove it
[[mcp]]
id = \"search\"
";

    let removed = edit::apply(DOCUMENTED, &[Edit::remove("mcp[0]")]).unwrap();
    assert!(
        removed.contains("# search is the one the team actually uses"),
        "removing the first entry deleted the second entry's comment:\n{removed}"
    );
    assert!(!removed.contains("docs"), "the wrong entry survived");
    // The blank line that separated the removed entry from that comment goes with
    // it, so repeated removals do not leave a growing stack of empty lines.
    assert!(
        removed.starts_with("# search"),
        "removal accumulated whitespace above the comment:\n{removed:?}"
    );

    // The same bytes, carried away by a move rather than a removal.
    let moved = edit::apply(DOCUMENTED, &[Edit::move_entry("mcp", 0, 1)]).unwrap();
    assert!(
        moved.contains("# search is the one the team actually uses"),
        "the moved entry took the next entry's comment with it:\n{moved}"
    );
    let comment = moved.find("# search").unwrap();
    let docs = moved.find("docs").unwrap();
    assert!(
        comment < docs,
        "the comment moved down with the entry it does not describe:\n{moved}"
    );
}

/// **A move has no newline guard, and `Kind::Append` right beside it does.**
///
/// The last region's `body.end` is the length of the file, so moving an entry
/// into last place splices its header straight onto whatever the final line was.
/// Usually that is a parse refusal; when the final line is a COMMENT with no
/// newline after it the comment swallows the header, the moved entry's keys land
/// inside the previous table, and the file still parses — as something else.
///
/// Sabotage: delete either half of the guard in `Kind::Move`. The down arm loses
/// the `\n` in front of the block and the up arm loses the one behind it.
#[test]
fn f1_a_move_against_a_file_with_no_trailing_newline_still_parses() {
    // No newline after the last line, and the last line is a comment.
    const COMMENTED: &str = "\
[[provider]]
kind = \"openrouter\"
model = \"a\"

[[provider]]
kind = \"compatible\"
preset = \"groq\"
model = \"b\"
# groq answers last, and there is no newline after this line";

    let down = edit::apply(COMMENTED, &[Edit::move_entry("provider", 0, 1)]).unwrap();
    let config = io_harness::Config::from_toml(&down).expect("the moved file loads");
    assert_eq!(
        config.fallback_specs().len(),
        1,
        "the moved entry's keys were absorbed by the entry above it:\n{down}"
    );
    assert!(
        down.find("groq").unwrap() < down.find("openrouter").unwrap(),
        "the move did not reorder the chain:\n{down}"
    );
    assert!(
        down.contains("# groq answers last"),
        "the trailing comment was overwritten:\n{down}"
    );

    // The mirror: the block itself ends without a newline, and the destination's
    // header is spliced onto the end of it.
    const TAIL: &str = "\
[[provider]]
kind = \"openrouter\"
model = \"a\"

[[provider]]
kind = \"compatible\"
preset = \"groq\"
model = \"b\"";

    let up = edit::apply(TAIL, &[Edit::move_entry("provider", 1, 0)]).unwrap();
    let config = io_harness::Config::from_toml(&up).expect("the moved file loads");
    assert_eq!(config.fallback_specs().len(), 1, "{up}");
    assert!(
        up.find("groq").unwrap() < up.find("openrouter").unwrap(),
        "the move did not reorder the chain:\n{up}"
    );
}
