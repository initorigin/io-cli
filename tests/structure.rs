//! F5 — no alternate screen and no mouse capture, ever.
//! N3 — no full-screen clear during a session.
//!
//! Both are assertions over the bytes io-cli writes to the terminal, and both are
//! written so that a later release cannot reintroduce fullscreen or a clear-based
//! redraw without turning a named test red.
//!
//! O1, F14 and F17 — and three assertions that are not about bytes at all: the
//! order of two calls in `src/main.rs`, the writes reachable from a key handler,
//! and the third argument two doors hand `manage::plan`. Nothing under `tests/`
//! links the binary, so a decision made in the driver is one no test can drive;
//! this file reads the driver instead.

mod support;

use ratatui::text::Line;
use support::FORBIDDEN;

/// A session long enough to have written everything a session writes: a splash
/// into scrollback, a viewport frame, several committed messages, a resize, and
/// more frames after it.
fn scripted_session(width: u16, height: u16) -> support::Recorder {
    let (mut screen, recorder) = support::screen(width, height);

    screen
        .commit(&[Line::from("io-cli"), Line::from("")])
        .expect("splash");
    screen.draw(|_| {}).expect("first frame");

    for turn in 0..5 {
        screen
            .commit(&[
                Line::from(format!("> prompt {turn}")),
                Line::from(format!("assistant reply {turn}")),
                Line::from(""),
            ])
            .expect("commit");
        screen.draw(|_| {}).expect("frame");
    }

    support::resize(&mut screen, width, height + 4);
    screen.draw(|_| {}).expect("frame after resize");
    screen
        .commit(&[Line::from("after the resize")])
        .expect("commit after resize");

    drop(screen);
    recorder
}

#[test]
fn f5_never_enters_the_alternate_screen_or_captures_the_mouse() {
    let recorder = scripted_session(100, 30);
    let text = recorder.text();

    for (name, sequence) in FORBIDDEN {
        assert!(
            !text.contains(sequence),
            "the byte stream contains {name} ({}), which this product has no code path for",
            sequence.escape_debug(),
        );
    }
}

#[test]
fn f5_holds_at_eighty_columns_too() {
    // The narrow path takes a different branch through `insert_before`, which
    // loops when the content is taller than the screen. F5 has to hold there too.
    let recorder = scripted_session(80, 24);
    let text = recorder.text();

    for (name, sequence) in FORBIDDEN {
        assert!(
            !text.contains(sequence),
            "the byte stream at 80x24 contains {name} ({})",
            sequence.escape_debug(),
        );
    }
}

#[test]
fn n3_never_clears_the_whole_screen_during_a_session() {
    let recorder = scripted_session(100, 30);
    let text = recorder.text();

    // `ESC [ 2 J` erases the display, and `ESC [ 3 J` erases the scrollback —
    // which on this renderer is where the transcript lives, so it is worse than
    // the one the criterion names.
    assert!(
        !text.contains("\x1b[2J"),
        "the byte stream contains a full-screen clear",
    );
    assert!(
        !text.contains("\x1b[3J"),
        "the byte stream contains a scrollback erase, which would destroy the transcript",
    );
}

#[test]
fn every_frame_is_wrapped_in_synchronized_output() {
    let (mut screen, recorder) = support::screen(100, 30);
    // The two frames must DIFFER, and that is 0.6.0's doing rather than an
    // arbitrary choice. Since F6 a frame whose content matches the one on screen
    // is not drawn at all, so two empty frames would produce one begin and this
    // test would read the skip as a missing wrapper — a green-to-red for the
    // opposite of the reason it exists. What it is actually about is that every
    // frame io-cli *does* draw is wrapped, so the frames are given something to
    // differ by.
    for word in ["ready", "working"] {
        screen
            .draw(|frame| {
                // `frame.area()` and not a rectangle of the test's own: an inline
                // viewport sits at the bottom of the terminal, so its area has a
                // non-zero origin and anything drawn at row zero is outside the
                // buffer.
                frame.render_widget(ratatui::widgets::Paragraph::new(word), frame.area());
            })
            .expect("frame");
    }
    drop(screen);

    let text = recorder.text();
    let begins = text.matches("\x1b[?2026h").count();
    let ends = text.matches("\x1b[?2026l").count();

    assert_eq!(begins, 2, "one begin-synchronized-update per frame");
    assert_eq!(ends, begins, "every begin is closed by an end");
}

