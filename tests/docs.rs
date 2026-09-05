//! The documentation is checked against the code, not against a reviewer's
//! memory.
//!
//! The keybinding table is a shipped artifact, and a table that has drifted from
//! what the keys actually do is worse than no table: it is folklore with a
//! typeface. The same constants feed `/help` and the README, and these tests fail
//! when one of them stops agreeing.

use std::path::PathBuf;
use std::sync::{Mutex, MutexGuard};

use io_cli::commands::{COMMANDS, KEYS};

/// Held by every test in this file that reads or writes an `IO_CONFIG*` variable.
///
/// The same shape `tests/wizard.rs` and `tests/home.rs` use. The environment is
/// process-wide and this binary's tests share a process, so a second writer makes
/// the first one's answer wrong intermittently — the most expensive kind of
/// failure to diagnose, and the reason this guard exists before there is a
/// second writer rather than after.
fn env_lock() -> MutexGuard<'static, ()> {
    static LOCK: Mutex<()> = Mutex::new(());
    LOCK.lock().unwrap_or_else(|poisoned| poisoned.into_inner())
}

fn repo() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
}

/// One shipped file, with its line endings normalised.
///
/// **The `\r` strip is why this file passes on Windows.** Git checks these
/// documents out with CRLF there, and every helper below reasons about `\n` — most
/// sharply [`paragraph`], which ends a paragraph at `"\n\n"` and on a CRLF
/// checkout found no boundary at all, took the rest of the document, and reported
/// that a page naming eleven commands named all thirty-eight. Normalising once
/// here rather than in each helper is the difference between one rule and a guard
/// every future reader has to remember.
///
/// Caught by the release matrix on 0.33.0, with macOS and Linux green — which is
/// what the matrix is for.
fn read(name: &str) -> String {
    std::fs::read_to_string(repo().join(name))
        .unwrap_or_else(|error| panic!("{name}: {error}"))
        .replace("\r\n", "\n")
}

/// The pages that describe the product **as it is now**, for a sweep that
/// forbids a stale claim.
///
/// `CHANGELOG.md` is excluded, and the exclusion is the point rather than a
/// convenience. A changelog is a diary: its 0.16.0 entry records
/// "a contained turn cannot be steered" under Known limitations, which was true
/// when it was written and is exactly what a reader of that entry needs to see.
/// Sweeping it for present-tense truth would force the history to be rewritten
/// every time the product moved, which is the one thing a changelog must never do.
fn shipped_prose() -> Vec<(String, String)> {
    shipped_markdown()
        .into_iter()
        .filter(|(path, _)| path != "CHANGELOG.md")
        .collect()
}

/// One guide page, by slug.
///
/// 0.30.2 moved the manual off the README and onto `docs/guide/`, and a needle
/// left pointing at the README would have gone **vacuous rather than red** for
/// every negative assertion — `!contains` is satisfied by any file that does not
/// carry the claim, an empty one included. Every gate below names the page that
/// carries the claim it is checking, and the positive ones fail loudly if that
/// page is wrong, which is what makes the re-pointing checkable at all.
fn guide(slug: &str) -> String {
    read(&format!("docs/guide/{slug}.md"))
}

/// The text between two markers.
fn section<'a>(text: &'a str, name: &str) -> &'a str {
    let start = format!("<!-- {name}:start -->");
    let end = format!("<!-- {name}:end -->");
    let from = text
        .find(&start)
        .unwrap_or_else(|| panic!("the README has no {start}"))
        + start.len();
    let to = text
        .find(&end)
        .unwrap_or_else(|| panic!("the README has no {end}"));
    &text[from..to]
}

#[test]
fn the_readme_key_table_is_the_key_table() {
    let page = guide("keys");
    let table = section(&page, "keys");

    for (key, what) in KEYS {
        assert!(
            table.contains(key),
            "the guide's key table is missing {key}",
        );
        assert!(
            table.contains(what),
            "the guide describes {key} differently from the code",
        );
    }

    let rows = table.lines().filter(|line| line.starts_with("| `")).count();
    assert_eq!(
        rows,
        KEYS.len(),
        "the guide's key table has {rows} rows and the code binds {}",
        KEYS.len(),
    );
}

/// The rebinding table under the key table, which is a second table with a second
/// job: the first says what a key does, this one says what a configuration file
/// may call it and what it is bound to today.
///
/// It sits outside the `keys` markers because [`KEYS`] is not a list of actions —
/// it holds the composer's keys and an approval's letters too — so a "rebindable?"
/// column there would have been a column that is empty for five of eleven rows.
/// What replaces it is this: the row is generated from `Action` and asserted, so
/// the prose cannot name an action the code does not have, quote a default the
/// code does not bind, or offer `interrupt` as something to move.
#[test]
fn the_readme_says_which_actions_can_be_rebound_and_to_what() {
    use io_cli::keys::{Action, Keys};

    let page = guide("keys");
    let keys = Keys::default();

    for action in Action::ALL.iter().copied() {
        let row = format!("| `{}` | `{}` |", action.name(), keys.binding(action));
        if action.rebindable() {
            assert!(
                page.contains(&row),
                "the guide should offer `{}` with its default, as `{row}`",
                action.name(),
            );
        } else {
            assert!(
                !page.contains(&format!("| `{}` |", action.name())),
                "the guide offers `{}` as rebindable and the code refuses it",
                action.name(),
            );
        }
    }
}

#[test]
fn the_readme_command_table_is_the_command_table() {
    let page = guide("commands");
    let table = section(&page, "commands");

    for (name, what) in COMMANDS {
        assert!(table.contains(name), "the guide is missing {name}");
        assert!(
            table.contains(what),
            "the guide describes {name} differently from the code",
        );
    }

    let rows = table
        .lines()
        .filter(|line| line.starts_with("| `/"))
        .count();
    assert_eq!(rows, COMMANDS.len());
}

/// The README's account of the skills this crate ships.
///
/// The names are read out of `skills/` rather than listed here, so a sixth
/// shipped skill fails this test instead of arriving undocumented — which is the
/// shape of drift the command table already has a gate for and this section did
/// not. The resolved name is the frontmatter `name:` and not the filename,
/// because that is the name io-harness addresses a skill by and therefore the
/// name an operator has to avoid claiming.
///
/// The two rules asserted under it are the ones an operator can only learn from
/// prose: a duplicate name is fatal at the harness level, and there is a ceiling
/// that rejects the whole set rather than the excess. Both are asserted on a
/// short distinctive phrase rather than on a sentence, because a sentence gets
/// reworded and a gate that fails on an improvement teaches people to delete
/// gates.
#[test]
fn the_readme_documents_every_skill_this_crate_ships() {
    let page = guide("skills");
    // The whole page is the section now. Splitting on a heading was how this
    // narrowed a 2,847-line README to the part that was about skills; a guide
    // page needs no narrowing, and keeping the split would have meant asserting
    // against whatever happened to precede the first sub-heading.
    let section = page.as_str();

    let mut shipped = 0;
    for entry in std::fs::read_dir(repo().join("skills")).expect("the shipped skills directory") {
        let path = entry.expect("a directory entry").path();
        if path.extension().and_then(|end| end.to_str()) != Some("md") {
            continue;
        }
        let body = std::fs::read_to_string(&path).expect("a skill body");
        let name = body
            .lines()
            .find_map(|line| line.strip_prefix("name:"))
            .unwrap_or_else(|| panic!("{} declares no name in its frontmatter", path.display()))
            .trim();
        assert!(
            section.contains(&format!("`{name}`")),
            "the guide should name the {name} skill and say what it is for",
        );
        shipped += 1;
    }
    assert!(shipped > 0, "there should be skills to document");

    // The collision rule, in both halves: what it costs, and what io-cli does
    // about it. Half of it is worse than none — "io-cli withholds a colliding
    // skill" without the consequence reads as tidiness rather than as the
    // session-killer it avoids.
    assert!(
        section.contains("every turn of that session"),
        "the guide should say what a duplicate skill name costs",
    );
    assert!(
        section.contains("frontmatter"),
        "the guide should say the claimed name is the frontmatter name, not the filename",
    );
    assert!(
        section.contains("withheld"),
        "the guide should say io-cli withholds rather than overwrites a claimed name",
    );

    // And the ceiling, with the number, because "a limit on skills" is not
    // actionable and 64 is.
    assert!(
        section.contains("64 skills"),
        "the guide should name the 64-skill ceiling",
    );
    assert!(
        section.contains("rejects the whole set"),
        "the guide should say the ceiling rejects the set rather than trimming it",
    );
}

/// The `[app.io-cli]` table in the README, sliced by the prose either side of it.
///
/// It has no `<!-- -->` markers because it is not generated — the descriptions are
/// written for a reader, not derived from a doc comment. What is asserted instead
/// is that it has a row for every key the struct actually carries, which is the
/// gate that was missing: `max_steps` was added to `CliSettings` in 0.10.0 and
/// this table never mentioned it once.
fn settings_table(readme: &str) -> &str {
    let from = readme
        .find("keys live there")
        .expect("the README introduces the [app.io-cli] table");
    let to = readme[from..]
        .find("Because the section is unvalidated")
        .expect("the prose that closes the [app.io-cli] table")
        + from;
    &readme[from..to]
}

#[test]
fn the_readme_documents_every_key_of_the_io_cli_section() {
    // Serialized rather than listed by hand, so a key added to `CliSettings`
    // fails here instead of shipping undocumented. A scalar is written as
    // `theme`; a table is `[app.io-cli.keys]` and a list of tables is
    // `[[app.io-cli.mcp]]`, and any of the three spellings satisfies the row.
    let page = guide("configuration");
    let table = settings_table(&page);

    // Every field set, because `skip_serializing_if` drops a `None` — a default
    // value here would assert against a table with nothing in it.
    let every = io_cli::settings::CliSettings {
        theme: Some("dark".into()),
        diff: Some("unified".into()),
        glyphs: Some("ascii".into()),
        plain: Some(false),
        keys: Some(Default::default()),
        containment: Some(io_harness::Containment::new(12, 4, 2, 200_000)),
        mcp: Some(Vec::new()),
        lsp: Some(Vec::new()),
        browser: Some(io_harness::BrowserConfig::default()),
        skills: Some("/skills".into()),
        max_parallel_reads: Some(16),
        spawn_background_after_secs: Some(120),
        detached_spawns: Some(true),
        prices: Some(io_cli::settings::PriceSettings {
            source_url: Some("https://example.invalid/models".into()),
            source: Some("the reference catalogue".into()),
            models: Some(417),
        }),
        // Written out field by field rather than as a `Default::default()`, which
        // would serialize to `{}` and satisfy the row check just as well. The
        // point is the compile error: `[app.io-cli.gates]` is the one nested
        // table whose keys are explained in README prose rather than in a row of
        // their own, so a key added to `gates::Settings` has to break something
        // to be noticed. This is that something.
        gates: Some(io_cli::gates::Settings {
            retries: Some(2),
            command: Some(vec!["cargo".into(), "test".into()]),
            expect_exit: Some(0),
            file: Some("CHANGELOG.md".into()),
            contains: Some("## [0.24.0]".into()),
            rubric: Some("the change is covered by a test that fails without it".into()),
            reviewer: Some("a-reviewing-model".into()),
            allow_self_review: Some(false),
        }),
        conversational: Some(false),
        // Written out field by field for `gates`' reason, and with the same
        // consequence: `[app.io-cli.routing]`'s two rules are sub-tables whose
        // keys are explained in README prose rather than in rows of their own, so
        // a key added to `routing::Settings` has to break something to be noticed.
        routing: Some(io_cli::routing::Settings {
            escalate_after: Some(io_cli::routing::Escalation {
                failures: Some(3),
                model: Some("a-stronger-model".into()),
            }),
            downshift_under: Some(io_cli::routing::Downshift {
                bytes: Some(2_000),
                model: Some("a-cheaper-model".into()),
            }),
        }),
    };
    let value = serde_json::to_value(&every).expect("[app.io-cli] serializes");
    let keys = value.as_object().expect("a table");

    let (mut scalars, mut tables) = (0, 0);
    for (name, held) in keys {
        let spellings = [
            format!("| `{name}` |"),
            format!("| `[app.io-cli.{name}]` |"),
            format!("| `[[app.io-cli.{name}]]` |"),
        ];
        assert!(
            spellings.iter().any(|row| table.contains(row)),
            "the guide's [app.io-cli] table has no row for `{name}`",
        );
        if held.is_object() || held.is_array() {
            tables += 1;
        } else {
            scalars += 1;
        }
    }

    let rows = table.lines().filter(|line| line.starts_with("| `")).count();
    assert_eq!(
        rows,
        keys.len(),
        "the guide's [app.io-cli] table has {rows} rows and the section has {}",
        keys.len(),
    );

    // The sentence above the table counts them, and a count written in prose is
    // the first half of a table to go stale — this one said "five keys and four
    // tables" over a table of five keys and five tables, for four releases.
    let words = [
        "no", "One", "Two", "Three", "Four", "Five", "Six", "Seven", "Eight", "Nine", "Ten",
        "Eleven", "Twelve",
    ];
    let sentence = format!(
        "{} keys live there, and {} tables",
        words[scalars],
        words[tables].to_lowercase(),
    );
    assert!(
        page.contains(&sentence),
        "the guide should say `{sentence}`",
    );
}

#[test]
fn the_readme_quotes_the_budget_fields_the_status_line_actually_draws() {
    // 0.14.0's F6. The budgets are the one part of the status line a README can
    // quote verbatim, and a quoted format that has drifted is worse than no
    // example: an operator reads it as the thing to grep their scrollback for.
    let page = guide("the-session");
    let mut status = io_cli::status::Status::new("a-model");
    status.budgets = io_cli::status::Budgets {
        steps: Some(20),
        tokens: Some(200_000),
        duration: Some(std::time::Duration::from_secs(600)),
        // The context window is a denominator rather than a remainder, so it
        // draws no budget row of its own — `ctx N%` is where it shows. `None`
        // here keeps this fixture about the three that do draw rows.
        window: None,
    };
    status.steps = Some(3);
    status.run_tokens = Some(187_600);
    status.elapsed = std::time::Duration::from_secs(330);

    let drawn = status.budgets_left();
    assert_eq!(drawn.len(), 3, "one field per budget in force: {drawn:?}");
    for text in drawn {
        assert!(
            page.contains(&format!("`{text}`")),
            "the guide should quote the budget field the line draws: `{text}`",
        );
    }

    // And a session that configured nothing draws none of them, which is the
    // half of F6 that keeps io-cli's own step floor off the line.
    let quiet = io_cli::status::Status::new("a-model");
    assert!(quiet.budgets_left().is_empty());
}

