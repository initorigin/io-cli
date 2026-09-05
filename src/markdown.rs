//! The model's own markdown, rendered rather than printed.
//!
//! Models answer in markdown whether or not anything asked them to, so a
//! transcript that commits the text verbatim shows `## Layout`, `**Binary**` and
//! `` `src/main.rs` `` to somebody who wanted a heading, a bold word and a path.
//! That is the same defect as printing `prompt_composed`: the reader is being
//! handed the notation instead of the thing.
//!
//! **What this is not.** It is not a markdown parser and does not want to be. It
//! is a line-at-a-time renderer over the handful of constructs a model actually
//! uses in an answer — headings, bullets, quotes, rules, fenced code, and inline
//! bold, italic and code. Anything it does not recognise is left exactly as the
//! model wrote it, which is the only safe direction: a renderer that guessed
//! would silently eat characters out of an answer.
//!
//! A line at a time because that is how the transcript commits — a finished line
//! belongs to the terminal the moment it arrives, and nothing here may wait for a
//! document to end. Only the fenced-code state spans lines, and that is why this
//! is a struct rather than a function.

use ratatui::style::Modifier;
use ratatui::text::{Line, Span};

use crate::theme::{Theme, Tone};

/// Renders one line of the model's markdown, remembering what spans lines.
#[derive(Debug, Default, Clone, Copy)]
pub struct Markdown {
    /// Whether the last line opened a fence that nothing has closed.
    ///
    /// The one piece of state here. Everything inside a fence is the model's
    /// text exactly as written — no bold, no headings, no bullets — because
    /// inside a fence those characters are code and not notation.
    fenced: bool,
}

impl Markdown {
    /// Forget any fence left open. Called where a conversation ends.
    pub fn forget(&mut self) {
        self.fenced = false;
    }

    /// One line of markdown as the spans a terminal should draw.
    pub fn line(&mut self, text: &str, theme: &Theme) -> Line<'static> {
        let trimmed = text.trim_start();
        let indent = &text[..text.len() - trimmed.len()];

        // A fence opens and closes on its own line, and the line itself is not
        // content — so it draws a blank row, which is where the code begins and
        // ends.
        //
        // **The language tag used to be drawn on that row and it read as content
        // (0.38.1).** An opening ```` ```python ```` committed the bare word
        // `python`, muted, on the line above the code. The intent was to show a
        // reader where the block starts; what a reader saw was a one-word
        // paragraph, indistinguishable from the model having written the word
        // `python` on its own line — which is what the 2026-09-05 field test
        // reported it as. Muting is not a distinction at that width: a short
        // muted line and a short prose line are the same shape.
        //
        // Dropped rather than decorated, because the boundary was never carried
        // by the word. Everything inside the fence is drawn `Tone::Literal` and
        // everything outside it is not, so where the code begins and ends is
        // already on the screen in the only way that survives a narrow terminal,
        // `--plain`, and `NO_COLOR`. The language is notation about the block and
        // the reader is looking at the block.
        if trimmed.starts_with("```") || trimmed.starts_with("~~~") {
            self.fenced = !self.fenced;
            return Line::from(Span::styled(String::new(), theme.style(Tone::Muted)));
        }
        if self.fenced {
            return Line::from(Span::styled(text.to_string(), theme.style(Tone::Literal)));
        }

        // `# ` through `###### `. The hashes are notation and the text is a
        // heading, so the text is drawn as one and the hashes are not drawn at
        // all.
        let hashes = trimmed.chars().take_while(|c| *c == '#').count();
        if (1..=6).contains(&hashes) && trimmed[hashes..].starts_with(' ') {
            return Line::from(Span::styled(
                format!("{indent}{}", trimmed[hashes + 1..].trim_end()),
                theme
                    .style(Tone::Accent)
                    .add_modifier(Modifier::BOLD | Modifier::UNDERLINED),
            ));
        }

        // A horizontal rule, in the glyph set's own character rather than in the
        // three hyphens the model typed.
        if matches!(trimmed.trim_end(), "---" | "***" | "___") {
            return Line::from(Span::styled(
                format!("{indent}{}", theme.glyphs.rule.to_string().repeat(24)),
                theme.style(Tone::Muted),
            ));
        }

        // `> `, which a model uses for an aside. The marker stays — it is what
        // says the words are quoted — and the tone says the same thing again for
        // a reader who cannot see tone.
        if let Some(quoted) = trimmed.strip_prefix("> ") {
            let mut spans = vec![Span::styled(
                format!("{indent}{} ", theme.glyphs.rule),
                theme.style(Tone::Muted),
            )];
            spans.extend(inline(quoted, theme, Tone::Muted));
            return Line::from(spans);
        }

