//! Drawing an image with the cells a terminal already has.
//!
//! The module is `picture` rather than `image` on purpose: a module of that name
//! at the crate root is ambiguous with the decoder crate of the same name, and an
//! ambiguity error in a path is a poor trade for a word.
//!
//! # Why half blocks
//!
//! A terminal cell is about twice as tall as it is wide. `▀` fills the top half
//! of one and leaves the bottom half showing the background, so a single cell
//! carries two colours stacked — and each of those halves is about as tall as the
//! cell is wide. **A half-block pixel is therefore square**, which is the whole
//! reason the technique is used, and it means fitting a picture is an ordinary
//! box fit against a box that is `cols` wide and `rows * 2` tall.
//!
//! The arithmetic that looks right and is not is to fit against the row count
//! instead of against half rows. It produces a picture at half its height, which
//! is only obviously wrong beside the original — see the proportions test in
//! `tests/image.rs`.
//!
//! # What this module does not do
//!
//! It does not read files and it does not decide what may be read. The bytes
//! arrive from `io_harness::tools::Workspace`, under the session's own policy,
//! and they have already been through `Media::attach` — so by the time anything
//! here runs, io-harness has already refused an unsupported format by name and an
//! oversized file by its bound. This module inherits those limits by running
//! after them rather than by restating them.

use io_harness::Media;
use ratatui::style::{Color, Style};
use ratatui::text::{Line, Span};

/// The glyph that carries two pixel rows in one cell.
///
/// Public because the tests assert on it, and because a second spelling of it
/// somewhere else would be a second answer to what a picture is made of.
pub const UPPER_HALF: char = '▀';

/// Turn the bytes of a file into pixels.
///
/// The error is a sentence rather than the decoder's own type: every caller puts
/// it in front of an operator, and nothing here can act on the distinction
/// between one decode failure and another.
///
/// A file whose extension lies is a real input — io-harness refuses by *media
/// type*, which is inferred from the name — so this returns an error rather than
/// letting a decoder panic take the session with it.
pub fn decode(bytes: &[u8]) -> Result<::image::DynamicImage, String> {
    ::image::load_from_memory(bytes).map_err(|error| error.to_string())
}

/// Draw a picture as half-block cells, fitted into `cols` by `rows`.
///
/// Never upscales. A picture smaller than the box is drawn at its own size,
/// because a four-pixel icon blown up to eighty columns is not a better rendering
/// of it, and because rounding a tiny picture *up* is how a one-pixel image
/// becomes a screenful.
///
/// Never returns zero lines for a picture that has pixels: `insert_before` given
/// a height of zero commits nothing at all, which would drop the content in
/// silence.
pub fn cells(picture: &::image::DynamicImage, cols: u16, rows: u16) -> Vec<Line<'static>> {
    use ::image::GenericImageView;

    let (cols, rows) = (cols.max(1), rows.max(1));
    let (width, height) = picture.dimensions();
    if width == 0 || height == 0 {
        return Vec::new();
    }

    // The box is measured in pixels: one column is one pixel wide, one row is two
    // pixels tall.
    let (box_w, box_h) = (u32::from(cols), u32::from(rows) * 2);
    let fitted = if width > box_w || height > box_h {
        // `resize` preserves the aspect ratio and clamps each side to at least
        // one, so a very tall picture keeps a column rather than becoming empty.
        picture.resize(box_w, box_h, ::image::imageops::FilterType::Triangle)
    } else {
        picture.clone()
    };

    let rgba = fitted.to_rgba8();
    let (width, height) = rgba.dimensions();
    let mut lines = Vec::with_capacity(height.div_ceil(2) as usize);
    for top in (0..height).step_by(2) {
        let mut spans = Vec::with_capacity(width as usize);
        for x in 0..width {
            let upper = rgba.get_pixel(x, top).0;
            let mut style = Style::default().fg(Color::Rgb(upper[0], upper[1], upper[2]));
            // An odd final row has no pixel underneath. Leaving the background
            // alone is the honest answer: any colour chosen here would be a row
            // of the picture that was never in the file.
            if top + 1 < height {
                let lower = rgba.get_pixel(x, top + 1).0;
                style = style.bg(Color::Rgb(lower[0], lower[1], lower[2]));
            }
            spans.push(Span::styled(UPPER_HALF.to_string(), style));
        }
        lines.push(Line::from(spans));
    }
    lines
}

/// Whether a picture may be drawn from coloured cells at all.
///
/// One expression, for the reason `Status::indicator` is one expression: three
/// separate suppressions written at three call sites drift, and the first time
/// one of them is forgotten it is forgotten on the surface whose reader can least
/// afford it.
///
/// - `--plain` removes it because a half-block picture is pure decoration to a
///   screen reader and thousands of cells of it are worse than none.
/// - `NO_COLOR` removes it because the picture IS the colour: with no colour a
///   half block carries nothing at all.
/// - The ASCII glyph set removes it because `▀` is not in it, and a terminal that
///   asked for ASCII would be sent a character it cannot draw.
pub fn drawable(coloured: bool, plain: bool, glyphs: &crate::glyphs::Glyphs) -> bool {
    coloured && !plain && glyphs.name != crate::glyphs::ASCII.name
}

