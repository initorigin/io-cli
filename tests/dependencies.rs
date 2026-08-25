//! F7 — the forbidden subsystems are absent.
//! N1 — io-cli contains no second harness.
//!
//! This is the criterion that keeps the archived product from growing back. That
//! product was a fork of a 1.2-million-line agent whose interface could not be
//! lifted out of it, because roughly sixty of its crates duplicated what
//! io-harness already does. The rule that prevents a repeat is not a convention:
//! it is that the moment something is missing, it goes into io-harness and is
//! consumed from here.
//!
//! So this file asserts, mechanically, that io-cli has no agent loop, no provider
//! client, no tool implementation, no sandbox, no policy engine and no session
//! store — by dependency and by source.

use std::collections::BTreeSet;
use std::path::{Path, PathBuf};

/// Exactly what N1 permits. Anything added here has to be argued in the release
/// record, which is the point: the list is short enough that adding to it is a
/// decision somebody makes rather than a line somebody adds.
const ALLOWED: &[&str] = &[
    "io-harness",
    "ratatui",
    "crossterm",
    "tui-textarea",
    "clap",
    "tokio",
    "serde",
    "toml",
    // Ninth, and the first name added since 0.1.0. Syntax highlighting inside a
    // diff, argued in 0.3.0's release record. Its FEATURE SET is asserted below
    // as well as its name — see `n2_syntect_is_taken_with_exactly_the_features_
    // the_cross_compiles_survive`.
    "syntect",
    // Tenth, added in 0.5.0 for `io exec --json`. It serializes a type io-harness
    // declares and already derives `Serialize` for, using the crate io-harness
    // already depends on — so this name buys a correctness property (escaping is
    // not this crate's to get right) rather than a subsystem.
    "serde_json",
    // Eleventh, added in 0.9.0. Decoding an image into pixels, so a picture can be
    // drawn as half-block cells where the terminal speaks no graphics protocol.
    // Already in the tree through io-harness's `media` feature, so declaring it
    // directly adds a name rather than a subtree. Its FEATURE SET is asserted below
    // — see `n3_image_is_taken_with_exactly_the_formats_io_harness_will_accept`.
    "image",
];

/// Crates that would mean a subsystem had been rebuilt here. Matched against
/// direct dependency names, since a transitive one arrives through io-harness and
/// is the harness's business.
const FORBIDDEN: &[(&str, &str)] = &[
    ("reqwest", "an HTTP client"),
    ("hyper", "an HTTP client"),
    ("ureq", "an HTTP client"),
    ("curl", "an HTTP client"),
    ("isahc", "an HTTP client"),
    ("surf", "an HTTP client"),
    ("http", "an HTTP stack"),
    ("rustls", "a TLS stack"),
    ("native-tls", "a TLS stack"),
    ("openssl", "a TLS stack"),
    ("rusqlite", "a database"),
    ("libsqlite3-sys", "a database"),
    ("sqlx", "a database"),
    ("diesel", "a database"),
    ("redb", "a database"),
    ("sled", "a database"),
    ("seccompiler", "a sandbox"),
    ("landlock", "a sandbox"),
    ("libseccomp", "a sandbox"),
    ("caps", "a sandbox"),
    ("nix", "process and sandbox syscalls"),
    ("async-openai", "a provider client"),
    ("eventsource-stream", "a provider response stream"),
];

fn manifest() -> toml::Value {
    let path = Path::new(env!("CARGO_MANIFEST_DIR")).join("Cargo.toml");
    let text = std::fs::read_to_string(path).expect("this crate's own manifest");
    toml::from_str(&text).expect("the manifest parses")
}

fn direct_dependencies() -> BTreeSet<String> {
    let manifest = manifest();
    let table = manifest
        .get("dependencies")
        .and_then(toml::Value::as_table)
        .expect("a dependencies table");
    table.keys().cloned().collect()
}

#[test]
fn f7_n1_the_dependency_set_is_the_one_the_contract_names() {
    let declared = direct_dependencies();
    let allowed: BTreeSet<String> = ALLOWED.iter().map(|name| (*name).to_string()).collect();

    let extra: Vec<&String> = declared.difference(&allowed).collect();
    assert!(
        extra.is_empty(),
        "io-cli has taken dependencies the release does not permit: {extra:?}. \
         If one of them is genuinely needed, it is argued in the release record \
         and added to ALLOWED — it is not added quietly.",
    );

    let missing: Vec<&String> = allowed.difference(&declared).collect();
    assert!(
        missing.is_empty(),
        "these are permitted but no longer used: {missing:?}",
    );
}