        // `- `, `* ` and `+ ` become the theme's bullet, at whatever depth the
        // model indented them to.
        for marker in ["- ", "* ", "+ "] {
            if let Some(item) = trimmed.strip_prefix(marker) {
                let mut spans = vec![Span::styled(
                    format!("{indent}{} ", theme.glyphs.bullet),
                    theme.style(Tone::Muted),
                )];
                spans.extend(inline(item, theme, Tone::Normal));
                return Line::from(spans);
            }
        }

        let mut spans = Vec::new();
        if !indent.is_empty() {
            spans.push(Span::styled(indent.to_string(), theme.style(Tone::Normal)));
        }
        spans.extend(inline(trimmed, theme, Tone::Normal));
        Line::from(spans)
    }
}

/// `**bold**`, `*italic*`, `_italic_` and `` `code` ``, in one pass.
///
/// Unclosed notation is not notation: `**` with nothing after it is two
/// asterisks the model typed, and they are drawn. That is what keeps this safe
/// on a streaming line — a bold run that has not finished arriving reads as
/// asterisks for one frame rather than eating the rest of the answer.
fn inline(text: &str, theme: &Theme, base: Tone) -> Vec<Span<'static>> {
    let style = theme.style(base);
    let mut spans: Vec<Span<'static>> = Vec::new();
    let mut plain = String::new();
    let mut rest = text;

    while !rest.is_empty() {
        // Ordered so that at the same position the LONGER marker wins: `**` and
        // `*` both match at the front of `**bold**`, and a plain `.min()` over
        // the pairs picks `*` because it sorts first — which left one asterisk
        // on each side of every bold word in a real answer.
        let marker = ["**", "`", "*", "_"]
            .into_iter()
            .enumerate()
            .filter_map(|(rank, mark)| rest.find(mark).map(|at| (at, rank, mark)))
            .min();
        let Some((at, _, mark)) = marker else { break };

        let (before, from_mark) = rest.split_at(at);
        plain.push_str(before);
        let body = &from_mark[mark.len()..];
        let Some(end) = body.find(mark) else {
            // Unclosed. The marker is text, and so is everything after it.
            plain.push_str(&from_mark[..mark.len()]);
            rest = body;
            continue;
        };
        // **Emphasis hugs its text.** `2 * 3 * 4` is multiplication and
        // `a ** b` is not a bold run, so a marker with a space after it — or a
        // closing one with a space before it — is a character the model typed.
        // This is markdown's own rule and it is the difference between rendering
        // an answer and editing one.
        let inner = &body[..end];
        let hugs = !inner.starts_with(char::is_whitespace) && !inner.ends_with(char::is_whitespace);
        // `**` with nothing between it is not emphasis either, and `_` inside a
        // word is a name — `run_id`, not italics.
        if end == 0 || !hugs || (mark == "_" && !starts_word(before)) {
            plain.push_str(&from_mark[..mark.len()]);
            rest = body;
            continue;
        }

        if !plain.is_empty() {
            spans.push(Span::styled(std::mem::take(&mut plain), style));
        }
        match mark {
            // Code is literal all the way down: backticks are the one marker
            // whose contents are not notation, so nothing inside is looked at.
            "`" => spans.push(Span::styled(inner.to_string(), theme.style(Tone::Literal))),
            // **Emphasis nests, and a real answer nests it.** A model writes
            // ``**`src/main.rs`**`` — bold around code — and a pass that took the
            // inner text verbatim drew the backticks it was meant to remove. The
            // recursion terminates because the inner slice is strictly shorter
            // than the one that found it.
            _ => {
                let modifier = if mark == "**" {
                    Modifier::BOLD
                } else {
                    Modifier::ITALIC
                };
                spans.extend(
                    inline(inner, theme, base)
                        .into_iter()
                        .map(|span| {
                            let style = span.style.add_modifier(modifier);
                            span.style(style)
                        })
                        .filter(|span| !span.content.is_empty()),
                );
            }
        }
        rest = &body[end + mark.len()..];
    }

    plain.push_str(rest);
    if !plain.is_empty() {
        spans.push(Span::styled(plain, style));
    }
    if spans.is_empty() {
        spans.push(Span::styled(String::new(), style));
    }
    spans
}

/// Whether an underscore here opens emphasis rather than sitting inside a name.
fn starts_word(before: &str) -> bool {
    before
        .chars()
        .next_back()
        .is_none_or(|c| !c.is_alphanumeric())
}
