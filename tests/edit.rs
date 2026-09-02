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

mod support;

/// Load a written file the way io-harness would load the operator's own.
///
/// **Never `Config::from_toml` for a file with `[[provider]]` in it.** `from_toml`
/// parses at `Scope::Project`, and io-harness 0.74.0 refuses a provider there — a
/// provider names the endpoint this run's credential is sent to, and `io.toml`
/// arrives with a `git clone`. The moves below are moves of provider entries, so
/// the round trip that proves the move survived has to read at the scope that may
/// declare one. Files with no refused section still go through `from_toml`.
fn loaded(text: &str) -> io_harness::Config {
    support::user_scope(text).config.clone()
}

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
    let config = loaded(&up);
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
    // `4.5` rather than `4.0`, and the digit after the point is the point.
    // `assert_only_span_changed` reports the span between the longest common
    // prefix and the longest common suffix, so replacing `3.0` with `4.0` reports
    // `3` -> `4`: the shared `.0` is swallowed by the suffix scan. That would be a
    // test asserting against its own helper's arithmetic rather than against the
    // writer, and it would go on passing if the writer started truncating floats.
    let after = edit::apply(
        QUOTED,
        &[Edit::set("prices.models.\"gpt-4.1\".input", "4.5")],
    )
    .unwrap();
    assert_eq!(
        assert_only_span_changed(QUOTED, &after, "prices.models.\"gpt-4.1\".input"),
        "4.5"
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

/// The fixture with one line taken out of it, rebuilt line by line.
///
/// Byte-exact, and that is the only assertion worth making about an unset: a
/// `contains` check passes for a writer that reordered the file, dropped a
/// comment or rewrote the whitespace, which is every failure this module exists
/// to prevent. Every line of the fixture ends in a newline and there is no `\r`
/// in one, so splitting and rejoining reproduces the file exactly.
fn without_line(text: &str, line: &str) -> String {
    let mut kept = String::new();
    let mut found = false;
    for each in text.lines() {
        if each == line {
            found = true;
            continue;
        }
        kept.push_str(each);
        kept.push('\n');
    }
    assert!(found, "the fixture has no line {line:?} to take out");
    kept
}

/// **Unset deletes one key's line and leaves every other byte where it was.**
///
/// The destructive failure mode is not deleting the wrong key, it is deleting
/// the right key and taking the section with it — a cut that ran from the key to
/// the next header, or from the section's header to the key, would satisfy any
/// assertion that only asks whether the key is gone. So the whole document is
/// compared, byte for byte, against itself with exactly one line removed.
///
/// Sabotage: cut from `region.start` rather than from the key's own line, or run
/// forward to `region.body.end` rather than to the newline after the value, and
/// the sibling key, the header or the blank-line rhythm goes with it.
#[test]
fn f1_an_unset_takes_one_line_and_leaves_the_section_standing() {
    let after = edit::apply(OPERATORS_FILE, &[Edit::unset("run.max_tokens")]).unwrap();

    assert_eq!(
        after,
        without_line(OPERATORS_FILE, "max_tokens = 100000"),
        "unsetting one key changed bytes that were not on its line:\n{after}"
    );

    // Named one at a time, so a failure says which kind of loss happened.
    assert!(
        after.contains("[run]"),
        "the enclosing section header went too"
    );
    assert!(
        after.contains("max_steps = 30   # deliberately low while I am debugging"),
        "the sibling key lost its inline comment, or its spacing"
    );
    assert!(
        after.contains("# My io configuration."),
        "the header comment was lost"
    );
    assert!(after.contains("[[agent]]"), "a later section moved");

    // And the absence is a real absence: the key falls back to its default
    // rather than being shadowed by an empty value left in its place.
    assert_eq!(edit::value_at(&after, "run.max_tokens"), None);
    assert_eq!(
        edit::value_at(&after, "run.max_steps").as_deref(),
        Some("30"),
        "the sibling's value moved"
    );
}

#[test]
fn f1_an_unset_reaches_a_key_in_a_nested_section_by_its_full_path() {
    // `[app.io-cli]` is two segments and the key is a third, so this fails for a
    // resolver that treats the last dot as the only separator.
    let after = edit::apply(OPERATORS_FILE, &[Edit::unset("app.io-cli.theme")]).unwrap();

    assert_eq!(after, without_line(OPERATORS_FILE, "theme = \"dark\""));
    assert!(
        after.contains("[app.io-cli]"),
        "the section header went with its last key:\n{after}"
    );
    // A section with nothing in it is still a section, and still parses.
    let parsed: toml::Value = toml::from_str(&after).expect("the result parses");
    assert!(parsed["app"]["io-cli"].get("theme").is_none());
}

#[test]
fn f1_an_unset_reaches_a_top_level_key_by_its_bare_name() {
    const TOP: &str = "\
# what this file is for
name = \"my project\"
version = \"1\"

[run]
max_steps = 30
";
    let after = edit::apply(TOP, &[Edit::unset("name")]).unwrap();

    assert_eq!(after, without_line(TOP, "name = \"my project\""));
    assert!(
        after.contains("# what this file is for"),
        "the comment above the file's first key was taken with it:\n{after}"
    );
    assert!(
        after.contains("version = \"1\""),
        "a top-level sibling was lost"
    );
}

/// **A key that is not there is refused by name, not passed over.**
///
/// A caller that asks to remove a setting and is told nothing believes the
/// setting was removed. So an unset that found nothing says so, in the same
/// shape [`Edit::remove`]'s refusal has, and names the path it was given.
#[test]
fn f1_an_unset_of_a_key_the_file_does_not_have_is_refused_by_name() {
    // The section is there and the key is not.
    let err = edit::apply(OPERATORS_FILE, &[Edit::unset("run.max_retries")]).unwrap_err();
    assert!(
        err.contains("run.max_retries") && err.contains("unset"),
        "the refusal does not name the path it refused: {err}"
    );

    // Neither the section nor the key is there, which is a different arm.
    let missing = edit::apply(OPERATORS_FILE, &[Edit::unset("memory.max_rows")]).unwrap_err();
    assert!(
        missing.contains("memory.max_rows"),
        "the refusal does not name the path it refused: {missing}"
    );

    // Nothing partial: a batch with one bad edit in it writes nothing at all.
    let batch = edit::apply(
        OPERATORS_FILE,
        &[
            Edit::unset("run.max_retries"),
            Edit::set("run.max_steps", "45"),
        ],
    )
    .unwrap_err();
    assert!(!batch.is_empty());
}

/// **`remove` names a section and `unset` names a key, and neither answers for
/// the other.**
///
/// `remove` finds a REGION by matching a header, so `run.max_steps` looks for a
/// `[run.max_steps]` header and finds none — and that refusal has to survive,
/// because a `remove` that fell back to deleting a key line when it could not
/// find a section would make the two constructors interchangeable at the call
/// site and the difference between them is one setting against one whole block.
/// The mirror holds too: `unset` given a section path finds no key of that name.
///
/// Sabotage: give either arm a fallback into the other, and one of these two
/// halves stops erroring.
#[test]
fn f1_remove_will_not_delete_a_key_and_unset_will_not_delete_a_section() {
    let err = edit::apply(OPERATORS_FILE, &[Edit::remove("run.max_steps")]).unwrap_err();
    assert!(
        err.contains("run.max_steps") && err.contains("remove"),
        "`remove` deleted a key line, or refused without naming what it refused: {err}"
    );

    let mirror = edit::apply(OPERATORS_FILE, &[Edit::unset("run")]).unwrap_err();
    assert!(
        mirror.contains("run") && mirror.contains("unset"),
        "`unset` deleted a whole section, or refused without naming it: {mirror}"
    );

    // And `remove` still does its own job, so the assertion above is about the
    // key path rather than about `remove` being broken.
    let removed = edit::apply(OPERATORS_FILE, &[Edit::remove("run")]).unwrap();
    assert!(!removed.contains("[run]"));
    assert!(!removed.contains("max_steps"));
}

#[test]
fn f1_an_unset_leaves_the_comments_and_blank_lines_around_it() {
    // The comment above a key is an operator's sentence about a decision, and
    // the blank line below it is the rhythm they typed. Neither is on the key's
    // line, so neither goes.
    const DOCUMENTED: &str = "\
[run]
# raised while the model was being slow
max_steps = 30

# tokens: this ceiling is what the budget allows
max_tokens = 100000

[instructions]
files = [\"AGENTS.md\"]
";
    let after = edit::apply(DOCUMENTED, &[Edit::unset("run.max_steps")]).unwrap();

    assert_eq!(
        after,
        without_line(DOCUMENTED, "max_steps = 30"),
        "an unset moved bytes that were not on the key's line:\n{after:?}"
    );
    assert!(
        after.contains("# raised while the model was being slow"),
        "the comment above the removed key went with it"
    );
    assert!(
        after.contains("# tokens: this ceiling is what the budget allows"),
        "the comment below the removed key went with it"
    );
    assert!(!after.contains("max_steps"), "the key survived the unset");

    // The inline comment on the removed line itself DOES go, because it is a
    // note about the key that is leaving.
    let inline = edit::apply(OPERATORS_FILE, &[Edit::unset("run.max_steps")]).unwrap();
    assert!(
        !inline.contains("# deliberately low while I am debugging"),
        "the removed key's own inline comment was left behind as an orphan:\n{inline}"
    );
}

/// **A value spelled across several lines takes every one of them.**
///
/// The span is the value's, so a cut that ran from the key's line to the first
/// newline would delete `files = [` and leave the array's rows and its closing
/// bracket stranded in the file. That does not parse, which is the good case;
/// the bad case is the `"""` block, where the leftover `[run]` line inside the
/// prose becomes a section header and the rest of the file lands inside it.
///
/// Sabotage: run the cut forward from the value's START rather than its end, and
/// both halves of this test fail — the first as a refusal, the second as a file
/// that parses into a different configuration.
#[test]
fn f1_an_unset_of_a_multi_line_value_takes_the_whole_value() {
    const SPREAD: &str = "\
[instructions]
files = [
  \"AGENTS.md\",
  \"CONTRIBUTING.md\",
]
text = \"\"\"
[run]
this is prose, not a section
\"\"\"
mode = \"append\"
";

    let array = edit::apply(SPREAD, &[Edit::unset("instructions.files")]).unwrap();
    assert!(
        !array.contains("AGENTS.md"),
        "the array's first row survived"
    );
    assert!(
        !array.contains("CONTRIBUTING.md"),
        "only the first line of the array was deleted:\n{array}"
    );
    assert!(
        !array.contains("\n]"),
        "the array's closing bracket was stranded:\n{array}"
    );
    assert!(
        array.contains("mode = \"append\""),
        "a sibling key was lost"
    );
    assert!(
        array.contains("this is prose, not a section"),
        "the multi-line string below the array was damaged:\n{array}"
    );

    let block = edit::apply(SPREAD, &[Edit::unset("instructions.text")]).unwrap();
    assert!(
        !block.contains("this is prose, not a section"),
        "the body of the multi-line string was left behind:\n{block}"
    );
    assert!(block.contains("AGENTS.md"), "the array above it was lost");
    assert!(
        block.contains("mode = \"append\""),
        "a sibling key was lost"
    );
    // The tell that nothing was stranded: the `[run]` line inside the prose did
    // not become a section header.
    let parsed: toml::Value = toml::from_str(&block).expect("the result parses");
    assert!(
        parsed.get("run").is_none(),
        "a fragment of the deleted string became a section:\n{block}"
    );
}

/// **An unset in a batch resolves against the same document every other edit
/// does, and takes more bytes than any of them.**
///
/// [`edit::apply`] walks the headers once and resolves every edit against the
/// document as it was *before* the batch, then splices right to left so an
/// earlier cut cannot invalidate a later offset. An unset is the widest cut in
/// the module — a whole line rather than a value's span — so it is the one that
/// can swallow another edit's landing place: unsetting a section's LAST key puts
/// the range's end exactly at the point where a `set` of a new key into that
/// section wants to insert.
///
/// Sabotage: sort the splices ascending, or apply them as they were pushed, and
/// the new key lands inside the deleted range or the deletion runs off the end
/// of a string that has already grown.
#[test]
fn f1_an_unset_and_another_edit_share_one_pass_without_disturbing_each_other() {
    let after = edit::apply(
        OPERATORS_FILE,
        &[
            Edit::unset("run.max_tokens"),
            Edit::set("app.io-cli.theme", "\"light\""),
        ],
    )
    .unwrap();

    assert!(!after.contains("max_tokens"), "the unset did not happen");
    assert!(
        after.contains("theme = \"light\""),
        "the set did not happen"
    );
    assert!(
        after.contains("max_steps = 30   # deliberately low while I am debugging"),
        "the batch disturbed a line neither edit named:\n{after}"
    );

    // Order in the slice must not decide the result.
    let swapped = edit::apply(
        OPERATORS_FILE,
        &[
            Edit::set("app.io-cli.theme", "\"light\""),
            Edit::unset("run.max_tokens"),
        ],
    )
    .unwrap();
    assert_eq!(after, swapped, "the batch's result depends on edit order");

    // The touching case: the unset takes the last line of `[run]`, and the new
    // key is inserted at the byte where that line ended.
    let touching = edit::apply(
        OPERATORS_FILE,
        &[
            Edit::unset("run.max_tokens"),
            Edit::set("run.max_retries", "3"),
        ],
    )
    .unwrap();
    assert!(
        !touching.contains("max_tokens"),
        "the insertion re-created the line that was unset:\n{touching}"
    );
    assert!(
        touching.contains("max_retries = 3"),
        "the new key was deleted by the unset beside it:\n{touching}"
    );
    let inserted = touching.find("max_retries").unwrap();
    let agent = touching.find("[[agent]]").unwrap();
    assert!(
        inserted < agent,
        "the new key landed outside `[run]`:\n{touching}"
    );

    // And the one batch that has no reading worth guessing at is refused rather
    // than resolved: two edits on one path describe two overlapping splices of
    // the same line, computed against offsets that no longer hold once either
    // has been applied.
    let conflict = edit::apply(
        OPERATORS_FILE,
        &[
            Edit::unset("run.max_steps"),
            Edit::set("run.max_steps", "45"),
        ],
    )
    .unwrap_err();
    assert!(
        conflict.contains("run.max_steps"),
        "the refusal does not name the path both edits claimed: {conflict}"
    );
    assert!(
        edit::apply(OPERATORS_FILE, &[Edit::set("run.max_steps", "45")])
            .unwrap()
            .contains("max_steps = 45"),
        "the guard refuses a batch that has only one edit on the path"
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
    let config = loaded(&down);
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
    let config = loaded(&up);
    assert_eq!(config.fallback_specs().len(), 1, "{up}");
    assert!(
        up.find("groq").unwrap() < up.find("openrouter").unwrap(),
        "the move did not reorder the chain:\n{up}"
    );
}

/// **A whole new section, once — the shape `set` cannot express and the one the
/// price fill could not do without.**
///
/// Every edit in a batch is resolved against the document as it was *before* the
/// batch: [`edit::apply`] walks the headers once and answers every `set` from that
/// walk. So N `set`s addressing keys of a section that does not exist yet each
/// fall through to the append arm, and each emits its own copy of the header. The
/// file gains N definitions of one table and the read-back refuses the lot — which
/// is what happened to the first price fill, at four hundred models, before this
/// existed. Nothing about the failure named the cause: the operator was told "the
/// edit would have produced a file that does not parse".
///
/// `Kind::Section` is the answer, and the constraint on it is as important as the
/// capability. It **refuses** a section that is already there rather than replacing
/// it, because a caller holding an existing section wants `set` per key — that
/// leaves every row it did not name alone, which for `[prices.models]` is every
/// model the catalogue stopped serving and every rate the operator corrected by
/// hand. A replace here would delete both from a call site whose author believed
/// they were adding rows.
///
/// Sabotage: drop the existence check and splice unconditionally, under which the
/// second half of this test passes by writing a file with two `[app.io-cli]`
/// headers in it. Or emit the header per key rather than once, which is the shape
/// that made the failure this exists to remove.
#[test]
fn f1_a_whole_section_is_written_once_and_refused_when_it_is_already_there() {
    let body = "\"gpt-4.1\" = { input = 2000000, output = 8000000 }\n\
                \"gpt-4o\" = { input = 2500000, output = 10000000 }\n\
                \"o3\" = { input = 2000000, output = 8000000 }";
    let after = edit::apply(
        OPERATORS_FILE,
        &[
            Edit::set("prices.as_of", "\"2026-08-27\""),
            Edit::section("prices.models", body),
        ],
    )
    .expect("a section the file does not have is written whole");

    // One header, whatever the row count. The count is asserted rather than left
    // to the parse, because a duplicate is only usually a parse error.
    assert_eq!(
        after.matches("[prices.models]").count(),
        1,
        "the section was written more than once:\n{after}"
    );

    // The operator's own file is still their file, byte for byte, with the new
    // sections after it — the same preservation property every other edit here
    // keeps.
    assert!(
        after.starts_with(OPERATORS_FILE),
        "writing a new section rewrote the document above it:\n{after}"
    );

    // And it says what it was asked to say. Three dotted ids, each a key rather
    // than a path, read back through a parser that has no opinion about io-cli.
    let parsed: toml::Value = toml::from_str(&after).expect("the result parses");
    assert_eq!(parsed["prices"]["as_of"].as_str(), Some("2026-08-27"));
    for model in ["gpt-4.1", "gpt-4o", "o3"] {
        assert!(
            parsed["prices"]["models"][model]["input"]
                .as_integer()
                .is_some(),
            "`{model}` did not land as a key of the section:\n{after}"
        );
    }

    // The refusal. `[app.io-cli]` is in the fixture, so writing it whole would
    // discard the theme — and the message says which section and why rather than
    // reporting a parse failure the caller cannot act on.
    let error = edit::apply(
        OPERATORS_FILE,
        &[Edit::section("app.io-cli", "theme = \"light\"")],
    )
    .expect_err("a section that already exists must not be written whole");
    assert!(
        error.contains("app.io-cli") && error.contains("already in this file"),
        "the refusal does not name the section it refused: {error}"
    );
    assert!(
        error.contains("key by key"),
        "the refusal does not say what to do instead: {error}"
    );
}

// The adversarial pass over `Edit::unset`'s byte arithmetic, written as cases
// rather than as a reading. One of the two review agents for this release died to
// a rate limit before it reached this module, so these stand in for it — a
// self-review finds different things from an independent one and is not a
// substitute, but an unrun gate is worse than a narrower one.

/// A CRLF file keeps its CRLF, and the cut does not strand a lone `\r`.
///
/// `Edit::unset` runs forward to the first `\n` at or after the value's end. On a
/// Windows checkout that newline is preceded by `\r`, which belongs to the line
/// being removed — a cut that stopped at the `\r` would leave it behind and put a
/// bare carriage return in the middle of the next key's line.
#[test]
fn unset_takes_the_whole_line_on_a_crlf_file() {
    let text = "[run]\r\nmax_steps = 8\r\nmax_tokens = 500\r\n";
    let after = io_cli::edit::apply(text, &[io_cli::edit::Edit::unset("run.max_steps")])
        .expect("the unset applies");

    assert_eq!(after, "[run]\r\nmax_tokens = 500\r\n");
    assert!(
        !after.contains("\r\r") && !after.contains("\n\r\n"),
        "a stray carriage return was left behind: {after:?}"
    );
    let config = io_harness::config::Config::from_toml(&after).expect("it still parses");
    assert!(io_cli::edit::value_at(&after, "run.max_steps").is_none());
    let _ = config;
}

/// The last key of a file with no trailing newline.
///
/// The forward scan finds no `\n` and falls back to `text.len()`. If that fallback
/// were off by one the splice would panic on a range past the end, and it is the
/// one input where the fallback is taken at all.
#[test]
fn unset_removes_the_last_key_of_a_file_that_does_not_end_in_a_newline() {
    let text = "[run]\nmax_steps = 8\nmax_tokens = 500";
    let after = io_cli::edit::apply(text, &[io_cli::edit::Edit::unset("run.max_tokens")])
        .expect("the unset applies");

    assert_eq!(after, "[run]\nmax_steps = 8\n");
    assert!(io_cli::edit::value_at(&after, "run.max_tokens").is_none());
    assert_eq!(
        io_cli::edit::value_at(&after, "run.max_steps").as_deref(),
        Some("8")
    );
}

/// A value containing the bytes of a section header does not confuse the cut.
///
/// The scan works from the value's own span, so a `#` or a `[run]` inside a string
/// is just text. Asserted because a line-oriented splicer that searched for those
/// markers instead would cut in the wrong place, and this module is line-oriented.
#[test]
fn unset_is_not_confused_by_a_value_that_looks_like_syntax() {
    let text = "[app.io-cli.gates]\n\
                contains = \"[run] # not a header\"\n\
                rubric = \"keep me\"\n";
    let after = io_cli::edit::apply(
        text,
        &[io_cli::edit::Edit::unset("app.io-cli.gates.contains")],
    )
    .expect("the unset applies");

    assert_eq!(after, "[app.io-cli.gates]\nrubric = \"keep me\"\n");
    assert_eq!(
        io_cli::edit::value_at(&after, "app.io-cli.gates.rubric").as_deref(),
        Some("\"keep me\""),
        "the sibling was disturbed by a value that contained a header"
    );
}

/// Unsetting the only key of a section leaves the section standing and empty.
///
/// Deliberate rather than incidental: an empty `[run]` is legal TOML and still
/// says the operator meant to configure the section. Removing the header would be
/// `Edit::remove`'s job, and doing it here is the destructive ambiguity the two
/// verbs are kept apart to prevent.
#[test]
fn unset_of_the_only_key_leaves_its_section_standing() {
    let text = "[run]\nmax_steps = 8\n\n[memory]\nmax_entries = 5\n";
    let after = io_cli::edit::apply(text, &[io_cli::edit::Edit::unset("run.max_steps")])
        .expect("the unset applies");

    assert!(
        after.contains("[run]"),
        "the header was taken too: {after:?}"
    );
    assert_eq!(
        io_cli::edit::value_at(&after, "memory.max_entries").as_deref(),
        Some("5"),
        "the following section was disturbed"
    );
    io_harness::config::Config::from_toml(&after).expect("an empty section still parses");
}