#[test]
fn f7_there_is_no_http_client_no_tls_no_database_and_no_sandbox() {
    let declared = direct_dependencies();
    for (crate_name, what) in FORBIDDEN {
        assert!(
            !declared.contains(*crate_name),
            "io-cli depends directly on {crate_name}, which is {what}. \
             That belongs in io-harness and is consumed from there.",
        );
    }
}

/// Every `.rs` file under `src/`.
fn sources() -> Vec<(PathBuf, String)> {
    fn walk(dir: &Path, out: &mut Vec<(PathBuf, String)>) {
        for entry in std::fs::read_dir(dir).expect("src is readable").flatten() {
            let path = entry.path();
            if path.is_dir() {
                walk(&path, out);
            } else if path.extension().is_some_and(|ext| ext == "rs") {
                let text = std::fs::read_to_string(&path).expect("a source file");
                out.push((path, text));
            }
        }
    }
    let mut out = Vec::new();
    walk(&Path::new(env!("CARGO_MANIFEST_DIR")).join("src"), &mut out);
    assert!(!out.is_empty(), "there should be source to check");
    out
}

#[test]
fn f7_no_source_file_loops_over_provider_responses() {
    // The agent loop is io-harness's. This crate calls a provider exactly once —
    // the wizard's verification handshake — and never in a loop; everything else
    // goes through `Session::turn_steered`, which IS the harness's loop.
    let mut calls = Vec::new();
    for (path, text) in sources() {
        let count =
            text.matches(".complete(").count() + text.matches(".complete_streaming(").count();
        if count == 0 {
            continue;
        }
        calls.push((path.clone(), count));

        // A provider call and a loop in the same file is the shape of a second
        // agent loop. Asserted per file rather than per function on purpose: it is
        // coarse, and being coarse is what makes it hard to creep past.
        for keyword in ["\n    loop {", "\n    while ", "\n    for "] {
            assert!(
                !text.contains(keyword),
                "{} both calls a provider and contains a top-level `{}` — \
                 which is what an agent loop looks like",
                path.display(),
                keyword.trim(),
            );
        }
    }

    let total: usize = calls.iter().map(|(_, count)| count).sum();
    assert_eq!(
        total, 1,
        "io-cli should call a provider exactly once, for the wizard's verification \
         handshake. Found {calls:?}",
    );
}

/// The one module 0.7.0 permits a spawn in. A **path**, not a name: a second
/// `shell.rs` nested somewhere else in the tree would be a second spawn, which is
/// exactly what is being prevented.
fn spawning_module() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("src/shell.rs")
}

#[test]
fn f7_no_source_file_runs_a_command_or_touches_the_network() {
    // Running commands **for the agent** is the harness's tool layer, inside its
    // sandbox: a command there is governed by a policy and recorded in the run's
    // trace. A `std::process::Command` written here for that purpose would be a
    // tool implementation that no policy governs and no trace records, and it
    // stays forbidden in every file.
    //
    // 0.7.0 permits the literal in exactly one module, `src/shell.rs`, for
    // exactly one thing that is not that. A `!` line is the operator's own
    // keystroke: they typed it, they govern it, and its output goes into the
    // scrollback and NOT into the run's trace — because the agent did not do it,
    // and a trace that recorded it would be a trace that lies about who acted.
    // That distinction is the whole argument, and it is why one permission does
    // not reopen the rest.
    //
    // The network half is permitted nowhere at all, this module included.
    let permitted = spawning_module();
    let mut spawns = Vec::new();

    for (path, text) in sources() {
        for forbidden in ["TcpStream", "reqwest::", "std::net::"] {
            assert!(
                !text.contains(forbidden),
                "{} contains {forbidden}, which belongs to io-harness",
                path.display(),
            );
        }

        // Spelling the name around is the same act as writing it, and the
        // criterion names it as evasion. `use std::process as p` and
        // `use std::process::{Command}` both put a spawn in a file where neither
        // literal below ever appears, so those spellings are forbidden
        // everywhere — including in the permitted module, which therefore has to
        // write `std::process::Command` in full and is asserted to below. What
        // that buys is that this test, and a reader, only ever have to look for
        // one string.
        for evasion in [
            "use std::process as ",
            "use std::process::{",
            "use std::process::*",
        ] {
            assert!(
                !text.contains(evasion),
                "{} imports std::process under another spelling; a spawn is written \
                 out in full or it is not written",
                path.display(),
            );
        }

        if text.contains("std::process::Command") || text.contains("process::Command::new") {
            spawns.push(path);
        }
    }

    // `read_dir` hands its entries back in whatever order the filesystem holds
    // them, so every list this file compares is sorted first. A test that passes
    // on one machine and fails on another is worse than no test.
    spawns.sort();
    assert_eq!(
        spawns,
        vec![permitted],
        "a process spawn appears somewhere other than src/shell.rs. There is one \
         module permitted to run a command and it is the one the release record \
         argues for; anywhere else is a tool implementation that no policy governs \
         and no trace records.",
    );
}