#[test]
fn no_documentation_surface_still_claims_the_old_asymmetry() {
    // Through 0.13.1 both files said an interactive session read past most of
    // `io.toml` and that `[app.io-cli.containment]` was what carried the
    // capabilities. The first stopped being true in 0.14.0 and the second in
    // 0.11.0. A claim this specific cannot be caught by reading the diff of the
    // release that falsifies it, so it is caught here.
    // Every shipped page rather than the two files that carried the claim in
    // 0.13.1. A negative gate pointed at one file goes **vacuous** the moment the
    // prose moves — `!contains` is satisfied by a file that never mentioned the
    // subject — and 0.30.2 moved this claim onto a guide page. Scanning the whole
    // set cannot go quiet that way, and costs nothing.
    let example = read("docs/config.example.toml");
    let mut surfaces: Vec<(String, String)> = shipped_prose();
    surfaces.push(("docs/config.example.toml".to_string(), example.clone()));

    for (name, text) in &surfaces {
        for stale in [
            "Not read by an interactive session",
            "Contained turns only",
            "and on no other turn",
        ] {
            assert!(
                !text.contains(stale),
                "{name} still says {stale:?}, which has not been true since 0.14.0",
            );
        }
    }

    // Every section `Config::apply_to` carries is named in the example as
    // something that reaches both arms, so a reader looking for one finds it
    // where the old block used to deny it.
    for section in [
        "[sandbox]",
        "[run]",
        "[run.commit_identity]",
        "[instructions]",
        "[[mcp]]",
        "[[lsp]]",
        "[[agent]]",
        "[web]",
        "[memory]",
        "[browser]",
    ] {
        assert!(
            example.contains(section),
            "docs/config.example.toml should document {section}",
        );
    }

    // The two facts about that widening an operator has to be told rather than
    // left to discover: `[web]` is a capability the *vendor* exercises, so the
    // local `net` rule is not what governs it, and `[browser]` is refused in a
    // project-scoped file by io-harness itself.
    let configuration = guide("configuration");
    for (name, text) in [
        ("docs/guide/configuration.md", &configuration),
        ("docs/config.example.toml", &example),
    ] {
        let said = text.to_lowercase();
        assert!(
            said.contains("capability") && said.contains("vendor"),
            "{name} should say `[web]` is a capability and that the vendor dials the URL",
        );
        assert!(
            said.contains("project"),
            "{name} should say where `[browser]` is refused",
        );
    }
}

/// Every comment in the crate, read for this release's version of the same
/// mistake: **a sentence saying a turn cannot be steered.**
///
/// The test above catches the 0.14.0 shape of it in two prose files. This one is
/// the 0.17.0 shape and it reads the *source*, because that is where the claim
/// lived: through io-harness 0.66 no session entry point took a caller's
/// containment and a `SteerInbox` on one call, so a dozen comments explained the
/// arms in terms of what each gave up to get the other. 0.67.0 opened
/// `turn_bounded_steered` and `turn_contained_bounded_steered`, io-cli took both,
/// and every one of those sentences became folklore with a typeface — the exact
/// failure this file exists for. A reviewer reading the release diff sees the two
/// call sites change and has no reason to open `settings.rs`; this does.
///
/// **Comment lines only, and that is the whole of the narrowing.** Two things
/// must go on saying these words and neither is a claim about today:
///
/// - the assertions that hold io-harness's *deaf* entry points to their name —
///   `turn_bounded_observed` really does take no inbox, and `tests/steer.rs` says
///   so in a `format!` — and the arm of `tests/contain.rs` that asserts
///   `"cannot be steered"` is absent from the containment notice. Both are code,
///   not comments, so neither is read here.
/// - `CHANGELOG.md`, which is a record of releases rather than a description of
///   this one: its 0.14.0 and 0.11.0 entries were true when they were written and
///   rewriting them would be falsifying the history. Its `0.17.0` section is
///   checked by `the_changelog_has_a_section_for_this_version` instead.
///
/// And the sentences explaining why `Ctrl+C` is **not** on the inbox are the
/// point of the release rather than a violation of it. They say the interrupt
/// travels as `Flow::Cancel` on the observer; none of them says a turn cannot be
/// steered, so none of the phrases below can match one.
///
/// Sabotage: put any single corrected comment back — `settings.rs`'s "neither
/// turn takes a `SteerInbox` any more", `main.rs`'s "Neither turn takes a
/// `SteerInbox`", `commands.rs`'s "fan out, or be steered", `README.md`'s "no
/// session turn takes a steer inbox". Only this test fails, and it names the file
/// and the sentence.
#[test]
fn no_comment_still_says_a_turn_cannot_be_steered() {
    // Each of these was a true sentence in some release and is a false one now.
    // Written as the words that carry the claim rather than as whole sentences,
    // because the sentence around them is what an author rewrites while leaving
    // the claim standing.
    const FALSEHOODS: &[&str] = &[
        // "no entry point takes a contract and an inbox together"
        "containment and a steer inbox together",
        "a contract and a steer inbox together",
        // "neither arm takes an inbox"
        "neither takes a steer inbox",
        "neither turn takes a `SteerInbox`",
        "Neither turn takes a `SteerInbox`",
        "takes a `SteerInbox` any more",
        "no session turn takes a steer inbox",
        "session turn takes a `SteerInbox`",
        // "a contained turn takes no inbox"
        "A contained turn takes no",
        "contained turn takes no `SteerInbox`",
        // "containment is what decides whether a turn can be steered"
        "fan out, or be steered",
        "be steered and one that can fan out",
        "either fans out or is steerable",
        "turned steering off",
        "cannot be steered",
    ];

    let mut sources = Vec::new();
    collect(&repo().join("src"), &mut sources);
    collect(&repo().join("tests"), &mut sources);

    for path in sources {
        // This file, and only this file, is allowed to write the sentences down:
        // the list above is what they are, and the doc comment names four of them
        // so the sabotage arm can be run without going to find them. A gate that
        // could not quote what it forbids would have to describe it instead, and
        // a description is the thing that drifts.
        if path.file_name().is_some_and(|name| name == "docs.rs") {
            continue;
        }
        let text = std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("{path:?}: {e}"));
        let shown = path.strip_prefix(repo()).unwrap_or(path.as_path());
        for (number, line) in text.lines().enumerate() {
            if !line.trim_start().starts_with("//") {
                continue;
            }
            for claim in FALSEHOODS {
                assert!(
                    !line.contains(claim),
                    "{}:{} still says {claim:?}. A contained turn CAN be steered \
                     since 0.17.0 — both arms are driven through the `_steered` \
                     entry points and both hold a `SteerInbox`. Containment \
                     decides fan-out and nothing else.\n  {}",
                    shown.display(),
                    number + 1,
                    line.trim(),
                );
            }
        }
    }

    // The prose carries the same claim in sentences rather than in comments, and
    // it is spread across the guide pages now. Every shipped page is read, not
    // just the README: a negative gate aimed at one file stops being a gate the
    // moment the sentence it was watching moves to another.
    for (name, text) in shipped_prose() {
        for claim in FALSEHOODS {
            assert!(
                !text.contains(claim),
                "{name} still says {claim:?}, which has not been true since 0.17.0",
            );
        }
    }

    // And the positive half, because a gate made only of absences passes on a
    // file that says nothing at all. Somewhere the documentation has to state
    // what is true now, or an operator learns that containment costs a steer from
    // the silence where the correction should be.
    // "Somewhere" is the original gate's own word, and translating it to one named
    // page would be a stronger claim than the gate ever made — `/steer` is
    // documented on the commands page and the keys page, and which of those owns
    // the sentence is an editorial choice this test has no business deciding.
    let stated = shipped_prose().into_iter().any(|(_, text)| {
        let said = text.to_lowercase();
        said.contains("steer inbox") || said.contains("/steer")
    });
    assert!(
        stated,
        "no page says a turn can be spoken to while it runs, so an operator \
         learns that containment costs a steer from the silence where the \
         correction should be",
    );
    let corrected = shipped_prose().into_iter().any(|(_, text)| {
        text.to_lowercase()
            .contains("contained turn can be steered")
    });
    assert!(
        corrected,
        "no page says the containment switch no longer decides whether a turn \
         can be steered, which is the half a reader who remembers 0.16.0 needs",
    );
}

/// Every `.rs` file under `dir`, recursively.
fn collect(dir: &std::path::Path, into: &mut Vec<PathBuf>) {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            collect(&path, into);
        } else if path.extension().is_some_and(|ext| ext == "rs") {
            into.push(path);
        }
    }
}

#[test]
fn the_removed_step_cap_is_announced_where_it_was_promised() {
    // F12. The key is GONE from `CliSettings` as of 0.16.0, and the notice is
    // not — because `CliSettings` carries no `deny_unknown_fields`, so a file
    // still holding the key would otherwise be ignored in silence and the
    // operator's step cap would change with no error anywhere.
    //
    // So the notice reads the RAW section now. It is built through
    // `Config::from_toml` rather than from a struct, which is the only way left
    // to express "a file that still has this key" — there is no field to set.
    let still_has_it =
        io_harness::Config::from_toml("[app.io-cli]\nmax_steps = 40\ntheme = \"dark\"\n")
            .expect("a leftover key still LOADS, which is exactly the problem");
    let notice = io_cli::settings::deprecated_max_steps(&still_has_it)
        .expect("a file that still writes the key earns the notice");
    assert!(notice.contains("[run] max_steps"), "{notice}");
    assert!(notice.contains("0.16.0"), "{notice}");
    assert!(
        notice.contains("no longer read"),
        "the notice must say the key is dead, not merely deprecated: {notice}"
    );

    for name in ["README.md", "CHANGELOG.md"] {
        let text = read(name);
        assert!(
            text.contains("`[app.io-cli] max_steps`"),
            "{name} should name the removed key",
        );
        assert!(
            text.contains("removed in 0.16.0"),
            "{name} should say when the key stopped being read",
        );
    }

    // A file that never wrote it is told nothing, which is the other half of F12
    // — a notice on a session that is not affected teaches operators to stop
    // reading notices.
    let clean = io_harness::Config::from_toml("[app.io-cli]\ntheme = \"dark\"\n").unwrap();
    assert!(io_cli::settings::deprecated_max_steps(&clean).is_none());
    let empty = io_harness::Config::from_toml("").unwrap();
    assert!(io_cli::settings::deprecated_max_steps(&empty).is_none());
}

#[test]
fn the_changelog_has_a_section_for_this_version() {
    // `release.yml` refuses to cut a Release without one, and finding that out
    // from a failed workflow is four cross-compiles too late.
    let version = env!("CARGO_PKG_VERSION");
    let changelog = read("CHANGELOG.md");
    assert!(
        changelog.contains(&format!("## [{version}]")),
        "CHANGELOG.md has no '## [{version}]' section",
    );

    let start = changelog
        .find(&format!("## [{version}]"))
        .expect("the section");
    let rest = &changelog[start..];
    let body = rest
        .split_once("\n## [")
        .map(|(body, _)| body)
        .unwrap_or(rest);
    assert!(
        body.lines().filter(|line| !line.trim().is_empty()).count() > 3,
        "the {version} section is a heading with nothing under it",
    );
}

/// The 0.19.0 section, and the one thing in it a reader cannot find out any
/// other way.
///
/// A command that changes group changes where an operator finds it: nothing
/// about `/mcp` or `/provider` was rewritten, so the only trace of the move is
/// that `/help` lists them somewhere else than it did yesterday. A release note
/// that leaves that out has documented the features and hidden the one change
/// that will make somebody think a command was removed. Pinned to `0.19.0`
/// rather than to `CARGO_PKG_VERSION`, because this is a fact about one release
/// and not a rule about every one.
#[test]
fn the_changelog_says_which_commands_changed_group() {
    let changelog = read("CHANGELOG.md");
    let start = changelog
        .find("## [0.19.0]")
        .expect("CHANGELOG.md should have a 0.19.0 section");
    let rest = &changelog[start..];
    let body = rest
        .split_once("\n## [")
        .map(|(body, _)| body)
        .unwrap_or(rest);

    for moved in ["`/mcp`", "`/provider`"] {
        assert!(
            body.contains(moved),
            "the 0.19.0 section should say {moved} changed group",
        );
    }
    assert!(
        body.contains("configure"),
        "the 0.19.0 section should say which group they moved to",
    );
}

#[test]
fn the_readme_states_what_the_checksum_does_not_defend_against() {
    // N7's own words: the README says this rather than implying more. A `curl |
    // sh` install is a trust-the-publisher model however the script is written,
    // and a page that leaves that unsaid is overselling it.
    let readme = read("README.md");
    assert!(
        readme.contains("compromised repository"),
        "the README should say what the checksum does not cover",
    );
    assert!(readme.contains("trust-the-publisher"), "{readme}");
}

/// The paragraph that says where io keeps an operator's files.
///
/// It is prose rather than a generated table, and until 0.15.0 nothing asserted
/// a word of it — which is how a repository ends up with a discovery ladder in
/// its README that no release has read since the one that wrote it. The
/// directory name is taken from [`io_cli::home::path`] and the variable from
/// io-harness, so renaming either fails here rather than leaving the README
/// naming a directory the binary no longer uses.
#[test]
fn the_readme_says_where_io_keeps_its_files() {
    let page = guide("configuration");
    let (_, section) = page
        .split_once("### Where io keeps your things")
        .expect("the guide should have a section saying where io keeps its files");
    let section = section.split("\n## ").next().unwrap_or(section);

    let home = io_cli::home::path().expect("a home directory to take the name from");
    let dir = home
        .file_name()
        .expect("the home is a directory under the operator's own")
        .to_string_lossy()
        .into_owned();
    // The last two arrived with 0.19.0, and they are here rather than in the
    // skills section's own gate because this is the paragraph an operator reads
    // when they are asking what io put on their disk. `disabled/` is a directory
    // that changes behaviour, and the manifest is a dotfile they did not create;
    // both are things to find named somewhere before finding them by accident.
    for named in [
        format!("`~/{dir}`"),
        format!("`%USERPROFILE%\\{dir}`"),
        format!("`~/{dir}/skills`"),
        format!("`~/{dir}/skills/disabled/`"),
        format!("`~/{dir}/.skills-manifest`"),
    ] {
        assert!(
            section.contains(&named),
            "the guide should name {named}, which is where io-cli actually looks",
        );
    }

    // The ladder in the order io-harness walks it. Asserted as positions and not
    // as four `contains`, because a paragraph that names all four in the wrong
    // order is the way this sentence goes wrong and every `contains` passes on
    // it.
    let mut walked = 0;
    for rung in [
        "`$IO_CONFIG`",
        "`$IO_CONFIG_HOME/io.toml`",
        "`$XDG_CONFIG_HOME/io/io.toml`",
        "`%APPDATA%\\io\\io.toml`",
    ] {
        let at = section
            .find(rung)
            .unwrap_or_else(|| panic!("the guide should name {rung} in the discovery order"));
        assert!(at > walked, "{rung} is out of order in the guide's ladder");
        walked = at;
    }

    // N4: io-cli sets the variable in its own process environment, so every
    // child a session starts inherits it. A README that names the home and
    // leaves that out has documented the outcome and hidden the mechanism.
    let var = io_harness::config::CONFIG_HOME_VAR;
    assert!(
        section.contains(var),
        "the guide should name the {var} io-cli sets to put the file there",
    );
    assert!(
        section.contains("inherit") && section.contains("nested `io`"),
        "the guide should say every child a session starts inherits {var}",
    );

    // And the one act of this release that touches files io-cli did not create,
    // said where an operator upgrading will read it rather than in a changelog.
    assert!(
        section.contains("moved into the home"),
        "the guide should say an existing install is moved on the first run",
    );
    assert!(
        section.contains("nothing is overwritten") && section.contains("Nothing is deleted"),
        "the guide should say what the move does not do",
    );
}

