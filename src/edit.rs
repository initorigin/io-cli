//! Changing one value in a configuration file without rewriting the file.
//!
//! io-cli's other writer is [`crate::settings::render`], which serialises a whole
//! private `File` struct and hands it to [`crate::settings::write`]. That is right
//! for the wizard, which creates a file that did not exist, and it is destructive
//! for everything 0.16.0 does: an operator's `io.toml` carries comments, a blank
//! line rhythm, a key order they chose, and whole sections this crate has no type
//! for — `[[agent]]`, `[instructions]`, `[[hook]]`, `[toolchain]`, `[prices]`,
//! `[[plugin]]`. Re-serialising from io-cli's model would drop every one of them,
//! and the result would still parse and still carry the value that was asked for,
//! which is what makes that mistake invisible to any test that re-parses.
//!
//! So this module never serialises a document. It locates the bytes of one value
//! and replaces exactly those, copying every other byte through untouched.
//!
//! # How a value's bytes are found
//!
//! `toml` 1.1.4 exposes no format-preserving document model — its public surface
//! is `Value`, `Table`, `from_str`, `to_string`/`to_string_pretty` and `Spanned`
//! — and `toml_edit` is not in this crate's dependency tree, so reaching for it
//! would be a new direct dependency against `tests/dependencies.rs`. `Spanned` is
//! re-exported by `toml` and already here.
//!
//! Three things about `Spanned` were measured against toml 1.1.4 before this
//! module was written, because each of them decides the design:
//!
//! 1. **A flat block of `key = value` lines yields exact VALUE spans** for every
//!    scalar kind — `30`, `"scout"`, `1.5`, `true`, `[1, 2, 3]` and a `"""`
//!    string — with any inline comment excluded. That is the primitive this
//!    module is built on.
//! 2. **A table's own span is its HEADER**, not its body: `[run]` reports the
//!    five bytes of `[run]`. So a document cannot be walked by nesting spans; it
//!    has to be cut into regions by header first, and each region parsed alone.
//! 3. **A recursive `#[serde(untagged)]` node type silently loses every nested
//!    span.** Untagged deserialisation buffers the content, and a buffer has no
//!    span to give `Spanned`, so the table and array variants fail to match and
//!    the value falls through to the scalar arm — parsing successfully with no
//!    span anywhere. It does not error. That is the generic design this module
//!    would obviously have used, and it fails in exactly the silent way the rest
//!    of this file exists to avoid.
//!
//! A **dotted key** is refused rather than edited, and that is measurement 4:
//! `a.b = 1` reports the span of `a`, the key, not of `1`. Splicing there would
//! replace a key with a value and produce a file that no longer parses. The
//! writer says so instead.

use std::collections::BTreeMap;
use std::ops::Range;
use std::path::Path;

use toml::Spanned;

/// One change to make: the dotted path of a key, and the TOML text of its value.
///
/// The value is **TOML source**, not a Rust value — `"45"`, `"\"light\""`,
/// `"true"`. That is deliberate: this module's whole job is bytes, and a caller
/// that has a string to write already knows whether it needs quotes. Rendering a
/// Rust value into TOML is [`crate::settings`]'s business and it has serde for it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Edit {
    path: String,
    value: String,
}

impl Edit {
    /// Set `path` to the TOML source `value`.
    ///
    /// `path` is dotted, and a segment may carry an index for an array of
    /// tables: `run.max_steps`, `app.io-cli.theme`, `mcp[1].command`.
    pub fn set(path: impl Into<String>, value: impl Into<String>) -> Self {
        Self {
            path: path.into(),
            value: value.into(),
        }
    }

    /// The dotted path this edit addresses.
    pub fn path(&self) -> &str {
        &self.path
    }

    /// The TOML source this edit writes.
    pub fn value(&self) -> &str {
        &self.value
    }
}

