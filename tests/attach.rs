//! F1 — an image is attached through the session's own policy, and not around it.
//! F2 — a refusal is by name and by reason, before anything goes to a provider.
//! F7 — an attachment rides exactly one turn.
//!
//! The subject of F1 and F2 is `io_cli::attach::prepare`, which is a function of a
//! root, a policy and a path — never of a `Session` — so a test can state a
//! workspace by hand. F7's subject is io-harness's own staging, so it needs a real
//! turn and a provider that can say what reached it.

mod support;

use std::fs;

use io_cli::attach::prepare;
use io_cli::picture::decode;
use io_harness::{ApproveAll, Policy, Session, Store};
use tempfile::TempDir;

/// A workspace with one real PNG in it, and a second one in a subdirectory.
fn workspace() -> TempDir {
    let dir = tempfile::tempdir().expect("a temporary directory");
    fs::write(dir.path().join("shot.png"), support::png_bytes(4, 2)).expect("write");
    fs::create_dir_all(dir.path().join("docs")).expect("mkdir");
    fs::write(dir.path().join("docs/deep.png"), support::png_bytes(2, 2)).expect("write");
    fs::write(dir.path().join("notes.md"), b"not an image").expect("write");
    dir
}

#[test]
fn an_image_under_the_root_is_read_and_becomes_media() {
    let dir = workspace();
    let staged = prepare(dir.path(), &Policy::permissive(), true, "shot.png")
        .expect("a png inside the workspace");

    assert_eq!(staged.media_type, "image/png");
    assert_eq!(staged.path, "shot.png");
    assert!(staged.media.byte_len() > 0);
    // The bytes kept for the screen are the file's own, so the picture is drawn
    // without decoding the base64 the harness made for the wire.
    assert_eq!(
        decode(&staged.bytes).map(|p| (p.width(), p.height())).ok(),
        Some((4, 2)),
    );
}

#[test]
fn the_leading_at_sign_of_a_completed_path_is_not_part_of_the_path() {
    // `@` is what opens this product's path completion, so it is still on the
    // line when `/attach @docs/deep.png` is submitted.
    let dir = workspace();
    let staged = prepare(dir.path(), &Policy::permissive(), true, "@docs/deep.png")
        .expect("the marker is a completion trigger, not a path character");

    assert_eq!(staged.path, "docs/deep.png");
}

#[test]
fn a_path_outside_the_root_is_read_because_the_operator_pointed_at_it() {
    // **Superseded in 0.13.1, deliberately.** Through 0.13.0 this asserted the
    // opposite: a file outside the session root was refused, and reading it with
    // `std::fs` was named here as the sabotage arm. That rule made `/attach`
    // unusable for the only file most operators ever attach — a screenshot, which
    // macOS writes to `~/Pictures` and never to the repository — and
    // `Workspace::resolve` refuses every absolute path, so there was no spelling
    // that worked.
    //
    // What changed is whose action this is. Every other read in this product is
    // the agent's, gated by the policy, about a path a model chose. `/attach` is
    // a person pointing at their own file, which is the boundary `!` already
    // crosses when it runs the operator's own shell line unpoliced. The gate
    // still governs everything inside the root — the test below this one is that
    // claim, and it is what the sabotage arm now belongs to.
    let dir = workspace();
    let outside = tempfile::tempdir().expect("somewhere else");
    fs::write(outside.path().join("secret.png"), support::png_bytes(2, 2)).expect("write");

    let elsewhere = outside.path().join("secret.png");
    let staged = prepare(
        dir.path(),
        &Policy::permissive(),
        true,
        elsewhere.to_str().expect("utf-8 path"),
    )
    .unwrap_or_else(|error| panic!("a file the operator pointed at was refused: {error}"));

    assert_eq!(staged.media_type, "image/png");
}