/// F5 — and the half that a file list cannot state.
///
/// Permitting the spawn in one module is the first half of the argument. The
/// second is that nothing io-harness drives can reach it: the day a `RunEvent`
/// handler can run a command is the day this crate has written a tool and the
/// harness's policy has a hole in it, and that day would arrive without the test
/// above going red.
///
/// Asserted at module granularity — the same coarseness
/// `f7_no_source_file_loops_over_provider_responses` uses, for the same reason.
/// It is crude, and being crude is what makes it hard to creep past.
///
/// Three facts, and together they are the reachability argument:
///
/// 1. **The spawning module names nothing the event stream carries.** No
///    `RunEvent`, no `EventKind`, no `Observer` — and no `Session` or `Store`
///    either, which is also what keeps a `!` line's output out of the trace. So
///    it cannot itself *be* an event handler, whoever calls it.
/// 2. **Nothing but the driver mentions the module.** The event path is
///    `bridge::Observer` → `App::event` → `Events::event` → `diff::cell`, and
///    none of those files may spell a call into it. Aliasing the module is the
///    same evasion as aliasing the type, so `use crate::shell` is forbidden
///    alongside the call syntax.
/// 3. **The value that asks for a spawn is built in one place.**
///    `app::Command::Shell` is constructed exactly once, in `src/app.rs`, by
///    `App::compose` — a `KeyEvent` handler. The driver matches on it. Nothing
///    io-harness drives can produce one, so nothing io-harness drives can reach
///    the spawn. Counted with its opening parenthesis, which is what a
///    construction and a pattern have in common and a doc link does not.
#[test]
fn f5_the_spawn_is_unreachable_from_the_event_path() {
    let permitted = spawning_module();
    let shell = std::fs::read_to_string(&permitted).expect("the spawning module");

    for name in ["RunEvent", "EventKind", "Observer", "Session", "Store"] {
        assert!(
            !shell.contains(name),
            "src/shell.rs names {name}. A module that can see the event stream, the \
             conversation or the trace is a module the agent can reach — or one that \
             can write the operator's own keystroke into a record of what the agent \
             did.",
        );
    }
    assert!(
        shell.contains("std::process::Command"),
        "src/shell.rs is the module permitted to spawn, so it is the module that has \
         to spell the spawn in full — otherwise the gate above is watching a string \
         that is no longer there",
    );

    let driver = Path::new(env!("CARGO_MANIFEST_DIR")).join("src/main.rs");
    let app = Path::new(env!("CARGO_MANIFEST_DIR")).join("src/app.rs");
    let mut callers = Vec::new();
    let mut builders = Vec::new();
    for (path, text) in sources() {
        if path == permitted {
            continue;
        }
        if text.contains("shell::") || text.contains("use crate::shell") {
            callers.push(path.clone());
        }
        if text.contains("Command::Shell(") {
            builders.push((path, text.matches("Command::Shell(").count()));
        }
    }
    callers.sort();
    builders.sort();

    assert_eq!(
        callers,
        vec![driver.clone()],
        "only the driver may call into the spawning module. Anything else is a \
         second route to a process, and the event path is full of things that would \
         look like a reasonable place for one.",
    );

    // The driver's mentions are all matches; `app.rs`'s one is the single
    // construction, and it is inside `App::compose`, which takes a `KeyEvent`.
    let named: Vec<&PathBuf> = builders.iter().map(|(path, _)| path).collect();
    assert_eq!(
        named,
        vec![&app, &driver],
        "`Command::Shell` is spelled somewhere unexpected: {builders:?}",
    );
    assert_eq!(
        builders
            .iter()
            .find(|(path, _)| *path == app)
            .map(|(_, count)| *count),
        Some(1),
        "src/app.rs should build a `Command::Shell` exactly once. Two is two ways to \
         ask for a process, and only one of them was argued.",
    );
}