#[test]
fn the_shipped_configuration_example_is_a_file_io_harness_accepts() {
    // The example is documentation that runs: if the harness stops accepting it,
    // it has stopped being an example and started being a trap.
    let text = std::fs::read_to_string(repo().join("docs/config.example.toml"))
        .expect("docs/config.example.toml");
    let dir = tempfile::tempdir().expect("a temporary directory");
    let home = dir.path().join("home");
    let workspace = dir.path().join("workspace");
    std::fs::create_dir_all(&home).expect("a home");
    std::fs::create_dir_all(&workspace).expect("a workspace");
    let path = home.join("io.toml");
    std::fs::write(&path, &text).expect("write the example");

    // Read as the USER scope, which is what it documents and where widening is
    // allowed. Read as a project file it would rightly be refused.
    //
    // Under the lock from 0.15.0. This was the one `IO_CONFIG*` writer in this
    // repository's tests that took no guard, and it was harmless only while
    // nothing else in the process wrote that variable — which stopped being true
    // when `home::adopt` became a writer of it.
    let _guard = env_lock();
    std::env::set_var("IO_CONFIG", &path);
    let config = io_harness::Config::discover(&workspace);
    std::env::remove_var("IO_CONFIG");

    let config = config.expect("io-harness accepts the shipped example");
    assert!(
        config.provider_spec().is_some(),
        "the example should configure a provider",
    );
    assert!(
        config.policy().is_some(),
        "the example should configure a policy",
    );
    let settings: Option<io_cli::settings::CliSettings> =
        config.app(io_cli::settings::APP_KEY).expect("[app.io-cli]");
    assert_eq!(
        settings.and_then(|settings| settings.theme).as_deref(),
        Some("dark"),
    );
}

#[test]
fn every_documentation_file_the_repository_promises_exists() {
    for name in [
        "LICENSE",
        "NOTICE",
        "README.md",
        "CHANGELOG.md",
        "CONTRIBUTING.md",
        "CODE_OF_CONDUCT.md",
        "SECURITY.md",
        "install.sh",
        "install.ps1",
        "docs/config.example.toml",
        ".github/CODEOWNERS",
        ".github/dependabot.yml",
        ".github/PULL_REQUEST_TEMPLATE.md",
        ".github/workflows/ci.yml",
        ".github/workflows/release.yml",
    ] {
        assert!(
            repo().join(name).is_file(),
            "{name} is referred to but missing",
        );
    }
}

#[test]
fn the_notice_carries_the_attribution_and_the_licence_body_is_untouched() {
    let notice = read("NOTICE");
    assert!(
        notice.contains("Copyright 2026 Aakash Pawar (InitOrigin)"),
        "{notice}"
    );
    assert!(read("README.md").contains("Copyright 2026 Aakash Pawar (InitOrigin)"));

    // The Apache text itself is pristine: the attribution goes in NOTICE and the
    // README, never into the licence.
    let licence = read("LICENSE");
    assert!(licence.starts_with("                                 Apache License"));
    assert!(
        !licence.contains("InitOrigin"),
        "the Apache-2.0 body must be unmodified",
    );
}

#[test]
fn the_workspace_directory_is_not_shipped_inside_the_crate() {
    // `.ultraship` is planning state: tool-written maintainer notes, not part of
    // what a user installs and not a contributor-facing surface. The whole tree
    // is ignored, so the rule is one line and there is no sub-path to keep in
    // step with it. The bare entry is asserted rather than a `contains`, because
    // a narrower rule such as `.ultraship/products/*/evidence/` also contains it.
    let gitignore = read(".gitignore");
    assert!(
        gitignore.lines().any(|line| line.trim() == ".ultraship/"),
        "the planning tree must stay off the repository: {gitignore}",
    );
}

#[test]
fn the_readme_exit_table_is_the_exit_table() {
    // The exit codes are public contract from 0.5.0 onward: a script branches on
    // them and cannot be migrated by fixing forward. A table in the guide that
    // drifted from the constants would be worse than no table at all.
    let page = guide("headless");
    for (code, what) in [
        (io_cli::exec::OK, "of its own accord"),
        (io_cli::exec::FAILED, "never got that far"),
        (io_cli::exec::REFUSED, "a boundary said no"),
        (io_cli::exec::CEILING, "a ceiling was reached"),
        (io_cli::exec::PAUSED, "needing a human"),
        (io_cli::exec::UNFINISHED, "without finishing"),
    ] {
        let row = format!("| `{code}` |");
        assert!(
            page.contains(&row),
            "the guide's exit table has no row for {code}",
        );
        assert!(
            page.contains(what),
            "the guide describes {code} differently from the code: {what}",
        );
    }

    // 0.24.0's `6`, and it is asserted as a NUMBER rather than through a constant
    // on purpose. What a script branches on is the integer, and the one mistake
    // this release could make that a caller cannot recover from is landing the
    // gate code on a number that already meant something else. So the claim is
    // that `6` is the code immediately after `UNFINISHED`, which is where the six
    // that shipped in 0.5.0 stopped. Move any of those six and this fails; give
    // the gate a number one of them already holds and it fails too.
    assert_eq!(
        io_cli::exec::UNFINISHED + 1,
        6,
        "the gate code sits after the six that shipped in 0.5.0; a code that moved \
         one of them cannot be migrated by fixing forward",
    );
    assert!(
        page.contains("| `6` |"),
        "the guide's exit table has no row for the gate that did not pass",
    );
    assert!(
        page.contains("does not hold up"),
        "the guide should say what `6` means in the words the release uses: the \
         agent finished and the work does not hold up",
    );
    // And the half a reader most needs: nothing above it moved. A release that
    // renumbered `5` would leave every script branching on it silently wrong,
    // which is the failure the sentence exists to rule out.
    assert!(
        page.contains("No exit code was renumbered"),
        "the guide should say, where the table is, that no existing code changed \
         meaning",
    );
}

#[test]
fn the_readme_lists_every_flag_io_exec_actually_takes() {
    use clap::CommandFactory;

    let page = guide("headless");
    let cli = io_cli::cli::Cli::command();
    let exec = cli
        .get_subcommands()
        .find(|sub| sub.get_name() == "exec")
        .expect("`io exec` is a subcommand");

    let flags: Vec<String> = exec
        .get_arguments()
        .filter_map(|arg| arg.get_long().map(|long| format!("--{long}")))
        .filter(|long| long != "--help")
        .collect();

    assert!(!flags.is_empty(), "there should be flags to check");
    for flag in &flags {
        assert!(
            page.contains(&format!("`{flag}")),
            "`io exec` takes {flag} and the guide does not mention it",
        );
    }

    // And nothing the guide promises has been removed from the binary.
    for promised in ["--json", "--sandbox", "--policy", "--provider"] {
        assert!(
            flags.iter().any(|flag| flag == promised),
            "the guide documents {promised} and `io exec` no longer takes it",
        );
    }
}

/// The same gate for `io resume`, which is 0.23.0's subcommand and has three
/// times as many flags to go stale.
///
/// The absent flag is asserted as well, and it is not symmetry for its own sake:
/// the README states in words that there is deliberately no `--sandbox` here,
/// because a resumed run already started under a boundary and widening it halfway
/// through is a widening nobody asked for at the point nobody is watching. A
/// `--sandbox` added later would leave that paragraph standing and wrong, which is
/// precisely the failure this file exists for.
#[test]
fn the_readme_documents_every_flag_io_resume_actually_takes() {
    use clap::CommandFactory;

    let page = guide("headless");
    let cli = io_cli::cli::Cli::command();
    let resume = cli
        .get_subcommands()
        .find(|sub| sub.get_name() == "resume")
        .expect("`io resume` is a subcommand");

    let flags: Vec<String> = resume
        .get_arguments()
        .filter_map(|arg| arg.get_long().map(|long| format!("--{long}")))
        .filter(|long| long != "--help")
        .collect();

    assert!(!flags.is_empty(), "there should be flags to check");
    for flag in &flags {
        assert!(
            page.contains(&format!("`{flag}")),
            "`io resume` takes {flag} and the guide does not mention it",
        );
    }

    // Each pause's own input, because a table that lost one of these would leave
    // an operator with a run they can see and cannot decide.
    for promised in ["--list", "--answer", "--plan", "--recovery", "--goal"] {
        assert!(
            flags.iter().any(|flag| flag == promised),
            "the guide documents {promised} and `io resume` no longer takes it",
        );
    }
    assert!(
        !flags.iter().any(|flag| flag == "--sandbox"),
        "the guide says `io resume` takes no --sandbox, and it now does",
    );
}

/// The pause that cannot be resumed, said where an operator is deciding what to
/// do about one.
///
/// **A gate on an absence would pass on a page that says nothing**, which is why
/// this reads the section rather than sweeping the file: a turn the operator
/// interrupted is recorded `cancelled`, mapped to a *completed* run, and every
/// io-harness resume entry point hands back the original outcome having driven
/// nothing. An operator who is not told that reads a `/resume` row for such a
/// session as a row they can choose, and the release's own answer — `/fork` from
/// the turn before — is the thing they never find.
///
/// The marks are taken from [`io_cli::sessions`] rather than listed here, for the
/// reason the command table is generated: a sixth state, or a renamed one, fails
/// this test instead of leaving the README teaching a word that is no longer
/// drawn.
#[test]
fn the_readme_says_which_pause_cannot_be_resumed() {
    use io_cli::sessions::{DIED_MARK, ENDED_MARK, PLAN_MARK, QUESTION_MARK, RECOVERY_MARK};

    let page = guide("resume");
    let section = page.as_str();

    for mark in [
        QUESTION_MARK,
        PLAN_MARK,
        RECOVERY_MARK,
        DIED_MARK,
        ENDED_MARK,
    ] {
        assert!(
            section.contains(&format!("`{mark}`")),
            "the guide should name the `{mark}` mark and say what it means",
        );
    }

    // The mechanism, then the consequence, then what to do instead. Short
    // distinctive phrases rather than sentences, because the sentence around a
    // claim is what an author rewrites while leaving the claim standing.
    assert!(
        section.contains("cancelled"),
        "the guide should say what io-harness records a Ctrl+C as",
    );
    assert!(
        section.contains("cannot be answered"),
        "the guide should say an interrupted turn is the one pause that cannot be resumed",
    );
    assert!(
        section.contains("`/fork`"),
        "the guide should offer /fork, which is what an ended turn leaves you",
    );
}

// ---------------------------------------------------------------------------
// 0.30.0 F19 — every claim a module makes about io-harness is true of the
// io-harness that is pinned.
// ---------------------------------------------------------------------------

/// The io-harness version `Cargo.lock` actually pins.
///
/// Read from the lock and never from `Cargo.toml`, for the reason the release
/// process records: the manifest states a *requirement* (`"0.71"`) and the lock
/// states the *resolution* (`0.71.0`), and a source comment citing a line number
/// is citing the file that was resolved.
fn pinned_harness() -> String {
    let lock = read("Cargo.lock");
    let at = lock
        .find("name = \"io-harness\"")
        .expect("io-harness is in the lock file");
    let rest = &lock[at..];
    let line = rest
        .lines()
        .find(|line| line.starts_with("version = "))
        .expect("the pinned version follows the name");
    line.trim_start_matches("version = ")
        .trim_matches('"')
        .to_string()
}

/// **No source file cites a line inside an io-harness this crate does not pin.**
///
/// This is the mechanical half of the doc-truth sweep, and it exists because the
/// expensive half is not mechanical at all. Nine citations of the form
/// `io-harness-0.69.0/src/config.rs:1888` were still in the tree at 0.30.0, two
/// pins after that version — each one a line number that had moved, in a file
/// whose contents had changed, presented to the next reader as a fact they could
/// go and check.
///
/// A citation into the pinned version can still be *wrong*; a citation into a
/// version that is not pinned cannot even be checked. This gate catches the second
/// kind, which is the kind that accumulates silently.
///
/// Sabotage: put back any one of the nine. It names the file and the version.
#[test]
fn f19_no_source_cites_an_io_harness_this_crate_does_not_pin() {
    let pinned = pinned_harness();
    let wanted = format!("io-harness-{pinned}/");
    let mut stale: Vec<String> = Vec::new();

    // **Recursive, and every citation on a line rather than the first.** Both were
    // holes in the first version of this gate: `find` returns one match, so a line
    // carrying two citations was checked once and the second could name any version
    // at all; and a flat `read_dir` covers `src/` only for as long as `src/` stays
    // flat, which is not a property anybody is maintaining.
    //
    // **0.36.0 — `tests/` is swept too, and it was not before.** The pin to
    // 0.76.0 left a stale `io-harness-0.74.0/src/plugin.rs:1097` in
    // `tests/adapt.rs` and this gate passed over it, because the walk started at
    // `src/` alone. A test file's citation is read by exactly the same person for
    // exactly the same reason as a source file's, and the argument above — that a
    // citation into an unpinned version cannot even be checked — does not care
    // which directory the comment is in.
    let mut files: Vec<std::path::PathBuf> = Vec::new();
    let mut dirs = vec![repo().join("src"), repo().join("tests")];
    while let Some(dir) = dirs.pop() {
        for entry in std::fs::read_dir(&dir).expect("a source directory") {
            let path = entry.expect("a directory entry").path();
            if path.is_dir() {
                dirs.push(path);
            } else if path.extension().and_then(|e| e.to_str()) == Some("rs")
                // **This file is exempt from its own sweep, and the exemption is
                // the point rather than a convenience.** A gate that reads prose
                // forbids a file from explaining itself — a rule this repository
                // has now paid for four times, most visibly when the stale-claim
                // sweep hit a CHANGELOG entry written to say a placeholder had
                // been removed. This test cannot state which citation form it
                // looks for, quote the nine that motivated it, or build its own
                // needle with `format!`, without matching itself.
                && path.file_name().and_then(|n| n.to_str()) != Some("docs.rs")
            {
                files.push(path);
            }
        }
    }
    files.sort();

    for path in &files {
        let text = std::fs::read_to_string(path).expect("a source file");
        for (number, line) in text.lines().enumerate() {
            // Only the `io-harness-<version>/` form, which is a path into a
            // vendored source tree. A bare "io-harness 0.70.0" is history — a
            // sentence about when something changed — and is not a citation.
            for (at, _) in line.match_indices("io-harness-") {
                // A version literal begins with a digit. Without this the sweep
                // also flags the machinery that *builds* such a path —
                // `format!("io-harness-{version}")` in `tests/support/mod.rs`
                // resolves at runtime to whatever is pinned and is the opposite
                // of a stale citation. Checking the shape is better than
                // exempting the file: the exemption would also hide a real
                // citation that file later grew.
                let rest = &line[at + "io-harness-".len()..];
                if !rest.starts_with(|c: char| c.is_ascii_digit()) {
                    continue;
                }
                if !line[at..].starts_with(&wanted) {
                    stale.push(format!(
                        "{}:{}: {}",
                        path.strip_prefix(repo()).unwrap_or(path).display(),
                        number + 1,
                        line.trim(),
                    ));
                }
            }
        }
    }

    assert!(
        stale.is_empty(),
        "these cite a line inside an io-harness that is not pinned (the pin is \
         {pinned}), so the line numbers are unverifiable and the surrounding \
         claims are unchecked:\n{}",
        stale.join("\n"),
    );
}