#[test]
fn an_image_the_policy_denies_is_refused_even_though_it_is_under_the_root() {
    // This is the case F1 exists for. A traversal is refused by path resolution
    // whatever the policy says; a file INSIDE the workspace that the policy denies
    // is refused only if the read goes through the gate. `std::fs::read` after
    // resolving would sail straight past it — which is exactly the sabotage arm.
    let dir = workspace();
    fs::create_dir_all(dir.path().join("private")).expect("mkdir");
    fs::write(
        dir.path().join("private/badge.png"),
        support::png_bytes(2, 2),
    )
    .expect("write");

    let policy = Policy::permissive().layer("test").deny_read("private/**");
    let error = prepare(dir.path(), &policy, true, "private/badge.png")
        .err()
        .expect("the policy denies reads under private/");

    assert!(error.contains("private/badge.png"), "{error}");
}

#[test]
fn a_traversal_out_of_the_root_is_refused() {
    let dir = workspace();
    assert!(prepare(dir.path(), &Policy::permissive(), true, "../secret.png").is_err());
}

#[test]
fn a_file_that_is_not_an_image_is_refused_by_name_before_it_is_read() {
    // `Media::source_type_for` is the harness's table and the only one. A second
    // extension list here would disagree with it the first time either changed.
    let dir = workspace();
    let error = prepare(dir.path(), &Policy::permissive(), true, "notes.md")
        .err()
        .expect("a markdown file is not an image");

    assert!(error.contains("notes.md"), "{error}");
    assert!(error.contains("not an image"), "{error}");
}

#[test]
fn a_format_nothing_can_decode_is_refused_by_the_harness_and_named() {
    // SVG, HEIC and AVIF are formats `source_type_for` deliberately NAMES and
    // `Media::attach` deliberately refuses, so the refusal can say which one it
    // was rather than "unsupported".
    let dir = workspace();
    fs::write(dir.path().join("logo.svg"), b"<svg/>").expect("write");

    let error = prepare(dir.path(), &Policy::permissive(), true, "logo.svg")
        .err()
        .expect("nothing can decode an svg into pixels for a provider");

    assert!(
        error.to_ascii_uppercase().contains("SVG"),
        "the harness names the format it refused: {error}",
    );
}

#[test]
fn a_provider_that_cannot_look_at_pictures_is_refused_at_the_door() {
    // io-harness has its own guard, `ensure_media_accepted`. It fires INSIDE the
    // turn, after the operator has typed a prompt, and turns a composed turn into
    // an `Error::Config`. Refusing here costs one line and no work — which is the
    // whole of F2's second half, and the sabotage arm is to stage anyway.
    let dir = workspace();
    let error = prepare(dir.path(), &Policy::permissive(), false, "shot.png")
        .err()
        .expect("a text-only provider cannot be handed an image");

    assert!(error.contains("does not accept image input"), "{error}");
    assert!(error.contains("shot.png"), "{error}");
}

#[test]
fn a_missing_file_says_so_rather_than_attaching_nothing() {
    let dir = workspace();
    assert!(prepare(dir.path(), &Policy::permissive(), true, "absent.png").is_err());
}

#[test]
fn an_empty_argument_asks_for_a_path_instead_of_reading_the_root() {
    let dir = workspace();
    let error = prepare(dir.path(), &Policy::permissive(), true, "   ")
        .err()
        .expect("no path is not a path");
    assert!(error.contains("/attach"), "{error}");
}

/// F7 — the staging is io-harness's, and it lasts one turn.
///
/// Asserted on what reached the PROVIDER rather than on a cleared field: a field
/// that io-cli emptied would pass a field assertion while a copy kept somewhere
/// else went on riding every turn. What matters is that the second turn's
/// requests carry no image.
#[tokio::test]
async fn an_attachment_rides_one_turn_and_the_next_turn_carries_none() {
    let dir = workspace();
    let store = Store::open(dir.path().join("store.db")).expect("a store");
    let mut session = Session::open(&store, dir.path()).expect("a session");
    let provider = support::Watching::new();
    let policy = Policy::permissive();

    let staged = prepare(dir.path(), &policy, true, "shot.png").expect("a png");
    session.attach([staged.media]);

    session
        .turn("what is this?", &provider, &store, &policy, &ApproveAll)
        .await
        .expect("a turn");
    let first = provider.take_media_counts();

    session
        .turn("and now?", &provider, &store, &policy, &ApproveAll)
        .await
        .expect("a second turn");
    let second = provider.take_media_counts();

    assert!(
        first.contains(&1),
        "the turn the image was attached to carried it: {first:?}",
    );
    assert!(
        second.iter().all(|count| *count == 0),
        "the next turn carries no image unless another is attached: {second:?}",
    );
}