/// One `[table]` or `[[array]]` block, and the bytes between it and the next.
#[derive(Debug)]
struct Region {
    /// The header's dotted path. Empty for the implicit region above the first
    /// header, which is where a file's top-level keys live.
    path: Vec<String>,
    /// Which occurrence of `path` this is. Always 0 for a `[table]`; an
    /// `[[array]]` entry counts up from 0 in the order the file lists them.
    index: usize,
    /// The bytes after the header line, up to the next header or the end.
    body: Range<usize>,
}

/// Apply every edit to `text` and return the new document.
///
/// Returns `Err` with a sentence naming the problem when an edit cannot be made
/// safely: a value that is not TOML, a key expressed as a dotted key or inside an
/// inline table, or a result that would not parse. Nothing partial is ever
/// returned — either every edit applied or none did.
pub fn apply(text: &str, edits: &[Edit]) -> Result<String, String> {
    // A value that is not TOML is refused before anything is spliced, so the
    // message names the value rather than the wreckage it would have made.
    for edit in edits {
        if toml::from_str::<toml::value::Table>(&format!("probe = {}", edit.value)).is_err() {
            return Err(format!(
                "`{}` is not a TOML value, so `{}` was not written and the file is unchanged",
                edit.value, edit.path
            ));
        }
    }

    let regions = regions(text)?;
    let parsed: toml::Value = toml::from_str(text)
        .map_err(|e| format!("this configuration file does not parse, so nothing was written: {e}"))?;

    // Each edit becomes one splice: a byte range to replace and the text to put
    // there. A replacement is the value's own span; an insertion is an empty
    // range at the point the new line goes.
    let mut splices: Vec<(Range<usize>, String)> = Vec::new();

    for edit in edits {
        let (table_path, key) = split_path(&edit.path)?;
        let region = regions.iter().find(|r| {
            r.path == table_path.names && r.index == table_path.index
        });

        match region {
            Some(region) => {
                let body = &text[region.body.clone()];
                let flat: BTreeMap<String, Spanned<toml::Value>> = toml::from_str(body)
                    .map_err(|e| {
                        format!("the `{}` section does not parse: {e}", edit.path)
                    })?;

                match flat.get(&key) {
                    Some(spanned) => {
                        let span = spanned.span();
                        let absolute = region.body.start + span.start..region.body.start + span.end;
                        // Measurement 4: a dotted key reports the key's span, not
                        // the value's. The tell is that the span's text is not a
                        // value on its own.
                        let found = &text[absolute.clone()];
                        if toml::from_str::<toml::value::Table>(&format!("probe = {found}")).is_err()
                        {
                            return Err(dotted_refusal(&edit.path));
                        }
                        splices.push((absolute, edit.value.clone()));
                    }
                    None => {
                        // The section exists and the key does not. Put it at the
                        // end of this section rather than the end of the file,
                        // where it would silently join whatever section is last.
                        let at = insertion_point(text, &region.body);
                        splices.push((at..at, format!("{key} = {}\n", edit.value)));
                    }
                }
            }
            None => {
                // No such section. Before appending one, ask whether the key is
                // already in the document in a shape this writer does not edit —
                // a dotted key, or a value inside an inline table. Appending a
                // second definition of it would be a duplicate-key error at best
                // and two competing values at worst.
                if resolve(&parsed, &table_path.names, &key).is_some() {
                    return Err(dotted_refusal(&edit.path));
                }
                let header = table_path.names.join(".");
                let mut block = String::new();
                if !text.is_empty() && !text.ends_with('\n') {
                    block.push('\n');
                }
                block.push_str(&format!("\n[{header}]\n{key} = {}\n", edit.value));
                splices.push((text.len()..text.len(), block));
            }
        }
    }

    // Right to left, so an earlier splice never invalidates a later offset.
    // Doing it in one pass is the point: two sequential calls would hide an
    // offset bug that only shows when two edits share a document.
    splices.sort_by(|a, b| b.0.start.cmp(&a.0.start));
    let mut out = text.to_string();
    for (range, replacement) in splices {
        out.replace_range(range, &replacement);
    }

    // Read back. Nothing leaves this function that does not parse.
    toml::from_str::<toml::Value>(&out).map_err(|e| {
        format!("the edit would have produced a file that does not parse, so it was not made: {e}")
    })?;

    Ok(out)
}

