//! N5, O16, O17 — the viewport is the size of what it has to show.
//!
//! **This is the property 0.32.0 stands on.** Four scope lines rest on the inline
//! viewport being able to grow and shrink while a session is running: the question
//! overlay's composer, the plan overlay's pinned footer, the queue drawn in full,
//! and every picker's elision. If a growth duplicates or loses a committed row,
//! none of them can ship in the form the contract describes.
//!
//! The release's own planning called this untried ground. It is not — the composer
//! has grown the viewport since 0.7.0, through `App::viewport_wanted` and
//! `Screen::replace` — but it had never been *tested*, because `Screen::replace`
//! builds its replacement with `Screen::attach_with`, which enables raw mode and
//! queries a real tty, and so lives on the stdout-backed impl where nothing under
//! `tests/` can reach it. `Screen::replace_from` takes the constructor as an
//! argument so this file can run the real sequence — the erase, the restore, the
//! re-attach, the fall back to the session's height — against the recorder.
//!
//! Every assertion here is over the **byte stream**, not over a rendered buffer. A
//! duplicated row is a duplicated write; a lost row is a write that never
//! happened. Neither is visible in a cell grid, which is the same reason
//! `tests/support` uses `Fixed` over a real `CrosstermBackend` rather than
//! `TestBackend`.

mod support;

use ratatui::text::Line;
use ratatui::widgets::Paragraph;

use io_cli::term::VIEWPORT_HEIGHT;

/// The composer, drawn into the viewport. Deliberately carries none of the
/// markers the assertions count, so an occurrence in the byte stream is always a
/// commit and never a frame.
fn frame(screen: &mut io_cli::term::Screen<support::Fixed>) {
    screen
        .draw(|frame| {
            let area = frame.area();
            frame.render_widget(Paragraph::new("> "), area);
        })
        .expect("frame");
}

fn commit(screen: &mut io_cli::term::Screen<support::Fixed>, marker: &str) {
    screen.commit(&[Line::from(marker)]).expect("commit");
    frame(screen);
}

/// How many times `needle` appears in everything written to the terminal.
fn written(recorder: &support::Recorder, needle: &str) -> usize {
    recorder.text().matches(needle).count()
}

/// Every marker committed appears in the byte stream exactly once — not zero
/// times, which is a lost row, and not twice, which is a duplicated one.
fn each_committed_once(recorder: &support::Recorder, markers: &[&str]) {
    for marker in markers {
        let seen = written(recorder, marker);
        assert_eq!(
            seen, 1,
            "{marker:?} was written {seen} times; a committed row must reach the \
             scrollback exactly once across a viewport re-placement",
        );
    }
}

#[test]
fn n5_a_growth_neither_duplicates_nor_loses_committed_rows() {
    let (mut screen, recorder) = support::screen(100, 30);

    commit(&mut screen, "before-the-growth-alpha");
    commit(&mut screen, "before-the-growth-beta");

    // The question overlay asking for room: eight rows becomes twenty.
    support::replace(&mut screen, &recorder, 100, 30, 20);
    assert_eq!(screen.rows(), 20, "the viewport did not take the rows it asked for");

    commit(&mut screen, "after-the-growth-gamma");

    each_committed_once(
        &recorder,
        &[
            "before-the-growth-alpha",
            "before-the-growth-beta",
            "after-the-growth-gamma",
        ],
    );
}

#[test]
fn n5_a_shrink_back_to_the_floor_neither_duplicates_nor_loses() {
    let (mut screen, recorder) = support::screen(100, 30);

    commit(&mut screen, "committed-at-the-floor");
    support::replace(&mut screen, &recorder, 100, 30, 20);
    commit(&mut screen, "committed-while-grown");

    // The overlay closes. This is the direction that matters most: the grown
    // viewport occupied rows the shrunken one does not, and anything left
    // standing in them is a row the operator sees twice.
    support::replace(&mut screen, &recorder, 100, 30, VIEWPORT_HEIGHT);
    assert_eq!(
        screen.rows(),
        VIEWPORT_HEIGHT,
        "the viewport did not return to its floor when the surface closed",
    );

    commit(&mut screen, "committed-after-the-shrink");

    each_committed_once(
        &recorder,
        &[
            "committed-at-the-floor",
            "committed-while-grown",
            "committed-after-the-shrink",
        ],
    );
}