/// How tall a committed picture is allowed to be.
///
/// ponytail: a constant rather than a fraction of the terminal, because a
/// committed picture goes into scrollback where the terminal's height is not the
/// bound that matters — what matters is how much of the conversation it pushes
/// off the screen, and twenty rows is about one screenful on a normal terminal.
/// Make it a fraction of the height if anyone ever asks for a taller one.
pub const MAX_ROWS: u16 = 20;

/// The largest base64 payload one Kitty escape may carry.
///
/// The protocol's own limit, not a taste: a transmission longer than this is
/// split across escapes carrying `m=1` until the last, which carries `m=0`.
const KITTY_CHUNK: usize = 4096;

/// How a picture is going to reach the terminal.
///
/// One type so the choice is made in ONE place. Two call sites each deciding
/// whether to draw cells or emit an escape is the shape `drawable` exists to
/// avoid, and the failure mode is worse here: a site that got it wrong would put
/// an unreadable escape into somebody's permanent scrollback.
pub enum Drawn {
    /// Ordinary cells, committed the way every other line is.
    Lines(Vec<Line<'static>>),
    /// A graphics-protocol escape, and the rows it must be given.
    ///
    /// The payload is written into a region of exactly `rows` rows whose other
    /// cells are empty — see `Screen::commit_raw`. Which protocol built it does
    /// not survive into this type on purpose: by the time it is committed the
    /// escape is bytes and a height, and the commit path treats both the same.
    Graphics { payload: String, rows: u16 },
}

/// A Kitty graphics escape placing `base64` into `cols` by `rows` cells.
///
/// `a=T` transmits and displays in one go. **`C=1` is the load-bearing flag**: it
/// tells Kitty not to move the cursor, and that is what makes the escape compose
/// with a renderer that draws the cells around it. `CrosstermBackend::draw`
/// re-anchors with an absolute `MoveTo` at the start of every row, so a placement
/// that moves nothing cannot desynchronise the draw — and one that did move the
/// cursor might scroll, which would change what every later `MoveTo` means.
///
/// `f=100` is PNG, which is why the caller only reaches this for a PNG payload.
///
/// Control keys ride the first escape only; every later chunk carries `m=` and
/// nothing else, which is what the protocol specifies.
pub fn kitty(base64: &str, cols: u16, rows: u16) -> String {
    let chunks: Vec<&str> = base64
        .as_bytes()
        .chunks(KITTY_CHUNK)
        .map(|chunk| std::str::from_utf8(chunk).unwrap_or_default())
        .collect();
    let mut out = String::with_capacity(base64.len() + chunks.len() * 32);
    for (i, chunk) in chunks.iter().enumerate() {
        let more = u8::from(i + 1 < chunks.len());
        if i == 0 {
            out.push_str(&format!(
                "\x1b_Ga=T,f=100,c={cols},r={rows},C=1,m={more};{chunk}\x1b\\"
            ));
        } else {
            out.push_str(&format!("\x1b_Gm={more};{chunk}\x1b\\"));
        }
    }
    out
}

/// An iTerm2 inline-image escape placing `base64` into `cols` by `rows` cells.
///
/// **The save and restore are what Kitty's `C=1` is.** iTerm2 has no flag that
/// leaves the cursor alone — the placement advances it, and an advance at the
/// bottom of the screen scrolls, which changes what every later absolute `MoveTo`
/// means. `\x1b7` and `\x1b8` (DECSC/DECRC) put the cursor back where the
/// renderer left it, so the escape composes with the cells drawn around it. That
/// is the whole of what `US-IO-CLI-0.9.0-I01` deferred.
///
/// `width` and `height` are in cells, from the same fitter the half-block form
/// uses, so the rows the picture costs are known before it is written rather than
/// discovered from where the cursor ended up. `preserveAspectRatio=1` keeps the
/// fit the fitter computed instead of stretching to the box.
///
/// Unlike Kitty's `f=100`, no format is named: iTerm2 decodes the file itself, so
/// a jpeg or a gif rides as itself.
pub fn iterm2(base64: &str, cols: u16, rows: u16) -> String {
    format!(
        "\x1b7\x1b]1337;File=inline=1;width={cols};height={rows};preserveAspectRatio=1:{base64}\x07\x1b8"
    )
}

/// Bytes on disk to lines in the scrollback — the whole of what both directions
/// of this release share.
///
/// `drawable` is [`drawable`]'s answer, computed by the caller because the caller
/// is what holds the theme. A file that will not decode is NOT an error: for an
/// attachment io-harness has already accepted it for the wire, so the agent is
/// going to see it, and for a `view_image` the agent has already seen it. What
/// failed is this crate's ability to show the operator the same thing, and saying
/// so beats pretending nothing happened.
pub fn render(
    bytes: &[u8],
    path: &str,
    media_type: &str,
    drawable: bool,
    graphics: crate::term::Graphics,
    width: u16,
) -> Drawn {
    use ::image::GenericImageView;

    let Ok(picture) = decode(bytes) else {
        return Drawn::Lines(vec![Line::from(format!("{path} could not be drawn here"))]);
    };
    let (w, h) = picture.dimensions();
    if !drawable {
        return Drawn::Lines(vec![describe(path, media_type, w, h)]);
    }

    // The real image, where the terminal can take one AND the payload is already
    // the shape the protocol wants. `Media::attach` is what produces the base64 —
    // this crate encodes nothing and takes no base64 dependency — and it is also
    // what decides the question: it PASSES THROUGH the four formats a provider
    // accepts and TRANSCODES the other five to PNG. Kitty's `f=100` is PNG, so a
    // png, a bmp, a tiff, an ico, a tga or a pnm arrives ready, while a jpeg, a
    // gif or a webp comes back as itself and takes the cell form instead.
    //
    // That is a real limit and it is stated rather than hidden: the alternative
    // is a base64 encoder in this crate, which is a dependency or a hand-rolled
    // codec for a path that a screenshot — the case this release exists for, and
    // a PNG on every platform that takes one — does not need.
    if graphics != crate::term::Graphics::None {
        if let Ok(media) = Media::attach(media_type, bytes) {
            // Kitty's `f=100` is PNG and nothing else, so the four formats
            // `Media::attach` passes through unchanged — jpeg, gif and webp among
            // them — take the cell form there. iTerm2 decodes the file itself, so
            // the same jpeg is the real image; webp is the one it will not take,
            // and a format the terminal cannot decode must not be sent to it.
            let carried = match graphics {
                crate::term::Graphics::Kitty => media.media_type == "image/png",
                crate::term::Graphics::Iterm2 => media.media_type != "image/webp",
                crate::term::Graphics::None => false,
            };
            if carried {
                let (cols, rows) = fitted_cells(w, h, width, MAX_ROWS);
                let payload = match graphics {
                    crate::term::Graphics::Iterm2 => iterm2(&media.base64, cols, rows),
                    _ => kitty(&media.base64, cols, rows),
                };
                return Drawn::Graphics { payload, rows };
            }
        }
    }

    Drawn::Lines(cells(&picture, width, MAX_ROWS))
}

/// How many cells a picture of `w` by `h` occupies inside `cols` by `rows`.
///
/// The same box fit [`cells`] performs, so a Kitty placement and a half-block
/// drawing of the same file claim the same area — which is what stops the two
/// forms disagreeing about how much scrollback a picture costs.
fn fitted_cells(w: u32, h: u32, cols: u16, rows: u16) -> (u16, u16) {
    let (box_w, box_h) = (u32::from(cols.max(1)), u32::from(rows.max(1)) * 2);
    let (w, h) = if w > box_w || h > box_h {
        let scale = f64::from(box_w) / f64::from(w.max(1));
        let scale = scale.min(f64::from(box_h) / f64::from(h.max(1)));
        (
            ((f64::from(w) * scale).round() as u32).max(1),
            ((f64::from(h) * scale).round() as u32).max(1),
        )
    } else {
        (w, h)
    };
    (
        u16::try_from(w).unwrap_or(u16::MAX).max(1),
        u16::try_from(h.div_ceil(2)).unwrap_or(u16::MAX).max(1),
    )
}

/// A size a person reads, from a byte count.
///
/// Two significant figures and a unit: `382 KB` is what an operator checks
/// against the file they attached, and `391790 bytes` is not.
pub fn bytes(count: usize) -> String {
    const UNITS: [&str; 4] = ["B", "KB", "MB", "GB"];
    let mut size = count as f64;
    let mut unit = 0;
    while size >= 1024.0 && unit + 1 < UNITS.len() {
        size /= 1024.0;
        unit += 1;
    }
    if unit == 0 {
        format!("{count} B")
    } else {
        format!("{size:.1} {}", UNITS[unit])
    }
}

/// The line that introduces a picture drawn on demand.
///
/// Everything a reader needs to tell one attachment from another — which number
/// it is, which file, what it is and how large — on one row above the picture
/// rather than as a paragraph beside it.
pub fn caption(number: usize, path: &str, media_type: &str, size: usize) -> String {
    let kind = media_type.strip_prefix("image/").unwrap_or(media_type);
    format!("[Image #{number}] {path} ({kind}, {})", bytes(size))
}

/// The one line a picture becomes where cells are not allowed to carry it.
///
/// Under `--plain`, under `NO_COLOR`, and under the ASCII glyph set, a half-block
/// picture is colour carrying the entire meaning — which is the thing §4 of the
/// design refuses. What is left is the sentence: which file, what it is, how big.
pub fn describe(path: &str, media_type: &str, width: u32, height: u32) -> Line<'static> {
    let kind = media_type.strip_prefix("image/").unwrap_or(media_type);
    Line::from(format!("image {path} ({kind}, {width}x{height})"))
}
