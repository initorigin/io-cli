//! Rendering an edit the agent made.
//!
//! **io-cli computes no diff.** io-harness renders a unified diff for every edit
//! its tools make and keeps it in the run's durable trace, so what this module
//! draws is text that already exists — `io_harness::Edit::hunk`, a hunk body
//! whose `@@` line numbers are the *file's* rather than the fragment's. That is
//! the difference between a diff this product can show and one it could only
//! approximate: io-cli never sees the file, so it could not number a hunk
//! correctly even if it wanted to compute one.
//!
//! Three decisions shape everything here.
//!
//! **The hunk is passed through, never reconstructed.** Its markers, its
//! spacing and its header are the harness's; this module decides colour and
//! nothing else. A reader who copies a rendered line into `patch` gets what
//! `patch` expects.
//!
//! **An absent hunk is a fact, not an empty one.** `Edit::hunk` is `None` when
//! the row predates the harness release that added hunks, when the file's
//! previous contents were not kept (over the store's snapshot cap, or not
//! UTF-8), or when the rendered diff would itself have exceeded that cap. Not
//! one of those is "nothing changed" — and `lines_added` and `lines_removed`
//! are still there to prove it — so the cell says the counts and says the diff
//! is missing, rather than drawing an empty body that reads as an untouched
//! file.
//!
//! **Green means added, and it never also means "this is a string".** The
//! obvious way to combine syntax highlighting with a diff is to colour the whole
//! changed line green or red, which leaves no colour for the code, or to syntax
//! colour the whole line, which leaves no colour for the change. Neither is
//! taken. The marker keeps the diff's colour, the parts of a line that both
//! sides share are syntax coloured, and **the words that actually changed are
//! drawn in the diff's colour and emphasised** — so the add/remove colour is
//! still on screen and is now pointing at the exact words rather than washing
//! the line. A line with no partner to compare against gets the whole wash,
//! because on that line everything did change.

use std::ops::Range;
use std::sync::OnceLock;

use io_harness::Edit;
use ratatui::text::{Line, Span};
use syntect::easy::ScopeRangeIterator;
use syntect::parsing::{ParseState, Scope, ScopeStack, SyntaxReference, SyntaxSet};

use crate::settings::DiffStyle;
use crate::theme::{Theme, Tone};

/// What every line of the cell is indented by, so a hunk sits under its header
/// the way a tool call sits under a step.
const INDENT: &str = "  ";

/// The most body lines one cell will draw.
///
/// A hunk is capped by the harness at a mebibyte, which is on the order of
/// twenty-six thousand lines, and two separate things go wrong long before that.
/// Highlighting is a fresh parse per line and runs synchronously on the loop that
/// also delivers keystrokes, so a large hunk makes `Ctrl+C` unreachable for as
/// long as it takes. And `Screen::commit` hands `insert_before` a `u16` height,
/// which silently clamps: past sixty-five thousand rows the content is truncated
/// rather than wrapped, and the buffer it allocates first is the terminal's width
/// times that height.
///
/// Five hundred is far more of a change than anybody reads in a terminal and far
/// less than either failure. What is cut is said, never dropped quietly — the
/// whole change is still in the trace, and `/copy diff` still carries all of it.
pub const MAX_BODY_LINES: usize = 500;

/// The width below which word-level emphasis drops to the line.
///
/// A hunk line is committed into the terminal's own scrollback, where a line
/// wider than the terminal wraps rather than truncating — nothing is lost. What
/// is lost is *findability*: a bolded fragment in the middle of a line that now
/// occupies three rows is harder to locate than a whole row that is red. So
/// below a hundred columns the emphasis is the line, which is the same
/// information at a scale the terminal can still show.
///
/// A hundred rather than eighty because the floor is about the wrap, not about
/// the supported size: an eighty-column terminal is supported, and a
/// ninety-column one wraps just as badly.
pub const EMPHASIS_FLOOR: u16 = 100;

