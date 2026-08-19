//! Putting an image in front of the agent.
//!
//! The whole of it is four public io-harness calls, and the ordering is the
//! design:
//!
//! 1. [`io_harness::Media::source_type_for`] says what the file is, from its
//!    name. Every image format the harness recognises, including the three it
//!    cannot decode — naming those is what makes the refusal actionable.
//! 2. [`io_harness::tools::Workspace::read_bytes`] reads it. Its own
//!    documentation says it is the same policy gate as `read_file`, not a second
//!    one, which is what makes an image governed by the rules that already govern
//!    source.
//! 3. [`io_harness::Media::attach`] converts or refuses. It transcodes
//!    BMP/TIFF/ICO/TGA/PNM to PNG, refuses SVG/HEIC/AVIF by name, and applies
//!    `MAX_IMAGE_BYTES` and `MAX_IMAGE_PIXELS`.
//! 4. `Session::attach` stages it, and io-harness's `drive` folds it into the
//!    turn's contract with `std::mem::take` — so it rides one turn and the
//!    clearing is the harness's, not something this crate has to remember.
//!
//! **This module contains no extension table, no size bound and no encoder**, and
//! that is not modesty: a second answer to "what may be sent" would disagree with
//! the harness's the first time either changed. The one refusal that is io-cli's
//! own is the provider check, and it is here because it has to happen *earlier*
//! than the harness's — see [`prepare`].
//!
//! Decoding for the screen happens after all of this, in [`crate::picture`], so
//! the harness's bounds have already refused an absurd file before this crate's
//! decoder ever sees one. The limits are inherited by ordering rather than
//! restated.

use std::path::Path;

use io_harness::{EventKind, RunEvent};

use io_harness::tools::{Workspace, VIEW_IMAGE_TOOL};
use io_harness::{Media, Policy};
use ratatui::text::Line;

use crate::picture::Drawn;

/// An image that is ready to go, and the bytes it was read from.
///
/// The bytes are kept for the *screen*, not for the wire: `media` is what the
/// turn carries, and rendering from the original file is what lets the picture be
/// drawn without a base64 decoder — the harness has already encoded a copy for
/// the provider and this crate never decodes that back.
pub struct Staged {
    /// What `Session::attach` is given.
    pub media: Media,
    /// The path as the operator wrote it, for the line that says what happened.
    pub path: String,
    /// What the file is, as the harness named it.
    pub media_type: &'static str,
    /// The file as it was on disk, for [`crate::picture::decode`].
    pub bytes: Vec<u8>,
}

/// Read an image under the session's policy and make it into a [`Media`].
///
/// `root` is [`io_harness::Session::root`] — the root `io -C` set — and never the
/// process working directory. The two agree right up until `io -C` is used, which
/// is the case an operator actually runs and the case a fixture built from the
/// current directory cannot see. 0.3.0 paid for that once.
///
/// `accepts_images` is [`io_harness::Provider::accepts_images`] for the provider
/// this session is running against. The check is here, at attach time, rather
/// than left to the harness — io-harness has its own guard, `ensure_media_accepted`,
/// but that one fires *inside the turn*, after the operator has typed a prompt,
/// and turns a composed turn into an `Error::Config`. Refusing at the door costs
/// the operator one line and no work.
///
/// A leading `@` is stripped. It is not decoration: `@` is what opens this
/// product's path completion, so `/attach @docs/shot.png` is how the path gets
/// typed in the first place, and the marker is still on the line when the command
/// is submitted.
pub fn prepare(
    root: &Path,
    policy: &Policy,
    accepts_images: bool,
    path: &str,
) -> Result<Staged, String> {
    let path = path.trim().trim_start_matches('@');
    if path.is_empty() {
        return Err("say which file: /attach @path/to/image.png".to_string());
    }

    // The harness's table, never a second one here.
    let Some(media_type) = Media::source_type_for(path) else {
        return Err(format!(
            "{path} is not an image this crate can name. Attach a jpeg, png, gif \
             or webp — or a bmp, tiff, ico, tga or pnm, which io-harness converts."
        ));
    };

    // Before the read, so a provider that cannot look at pictures costs nothing
    // and discloses nothing about a file the operator may not be able to read
    // anyway.
    if !accepts_images {
        return Err(format!(
            "this provider does not accept image input, so {path} cannot be \
             attached. Switch the provider, or describe the picture in words."
        ));
    }

    // The same gate as any read the agent makes. A denied path is refused with
    // the harness's own sentence, which names the path and the rule.
    let bytes = Workspace::with_policy(root, policy.clone())
        .read_bytes(path)
        .map_err(|error| error.to_string())?;

    // Converts, or refuses by name and by bound. Both messages are the
    // harness's and are shown unchanged.
    let media = Media::attach(media_type, &bytes).map_err(|error| error.to_string())?;

    Ok(Staged {
        media,
        path: path.to_string(),
        media_type,
        bytes,
    })
}

/// The sentence that says an image is on the next turn, and only the next one.
///
/// Said before the turn goes rather than after it arrives, because "one turn
/// only" is the part an operator has to know in advance — it is what tells them
/// to attach again rather than wondering why the follow-up question got a
/// different answer.
pub fn staged_note(staged: &Staged) -> String {
    let kind = staged
        .media_type
        .strip_prefix("image/")
        .unwrap_or(staged.media_type);
    format!(
        "attached {} ({}, {} bytes) to the next turn, and only the next one",
        staged.path,
        kind,
        staged.media.byte_len(),
    )
}

/// The picture the agent just looked at, if this event is one looking.
///
/// **Here rather than in `src/main.rs`, and that placement is the point.** No
/// integration test links a binary, so a decision in a match arm there cannot be
/// sabotaged — 0.4.0 found that the hard way and the fix that worked was to move
/// the decision into the library even as a one-line wrapper. Every branch below
/// is a branch a test can flip.
///
/// `None` means "not a picture to show": a different event, a different tool, or
/// a target whose name is not an image. `Some` always carries something to
/// commit, including when the read or the decode failed — the agent has already
/// seen the file, so silence would leave the operator with less than the agent
/// has.
///
/// `target` is **the raw argument the model wrote**, which `EventKind::ToolCall`
/// documents and which this product paid for in 0.3.0. It is resolved against the
/// session root, never the process working directory: the two agree right up
/// until `io -C` sets one, which is the case an operator actually runs.
///
/// The read goes through the same `Workspace` and the same policy as everything
/// else. A target the policy denies renders nothing but the refusal — drawing a
/// file the session may not read would be this crate reaching around its own
/// boundary to show a picture.
pub fn viewed(
    root: &Path,
    policy: &Policy,
    event: &RunEvent,
    drawable: bool,
    graphics: crate::term::Graphics,
    width: u16,
) -> Option<Drawn> {
    let EventKind::ToolCall { name, target } = &event.kind else {
        return None;
    };
    if name != VIEW_IMAGE_TOOL {
        return None;
    }
    let media_type = Media::source_type_for(target)?;
    Some(
        match Workspace::with_policy(root, policy.clone()).read_bytes(target) {
            Ok(bytes) => {
                crate::picture::render(&bytes, target, media_type, drawable, graphics, width)
            }
            Err(error) => Drawn::Lines(vec![Line::from(format!(
                "the agent looked at {target}, which cannot be shown here: {error}"
            ))]),
        },
    )
}
