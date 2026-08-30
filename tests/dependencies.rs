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
    // `tui-textarea` stood here from 0.1.0 and was **removed** in 0.18.0 — the
    // first name this crate has ever given back. It pinned `ratatui ^0.29.0`
    // through every feature path it had, 0.29 pinned `lru ^0.12.0`, and `lru`
    // carried an advisory whose fix landed in 0.16.3 and was never backported to
    // the 0.12 line. Upstream published nothing after 2024-10-22, so there was no
    // version to wait for: the composer's editing model became this crate's own
    // (`src/editor.rs`) and the name went.
    // `n1_the_composer_is_free_and_no_lru_is_in_the_advisorys_range` below asserts
    // it is gone from the lockfile — a count on its own would be satisfied by any
    // replacement — and asserts the `lru` still in the tree is outside the
    // advisory's range, which is the property that actually mattered.
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

fn names_in(table: Option<&toml::Value>) -> BTreeSet<String> {
    table
        .and_then(toml::Value::as_table)
        .map(|table| table.keys().cloned().collect())
        .unwrap_or_default()
}

/// Every crate this manifest asks for by name — which is not the same set as
/// `[dependencies]`, and 0.17.0 is the release that stopped pretending it was.
///
/// N1 is about what `cargo tree --depth 1` prints, and `--depth 1` prints more
/// tables than one. A `[build-dependencies]` entry is a crate compiled and *run*
/// on the build machine. A `[target.'cfg(unix)'.dependencies] nix` is a direct
/// dependency on three of the four release artifacts and invisible on the fourth
/// — and `nix` is in `FORBIDDEN` below precisely because it is the shape a
/// sandbox arrives in, which is also the shape that arrives `cfg`-guarded. Before
/// this release either one could have been added and no gate in this file would
/// have said a word.
///
/// So they are folded in here rather than given a test of their own: every
/// assertion built on this function — the ALLOWED set in both directions, and the
/// forbidden-subsystem sweep — now covers them for free, and a dependency is a
/// dependency whichever `cfg` happens to guard it.
fn direct_dependencies() -> BTreeSet<String> {
    let manifest = manifest();
    let mut names = names_in(manifest.get("dependencies"));
    assert!(
        !names.is_empty(),
        "a dependencies table — an empty read here would pass every assertion below \
         it for the wrong reason",
    );
    names.extend(names_in(manifest.get("build-dependencies")));

    // `[target.<cfg>.dependencies]`, for every `<cfg>` the manifest names. The
    // cfg keys are not enumerated: a gate that listed the platforms it knew about
    // would stop covering the platform somebody adds.
    if let Some(targets) = manifest.get("target").and_then(toml::Value::as_table) {
        for spec in targets.values() {
            names.extend(names_in(spec.get("dependencies")));
            names.extend(names_in(spec.get("build-dependencies")));
        }
    }
    names
}