/// `src/main.rs`, with every comment taken off before anything is matched.
///
/// The stripping is the whole difference between a gate and a green light. 0.14.0
/// shipped a check that asserted the source contained `EventKind::Dialed` and was
/// satisfied by a *comment* naming the variant — a passing test over code that had
/// none of it. Prose about `adopt` is exactly as easy to write, so the prose is
/// removed and what is left is the code the compiler sees. `//` appears in no
/// string literal in this file and it has no block comments, so a line cut at the
/// first `//` is a line cut at its comment.
fn driver_without_comments() -> String {
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("src/main.rs");
    let text = std::fs::read_to_string(path).expect("the driver is readable");
    text.lines()
        .map(|line| match line.find("//") {
            Some(at) => &line[..at],
            None => line,
        })
        .collect::<Vec<_>>()
        .join("\n")
}

/// **O1.** The home is adopted before the configuration is discovered.
///
/// Presence is not the property; order is. `io_harness::config::user_path` reads
/// the environment at call time, so a `Config` discovered before `home::adopt` set
/// `IO_CONFIG_HOME` is a configuration read out of the directory the run store —
/// derived from that file's own directory — has already left. The symptom is not
/// an error: it is a session that starts fine, writes to one place, and answers
/// `/resume` from another that is empty.
///
/// It is asserted here because nothing under `tests/` links the binary, which is
/// how this repository already pins the decisions `src/main.rs` makes — see
/// `tests/contract.rs` and `tests/plan.rs`. The offsets come from the source with
/// its comments removed, so the paragraph you are reading could be pasted into the
/// driver and this test would still fail.
#[test]
fn o1_the_home_is_adopted_before_the_configuration_is_discovered() {
    let text = driver_without_comments();

    let adopt = text
        .find("io_cli::home::adopt()")
        .expect("`run` calls `io_cli::home::adopt`, in code and not in a sentence about it");
    // The first one, which is the only one either arm reaches — the wizard's
    // re-read below it happens after a file has been written.
    let discover = text
        .find("Config::discover(")
        .expect("`run` discovers the configuration");

    assert!(
        adopt < discover,
        "the home is adopted at byte {adopt} and the configuration discovered at {discover}: \
         a configuration discovered first is read from the directory the store has left",
    );
}

/// **F6, the session arm.** What the migration did is committed into the
/// scrollback rather than said on a row that repaints.
///
/// `App::say` answers a keystroke and is gone at the next one. A migration happens
/// once, on the run after an upgrade, and the operator it matters to has pressed
/// nothing yet — so said, it would be replaced by the first thing they typed and
/// never be seen again. `App::record` is the half that belongs to the conversation.
///
/// The call is matched with the loop it sits in, so the assertion is over the code
/// that runs and not over the word `record` appearing anywhere in the file.
#[test]
fn f6_the_migration_report_is_recorded_rather_than_said() {
    let text = driver_without_comments();

    assert!(
        text.contains("for line in report {\n        app.record(Tone::Muted, line);"),
        "the migration report reaches the scrollback through `App::record`; `App::say` would \
         put it on the footer's row, where the first keystroke replaces it",
    );
}