/// The markdown a reader meets, by path relative to the root.
///
/// Walked rather than listed, because a list is a second place to add a file and
/// the one that never gets updated. Dotted directories are skipped, which drops
/// `.ultraship/` — gitignored whole, the maintainer's working notes rather than a
/// reader-facing surface — and also `.github/`'s issue and pull-request
/// templates, which no criterion here covers. `target/` holds nothing shipped.
fn shipped_markdown() -> Vec<(String, String)> {
    fn walk(dir: &std::path::Path, root: &std::path::Path, out: &mut Vec<(String, String)>) {
        let Ok(entries) = std::fs::read_dir(dir) else {
            return;
        };
        for entry in entries.flatten() {
            let path = entry.path();
            let name = entry.file_name();
            let name = name.to_string_lossy();
            if name.starts_with('.') || name == "target" || name == "node_modules" {
                continue;
            }
            if path.is_dir() {
                walk(&path, root, out);
            } else if path.extension().is_some_and(|ext| ext == "md") {
                let relative = path
                    .strip_prefix(root)
                    .expect("walked from the root")
                    .to_string_lossy()
                    .replace('\\', "/");
                // Line endings normalised for the same reason the path separator
                // above is: these pages are read on three platforms and every
                // sweep below reasons about `\n`. See `read`.
                let text = std::fs::read_to_string(&path)
                    .unwrap_or_default()
                    .replace("\r\n", "\n");
                out.push((relative, text));
            }
        }
    }

    let root = repo();
    let mut out = Vec::new();
    walk(&root, &root, &mut out);
    out.sort();
    out
}

/// **F7 — no shipped document contains an unfilled placeholder.**
///
/// `SECURITY.md` forbade the public-issue fallback and then named a literal
/// `<project-contact-email>`, and `CODE_OF_CONDUCT.md` carried the same token —
/// so from 0.1.0 to 0.30.1 this project told people not to open an issue and
/// gave them nowhere else to go. The README routes every vulnerability report at
/// `SECURITY.md`, so the dead end was the only documented path.
///
/// The needle is not "an angle bracket": `docs/CONTRACT.md` legitimately writes
/// `io exec "<goal>"` and `<subcommand>`, and a gate that banned those would be
/// reverted the first time someone documented a command's arguments. It is the
/// **template** shape — an angle-bracket token naming a contact that was meant to
/// be substituted and was not.
///
/// The changelog is exempt, through [`shipped_prose`], and it earned the
/// exemption immediately: 0.30.2's own entry has to quote the placeholder in
/// order to say it was removed, and the first run of this gate failed on that
/// sentence. **A gate that reads prose can forbid a file from explaining
/// itself** — the third time this repository has hit that shape — and the
/// changelog is the one file whose job is to describe what used to be there.
///
/// Sabotage: put `<project-contact-email>` back into either file. Only this fails.
#[test]
fn f7_no_shipped_document_leaves_a_contact_placeholder_unfilled() {
    let mut offenders = Vec::new();

    for (path, text) in shipped_prose() {
        for (number, line) in text.lines().enumerate() {
            // An angle-bracket token that names a contact rather than an argument.
            // `<goal>`, `<path>` and `<version>` are argument spellings and are
            // deliberately not matched.
            let mut rest = line;
            while let Some(open) = rest.find('<') {
                let after = &rest[open + 1..];
                let Some(close) = after.find('>') else { break };
                let token = &after[..close];
                let looks_like_contact =
                    token.contains("contact") || token.contains("email") || token.contains("your-");
                // A URL in angle brackets is a markdown autolink, not a placeholder.
                let is_autolink = token.starts_with("http");
                if looks_like_contact && !is_autolink {
                    offenders.push(format!("{path}:{}: <{token}>", number + 1));
                }
                rest = &after[close + 1..];
            }
        }
    }

    assert!(
        offenders.is_empty(),
        "these documents ship an unfilled contact placeholder, so a reader \
         following them reaches nobody:\n{}",
        offenders.join("\n"),
    );
}

/// Every relative link in a markdown file, as (containing file, target).
///
/// An absolute URL is not this gate's business: `https://` is checked by nobody
/// offline. What is left is the set of links that can rot silently when a file
/// moves — which is exactly what this release does to a 2,847-line README. The
/// fragment on a target is dropped here and checked by [`anchor_links`], which
/// asks the other half of the question: the file is there, but is the heading?
fn relative_links() -> Vec<(String, String)> {
    let mut links = Vec::new();

    for (path, text) in shipped_markdown() {
        let mut rest = text.as_str();
        while let Some(open) = rest.find("](") {
            let after = &rest[open + 2..];
            let Some(close) = after.find(')') else { break };
            let target = &after[..close];
            rest = &after[close + 1..];

            // A link target may carry a title: `](path "Title")`. Take the path.
            let target = target.split_whitespace().next().unwrap_or(target);
            if target.is_empty()
                || target.starts_with('#')
                || target.starts_with("http://")
                || target.starts_with("https://")
                || target.starts_with("mailto:")
            {
                continue;
            }
            // Drop a fragment on a file link: `guide/keys.md#moving-a-key`.
            let target = target.split('#').next().unwrap_or(target);
            if target.is_empty() {
                continue;
            }
            links.push((path.clone(), target.to_string()));
        }
    }

    links
}

/// **F4 — no relative link is dead.**
///
/// Written before the guide pages exist, so it fails until they do. A link check
/// is a few lines of `std`; adding a markdown crate to a tree held at ten direct
/// dependencies to find a missing file would be the expensive answer to the cheap
/// question.
///
/// Sabotage: point any link at a file that is not there. Only this fails.
#[test]
fn f4_every_relative_link_resolves_to_a_file_that_exists() {
    let root = repo();
    let mut dead = Vec::new();

    for (from, target) in relative_links() {
        let base = std::path::Path::new(&from)
            .parent()
            .map(std::path::Path::to_path_buf)
            .unwrap_or_default();
        let resolved = root.join(&base).join(&target);
        if !resolved.exists() {
            dead.push(format!("{from} → {target}"));
        }
    }

    assert!(
        dead.is_empty(),
        "these links point at files that are not there, so a reader following \
         the documentation reaches a 404:\n{}",
        dead.join("\n"),
    );
}

/// The anchor GitHub gives each heading in a markdown file, in document order.
///
/// GitHub's rule: lowercase, punctuation dropped, spaces to hyphens. So
/// `## What it costs` is reached as `#what-it-costs`, and the backticks around
/// `## `[app.io-cli.keys]`` are no part of its anchor. A heading repeated inside
/// one file gets `-1` on the second and `-2` on the third, which is why this
/// returns the slugs in order rather than a set.
///
/// A `#` inside a fenced block is a shell prompt or a comment, not a heading, so
/// the fences are tracked and their contents skipped.
fn heading_slugs(text: &str) -> Vec<String> {
    let mut bases: Vec<String> = Vec::new();
    let mut slugs = Vec::new();
    let mut fenced = false;

    for line in text.lines() {
        let trimmed = line.trim_start();
        if trimmed.starts_with("```") || trimmed.starts_with("~~~") {
            fenced = !fenced;
            continue;
        }
        if fenced {
            continue;
        }
        let Some(rest) = trimmed.strip_prefix('#') else {
            continue;
        };
        let heading = rest.trim_start_matches('#');
        // `#notaheading` is a fragment or a hashtag; a heading has a space.
        if !heading.starts_with(' ') {
            continue;
        }
        let base: String = heading
            .trim()
            .to_lowercase()
            .chars()
            .filter(|c| c.is_alphanumeric() || *c == ' ' || *c == '-' || *c == '_')
            .map(|c| if c == ' ' { '-' } else { c })
            .collect();

        let seen = bases.iter().filter(|already| *already == &base).count();
        slugs.push(if seen == 0 {
            base.clone()
        } else {
            format!("{base}-{seen}")
        });
        bases.push(base);
    }

    slugs
}

/// Every anchor link in a markdown file, as (containing file, line, the page the
/// heading is promised to be in, fragment).
///
/// Both spellings land here with the promise made explicit: `](#x)` promises a
/// heading in the containing file, `](page.md#x)` one in `page.md`, so the gate
/// below asks a single question of both. Links inside a fenced block are examples
/// of what to type, not links a reader can click, and are skipped.
fn anchor_links() -> Vec<(String, usize, PathBuf, String)> {
    let root = repo();
    let mut links = Vec::new();

    for (path, text) in shipped_markdown() {
        let base = std::path::Path::new(&path)
            .parent()
            .map(std::path::Path::to_path_buf)
            .unwrap_or_default();
        let mut fenced = false;

        for (number, line) in text.lines().enumerate() {
            let trimmed = line.trim_start();
            if trimmed.starts_with("```") || trimmed.starts_with("~~~") {
                fenced = !fenced;
                continue;
            }
            if fenced {
                continue;
            }

            let mut rest = line;
            while let Some(open) = rest.find("](") {
                let after = &rest[open + 2..];
                let Some(close) = after.find(')') else { break };
                let target = &after[..close];
                rest = &after[close + 1..];

                // A link target may carry a title: `](path "Title")`. Take the path.
                let target = target.split_whitespace().next().unwrap_or(target);
                if target.starts_with("http://")
                    || target.starts_with("https://")
                    || target.starts_with("mailto:")
                {
                    continue;
                }
                let Some((page, fragment)) = target.split_once('#') else {
                    continue;
                };
                if fragment.is_empty() {
                    continue;
                }
                let page = if page.is_empty() {
                    root.join(&path)
                } else {
                    root.join(&base).join(page)
                };
                links.push((path.clone(), number + 1, page, fragment.to_string()));
            }
        }
    }

    links
}

/// **F4, the third part — no anchor link is dead.**
///
/// The split moved byte ranges faithfully and left every in-page anchor pointing
/// at a heading that had gone to another file: 34 of the 46 fragment-only links
/// in the shipped documentation resolved to nothing, and a reader clicking one
/// watched the page not move. Nothing caught it, because the link gate above
/// dropped the fragment and asked only whether the file was there — for a
/// fragment-only link, the file is always there.
///
/// The non-empty assertion is load-bearing for the same reason it is on the
/// orphan gate: a parser that quietly stops matching would iterate nothing and
/// pass, which is the vacuous-gate shape this repository has shipped four times.
///
/// Sabotage: change one anchor to a heading that is not in the page it names, or
/// move a heading whose anchor is linked. Only this fails.
#[test]
fn f4_every_anchor_link_resolves_to_a_heading() {
    let root = repo();
    let links = anchor_links();

    assert!(
        links.len() > 20,
        "only {} anchor links were found across the documentation, so this gate \
         is checking almost nothing — the parser stopped matching",
        links.len(),
    );

    let mut dead = Vec::new();
    for (from, line, page, fragment) in links {
        let shown = page.strip_prefix(&root).unwrap_or(&page).display();
        let Ok(text) = std::fs::read_to_string(&page) else {
            dead.push(format!(
                "{from}:{line}: #{fragment} — no {shown} to hold it"
            ));
            continue;
        };
        if !heading_slugs(&text).iter().any(|slug| slug == &fragment) {
            dead.push(format!(
                "{from}:{line}: #{fragment} is in no heading of {shown}"
            ));
        }
    }

    assert!(
        dead.is_empty(),
        "these anchors name a heading that is not in the page they point at, so \
         a reader clicking one watches the page not move:\n{}",
        dead.join("\n"),
    );
}

/// **F4, the other half — no guide page is orphaned.**
///
/// A split that leaves a page linked from nowhere is worse than not splitting:
/// the content is gone from where it was and reachable only by someone who
/// already knows the filename. Every page must be reachable from **both**
/// indexes, because each is a way in — the README for a reader arriving at the
/// repository, `docs/CAPABILITIES.md` for one arriving at the docs directory.
///
/// The non-empty assertion is load-bearing. With no guide pages on disk the two
/// loops below iterate nothing and the test passes while proving nothing, which
/// is the vacuous-gate shape this repository has shipped four times.
///
/// Sabotage: add a guide page linked from neither index; or drop one row from
/// either index. Each fails on its own.
#[test]
fn f4_every_guide_page_is_reachable_from_both_indexes() {
    let guides: Vec<String> = shipped_markdown()
        .into_iter()
        .map(|(path, _)| path)
        .filter(|path| path.starts_with("docs/guide/"))
        .collect();

    assert!(
        !guides.is_empty(),
        "no guide pages were found under docs/guide/, so the two checks below \
         would pass by iterating nothing",
    );

    let readme = read("README.md");
    let capabilities = read("docs/CAPABILITIES.md");

    let mut orphans = Vec::new();
    for guide in &guides {
        // The README sits at the root and links `docs/guide/x.md`;
        // CAPABILITIES.md sits in `docs/` and links `guide/x.md`.
        let from_readme = guide.as_str();
        let from_capabilities = guide.trim_start_matches("docs/");

        if !readme.contains(from_readme) {
            orphans.push(format!("{guide} is not linked from README.md"));
        }
        if !capabilities.contains(from_capabilities) {
            orphans.push(format!("{guide} is not linked from docs/CAPABILITIES.md"));
        }
    }

    assert!(
        orphans.is_empty(),
        "these guide pages are not reachable from an index, so the split moved \
         content somewhere nobody is pointed at:\n{}",
        orphans.join("\n"),
    );
}

/// **F1 — a command is documented under the group the code files it in.**
///
/// `the_readme_command_table_is_the_command_table` above checks that every
/// command appears with its description, and that the row count matches. Both
/// were true while `/contain` sat under **this turn** in the prose and under
/// `Group::Session` in `GROUPS` — the name was present, the description matched,
/// and the total was right, because a row in the wrong table is still a row.
///
/// The cost was not only a misfiled command. `Group::Turn` is capped at ten and
/// the same page says so, so the printed table showed eleven rows against a bound
/// stated three hundred lines below it. It drifted for three releases because
/// nothing joined a command to its heading.
///
/// Sabotage: move any row into another group's table. Only this fails.
#[test]
fn f1_every_command_is_documented_under_its_own_group() {
    use io_cli::commands::{grouped, Group};

    let page = guide("commands");
    let table = section(&page, "commands");

    // The prose draws each group under a bold title, which is `Group::title()` —
    // the same string the palette and `/help` draw, so the join is on a value the
    // code owns rather than on a heading a writer chose.
    let titles: Vec<(Group, String)> = Group::all()
        .into_iter()
        .map(|group| (group, format!("**{}**", group.title())))
        .collect();

    for (_, title) in &titles {
        assert!(
            table.contains(title.as_str()),
            "the guide's command section has no {title} heading",
        );
    }

    for (group, rows) in grouped() {
        let title = format!("**{}**", group.title());
        let from = table.find(&title).expect("checked above") + title.len();
        let rest = &table[from..];
        // Up to the next group heading, whichever comes first.
        let to = titles
            .iter()
            .filter_map(|(_, other)| {
                if *other == title {
                    None
                } else {
                    rest.find(other.as_str())
                }
            })
            .min()
            .unwrap_or(rest.len());
        let block = &rest[..to];

        for (name, _) in rows {
            let row = format!("| `{name}` |");
            assert!(
                block.contains(&row),
                "`{name}` is filed under `{}` in GROUPS and is not in the \
                 README's {title} table, so the palette and the documentation \
                 disagree about where an operator should look for it",
                group.title(),
            );
        }
    }
}