#[test]
fn n5_a_terminal_resize_while_grown_neither_duplicates_nor_loses() {
    let (mut screen, recorder) = support::screen(100, 30);

    commit(&mut screen, "committed-before-anything-moved");
    support::replace(&mut screen, &recorder, 100, 30, 20);
    commit(&mut screen, "committed-while-grown-at-thirty");

    // The window is dragged narrower and shorter while an overlay is open. The
    // committed lines above belong to the terminal and must not be redrawn — the
    // duplicated history a full-screen renderer shows on resize is exactly what
    // `Screen::resize` exists to avoid, and a grown viewport must not reintroduce
    // it.
    support::resize(&mut screen, 80, 24);
    frame(&mut screen);
    commit(&mut screen, "committed-after-the-resize");

    each_committed_once(
        &recorder,
        &[
            "committed-before-anything-moved",
            "committed-while-grown-at-thirty",
            "committed-after-the-resize",
        ],
    );
}

#[test]
fn n5_a_surface_opening_while_another_is_grown_neither_duplicates_nor_loses() {
    let (mut screen, recorder) = support::screen(100, 30);

    commit(&mut screen, "committed-before-the-first-surface");

    // A question overlay opens and takes twelve rows.
    support::replace(&mut screen, &recorder, 100, 30, 12);
    commit(&mut screen, "committed-under-the-first-surface");

    // A picker opens on top of it and wants more. The viewport goes straight from
    // one grown height to another without passing through the floor, which is the
    // case a re-placement written as "shrink then grow" would never exercise.
    support::replace(&mut screen, &recorder, 100, 30, 20);
    commit(&mut screen, "committed-under-the-second-surface");

    // Both close at once.
    support::replace(&mut screen, &recorder, 100, 30, VIEWPORT_HEIGHT);
    commit(&mut screen, "committed-after-both-closed");

    each_committed_once(
        &recorder,
        &[
            "committed-before-the-first-surface",
            "committed-under-the-first-surface",
            "committed-under-the-second-surface",
            "committed-after-both-closed",
        ],
    );
}

#[test]
fn n5_the_erase_precedes_the_replacement_and_starts_at_the_viewport_top() {
    // The ordering the whole property rests on. `Screen::replace_from` erases from
    // the viewport's own top row down before it lets go of the terminal, because
    // those rows are the screen and not the scrollback: nothing scrolls them away
    // and nothing repaints them once the old `Screen` is gone. Without the erase
    // the next viewport is placed at the cursor and draws OVER the old rows.
    //
    // Asserted as bytes because that is what it is: a CUP to the top row, then
    // ESC[0J. A rendered buffer cannot show it.
    let (mut screen, recorder) = support::screen(100, 30);
    commit(&mut screen, "a-committed-row");

    let top = screen.terminal_mut().get_frame().area().y.saturating_add(1);
    support::replace(&mut screen, &recorder, 100, 30, 20);

    let expected = format!("\x1b[{top};1H\x1b[0J");
    assert!(
        recorder.text().contains(&expected),
        "the viewport was replaced without erasing itself first; expected {expected:?} \
         in the byte stream",
    );
}

#[test]
fn o17_a_viewport_never_exceeds_what_the_terminal_can_give() {
    // Growth is a request, not a guarantee. A surface that asks for more rows than
    // the terminal has must degrade rather than overflow — 80x24 is a supported
    // size, not a degraded one.
    let (mut screen, recorder) = support::screen(80, 24);
    commit(&mut screen, "committed-on-a-small-terminal");

    support::replace(&mut screen, &recorder, 80, 24, 100);

    let rows = screen.rows();
    assert!(
        rows <= 24,
        "the viewport took {rows} rows on a 24-row terminal",
    );
    each_committed_once(&recorder, &["committed-on-a-small-terminal"]);
}