/// The syntax highlighter, built once and only if a diff is ever drawn.
///
/// **Never on the startup path.** `SyntaxSet::load_defaults_newlines`
/// decompresses a dump of every grammar syntect ships, which is real work and
/// real memory, and this product's own bar is a first paint inside a hundred
/// milliseconds. A session that never edits a file never pays for it — which is
/// most sessions, since a question does not touch the workspace.
///
/// A `OnceLock` rather than a field on anything: the set is immutable once
/// built, every diff in the session shares it, and threading it through [`cell`]
/// would put a parameter on a function whose callers have no business knowing a
/// highlighter exists.
static HIGHLIGHTER: OnceLock<Highlighter> = OnceLock::new();

/// The grammar set, plus the four scopes worth colouring.
struct Highlighter {
    syntaxes: SyntaxSet,
    /// Matched by prefix, so `keyword` catches `keyword.control.rust` and every
    /// other language's spelling of the same idea without a table per language.
    keyword: Scope,
    /// `storage`, and it is not redundant with `keyword`. Sublime's scope
    /// vocabulary — which syntect's grammars are written in — files `let`,
    /// `const`, `fn`, `struct` and `pub` under `storage.type` and
    /// `storage.modifier`, leaving `keyword` for control flow and operators. A
    /// table without this entry highlights `=` and misses every declaration on
    /// the line, which is the opposite of what a reader wants. Found by a test,
    /// not by reading the vocabulary.
    storage: Scope,
    string: Scope,
    comment: Scope,
    constant: Scope,
}

impl Highlighter {
    fn get() -> &'static Self {
        HIGHLIGHTER.get_or_init(|| Self {
            syntaxes: SyntaxSet::load_defaults_newlines(),
            // `Scope::new` fails only on a malformed selector and these four are
            // literals in this file. The fallback is the empty scope, which is a
            // prefix of everything — so the impossible case colours a line
            // wrongly rather than panicking inside a renderer.
            keyword: Scope::new("keyword").unwrap_or_default(),
            storage: Scope::new("storage").unwrap_or_default(),
            string: Scope::new("string").unwrap_or_default(),
            comment: Scope::new("comment").unwrap_or_default(),
            constant: Scope::new("constant").unwrap_or_default(),
        })
    }

    /// The tone for the scope stack a token was parsed under.
    ///
    /// Read from the top of the stack downwards, because the innermost scope is
    /// the specific one: a keyword inside a string is a string.
    fn tone(&self, stack: &ScopeStack) -> Tone {
        for scope in stack.as_slice().iter().rev() {
            if self.comment.is_prefix_of(*scope) {
                return Tone::Muted;
            }
            if self.string.is_prefix_of(*scope) {
                return Tone::StringLiteral;
            }
            if self.constant.is_prefix_of(*scope) {
                return Tone::Literal;
            }
            if self.keyword.is_prefix_of(*scope) || self.storage.is_prefix_of(*scope) {
                return Tone::Keyword;
            }
        }
        Tone::Normal
    }

    /// `text` split into byte runs that share a tone.
    ///
    /// **A hunk is a fragment of a file, and this parses each line from a clean
    /// state.** A block comment or a multi-line string opened above the hunk is
    /// therefore not known here and its lines read as code. Carrying state would
    /// mean two parsers — one walking the old side of the diff and one the new —
    /// fed an interleaved stream, for at most seven lines of context. The
    /// ceiling is worth naming and is not worth the machinery.
    fn runs(&self, text: &str, syntax: &SyntaxReference) -> Vec<(Range<usize>, Tone)> {
        // The grammars are the newline-terminated set, so a line without its
        // terminator can leave a rule unmatched at the end.
        let terminated = format!("{text}\n");
        let mut state = ParseState::new(syntax);
        let Ok(ops) = state.parse_line(&terminated, &self.syntaxes) else {
            return vec![(0..text.len(), Tone::Normal)];
        };

        let mut stack = ScopeStack::new();
        let mut runs: Vec<(Range<usize>, Tone)> = Vec::new();
        let mut at = 0;
        for (range, op) in ScopeRangeIterator::new(&ops, &terminated) {
            if stack.apply(op).is_err() {
                return vec![(0..text.len(), Tone::Normal)];
            }
            // Clamped: the newline added above is not part of the caller's text.
            let end = range.end.min(text.len());
            if end <= at {
                continue;
            }
            let tone = self.tone(&stack);
            match runs.last_mut() {
                Some((last, seen)) if *seen == tone => last.end = end,
                _ => runs.push((at..end, tone)),
            }
            at = end;
        }
        if at < text.len() {
            runs.push((at..text.len(), Tone::Normal));
        }
        if runs.is_empty() {
            runs.push((0..text.len(), Tone::Normal));
        }
        runs
    }
}

