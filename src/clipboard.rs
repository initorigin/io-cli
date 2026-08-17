//! Putting text on the clipboard of the terminal a session is actually being
//! watched on, which over SSH is not this machine.
//!
//! io-cli is used through an SSH connection and inside tmux. A local clipboard
//! call — pasteboard, X11, Wayland — sets the clipboard of the host the agent is
//! running on, which is the wrong computer: the reader is at the far end of the
//! connection, and the text they asked for would land somewhere they cannot
//! reach. OSC 52 is the one mechanism that goes the other way. It is an escape
//! sequence, so it travels back up the same stream the transcript does and is
//! interpreted by the terminal emulator in front of the reader.
//!
//! # Nothing ever answers
//!
//! A terminal does not reply to an OSC 52 write. There is no acknowledgement, no
//! error and no refusal on the wire — the sequence is written and that is the end
//! of what this process can observe. Meanwhile the two most common deployments
//! both drop it quietly: tmux discards the sequence unless the user has set
//! `set -g set-clipboard on`, and most terminals silently ignore a payload past
//! some internal ceiling they do not publish.
//!
//! So a `copied` message here would be a claim io-cli cannot support, printed at
//! exactly the moment the reader goes to paste and finds the old contents. That
//! is why this module hands back [`describe`] and not a `bool`: there is no
//! success value to return, and the shape of the API is what keeps a call site
//! from inventing one. What can honestly be said is what was sent and how big it
//! was, which is also the useful thing — a reader whose paste comes up empty can
//! compare the size against their terminal's limit and their tmux setting.
//!
//! # The size ceiling
//!
//! io-cli sends the payload whatever its size. The cap is the terminal's, it
//! differs between emulators, and nothing on this side can query it, so refusing
//! to send at some invented threshold would fail writes that would have worked.
//! Sending and reporting the size leaves the reader holding the one fact that
//! makes the failure diagnosable. Note that the sequence on the wire is about a
//! third larger than the payload, because base64 spends four characters on every
//! three bytes, and it is the encoded form the terminal measures.
//!
//! Nothing here wraps the sequence for tmux's DCS passthrough. Passthrough has to
//! be applied only when actually inside tmux, and the sequence sent to a terminal
//! that is not tmux is garbage printed into the transcript; `$TMUX` is not a
//! reliable enough signal to bet the transcript on, and `set-clipboard on` is a
//! setting the user can make once and keep.

/// Standard base64, the `+/` alphabet of RFC 4648 — not the URL-safe variant.
/// OSC 52 payloads are read by terminal emulators that implement the original,
/// and a `-_` payload decodes to different bytes rather than failing loudly.
const ALPHABET: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";

/// The full sequence to write to the terminal to put `payload` on the system
/// clipboard: `ESC ] 52 ; c ; <base64> BEL`.
///
/// Selection `c` is the system clipboard, the one a paste reads. The other
/// selections OSC 52 can address — the X11 primary and the cut buffers — are not
/// where a reader who pressed the copy key expects to find their text.
///
/// The encoding is not decoration. An OSC string ends at the first BEL or ST, so
/// a payload carrying either byte — and a transcript can carry both, since tool
/// output is arbitrary — would terminate the sequence early and leave the rest of
/// itself printed on screen as text. Base64 removes every byte that could do
/// that, which is why the sequence is built here rather than by a caller.
///
/// Terminated with BEL rather than ST because the whole xterm lineage and tmux
/// accept BEL, while a handful of terminals only ever learned that form.
pub fn sequence(payload: &str) -> String {
    format!("\x1b]52;c;{}\x07", encode(payload.as_bytes()))
}

/// Base64-encode `bytes`, with `=` padding.
///
/// Hand-written because it is twenty lines against a dependency, and this crate's
/// dependency list is asserted by `tests/dependencies.rs` — the list is the claim
/// that io-cli contains no agent machinery of its own, and it is worth more than
/// the twenty lines cost.
pub fn encode(bytes: &[u8]) -> String {
    let mut encoded = String::with_capacity(bytes.len().div_ceil(3) * 4);
    for chunk in bytes.chunks(3) {
        // The missing bytes of a short final chunk encode as zero bits, and the
        // characters that carry only those bits become padding below.
        let second = chunk.get(1).copied().unwrap_or(0);
        let third = chunk.get(2).copied().unwrap_or(0);
        let group = (u32::from(chunk[0]) << 16) | (u32::from(second) << 8) | u32::from(third);
        for position in 0..4 {
            // One input byte fills two output characters, two fill three, three
            // fill four; everything past that is `=`.
            if position <= chunk.len() {
                let index = (group >> (18 - position * 6)) & 0b11_1111;
                encoded.push(ALPHABET[index as usize] as char);
            } else {
                encoded.push('=');
            }
        }
    }
    encoded
}

/// What can truthfully be said after writing [`sequence`]: `sent 4.2 kB to the
/// terminal clipboard`.
///
/// Deliberately not a past-tense claim about the clipboard's contents. See the
/// module documentation — no terminal answers an OSC 52 write, so `copied` is a
/// statement about something this process never observes. This wording says only
/// what io-cli did, which is the part it knows.
///
/// The size is the payload's, in bytes rather than characters, because bytes are
/// what a terminal's limit is expressed in and a line of CJK text is three times
/// its own length on the wire.
pub fn describe(payload: &str) -> String {
    format!(
        "sent {} to the terminal clipboard",
        format_bytes(payload.len())
    )
}

/// `1 byte`, `840 bytes`, `4.2 kB`. Decimal kilobytes, matching the `kB` it is
/// spelled with: the number exists to be held against a terminal's documented
/// limit, and those limits are quoted in round decimal thousands.
fn format_bytes(bytes: usize) -> String {
    match bytes {
        1 => "1 byte".to_string(),
        0..=999 => format!("{bytes} bytes"),
        _ => format!("{:.1} kB", bytes as f64 / 1000.0),
    }
}