/// Read `path`, apply every edit, and put it back atomically.
///
/// The new bytes go to a temporary file in the same directory and are renamed
/// over the original, so a failure part way through cannot truncate a
/// configuration — a rename within a directory is atomic, and a partial write is
/// only ever visible in a file nothing reads. The original's permissions are
/// carried over, because the file holds a credential and a write is not a reason
/// to widen it.
pub fn write(path: &Path, edits: &[Edit]) -> Result<(), String> {
    let text = std::fs::read_to_string(path)
        .map_err(|e| format!("{}: {e}", path.display()))?;
    let updated = apply(&text, edits)?;

    let dir = path.parent().unwrap_or_else(|| Path::new("."));
    let temp = dir.join(format!(
        ".{}.io-cli.tmp",
        path.file_name().unwrap_or_default().to_string_lossy()
    ));

    let result = (|| -> std::io::Result<()> {
        #[cfg(unix)]
        {
            use std::io::Write as _;
            use std::os::unix::fs::{OpenOptionsExt, PermissionsExt};

            let mode = std::fs::metadata(path)
                .map(|m| m.permissions().mode() & 0o777)
                .unwrap_or(0o600);
            let mut file = std::fs::OpenOptions::new()
                .write(true)
                .create(true)
                .truncate(true)
                .mode(mode)
                .open(&temp)?;
            file.write_all(updated.as_bytes())?;
            file.sync_all()?;
        }
        #[cfg(not(unix))]
        {
            std::fs::write(&temp, updated.as_bytes())?;
        }
        std::fs::rename(&temp, path)
    })();

    if result.is_err() {
        // Leave nothing behind. The original is untouched either way, because
        // nothing has been renamed over it.
        let _ = std::fs::remove_file(&temp);
    }
    result.map_err(|e| format!("{}: {e}", path.display()))
}

/// The message for a key this writer will not edit in place.
fn dotted_refusal(path: &str) -> String {
    format!(
        "`{path}` is written as a dotted key or inside an inline table, and this editor \
         replaces a value in a `[section]` block. Nothing was written. Move the key under \
         its own section header and it can be edited here."
    )
}

/// A path split into the section that holds the key, and the key itself.
struct TablePath {
    names: Vec<String>,
    index: usize,
}

fn split_path(path: &str) -> Result<(TablePath, String), String> {
    let mut names: Vec<String> = Vec::new();
    let mut index = 0usize;

    let segments: Vec<&str> = path.split('.').collect();
    let (key, table) = segments
        .split_last()
        .ok_or_else(|| format!("`{path}` names no key"))?;

    for segment in table {
        // `mcp[1]` addresses the second `[[mcp]]` entry. Only the last section
        // segment may carry an index, which is what an array of tables is.
        if let Some((name, rest)) = segment.split_once('[') {
            let number = rest.strip_suffix(']').ok_or_else(|| {
                format!("`{path}` has an unclosed index")
            })?;
            index = number
                .parse()
                .map_err(|_| format!("`{path}` has an index that is not a number"))?;
            names.push(name.to_string());
        } else {
            names.push(segment.to_string());
        }
    }

    if key.is_empty() {
        return Err(format!("`{path}` names no key"));
    }
    Ok((TablePath { names, index }, (*key).to_string()))
}

/// Whether the parsed document already holds this path, in any shape.
fn resolve<'a>(value: &'a toml::Value, table: &[String], key: &str) -> Option<&'a toml::Value> {
    let mut node = value;
    for name in table {
        node = node.get(name)?;
    }
    node.get(key)
}