/// The grammar for a path, by extension, or `None` when nothing claims it.
///
/// `None` is not a failure. A diff of a file syntect has no grammar for renders
/// in the terminal's own foreground, which is what it should look like.
fn syntax_for(path: &str) -> Option<&'static SyntaxReference> {
    let extension = std::path::Path::new(path).extension()?.to_str()?;
    Highlighter::get()
        .syntaxes
        .find_syntax_by_extension(extension)
}

/// The edits a given step made, out of everything the run recorded.
///
/// `Store::edits` answers for the whole run, so a caller that draws what it
/// returns re-renders every earlier edit at every later step — the transcript
/// grows quadratically and the same diff appears four times. Kept here rather
/// than in the driver so that it is reachable from a test; the driver is a
/// binary and an integration test cannot link one.
pub fn for_step(edits: Vec<Edit>, step: u32) -> Vec<Edit> {
    edits.into_iter().filter(|edit| edit.step == step).collect()
}

/// One edit, as lines for the terminal's scrollback.
///
/// The header first — the path, then the counts, then the tool — because the
/// path is the content and the rest is metadata, which is the rule the whole
/// interface follows.
pub fn cell(edit: &Edit, theme: &Theme, width: u16) -> Vec<Line<'static>> {
    cell_styled(edit, theme, width, DiffStyle::Unified)
}

/// The same, at a chosen style.
pub fn cell_styled(edit: &Edit, theme: &Theme, width: u16, style: DiffStyle) -> Vec<Line<'static>> {
    let glyphs = &theme.glyphs;
    let mut lines = vec![header(edit, theme)];

    let Some(hunk) = edit.hunk.as_deref() else {
        // The header already carried the absence. Returning one line here is not
        // a shortcut: a body of zero lines under a header is exactly the empty
        // diff this must not draw.
        return lines;
    };

    let mut drawn = body(hunk, &edit.path, theme, width >= EMPHASIS_FLOOR, style);
    let cut = drawn.len().saturating_sub(MAX_BODY_LINES);
    if cut > 0 {
        drawn.truncate(MAX_BODY_LINES);
        let (elision, dash) = (glyphs.elision, glyphs.dash);
        drawn.push(Line::from(Span::styled(
            format!("{INDENT}{elision} {cut} more lines of this change {dash} the whole of it is in the trace, and `/copy diff` carries it"),
            theme.style(Tone::Muted),
        )));
    }
    lines.extend(drawn);
    lines.push(Line::from(""));
    lines
}

/// `  src/theme.rs · +1 -1 · edit_file`, and `· no diff stored` when there is
/// no hunk to draw under it.
fn header(edit: &Edit, theme: &Theme) -> Line<'static> {
    let separator = theme.glyphs.separator;
    let mut spans = vec![
        Span::styled(INDENT.to_string(), theme.style(Tone::Muted)),
        Span::styled(edit.path.clone(), theme.style(Tone::Accent)),
        Span::styled(separator.to_string(), theme.style(Tone::Muted)),
        Span::styled(format!("+{}", edit.lines_added), theme.style(Tone::Added)),
        Span::styled(" ".to_string(), theme.style(Tone::Muted)),
        Span::styled(
            format!("-{}", edit.lines_removed),
            theme.style(Tone::Removed),
        ),
        Span::styled(
            format!("{separator}{}", edit.tool),
            theme.style(Tone::Muted),
        ),
    ];
    if edit.hunk.is_none() {
        spans.push(Span::styled(
            format!("{separator}no diff stored"),
            theme.style(Tone::Muted),
        ));
    }
    Line::from(spans)
}