/// The text under a heading, up to the next heading at the same level or above.
///
/// The `<!-- name:start -->` markers above are for tables a test needs to read
/// exactly. This is for prose, where wrapping a section in comment markers to
/// make it checkable would put scaffolding in the file a reader sees.
fn under_heading<'a>(text: &'a str, heading: &str) -> &'a str {
    let from = text
        .find(heading)
        .unwrap_or_else(|| panic!("no heading {heading:?}"))
        + heading.len();
    let rest = &text[from..];
    let to = rest.find("\n## ").unwrap_or(rest.len());
    &rest[..to]
}

/// **F2 — the plugin install is described as it behaves since 0.30.0.**
///
/// Through 0.29.0 there was no io-harness loader that took a directory, so the
/// only way to have a stranger's bundle validated was to declare it: the install
/// wrote `enabled = false`, re-discovered, and disclosed off the result. A bundle
/// io-harness then refused left an entry behind in a file the operator had agreed
/// to nothing about. 0.71.0 published `Plugins::inspect` and 0.30.0 switched to
/// it — and the README went on describing the mechanism that had been deleted,
/// telling a reader their configuration is written to before they consent.
///
/// Both halves are asserted. The prose half alone would pass if the feature were
/// ripped out; the code half alone would pass while the prose stayed wrong.
///
/// Sabotage: restore the write-then-rediscover paragraph, or delete the
/// `Plugins::inspect` call. Each fails on its own.
#[test]
fn f2_the_install_discloses_before_it_writes_and_says_so() {
    // The mechanism exists in the code the prose is describing.
    let marketplace = std::fs::read_to_string(repo().join("src/marketplace.rs"))
        .expect("src/marketplace.rs exists");
    assert!(
        marketplace.contains("Plugins::inspect"),
        "the disclosure is documented as reading the bundle with the operator's \
         file untouched, which is `Plugins::inspect`; nothing in \
         src/marketplace.rs calls it",
    );

    let page = guide("plugins");
    let section = under_heading(
        &page,
        "### What a bundle is allowed to do is shown before it is allowed to do it",
    );

    assert!(
        section.contains("Nothing is written to your configuration before you agree"),
        "the disclosure section should state the property that makes it a \
         disclosure rather than a fait accompli",
    );

    // The retracted narrative, in its own words. Taken from the bytes 0.29.0
    // shipped rather than from a paraphrase — a needle written from memory is how
    // this repository has shipped four gates that matched nothing.
    for retracted in [
        "The entry is written **`enabled = false`** first",
        "Saying no leaves the bundle declared, switched off",
        "read from the manifest",
    ] {
        assert!(
            !section.contains(retracted),
            "the install section describes the pre-0.30.0 mechanism, which wrote \
             to the operator's configuration before asking: {retracted:?}",
        );
    }
}

/// **F6 — `docs/CONTRACT.md` agrees with the code it describes.**
///
/// A contract page is the one document a script author depends on, so a wrong
/// exit code or a missing configuration key there is worse than no page. All
/// three halves are asked of the code rather than of a second copy of the list:
/// the exit codes come from `exec`'s own constants, the configuration keys from
/// `CliSettings`' fields, and the subcommands from `clap` itself.
///
/// Sabotage: add a field to `CliSettings`, add a subcommand, or change an exit
/// constant, without documenting it. Each fails.
#[test]
fn f6_the_contract_page_agrees_with_the_code() {
    use clap::CommandFactory as _;

    let contract = read("docs/CONTRACT.md");

    // The exit codes, by number and by name.
    for (code, name) in [
        (io_cli::exec::OK, "OK"),
        (io_cli::exec::FAILED, "FAILED"),
        (io_cli::exec::REFUSED, "REFUSED"),
        (io_cli::exec::CEILING, "CEILING"),
        (io_cli::exec::PAUSED, "PAUSED"),
        (io_cli::exec::UNFINISHED, "UNFINISHED"),
        (io_cli::exec::UNVERIFIED, "UNVERIFIED"),
    ] {
        let row = format!("| `{code}` | {name} |");
        assert!(
            contract.contains(&row),
            "docs/CONTRACT.md should carry the exit code {name} as `{row}`; a \
             script keying on an exit code reads this table and nothing else",
        );
    }

    // Every subcommand clap routes is named on the page.
    for sub in io_cli::cli::Cli::command().get_subcommands() {
        let name = sub.get_name();
        assert!(
            contract.contains(&format!("`io {name}")),
            "clap routes `io {name}` and docs/CONTRACT.md does not name it",
        );
    }

    // Every `[app.io-cli]` key. The field list is read out of the struct's own
    // source rather than hand-listed here, so a field added without a row on the
    // page fails instead of arriving undocumented.
    let settings =
        std::fs::read_to_string(repo().join("src/settings.rs")).expect("src/settings.rs exists");
    let body = settings
        .split_once("pub struct CliSettings")
        .expect("CliSettings is declared")
        .1;
    let body = body.split_once("\n}").expect("the struct closes").0;

    let mut keys = 0;
    for line in body.lines() {
        let Some(field) = line.trim().strip_prefix("pub ") else {
            continue;
        };
        let Some(field) = field.split(':').next() else {
            continue;
        };
        assert!(
            contract.contains(&format!("| `{field}` |")),
            "`[app.io-cli] {field}` is a field of CliSettings and has no row in \
             docs/CONTRACT.md, so it is a key an operator has no way to learn \
             exists from the one page that promises to list them",
        );
        keys += 1;
    }
    assert!(
        keys > 10,
        "only {keys} fields were read out of CliSettings, so the parse above is \
         matching almost nothing and this gate is not checking what it claims",
    );
    assert!(
        contract.contains(&format!("carries **{}** keys", spell(keys))),
        "docs/CONTRACT.md should say how many keys `[app.io-cli]` carries, and \
         the code has {keys}",
    );
}

/// A small number as the word this repository's prose uses.
fn spell(n: usize) -> &'static str {
    match n {
        11 => "eleven",
        12 => "twelve",
        13 => "thirteen",
        14 => "fourteen",
        15 => "fifteen",
        16 => "sixteen",
        17 => "seventeen",
        18 => "eighteen",
        19 => "nineteen",
        20 => "twenty",
        _ => "an unspelled number",
    }
}

/// **Every CHANGELOG heading is a link, and every link definition has a heading.**
///
/// Thirty of thirty-three version headings had no link definition, so they
/// rendered as literal `[0.29.0]` text on GitHub while the file's own header
/// claimed Keep a Changelog conformance — and `[Unreleased]` compared from a tag
/// four releases old, which is the kind of wrong that looks right.
///
/// Most of F9 is prose with no oracle. This part is not, so it is asserted rather
/// than reviewed: a definition is a mechanical consequence of adding a heading,
/// and mechanical things belong in a test.
///
/// Sabotage: delete any `[x.y.z]:` line, or add a heading without one. Either fails.
#[test]
fn every_changelog_heading_has_a_link_definition_and_the_reverse() {
    let changelog = read("CHANGELOG.md");

    let headings: Vec<String> = changelog
        .lines()
        .filter_map(|line| line.strip_prefix("## ["))
        .filter_map(|rest| rest.split(']').next())
        .map(str::to_string)
        .collect();

    let defined: Vec<String> = changelog
        .lines()
        .filter(|line| line.starts_with('['))
        .filter(|line| line.contains("]: "))
        .filter_map(|line| line.strip_prefix('['))
        .filter_map(|rest| rest.split(']').next())
        .map(str::to_string)
        .collect();

    assert!(
        headings.len() > 30,
        "the CHANGELOG should carry every released version as a heading; found {}",
        headings.len(),
    );

    let undefined: Vec<&String> = headings.iter().filter(|h| !defined.contains(h)).collect();
    assert!(
        undefined.is_empty(),
        "these headings have no link definition, so they render as literal text \
         rather than as a link to the diff: {undefined:?}",
    );

    let dangling: Vec<&String> = defined.iter().filter(|d| !headings.contains(d)).collect();
    assert!(
        dangling.is_empty(),
        "these link definitions name no heading, so a version was renamed or \
         removed and its link was left behind: {dangling:?}",
    );

    assert!(
        changelog.contains("[Unreleased]: ") && changelog.contains("...HEAD"),
        "[Unreleased] should compare the newest tag against HEAD",
    );
}

/// **F7, the other half — the private report route is really named.**
///
/// The test above only proves a placeholder is absent, which an empty file also
/// satisfies. This one proves the replacement is present, so deleting the
/// reporting section to make the first test pass fails this one.
#[test]
fn f7_the_security_policy_names_a_private_route_that_is_not_a_public_issue() {
    const ADVISORY: &str = "security/advisories/new";

    let security = read("SECURITY.md");
    assert!(
        security.contains(ADVISORY),
        "SECURITY.md forbids opening a public issue, so it has to name the \
         private route that replaces it",
    );
    assert!(
        security.contains("Do not open a public issue"),
        "the policy's central instruction is missing",
    );

    let conduct = read("CODE_OF_CONDUCT.md");
    assert!(
        conduct.contains(ADVISORY),
        "CODE_OF_CONDUCT.md asks for reports to reach the maintainers privately, \
         so it has to say through what",
    );
}

/// Every run of whitespace as one space, so a needle is a claim rather than a
/// claim **plus the column the paragraph happened to wrap at**.
///
/// Every gate above matches raw bytes, which is right for a table row and wrong
/// for a sentence: markdown prose here is hard-wrapped at eighty columns, so a
/// two-line sentence carries a newline and two indents that no author chose and
/// every reflow moves. A needle written against one wrapping either fails on the
/// next edit — teaching people to delete the gate — or is shortened until it stops
/// being the claim. Flattening first is what lets the needle be the sentence.
///
/// It does **not** strip markup, and must not: the pages carry `` `/config` ``
/// with its backticks in the bytes, so a needle drops the backticks only where the
/// file does.
fn flat(text: &str) -> String {
    text.split_whitespace().collect::<Vec<_>>().join(" ")
}

/// The paragraph beginning at `opening`, up to the blank line that ends it.
///
/// Narrower than [`under_heading`] because these two lists are paragraphs rather
/// than sections, and a section-wide search would find `/config` in the three
/// paragraphs of prose *about* the split and read them as members of it.
fn paragraph<'a>(text: &'a str, opening: &str) -> &'a str {
    let from = text
        .find(opening)
        .unwrap_or_else(|| panic!("no paragraph opening {opening:?}"));
    let rest = &text[from..];
    let to = rest.find("\n\n").unwrap_or(rest.len());
    &rest[..to]
}

/// Every backticked word in `region` that begins with a slash, in the order the
/// prose names them.
///
/// A command and nothing else: the refused paragraph also backticks `!`, which is
/// a shell escape rather than a command and is asserted for separately.
fn slash_words(region: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut rest = region;
    while let Some(open) = rest.find('`') {
        let after = &rest[open + 1..];
        let Some(close) = after.find('`') else { break };
        let word = &after[..close];
        rest = &after[close + 1..];
        if word.starts_with('/') && word.len() > 1 {
            out.push(word.to_string());
        }
    }
    out
}

/// `spell`'s answer with the capital a sentence opens on.
fn capitalised(word: &str) -> String {
    let mut chars = word.chars();
    match chars.next() {
        Some(first) => first.to_uppercase().collect::<String>() + chars.as_str(),
        None => String::new(),
    }
}