/// F5 — when the agent looks, the operator sees the same picture.
///
/// The subject is `attach::viewed`, which is in the library rather than in
/// `src/main.rs` for the reason 0.4.0 recorded: no integration test links a
/// binary, so a decision in a match arm there is unsabotageable. Every branch
/// below is one a sabotage can flip.
mod viewed {
    use super::*;
    use io_cli::attach::viewed;
    use io_harness::tools::VIEW_IMAGE_TOOL;
    use io_harness::{EventKind, RunEvent};

    const WIDE: u16 = 80;

    fn call(name: &str, target: &str) -> RunEvent {
        RunEvent::new(
            1,
            0,
            EventKind::ToolCall {
                name: name.to_string(),
                target: target.to_string(),
            },
        )
    }

    /// The cell form's text. A test that asked for cells and got an escape has
    /// found a defect, so this asserts the form rather than accommodating both.
    fn lines_of(drawn: io_cli::picture::Drawn) -> Vec<ratatui::text::Line<'static>> {
        match drawn {
            io_cli::picture::Drawn::Lines(lines) => lines,
            io_cli::picture::Drawn::Graphics { .. } => {
                panic!("asked for cells and got a graphics escape")
            }
        }
    }

    fn text(lines: &[ratatui::text::Line<'_>]) -> String {
        lines
            .iter()
            .map(|line| {
                line.spans
                    .iter()
                    .map(|span| span.content.as_ref())
                    .collect::<String>()
            })
            .collect::<Vec<_>>()
            .join("\n")
    }

    #[test]
    fn a_look_at_an_image_renders_it() {
        let dir = workspace();
        let lines = viewed(
            dir.path(),
            &Policy::permissive(),
            &call(VIEW_IMAGE_TOOL, "shot.png"),
            true,
            io_cli::term::Graphics::None,
            WIDE,
        )
        .map(lines_of)
        .expect("a view_image call naming a png is a picture to show");

        assert!(!lines.is_empty());
        // 4x2 pixels is one row of four half-block cells, undisturbed by the
        // fit because the terminal is wider than the picture.
        assert_eq!(lines.len(), 1);
    }

    #[test]
    fn the_target_is_resolved_against_the_session_root_and_not_the_process_cwd() {
        // `io -C <dir>` sets a session root without changing the process working
        // directory. A resolver that used the cwd agrees with this one right up
        // until somebody uses that flag — the exact shape 0.3.0 shipped and paid
        // for, in the same product, through a different door.
        let dir = workspace();
        let lines = viewed(
            dir.path(),
            &Policy::permissive(),
            &call(VIEW_IMAGE_TOOL, "docs/deep.png"),
            true,
            io_cli::term::Graphics::None,
            WIDE,
        )
        .map(lines_of)
        .expect("a path under the session root resolves");

        assert!(
            !text(&lines).contains("cannot be shown"),
            "the file under the root was found: {}",
            text(&lines),
        );
    }

    #[test]
    fn another_tool_is_not_a_picture() {
        let dir = workspace();
        assert!(viewed(
            dir.path(),
            &Policy::permissive(),
            &call("read_file", "shot.png"),
            true,
            io_cli::term::Graphics::None,
            WIDE,
        )
        .is_none());
    }

    #[test]
    fn a_target_that_is_not_an_image_is_not_a_picture() {
        let dir = workspace();
        assert!(viewed(
            dir.path(),
            &Policy::permissive(),
            &call(VIEW_IMAGE_TOOL, "notes.md"),
            true,
            io_cli::term::Graphics::None,
            WIDE,
        )
        .is_none());
    }

    #[test]
    fn a_target_the_policy_denies_says_so_and_draws_nothing() {
        // The agent may have been refused too. Drawing a file the session may not
        // read would be this crate reaching around its own boundary in order to
        // show a picture.
        let dir = workspace();
        fs::create_dir_all(dir.path().join("private")).expect("mkdir");
        fs::write(
            dir.path().join("private/badge.png"),
            support::png_bytes(2, 2),
        )
        .expect("write");
        let policy = Policy::permissive().layer("test").deny_read("private/**");

        let lines = viewed(
            dir.path(),
            &policy,
            &call(VIEW_IMAGE_TOOL, "private/badge.png"),
            true,
            io_cli::term::Graphics::None,
            WIDE,
        )
        .map(lines_of)
        .expect("a denied look is still something to say");

        assert!(text(&lines).contains("cannot be shown"), "{}", text(&lines));
        assert!(
            text(&lines).contains("private/badge.png"),
            "{}",
            text(&lines)
        );
    }

    #[test]
    fn the_plain_form_names_the_file_instead_of_drawing_it() {
        let dir = workspace();
        let lines = viewed(
            dir.path(),
            &Policy::permissive(),
            &call(VIEW_IMAGE_TOOL, "shot.png"),
            false,
            io_cli::term::Graphics::None,
            WIDE,
        )
        .map(lines_of)
        .expect("a picture to describe");

        let rendered = text(&lines);
        assert!(rendered.contains("shot.png"), "{rendered}");
        assert!(rendered.contains("4x2"), "{rendered}");
        assert!(
            !rendered.contains('\u{2580}'),
            "no half blocks under the plain form: {rendered}",
        );
    }
}