/// A hunk body, with the words that changed emphasised where they can be known.
///
/// **The pairing rule is deliberately timid.** A run of removals is paired with
/// the run of additions immediately after it, and only when the two runs are the
/// same length — then line *n* of one is compared with line *n* of the other.
/// Anything else is drawn without a pairing, which means the whole line reads as
/// changed.
///
/// The reason is in io-harness's own diff module: an `edit_file` is one
/// contiguous replacement by construction, but a `write_file` that rewrote two
/// distant regions arrives as **one hunk spanning both**. A greedy rule would
/// then emphasise the difference between lines that have nothing to do with each
/// other, which is worse than no emphasis — it invents a relationship and paints
/// it.
fn body(
    hunk: &str,
    path: &str,
    theme: &Theme,
    emphasis: bool,
    style: DiffStyle,
) -> Vec<Line<'static>> {
    let syntax = syntax_for(path);
    let raw: Vec<&str> = hunk.lines().collect();
    let mut out = Vec::new();
    let mut at = 0;

    while at < raw.len() {
        // A `\ No newline at end of file` marker is emitted on its own line
        // immediately after the line it applies to, so it can land BETWEEN a
        // removal and its addition. Scanned through rather than treated as the
        // end of a run: otherwise the additions run comes back empty, the pair is
        // never made, and the word-level emphasis is lost on exactly the lines
        // where a file's last line changed.
        let marker = |line: &str| line.starts_with('\\');

        let removals_from = at;
        while at < raw.len()
            && (raw[at].starts_with('-') || (at > removals_from && marker(raw[at])))
        {
            at += 1;
        }
        let removals = &raw[removals_from..at];

        let additions_from = at;
        while at < raw.len()
            && (raw[at].starts_with('+') || (at > additions_from && marker(raw[at])))
        {
            at += 1;
        }
        let additions = &raw[additions_from..at];

        if removals.is_empty() && additions.is_empty() {
            // `minimal` keeps the `@@` header — a change with no line numbers is a
            // change that does not say where it is — and drops the context, which
            // is the only thing it drops.
            let keep = style == DiffStyle::Unified || raw[at].starts_with("@@");
            if keep {
                out.push(unchanged(raw[at], syntax, theme));
            }
            at += 1;
            continue;
        }

        // Below the emphasis floor nothing is paired, so every changed line takes
        // the whole wash — see `EMPHASIS_FLOOR`.
        // Pairing counts only the changed lines; a marker is punctuation about
        // the bytes and has no partner on the other side.
        let removed: Vec<&&str> = removals.iter().filter(|l| !marker(l)).collect();
        let added: Vec<&&str> = additions.iter().filter(|l| !marker(l)).collect();
        let paired = emphasis && !removed.is_empty() && removed.len() == added.len();

        let mut emit = |lines: &[&str], partners: &[&&str], tone| {
            let mut nth = 0;
            for line in lines {
                if marker(line) {
                    out.push(unchanged(line, syntax, theme));
                    continue;
                }
                let other = paired.then(|| *partners[nth]);
                out.push(changed(line, other, tone, syntax, theme));
                nth += 1;
            }
        };
        emit(removals, &added, Tone::Removed);
        emit(additions, &removed, Tone::Added);
    }

    out
}

/// A line that is not an addition or a removal: context, a `@@` header, or the
/// no-final-newline marker.
fn unchanged(line: &str, syntax: Option<&SyntaxReference>, theme: &Theme) -> Line<'static> {
    match line.as_bytes().first() {
        // `@@ … @@` — where in the file this is. The one part of a hunk a reader
        // navigates by, so it takes the product's own colour and no highlighting:
        // it is not code.
        Some(b'@') => Line::from(Span::styled(
            format!("{INDENT}{line}"),
            theme.style(Tone::Accent),
        )),
        // `\ No newline at end of file`. Real information about the bytes, and
        // not a change, so it reads as neither.
        Some(b'\\') => Line::from(Span::styled(
            format!("{INDENT}{line}"),
            theme.style(Tone::Muted),
        )),
        // A context line. Its leading space is part of the diff and stays.
        _ => {
            let (marker, text) = line.split_at(usize::from(!line.is_empty()));
            spans(marker, text, Tone::Normal, None, syntax, theme)
        }
    }
}