/// **The mid-turn split in the prose is the split `runs_mid_turn` makes.**
///
/// The count and both halves, asked of the code. `docs/guide/keys.md` printed
/// twenty-one refused commands while the code refused twenty-two of them —
/// `/exit` and `/export` had never been listed — and the number in front of the
/// admitted half was `Ten` for a release in which it was eleven. Neither was
/// catchable by reading a diff: the sentence was not touched by the release that
/// falsified it.
///
/// **The two halves are read out of `COMMANDS` and never written down here.**
/// `/steer` and `/compact` are subtracted by name, and that subtraction is the one
/// thing this test states rather than derives: both are refused by `runs_mid_turn`
/// and both nevertheless reach a running turn, through the arms that existed
/// before the mid-turn set did. The prose says so in its own paragraph, so listing
/// them under "refused" would be the page contradicting itself two lines later.
///
/// Sorted before comparison rather than compared in order: membership and count
/// are the claim, and a gate that failed because an author moved `/undo` two
/// commas to the left is a gate that gets deleted.
///
/// Sabotage: put `/config` back in the refused paragraph; drop `/export` from it;
/// add a twelfth name to the admitted paragraph; or write `Ten` in front of
/// either sentence. Each fails, naming the command or the number.
#[test]
fn the_prose_splits_the_commands_the_way_runs_mid_turn_splits_them() {
    use io_cli::commands::runs_mid_turn;

    // Reached through their own arms rather than through the mid-turn set, and
    // named in the prose as exactly that.
    const OWN_ARMS: [&str; 2] = ["/steer", "/compact"];

    let keys = guide("keys");
    let contract = read("docs/CONTRACT.md");

    // **Counted from `COMMANDS`, not from `MID_TURN`.** `MID_TURN` holds
    // *spellings*, and `/settings` is a second spelling of `/config` rather than a
    // command of its own — `parse` reads `"config" | "settings"` in every arm, and
    // `COMMANDS` carries one row for the pair. A count taken from `MID_TURN` would
    // tell an operator there is a twelfth command and then fail to name it. The
    // same reasoning already applies in the other direction to `/copy diff`, which
    // is a `COMMANDS` row and not a command, so the set is deduplicated by first
    // word before it is counted.
    let mut admitted: Vec<&str> = COMMANDS
        .iter()
        .map(|(name, _)| *name)
        .filter(|name| runs_mid_turn(name.trim_start_matches('/')))
        .map(|name| name.split_whitespace().next().unwrap_or(name))
        .collect();
    admitted.dedup();
    let count = spell(admitted.len());
    assert_ne!(
        count,
        "an unspelled number",
        "the mid-turn set is {} commands and `spell` has no word for it, so the \
         two sentences below cannot be checked at all",
        admitted.len(),
    );

    // **`run`, not `report`, since 0.37.0.** The needle said "report" and that was
    // true of all eleven until `/context withhold` was admitted: it changes the
    // session's mask rather than describing anything. The word was the claim, so
    // the claim moved rather than the count — which did not move at all, because
    // `/context` was already in the mid-turn set and gained verbs rather than a
    // row. Weakening this to a substring that spans both words would be the
    // repair that costs the gate its meaning.
    let opens = format!(
        "**{} commands run while the agent works**",
        capitalised(count)
    );
    assert!(
        keys.contains(&opens),
        "docs/guide/keys.md should open the mid-turn paragraph {opens:?}; \
         `COMMANDS` admits {} of them",
        admitted.len(),
    );
    assert!(
        flat(&contract).contains(&format!(
            "**{} commands run while a turn is in flight, and the rest are refused.**",
            capitalised(count),
        )),
        "docs/CONTRACT.md should say how many commands run mid-turn, and the code \
         admits {}",
        admitted.len(),
    );

    // The admitted half, as the prose names it. Compared against the same
    // deduplicated set the count came from, so the sentence and the number in
    // front of it cannot disagree — and so `/settings`, which is `/config`'s other
    // spelling rather than a command, is not demanded of a page that lists
    // commands.
    let mut named = slash_words(paragraph(&keys, &opens));
    named.sort();
    let mut listed_admitted: Vec<String> = admitted.iter().map(|name| name.to_string()).collect();
    listed_admitted.sort();
    assert_eq!(
        named, listed_admitted,
        "docs/guide/keys.md names a different set of mid-turn commands than \
         `COMMANDS` admits",
    );

    // The refused half, likewise — read out of `COMMANDS` through the same
    // predicate the driver asks, so a command that changes sides changes this.
    let refused_opens = "**Everything else keeps that refusal";
    let mut listed = slash_words(paragraph(&keys, refused_opens));
    listed.sort();
    let mut refused: Vec<String> = COMMANDS
        .iter()
        .map(|(name, _)| *name)
        .filter(|name| !runs_mid_turn(name.trim_start_matches('/')))
        .filter(|name| !OWN_ARMS.contains(name))
        .map(str::to_string)
        .collect();
    refused.sort();
    assert_eq!(
        listed, refused,
        "docs/guide/keys.md's refused list is not the refused half of COMMANDS; \
         an operator reading it is told a command is refused that runs, or told \
         nothing about one that is",
    );
    assert!(
        paragraph(&keys, refused_opens).contains("`!` line"),
        "a `!` line is refused mid-turn too and the paragraph has to say so, \
         because it is the one refusal that is not a slash command",
    );

    // And the two that are neither: refused by the predicate, and reaching the
    // turn anyway. A page that dropped this paragraph would leave the list above
    // reading as the whole truth.
    let flattened = flat(&keys);
    for name in OWN_ARMS {
        assert!(
            flattened.contains(&format!("`{name}`")),
            "docs/guide/keys.md stops saying that {name} reaches a running turn \
             through its own arm, so its absence from both lists reads as an \
             omission",
        );
    }
}

/// **No page still describes the two writes 0.33.0 took out of `/config`.**
///
/// Both were true sentences and both are now folklore. The bare `/config` list
/// carried a row that re-read the provider's catalogue and wrote a scope file, and
/// a horizontal arrow on a row wrote the scope file on the keystroke — the only
/// unconfirmed write in the product reachable from a bare arrow key. Every page
/// that explained why `/config` was refused mid-turn explained it in terms of one
/// of those.
///
/// Every shipped page rather than the two that carried the claims, for
/// `no_documentation_surface_still_claims_the_old_asymmetry`'s reason: a negative
/// gate aimed at one file goes **vacuous** the moment the sentence moves, and this
/// project has moved its prose between files once already.
///
/// The positives are the load-bearing half. Four `!contains` assertions are
/// satisfied by four empty files, so each falsehood is paired with the sentence
/// that replaces it, on the page that owns it.
///
/// Sabotage: restore any one of the four sentences, or delete any one of the
/// replacements. Each fails on its own and names the file.
#[test]
fn no_page_still_says_config_writes_a_file_from_the_list_or_from_an_arrow() {
    const FALSEHOODS: &[&str] = &[
        // The count in front of the mid-turn set, which was ten through 0.32.0.
        "Ten commands report while the agent works",
        "Ten commands run while a turn is in flight",
        // The whole-command refusal, and the reason given for it.
        "`/config` is refused even bare",
        "refused mid-turn in every form",
        "picker offers a row that re-reads",
        // The arrow that wrote where it stood.
        "A horizontal arrow changes a boolean or a closed set of words where it stands",
    ];

    for (name, text) in shipped_prose() {
        let said = flat(&text);
        for claim in FALSEHOODS {
            assert!(
                !said.contains(claim),
                "{name} still says {claim:?}, which has not been true since \
                 0.33.0: the bare `/config` list has no row that acts, and \
                 `Left`/`Right` open a setting's values instead of writing one",
            );
        }
    }

    // The replacements, each on the page that owns the claim: which form of
    // `/config` runs mid-turn, that the arrows stopped writing, and where the
    // refresh went.
    for (page, needle) in [
        (
            "keys",
            "`/config` joined the first list in 0.33.0, and only in its bare form.",
        ),
        (
            "configuration",
            "**No arrow key writes a configuration file, and until 0.33.0 one did.**",
        ),
        (
            "configuration",
            "**The price refresh is one descent below `prices.as_of`",
        ),
    ] {
        assert!(
            flat(&guide(page)).contains(needle),
            "docs/guide/{page}.md is missing {needle:?}, so the sentence that \
             replaces a falsehood above is not there and the page is silent \
             rather than corrected",
        );
    }

    const ADMITTED: &str =
        "**`/config` is admitted bare and refused the moment it carries a word.**";
    assert!(
        flat(&read("docs/CONTRACT.md")).contains(ADMITTED),
        "docs/CONTRACT.md is the page a script author reads, so it has to state \
         which spellings of `/config` a running turn accepts",
    );
}

/// **The question surface is documented as the one surface it became.**
///
/// io-harness 0.72.0 shipped batched asks, described choices, previews and
/// multi-select, and every one of them reached this crate silently when the pin
/// moved. A capability that arrives without a sentence is a capability nobody
/// finds: an operator meets `PgUp` by pressing it, or does not.
///
/// Each claim is asserted on the page that owns it rather than through an `any()`
/// over the set, so moving one to a different page fails here and has to be moved
/// deliberately.
///
/// Sabotage: delete any one of these sentences. Each fails, naming the page and
/// the sentence.
#[test]
fn the_guide_describes_a_batched_ask_a_described_offer_and_a_marked_set() {
    let session = flat(&guide("the-session"));
    for needle in [
        // A batch is one overlay, walked, and nothing is sent until it is whole.
        "**An agent can ask several things at once, and they arrive as one overlay.**",
        "**`PgUp` and `PgDn` walk the batch**",
        "**Nothing is sent until every question is decided.**",
        // ...and the two things a reader would otherwise go looking for.
        "There is no review pane and no submit key",
        "`Esc` decides the question on the screen",
        // A description is always drawn; a preview unfolds one at a time.
        "A description is always on the screen, on a row of its own under the label",
        "A preview is a block",
        // And the set.
        "`Space` marks and unmarks the one under the marker",
    ] {
        assert!(
            session.contains(needle),
            "docs/guide/the-session.md is missing {needle:?}, so the question \
             surface 0.33.0 rebuilt is undocumented on the page that owns it",
        );
    }

    // The spacebar is a borrowed key rather than a bound one, and the keys page is
    // where a reader looks for a key. It is deliberately NOT in `commands::KEYS`
    // — that table is what the session binds all the time, asserted row for row by
    // `the_readme_key_table_is_the_key_table`, and this key is held only while a
    // list that accepts several answers is open, exactly as `/config`'s two arrows
    // and the queue's four keys are. A row for it in `KEYS` would put a key in
    // `/help` that most sessions never have.
    const SPACEBAR: &str =
        "borrowed by a question that takes several answers, and it is the spacebar";
    assert!(
        flat(&guide("keys")).contains(SPACEBAR),
        "docs/guide/keys.md lists every key the session borrows, and the spacebar \
         is one from 0.33.0",
    );

    // Headless: one row, one id, one `--answer`, and the limitation stated.
    let headless = flat(&guide("headless"));
    for needle in [
        "**A batched ask is one row, one id and one `--answer`.**",
        "there is no per-question flag",
        "**One limitation to know before you script against it.**",
    ] {
        assert!(
            headless.contains(needle),
            "docs/guide/headless.md is missing {needle:?}; an operator scripting \
             against a parked batch has no other page to learn it from",
        );
    }
}

/// **The two management verbs 0.33.0 repaired say what they now accept.**
///
/// `io skill add <dir>/SKILL.md` installed a file that neither lever could touch
/// again, and `io plugin add` printed a removal line the removal verb could not
/// read. Both are fixes to a *sentence the product itself printed*, so a fix with
/// no documentation leaves the operator with the old sentence and a verb that has
/// quietly changed under it.
///
/// Sabotage: delete either page's paragraph. Each fails on its own.
#[test]
fn the_guide_says_a_skill_installs_under_its_own_name_and_a_bundle_removes_by_name() {
    // Each needle names the page that owns the claim. The first two are the shape
    // of the repair — the installed file is named from the skill's own name, and a
    // folder skill is manageable — and the rest are the second reading `plugin
    // remove` grew, on both the panel page and the argv page.
    for (page, needle) in [
        (
            "skills",
            "**`io skill add ./my-skill/SKILL.md` works, and until 0.33.0 it did not.**",
        ),
        (
            "skills",
            "**A skill `/import` wrote as a folder is manageable too, and it never was.**",
        ),
        (
            "plugins",
            "**From a shell it is `io plugin remove`, and it takes a directory or a name.**",
        ),
        (
            "plugins",
            "**Two bundles of one name are refused, with both directories named.**",
        ),
        (
            "headless",
            "**`io plugin remove` takes a directory or a bundle's name.**",
        ),
    ] {
        assert!(
            flat(&guide(page)).contains(needle),
            "docs/guide/{page}.md is missing {needle:?}. Both verbs were repaired \
             in 0.33.0 and both were repairs to a sentence the product itself \
             printed, so an undocumented fix leaves the operator with the old \
             sentence over a verb that has quietly changed",
        );
    }
}

/// Prose with its line breaks taken out, so an assertion is about the sentence
/// rather than about where the paragraph happened to wrap.
///
/// This also normalises the CRLF working copy git hands Windows, which is how
/// 0.33.0 shipped a docs gate that read a whole document as one paragraph on one
/// platform and nowhere else.
fn unwrapped_prose(text: &str) -> String {
    text.split_whitespace().collect::<Vec<_>>().join(" ")
}

/// **0.34.1 F6 — the exit-`4` documentation names the pause `io resume` cannot take.**
///
/// The sentence said exit `4` names the id "that `io resume` needs", full stop.
/// That is true of three of the four pauses. An approval names no invocation,
/// because io-harness publishes no resume entry point that takes one — so a
/// script written from the old sentence went looking for a command that does not
/// exist, in the one case where a person is already waiting.
///
/// Bound to the code and not to itself: [`io_cli::exec::parked`] is asked what it
/// really prints for each pause, so the page cannot be made true by editing the
/// page. Sabotage: drop the carve-out clause from either surface, or teach
/// `parked` to offer an `io resume` for an approval. Each fails this.
#[test]
fn f6_the_exit_four_documentation_names_the_approval_carve_out() {
    let approval = io_cli::exec::parked(
        &io_harness::RunOutcome::AwaitingApproval {
            request_id: 7,
            steps: 2,
        },
        41,
    )
    .expect("an approval is a pause, and `parked` names every pause");

    // The premise the documentation rests on. If this stops holding, the
    // carve-out below is stale prose and should fail here rather than quietly
    // keep describing behaviour that changed.
    //
    // The needle is the *invocation*, not the words `io resume` — this line
    // names `io resume` in order to say it is not the answer, which is the whole
    // point of it and the trap a `!contains("io resume")` walks into.
    assert!(
        !approval.contains("io resume 41"),
        "`parked` now offers an `io resume` invocation for an approval, so the \
         carve-out written into the documentation is out of date: {approval}",
    );
    assert!(
        approval.contains("not by `io resume`"),
        "the approval pause stopped saying what does not answer it, so an \
         operator is left to infer it from the absence of a command: {approval}",
    );

    for (pause, id) in [
        (
            io_harness::RunOutcome::AwaitingAnswer {
                question_id: 7,
                steps: 2,
            },
            "question",
        ),
        (
            io_harness::RunOutcome::AwaitingPlan {
                plan_id: 7,
                steps: 2,
            },
            "plan",
        ),
        (
            io_harness::RunOutcome::AwaitingRecovery {
                attempt_id: 7,
                steps: 2,
            },
            "call",
        ),
    ] {
        let line = io_cli::exec::parked(&pause, 41).expect("a pause names itself");
        assert!(
            line.contains("io resume 41"),
            "the {id} pause is one of the three that DO name an invocation, and it \
             stopped naming one — the documentation now over-carves rather than \
             over-claims: {line}",
        );
    }

    let contract = unwrapped_prose(&read("docs/CONTRACT.md"));
    for said in [
        // The table's own cell, so a reader who only skims the table is not told
        // that every pause resumes.
        "resumable with `io resume`, except an approval",
        "for three of the four pauses",
        "An approval is the fourth and it names no invocation",
    ] {
        assert!(
            contract.contains(said),
            "`docs/CONTRACT.md` no longer says {said:?}, so the exit-`4` contract is \
             back to promising an invocation for a pause that has none",
        );
    }

    assert!(
        unwrapped_prose(&guide("headless")).contains("for three of the four"),
        "the headless guide's exit-`4` claim is unqualified again",
    );
}