/// **F6, the last-resort arm.** What nobody said is delivered on the way out of
/// `main`, on every arm rather than on one of them.
///
/// **This is a defect that shipped, and the shape of the gate is the whole
/// lesson.** `main` holds the report precisely so no early return has to know to
/// deliver it — the comment above `let mut report` says so — and then, until
/// 0.35.0, drained it inside the `Err` arm alone. The wizard's own decline is not
/// an error: answering `io setup` with Esc returns `Ok(exec::OK)`. So an operator
/// upgrading from a pre-0.15.0 install had `io.toml`, `runs.db` and the rest moved
/// into `~/.io-cli`, pressed one key, and was told nothing, with their old
/// directory now empty. The same arm swallowed the line that says a file could
/// **not** be moved because another `io` holds it.
///
/// **Position is the property, so `contains` cannot express it.** The old code
/// contained this exact loop; it was in the wrong place. The assertion is
/// therefore an ordering — the drain stands before the `match` that splits the two
/// arms — plus a count, because a release that "fixes" this by adding a second
/// loop to the `Ok` arm has reintroduced the thing the comment warns about: the
/// next early return will not know to do it either.
///
/// Nothing under `tests/` links `src/main.rs`, so a source-text gate is the only
/// assertion available here at all. That is a limitation and it is stated rather
/// than dressed up: this proves where the code sits, not that it runs.
///
/// Sabotage: move the loop inside the `Err` arm, which is exactly the pre-0.35.0
/// source. The ordering assertion fails and nothing else in the suite does.
#[test]
fn f6_what_nobody_said_is_delivered_on_every_arm_and_not_only_the_failing_one() {
    let text = driver_without_comments();

    // The needle carries no indentation, deliberately. An earlier draft matched
    // `for line in &report {\n        eprintln!` — eight spaces — and the sabotage
    // below then failed on the `find` rather than on the ordering, because moving
    // the loop into an arm indents it to twelve. That is a gate coupled to
    // whitespace: it would have gone red on a reformat that changed nothing, and
    // it would have reported the wrong reason for a real regression.
    let drain = text.find("for line in &report {").expect(
        "`main` drains whatever the report still holds to stderr — every arm that had somewhere \
         better to speak has already taken it by `std::mem::take`, so what is left is what nobody \
         said",
    );
    assert!(
        text[drain..].starts_with("for line in &report {\n")
            && text[drain..].contains("eprintln!(\"{line}\");"),
        "and it is the stderr drain, not some other loop over the same vector",
    );

    assert_eq!(
        text.matches("for line in &report {").count(),
        1,
        "exactly one drain. A second loop added to an arm is how this defect returns: the report \
         exists so that no early return has to know to deliver it, and a per-arm drain hands that \
         obligation back to the next arm somebody writes",
    );

    let split = text
        .find("match outcome {")
        .expect("`main` matches on what `run` returned");

    assert!(
        drain < split,
        "the drain is at byte {drain} and the match that splits the arms at {split}: a drain \
         inside an arm is the pre-0.35.0 source, where `io setup` answered with Esc returned \
         `Ok` and told an operator nothing about the install that had just moved underneath them",
    );
}

/// **F14.** Neither arrow key writes a configuration file without a confirmation.
///
/// **Counted, not `contains`ed.** `configure::write` is called from two doors and
/// nineteen places, so asking whether the driver contains it answers yes forever
/// and answers nothing: the sabotage that matters adds a twentieth, on a
/// keystroke, and every existing site keeps a `contains` green through it. The
/// number is the assertion, and a release adding a write has to come here and say
/// which door it belongs to.
///
/// **Where the nineteen are, so the number is a claim and not a constant.** One in
/// `import_written`, the offer's own confirmation. Sixteen in `loop_over`: twelve
/// under the picker's single `Outcome::Chosen` arm — Enter on a row, which is what
/// a confirmation *is* in this product — and four under an `Action::` arm of a
/// typed command (`Action::Manage`, two `Action::Profile` verbs and
/// `Action::Config` with a value), where the Enter that submitted the line is the
/// consent. One in `refresh_prices`, the act `prices.as_of`'s descent offers, one
/// keystroke below the row it writes. One in `manage_main`, the headless
/// `io config|mcp|plugin` door, which has no keyboard at all. Nineteen sites, no
/// tenth surface, and none of them on a bare arrow.
///
/// **The arrow arm is read as a region, not searched for as a word**, because a
/// count alone cannot see a write moved *into* the arm from somewhere else. The
/// slice runs from the `Pick::Config` interception to the general picker arm below
/// it, which is exactly the code a `Left` or `Right` runs before every other
/// surface gets the key.
///
/// **The comment stripping is load-bearing and proves itself here.** This release
/// deliberately left the word `cycle_setting` in a historical comment in
/// `src/main.rs` — so the `cycle_setting` assertion below passes *only* while the
/// stripper strips. A stripper that stopped stripping fails this test rather than
/// quietly weakening every other assertion in this file.
///
/// Sabotage, each of which fails here and nowhere else: restore a `cycle_setting`
/// that writes on the keystroke (the count moves to twenty, and the name comes
/// back in code); call `configure::write` inside the arrow arm (the region);
/// route `refresh_prices` onto an arrow (the region, and the count does not move).
#[test]
fn f14_no_arrow_key_writes_a_configuration_file() {
    let text = driver_without_comments();

    assert!(
        !text.contains("cycle_setting"),
        "`cycle_setting` is named in the driver's code: it wrote a scope file on a bare \
         `Left`/`Right` and 0.33.0 removed it. If this fails with no such function in \
         `src/main.rs`, the comment stripper has stopped stripping and every assertion \
         in this file is weaker than it reads",
    );

    const OPENS: &str = "if let Some((open, Pick::Config(paths))) = picker.as_mut() {";
    const CLOSES: &str = "if let Some((open, kind)) = picker.as_mut() {";
    let from = text
        .find(OPENS)
        .expect("the idle loop intercepts the arrows over a `/config` row");
    let to = text[from..]
        .find(CLOSES)
        .map(|at| from + at)
        .expect("the general picker arm follows the interception");
    let arm = &text[from..to];

    // The slice is the arrow arm and not some other `if let`: both codes are
    // matched inside it, and a region that lost them is a region pointing
    // somewhere else — which would make every assertion below vacuous.
    assert!(
        arm.contains("KeyCode::Right") && arm.contains("KeyCode::Left"),
        "the sliced region does not test either arrow, so it is not the interception",
    );
    assert!(
        !arm.contains("configure::write"),
        "the arrow interception writes a configuration file. An arrow is not a \
         confirmation: it opens the values and `Enter` on one of them decides, which is \
         the shape every other managed surface in this product already has",
    );
    assert!(
        !arm.contains("refresh_prices("),
        "the arrow interception starts the price refresh, which writes `prices.as_of` — \
         a write on a bare keystroke that moves no call count, because the call site \
         already existed one descent below",
    );

    assert_eq!(
        text.matches("io_cli::configure::write(").count(),
        19,
        "the driver's configuration writes moved. Each one is a confirmed door — see \
         this test's own documentation for the nineteen and what confirms them — so a \
         new one is either a door that needs naming there or a write on a keystroke",
    );
}