/// F6 — the path a drag pastes is the path that is attached.
///
/// Dragging a file into the terminal, or copying one out of Finder, pastes its
/// path, and the composer wraps it so that a path with a space in it stays one
/// word. Until 0.13.1 the quotes reached `Media::source_type_for`, which read the
/// extension as `png"` and correctly said it was not an image — so io refused the
/// operator's own screenshot in a sentence about image formats. The name below is
/// the shape macOS actually writes: a space before the date, and a U+202F narrow
/// no-break space before the `AM`.
#[test]
fn f6_a_quoted_path_with_a_space_and_a_narrow_no_break_space_is_attached() {
    let dir = tempfile::tempdir().expect("a temporary directory");
    let name = "Screenshot 2026-08-24 at 8.00.01\u{202f}AM.png";
    fs::write(dir.path().join(name), support::png_bytes(4, 2)).expect("write");

    let quoted = format!("\"{name}\"");
    let staged = prepare(dir.path(), &Policy::permissive(), true, &quoted)
        .unwrap_or_else(|error| panic!("a quoted path was refused: {error}"));
    assert_eq!(staged.media_type, "image/png");
    assert_eq!(staged.path, name, "the quotes belong to the prompt, not to the path");

    // A single-quoted one too, because that is what the composer writes for a
    // path that itself carries a double quote.
    let staged = prepare(dir.path(), &Policy::permissive(), true, &format!("'{name}'"))
        .unwrap_or_else(|error| panic!("a single-quoted path was refused: {error}"));
    assert_eq!(staged.path, name);

    // And the unquoted one still works, which is what everything that types a
    // path by hand sends.
    prepare(dir.path(), &Policy::permissive(), true, name).expect("an unquoted path");
}

/// One pair, and only a matching one. A file may legally have a quote in its
/// name, and stripping every quote would be this crate overruling the
/// filesystem.
#[test]
fn f6_only_one_matching_pair_of_quotes_comes_off() {
    let dir = tempfile::tempdir().expect("a temporary directory");
    let name = "\"quoted\".png";
    fs::write(dir.path().join(name), support::png_bytes(2, 2)).expect("write");

    let staged = prepare(dir.path(), &Policy::permissive(), true, name)
        .unwrap_or_else(|error| panic!("a file whose name carries quotes was refused: {error}"));
    assert_eq!(staged.path, name);
}