/// **0.34.1 F6, the second sentence — the headless guide carves out the `net` gate.**
///
/// "Every approval in a headless run is declined, and the refusal is fed back to
/// the agent as an observation it can adapt to" was true of every approval a tool
/// call raises and false of one: the provider's own endpoint is authorized once,
/// before the run's first step, so an `ask` verdict there ends the run with no
/// turn to tell about it and nothing to adapt.
///
/// The non-vacuity is the second half of the guide's claim — that nothing warns
/// beforehand — asserted against [`io_cli::exec::asks_nobody_can_answer`], which
/// reads `write` and `exec` and never `net`. A page can be edited; that function
/// cannot be edited by editing the page.
///
/// **0.35.0 deletes the exit-code half of this test rather than adjusting it.**
/// It pinned two paragraphs — one here, one on `docs/CONTRACT.md` — saying such a
/// run exits `1`, which is the known defect 0.34.1 documented instead of fixing.
/// 0.35.0 fixes it, so both paragraphs are gone and the assertions that held them
/// fail by design. What replaces them is a count over both headless doors in
/// `tests/exec.rs`, which reads the code out of the seam that chooses it rather
/// than out of a page. The carve-out itself is untouched and still asserted here:
/// the provider endpoint is still authorized before the first step, and there is
/// still nothing to adapt.
#[test]
fn f6_the_headless_guide_carves_out_the_provider_endpoint() {
    // The notice an asking posture earns. `net` is deliberately not one of the
    // tiers it inspects, which is exactly why the carve-out needs writing down.
    //
    // Every tier is set here rather than only `net`: io-harness's own default
    // policy already asks about writes and commands, so a policy built by
    // changing one field would earn the notice for the two tiers this test is
    // not about and prove nothing.
    let mut asking_net = io_harness::Policy::default();
    asking_net.defaults.write = io_harness::Effect::Deny;
    asking_net.defaults.exec = io_harness::Effect::Deny;
    asking_net.defaults.net = io_harness::Effect::Ask;
    assert!(
        io_cli::exec::asks_nobody_can_answer(&asking_net).is_none(),
        "`asks_nobody_can_answer` now warns about a `net` tier that asks, so the \
         guide's \"nothing warns first\" is no longer true and the paragraph needs \
         rewriting rather than re-asserting",
    );

    let guide = unwrapped_prose(&guide("headless"));
    for said in [
        // The scope of the general claim, narrowed to what is actually true.
        "Every approval a *tool call* raises is declined",
        "The provider's own endpoint is the exception, and it is not adaptable.",
        // The one configuration that reaches it, so the carve-out is actionable
        // rather than a warning about nothing.
        r#"`act = "net", effect = "ask"`"#,
    ] {
        assert!(
            guide.contains(said),
            "`docs/guide/headless.md` no longer says {said:?}, so the headless \
             refusal claim is back to covering a case it does not cover",
        );
    }
}

/// **N3 — no shipped page says withholding a tool saves anything.**
///
/// **This is the release's top risk written as a gate, and it is written because
/// the risk is a sentence rather than a branch.** io-harness offers a masked turn a
/// byte-identical catalogue on purpose: the tool array sits ahead of the provider's
/// cache breakpoint, so dropping a definition would save its tokens once and pay a
/// cache *write* on every later turn (`io-harness-0.78.0/src/tools/mod.rs:33-42`).
/// Withholding in fact makes the request marginally **larger**, by one sentence
/// naming what is withheld (`src/run/prompts.rs:1133`).
///
/// The roadmap entry 0.37.0 was planned from assumed the opposite and said so in
/// its headline. That framing is what a writer reaches for, because "withhold" means
/// "remove" everywhere else in computing — so the wrong sentence is the *natural*
/// one to write and nothing else in the suite can see it. A page claiming a saving
/// compiles, passes every other gate, and misleads the operator into using the
/// wrong lever for the problem they have.
///
/// Scoped to sentences that put a saving verb near the masking vocabulary rather
/// than to the verbs alone: `docs/guide/accounting.md` legitimately discusses cost
/// and cheapness throughout, and a blanket ban on the word "cheaper" would forbid
/// the page that exists to talk about money from doing so.
///
/// Sabotage: write "withholding a tool makes the turn cheaper" into any guide page.
/// Every other test in the repository stays green.
#[test]
fn n3_no_shipped_page_claims_a_mask_reduces_what_a_turn_costs() {
    // The words that make a sentence about masking into a claim about cost.
    const SAVING: [&str; 8] = [
        "saves",
        "saving",
        "cheaper",
        "reduces",
        "reduce",
        "shrink",
        "smaller",
        "less context",
    ];
    // The vocabulary that makes a sentence be about masking at all.
    const MASKING: [&str; 4] = ["withhold", "withheld", "tool mask", "/context allow"];

    let mut bad = Vec::new();
    for (name, text) in shipped_prose() {
        for sentence in flat(&text).split(['.', '!', '?']) {
            let lower = sentence.to_lowercase();
            if !MASKING.iter().any(|m| lower.contains(m)) {
                continue;
            }
            // A sentence may say a mask does NOT save — that is the correction
            // this release exists to make, and forbidding it would forbid the fix.
            let denied = lower.contains("not")
                || lower.contains("never")
                || lower.contains("nothing")
                || lower.contains("no cache")
                || lower.contains("does not");
            if denied {
                continue;
            }
            if let Some(word) = SAVING.iter().find(|w| lower.contains(**w)) {
                bad.push(format!("{name}: {word:?} in {:?}", sentence.trim()));
            }
        }
    }
    assert!(
        bad.is_empty(),
        "a shipped page claims withholding a tool reduces what a turn costs. It does \
         not: the catalogue sent is byte-identical and the mask ADDS a sentence. \
         This is the one claim 0.37.0 exists to get right, and it is the one the \
         roadmap entry got wrong.\n{}",
        bad.join("\n"),
    );
}

// ---------------------------------------------------------------------------
// 0.38.1 F10 — the front page and the contract page reach the five releases of
// work that happened after them.
//
// The README was 240 lines that predated 0.34.0. ACP — two releases of work and
// this crate's whole answer to an editor — was named once, in a row of the guides
// table. The tool mask, the schema contract and `RunOutcome::SchemaUnsatisfied`
// were named zero times, and the last of those is an exit a scripted caller
// branches on. Nothing failed, because nothing asked.
//
// These four gates are the asking. Each one is bound to something the page cannot
// edit: a constant, a parse, an outcome's own mapping, or a file on disk. A page
// that stops being true has to break one of them.
// ---------------------------------------------------------------------------

/// **F10 — both front pages name the editor door, and the door is still there.**
///
/// `io acp` is 0.36.0's adapter and 0.38.0's permission round trip, and until this
/// release an operator arriving at the repository could learn of it only from a
/// six-word cell in the guides table. Zed and a JetBrains IDE are the two clients
/// the protocol work was written against, so naming the protocol without naming
/// them leaves a reader unable to tell whether their editor is one of them.
///
/// The transport claim is the one most able to go quietly wrong: ACP is
/// **newline-delimited** JSON-RPC 2.0 and not LSP's `Content-Length` framing, and a
/// writer who has met LSP first will write the other one. Both pages say the
/// newline form and neither offers the header form.
///
/// Bound to the code twice. [`io_cli::acp::PROTOCOL_VERSION`] is the version the
/// server answers with, so a v2 that moved past these pages fails here; and
/// `src/acp.rs` really does raise `session/request_permission`, which is the
/// sentence 0.38.0 earned and the one a page could otherwise claim on its own.
///
/// Sabotage: delete any needle from either page; bump `PROTOCOL_VERSION`; or take
/// the permission request out of `src/acp.rs`. Each fails on its own.
#[test]
fn f10_the_front_page_and_the_contract_name_the_editor_door() {
    // The permission round trip exists in the code the prose is describing —
    // `f2_the_install_discloses_before_it_writes_and_says_so`'s shape, for its
    // reason: the prose half alone would pass if the feature were ripped out.
    let acp = std::fs::read_to_string(repo().join("src/acp.rs")).expect("src/acp.rs exists");
    assert!(
        acp.contains("\"session/request_permission\""),
        "both pages say an editor session is asked for permission, and nothing in \
         src/acp.rs sends the request that asks",
    );
    assert_eq!(
        io_cli::acp::PROTOCOL_VERSION,
        1,
        "the pages describe the v1 protocol; the server now answers a different \
         version and both are describing a wire this build does not speak",
    );

    let readme = flat(&read("README.md"));
    for needle in [
        // The protocol, its framing, and the two clients — in one cell, because a
        // reader deciding whether this product reaches their editor is asking one
        // question.
        "`io acp` serves the Agent Client Protocol (ACP) as newline-delimited JSON-RPC 2.0 over stdio",
        "Zed or a JetBrains IDE",
        // 0.38.0's half. Without it the row describes 0.36.0.
        "arrives as a `session/request_permission` you answer in the editor",
    ] {
        assert!(
            readme.contains(needle),
            "README.md is missing {needle:?}. ACP is this crate's answer to an \
             editor and two releases of work; a front page that names it once in a \
             table row has not reached it",
        );
    }

    let contract = flat(&read("docs/CONTRACT.md"));
    for needle in [
        "Serves the Agent Client Protocol (ACP) on stdin and stdout",
        "an ACP client such as Zed or a JetBrains IDE spawns it",
        // Already on the page and asserted here so it cannot leave with the rest:
        // the framing is the claim a rewrite is most likely to get wrong.
        "speaks newline-delimited JSON-RPC 2.0 at its stdin",
        "session/request_permission",
    ] {
        assert!(
            contract.contains(needle),
            "docs/CONTRACT.md is missing {needle:?}; `io acp` is a subcommand a \
             script and an editor both address and this is the page that enumerates \
             what may be depended on",
        );
    }

    // And the framing nobody may claim: ACP is newline-delimited, and a page that
    // said `Content-Length` would be describing LSP.
    for (name, text) in shipped_prose() {
        assert!(
            !text.contains("Content-Length"),
            "{name} describes the ACP transport with LSP's header framing; ACP \
             delimits frames with newlines and forbids an interior one",
        );
    }
}

/// **F10 — both front pages name the tool mask, in the direction that is true.**
///
/// 0.37.0 built `/context withhold` and neither front page mentioned it. The gate
/// that already exists — `n3_no_shipped_page_claims_a_mask_reduces_what_a_turn_costs`
/// — forbids the wrong sentence everywhere, and a page that says nothing satisfies
/// it perfectly. This is the positive half: the pages have to carry the claim, and
/// carrying it is what puts them under that sweep at all.
///
/// Bound to [`io_cli::context::withheld_line`], which is the sentence the product
/// itself prints. A release that changed the mask's cost story would change that
/// function, and the pages would then be describing something else.
///
/// Sabotage: delete either page's mask sentence, or teach `withheld_line` to
/// promise a saving. Each fails on its own.
#[test]
fn f10_both_pages_name_the_tool_mask_and_say_what_it_does_not_do() {
    // What the product says when a tool is withheld. The needle is the *direction*
    // — the catalogue is unchanged — rather than the whole sentence, because the
    // sentence is prose and gets reworded.
    let mask = io_harness::ToolMask::withholding(["docx_write"]);
    let line = io_cli::context::withheld_line(&mask, "—")
        .expect("a non-empty mask earns the line that explains it");
    assert!(
        line.contains("The catalogue above is unchanged"),
        "`withheld_line` stopped saying the catalogue is unchanged, so the pages \
         below are describing a cost story the product no longer tells: {line}",
    );

    let readme = flat(&read("README.md"));
    for needle in [
        "`/context withhold <tool>` builds the session's tool mask",
        "keeps that tool refused until `/context allow`",
        // The honest half, which is the whole reason this row is hard to write.
        "io-harness sends a byte-identical catalogue either way, so the request \
         grows by the one sentence naming what is withheld",
    ] {
        assert!(
            readme.contains(needle),
            "README.md is missing {needle:?}. The mask is 0.37.0's capability and \
             the front page named it zero times; a row that named it without the \
             second half would be the exact sentence N3 exists to forbid",
        );
    }

    let contract = flat(&read("docs/CONTRACT.md"));
    for needle in [
        "**The mask changes what the agent may call and not what a turn costs.**",
        "io-harness sends the same catalogue either way and io adds one sentence \
         naming what is withheld, so a masked request is the longer of the two",
        // The portability rule, which is why a misspelling is kept rather than
        // refused — a script author reading this page is the person most likely to
        // write a name against the wrong build.
        "because io-harness keeps an unknown name deliberately so a mask stays \
         portable between builds",
    ] {
        assert!(
            contract.contains(needle),
            "docs/CONTRACT.md is missing {needle:?}; the mask is the one mid-turn \
             command that changes something, and this page already says so without \
             saying what it changes",
        );
    }
}

/// **F10 — both front pages name the schema contract and the outcome it produces.**
///
/// `RunOutcome::SchemaUnsatisfied` is io-harness 0.77.0's, undeclared in its own
/// changelog, and `RunOutcome` is `#[non_exhaustive]` — so it arrived here silently
/// and `src/exec.rs` maps it to `6`. Every documented account of `6` said "a
/// verification gate", which is one of the two failures that reach it. A CI job
/// branching on `6` and reading the gate story goes looking for a gate it never
/// configured.
///
/// The three claims are asked of the code, not of a second copy of the list: the
/// mapping from [`io_cli::exec::code`], the stderr wording from
/// [`io_cli::exec::describe`], and the fact that a gate failure lands on the same
/// number — which is what makes "read stderr to tell them apart" the honest advice
/// rather than a hedge.
///
/// Sabotage: remap `SchemaUnsatisfied`, reword `describe`, or drop either page's
/// paragraph. Each fails on its own.
#[test]
fn f10_both_pages_name_the_schema_contract_and_the_outcome_that_reaches_six() {
    let schema = io_harness::RunOutcome::SchemaUnsatisfied { steps: 3 };
    let gate = io_harness::RunOutcome::VerificationFailed { steps: 3 };

    assert_eq!(
        io_cli::exec::code(&schema),
        io_cli::exec::UNVERIFIED,
        "an unsatisfied output schema no longer exits `6`, so both pages now \
         document a code a caller will not see for it",
    );
    assert_eq!(
        io_cli::exec::code(&gate),
        io_cli::exec::UNVERIFIED,
        "a failed verification gate no longer exits `6`, so the pages' \"two \
         different failures reach it\" is one failure",
    );

    // The line that separates them, which is what both pages tell a caller to
    // read. Quoted on the contract page verbatim, so the advice is followable.
    let said = io_cli::exec::describe(&schema);
    const STDERR: &str = "never produced the shape its output schema asked for";
    assert!(
        said.contains(STDERR),
        "`describe` stopped naming the schema failure in the words the contract \
         page quotes, so a caller grepping stderr for it finds nothing: {said}",
    );

    let readme = flat(&read("README.md"));
    for needle in [
        "io-harness's `RunOutcome::SchemaUnsatisfied`",
        "a run that never produced the shape `[run] output_schema` asked for",
        "a verification gate that failed",
    ] {
        assert!(
            readme.contains(needle),
            "README.md is missing {needle:?}. Exit `6` is a number a scripted \
             caller branches on and the front page described only one of the two \
             ways to reach it",
        );
    }

    let contract = flat(&read("docs/CONTRACT.md"));
    for needle in [
        "**Two different failures reach `6`, and a caller that has to tell them \
         apart reads stderr rather than the code.**",
        "**The schema contract is io-harness's `[run] output_schema`**",
        "a run that never produces the shape it asks for is `RunOutcome::SchemaUnsatisfied`",
        // The table cell itself, so a reader who only skims the table is not told
        // that `6` means a gate and nothing else.
        "| `6` | UNVERIFIED | The work was judged and did not hold up",
    ] {
        assert!(
            contract.contains(needle),
            "docs/CONTRACT.md is missing {needle:?}; this is the page a script \
             author reads and `6` had one of its two meanings written down",
        );
    }
    assert!(
        contract.contains(STDERR),
        "docs/CONTRACT.md tells a caller to read stderr to tell the two `6`s \
         apart, so it has to quote the line they will be reading",
    );
}