/// Where a new `key = value` line goes inside an existing section.
///
/// After the last line that has anything on it, so a key lands with its
/// siblings rather than below the blank line that separates this section from
/// the next one.
fn insertion_point(text: &str, body: &Range<usize>) -> usize {
    let slice = &text[body.clone()];
    match slice.rfind(|c: char| !c.is_whitespace()) {
        // One past the end of the last non-blank character's line.
        Some(last) => {
            let from = body.start + last;
            text[from..]
                .find('\n')
                .map(|n| from + n + 1)
                .unwrap_or(body.end)
        }
        None => body.start,
    }
}

/// Cut the document into regions at every table header.
///
/// The scan is a character state machine rather than a line filter, because a
/// `[` at the start of a line inside a `"""` string is not a header and a
/// document split there would be cut in half.
fn regions(text: &str) -> Result<Vec<Region>, String> {
    #[derive(PartialEq)]
    enum State {
        Normal,
        Comment,
        Basic,
        Literal,
        MultiBasic,
        MultiLiteral,
    }

    let bytes = text.as_bytes();
    let mut state = State::Normal;
    let mut header_starts: Vec<usize> = Vec::new();
    let mut at_line_start = true;
    let mut i = 0usize;

    while i < bytes.len() {
        let c = bytes[i];
        match state {
            State::Normal => {
                if at_line_start && (c == b'[') {
                    header_starts.push(i);
                }
                match c {
                    b'#' => state = State::Comment,
                    b'"' => {
                        if text[i..].starts_with("\"\"\"") {
                            state = State::MultiBasic;
                            i += 2;
                        } else {
                            state = State::Basic;
                        }
                    }
                    b'\'' => {
                        if text[i..].starts_with("'''") {
                            state = State::MultiLiteral;
                            i += 2;
                        } else {
                            state = State::Literal;
                        }
                    }
                    _ => {}
                }
            }
            State::Comment => {
                if c == b'\n' {
                    state = State::Normal;
                }
            }
            State::Basic => match c {
                b'\\' => i += 1,
                b'"' => state = State::Normal,
                _ => {}
            },
            State::Literal => {
                if c == b'\'' {
                    state = State::Normal;
                }
            }
            State::MultiBasic => {
                if text[i..].starts_with("\"\"\"") {
                    state = State::Normal;
                    i += 2;
                } else if c == b'\\' {
                    i += 1;
                }
            }
            State::MultiLiteral => {
                if text[i..].starts_with("'''") {
                    state = State::Normal;
                    i += 2;
                }
            }
        }

        // A header only counts at the very start of a line, so leading spaces
        // keep the flag alive and anything else clears it.
        if c == b'\n' {
            at_line_start = true;
        } else if !(c as char).is_whitespace() {
            at_line_start = false;
        }
        i += 1;
    }

    let mut regions = Vec::new();
    let first = header_starts.first().copied().unwrap_or(bytes.len());
    regions.push(Region {
        path: Vec::new(),
        index: 0,
        body: 0..first,
    });

    let mut seen: BTreeMap<Vec<String>, usize> = BTreeMap::new();
    for (n, &start) in header_starts.iter().enumerate() {
        let line_end = text[start..]
            .find('\n')
            .map(|e| start + e + 1)
            .unwrap_or(bytes.len());
        let header = text[start..line_end].trim();
        let inner = header
            .strip_prefix("[[")
            .and_then(|h| h.strip_suffix("]]"))
            .or_else(|| header.strip_prefix('[').and_then(|h| h.strip_suffix(']')))
            .ok_or_else(|| format!("`{header}` is not a section header this editor understands"))?;

        let path: Vec<String> = inner
            .split('.')
            .map(|s| s.trim().trim_matches('"').trim_matches('\'').to_string())
            .collect();

        let index = seen.entry(path.clone()).or_insert(0);
        let body_end = header_starts
            .get(n + 1)
            .copied()
            .unwrap_or(bytes.len());

        regions.push(Region {
            path,
            index: *index,
            body: line_end..body_end,
        });
        *index += 1;
    }

    Ok(regions)
}
