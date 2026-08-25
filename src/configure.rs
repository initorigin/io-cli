//! The configuration file as a surface: what is set, and which file decided it.
//!
//! io-harness has recorded the origin of every key since its 0.30.0 —
//! [`Config::origin`] answers "which file decided this one" and [`Config::origins`]
//! walks the lot — and no io-cli release has ever asked. This module is that
//! question asked, and the answer rendered.
//!
//! # Three scopes, and a fourth answer that is not a file
//!
//! [`Scope`] is `User`, `Project` and `Local`: the operator's own file, the
//! committed `io.toml`, and the gitignored `io.local.toml`. There is no fourth.
//!
//! What there is instead is a key **no file named**, and it is the case worth
//! being careful about. `Config::origin` returns an **empty slice** for it, which
//! is io-harness's own default speaking — and a surface that reached for the
//! lowest-precedence source file to fill the column in would attribute a crate
//! default to a file the operator never wrote it in. That is a lie a reader
//! cannot detect, so [`Decided::Default`] is its own answer and names no path.
//!
//! This crate has paid for that distinction once already: 0.15.0's
//! `home::origin` reported `IO_CONFIG_HOME` for io-cli's own default because
//! `adopt` had set the variable itself, crediting the operator for a choice they
//! never made.
//!
//! # Where a value's text comes from
//!
//! Mostly from io-harness's typed accessors, which are the authority on what a
//! merged configuration means. But **not every section has one**: `MemorySection`
//! is private and there is no `Config::memory()`, so a surface built only on
//! accessors would have a hole exactly where an operator had written something.
//!
//! For those keys the value is **quoted from the file `origin()` names**, through
//! [`crate::edit::value_at`]. Quoting a named file's own bytes is a different act
//! from deciding what a setting means — the origin says which file, and the text
//! is what that file says. Nothing here merges, defaults or interprets.

use std::collections::BTreeSet;
use std::path::PathBuf;

use io_harness::config::{Config, Scope};

/// What decided a key.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Decided {
    /// A file named it. Which scope, and the path as it was read.
    File { scope: Scope, path: PathBuf },
    /// No file named it, so io-harness's own default is in force. This names no
    /// path on purpose.
    Default,
}

impl Decided {
    /// The word shown in the origin column.
    ///
    /// A scope rather than a filename, because three files are called `io.toml`
    /// and only one of them is the operator's own.
    pub fn word(&self) -> &'static str {
        match self {
            Decided::File {
                scope: Scope::User, ..
            } => "user",
            Decided::File {
                scope: Scope::Project,
                ..
            } => "project",
            Decided::File {
                scope: Scope::Local,
                ..
            } => "local",
            Decided::Default => "default",
        }
    }

    /// The file, where there is one.
    pub fn path(&self) -> Option<&std::path::Path> {
        match self {
            Decided::File { path, .. } => Some(path),
            Decided::Default => None,
        }
    }
}

/// One row of the surface.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Setting {
    /// The dotted key, as io-harness spells it.
    pub path: String,
    /// The value as the deciding file writes it, already redacted where it is a
    /// credential. `None` where no file named the key — there is nothing to
    /// quote, and inventing the crate's default here would be this module
    /// claiming to know a value it did not read.
    pub value: Option<String>,
    /// What decided it.
    pub decided: Decided,
}

/// The keys this surface offers even when no file names them.
///
/// io-cli's own list, because io-harness exposes no enumeration of its schema —
/// every section struct is private and `deny_unknown_fields` is the only thing
/// that speaks. A list nothing has to agree with is decoration, so
/// `tests/configure.rs` asserts every key here is one `docs/config.example.toml`
/// documents, and that file is itself gated.
///
/// It is deliberately not exhaustive of the file format: `[[agent]]`, `[[hook]]`,
/// `[[plugin]]` and `[toolchain]` are readable through [`settings`] below but are
/// not editable here, which the release contract excludes by name.
pub const CATALOGUE: &[&str] = &[
    // The boundary, first, because it is the one an operator changes with the
    // most at stake. Four acts and not three: io-harness separates `read` from
    // `write`, and a surface that offered one `fs` row would be inventing a key
    // the schema rejects.
    "policy.defaults.read",
    "policy.defaults.write",
    "policy.defaults.exec",
    "policy.defaults.net",
    "sandbox.mode",
    "sandbox.allow_network",
    "sandbox.force_floor",
    // What one turn may spend.
    "run.max_steps",
    "run.max_tokens",
    "run.max_duration_secs",
    "run.max_retries",
    "run.exec_timeout_secs",
    // What the store keeps, which outlives every run over it.
    "memory.max_entries",
    "memory.max_chars",
    "memory.max_entry_chars",
    // io-cli's own.
    "app.io-cli.theme",
    "app.io-cli.diff",
    "app.io-cli.glyphs",
    "app.io-cli.plain",
];

