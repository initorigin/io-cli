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

#[test]
fn f7_no_source_file_runs_a_command_or_touches_the_network() {
    // Running commands is the harness's tool layer, inside its sandbox. A
    // `std::process::Command` here would be a tool implementation that no policy
    // governs and no trace records.
    for (path, text) in sources() {
        for forbidden in [
            "std::process::Command",
            "process::Command::new",
            "TcpStream",
            "reqwest::",
            "std::net::",
        ] {
            assert!(
                !text.contains(forbidden),
                "{} contains {forbidden}, which belongs to io-harness",
                path.display(),
            );
        }
    }
}

#[test]
fn f7_the_configuration_is_read_through_the_harness_and_never_parsed_here() {
    // io-harness owns discovery, layering and validation. This crate serializes
    // the harness's own types to write a file and reads it back through `Config`;
    // a `from_str` into a shape of our own would be a second, disagreeing answer
    // to what a configuration file means.
    for (path, text) in sources() {
        assert!(
            !text.contains("toml::from_str"),
            "{} parses TOML itself; configuration is io-harness's",
            path.display(),
        );
        assert!(
            !text.contains("toml::de::"),
            "{} reaches into TOML deserialization",
            path.display(),
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
