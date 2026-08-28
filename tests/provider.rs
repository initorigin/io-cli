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

/// The verification gate's second model, excluded for the handshake's reason.
///
/// 0.24.0 lets an operator have a rubric judged by a model that is **not** the one
/// doing the work, and `TaskContract::with_reviewer` takes an `Arc<dyn Reviewer>`
/// built over a provider of its own. That is a third construction, like the
/// wizard's ping: it never drives a turn, it is deliberately a different model
/// from the session's, and merging it into `build`'s maker would mean the
/// reviewer changing model whenever `/model` did — which is the one thing a second
/// opinion must not do.
///
/// **This exemption was earned twice over.** The construction was written into
/// `src/gates.rs` first and into `src/provider.rs` second, and this gate refused
/// both. Excluded by name like the other two, and held to the distinction by
/// [`f10_the_reviewer_judges_and_never_drives`] below.
const REVIEW: &str = "reviewer.rs";

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
            .filter(|(path, _)| {
                !path.ends_with(HANDSHAKE) && !path.ends_with(PANEL) && !path.ends_with(REVIEW)
            })
            .map(|(path, text)| (path.clone(), text.matches(constructor).count()))
            .filter(|(_, count)| *count > 0)
            .collect();

        let total: usize = sites.iter().map(|(_, count)| count).sum();
        assert_eq!(
            total, 1,
            "`{constructor}` should be written exactly once outside {HANDSHAKE}, \
             {PANEL} and {REVIEW}, so that the interactive and the headless entry \
             points cannot drift apart. Found {sites:?}",
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

/// The reviewer module judges work and never drives a turn.
///
/// Without this, excluding `reviewer.rs` from the construction count would be a
/// hole rather than a boundary: anything at all could be built there and the gate
/// would say nothing. The same shape as
/// [`f10_the_panel_interrogates_presets_and_never_runs_one`], for the same reason.
///
/// Sabotage: give `src/reviewer.rs` a `WithProvider` impl or a `Session` — under
/// which only this fails, and it fails by letting a second driving path grow
/// inside the one exemption that was granted for not being one.
#[test]
fn f10_the_reviewer_judges_and_never_drives() {
    let sources = sources();

    let (_, text) = sources
        .iter()
        .find(|(path, _)| path.ends_with(REVIEW))
        .expect("src/reviewer.rs exists, or the exemption above names nothing");

    // It is what it claims to be: io-harness's own judge, built here and handed
    // over. Without this the file could be exempt and empty of the one thing the
    // exemption was granted for.
    assert!(
        text.contains("ModelReviewer"),
        "src/reviewer.rs is exempted because it builds a reviewer; it names none",
    );

    // And it is nothing else. Each of these would make it a second path a turn
    // could run on, which is exactly what the count exists to prevent.
    for driving in [
        "WithProvider",
        "Watched",
        "Session",
        "Store",
        "TaskContract",
        "turn_bounded",
        ".complete(",
    ] {
        assert!(
            !text.contains(driving),
            "src/reviewer.rs names `{driving}` — it is exempted from the one-site \
             rule precisely because it never drives a turn",
        );
    }

    // Reachable from the contract and from nowhere else, so the exemption cannot
    // become a back door into the turn path.
    let callers: Vec<String> = sources
        .iter()
        .filter(|(_, text)| text.contains("reviewer::build"))
        .map(|(path, _)| {
            path.file_name()
                .expect("a file name")
                .to_string_lossy()
                .into_owned()
        })
        .collect();
    assert_eq!(
        callers,
        vec!["contract.rs".to_string()],
        "the reviewer is built for the contract's criterion and for nothing else",
    );
}

// ---------------------------------------------------------------------------
// F5 and F6 — the chain that runs is the chain the panel draws.

use io_cli::provider::Chain;
use io_harness::{CompletionRequest, CompletionResponse, Provider, ProviderErrorKind};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;

/// A link that answers, or fails in a chosen way, and counts what it was asked.
///
/// The counter lives outside the link because [`Chain::of`] takes ownership of its
/// links. It is what makes F6 assertable at all: "the second provider is never
/// called" is a statement about a call that did not happen, and no message can tell
/// that apart from a call that happened and was discarded.
struct Fake {
    label: String,
    fail: Option<ProviderErrorKind>,
    calls: Arc<AtomicUsize>,
    /// Whether this link would take an image.
    ///
    /// Settable because the trait's default is `false`, and a fixture where every
    /// link answers `false` cannot tell a conjunction from a disjunction or from a
    /// head-only read — the first version of the image test asserted `!all` over
    /// two defaulted links and would have passed against any of the three.
    images: bool,
}

/// A counter to read afterwards, and the link that increments it.
fn link(label: &str, fail: Option<ProviderErrorKind>) -> (Fake, Arc<AtomicUsize>) {
    accepting(label, fail, true)
}

/// A link that says whether it would take an image.
fn accepting(
    label: &str,
    fail: Option<ProviderErrorKind>,
    images: bool,
) -> (Fake, Arc<AtomicUsize>) {
    let calls = Arc::new(AtomicUsize::new(0));
    (
        Fake {
            label: label.into(),
            fail,
            calls: Arc::clone(&calls),
            images,
        },
        calls,
    )
}

fn asked(calls: &Arc<AtomicUsize>) -> usize {
    calls.load(Ordering::Relaxed)
}

impl Provider for Fake {
    async fn complete(
        &self,
        _request: CompletionRequest,
    ) -> io_harness::Result<CompletionResponse> {
        self.calls.fetch_add(1, Ordering::Relaxed);
        match self.fail {
            Some(kind) => Err(io_harness::Error::Provider {
                kind,
                status: None,
                retry_after: None,
                message: format!("{} was asked and refused", self.label),
            }),
            None => Ok(CompletionResponse {
                text: Some(self.label.clone()),
                ..Default::default()
            }),
        }
    }

    fn name(&self) -> &str {
        &self.label
    }

    fn endpoints(&self) -> Vec<&str> {
        vec![&self.label]
    }

    fn accepts_images(&self) -> bool {
        self.images
    }
}

fn asking() -> CompletionRequest {
    CompletionRequest {
        system: "s".into(),
        user: "u".into(),
        ..Default::default()
    }
}

#[tokio::test]
async fn f5_a_retryable_failure_falls_through_to_the_next_link() {
    let (first, first_calls) = link("first", Some(ProviderErrorKind::Server));
    let (second, second_calls) = link("second", None);
    let chain = Chain::of(vec![first, second]).expect("two links are a chain");

    let answer = chain.complete(asking()).await.expect("the second answers");

    assert_eq!(
        answer.text.as_deref(),
        Some("second"),
        "a head that failed in a way another vendor might survive must not end the turn",
    );
    assert_eq!(
        chain.last_served().as_deref(),
        Some("second"),
        "the link that answered is what io-harness records and what the status line moves to",
    );
    assert_eq!((asked(&first_calls), asked(&second_calls)), (1, 1));
}

#[tokio::test]
async fn f5_the_head_is_asked_first_and_a_link_below_it_is_not_asked_at_all() {
    let (first, first_calls) = link("first", None);
    let (second, second_calls) = link("second", None);
    let chain = Chain::of(vec![first, second]).expect("a chain");

    let answer = chain.complete(asking()).await.expect("the head answers");

    assert_eq!(
        answer.text.as_deref(),
        Some("first"),
        "the operator's first choice is the provider a question is put to",
    );
    assert_eq!(
        (asked(&first_calls), asked(&second_calls)),
        (1, 0),
        "a link below a head that answered is not asked, so it is not billed",
    );
}

#[tokio::test]
async fn f5_a_head_that_answered_reports_no_fallover() {
    // **The whole reason this crate does not build on `io_harness::Fallback`.**
    // io-harness emits `EventKind::FellBackTo` for any `Some` from `last_served`
    // (`run/step.rs:503`), and `Fallback::last_served` answers `Some` for its own
    // primary — so a chain built from that type would tell the operator "the
    // provider fell over" on every step of every run.
    let (first, _) = link("first", None);
    let (second, _) = link("second", None);
    let chain = Chain::of(vec![first, second]).expect("a chain");
    chain.complete(asking()).await.expect("the head answers");

    assert_eq!(
        chain.last_served(),
        None,
        "nothing fell over, so nothing may be reported as having fallen over",
    );
}

#[tokio::test]
async fn f5_one_provider_is_a_chain_that_behaves_exactly_as_no_chain_did() {
    let (only, _) = link("only", None);
    let chain = Chain::of(vec![only]).expect("one link is a chain");

    let answer = chain
        .complete(asking())
        .await
        .expect("the only link answers");

    assert_eq!(answer.text.as_deref(), Some("only"));
    assert_eq!(
        chain.last_served(),
        None,
        "an operator who configured one provider must stay on the path they were on",
    );
}

#[tokio::test]
async fn f6_a_bad_credential_on_the_head_does_not_spend_the_link_below_it() {
    let (first, first_calls) = link("first", Some(ProviderErrorKind::Auth));
    let (second, second_calls) = link("second", None);
    let chain = Chain::of(vec![first, second]).expect("a chain");

    let refused = chain.complete(asking()).await;

    assert!(
        refused.is_err(),
        "a wrong key is not a failure another vendor can survive",
    );
    assert_eq!(
        (asked(&first_calls), asked(&second_calls)),
        (1, 0),
        "the second link is never called, so a typo in a key cannot start spending elsewhere",
    );
    assert_eq!(
        chain.last_served(),
        None,
        "nobody answered, so no provider may be named as having done so",
    );
}

/// What a caller learns about the chain `provider::build` handed it.
///
/// The only way to assert `build` itself. Everything above tests [`Chain`]
/// directly, which cannot catch a `build` that assembles the wrong links — and
/// "assembles the wrong links" is precisely F5's named sabotage, since dropping
/// the head still produces a perfectly working chain of the operator's second
/// choice.
struct Probe;

impl io_cli::provider::WithProvider for Probe {
    /// The model the chain will ask first, and every host it may reach.
    type Out = (String, usize);

    async fn call<P: Provider>(
        self,
        make: impl Fn(&str) -> Result<P, String>,
        model: String,
    ) -> Self::Out {
        let provider = make(&model).expect("the chain builds from two valid specs");
        (
            provider.model_hint().unwrap_or_default().to_string(),
            provider.endpoints().len(),
        )
    }
}

#[tokio::test]
async fn f5_build_asks_the_operators_first_choice_first() {
    // Two vendors rather than two of one, so the links are distinguishable by
    // something other than the model name — and the credential is in the spec so
    // no test has to touch the process environment to run.
    let specs = vec![
        io_harness::ProviderSpec::OpenRouter {
            model: "head-model".into(),
            api_key: Some("k".into()),
        },
        io_harness::ProviderSpec::Anthropic {
            model: "tail-model".into(),
            api_key: Some("k".into()),
        },
    ];

    let (asked_first, hosts) = io_cli::provider::build(specs, None, Probe)
        .await
        .expect("a chain of two");

    assert_eq!(
        asked_first, "head-model",
        "the head of the chain is the provider the operator wrote first; a chain \
         folded from the tail alone answers every request from their second choice",
    );
    assert_eq!(
        hosts, 2,
        "every link's host reaches the egress policy, or a fall-through is a way \
         to reach a host the policy never saw",
    );
}

#[tokio::test]
async fn f5_a_model_override_replaces_the_heads_model_and_not_the_tails() {
    // `-m/--model` names one model, and a chain has several. Applying it to every
    // link would ask a second vendor for a model id only the first one serves.
    let specs = vec![
        io_harness::ProviderSpec::OpenRouter {
            model: "head-model".into(),
            api_key: Some("k".into()),
        },
        io_harness::ProviderSpec::Anthropic {
            model: "tail-model".into(),
            api_key: Some("k".into()),
        },
    ];

    let (asked_first, _) = io_cli::provider::build(specs, Some("chosen".into()), Probe)
        .await
        .expect("a chain of two");

    assert_eq!(asked_first, "chosen");
}

#[test]
fn f5_every_link_s_host_is_authorized_and_not_only_the_head_s() {
    // io-harness's egress policy is deny-by-default and authorizes the provider's
    // hosts before the first step. A chain reporting only its head's host would
    // make a fall-through a way to reach a host the policy never saw.
    let (first, _) = link("first", None);
    let (second, _) = link("second", None);
    let chain = Chain::of(vec![first, second]).expect("a chain");

    assert_eq!(chain.endpoints(), vec!["first", "second"]);
}

/// **The conjunction, asserted as one rather than read off a default.**
///
/// io-harness's own rule for the same question, and the reason is that the
/// fall-through is the one call where it matters: reporting the head's answer
/// would let an image reach a link that cannot read it on exactly the call that
/// went wrong.
///
/// The first version of this test built two links that both took the trait's
/// default of `false` and asserted `!accepts_images()`. Every possible
/// implementation passes that — `all`, `any`, and a head-only read alike — so it
/// was a gate over nothing, which the adversarial review caught. The pair below is
/// what makes it a gate: one mixed chain and one where every link agrees.
#[test]
fn f5_an_image_is_refused_unless_every_link_accepts_one() {
    let (yes, _) = accepting("first", None, true);
    let (no, _) = accepting("second", None, false);
    let mixed = Chain::of(vec![yes, no]).expect("a chain");
    assert!(
        !mixed.accepts_images(),
        "one link that cannot read an image is enough to refuse it before the run \
         rather than midway through a fall-through",
    );

    let (one, _) = accepting("first", None, true);
    let (two, _) = accepting("second", None, true);
    let agreed = Chain::of(vec![one, two]).expect("a chain");
    assert!(
        agreed.accepts_images(),
        "a chain whose links all accept images accepts them — without this arm the \
         assertion above is satisfied by a function that always answers false",
    );
}