/// Whether a key's value is a credential and must never be shown in full.
///
/// N2. The test is the key rather than the value, because a credential that
/// happened to look like a word would otherwise be printed, and a key named
/// `api_key` is a credential whatever it contains.
fn is_credential(path: &str) -> bool {
    let last = path.rsplit('.').next().unwrap_or(path);
    matches!(last, "api_key" | "token" | "secret" | "password")
}

/// A value as it is safe to show.
///
/// A `${env:VAR}` or `${file:PATH}` reference is shown **as written**: the
/// variable's name is the information an operator needs and its contents are not,
/// and io-harness substitutes those two forms and nothing else. Anything else in
/// a credential key is reduced to its last four characters, which is enough to
/// tell two keys apart and not enough to use.
pub fn redact(path: &str, value: &str) -> String {
    if !is_credential(path) {
        return value.to_string();
    }
    let bare = value.trim().trim_matches('"');
    if bare.starts_with("${env:") || bare.starts_with("${file:") {
        return value.to_string();
    }
    let tail: String = bare.chars().rev().take(4).collect::<Vec<_>>().into_iter().rev().collect();
    if bare.len() <= 4 {
        "\"set\"".to_string()
    } else {
        format!("\"…{tail}\"")
    }
}

/// Every setting this surface shows, in catalogue order then whatever else the
/// files named.
///
/// The second half is the property that matters: a key an operator wrote which
/// io-cli's catalogue does not know about is still shown, with its origin, rather
/// than being invisible. A surface that only listed what it already knew would
/// hide exactly the keys a reader went looking for.
pub fn settings(config: &Config) -> Vec<Setting> {
    let mut seen: BTreeSet<String> = BTreeSet::new();
    let mut rows: Vec<Setting> = Vec::new();

    for key in CATALOGUE {
        seen.insert((*key).to_string());
        rows.push(setting(config, key));
    }

    // Anything a file named that the catalogue does not carry.
    let mut extra: Vec<String> = config
        .origins()
        .map(|(key, _)| key.to_string())
        .filter(|key| !seen.contains(key))
        .collect();
    extra.sort();
    for key in extra {
        rows.push(setting(config, &key));
    }

    rows
}

/// One key, resolved.
pub fn setting(config: &Config, key: &str) -> Setting {
    let origins = config.origin(key);
    let decided = match origins.last() {
        // The last origin is the winning one: the scopes merge in precedence
        // order, and a key more than one file named lists them in that order.
        Some(origin) => Decided::File {
            scope: origin.scope,
            path: origin.path.clone(),
        },
        None => Decided::Default,
    };

    let value = decided.path().and_then(|path| {
        let text = std::fs::read_to_string(path).ok()?;
        crate::edit::value_at(&text, key)
    });

    Setting {
        path: key.to_string(),
        value: value.map(|v| redact(key, &v)),
        decided,
    }
}

/// The rows as the picker draws them: the key, then its value and its origin.
///
/// Content before metadata, which is this product's rule everywhere: the key is
/// the label and the value rides the detail column with the origin word after it.
pub fn rows(settings: &[Setting]) -> Vec<crate::picker::Row> {
    settings
        .iter()
        .map(|setting| {
            let value = setting.value.as_deref().unwrap_or("—");
            crate::picker::Row::with_detail(
                setting.path.clone(),
                format!("{value}   {}", setting.decided.word()),
            )
        })
        .collect()
}
