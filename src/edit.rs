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
    kind: Kind,
}

/// What an edit does to the document.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Kind {
    /// Replace one value's bytes, or add the key to its section.
    Set,
    /// Append a whole `[[path]]` entry to the end of the document.
    Append,
    /// Create the `[path]` section with a whole body, refusing if it exists.
    Section,
    /// Remove a whole `[[path]]` entry, or a `[path]` section, bytes and all.
    Remove,
    /// Delete one `key = value` line, leaving the section around it standing.
    Unset,
    /// Move one `[[path]]` entry to another position in its array.
    Move,
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
            kind: Kind::Set,
        }
    }

    /// Append a new `[[path]]` entry whose body is `body`.
    ///
    /// The shape [`Edit::set`] cannot express: `set` reaches a key inside an
    /// entry that already exists, and an array of tables grows by gaining a whole
    /// new block. `body` is the entry's own `key = value` lines, without the
    /// header — this writes the header.
    ///
    /// Appended at the end of the document rather than beside its siblings,
    /// because an array of tables is ordered and a new entry belongs last: for
    /// `[[provider]]` that order is the fallback chain, and inserting into the
    /// middle of it would silently rearrange which provider a run uses.
    pub fn append(path: impl Into<String>, body: impl Into<String>) -> Self {
        Self {
            path: path.into(),
            value: body.into(),
            kind: Kind::Append,
        }
    }

    /// Create the `[path]` section with `body` as its whole contents.
    ///
    /// **The shape neither [`Edit::set`] nor [`Edit::append`] can express, and the
    /// gap is not academic.** `append` writes `[[path]]`, an array-of-tables
    /// entry. `set` writes one key, and when the section it names does not exist
    /// it appends a whole new header to carry that one key — which is correct for
    /// one key and catastrophic for many: sixty `set`s into a file with no
    /// `[prices.models]` each append their own `[prices.models]` header, because
    /// every edit in a batch is resolved against the document as it was *before*
    /// the batch. The result is sixty duplicate table definitions, and the whole
    /// write is refused by the read-back — so the very first fill of a price table
    /// could never succeed.
    ///
    /// `body` is the section's own `key = value` lines without the header; this
    /// writes the header. **It refuses when the section already exists**, rather
    /// than replacing it: a caller with an existing section wants `set` per key,
    /// which preserves every row this one does not name, and silently discarding
    /// an operator's hand-added rows is not a thing to do by accident.
    pub fn section(path: impl Into<String>, body: impl Into<String>) -> Self {
        Self {
            path: path.into(),
            value: body.into(),
            kind: Kind::Section,
        }
    }

    /// Remove the whole `[path]` section or `[[path]][index]` entry.
    pub fn remove(path: impl Into<String>) -> Self {
        Self {
            path: path.into(),
            value: String::new(),
            kind: Kind::Remove,
        }
    }

    /// Delete the single `key = value` line at `path`.
    ///
    /// **A different verb from [`Edit::remove`], and keeping the two apart is
    /// the whole of the design.** `remove` takes a REGION away — a `[section]`
    /// or one `[[array]]` entry, header and body and every unmodelled key
    /// inside it — and it finds that region by matching a header, so it has
    /// never been able to name a key: asked for `run.max_steps` it looks for a
    /// `[run.max_steps]` header, finds none, and refuses. One verb that deleted
    /// a key when it found a key and a whole section when it found a section
    /// would read as the same call at every call site, and the two outcomes are
    /// the difference between clearing one setting and deleting an operator's
    /// entire `[run]` block. Callers of this module are one keystroke from a
    /// file somebody's runs depend on, so the ambiguity is settled by which
    /// constructor was written rather than by what the document happened to
    /// hold — and a path that names the wrong kind of thing is an error instead
    /// of a larger deletion than anyone asked for.
    ///
    /// **Unset, not set-to-empty.** What a caller wants here is the ABSENCE of
    /// the key: an io-harness setting the file does not carry falls back to its
    /// default or to the layer below, and `key = ""` is a value that shadows
    /// both. So the line goes and nothing is left in its place.
    ///
    /// The whole physical line goes, from its first byte to the newline after
    /// the value, which takes an inline comment on that line with it — that
    /// comment is a note about the key that is leaving. Nothing else moves: the
    /// section header survives even when its last key is unset, and every
    /// sibling key, blank line and comment around the deleted line is copied
    /// through byte for byte, the same preservation property every other edit
    /// in this module keeps.
    ///
    /// **A value spelled across more than one line takes all of its lines.**
    /// The span this works from is the value's, and the cut runs to the end of
    /// it, so an array written over four lines or a `"""` block leaves nothing
    /// behind. Cutting only the first line would strand the rest of the value
    /// as a fragment and turn a deletion into a file that no longer parses,
    /// which is the one outcome a writer must never reach quietly.
    ///
    /// `path` is resolved the way [`value_at`] resolves it, through the same
    /// `segments` splitter, so a key written `c = 1` inside `[a.b]` is `a.b.c`
    /// and a top-level key is its own bare name. An unindexed path names entry
    /// 0 of an array of tables, as [`Edit::set`] does: TOML forbids one table
    /// from carrying a key twice, so `[[array]]` entries are the only way one
    /// path can address two lines in a document, and the index is how a caller
    /// picks between them.
    ///
    /// A key the file does not carry is refused by name rather than passed over
    /// in silence, because a caller told nothing after asking for a removal
    /// will believe the removal happened.
    pub fn unset(path: impl Into<String>) -> Self {
        Self {
            path: path.into(),
            value: String::new(),
            kind: Kind::Unset,
        }
    }

    /// Move the `from`-th entry of the `[[path]]` array to position `to`.
    ///
    /// **Order is meaning for an array of tables, and for `[[provider]]` it is
    /// the fallback chain**: the first entry is the provider a run uses and each
    /// later one is the next link. So this is not a cosmetic operation and it
    /// cannot be done by rewriting the array — the entry's own bytes, comments
    /// and unmodelled keys have to arrive at the new position intact, which is
    /// the same property every other edit here keeps.
    pub fn move_entry(path: impl Into<String>, from: usize, to: usize) -> Self {
        Self {
            path: path.into(),
            value: format!("{from}:{to}"),
            kind: Kind::Move,
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
    /// Where the header line itself starts. Equal to `body.start` for the
    /// implicit region, which has no header.
    start: usize,
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
        if edit.kind != Kind::Set {
            continue;
        }
        if toml::from_str::<toml::value::Table>(&format!("probe = {}", edit.value)).is_err() {
            return Err(format!(
                "`{}` is not a TOML value, so `{}` was not written and the file is unchanged",
                edit.value, edit.path
            ));
        }
    }

    // **An unset and another edit on the same path cannot share a batch**, and
    // this is the one place that can see both. Every edit here is resolved
    // against the document as it was BEFORE the batch, and an unset resolves to
    // a whole line rather than a value's span — so a `set` on the same key
    // resolves to a range *inside* the range the unset is about to take away.
    // Two overlapping splices against stale offsets do not produce a compromise,
    // they produce whichever bytes the arithmetic lands on, and on a short
    // replacement they run off the end of the string and panic in a function
    // whose whole promise is that a refused edit leaves the file alone. There is
    // no reading of "delete this key and also write it" worth guessing at, so it
    // is named and refused. The scan is quadratic in the edit count and gated on
    // there being an unset at all, which keeps a four-hundred-key price fill at
    // no cost.
    for edit in edits.iter().filter(|e| e.kind == Kind::Unset) {
        if edits.iter().filter(|other| other.path == edit.path).count() > 1 {
            return Err(format!(
                "`{}` is unset and edited in the same batch, so nothing was written — \
                 every edit is resolved against the file as it was before the batch, \
                 and an unset takes the whole line the other edit was going to land on",
                edit.path
            ));
        }
    }

    let regions = regions(text)?;
    let parsed: toml::Value = toml::from_str(text).map_err(|e| {
        format!("this configuration file does not parse, so nothing was written: {e}")
    })?;

    // Each edit becomes one splice: a byte range to replace and the text to put
    // there. A replacement is the value's own span; an insertion is an empty
    // range at the point the new line goes.
    let mut splices: Vec<(Range<usize>, String)> = Vec::new();

    for edit in edits {
        match edit.kind {
            Kind::Append => {
                let mut block = String::new();
                if !text.is_empty() && !text.ends_with('\n') {
                    block.push('\n');
                }
                let body = edit.value.trim_end();
                block.push_str(&format!("\n[[{}]]\n{body}\n", edit.path));
                splices.push((text.len()..text.len(), block));
                continue;
            }
            Kind::Section => {
                // **Refused rather than replaced when it already exists**, and the
                // refusal is the point: a caller holding an existing section wants
                // `set` per key, which leaves every row it does not name alone.
                // Replacing here would silently discard whatever the operator had
                // added by hand, from a call site whose author believed they were
                // adding rows.
                let names = segments(&edit.path)?;
                if regions.iter().any(|r| r.path == names) {
                    return Err(format!(
                        "`[{}]` is already in this file, so it was not written whole — \
                         a section that exists is edited key by key, which is what keeps \
                         the rows nobody named",
                        edit.path
                    ));
                }
                let header = names
                    .iter()
                    .map(|name| spell(name))
                    .collect::<Vec<_>>()
                    .join(".");
                let mut block = String::new();
                if !text.is_empty() && !text.ends_with('\n') {
                    block.push('\n');
                }
                let body = edit.value.trim_end();
                block.push_str(&format!("\n[{header}]\n{body}\n"));
                splices.push((text.len()..text.len(), block));
                continue;
            }
            Kind::Move => {
                let (from, to) = edit
                    .value
                    .split_once(':')
                    .and_then(|(a, b)| Some((a.parse::<usize>().ok()?, b.parse::<usize>().ok()?)))
                    .ok_or_else(|| format!("`{}` has no positions to move between", edit.path))?;

                let names = segments(&edit.path)?;
                let at = |index: usize| {
                    regions
                        .iter()
                        .find(|r| r.path == names && r.index == index)
                        .ok_or_else(|| format!("there is no `{}[{index}]` in this file", edit.path))
                };
                if from == to {
                    continue;
                }
                let source = at(from)?;
                // Stops before any comment run trailing the entry, which belongs
                // to the section below it. See [`removal_end`].
                let taken = source.start..removal_end(text, &source.body);

                // Both splices are computed against the ORIGINAL text and applied
                // right to left, which is what makes a move one pass rather than
                // a remove followed by an append that has to re-find everything.
                let destination = at(to)?;
                let insert = if to > from {
                    // Moving down: land after the entry currently at `to`.
                    destination.body.end
                } else {
                    // Moving up: land where that entry's header begins.
                    destination.start
                };

                // **The guard [`Kind::Append`] has, in both directions.** The
                // last region's `body.end` is the length of the file, so a move
                // into last place against a file that does not end in a newline
                // splices the header onto whatever the final line was — and when
                // that line is a comment, the comment swallows the header, the
                // moved entry's keys join the table above it, and the result
                // parses cleanly as a different configuration. The mirror is a
                // moved block that ends without a newline, which swallows the
                // destination's header the same way.
                let mut block = text[taken.clone()].to_string();
                if !block.ends_with('\n') {
                    block.push('\n');
                }
                if insert > 0 && !text[..insert].ends_with('\n') {
                    block.insert(0, '\n');
                }

                splices.push((taken, String::new()));
                splices.push((insert..insert, block));
                continue;
            }
            Kind::Remove => {
                let (table_path, last) = split_path(&edit.path)?;
                // `remove` names a SECTION, so every segment is part of the
                // header — including the one `split_path` peeled off as a key.
                // Its index has to be read here too: `split_path` only looks for
                // one on the segments it treats as a table, and for `mcp[1]` the
                // indexed segment IS the last one.
                let mut names = table_path.names;
                let mut index = table_path.index;
                match last.split_once('[') {
                    Some((name, rest)) => {
                        let number = rest
                            .strip_suffix(']')
                            .ok_or_else(|| format!("`{}` has an unclosed index", edit.path))?;
                        index = number.parse().map_err(|_| {
                            format!("`{}` has an index that is not a number", edit.path)
                        })?;
                        names.push(name.to_string());
                    }
                    None => names.push(last),
                }
                let region = regions
                    .iter()
                    .find(|r| r.path == names && r.index == index)
                    .ok_or_else(|| format!("there is no `{}` to remove in this file", edit.path))?;
                // Not `region.body.end`: that is the first byte of the next
                // header, and the lines just above it document the next section
                // rather than this one. See [`removal_end`].
                splices.push((region.start..removal_end(text, &region.body), String::new()));
                continue;
            }
            Kind::Unset => {
                // The same resolution `value_at` does, because a verb that
                // deletes a key has to reach exactly the key a reader would have
                // been shown — a second path resolver would eventually disagree
                // with this one, and the disagreement would surface as a deleted
                // line nobody named.
                let (table_path, key) = split_path(&edit.path)?;
                let region = regions
                    .iter()
                    .find(|r| r.path == table_path.names && r.index == table_path.index);
                let absent = || format!("there is no `{}` to unset in this file", edit.path);

                let region = match region {
                    Some(region) => region,
                    None => {
                        // No such section. Before reporting the key as absent,
                        // ask the parsed document — a dotted key or a key inside
                        // an inline table IS in the file, in a shape that has no
                        // line of its own to take away, and "there is no such
                        // key" would be a sentence the caller cannot act on.
                        if resolve(&parsed, &table_path.names, &key).is_some() {
                            return Err(dotted_refusal(&edit.path));
                        }
                        return Err(absent());
                    }
                };

                let body = &text[region.body.clone()];
                let flat: BTreeMap<String, Spanned<toml::Value>> = toml::from_str(body)
                    .map_err(|e| format!("the `{}` section does not parse: {e}", edit.path))?;
                let span = flat.get(&key).ok_or_else(absent)?.span();
                let absolute = region.body.start + span.start..region.body.start + span.end;

                // Measurement 4 again, and it matters more here than it does to
                // a `set`: a dotted key reports the KEY's span, so a cut around
                // it would take the line `a.b = 1` away when `a.c` was what the
                // caller asked to be rid of.
                let found = &text[absolute.clone()];
                if toml::from_str::<toml::value::Table>(&format!("probe = {found}")).is_err() {
                    return Err(dotted_refusal(&edit.path));
                }

                // The span covers the value alone, and what has to go is the
                // line carrying it: back to the start of the line the key is
                // written on, forward past the newline that ends the value's
                // last line. Running forward from the value's END rather than
                // from its start is what makes a multi-line array or a `"""`
                // block leave whole, and what stops this from stranding half a
                // value in a file that then does not parse.
                let start = text[..absolute.start].rfind('\n').map_or(0, |at| at + 1);
                let end = text[absolute.end..]
                    .find('\n')
                    .map_or(text.len(), |at| absolute.end + at + 1);
                splices.push((start..end, String::new()));
                continue;
            }
            Kind::Set => {}
        }

        let (table_path, key) = split_path(&edit.path)?;
        let region = regions
            .iter()
            .find(|r| r.path == table_path.names && r.index == table_path.index);

        match region {
            Some(region) => {
                let body = &text[region.body.clone()];
                let flat: BTreeMap<String, Spanned<toml::Value>> = toml::from_str(body)
                    .map_err(|e| format!("the `{}` section does not parse: {e}", edit.path))?;

                match flat.get(&key) {
                    Some(spanned) => {
                        let span = spanned.span();
                        let absolute = region.body.start + span.start..region.body.start + span.end;
                        // Measurement 4: a dotted key reports the key's span, not
                        // the value's. The tell is that the span's text is not a
                        // value on its own.
                        let found = &text[absolute.clone()];
                        if toml::from_str::<toml::value::Table>(&format!("probe = {found}"))
                            .is_err()
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
                        // `key` is DECODED, so it is spelled back rather than
                        // pasted: a name with a dot or a space in it written bare
                        // is a dotted key or a parse error, never the key asked
                        // for.
                        splices.push((at..at, format!("{} = {}\n", spell(&key), edit.value)));
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
                let header = table_path
                    .names
                    .iter()
                    .map(|name| spell(name))
                    .collect::<Vec<_>>()
                    .join(".");
                let mut block = String::new();
                if !text.is_empty() && !text.ends_with('\n') {
                    block.push('\n');
                }
                block.push_str(&format!("\n[{header}]\n{} = {}\n", spell(&key), edit.value));
                splices.push((text.len()..text.len(), block));
            }
        }
    }

    // Right to left, so an earlier splice never invalidates a later offset.
    // Doing it in one pass is the point: two sequential calls would hide an
    // offset bug that only shows when two edits share a document.
    // Stable, which a move depends on: it pushes a removal and an insertion that
    // can share a start, and they have to stay in the order they were pushed.
    splices.sort_by_key(|(range, _)| std::cmp::Reverse(range.start));
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

/// Every section header in the document, as its dotted path.
///
/// The same scan [`apply`] cuts the document with, exposed because io-harness
/// has no accessor for one thing a caller needs to enumerate: the `[profile.*]`
/// names a file declares. `Config::with_profile` applies one by name and says so
/// when the name is wrong, and there is nothing that lists them — the merged
/// table is private and profile keys do not appear in `Config::origins`.
///
/// Header paths only, and nothing about what is inside them: this stays a module
/// that works in bytes.
pub fn sections(text: &str) -> Vec<Vec<String>> {
    regions(text)
        .map(|found| {
            found
                .into_iter()
                .filter(|region| !region.path.is_empty())
                .map(|region| region.path)
                .collect()
        })
        .unwrap_or_default()
}

/// The keys `[path]` declares directly, sorted.
///
/// **The read half of [`sections`], one level down.** That answers which tables a
/// document holds; this answers what one of them spells. Keys only and never their
/// values — [`value_at`] is where a value's own bytes come from — so this stays a
/// module that works in bytes and decides nothing about what a setting means.
///
/// It exists because `src/prices.rs` needed to count the models a file prices and
/// was parsing the whole document to `toml::Value` to do it, which made this
/// module's "the only file permitted to parse TOML" rule false while the gate that
/// enforced it named only two spellings. The honest fix was the accessor rather
/// than an exemption: `prices` never needed a document, only a count of one
/// table's keys, and [`sections`] already exists for exactly this reason one level
/// up.
///
/// Empty when the document does not parse, when it carries no such header, or when
/// the table is spelled as an inline `name = { … }` — which is a value rather than
/// a region and has no keys of its own to walk. An array-of-tables index is not
/// addressed here; the first region whose path matches answers.
#[must_use]
pub fn keys(text: &str, path: &str) -> Vec<String> {
    let Ok(names) = segments(path) else {
        return Vec::new();
    };
    let Ok(found) = regions(text) else {
        return Vec::new();
    };
    let Some(region) = found.iter().find(|region| region.path == names) else {
        return Vec::new();
    };
    let flat: BTreeMap<String, toml::Value> =
        toml::from_str(&text[region.body.clone()]).unwrap_or_default();
    flat.into_keys().collect()
}

/// The TOML source of the value at `path`, exactly as the file spells it.
///
/// **Quoting, not interpreting.** This returns the bytes between the `=` and the
/// end of the value and says nothing about what they mean; it is the read half of
/// the same span machinery [`apply`] writes through, and it exists because
/// io-harness does not expose an accessor for every section it validates —
/// `MemorySection` is private and there is no `Config::memory()`, so a surface
/// that showed only what the typed API hands back would have a hole in it exactly
/// where an operator had written something.
///
/// A caller pairs this with [`io_harness::Config::origin`], which names the file
/// that decided the key. Quoting a named file's own bytes is a different act from
/// deciding what a setting means, and `tests/dependencies.rs` holds this module to
/// that line by asserting it names no configuration type.
///
/// `None` when the file does not carry the key at all, or carries it in a shape
/// this module does not address — a dotted key or an inline table.
pub fn value_at(text: &str, path: &str) -> Option<String> {
    let (table_path, key) = split_path(path).ok()?;
    let regions = regions(text).ok()?;
    let region = regions
        .iter()
        .find(|r| r.path == table_path.names && r.index == table_path.index)?;

    let body = &text[region.body.clone()];
    let flat: BTreeMap<String, Spanned<toml::Value>> = toml::from_str(body).ok()?;
    let span = flat.get(&key)?.span();
    let absolute = region.body.start + span.start..region.body.start + span.end;
    let found = &text[absolute];

    // The dotted-key tell from [`apply`]: a span whose text is not a value on its
    // own is the key rather than the value, and quoting it would be a lie.
    toml::from_str::<toml::value::Table>(&format!("probe = {found}")).ok()?;
    Some(found.to_string())
}

/// Spell a list of strings as a TOML array literal.
///
/// The escaping is why this lives here and not at the call site. An absolute
/// Windows path is full of backslashes and `\U` opens an escape in a basic
/// string, so `format!("[\"{}\"]", path)` is either a parse error or a
/// different path — and a path that parses to something else is skipped in
/// silence by `config.rs:2925`, which is the quietest failure this crate can
/// ship. `toml`'s own serializer knows the rules; a format string does not.
///
/// This module is the crate's only TOML speller by rule
/// (`tests/dependencies.rs`), and that rule is about *meaning*, not syntax:
/// building a value is the same kind of act as locating one with
/// [`value_at`] — it is about how a file is spelled, never about what a
/// setting means. So the exception stays exactly as wide as it already was.
#[must_use]
pub fn array(items: &[&str]) -> String {
    let values = items
        .iter()
        .map(|item| toml::Value::String((*item).to_string()))
        .collect();
    toml::Value::Array(values).to_string()
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
    let text = std::fs::read_to_string(path).map_err(|e| format!("{}: {e}", path.display()))?;
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

    let parts = segments(path)?;
    let (key, table) = parts
        .split_last()
        .ok_or_else(|| format!("`{path}` names no key"))?;

    for segment in table {
        // `mcp[1]` addresses the second `[[mcp]]` entry. Only the last section
        // segment may carry an index, which is what an array of tables is.
        if let Some((name, rest)) = segment.split_once('[') {
            let number = rest
                .strip_suffix(']')
                .ok_or_else(|| format!("`{path}` has an unclosed index"))?;
            index = number
                .parse()
                .map_err(|_| format!("`{path}` has an index that is not a number"))?;
            names.push(name.to_string());
        } else {
            names.push(segment.clone());
        }
    }

    if key.is_empty() {
        return Err(format!("`{path}` names no key"));
    }
    Ok((TablePath { names, index }, key.clone()))
}

/// Cut a dotted TOML path into its segments, **decoded**.
///
/// The one splitter, used by both halves of this module, and that is the point
/// of it. A dot inside a quoted key is not a separator — TOML spells a bare key
/// out of `A-Za-z0-9_-` and nothing else, so a key carrying a dot can only ever
/// be written quoted (`"gpt-4.1"`, `"github.com/x"`) and `path.split('.')` cuts
/// it in half. [`split_path`] and [`regions`] each did exactly that and then
/// normalised what came out DIFFERENTLY, so a caller's path and the file's own
/// header never matched: the read half answered `None` for any dotted key and
/// the write half fell through to the append arm and emitted a second copy of a
/// table that was already there, caught only by the read-back in [`apply`] and
/// only as "would have produced a file that does not parse".
///
/// The segments come back decoded, which is the half `trim_matches('"')` cannot
/// do: a basic string takes the full escape set, so `"a\"b"` is a legal key
/// whose name is `a"b`, and trimming strips repeated quotes off both ends and
/// resolves no escape at all. A literal string `'...'` takes no escapes, so a
/// `'` can never appear inside one and finding its end is a search. The decoding
/// is `toml`'s own for the reason [`array()`] spells through `toml` rather than a
/// format string — the escape rules belong to the format, not to this file.
/// [`spell`] is the inverse, for the two places a segment goes back into a
/// document.
///
/// Whitespace around a dot is legal (`[ a . b ]`), so an unquoted segment is
/// trimmed and a quoted one is taken exactly as the quotes deliver it.
fn segments(path: &str) -> Result<Vec<String>, String> {
    let bytes = path.as_bytes();
    let mut out: Vec<String> = Vec::new();
    let mut i = 0usize;

    loop {
        while matches!(bytes.get(i), Some(b' ' | b'\t')) {
            i += 1;
        }
        match bytes.get(i).copied() {
            Some(quote) if quote == b'"' || quote == b'\'' => {
                // Find the closing quote. A backslash in a basic string escapes
                // whatever follows it, including the quote itself; a literal
                // string has no escapes, so the first `'` ends it.
                let mut j = i + 1;
                let end = loop {
                    match bytes.get(j).copied() {
                        None => {
                            return Err(format!(
                                "`{path}` has a quoted segment that is never closed"
                            ))
                        }
                        Some(b'\\') if quote == b'"' => j += 2,
                        Some(c) if c == quote => break j,
                        Some(_) => j += 1,
                    }
                };
                // `toml` decodes it, because `toml` is what wrote the rules.
                let literal = &path[i..=end];
                let decoded = toml::from_str::<toml::value::Table>(&format!("probe = {literal}"))
                    .ok()
                    .and_then(|table| Some(table.get("probe")?.as_str()?.to_string()))
                    .ok_or_else(|| {
                        format!("`{path}` has a quoted segment that is not a TOML string")
                    })?;
                out.push(decoded);
                i = end + 1;
                while matches!(bytes.get(i), Some(b' ' | b'\t')) {
                    i += 1;
                }
                if !matches!(bytes.get(i), None | Some(b'.')) {
                    return Err(format!(
                        "`{path}` has more than a quoted string in one of its segments"
                    ));
                }
            }
            _ => {
                let end = path[i..].find('.').map_or(bytes.len(), |n| i + n);
                out.push(path[i..end].trim().to_string());
                i = end;
            }
        }
        if i >= bytes.len() {
            return Ok(out);
        }
        i += 1; // The dot.
    }
}

/// Spell one decoded segment the way a document has to carry it.
///
/// The inverse of the decoding `segments` does, and it exists because that
/// decoding created the need: a segment in hand is now a NAME, and a name is not
/// always spellable as itself. A bare key is `A-Za-z0-9_-` and nothing else, so
/// anything holding a dot, a space, a slash or a quote has to go back into the
/// file quoted — and writing `[prices.models.gpt-4.1]` instead of
/// `[prices.models."gpt-4.1"]` names a different table two levels deeper, which
/// still parses.
///
/// The quoting goes through `toml`'s own serializer for the same reason
/// [`array()`] does: a format string cannot escape a `"` or a backslash correctly
/// and would produce either a parse error or a different name.
#[must_use]
pub fn spell(segment: &str) -> String {
    let bare = !segment.is_empty()
        && segment
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b == b'_' || b == b'-');
    if bare {
        return segment.to_string();
    }
    toml::Value::String(segment.to_string()).to_string()
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

/// Where a region's bytes end, for the purpose of taking the region away.
///
/// **A region's `body.end` is the first byte of the NEXT header**, which is what
/// makes it right for parsing the section and wrong for deleting it: everything
/// between this section's last key and that header is inside the range, and what
/// sits there is almost always the comment block an operator wrote to explain the
/// section BELOW. Splicing `region.start..region.body.end` away took somebody
/// else's sentence with it, and a moved entry carried it to a place it was no
/// longer true.
///
/// So the trailing lines are walked backwards over blank and comment lines, and
/// the answer is the first line of that trailing comment run — `body.end` when
/// there is no comment in it. The blank lines ABOVE the run stay inside the range
/// on purpose: they belonged to the section being taken away, and leaving them
/// would make every removal add an empty line to the file.
fn removal_end(text: &str, body: &Range<usize>) -> usize {
    let slice = &text[body.clone()];
    let mut starts: Vec<usize> = vec![0];
    starts.extend(slice.match_indices('\n').map(|(at, _)| at + 1));
    let mut answer = body.end;
    for &start in starts.iter().rev() {
        // The empty remainder after a final newline is not a line.
        if start >= slice.len() {
            continue;
        }
        let line = slice[start..].split('\n').next().unwrap_or_default().trim();
        if line.is_empty() {
            continue;
        }
        if !line.starts_with('#') {
            break;
        }
        answer = body.start + start;
    }
    answer
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
        start: 0,
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

        // The same splitter the caller's path goes through, which is the whole
        // reason there is only one: two scans that agree about what a segment is
        // are the only way a header and a path can ever be compared.
        let path = segments(inner)?;

        let index = seen.entry(path.clone()).or_insert(0);
        let body_end = header_starts.get(n + 1).copied().unwrap_or(bytes.len());

        regions.push(Region {
            path,
            index: *index,
            start,
            body: line_end..body_end,
        });
        *index += 1;
    }

    Ok(regions)
}