/// The policy still governs the workspace. What was added is the case the gate
/// could only ever answer "no" to, not a way around the gate.
#[test]
fn f6_a_denied_path_inside_the_workspace_is_still_refused() {
    let dir = workspace();
    let denied = Policy::permissive().layer("test").deny_read("shot.png");

    let refused = prepare(dir.path(), &denied, true, "shot.png")
        .err()
        .expect("a policy that denies the read still denies it");
    assert!(
        refused.contains("shot.png"),
        "the refusal names the path: {refused}"
    );

    // And the same file addressed absolutely is the same refusal, rather than a
    // way round it: a path under the root goes through the gate however it is
    // spelled.
    let absolute = dir.path().join("shot.png").display().to_string();
    let refused = prepare(dir.path(), &denied, true, &absolute)
        .err()
        .expect("an absolute path under the root goes through the gate too");
    assert!(
        refused.contains("shot.png"),
        "the refusal names the path: {refused}"
    );
}

/// F14 — an attachment is `[Image #1]`, not a picture.
///
/// Twenty rows of somebody's screenshot in the middle of a conversation is not
/// what a reader wants by default, and a committed picture cannot be folded away
/// afterwards: the row belongs to the terminal's scrollback. So the marker is
/// what the prompt carries, what the agent is told and what the transcript
/// keeps, and `/image 1` draws the picture when somebody wants it.
#[test]
fn f14_an_attachment_is_a_marker_the_composer_deletes_whole() {
    use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

    let mut composer = io_cli::composer::Composer::new();
    for character in "look at ".chars() {
        composer.key(KeyEvent::new(
            KeyCode::Char(character),
            KeyModifiers::NONE,
        ));
    }
    composer.attach("[Image #1]", "/tmp/shot.png");

    assert_eq!(composer.typed(), "look at [Image #1] ");
    assert_eq!(
        composer.text(),
        "look at [Image #1] ",
        "the marker is sent as itself: the picture rides the turn as media",
    );

    // One press per deletion key, and the whole marker goes — the same rule a
    // pasted block has.
    for key in [
        KeyEvent::new(KeyCode::Backspace, KeyModifiers::NONE),
        KeyEvent::new(KeyCode::Backspace, KeyModifiers::ALT),
    ] {
        let mut composer = io_cli::composer::Composer::new();
        composer.attach("[Image #1]", "/tmp/shot.png");
        // Past the trailing space the marker is followed by.
        composer.key(KeyEvent::new(KeyCode::Backspace, KeyModifiers::NONE));
        composer.key(key);
        assert_eq!(composer.typed(), "", "{key:?} left part of a marker behind");
    }
}

/// The numbering is the session's, one-based, and does not restart with a turn:
/// `#3` has to mean one thing to somebody scrolling back.
#[test]
fn f14_the_marker_number_is_a_handle_the_session_keeps() {
    use io_cli::app::App;
    use io_cli::theme::DARK;

    let mut app = App::new(DARK, "opus-5");
    assert_eq!(app.images(), 0);
    assert_eq!(app.image(1), None);

    assert_eq!(app.attached("/tmp/one.png"), 1);
    assert_eq!(app.attached("/tmp/two.png"), 2);
    assert_eq!(app.image(1), Some("/tmp/one.png"));
    assert_eq!(app.image(2), Some("/tmp/two.png"));
    assert_eq!(app.image(3), None, "a number nobody attached names nothing");
    assert_eq!(app.image(0), None, "the numbering a person reads starts at one");
}

/// What the caption says, which is everything needed to tell one attachment from
/// another, on one row.
#[test]
fn f14_the_caption_names_the_number_the_file_and_the_size() {
    let caption = io_cli::picture::caption(2, "/tmp/shot.png", "image/png", 391_790);
    assert!(caption.contains("[Image #2]"), "{caption}");
    assert!(caption.contains("/tmp/shot.png"), "{caption}");
    assert!(caption.contains("png"), "{caption}");
    assert!(
        caption.contains("382.6 KB"),
        "a size a person can check against the file they attached: {caption}",
    );
    assert!(
        !caption.contains("391790"),
        "a byte count is not a size anybody reads: {caption}",
    );
}

