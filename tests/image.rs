//! F3 — the picture is cells, and the cells are right.
//!
//! The subject is `io_cli::picture`, which takes the bytes of a file io-harness has
//! already accepted and returns lines. It is a function of bytes rather than of a
//! path for the same reason `diff::cell` is a function of an `Edit`: the read
//! belongs to the driver, which is the only thing holding a `Workspace`, and
//! keeping it out of here is what lets these tests state an image by hand.
//!
//! A cell is about twice as tall as it is wide, and a half block splits it in
//! two — so a half-block pixel is square, and fitting is a plain box fit against
//! a box whose height is measured in HALF rows. The mistake that looks right is
//! to fit against the row count instead, which renders every picture at half its
//! height; `a_picture_keeps_its_proportions_when_height_binds` is what fails when
//! it happens — and note that the width-binding case CANNOT see it, which is why
//! there are two.

mod support;

use image::{DynamicImage, Rgba, RgbaImage};
use ratatui::style::Color;
use ratatui::text::Line;

use io_cli::picture::{cells, decode, describe, UPPER_HALF};

/// A terminal wide enough that nothing is fitted, so a test about colour is not
/// silently also a test about scaling.
const WIDE: u16 = 120;

/// Rows enough that nothing is fitted vertically either.
const TALL: u16 = 40;

/// An image whose every pixel is stated, so the expected cells are computable
/// rather than eyeballed.
fn painted(width: u32, height: u32, pixels: &[(u8, u8, u8)]) -> DynamicImage {
    assert_eq!(pixels.len() as u32, width * height, "state every pixel");
    let mut buffer = RgbaImage::new(width, height);
    for (i, (r, g, b)) in pixels.iter().enumerate() {
        let x = i as u32 % width;
        let y = i as u32 / width;
        buffer.put_pixel(x, y, Rgba([*r, *g, *b, 255]));
    }
    DynamicImage::ImageRgba8(buffer)
}