#[test]
fn f7_the_configuration_is_read_through_the_harness_and_never_parsed_here() {
    // io-harness owns discovery, layering and validation. This crate serializes
    // the harness's own types to write a file and reads it back through `Config`;
    // a `from_str` into a shape of our own would be a second, disagreeing answer
    // to what a configuration file means.
    //
    // **`src/edit.rs` is the one exception and it is identified by path**, the
    // way `src/shell.rs` is permitted `std::process::Command` and for the same
    // reason: the rule is right and the module genuinely needs the thing it
    // bans. 0.16.0 writes one value back into a file an operator wrote by hand,
    // which means locating that value's bytes, which `toml`'s `Spanned` answers
    // and nothing in io-harness does — `Config` exposes no writer, and the
    // private `File` it wraps has no reachable `Serialize`.
    //
    // What keeps the exception honest is the assertion below it: `edit.rs`
    // parses to find **byte offsets** and to prove its own result still parses,
    // and never to decide what a setting means. It names no io-harness
    // configuration type, so it cannot be a second reader of the file's meaning
    // even by accident — which is the property this gate actually protects, and
    // it is asserted rather than promised in a comment.
    let editor = PathBuf::from("src/edit.rs");
    for (path, text) in sources() {
        let permitted = path.ends_with(&editor);
        if !permitted {
            assert!(
                !text.contains("toml::from_str"),
                "{} parses TOML itself; configuration is io-harness's",
                path.display(),
            );
        }
        assert!(
            !text.contains("toml::de::"),
            "{} reaches into TOML deserialization",
            path.display(),
        );
    }

    // The permitted module must exist — a gate whose exception has been renamed
    // away is a gate that quietly stops covering the file it was written for.
    let (_, editor_text) = sources()
        .into_iter()
        .find(|(path, _)| path.ends_with(&editor))
        .expect("src/edit.rs exists; the TOML-parsing exception is written for it");

    for named in [
        "io_harness::Config",
        "io_harness::config",
        "Config::discover",
        "Config::from_toml",
        "CliSettings",
        "ProviderSpec",
    ] {
        assert!(
            !editor_text.contains(named),
            "src/edit.rs names `{named}`. It is permitted to parse TOML only because it \
             works in bytes and never decides what a setting means; the moment it reaches \
             for a configuration type it has become the second reader this gate forbids.",
        );
    }
}

#[test]
fn n1_the_binary_is_named_io_and_there_is_exactly_one() {
    let manifest = manifest();
    let bins = manifest
        .get("bin")
        .and_then(toml::Value::as_array)
        .expect("a [[bin]] section");
    assert_eq!(bins.len(), 1, "one crate, one binary");
    assert_eq!(
        bins[0].get("name").and_then(toml::Value::as_str),
        Some("io"),
    );
}

#[test]
fn n1_the_crate_is_never_published() {
    let manifest = manifest();
    let publish = manifest
        .get("package")
        .and_then(|package| package.get("publish"))
        .and_then(toml::Value::as_bool);
    assert_eq!(
        publish,
        Some(false),
        "the crate name holds seventeen yanked versions and a yank does not free \
         a number; `publish = false` is what makes that mechanical rather than \
         remembered",
    );
}

/// N2 — the feature set, which is the load-bearing half of taking syntect.
///
/// The name alone is not the contract. `syntect`'s DEFAULT features include
/// `regex-onig`, which is oniguruma — a C library. Two of this product's four
/// release artifacts are cross-compiled, to `x86_64-unknown-linux-musl` and
/// `x86_64-pc-windows-msvc`, and a native build step in that path fails inside
/// the release workflow rather than in a test. So a later `default-features =
/// true`, or a merge that drops the feature list, has to fail here — where it
/// costs a red check — instead of there, where it costs the Release.
#[test]
fn n2_syntect_is_taken_with_exactly_the_features_the_cross_compiles_survive() {
    let manifest = manifest();
    let syntect = manifest
        .get("dependencies")
        .and_then(|table| table.get("syntect"))
        .and_then(toml::Value::as_table)
        .expect("syntect is a table, not a bare version string");

    assert_eq!(
        syntect
            .get("default-features")
            .and_then(toml::Value::as_bool),
        Some(false),
        "syntect's default features include regex-onig, which is a C library and \
         breaks the musl and MSVC cross-compiles",
    );

    let features: Vec<&str> = syntect
        .get("features")
        .and_then(toml::Value::as_array)
        .expect("a features array")
        .iter()
        .filter_map(toml::Value::as_str)
        .collect();
    assert_eq!(
        features,
        ["default-syntaxes", "regex-fancy"],
        "the smallest syntect that highlights: the bundled grammars and the \
         pure-Rust regex engine. `default-themes` is deliberately absent — the \
         colours are io-cli's own tokens, which is what keeps a highlighted diff \
         and the rest of the interface one aesthetic and NO_COLOR working from \
         one place.",
    );
}