/// N1 — the other half of `--depth 1`, which is deliberately not in `ALLOWED`.
///
/// A dev-dependency never reaches an artifact, so putting `tempfile` on the list
/// the release record argues over would blur what that list means: `ALLOWED` is
/// what ships. But `cargo tree --depth 1` prints dev-dependencies, N1 says that
/// output names exactly what it names today, and a test-only crate is still a
/// crate somebody has to justify — `tempfile` is here because a run store needs a
/// directory that cleans itself up, and the next name needs an argument of its
/// own. Pinning the set costs the same red check as `ALLOWED` does and keeps the
/// two meanings apart.
#[test]
fn n1_the_only_test_only_crate_is_the_one_that_makes_a_temporary_directory() {
    let manifest = manifest();
    let dev: Vec<String> = names_in(manifest.get("dev-dependencies"))
        .into_iter()
        .collect();
    assert_eq!(
        dev,
        vec!["tempfile".to_string()],
        "the dev-dependency set changed. `cargo tree --depth 1` prints these too, \
         so a name added here grows the crate by N1's own measure and is argued in \
         the release record like any other.",
    );

    // And no `[target.<cfg>.dev-dependencies]` either, which would be the same
    // name arriving where the assertion above cannot see it.
    if let Some(targets) = manifest.get("target").and_then(toml::Value::as_table) {
        for (spec, table) in targets {
            assert!(
                names_in(table.get("dev-dependencies")).is_empty(),
                "[target.{spec}.dev-dependencies] hides a test-only crate from the \
                 assertion above",
            );
        }
    }
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

/// N1: the two crates 0.18.0 gave back are gone from the whole tree, not just
/// from the manifest.
///
/// **The absence of these two names is the property, and the count is not.** A
/// dependency count stays constant under a substitution, so `ALLOWED` shrinking
/// by one proves only that something left — it would be equally satisfied by
/// swapping `tui-textarea` for another editing widget, which is the one outcome
/// this release exists to avoid.
///
/// `lru` is the whole reason, and **it is still in the tree** — which is the
/// correction this test exists in its present form to record. The advisory was
/// never about the crate being present; it was about the *version*.
/// GHSA-rhfx-m35p-ff5j covers `>= 0.9.0, < 0.16.3`, `lru` under `ratatui` 0.29
/// was 0.12.5, and the 0.12 line ends there with no backport. Under the 0.30
/// line the layout cache moved down into `ratatui-core`, which takes a current
/// `lru` — so the name reappears at a version the advisory does not cover. See
/// `US-IO-CLI-0.18.0-I02`.
///
/// So this asserts the property that actually matters: no `lru` in the
/// vulnerable range. It reads `Cargo.lock` rather than the manifest, because
/// `lru` is transitive at every point in this story and an advisory does not
/// care which level of the tree carries the crate.
///
/// `ratatui` must still be there, which is what stops this passing on a tree that
/// has lost the terminal library altogether.
#[test]
fn n1_the_composer_is_free_and_no_lru_is_in_the_advisorys_range() {
    let lock = std::fs::read_to_string(Path::new(env!("CARGO_MANIFEST_DIR")).join("Cargo.lock"))
        .expect("Cargo.lock exists; this crate is a binary and commits its lockfile");

    let mut names: Vec<(&str, &str)> = Vec::new();
    let mut name: Option<&str> = None;
    for line in lock.lines() {
        if let Some(found) = line
            .strip_prefix("name = \"")
            .and_then(|r| r.strip_suffix('"'))
        {
            name = Some(found);
        } else if let Some(version) = line
            .strip_prefix("version = \"")
            .and_then(|r| r.strip_suffix('"'))
        {
            if let Some(found) = name.take() {
                names.push((found, version));
            }
        }
    }

    assert!(
        !names.iter().any(|(name, _)| *name == "tui-textarea"),
        "`tui-textarea` is back in Cargo.lock. The editing model it provided is \
         `src/editor.rs` now, and its `ratatui ^0.29.0` pin is what held this \
         crate on the vulnerable `lru` line for two years.",
    );

    for (name, version) in names.iter().filter(|(name, _)| *name == "lru") {
        // The range is `>= 0.9.0, < 0.16.3`, and every version in it is `0.x`,
        // so comparing the minor is enough to place it: 0.16 is the first minor
        // that can be outside, and inside it only 0.16.3 and later are.
        let mut parts = version.split('.').map(|p| p.parse::<u32>().unwrap_or(0));
        let (major, minor, patch) = (
            parts.next().unwrap_or(0),
            parts.next().unwrap_or(0),
            parts.next().unwrap_or(0),
        );
        let vulnerable = major == 0 && (minor < 16 || (minor == 16 && patch < 3)) && minor >= 9;
        assert!(
            !vulnerable,
            "`{name} {version}` is inside GHSA-rhfx-m35p-ff5j (>= 0.9.0, < 0.16.3). \
             The 0.12 line was never patched, so there is no in-range update — \
             whatever pulled this in has to move instead.",
        );
    }

    assert!(
        names.iter().any(|(name, _)| *name == "ratatui"),
        "ratatui is not in the lockfile at all, so this gate is passing \
         vacuously rather than because the tree is actually clean",
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
    // goes through the harness's own steered turn entry points, which ARE the
    // loop.
    //
    // **0.17.0 amends this gate rather than relaxing it.** `/context` can only
    // say what is in the model's window by reading the request on its way to the
    // provider, so `src/provider.rs` gained `Watched`, which implements the trait
    // by handing every call straight to the provider it wraps. Those forwards are
    // provider calls by the letter of the count and are the opposite of a second
    // agent loop by its intent — so they are exempted BY PATH, and a new
    // assertion below keeps the exemption a boundary: a forward must hand the
    // call to the inner provider and must never look at what comes back. The
    // shape is 0.7.0's amendment of the spawn ban, for the same reason.
    let delegating = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("src")
        .join("provider.rs");
    let forwarding = std::fs::read_to_string(&delegating).expect("the provider module");
    // Whitespace squashed, because rustfmt decides where a long call breaks and a
    // count that matched a contiguous string found two of three the first time
    // this ran — the same reason `tests/contract.rs` squashes before it looks for
    // the turn entry points. An assertion about where a newline sits is an
    // assertion about formatting.
    let squashed: String = forwarding.chars().filter(|c| !c.is_whitespace()).collect();
    let forwards = squashed.matches("self.inner.complete").count();
    // Three, and the third is the one that matters: `complete_streaming_calls` is
    // what io-harness's own loop calls, and its TRAIT DEFAULT drops the tool-call
    // sink and forwards to `complete_streaming`. A decorator that did not override
    // it would compile, record the request, and silently take tool-call streaming
    // away from every run it wrapped.
    assert!(
        forwards >= 3,
        "the decorator forwards every completion method the trait declares — including \
         `complete_streaming_calls`, whose default would cost the run its tool-call \
         streaming; found {forwards}",
    );
    // A forward hands the request on and returns what the inner provider
    // returned. If this module ever reads a response — a token, a tool call, a
    // message — it has stopped decorating and started interpreting, which is the
    // first half of an agent loop wherever it is written.
    for interpreting in [
        ".choices",
        ".tool_calls",
        "match response",
        "let response =",
    ] {
        assert!(
            !forwarding.contains(interpreting),
            "src/provider.rs reads a provider response (`{interpreting}`) — a decorator \
             records the REQUEST and returns the answer untouched",
        );
    }

    // **The 0.26.0 exemption, held to being a boundary.** `src/provider.rs` is the
    // one file allowed a loop beside a provider call, because the chain asks each
    // configured vendor in turn when the one before it failed. Two properties keep
    // that from being an agent loop, and both are asserted rather than intended.
    //
    // First: every loop in the file iterates the chain's own links. A loop over
    // anything else — steps, messages, attempts at the same vendor — is the shape
    // this gate exists to refuse, and it would be refused here by name.
    let forwarding_code = code_of(&forwarding);
    for line in forwarding_code.lines().filter(|line| {
        let trimmed = line.trim_start();
        trimmed.starts_with("for ")
            || trimmed.starts_with("loop {")
            || trimmed.starts_with("while ")
    }) {
        assert!(
            line.contains("self.links.iter()"),
            "src/provider.rs loops over something that is not the chain's links: \
             `{}`. The exemption covers asking the next vendor and nothing else.",
            line.trim(),
        );
    }

    // Second: the decision to try another vendor is io-harness's own predicate.
    // A condition written here would be this crate forming an opinion about which
    // failures are worth retrying — which is the judgement that turns a fallover
    // into a loop with a policy.
    assert!(
        forwarding.contains("kind.is_retryable()"),
        "the chain must fall through on `ProviderErrorKind::is_retryable`, which is \
         what io-harness's own `Fallback` and its own in-run retry both ask",
    );

    let mut calls = Vec::new();
    for (path, text) in sources() {
        // The forwards are subtracted rather than the file being skipped, so the
        // decorator is still held to the no-top-level-loop rule below — an
        // exemption that took a module out of the sweep entirely would exempt it
        // from the half of this gate that matters most.
        let forwarded = if path == delegating { forwards } else { 0 };
        let count = (text.matches(".complete(").count()
            + text.matches(".complete_streaming(").count())
        .saturating_sub(forwarded);
        if count == 0 {
            continue;
        }
        calls.push((path.clone(), count));

        // A provider call and a loop in the same file is the shape of a second
        // agent loop. Asserted per file rather than per function on purpose: it is
        // coarse, and being coarse is what makes it hard to creep past.
        //
        // **0.26.0 amends this gate rather than relaxing it, and the amendment
        // makes it stricter in the place it was weakest.** Until this release the
        // three needles were anchored at four spaces, so they saw a loop written
        // at the top level of a function and were blind to one written inside an
        // `impl` — where an agent loop would actually be written. The release that
        // introduced this crate's first loop around provider calls is the release
        // that has to notice that, so the needles are now unanchored and every
        // loop in a provider-calling file is seen.
        //
        // `src/provider.rs`'s chain is exempted BY PATH, and the exemption is a
        // boundary rather than a hole: the assertions below it require that the
        // only thing it loops over is its own list of links, and that the decision
        // to try the next one is io-harness's `is_retryable` and not a predicate
        // of this crate's. Iterating vendors on a failure the dependency has
        // classified is not an agent loop — it never looks at a successful
        // response, which the needles above already require.
        //
        // The shape is 0.7.0's amendment of the spawn ban and 0.17.0's of this
        // gate, for the same reason both times.
        //
        // **Comments are stripped before the sweep looks**, which is the other
        // half of unanchoring the needles. `while` and `for` are ordinary English
        // words, and the first run of the wider gate refused `src/verify.rs` for
        // three doc comments containing the word "while" — a gate that reads prose
        // forbids a file from explaining itself, which this repository has now
        // paid for in 0.16.0, 0.19.0 and twice in one wave in 0.25.0. The rule
        // learned there is that the stripping belongs in the SWEEP and not only in
        // the exception, so it is here.
        let code = code_of(&text);
        if path != delegating {
            for keyword in ["loop {", "while ", "for "] {
                assert!(
                    !code.contains(keyword),
                    "{} both calls a provider and contains a `{}` — \
                     which is what an agent loop looks like",
                    path.display(),
                    keyword.trim(),
                );
            }
        }
    }

    // **The count is asserted over every file EXCEPT the delegating one, and that
    // is 0.26.0's amendment.** Subtracting the provider module's calls one by one
    // was how this held through 0.17.0, and it stopped being honest the moment the
    // module gained a chain: `Vendor` delegates across four arms and `Chain` asks
    // each link in turn, so the subtraction would have had to grow until it
    // cancelled everything the file contains — a gate that goes vacuous without
    // going red, which is the failure this suite has recorded twice.
    //
    // So the file is taken out of the count and held instead to the four
    // properties above, each of which is specific and each of which fails loudly:
    // its loops iterate only the chain's links, the decision to try another vendor
    // is io-harness's own predicate, it never reads a response, and it forwards
    // every completion method the trait declares. Those say what "no second agent
    // loop" actually means here, where a number no longer can.
    let elsewhere: usize = calls
        .iter()
        .filter(|(path, _)| path != &delegating)
        .map(|(_, count)| count)
        .sum();
    assert_eq!(
        elsewhere, 1,
        "outside src/provider.rs, io-cli should call a provider exactly once — the \
         wizard's verification handshake. Found {calls:?}",
    );

    // And the exempted file is still held to naming what it calls. Every call it
    // makes is on a link of the chain, on the inner provider of a decorator, or on
    // the arm of the vendor enum — never on a provider this module built in order
    // to ask it something, which is what the count used to prevent.
    //
    // Counted on whitespace-squashed text rather than line by line, for the reason
    // the forward count above already gives: rustfmt decides where a long call
    // breaks, and an assertion about where a newline sits is an assertion about
    // formatting. `link.complete_streaming_calls(..)` is split across three lines
    // by rustfmt and was refused by the first, line-based version of this check.
    let provider_module = code_of(&std::fs::read_to_string(&delegating).expect("the module"));
    let squashed_module: String = provider_module
        .chars()
        .filter(|c| !c.is_whitespace())
        .collect();
    let made = squashed_module.matches(".complete").count();
    let named: usize = ["self.inner.complete", "p.complete", "link.complete"]
        .iter()
        .map(|receiver| squashed_module.matches(receiver).count())
        .sum();
    assert_eq!(
        made, named,
        "src/provider.rs makes {made} provider calls and only {named} of them are on \
         a decorator's inner provider, a vendor arm or a chain link — the rest are \
         this module asking a provider something of its own, which is what the count \
         outside this file exists to prevent",
    );
}

/// The modules permitted a spawn: `src/shell.rs` since 0.7.0, `src/fetch.rs`
/// since 0.29.0. A **set of exact paths**, and both halves of that are load
/// bearing.
///
/// **Paths, not names**, because a second `shell.rs` nested somewhere else in the
/// tree would be a second spawn, which is exactly what 0.7.0 was preventing.
///
/// **Exact, compared as a set — never a substring or a `contains` over the path's
/// text**, which is the second sabotage F11 names. `src/fetching/anything.rs`
/// matches no entry in this list and is refused; a gate written as
/// `path.to_string_lossy().contains("fetch")` would admit it and go on passing
/// while a third module spawned. The whole value of a permission list is that
/// widening it is an edit somebody makes on purpose, and a substring match is a
/// list that widens itself.
///
/// **Sorted here rather than at every call site.** `read_dir` hands entries back
/// in whatever order the filesystem holds them, the lists compared against this
/// one are sorted, and a test that passes on one machine and fails on another is
/// worse than no test.
fn spawning_modules() -> Vec<PathBuf> {
    let src = Path::new(env!("CARGO_MANIFEST_DIR")).join("src");
    let mut permitted = vec![src.join("fetch.rs"), src.join("shell.rs")];
    permitted.sort();
    permitted
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
    // **0.29.0 amends this gate rather than relaxing it, and permits a second
    // module: `src/fetch.rs`.** io-harness owns git and publishes no way to run a
    // subcommand of it — `Git`, `Git::run` and `GitCmd` are `pub(crate)` there,
    // and its own tests assert no argv it builds can carry `clone` — so a
    // marketplace cannot be brought down through the dependency, and the only
    // alternative to a spawn is an HTTP client with a TLS stack under it and a
    // second network path beside io-harness's. That is a bigger hole in N1 than
    // one more permitted path, and it is the trade the release record argues.
    //
    // The permission is a boundary rather than a hole, and the boundary is
    // asserted twice below: `f10_the_fetch_spawns_git_and_builds_no_argument_out_
    // of_a_string` holds the new module to the four properties F10 names, and
    // `f5_the_spawn_is_unreachable_from_the_event_path` holds BOTH permitted
    // modules to naming nothing the event stream carries. What is not relaxed by
    // any of it: the aliased spellings below stay forbidden in every file, this
    // one included, and a THIRD file naming the literal still fails — naming both
    // permitted paths when it does, so the reader is told what the list is rather
    // than left to find it.
    //
    // The network half is permitted nowhere at all, these modules included.
    let permitted = spawning_modules();
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
    assert!(
        only_the_permitted_spawn(&spawns),
        "a process spawn appears somewhere other than the two modules the release \
         records argue for. Found {spawns:?}; exactly these are permitted, by exact \
         path: {permitted:?} — one for the operator's own `!` line, one for the `git \
         clone` that brings a marketplace down. Anywhere else is a tool \
         implementation that no policy governs and no trace records.",
    );
}

/// Is `found` **exactly** the permitted set?
///
/// A named predicate rather than an `assert_eq!` inline above, and the reason is
/// the only reason: **F11's second sabotage needs somewhere to run.** That arm is
/// "widen the permitted set to a substring match rather than a path set", and a
/// widening like that makes this file *more* permissive — so nothing fails, and
/// the gate goes vacuous without going red. There is no way to observe it from
/// the call site, because the call site's own list is the thing being widened.
///
/// With the comparison named, `f11_the_permitted_spawn_set_is_exact_paths…` can
/// hand it a near-miss set and watch it refuse. Rewrite this as a comparison of
/// **modules** rather than of files — `p.file_stem()` matched as a prefix,
/// `p.starts_with(q.with_extension(""))`, or any match over the path text with the
/// extension dropped — and that test goes red naming the file it just admitted.
/// Those are the widenings the list below discriminates, and they are the shapes a
/// rewrite reaches for, because "the fetch module" is how a person says it.
///
/// **What the near-miss list does not catch, said here rather than implied.** A
/// literal `found.iter().all(|p| permitted.iter().any(|q| p.starts_with(q)))`
/// leaves that test green: `Path::starts_with` is component-wise, the permitted
/// entries are files, and `src/fetching.rs`, `src/fetch_marketplace.rs`,
/// `src/fetch/mod.rs` and `src/shell_out.rs` are every one of them refused by it
/// too. It is a real weakening all the same — `all` stops requiring that each
/// permitted module still be *present*, so a set that had lost one would pass —
/// and the `==` in this function is the only thing that catches it. This doc named
/// that rewrite as one the near-misses kill until 0.29.0; they never did, and
/// naming a rewrite a gate cannot discriminate is the same vacuity in prose that
/// 0.25.0 and 0.27.0 shipped in code.
///
/// This product has shipped a gate that could not fail in 0.25.0 and again in
/// 0.27.0, and 0.28.0 recorded the rule it broke both times: enumerate the arms
/// and check each has a site before trusting the set.
fn only_the_permitted_spawn(found: &[PathBuf]) -> bool {
    found == spawning_modules().as_slice()
}

/// **F11 — the permitted set is exact paths, and widening it is an edit somebody
/// makes on purpose.**
///
/// The sibling arm of F11 — a third file naming the spawn — is already covered:
/// `f7_no_source_file_runs_a_command_or_touches_the_network` sweeps `src/` and
/// compares what it finds against the list. This one covers the arm that sweep
/// cannot see, because a gate that has been widened refuses nothing and therefore
/// fails nothing.
///
/// Each near-miss below shares a stem, a stem prefix or a module directory with a
/// permitted path and is a different file. Under the exact comparison every one is
/// refused; under a stem match, a stem-prefix match, or any comparison that drops
/// the extension, at least one is admitted and this test fails naming it. A
/// component-wise `Path::starts_with` is *not* among them — see the note on
/// `only_the_permitted_spawn`, which says what does and does not catch that.
#[test]
fn f11_the_permitted_spawn_set_is_exact_paths_and_never_a_substring() {
    let src = Path::new(env!("CARGO_MANIFEST_DIR")).join("src");
    let permitted = spawning_modules();

    // The control. Without it a predicate that answered `false` for everything
    // would satisfy every assertion below while refusing the real modules too.
    assert!(
        only_the_permitted_spawn(&permitted),
        "the permitted set does not admit itself, so nothing below means anything",
    );

    for near in [
        // Its stem opens with the permitted stem, which is what a `file_stem`
        // prefix match admits.
        src.join("fetching.rs"),
        // The permitted stem, whole, followed by a separator of its own — what an
        // extension-dropped text match admits.
        src.join("fetch_marketplace.rs"),
        // A module directory named for the permitted file, which is what
        // `p.starts_with(q.with_extension(""))` admits, and what a comparison of
        // module names rather than of files admits.
        src.join("fetch").join("mod.rs"),
        // And the same three shapes against the other permitted module, so the
        // property is asserted for both rather than for the new one only.
        src.join("shell_out.rs"),
    ] {
        let mut widened = permitted.clone();
        widened.push(near.clone());
        widened.sort();
        assert!(
            !only_the_permitted_spawn(&widened),
            "{} is admitted beside the permitted modules. The set is compared by \
             exact path for exactly this reason: a substring, stem or prefix match \
             is a permission list that widens itself, and a third module would then \
             spawn while this file went on passing.",
            near.display(),
        );
    }
}

/// **F10 — the four properties that keep 0.29.0's spawn exemption a boundary.**
///
/// The path in `spawning_modules` is the permission; this is what the permission
/// is *for*, asserted over the module's own text so that a later edit which
/// quietly widens it has to go red here.
///
/// 1. **One spawn, and its program is the literal `git`.** Asserted as a count and
///    as the constant, because either alone is escapable: a second
///    `Command::new` would be a second program with no argument about it, and a
///    program read from a variable is how a spawn of git becomes a spawn of
///    whatever the variable held.
/// 2. **No shell.** Nothing in the file names one, so there is no line for a
///    metacharacter in a repository name to be interpreted by.
/// 3. **The argv is built by a function that never sees the repository name.**
///    `argv(url: &str, into: &Path)` is handed a URL that `url()` assembled out of
///    `HOST` and a `Named` `resolve()` has already held to its alphabet, and a
///    destination path — and nothing else. The sabotage F10 names, an argv element
///    interpolated from `named.repo`, cannot be written inside it without first
///    widening that signature, so **the signature is the assertion**, and it is
///    asserted below. Paired with the structural half: the argv reaches the spawn
///    as the vector that pure function returned, in one `.args(…)`, so there is no
///    per-argument builder call where an interpolation could hide and nothing that
///    runs differs from what `tests/fetch.rs` asserts.
///
///    **The `format!` ban beside it is a backstop against an idiom, not the
///    property, and this doc claimed otherwise until 0.29.0.** `src/fetch.rs`
///    builds every string it produces with `String::push_str` — `url()` and
///    `Fetched::sentence()` both — so an interpolation written in the module's own
///    idiom passes a spelling ban without noticing it. The ban is kept because it
///    is free and because `format!` is what a hurried edit reaches for; what
///    actually holds the property is `resolve()`'s allow-list, asserted
///    behaviourally in `tests/fetch.rs` by the test that a name which could become
///    an argument or leave the directory is refused, and the five owned elements,
///    asserted there by
///    `f10_the_program_is_the_literal_git_and_the_argv_is_five_owned_elements`.
///    A gate that names a sabotage it cannot catch is worse than one that states
///    its limits, and this product has shipped the first kind three times.
/// 4. **It names nothing the event stream carries** — asserted for both permitted
///    modules in one loop by `f5_the_spawn_is_unreachable_from_the_event_path`,
///    which is where the same property already lives for `src/shell.rs`.
///
/// Read against `code_of`, so the module is free to explain itself in prose. A
/// gate that reads comments forbids a file from naming what it deliberately does
/// not do, and this repository has now paid for that in 0.16.0, 0.19.0 and twice
/// in one wave in 0.25.0.
#[test]
fn f10_the_fetch_spawns_git_and_builds_no_argument_out_of_a_string() {
    let module = Path::new(env!("CARGO_MANIFEST_DIR")).join("src/fetch.rs");
    let text = std::fs::read_to_string(&module)
        .expect("src/fetch.rs exists; 0.29.0's spawn exemption is written for it");
    let code = code_of(&text);

    assert_eq!(
        code.matches("Command::new(").count(),
        1,
        "src/fetch.rs spawns more than once. The exemption is for a `git clone` and \
         a second spawn is a second argument nobody has made.",
    );
    assert!(
        code.contains("Command::new(PROGRAM)"),
        "src/fetch.rs spawns something that is not the module's own constant. A \
         program that comes from a variable is a spawn of whatever that variable \
         held, and the permission was for git.",
    );
    assert!(
        code.contains("pub const PROGRAM: &str = \"git\";"),
        "the program constant is no longer the literal `git`, so the assertion \
         above is watching a name that means something else",
    );

    for shellish in [
        "/bin/sh",
        "cmd.exe",
        "COMSPEC",
        "SHELL",
        "powershell",
        "bash",
    ] {
        assert!(
            !code.contains(shellish),
            "src/fetch.rs names `{shellish}`. The fetch runs one program directly; a \
             shell between it and git would put every character of a repository name \
             back into a grammar.",
        );
    }

    // **The assertion the sabotage actually dies on.** An argv element built out
    // of the repository name has to get at the repository name first, and the one
    // function that builds the argv is handed a URL and a path. Widen it to take a
    // `Named` — which is the first line of writing that sabotage — and this goes
    // red naming the signature.
    assert!(
        code.contains("pub fn argv(url: &str, into: &Path) -> Vec<OsString>"),
        "the argv builder no longer takes a URL and a destination and nothing \
         else. F10's sabotage is an argv element interpolated from the repository \
         name; that name reaches this function only through a wider signature, so \
         the signature is what is pinned. If it genuinely has to change, the \
         behavioural assertions in tests/fetch.rs are what must be re-argued \
         first.",
    );
    // **0.31.0 pins a second builder, and this is where that gate was re-argued
    // rather than relaxed.** An index entry may name a commit, and a shallow clone
    // checked out at one is not a single `git clone`: `--revision` landed in git
    // 2.49, this product names no git floor, so the portable route is four
    // invocations. The literal that used to be asserted here — `.args(argv(` at
    // the spawn — cannot survive that, because the spawn now takes a finished list
    // from a loop rather than calling one builder inline.
    //
    // What replaces it is stronger rather than weaker, and the difference is worth
    // stating because 0.31.0's own risk list names "relaxing F10 to fit both
    // shapes" as a failure mode. Before: one builder's signature pinned, and the
    // spawn asserted to call it. After: **two** builders' signatures pinned, and
    // the spawn asserted to take a `Vec<OsString>` **parameter** — so it cannot
    // add an element at the call site even in principle, which the inline form
    // could have done and was only prevented from doing by `!contains(".arg(")`.
    // The count of spawns is unchanged at one, and every other absence below still
    // holds.
    assert!(
        code.contains("pub fn steps(url: &str, into: &Path, at: &Pin) -> Vec<Vec<OsString>>"),
        "the pinned-fetch builder no longer takes a URL, a destination and a pin \
         and nothing else. It is the second place an argv element could be \
         interpolated from something a stranger's index wrote, so its signature is \
         pinned for the reason `argv`'s is.",
    );
    assert!(
        code.contains("fn run(argv: Vec<OsString>)"),
        "the one spawn no longer takes a finished argv as a parameter. That is the \
         property that makes the assertions above load-bearing: the spawn never \
         sees the parts, so every element it passes was built by a pure function \
         `tests/fetch.rs` asserts the output of.",
    );
    // The backstop, and it is a backstop: this module builds every string it
    // produces with `String::push_str`, so an interpolation written in its own
    // idiom would pass this and die on the assertion above instead. Kept because
    // `format!` is what a hurried edit reaches for and the check is free.
    assert!(
        !code.contains("format!"),
        "src/fetch.rs builds a string with `format!`. The rule is written as an \
         absence rather than as a review of each call so that there is nothing to \
         argue about at the next edit — and it is the weaker half of F10's third \
         property, not the whole of it.",
    );
    assert!(
        !code.contains(".arg("),
        "src/fetch.rs adds arguments one at a time. Every element goes through the \
         pure `argv` function, in one `.args(…)`, so what runs is what \
         tests/fetch.rs asserts — a per-argument builder is where an interpolation \
         hides from both.",
    );
    assert!(
        code.contains(".args(argv)"),
        "the spawn no longer takes its arguments from the finished list handed to \
         it, so tests/fetch.rs is asserting the shape of a function nothing runs",
    );
    assert_eq!(
        code.matches(".args(").count(),
        1,
        "src/fetch.rs passes arguments in more than one place. Every element goes \
         through a pure builder and reaches the program through the single `run`, \
         so a second `.args(` is a second argv nothing asserts the shape of.",
    );
}

/// F5 — and the half that a file list cannot state.
///
/// Permitting the spawn in a named module is the first half of the argument. The
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
/// 1. **Neither spawning module names anything the event stream carries.** No
///    `RunEvent`, no `EventKind`, no `Observer` — and no `Session` or `Store`
///    either, which is also what keeps a `!` line's output out of the trace. So
///    neither can itself *be* an event handler, whoever calls it. **Asserted over
///    both permitted paths since 0.29.0**, in one loop rather than a copy: the
///    property is what makes a spawn exemption survivable and a second module
///    that was held to less would be the hole the first one was not.
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
///
/// **Facts 2 and 3 are about `src/shell.rs` and stay about `src/shell.rs`.** The
/// module 0.29.0 adds has no reachability argument of this shape to make yet —
/// nothing calls it, and when something does it will be a command surface rather
/// than the driver — so this test admits its callers by not asserting over them,
/// and says so here rather than leaving a reader to infer it from a needle. What
/// it does NOT do is widen the shell's own rule: `shell::` is still spelled in one
/// file, and that file is still the driver.
#[test]
fn f5_the_spawn_is_unreachable_from_the_event_path() {
    for module in spawning_modules() {
        let text = std::fs::read_to_string(&module).expect("a spawning module");
        for name in ["RunEvent", "EventKind", "Observer", "Session", "Store"] {
            assert!(
                !text.contains(name),
                "{} names {name}. A module that can see the event stream, the \
                 conversation or the trace is a module the agent can reach — or one \
                 that can write something the agent did not do into a record of what \
                 it did.",
                module.display(),
            );
        }
        assert!(
            text.contains("std::process::Command"),
            "{} is a module permitted to spawn, so it is a module that has to spell \
             the spawn in full — otherwise the gate above is watching a string that \
             is no longer there",
            module.display(),
        );
    }

    let interactive = Path::new(env!("CARGO_MANIFEST_DIR")).join("src/shell.rs");
    let driver = Path::new(env!("CARGO_MANIFEST_DIR")).join("src/main.rs");
    let app = Path::new(env!("CARGO_MANIFEST_DIR")).join("src/app.rs");
    let mut callers = Vec::new();
    let mut builders = Vec::new();
    for (path, text) in sources() {
        if path == interactive {
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

/// A source file with every comment line taken out.
///
/// Line-oriented and deliberately not a parser: a `//` inside a string literal
/// survives, which is the safe direction for a gate — it can only make the check
/// stricter, never blinder.
fn code_of(text: &str) -> String {
    text.lines()
        .map(str::trim_start)
        .filter(|line| !line.starts_with("//"))
        .collect::<Vec<_>>()
        .join("\n")
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
        // **Comments stripped for the sweep too, and 0.19.0 is why.** The rule
        // below already strips them for `src/edit.rs`, for the reason written
        // there — a gate that reads prose forbids a file from explaining itself.
        // The same thing is true of every other file, and `src/skills.rs` found
        // it: it writes a plain-text manifest and says so, and saying so means
        // naming the call it is deliberately not making. The property is about
        // what the code does, so the check reads code.
        let code = code_of(&text);
        let permitted = path.ends_with(&editor);
        if !permitted {
            assert!(
                !code.contains("toml::from_str"),
                "{} parses TOML itself; configuration is io-harness's",
                path.display(),
            );
        }
        assert!(
            !code.contains("toml::de::"),
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

    // Comments stripped first: this module's own documentation explains WHY it
    // may not reach for a configuration type, which means naming several of
    // them. A gate that read prose would forbid the file from explaining itself,
    // and the property is about what the code does.
    let editor_code = code_of(&editor_text);

    for named in [
        "io_harness::Config",
        "io_harness::config",
        "Config::discover",
        "Config::from_toml",
        "CliSettings",
        "ProviderSpec",
    ] {
        assert!(
            !editor_code.contains(named),
            "src/edit.rs names `{named}`. It is permitted to parse TOML only because it \
             works in bytes and never decides what a setting means; the moment it reaches \
             for a configuration type it has become the second reader this gate forbids.",
        );
    }
}

/// The modules permitted to turn somebody else's JSON into a value.
///
/// **Two, and they are files rather than modules.** `src/import.rs` has read the
/// operator's own `~/.claude.json` and Codex settings since 0.21.0;
/// `src/adapt.rs` reads the three foreign plugin manifest formats. Sorted here
/// for the reason `spawning_modules` is sorted: `read_dir` answers in whatever
/// order the filesystem holds, and a test that passes on one machine and fails on
/// another is worse than no test.
fn json_reading_modules() -> Vec<PathBuf> {
    let src = Path::new(env!("CARGO_MANIFEST_DIR")).join("src");
    let mut permitted = vec![src.join("adapt.rs"), src.join("import.rs")];
    permitted.sort();
    permitted
}

/// Is `found` **exactly** the set permitted to deserialize JSON?
///
/// Named for the reason `only_the_permitted_spawn` is named, and it is the same
/// reason: the sabotage arm for this criterion is "widen the set to a substring
/// or stem match", which makes the gate *more* permissive — so nothing fails and
/// it goes vacuous without going red, and the call site cannot observe it because
/// the call site's own list is the thing being widened.
fn only_the_permitted_json_reader(found: &[PathBuf]) -> bool {
    found == json_reading_modules().as_slice()
}

/// **N1 — a second reader of a stranger's file is a second opinion about it, and
/// JSON must not spread before it is confined.**
///
/// This is `f7_the_configuration_is_read_through_the_harness_and_never_parsed_
/// here`'s rule applied to the other format, and it is written in the same
/// release as the first line that reads a stranger's JSON rather than after it.
/// The TOML rule was learned expensively: `src/edit.rs` is the one exemption and
/// it is held to properties, because a module that decides what somebody else's
/// file *means* is a second answer to a question that already has one.
///
/// **Deserialization only, and the asymmetry is the whole argument.** The TOML
/// rule forbids `toml::from_str` — the parse — and says why in its own words at
/// `f7`'s comment: a `from_str` into a shape of our own is the second opinion.
/// Writing is not that. `src/exec.rs` builds `--json` event lines with
/// `serde_json::to_string` and `serde_json::json!`, reading nobody's file and
/// deciding nothing about anybody's format, and a gate that banned it would have
/// to exempt that module for a property which is not the one being protected —
/// an exemption granted for the wrong reason is how a permission list starts
/// widening itself.
#[test]
fn n1_json_is_deserialized_in_the_permitted_modules_and_nowhere_else() {
    let permitted = json_reading_modules();
    let mut readers = Vec::new();

    for (path, text) in sources() {
        // Comments stripped, for the reason the TOML sweep strips them and the
        // reason 0.16.0, 0.19.0, 0.25.0 and 0.26.0 each paid for: a gate that
        // reads prose forbids a file from explaining itself, and this module's
        // own documentation has to name the call it is permitted to make.
        let code = code_of(&text);

        // Spelling the name around is the same act as writing it, exactly as it
        // is for a spawn. `use serde_json as j` puts a parse in a file where the
        // literal below never appears, so the aliased forms are forbidden
        // everywhere — the permitted modules included, which therefore write the
        // call out in full and are found by the sweep like anything else.
        for evasion in [
            "use serde_json as ",
            "use serde_json::{",
            "use serde_json::*",
        ] {
            assert!(
                !code.contains(evasion),
                "{} imports serde_json under another spelling; a JSON parse is \
                 written out in full or it is not written",
                path.display(),
            );
        }

        if ["serde_json::from_str", "serde_json::from_slice", "serde_json::from_reader"]
            .iter()
            .any(|call| code.contains(call))
        {
            readers.push(path);
        }
    }

    readers.sort();
    assert!(
        only_the_permitted_json_reader(&readers),
        "JSON is deserialized somewhere other than the two modules permitted to do \
         it. Found {readers:?}; exactly these are permitted, by exact path: \
         {permitted:?} — one for the operator's own Claude and Codex files, one for \
         the foreign plugin manifests. A third is a second opinion about what \
         somebody else's file means, which is the defect the TOML rule beside this \
         one exists to prevent.",
    );
}

/// **N1's second half — the permitted set is exact paths, and widening it is an
/// edit somebody makes on purpose.**
///
/// The sibling arm — a third file parsing JSON — is covered by the sweep above.
/// This covers the arm that sweep cannot see, because a gate that has been
/// widened refuses nothing and therefore fails nothing. Each near-miss shares a
/// stem, a stem prefix or a module directory with a permitted path and is a
/// different file.
#[test]
fn n1_the_permitted_json_set_is_exact_paths_and_never_a_substring() {
    let src = Path::new(env!("CARGO_MANIFEST_DIR")).join("src");
    let permitted = json_reading_modules();

    // The control. Without it a predicate answering `false` for everything would
    // satisfy every assertion below while refusing the real modules too.
    assert!(
        only_the_permitted_json_reader(&permitted),
        "the permitted set does not admit itself, so nothing below means anything",
    );

    for near in [
        // Its stem opens with a permitted stem, which a `file_stem` prefix match
        // admits.
        src.join("adapter.rs"),
        // A permitted stem, whole, followed by a separator of its own — what an
        // extension-dropped text match admits.
        src.join("adapt_hooks.rs"),
        // A module directory named for a permitted file, which is what
        // `p.starts_with(q.with_extension(""))` admits.
        src.join("adapt").join("mod.rs"),
        // And the same shapes against the other permitted module, so the property
        // is asserted for both rather than for the new one only.
        src.join("importer.rs"),
        src.join("import").join("mod.rs"),
    ] {
        let mut widened = permitted.clone();
        widened.push(near.clone());
        widened.sort();
        assert!(
            !only_the_permitted_json_reader(&widened),
            "{} is admitted beside the permitted modules. The set is compared by \
             exact path for exactly this reason: a substring, stem or prefix match \
             is a permission list that widens itself, and a third module would then \
             read a stranger's JSON while this file went on passing.",
            near.display(),
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

/// **N1 — the three destructive store operations are named in exactly one
/// module each, and no model can reach any of them.**
///
/// The roadmap's words for `/store` are *none of which any model can call*, and
/// the way that stays true is by adding nothing: io-harness's workspace tool set
/// contains nothing that reaches `delete_session`, `sweep_sessions` or
/// `compact`, and this release registers no tool, no MCP server and no skill
/// that could.
///
/// **"Nothing was added" is not assertable, so this asserts the reachable form
/// of it**: each call is named in one file, that file is `src/store.rs`, and
/// `src/store.rs` is reached from command dispatch rather than from anything on
/// a run's path. A second module naming one of these would be the first step
/// towards a caller that is not a keystroke, and it fails here by name.
///
/// The needles are assembled at run time for the reason `tests/store.rs` records
/// at length: a gate that reads source cannot spell the thing it forbids, or its
/// own array is the first match. io-cli has now hit that in 0.16.0, 0.19.0,
/// 0.25.0, 0.26.0 and twice in 0.27.0.
#[test]
fn n1_the_store_operations_live_in_one_module_and_no_model_can_reach_them() {
    let calls = [
        format!("{}_{}", "delete", "session"),
        format!("{}_{}", "sweep", "sessions"),
        format!("{}{}", "compact", "()"),
    ];

    for call in &calls {
        let named: Vec<String> = sources()
            .into_iter()
            .filter(|(_, text)| code_of(text).contains(call.as_str()))
            .map(|(path, _)| {
                path.file_name()
                    .expect("a file name")
                    .to_string_lossy()
                    .into_owned()
            })
            .collect();

        assert_eq!(
            named,
            vec!["store.rs".to_string()],
            "`{call}` must be named in exactly one module, and it must be the one \
             whose whole subject is the store; found it in {named:?}",
        );
    }
}

/// **N1 — this release registers no tool, no MCP server and no skill.**
///
/// The other half of *no model can call it*. A tool is how a model reaches
/// anything at all, so the honest check is that the set of ways this crate hands
/// the model a capability did not grow: `with_tools` is io-harness's own field
/// and this crate never sets it, and the two surfaces that do add capability —
/// `[[mcp]]` servers and skills — are read from the operator's configuration
/// rather than written by this release.
#[test]
fn n1_this_release_hands_the_model_no_new_capability() {
    let forbidden = format!("{}_{}", "with", "tools");
    for (path, text) in sources() {
        assert!(
            !code_of(&text).contains(forbidden.as_str()),
            "{} calls `{forbidden}`: io-cli does not choose the model's tool set, \
             and a release that started to would be handing it reach this crate \
             has never granted",
            path.display(),
        );
    }
}
