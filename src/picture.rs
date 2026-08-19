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

/// The one line a picture becomes where cells are not allowed to carry it.
///
/// Under `--plain`, under `NO_COLOR`, and under the ASCII glyph set, a half-block
/// picture is colour carrying the entire meaning — which is the thing §4 of the
/// design refuses. What is left is the sentence: which file, what it is, how big.
pub fn describe(path: &str, media_type: &str, width: u32, height: u32) -> Line<'static> {
    let kind = media_type.strip_prefix("image/").unwrap_or(media_type);
    Line::from(format!("image {path} ({kind}, {width}x{height})"))
}