/// **F10 — the tap and the marketplace appear with the command that uses them.**
///
/// Both were mentions rather than paths. The tap and the bucket live in *this*
/// repository, which is why `brew tap` and `scoop bucket add` have to name a URL —
/// the repository is not `homebrew-io-cli`, so neither tool can derive one — and a
/// reader who copies a bare `brew install io` gets somebody else's formula. The
/// marketplace had its name on four pages and its verb on none of the two a reader
/// arrives at.
///
/// Bound to disk and to the parse. The two files are asserted to exist, because a
/// tap whose formula is gone installs nothing; the upgrade commands are taken from
/// [`io_cli::upgrade`]'s own constants, so a renamed formula fails here rather than
/// leaving the README printing a command that updates nothing; and the marketplace
/// verb is fed through [`io_cli::manage::parse`], which is the same parse both the
/// slash surface and the shell reach.
///
/// Sabotage: delete `Formula/io.rb`; change either upgrade constant; drop the
/// explicit-URL form from the install section; or remove the marketplace verb from
/// either page. Each fails on its own.
#[test]
fn f10_the_tap_and_the_marketplace_appear_with_the_command_that_uses_them() {
    for path in ["Formula/io.rb", "bucket/io.json"] {
        assert!(
            repo().join(path).is_file(),
            "{path} is what makes this repository its own tap, and the README \
             links it",
        );
    }

    let readme = flat(&read("README.md"));
    for needle in [
        // The explicit-URL form, both halves. `brew install io` alone resolves to
        // core, which is not this product.
        "brew tap initorigin/io-cli https://github.com/initorigin/io-cli",
        "brew install initorigin/io-cli/io",
        "scoop bucket add io-cli https://github.com/initorigin/io-cli",
        "scoop install io",
        // Why the URL is there, which is the part a reader otherwise reads as
        // ceremony and drops.
        "this repository is not named `homebrew-io-cli`",
        // And the two files, linked rather than described.
        "[`Formula/io.rb`](Formula/io.rb)",
        "[`bucket/io.json`](bucket/io.json)",
    ] {
        assert!(
            readme.contains(needle),
            "README.md is missing {needle:?}; the tap is a path with a command, \
             and a reader who cannot copy it has been told about it rather than \
             given it",
        );
    }

    // The upgrade lines, from the constants the binary prints. A formula renamed
    // without these moving would leave both pages naming a command that updates
    // nothing.
    let contract = flat(&read("docs/CONTRACT.md"));
    for command in [io_cli::upgrade::HOMEBREW, io_cli::upgrade::SCOOP] {
        assert!(
            readme.contains(&format!("`{command}`")),
            "README.md should quote `{command}`, which is what `io upgrade` prints \
             for that install",
        );
        assert!(
            contract.contains(&format!("`{command}`")),
            "docs/CONTRACT.md should quote `{command}`; `io upgrade` prints and \
             does not run, so what it prints is the whole of its contract",
        );
    }

    // The marketplace verb reaches the parse both doors share, so the command the
    // pages print is a command that is actually routed.
    for line in [
        "plugin marketplace add initorigin/io-cli",
        "plugin marketplace list",
        "plugin marketplace remove initorigin/io-cli",
        "plugin add ultraship",
    ] {
        assert!(
            io_cli::manage::parse(&io_cli::manage::tokens(line)).is_ok(),
            "both pages print `io {line}` and the parse both doors share refuses it",
        );
    }

    for needle in [
        "`/plugin marketplace add <owner>/<repo>` clones an index into your own home",
        "`/plugin add <name>` installs a bundle out of it",
        "as `io plugin marketplace add` and `io plugin add`",
    ] {
        assert!(
            readme.contains(needle),
            "README.md is missing {needle:?}. A marketplace was named on the front \
             page without the verb that adds one, which leaves a reader knowing the \
             word and not the command",
        );
    }
    for needle in [
        "`io plugin marketplace add <owner>/<repo>` clones an index",
        "`io plugin add <name>` installs a bundle out of one",
    ] {
        assert!(
            contract.contains(needle),
            "docs/CONTRACT.md is missing {needle:?}; the argv surface is what this \
             page exists to enumerate and `io plugin …` was an ellipsis",
        );
    }
}

// ---------------------------------------------------------------------------
// 0.38.1 F9 — a help page describes the binary in front of the reader, and an
// answer that is not an answer is not one.
// ---------------------------------------------------------------------------

/// Every `--help` page the binary can print, by the command that prints it.
///
/// Rendered rather than assembled out of `get_about` and `get_help`, because the
/// claim under test is about what an operator *sees*: a global flag's help is
/// reprinted on every subcommand's page, and a sweep that read each string once
/// would check a paragraph in one place while it shipped in nine.
fn help_pages() -> Vec<(String, String)> {
    use clap::CommandFactory;

    let mut cli = io_cli::cli::Cli::command();
    let names: Vec<String> = cli
        .get_subcommands()
        .map(|sub| sub.get_name().to_string())
        .collect();

    let mut pages = vec![("io".to_string(), cli.render_long_help().to_string())];
    for name in names {
        let page = cli
            .find_subcommand_mut(&name)
            .expect("a subcommand clap has just named")
            .render_long_help()
            .to_string();
        pages.push((format!("io {name}"), page));
    }
    pages
}

/// Every `<n>.<n>.<n>` literal in `text`.
///
/// Three components and not two, which is the difference between a version and a
/// protocol: `io acp --help` says "JSON-RPC 2.0" and must go on saying it.
fn versions_named(text: &str) -> Vec<String> {
    text.split(|c: char| !(c.is_ascii_digit() || c == '.'))
        .map(|token| token.trim_matches('.'))
        .filter(|token| {
            let parts: Vec<&str> = token.split('.').collect();
            parts.len() == 3 && parts.iter().all(|part| !part.is_empty())
        })
        .map(str::to_string)
        .collect()
}

/// **No `--help` page dates itself against a version this binary is not, and none
/// of them tells the reader a story about a release.**
///
/// A `--help` page describes the binary in front of the reader. Every version
/// number on one is therefore either the pin — which the reader can act on — or a
/// claim about a build they are not running, which at best costs them the time to
/// work out that it does not apply. The field test of 0.38.0 found three: the
/// harness release that added `--profile`, the io-cli release that first selected
/// one, and the release whose `-m` defect is why `--plain` is a global flag.
///
/// The pin is read from `Cargo.lock` through [`pinned_harness`] and never typed
/// here. A gate carrying the number goes stale on the next pin, which is the exact
/// defect class this one exists to remove.
///
/// Sabotage: put back either sentence `src/cli.rs` used to carry on `--profile` or
/// on `--plain`. The failure names the page and the number.
#[test]
fn f9_no_shipped_help_names_an_unpinned_harness_or_narrates_a_release() {
    // Narration that carries no number at all. The version sweep below is the wide
    // net; this is for the sentence that says a release happened without saying
    // which, which the sweep cannot see.
    const NARRATION: &[&str] = &[
        "shipped a defect",
        "did not exist because",
        "was missing until",
        "was green over",
        "no io-cli release",
        "in an earlier release",
        "until this release",
    ];

    let pinned = pinned_harness();
    let pages = help_pages();
    assert!(
        pages.len() > 5,
        "only {} help pages were rendered, so this sweep is reading almost \
         nothing and is not checking what it claims",
        pages.len(),
    );

    for (command, page) in &pages {
        for version in versions_named(page) {
            assert_eq!(
                version, pinned,
                "`{command} --help` names {version}, which is neither the pinned \
                 io-harness ({pinned}) nor anything the operator holding this \
                 binary can act on; a version on a help page is a claim about \
                 what is in front of the reader",
            );
        }
        // Whitespace flattened first, because clap wraps a help page to the
        // terminal's width: a phrase that fell across a line break would be
        // missed, and the gate would be quietly reading a different string than
        // the one an operator sees.
        let said = page
            .split_whitespace()
            .collect::<Vec<_>>()
            .join(" ")
            .to_lowercase();
        for phrase in NARRATION {
            assert!(
                !said.contains(phrase),
                "`{command} --help` says {phrase:?}, which is a changelog entry \
                 rather than a description of this binary; `CHANGELOG.md` and git \
                 hold the history",
            );
        }
    }
}

/// **No subcommand's help example names a subcommand other than its own.**
///
/// One `Args` type is shared by `io mcp`, `io plugin`, `io config` and `io skill`,
/// so an example written into its field is an example of one of them printed on
/// the help page of all four — which is what the 0.38.0 field test found, with an
/// `mcp add` line under `io config --help`. The example belongs on the variant,
/// where clap prints it once, on its own page.
///
/// The distinctness assertion is the one that would have caught the original: four
/// identical paragraphs is precisely the shape of a copy-paste, and every other
/// check here passes on it as long as the one string names its own subcommand.
///
/// Sabotage: move any one example back onto the `words` field of `Manage`, or copy
/// `io mcp`'s paragraph onto `io config`. The first empties the list and fails on
/// the four names below; the second fails on distinctness and on the name.
#[test]
fn f9_no_subcommands_help_example_names_another_subcommand() {
    use clap::CommandFactory;

    // The marker the examples are written behind. A convention rather than a
    // grammar, and it is what makes this gate about *examples* — `io acp --help`
    // mentions `io exec` in a sentence contrasting the two, which is a
    // cross-reference an operator wants and not an example of the wrong command.
    const MARKER: &str = "For example:";

    let cli = io_cli::cli::Cli::command();
    let names: Vec<String> = cli
        .get_subcommands()
        .map(|sub| sub.get_name().to_string())
        .collect();

    let mut examples: Vec<(String, String)> = Vec::new();
    for sub in cli.get_subcommands() {
        let name = sub.get_name().to_string();
        let about = sub
            .get_long_about()
            .or_else(|| sub.get_about())
            .map(|about| about.to_string())
            .unwrap_or_default();
        // Flattened per paragraph, for the reason the sweep above flattens a page:
        // where the source broke its lines is not a fact about the help text, and a
        // needle that fell across a break would be missed.
        let paragraphs: Vec<String> = about
            .split("\n\n")
            .map(|part| part.split_whitespace().collect::<Vec<_>>().join(" "))
            .collect();
        for paragraph in paragraphs.iter().filter(|part| part.contains(MARKER)) {
            for other in &names {
                if *other == name {
                    continue;
                }
                assert!(
                    !paragraph.contains(&format!("io {other} ")),
                    "`io {name} --help` gives an example of `io {other}`, so three \
                     readers out of four are being shown a command they did not \
                     ask about:\n{paragraph}",
                );
            }
            assert!(
                paragraph.contains(&format!("io {name} ")),
                "`io {name} --help` carries an example that never types \
                 `io {name}`:\n{paragraph}",
            );
            examples.push((name.clone(), paragraph.clone()));
        }
    }

    // The four that share one `Args` type, by name, because a gate made of
    // negatives passes on a binary that shows no examples at all.
    for wanted in ["mcp", "plugin", "config", "skill"] {
        assert!(
            examples.iter().any(|(name, _)| name == wanted),
            "`io {wanted} --help` shows no example, and it is one of the four \
             subcommands that share a single argument type — the place an example \
             cannot be written once for all of them",
        );
    }

    for (i, (name, paragraph)) in examples.iter().enumerate() {
        for (other, another) in &examples[i + 1..] {
            assert_ne!(
                paragraph, another,
                "`io {name}` and `io {other}` show the same example word for word, \
                 which is one subcommand's example on two pages",
            );
        }
    }
}

/// **`io config get` on a key nothing names says there is no such key.**
///
/// `Config::origin` returns an empty slice both for a real key no file sets and
/// for a misspelling, so through 0.38.0 both answered `default` — and an operator
/// checking a typo was told io-harness's own default was in force, which is a
/// wrong answer rather than a thin one.
///
/// The two assertions that must keep holding are the point of the other half: a
/// catalogue key no file sets is still a default, and no row of the listing reads
/// as unknown. A key a *file* sets which the catalogue does not name is a real key
/// the operator wrote, `configure::settings` lists it deliberately, and it is
/// unreachable here by construction — `Decided::Unknown` is only chosen when
/// `origin` came back empty.
///
/// Sabotage: drop the `CATALOGUE` test from `configure::setting` so every unset key
/// is `Unknown`. The listing assertion fails on the first catalogue row.
#[test]
fn f9_config_get_on_a_key_nothing_names_says_there_is_no_such_key() {
    // No files at all, which is the fixture this needs: every key is one no file
    // sets, so the catalogue is the only thing left deciding the answer.
    let config = io_harness::config::Config::default();

    let missing = io_cli::configure::setting(&config, "nonexistent.key");
    assert_eq!(
        missing.decided.word(),
        "no such key",
        "`io config get nonexistent.key` prints this word as its origin field, and \
         `default` there claims a setting exists",
    );
    assert_eq!(
        missing.value, None,
        "there is nothing to quote for a key no file names",
    );
    assert!(
        io_cli::configure::said(&missing).contains("no such key"),
        "the session's own `/config <key>` says: {}",
        io_cli::configure::said(&missing),
    );

    let unset = io_cli::configure::setting(&config, "run.max_retries");
    assert_eq!(
        unset.decided.word(),
        "default",
        "a catalogue key no file sets is a real key running on io-harness's own \
         default, and calling it unknown would be a second wrong answer",
    );

    for row in io_cli::configure::settings(&config) {
        assert_ne!(
            row.decided.word(),
            "no such key",
            "the listing offers {} and then denies it exists",
            row.path,
        );
    }
}

/// **`io plugin search` with no match says nothing matched.**
///
/// A search that prints nothing is indistinguishable from a search that did not
/// run, and the two want different next moves — "no bundle is called that" against
/// "this command is broken". The argv door printed nothing at all through 0.38.0;
/// the session door said so in a `format!` of its own, which is the second
/// implementation this crate keeps finding disagreeing later.
///
/// Both call sites are asserted, and that is the half that makes this a gate rather
/// than a sentence: `marketplace::nothing_matched` existing proves nothing about
/// what either door prints.
///
/// Sabotage: delete either call in `src/main.rs`, or re-spell one of them as a
/// literal.
#[test]
fn f9_plugin_search_with_no_match_says_nothing_matched() {
    let said = io_cli::marketplace::nothing_matched("nothing-is-called-this");
    assert!(
        said.contains("nothing-is-called-this"),
        "the answer does not repeat the term, so a scrolled terminal cannot say \
         which search it answers: {said}",
    );
    assert!(
        said.contains("marketplace"),
        "the answer should say where it looked: {said}",
    );

    let main = read("src/main.rs");
    let calls = main.matches("marketplace::nothing_matched").count();
    assert!(
        calls >= 2,
        "only {calls} of the two doors say it. `io plugin search <term>` and the \
         session's `/plugin search <term>` both answer an empty search, and both \
         call `marketplace::nothing_matched` — a door that prints nothing leaves \
         the operator unable to tell an empty result from a broken command",
    );
    assert!(
        !main.contains("no bundle in any marketplace matches"),
        "src/main.rs spells the sentence itself instead of calling \
         `marketplace::nothing_matched`, which is two answers to one question \
         agreeing only until one of them is edited",
    );
}
