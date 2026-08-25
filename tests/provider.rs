//! F10 — one provider construction site.
//!
//! `Provider` is not dyn-compatible, so a provider cannot be built behind a trait
//! object and every caller has to be reached from inside a match on
//! `ProviderSpec`. That match is worth exactly one copy. A second one is not a
//! duplicate to tidy up later: it is how the next provider the harness gains gets
//! added to the interactive path and not the headless one, and the failure is
//! silent on whichever path nobody ran.
//!
//! The count deliberately excludes `src/verify.rs`. The wizard's credential
//! handshake builds a provider too — pinged once and dropped before any session
//! or store exists — and that is a different operation from the session's, which
//! returns a *maker* the model switch calls again on every switch. Merging them to
//! satisfy this test would be the test driving the architecture. See
//! `.ultraship/iterations/US-IO-CLI-0.5.0-I01.yaml`.

use std::path::{Path, PathBuf};

/// The four constructors that turn a credential into a provider.
const CONSTRUCTORS: &[&str] = &[
    "OpenRouter::new",
    "Anthropic::new",
    "OpenAi::new",
    "Compatible::preset",
    "Compatible::new",
];

/// The wizard's live checks, excluded by name rather than by pattern, so that
/// adding a file cannot quietly widen the exemption.
const HANDSHAKE: &str = "verify.rs";

/// The `/provider` panel, excluded the same way and for a different reason.
///
/// 0.16.0's panel names `Compatible::preset` to **interrogate** a preset — what
/// base URL io-harness resolves it to, whether that URL is local, and what the
/// refusal lists when the name is not one — and never to build a provider a turn
/// will use. That is the opposite of what this gate protects: the rule is that
/// one site constructs the provider both entry points run on, and a module that
/// asks a question and throws the answer away is not a second site.
///
/// Excluded by name rather than by pattern, like `verify.rs`, and held to the
/// distinction by [`f10_the_panel_interrogates_presets_and_never_runs_one`]
/// below — without which this exemption would be a hole rather than a boundary.
const PANEL: &str = "providers.rs";

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
    assert!(out.len() >= 10, "there should be source to check");
    out
}

#[test]
fn f10_each_provider_is_constructed_in_exactly_one_place() {
    let sources = sources();

    for constructor in CONSTRUCTORS {
        let sites: Vec<(PathBuf, usize)> = sources
            .iter()
            .filter(|(path, _)| !path.ends_with(HANDSHAKE) && !path.ends_with(PANEL))
            .map(|(path, text)| (path.clone(), text.matches(constructor).count()))
            .filter(|(_, count)| *count > 0)
            .collect();

        let total: usize = sites.iter().map(|(_, count)| count).sum();
        assert_eq!(
            total, 1,
            "`{constructor}` should be written exactly once outside {HANDSHAKE} \
             and {PANEL}, so that the interactive and the headless entry points \
             cannot drift apart. Found {sites:?}",
        );

        // Naming the file as well as the count is what makes this fail while the
        // construction still sits inside the interactive driver: one site in
        // `main.rs` satisfies the count and still cannot be reached from `exec`.
        let (path, _) = &sites[0];
        assert!(
            path.ends_with("provider.rs"),
            "`{constructor}` should live in src/provider.rs, the one site both \
             entry points reach. Found it in {}",
            path.display(),
        );
    }
}

/// The exemption above is a boundary rather than a hole.
///
/// `src/providers.rs` may name a constructor because it asks presets questions.
/// What it may never do is keep one: a `Compatible` that reached a turn from
/// there would be the second construction site this file exists to prevent, and
/// it would be reached only on the interactive arm.
#[test]
fn f10_the_panel_interrogates_presets_and_never_runs_one() {
    let panel = sources()
        .into_iter()
        .find(|(path, _)| path.ends_with(PANEL))
        .map(|(_, text)| text)
        .expect("src/providers.rs exists; the exemption is written for it");

    // Nothing that runs a turn, and nothing that carries a provider outward.
    for forbidden in [
        "CompletionRequest",
        ".complete(",
        ".complete_streaming(",
        "WithProvider",
        "-> Compatible",
        "Provider>",
    ] {
        assert!(
            !panel.contains(forbidden),
            "src/providers.rs names `{forbidden}`. It is exempt from the \
             one-construction-site rule only because it interrogates presets and \
             keeps nothing; the moment it holds or hands out a provider it has \
             become the second site.",
        );
    }

    // And every constructor it does name is immediately asked something and
    // dropped — the calls are inside expressions that end in a question.
    for line in panel.lines().filter(|l| l.contains("Compatible::preset")) {
        let trimmed = line.trim();
        assert!(
            !trimmed.starts_with("let provider") && !trimmed.contains("self."),
            "a `Compatible` is being kept in src/providers.rs: {trimmed}",
        );
    }
}

#[test]
fn f10_both_entry_points_reach_a_provider_through_that_site() {
    let sources = sources();
    let find = |name: &str| {
        sources
            .iter()
            .find(|(path, _)| path.ends_with(name))
            .map(|(_, text)| text.clone())
            .unwrap_or_else(|| panic!("src/{name} should exist"))
    };

    // Neither entry point may name a constructor itself, and both must go through
    // the shared builder. Asserted on the callers rather than only on the callee,
    // because a second site that nothing calls is not the failure — a second site
    // that one caller uses is.
    for entry in ["main.rs", "exec.rs"] {
        let text = find(entry);
        for constructor in CONSTRUCTORS {
            assert!(
                !text.contains(constructor),
                "src/{entry} constructs a provider itself (`{constructor}`) \
                 instead of going through src/provider.rs",
            );
        }
        assert!(
            text.contains("provider::build"),
            "src/{entry} should reach a provider through `provider::build`",
        );
    }
}