/// **F17.** Both `manage::plan` doors hand it a resolved bundle list.
///
/// `plan(root, request, declared)` gained its third argument this release so
/// `plugin remove <name>` can resolve a word against what is declared instead of
/// demanding a path. **The revert is one `&[]` away and turns nothing red**: an
/// empty list is legal, is what every other verb on the headless door correctly
/// passes, and returns the verb to path-only behaviour with the whole suite green.
/// That is the widening shape — the gate does not fail, it stops being a gate.
///
/// So both spellings are pinned, and the resolve behind them is counted.
/// `pluginview::ids` is called twice in the driver and nowhere else: once in
/// `Action::Manage`, from the session's already-loaded holdings, and once in
/// `manage_main`, inside the `PluginVerb::Remove` arm that is the only headless
/// request needing it. Replacing either call with `Vec::new()` keeps the argument
/// named `declared` and fails the count; replacing either argument with `&[]`
/// fails its spelling.
///
/// Sabotage: pass `&[]` at either site, or drop either `pluginview::ids` call.
/// Comments cannot satisfy any of it — the text is read with comments stripped.
#[test]
fn f17_both_manage_doors_pass_a_resolved_bundle_list() {
    let text = driver_without_comments();

    assert_eq!(
        text.matches("io_cli::manage::plan(").count(),
        2,
        "there are two doors onto `manage::plan`, the session's `/mcp|/plugin|/config` \
         and the headless `io mcp|plugin|config`; a third is a third place the argument \
         can be got wrong",
    );
    assert!(
        text.contains("io_cli::manage::plan(&root, &request, &declared)"),
        "the session door no longer hands `plan` its resolved bundles, so \
         `/plugin remove <name>` is back to refusing everything that is not a path",
    );
    assert!(
        text.contains("io_cli::manage::plan(root, &request, &declared)"),
        "the headless door no longer hands `plan` its resolved bundles, so \
         `io plugin remove <name>` is back to refusing everything that is not a path",
    );
    assert!(
        !text.contains("&request, &[]"),
        "a door passes `plan` an empty declared list. That compiles, it is what every \
         non-`remove` request on the headless door correctly passes, and it silently \
         returns `plugin remove <name>` to path-only",
    );
    assert_eq!(
        text.matches("io_cli::pluginview::ids(").count(),
        2,
        "each door resolves the declared bundles it hands in — the session from its \
         loaded holdings, the headless door inside the `PluginVerb::Remove` arm. A \
         `declared` bound to `Vec::new()` keeps every spelling above and takes the \
         resolve away",
    );
}