/// The symbols of a rendered picture, one string per line.
fn symbols(lines: &[Line<'_>]) -> Vec<String> {
    lines
        .iter()
        .map(|line| {
            line.spans
                .iter()
                .map(|span| span.content.as_ref())
                .collect::<String>()
        })
        .collect()
}

/// Every cell's foreground and background, row by row.
fn colours(lines: &[Line<'_>]) -> Vec<Vec<(Color, Color)>> {
    lines
        .iter()
        .map(|line| {
            line.spans
                .iter()
                .flat_map(|span| {
                    span.content.chars().map(move |_| {
                        (
                            span.style.fg.unwrap_or(Color::Reset),
                            span.style.bg.unwrap_or(Color::Reset),
                        )
                    })
                })
                .collect()
        })
        .collect()
}

#[test]
fn two_pixel_rows_become_one_cell_of_two_colours() {
    // The whole of the half-block idea in four pixels: the upper row is the
    // foreground of the glyph, the lower row is its background.
    let picture = painted(
        2,
        2,
        &[
            (255, 0, 0),
            (0, 255, 0), // upper row
            (0, 0, 255),
            (255, 255, 0), // lower row
        ],
    );
    let lines = cells(&picture, WIDE, TALL);

    assert_eq!(symbols(&lines), vec![format!("{UPPER_HALF}{UPPER_HALF}")]);
    assert_eq!(
        colours(&lines),
        vec![vec![
            (Color::Rgb(255, 0, 0), Color::Rgb(0, 0, 255)),
            (Color::Rgb(0, 255, 0), Color::Rgb(255, 255, 0)),
        ]],
        "foreground is the pixel above, background is the pixel below",
    );
}

#[test]
fn an_odd_final_row_leaves_the_background_unset_rather_than_inventing_a_pixel() {
    // Three pixel rows is one and a half cells. The half that has no pixel under
    // it must not be painted: a fabricated colour there is a line of the picture
    // that was never in the file.
    let picture = painted(1, 3, &[(255, 0, 0), (0, 255, 0), (0, 0, 255)]);
    let lines = cells(&picture, WIDE, TALL);

    assert_eq!(lines.len(), 2, "three pixel rows occupy two cells");
    let painted_colours = colours(&lines);
    assert_eq!(
        painted_colours[0],
        vec![(Color::Rgb(255, 0, 0), Color::Rgb(0, 255, 0))]
    );
    assert_eq!(
        painted_colours[1],
        vec![(Color::Rgb(0, 0, 255), Color::Reset)],
        "the missing lower pixel leaves the background alone",
    );
}

#[test]
fn a_picture_keeps_its_proportions_when_width_binds() {
    // 100x50 into a twenty-column box. Twenty columns is twenty pixels wide, so
    // the height is fifty * twenty / one hundred = ten pixel rows = FIVE cells.
    let picture = painted(100, 50, &[(1, 2, 3); 5_000]);
    let lines = cells(&picture, 20, TALL);

    assert_eq!(
        lines.len(),
        5,
        "twenty columns of a 2:1 picture is five rows"
    );
    assert_eq!(symbols(&lines)[0].chars().count(), 20);
}

#[test]
fn a_picture_keeps_its_proportions_when_height_binds() {
    // The case above cannot see the defect this test exists for. When WIDTH is
    // what binds, the height bound never enters the arithmetic, so fitting
    // against rows instead of half rows produces the same answer and the test
    // stays green over a squashed picture. Sabotaging the box height proved
    // exactly that.
    //
    // So bind on height. A 50x100 picture in eighty columns and thirty rows has a
    // box of eighty by SIXTY pixels; the scale is sixty over one hundred, giving
    // thirty pixels wide by sixty tall — thirty cells, and thirty columns wide,
    // which is the source's one-to-two ratio drawn with square half-block pixels.
    //
    // Fitting against thirty instead of sixty halves both numbers: fifteen rows
    // of fifteen columns. Still square-looking, still plausible, and half the
    // picture's size.
    let picture = painted(50, 100, &[(1, 2, 3); 5_000]);
    let lines = cells(&picture, 80, 30);

    assert_eq!(lines.len(), 30, "the picture uses the rows it was given");
    assert_eq!(
        symbols(&lines)[0].chars().count(),
        30,
        "thirty cells wide: half-block pixels are square, so a 1:2 source drawn \
         thirty rows tall is thirty columns wide",
    );
}

#[test]
fn a_picture_is_fitted_to_the_terminal_and_never_wider_than_it() {
    // ratatui clips a row rather than complaining, and this product has paid for
    // that three times. A picture is fitted here, so there is nothing to clip.
    let picture = painted(400, 400, &[(9, 9, 9); 160_000]);
    let lines = cells(&picture, 80, TALL);

    for row in symbols(&lines) {
        assert!(
            row.chars().count() <= 80,
            "a row is {} cells wide in an eighty-column terminal",
            row.chars().count(),
        );
    }
    assert!(!lines.is_empty());
}

#[test]
fn a_tall_picture_is_bounded_by_the_rows_it_is_given() {
    // A committed picture scrolls the conversation off the screen, so height is
    // bounded even when width is not the binding constraint.
    let picture = painted(10, 4_000, &[(9, 9, 9); 40_000]);
    let lines = cells(&picture, 80, 12);

    assert_eq!(lines.len(), 12, "the row bound is what stops it");
}

#[test]
fn a_one_pixel_picture_still_renders_a_cell() {
    // The degenerate case: a picture that fits in less than one cell must not
    // round to nothing, because zero rows through `insert_before` is a commit
    // that silently drops the content.
    let picture = painted(1, 1, &[(7, 7, 7)]);
    let lines = cells(&picture, 80, 12);

    assert_eq!(lines.len(), 1);
    assert_eq!(
        colours(&lines)[0],
        vec![(Color::Rgb(7, 7, 7), Color::Reset)]
    );
}

#[test]
fn the_plain_form_names_the_file_and_paints_nothing() {
    // Under `--plain` or NO_COLOR a half-block picture is colour carrying the
    // whole meaning, which §4 of the design forbids. What is left is a sentence.
    let line = describe("shot.png", "image/png", 800, 600);
    let text: String = line.spans.iter().map(|s| s.content.as_ref()).collect();

    assert!(text.contains("shot.png"), "{text}");
    assert!(text.contains("800"), "{text}");
    assert!(text.contains("600"), "{text}");
    assert!(text.contains("png"), "{text}");
}

#[test]
fn decoding_reports_the_dimensions_the_plain_form_needs() {
    // The plain form's numbers come from the decoder, not from a second read of
    // the file: one source for the size, so the sentence and the picture cannot
    // disagree.
    let png = support::png_bytes(3, 2);
    let picture = decode(&png).expect("a png this crate declared a decoder for");

    assert_eq!((picture.width(), picture.height()), (3, 2));
}

#[test]
fn a_file_that_is_not_an_image_is_an_error_and_not_a_panic() {
    // io-harness refuses by media type before this is reached, but a file whose
    // extension lies is still a real input, and a decoder panic would take the
    // session down with it.
    assert!(decode(b"this is not an image").is_err());
}

/// F4 — where the terminal speaks the Kitty graphics protocol, the bytes are the
/// image; and where it does not, there is no escape at all.
///
/// The negative half carries more of this criterion than the positive one. A
/// wrong cell is a wrong frame; a graphics escape sent to a terminal that cannot
/// read it is unreadable bytes written permanently into scrollback that no later
/// redraw can clean.
mod graphics {
    use io_cli::picture::{kitty, render, Drawn};
    use io_cli::term::speaks_kitty_graphics;

    /// An environment lookup built from pairs, so no test mutates the process it
    /// runs in — and so every branch is reachable, including the ones this
    /// machine could never produce.
    fn env<'a>(pairs: &'a [(&'a str, &'a str)]) -> impl Fn(&str) -> Option<String> + 'a {
        move |name| {
            pairs
                .iter()
                .find(|(key, _)| *key == name)
                .map(|(_, value)| (*value).to_string())
        }
    }

    #[test]
    fn the_terminals_that_speak_it_are_recognised() {
        for pairs in [
            vec![("TERM", "xterm-kitty")],
            vec![("KITTY_WINDOW_ID", "1")],
            vec![("TERM", "xterm-256color"), ("TERM_PROGRAM", "ghostty")],
            vec![("TERM", "xterm-256color"), ("TERM_PROGRAM", "WezTerm")],
            vec![("KONSOLE_VERSION", "230800")],
            vec![("GHOSTTY_RESOURCES_DIR", "/usr/share/ghostty")],
        ] {
            assert!(
                speaks_kitty_graphics(env(&pairs)),
                "should be recognised: {pairs:?}",
            );
        }
    }

    #[test]
    fn an_unknown_terminal_gets_no_escape() {
        for pairs in [
            vec![("TERM", "xterm-256color")],
            vec![("TERM", "vt100")],
            vec![("TERM_PROGRAM", "Apple_Terminal")],
            // iTerm2 speaks its own protocol and not this one. 0.9.0 gives it
            // cells; US-IO-CLI-0.9.0-I01 records why.
            vec![("TERM_PROGRAM", "iTerm.app")],
            vec![],
        ] {
            assert!(
                !speaks_kitty_graphics(env(&pairs)),
                "should NOT be sent an escape: {pairs:?}",
            );
        }
    }

    #[test]
    fn a_multiplexer_is_refused_even_when_the_terminal_under_it_speaks_it() {
        // This is the case that does the damage. Kitty graphics inside tmux need
        // a passthrough that is off by default, and screen has no equivalent at
        // all — so a real kitty underneath is exactly when the escape is most
        // likely to be emitted and least likely to be drawn.
        for pairs in [
            vec![("TERM", "xterm-kitty"), ("TMUX", "/tmp/tmux-501/default")],
            vec![("KITTY_WINDOW_ID", "1"), ("TMUX", "/tmp/x")],
            vec![("TERM", "screen.xterm-kitty")],
            vec![("TERM", "tmux-256color"), ("KITTY_WINDOW_ID", "1")],
            vec![("TERM", "xterm-kitty"), ("STY", "1234.pts-0")],
        ] {
            assert!(
                !speaks_kitty_graphics(env(&pairs)),
                "a multiplexer is between us and the terminal: {pairs:?}",
            );
        }
    }

    #[test]
    fn the_escape_carries_the_flag_that_makes_it_compose() {
        // `C=1` is what tells Kitty not to move the cursor, and that is the whole
        // reason this composes with a renderer that draws the cells around it.
        // Without it the placement moves the cursor and may scroll, which changes
        // what every later absolute MoveTo in the same draw means.
        let escape = kitty("QUJD", 4, 2);

        assert!(escape.starts_with("\x1b_G"), "{escape:?}");
        assert!(escape.contains("a=T"), "{escape:?}");
        assert!(escape.contains("f=100"), "{escape:?}");
        assert!(escape.contains("C=1"), "{escape:?}");
        assert!(escape.contains("c=4"), "{escape:?}");
        assert!(escape.contains("r=2"), "{escape:?}");
        assert!(
            escape.contains("m=0"),
            "one chunk is the last chunk: {escape:?}"
        );
        assert!(escape.ends_with("\x1b\\"), "{escape:?}");
        assert!(
            escape.contains("QUJD"),
            "the payload is carried: {escape:?}"
        );
    }

    #[test]
    fn a_payload_longer_than_one_escape_is_chunked_with_the_control_keys_on_the_first() {
        // The protocol's rule: control keys ride the first escape only, every
        // chunk but the last says `m=1`, and the last says `m=0`.
        let payload = "A".repeat(10_000);
        let escape = kitty(&payload, 10, 5);

        assert_eq!(escape.matches("a=T").count(), 1, "control keys ride once");
        assert_eq!(
            escape.matches("\x1b_G").count(),
            3,
            "10000 bytes is three chunks"
        );
        assert_eq!(escape.matches("m=1").count(), 2);
        assert_eq!(escape.matches("m=0").count(), 1);
        assert!(escape.ends_with("\x1b\\"));
    }

    #[test]
    fn a_png_reaches_the_graphics_path_and_a_terminal_without_it_gets_cells() {
        let png = super::support::png_bytes(8, 4);

        let with = render(&png, "shot.png", "image/png", true, true, 80);
        assert!(
            matches!(with, Drawn::Kitty { .. }),
            "a png on a terminal that speaks the protocol is the real image",
        );

        let without = render(&png, "shot.png", "image/png", true, false, 80);
        let Drawn::Lines(lines) = without else {
            panic!("a terminal that does not speak the protocol must get cells")
        };
        assert!(!lines.is_empty());
    }

    #[test]
    fn the_plain_form_beats_a_terminal_that_speaks_the_protocol() {
        // `--plain`, `NO_COLOR` and the ASCII glyph set are each a reason a reader
        // wants no picture. A protocol the terminal happens to speak does not
        // override any of them.
        let png = super::support::png_bytes(8, 4);
        let drawn = render(&png, "shot.png", "image/png", false, true, 80);

        let Drawn::Lines(lines) = drawn else {
            panic!("plain mode must not emit a graphics escape")
        };
        assert_eq!(lines.len(), 1, "one line naming the file");
    }
}