/// N3 — the same argument as N2, for the decoder 0.9.0 takes.
///
/// `image`'s DEFAULT features are `rayon` plus `default-formats`, and
/// `default-formats` includes `avif` — which is `ravif`, an AV1 *encoder*, along
/// with `exr`, `hdr`, `dds`, `qoi` and `ff`. None of them decodes anything this
/// crate can be handed, because everything this crate renders has already been
/// through `Media::attach`, which accepts nine formats and refuses the rest by
/// name.
///
/// So the list is derived rather than chosen: the four that reach a provider
/// unchanged, and the five io-harness transcodes to PNG on the way in. A merge
/// that drops the list, or a later `default-features = true`, has to fail here.
#[test]
fn n3_image_is_taken_with_exactly_the_formats_io_harness_will_accept() {
    let manifest = manifest();
    let image = manifest
        .get("dependencies")
        .and_then(|table| table.get("image"))
        .and_then(toml::Value::as_table)
        .expect("image is a table, not a bare version string");

    assert_eq!(
        image.get("default-features").and_then(toml::Value::as_bool),
        Some(false),
        "image's default features pull an AV1 encoder and rayon into a crate that \
         only ever decodes a file io-harness already accepted",
    );

    let features: Vec<&str> = image
        .get("features")
        .and_then(toml::Value::as_array)
        .expect("a features array")
        .iter()
        .filter_map(toml::Value::as_str)
        .collect();
    assert_eq!(
        features,
        ["png", "jpeg", "gif", "webp", "bmp", "tiff", "ico", "tga", "pnm"],
        "exactly what `Media::source_type_for` names and something can decode: the \
         four wire formats first, then the five io-harness transcodes. `gif` and \
         `webp` are here and are NOT in io-harness's own list, because the harness \
         passes those two through without decoding them and a half-block cell needs \
         their pixels.",
    );
}

/// N4 — nothing in this crate encodes base64.
///
/// A graphics protocol wants base64, and the obvious move is to take a base64
/// crate. It is not needed: `Media` carries the encoded string already, because
/// io-harness encoded it to put the image on a wire. Taking a crate to recompute
/// a string this process is already holding would be a twelfth name for nothing.
#[test]
fn n4_no_base64_is_computed_here() {
    for (path, text) in sources() {
        assert!(
            !text.contains("base64::"),
            "{} reaches for a base64 crate; `Media::base64` is already the encoded \
             payload a graphics protocol needs",
            path.display(),
        );
    }
}

/// N4 — the grammar table is never touched on the startup path.
///
/// `SyntaxSet::load_defaults_newlines` decompresses every grammar syntect ships.
/// This product's own bar is a first paint inside a hundred milliseconds, and a
/// session that never edits a file should never pay for a highlighter. Asserted
/// structurally rather than by timing anything: the load appears once, in the
/// module that draws diffs, behind a `OnceLock`.
#[test]
fn n4_the_syntax_set_is_loaded_lazily_and_only_by_the_diff_renderer() {
    let mut loaders = Vec::new();
    for (path, text) in sources() {
        if text.contains("load_defaults") {
            loaders.push(path.clone());
        }
    }

    assert_eq!(
        loaders.len(),
        1,
        "the grammar table should be loaded in exactly one place: {loaders:?}",
    );
    assert!(
        loaders[0].ends_with("diff.rs"),
        "only the diff renderer has any reason to load grammars: {loaders:?}",
    );

    let diff = std::fs::read_to_string(&loaders[0]).expect("the diff renderer");
    assert!(
        diff.contains("OnceLock<Highlighter>"),
        "the load has to sit behind a once-cell, or it happens at startup",
    );

    // And the startup path does not reach it. `main.rs` builds the terminal, the
    // config and the session before anything is drawn; a highlighter mentioned
    // there is a highlighter built before the first frame.
    let main = std::fs::read_to_string(
        std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("src/main.rs"),
    )
    .expect("the driver");
    assert!(
        !main.contains("Highlighter") && !main.contains("SyntaxSet"),
        "the driver reached for the highlighter, which puts it on the startup path",
    );
}