/// F15 — a pasted picture is an attachment, and there is no command.
///
/// Dropping an image on the prompt is what an operator already does in every
/// other window they talk to a model in; `/attach` was something they had to be
/// told about first. What `App::paste` answers with is what the driver acts on:
/// a path naming an image that exists is staged and marked, anything else is
/// text.
#[test]
fn f15_a_pasted_image_path_is_recognised_as_a_picture() {
    use io_cli::app::{App, Pasted};
    use io_cli::theme::DARK;

    let dir = workspace();
    let picture = dir.path().join("shot.png");
    let mut app = App::new(DARK, "opus-5");

    assert_eq!(
        app.paste(&picture.display().to_string(), false),
        Pasted::Picture(
            picture
                .canonicalize()
                .expect("a real path")
                .display()
                .to_string()
        ),
        "a path naming an image that exists is an attachment",
    );

    // A path naming something that is not an image is a path, and prose is prose.
    let notes = dir.path().join("notes.md");
    assert_eq!(app.paste(&notes.display().to_string(), false), Pasted::Text);
    assert_eq!(
        app.paste("look at the picture in my documents", false),
        Pasted::Text,
    );
}

/// Pasting the same picture again toggles what the prompt shows, and never
/// attaches it twice.
#[test]
fn f15_pasting_the_same_picture_again_toggles_the_marker_and_the_path() {
    let mut composer = io_cli::composer::Composer::new();
    composer.attach("[Image #1]", "/tmp/shot.png");
    assert_eq!(composer.typed(), "[Image #1] ");
    assert!(composer.attached("/tmp/shot.png"));

    composer.attach("[Image #1]", "/tmp/shot.png");
    assert_eq!(
        composer.typed(),
        "\"/tmp/shot.png\" ",
        "the second paste shows the file it stands for, quoted",
    );

    composer.attach("[Image #1]", "/tmp/shot.png");
    assert_eq!(
        composer.typed(),
        "[Image #1] ",
        "and the third puts the marker back",
    );

    // A second picture is a second number; the first keeps its own.
    composer.attach("[Image #2]", "/tmp/other.png");
    let typed = composer.typed();
    assert!(typed.contains("[Image #1]"), "{typed:?}");
    assert!(typed.contains("[Image #2]"), "{typed:?}");
}

/// F15 — a marker deletes with the space that was written for it.
///
/// The marker is written as `[Image #1] ` so the next word does not run into the
/// bracket, and the cursor sits after that space. A deletion took the space
/// first, and a word-wise one then ate `1]` off the marker and left `[Image #` on
/// the prompt — which is the capture this was reported with.
#[test]
fn f15_one_press_removes_a_marker_and_its_space() {
    use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

    for modifiers in [
        KeyModifiers::NONE,
        KeyModifiers::ALT,
        KeyModifiers::CONTROL,
    ] {
        let mut composer = io_cli::composer::Composer::new();
        composer.attach("[Image #1]", "/tmp/shot.png");
        assert_eq!(composer.typed(), "[Image #1] ");

        composer.key(KeyEvent::new(KeyCode::Backspace, modifiers));
        assert_eq!(
            composer.typed(),
            "",
            "{modifiers:?} left something of the marker behind",
        );
    }
}

/// The path the toggle shows is quoted, the way any pasted path is.
#[test]
fn f15_the_toggled_path_is_quoted() {
    let mut composer = io_cli::composer::Composer::new();
    composer.attach("[Image #1]", "/tmp/two words.png");

    composer.attach("[Image #1]", "/tmp/two words.png");
    assert_eq!(
        composer.typed(),
        "\"/tmp/two words.png\" ",
        "a path with a space in it is two words to everything downstream unquoted",
    );

    composer.attach("[Image #1]", "/tmp/two words.png");
    assert_eq!(composer.typed(), "[Image #1] ", "and it toggles back");
}

/// F15 — a new conversation starts its numbering again.
#[test]
fn f15_clear_resets_the_image_numbering() {
    use io_cli::app::App;
    use io_cli::theme::DARK;

    let mut app = App::new(DARK, "opus-5");
    app.attached("/tmp/one.png");
    assert_eq!(app.attached("/tmp/two.png"), 2);

    assert!(app.clear_conversation(), "an idle session clears");

    assert_eq!(app.images(), 0, "the attachments belonged to that conversation");
    assert_eq!(
        app.attached("/tmp/three.png"),
        1,
        "the next conversation starts at #1",
    );
    assert_eq!(
        app.composer.text(),
        "",
        "and a prompt written against the conversation that ended goes with it",
    );
}