/// One changed line, against its partner on the other side where there is one.
///
/// With a partner, the shared head and tail are syntax coloured and only the
/// middle takes the diff's colour. Without one — or when the two share neither a
/// head nor a tail, in which case "what changed" is the whole line — the body
/// takes the diff's colour throughout.
fn changed(
    mine: &str,
    other: Option<&str>,
    tone: Tone,
    syntax: Option<&SyntaxReference>,
    theme: &Theme,
) -> Line<'static> {
    // The marker is the first byte and is ASCII, so this never splits a
    // character. It stays on the line — it is what carries the meaning when
    // colour does not.
    let (marker, text) = mine.split_at(1);

    let range = other.and_then(|other| {
        let other = &other[1..];
        let head = common_head(text, other);
        // Measured on what is left after the head, so `a` against `aa` cannot
        // count the same byte twice and produce a middle of negative length.
        let tail = common_tail(&text[head..], &other[head..]);
        // Sharing neither a head nor a tail means the whole line changed, which
        // is the same situation as having no partner at all.
        (head != 0 || tail != 0).then(|| head..text.len() - tail)
    });

    let Some(range) = range else {
        // No partner, so nothing to emphasise *against* — the whole body takes
        // the diff's colour and no emphasis. Bolding a whole line says "look
        // here" about every word on it, which is the same as saying nothing.
        return Line::from(Span::styled(
            format!("{INDENT}{marker}{text}"),
            theme.style(tone),
        ));
    };

    spans(marker, text, tone, Some(range), syntax, theme)
}

/// Build a body line: the marker in `tone`, the text syntax coloured, and
/// `changed` — where there is one — in `tone` and emphasised.
fn spans(
    marker: &str,
    text: &str,
    tone: Tone,
    changed: Option<Range<usize>>,
    syntax: Option<&SyntaxReference>,
    theme: &Theme,
) -> Line<'static> {
    let mut out = vec![Span::styled(format!("{INDENT}{marker}"), theme.style(tone))];

    let runs = match syntax {
        Some(syntax) => Highlighter::get().runs(text, syntax),
        // No grammar for this file, so every run is one run.
        None => vec![(0..text.len(), Tone::Normal)],
    };

    for (run, run_tone) in runs {
        let mut at = run.start;
        while at < run.end {
            // Three cases, and the loop advances in every one: before the change,
            // inside it, or past it.
            let (end, inside) = match &changed {
                Some(range) if at < range.start => (run.end.min(range.start), false),
                Some(range) if at < range.end => (run.end.min(range.end), true),
                _ => (run.end, false),
            };
            if end <= at {
                break;
            }
            let piece = text[at..end].to_string();
            out.push(match inside {
                true => Span::styled(piece, theme.emphasis(tone)),
                false => Span::styled(piece, theme.style(run_tone)),
            });
            at = end;
        }
    }

    // A changed line whose text is empty still needs its marker to be a line.
    Line::from(out)
}

/// How many bytes `a` and `b` agree on from the front, always on a character
/// boundary.
fn common_head(a: &str, b: &str) -> usize {
    let mut bytes = 0;
    for (one, two) in a.chars().zip(b.chars()) {
        if one != two {
            break;
        }
        bytes += one.len_utf8();
    }
    bytes
}

/// The same from the back.
fn common_tail(a: &str, b: &str) -> usize {
    let mut bytes = 0;
    for (one, two) in a.chars().rev().zip(b.chars().rev()) {
        if one != two {
            break;
        }
        bytes += one.len_utf8();
    }
    bytes
}
