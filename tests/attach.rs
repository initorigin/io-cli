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
fn a_path_outside_the_root_is_refused_by_the_workspace_and_not_read() {
    // The whole of F1: the gate is io-harness's, and it is the same gate a source
    // read passes. Reading with `std::fs` after resolving would attach a file the
    // session was told it may not read — which is the sabotage arm for this
    // criterion.
    let dir = workspace();
    let outside = tempfile::tempdir().expect("somewhere else");
    fs::write(outside.path().join("secret.png"), support::png_bytes(2, 2)).expect("write");

    let escape = outside.path().join("secret.png");
    let error = prepare(
        dir.path(),
        &Policy::permissive(),
        true,
        escape.to_str().expect("utf-8 path"),
    )
    .err()
    .expect("a path outside the session root is not attachable");

    assert!(
        !error.is_empty(),
        "the workspace's own refusal names the path",
    );
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
