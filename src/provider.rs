//! The one place a session's provider is built.
//!
//! `io_harness::Provider` returns `impl Future` from its methods, so it is not
//! dyn-compatible: there is no `Box<dyn Provider>` to hand around, and a function
//! cannot return one provider from four match arms. The shape that works is to
//! invert it — the caller is handed *into* the arm rather than receiving a value
//! out of it — and [`WithProvider`] is that inversion.
//!
//! What each arm hands over is a **maker** rather than a provider. Every turn
//! entry point in io-harness takes `provider: &P` as an argument while the
//! conversation lives in the `Session`, so changing the model mid-session is a new
//! provider handed to the next turn and nothing else. Building one needs the
//! credential, which only this module has seen, so a closure that captures it is
//! the whole mechanism.
//!
//! The maker is fallible for one arm's sake: a `Compatible` endpoint resolves from
//! a preset that can fail. Making every arm fallible keeps one signature and lets
//! a failed model switch report itself instead of ending the session.

use io_harness::{
    Anthropic, Auth, Compatible, CompletionRequest, CompletionResponse, ModelInfo, OpenAi,
    OpenRouter, PromptFamily, Provider, ProviderSpec, Reference, ToolCall,
};

use crate::cli::FromEnv;
use crate::context::Seen;

/// The host this process reads a model catalogue from, once one is built.
///
/// See [`build`], which is the only writer, and [`catalogue_host`], which is the
/// only reader.
static CATALOGUE_HOST: std::sync::OnceLock<String> = std::sync::OnceLock::new();

/// The catalogue host in force, or `None` when the operator turned it off or no
/// provider has been built yet.
///
/// Read by [`crate::failure::advice`] so that a run refused over this host says
/// which key turns it off. It is deliberately not read anywhere that decides
/// behaviour — a `OnceLock` is fine for a sentence and wrong for a rule, because
/// nothing clears it.
pub fn catalogue_host() -> Option<&'static str> {
    CATALOGUE_HOST.get().map(String::as_str)
}

/// What a caller does once there is a provider to build.
///
/// Implemented once per entry point — the interactive session and `io exec` — so
/// that both reach a provider through [`build`] and neither writes a match of its
/// own. `tests/provider.rs` asserts that, because a second match is how the next
/// provider the harness gains reaches one path and not the other.
#[allow(async_fn_in_trait)]
pub trait WithProvider {
    /// Whatever the entry point returns once it has run.
    type Out;

    /// Run, given a way to build a provider for a named model and the model to
    /// start with.
    async fn call<P: Provider>(
        self,
        make: impl Fn(&str) -> Result<P, String>,
        model: String,
    ) -> Self::Out;
}

/// Build the provider a spec names and hand it to `with`.
///
/// `model_override` is `-m/--model`: it replaces the model the configuration
/// names without touching anything else about the endpoint, which is why it is
/// applied to the extracted model rather than by rewriting the spec.
/// The model a spec names, whichever provider it names it through.
///
/// Read rather than reconstructed: `ProviderSpec` is io-harness's own type and
/// every arm of it carries a `model`, so a caller that wants the name should ask
/// for it in one place instead of matching four arms at each site.
pub fn model_of(spec: &ProviderSpec) -> &str {
    match spec {
        ProviderSpec::OpenRouter { model, .. }
        | ProviderSpec::Anthropic { model, .. }
        | ProviderSpec::OpenAi { model, .. }
        | ProviderSpec::Compatible { model, .. } => model,
        // `ProviderSpec` is `#[non_exhaustive]`, so an arm this crate has never
        // seen is a real possibility and an empty name is the honest answer:
        // the splash simply leaves the row out.
        _ => "",
    }
}

/// The whole chain a configuration names, head first.
///
/// `provider_spec()` is the head and `fallback_specs()` is the tail — io-harness's
/// own split, and the same one [`crate::providers::chain`] draws in the panel. The
/// panel has shown this list since 0.21.0 while [`build`] ran only its head; asking
/// for it in one place is what keeps the chain that runs and the chain that is drawn
/// from being two different answers.
pub fn chain_of(config: &io_harness::Config) -> Vec<ProviderSpec> {
    config
        .provider_spec()
        .into_iter()
        .chain(config.fallback_specs())
        .cloned()
        .collect()
}

/// One provider, whichever vendor built it.
///
/// **A chain of arbitrary length needs one concrete type, and this is it.**
/// `Provider::complete` returns `impl Future` (RPITIT), so the trait is not
/// dyn-compatible and there is no `Box<dyn Provider>` to put in a `Vec` —
/// `io_harness::provider::Fallback`'s own module says as much. An enum is the
/// remaining shape: four arms, one type, and `Fallback` can nest over it.
///
/// **It wraps and never constructs.** Every value inside one of these arms was
/// built in [`build`], which is still the only site in this crate that names a
/// vendor constructor — the rule `tests/provider.rs` enforces and which this
/// release does not widen.
///
/// Every method delegates across all four arms, for the reason [`Watched`] gives
/// at length: the trait has eight defaulted methods, and a default that fired here
/// would change behaviour silently rather than fail. The async ones are written as
/// `async fn` rather than by returning the inner future, because four arms return
/// four distinct future types and only an `async` body can unify them.
pub enum Vendor {
    OpenRouter(OpenRouter),
    Anthropic(Anthropic),
    OpenAi(OpenAi),
    Compatible(Compatible),
}

impl Provider for Vendor {
    async fn complete(&self, request: CompletionRequest) -> io_harness::Result<CompletionResponse> {
        match self {
            Self::OpenRouter(p) => p.complete(request).await,
            Self::Anthropic(p) => p.complete(request).await,
            Self::OpenAi(p) => p.complete(request).await,
            Self::Compatible(p) => p.complete(request).await,
        }
    }

    async fn complete_streaming(
        &self,
        request: CompletionRequest,
        on_token: &(dyn Fn(&str) + Send + Sync),
    ) -> io_harness::Result<CompletionResponse> {
        match self {
            Self::OpenRouter(p) => p.complete_streaming(request, on_token).await,
            Self::Anthropic(p) => p.complete_streaming(request, on_token).await,
            Self::OpenAi(p) => p.complete_streaming(request, on_token).await,
            Self::Compatible(p) => p.complete_streaming(request, on_token).await,
        }
    }

    async fn complete_streaming_calls(
        &self,
        request: CompletionRequest,
        on_token: &(dyn Fn(&str) + Send + Sync),
        on_call: &(dyn Fn(usize, &ToolCall) + Send + Sync),
    ) -> io_harness::Result<CompletionResponse> {
        match self {
            Self::OpenRouter(p) => p.complete_streaming_calls(request, on_token, on_call).await,
            Self::Anthropic(p) => p.complete_streaming_calls(request, on_token, on_call).await,
            Self::OpenAi(p) => p.complete_streaming_calls(request, on_token, on_call).await,
            Self::Compatible(p) => p.complete_streaming_calls(request, on_token, on_call).await,
        }
    }

    async fn models(&self) -> io_harness::Result<Vec<ModelInfo>> {
        match self {
            Self::OpenRouter(p) => p.models().await,
            Self::Anthropic(p) => p.models().await,
            Self::OpenAi(p) => p.models().await,
            Self::Compatible(p) => p.models().await,
        }
    }

    async fn reachable(&self) -> io_harness::Result<bool> {
        match self {
            Self::OpenRouter(p) => p.reachable().await,
            Self::Anthropic(p) => p.reachable().await,
            Self::OpenAi(p) => p.reachable().await,
            Self::Compatible(p) => p.reachable().await,
        }
    }

    fn model_hint(&self) -> Option<&str> {
        match self {
            Self::OpenRouter(p) => p.model_hint(),
            Self::Anthropic(p) => p.model_hint(),
            Self::OpenAi(p) => p.model_hint(),
            Self::Compatible(p) => p.model_hint(),
        }
    }

    // **The two window answers, and the reason they are written out rather than
    // left to the trait.** io-harness 0.81.0 added `context_window` and
    // `max_output_tokens` with `None` defaults, so every wrapper in this file
    // compiles without them and answers "this provider is not saying" — which
    // sends the run to the fallback rung.
    //
    // **That rung is `context::FALLBACK_WINDOW` since 0.82.0, not
    // `FALLBACK_MAX_TOKENS`, and the distinction is the release.**
    // `FALLBACK_MAX_TOKENS` still exists and is still 24,000, but it is now only
    // `ContextBudget::default`'s ceiling; what an unanswered provider gets is
    // `assumed_window` — 128,000 for a remote endpoint and 24,000 for a loopback
    // one, which sizes down to far less. A sentence here naming 24,000 as *the*
    // fallback would be describing the harness this crate stopped pinning.
    //
    // **It answers for real as of 0.39.0, and the paragraph this replaces said it
    // did not.** Through io-harness 0.81.0 exactly one shipped provider
    // implemented either method — `Compatible` — and only from a catalogue
    // `models()` had already fetched on that instance, so every configuration this
    // crate could build answered `None`, the harness emitted `source: "fallback"`,
    // and the ceiling was 24,000 whatever model was configured. That was
    // initorigin/io-cli#105 and io-harness#266, and `US-IO-CLI-0.38.2-I01` records
    // it. io-harness 0.82.0 closes it: `warm_sizing` is a real call before step 0,
    // `OpenRouter` reads its own `/models`, and `Anthropic` and `OpenAi` read a
    // reference catalogue this crate now turns on by default — see
    // [`crate::settings::reference_catalogue`].
    //
    // **`warm_sizing` and `assumed_window` are 0.82.0's, and they are delegated
    // here for the reason every other method is**: both are defaulted, so a
    // wrapper that forgets one substitutes the default silently. The cost of
    // forgetting is not academic — io-harness's own 0.82.0 review found `Record`,
    // `Fallback` and `eval::Capture` doing exactly that, flipping a wrapped local
    // provider's ceiling from 7,616 to 111,616 with no compiler error, no warning
    // and no failing test.
    //
    // There is no compiler error and no test failure for a defaulted method: the
    // only gate is `f2_every_provider_method_is_delegated_by_every_wrapper`, which
    // reads the method list out of the locked harness rather than a literal — and
    // it is a source-text gate, so it proves the method is written and never that
    // anything answers.
    fn context_window(&self) -> Option<u64> {
        match self {
            Self::OpenRouter(p) => p.context_window(),
            Self::Anthropic(p) => p.context_window(),
            Self::OpenAi(p) => p.context_window(),
            Self::Compatible(p) => p.context_window(),
        }
    }

    fn max_output_tokens(&self) -> Option<u64> {
        match self {
            Self::OpenRouter(p) => p.max_output_tokens(),
            Self::Anthropic(p) => p.max_output_tokens(),
            Self::OpenAi(p) => p.max_output_tokens(),
            Self::Compatible(p) => p.max_output_tokens(),
        }
    }

    /// An `async` block rather than a returned future, because the four arms have
    /// four future types and a `match` cannot be the body of an `impl Future`
    /// return. It is the same shape every other async method on this enum takes.
    async fn warm_sizing(&self) -> io_harness::Result<()> {
        match self {
            Self::OpenRouter(p) => p.warm_sizing().await,
            Self::Anthropic(p) => p.warm_sizing().await,
            Self::OpenAi(p) => p.warm_sizing().await,
            Self::Compatible(p) => p.warm_sizing().await,
        }
    }

    fn assumed_window(&self) -> u64 {
        match self {
            Self::OpenRouter(p) => p.assumed_window(),
            Self::Anthropic(p) => p.assumed_window(),
            Self::OpenAi(p) => p.assumed_window(),
            Self::Compatible(p) => p.assumed_window(),
        }
    }

    fn name(&self) -> &str {
        match self {
            Self::OpenRouter(p) => p.name(),
            Self::Anthropic(p) => p.name(),
            Self::OpenAi(p) => p.name(),
            Self::Compatible(p) => p.name(),
        }
    }

    fn prompt_family(&self) -> PromptFamily {
        match self {
            Self::OpenRouter(p) => p.prompt_family(),
            Self::Anthropic(p) => p.prompt_family(),
            Self::OpenAi(p) => p.prompt_family(),
            Self::Compatible(p) => p.prompt_family(),
        }
    }

    fn accepts_images(&self) -> bool {
        match self {
            Self::OpenRouter(p) => p.accepts_images(),
            Self::Anthropic(p) => p.accepts_images(),
            Self::OpenAi(p) => p.accepts_images(),
            Self::Compatible(p) => p.accepts_images(),
        }
    }

    fn endpoint(&self) -> Option<&str> {
        match self {
            Self::OpenRouter(p) => p.endpoint(),
            Self::Anthropic(p) => p.endpoint(),
            Self::OpenAi(p) => p.endpoint(),
            Self::Compatible(p) => p.endpoint(),
        }
    }

    fn endpoints(&self) -> Vec<&str> {
        match self {
            Self::OpenRouter(p) => p.endpoints(),
            Self::Anthropic(p) => p.endpoints(),
            Self::OpenAi(p) => p.endpoints(),
            Self::Compatible(p) => p.endpoints(),
        }
    }

    fn last_served(&self) -> Option<String> {
        match self {
            Self::OpenRouter(p) => p.last_served(),
            Self::Anthropic(p) => p.last_served(),
            Self::OpenAi(p) => p.last_served(),
            Self::Compatible(p) => p.last_served(),
        }
    }
}

/// A chain of providers, each answering when the one before it fails.
///
/// **The decision to fall over is io-harness's**, not this crate's:
/// [`io_harness::ProviderErrorKind::is_retryable`] is the same predicate its own
/// `Fallback` and its own in-run retry both ask, and the reasoning is its own — a
/// failure about the request or about the caller's configuration will happen
/// identically at the next vendor, so falling over on one fails twice and spends
/// twice. io-cli holds no opinion here and must not grow one.
///
/// **Flat rather than nested, and that is not a preference.** The obvious shape is
/// `Fallback<Vendor, Fallback<Vendor, Vendor>>`, folded recursively so the tail is
/// itself one type. It does not compile: `Provider::complete` returns `impl Future`
/// (RPITIT), so a `Chain` whose secondary is a `Chain` makes the future's auto-trait
/// inference depend on itself, and rustc reports a type cycle that `Box::pin` on the
/// recursive arm does not break. A list and a loop have neither problem.
///
/// **`last_served` answers `None` when the head served, and that is a deliberate
/// difference from `Fallback`.** io-harness emits `EventKind::FellBackTo` for any
/// `Some` (`run/step.rs:577`), while `Fallback::last_served` answers `Some` for its
/// *primary* too — so a chain built from that type would report "the provider fell
/// over" on every step of every run, including the ones where nothing did. Reporting
/// only a link that is not the head makes the event mean what its name says, and
/// makes a chain whose head is answering indistinguishable from no chain at all,
/// which is what an operator would expect.
///
/// Generic over the link so the fall-through is assertable. The chain a
/// configuration builds is `Chain<Vendor>`; a test builds `Chain<Fake>` over links
/// that fail on demand with a chosen `ProviderErrorKind`, which is the only way to
/// prove that a retryable failure falls through and an `Auth` failure does not
/// without a network and without spending anything.
pub struct Chain<P = Vendor> {
    links: Vec<P>,
    /// Which link answered last, as an index, or `usize::MAX` for nobody yet.
    ///
    /// An atomic rather than a lock for the reason io-harness gives for the same
    /// field: a `MutexGuard` is not `Send` and `complete`'s future has to be.
    served: std::sync::atomic::AtomicUsize,
}

/// Nobody has answered yet.
const UNSERVED: usize = usize::MAX;

impl<P: Provider> Chain<P> {
    /// A chain over `links`, head first. `None` when there are no links at all.
    pub fn of(links: Vec<P>) -> Option<Self> {
        if links.is_empty() {
            return None;
        }
        Some(Self {
            links,
            served: std::sync::atomic::AtomicUsize::new(UNSERVED),
        })
    }

    /// The head, which is the provider every question is asked of first.
    fn head(&self) -> &P {
        &self.links[0]
    }

    fn note(&self, who: usize) {
        self.served.store(who, std::sync::atomic::Ordering::Relaxed);
    }

    /// Whether a different vendor is worth trying, asked of io-harness.
    fn worth_another(error: &io_harness::Error) -> bool {
        matches!(error, io_harness::Error::Provider { kind, .. } if kind.is_retryable())
    }
}

impl<P: Provider + Sync> Provider for Chain<P> {
    async fn complete(&self, request: CompletionRequest) -> io_harness::Result<CompletionResponse> {
        let mut last = None;
        for (index, link) in self.links.iter().enumerate() {
            match link.complete(request.clone()).await {
                Ok(response) => {
                    self.note(index);
                    return Ok(response);
                }
                Err(error) if Self::worth_another(&error) => last = Some(error),
                // Not worth another vendor: a bad credential or a malformed
                // request fails the same way everywhere, and trying the next link
                // would spend somebody else's budget to be told so again.
                Err(error) => return Err(error),
            }
        }
        Err(last.expect("a chain has at least one link, so the loop ran at least once"))
    }

    async fn complete_streaming(
        &self,
        request: CompletionRequest,
        on_token: &(dyn Fn(&str) + Send + Sync),
    ) -> io_harness::Result<CompletionResponse> {
        let mut last = None;
        for (index, link) in self.links.iter().enumerate() {
            match link.complete_streaming(request.clone(), on_token).await {
                Ok(response) => {
                    self.note(index);
                    return Ok(response);
                }
                Err(error) if Self::worth_another(&error) => last = Some(error),
                Err(error) => return Err(error),
            }
        }
        Err(last.expect("a chain has at least one link, so the loop ran at least once"))
    }

    async fn complete_streaming_calls(
        &self,
        request: CompletionRequest,
        on_token: &(dyn Fn(&str) + Send + Sync),
        on_call: &(dyn Fn(usize, &ToolCall) + Send + Sync),
    ) -> io_harness::Result<CompletionResponse> {
        let mut last = None;
        for (index, link) in self.links.iter().enumerate() {
            match link
                .complete_streaming_calls(request.clone(), on_token, on_call)
                .await
            {
                Ok(response) => {
                    self.note(index);
                    return Ok(response);
                }
                Err(error) if Self::worth_another(&error) => last = Some(error),
                Err(error) => return Err(error),
            }
        }
        Err(last.expect("a chain has at least one link, so the loop ran at least once"))
    }

    async fn models(&self) -> io_harness::Result<Vec<ModelInfo>> {
        self.head().models().await
    }

    async fn reachable(&self) -> io_harness::Result<bool> {
        self.head().reachable().await
    }

    fn model_hint(&self) -> Option<&str> {
        self.head().model_hint()
    }

    /// **The smallest window any link declares, for `accepts_images`'s reason.**
    ///
    /// The head's window is the wrong answer for the same reason the head's image
    /// support is: a run assembles its context once, and the call that matters is
    /// the fall-through. Sizing to a 200,000-token head and falling through to a
    /// 32,000-token link overflows the request precisely when the first link has
    /// already failed, which is the worst moment to discover it.
    ///
    /// A link that says nothing is not a link that says zero — it is skipped, and
    /// `None` here means no link in the chain declared a window at all, which
    /// sends the run to io-harness's own fallback exactly as before.
    fn context_window(&self) -> Option<u64> {
        self.links.iter().filter_map(Provider::context_window).min()
    }

    /// **The largest reserved answer any link declares — the opposite of the
    /// window above, because this number is subtracted rather than divided by.**
    ///
    /// The instinct is that the two are the same question and take the same
    /// conservative direction. They are not. `ContextBudget::for_window` computes
    /// `max_tokens = window - (max_output + floor)`, so a *smaller* reserve leaves
    /// a *larger* assembled section: taking the minimum reserves room for the link
    /// that answers least and then overflows on the link that answers most.
    ///
    /// Concretely, two links on 128,000-token windows, the head allowing 4,096
    /// output tokens and the fall-through allowing 32,768. The minimum reserves
    /// 4,096, leaving 115,712 for the prompt; the fall-through may then answer
    /// 32,768 into a 128,000 window that already holds 115,712. That is the same
    /// overflow the window's minimum exists to prevent, arriving at the same
    /// moment — after the head has failed — through the arithmetic running the
    /// other way.
    fn max_output_tokens(&self) -> Option<u64> {
        self.links
            .iter()
            .filter_map(Provider::max_output_tokens)
            .max()
    }

    /// **Every link, not the head, and for the same reason the window is the
    /// minimum.** The number the run sizes with is the minimum over the links, so
    /// a link that never warmed contributes nothing to it and the chain sizes as
    /// though it did not exist — which is precisely the fall-through case
    /// `context_window` above exists to be conservative about.
    ///
    /// **It costs one fetch, not one per link**, because `provider::build` clones
    /// a single [`Reference`] into every vendor that takes one and that type's
    /// cache is an `Arc<OnceLock>`. Warming each link in turn is therefore the
    /// cheap option as well as the correct one.
    ///
    /// A failure is swallowed by io-harness at the call site and swallowed here
    /// too: a chain whose second link could not be sized is a chain that sizes
    /// from its first, not a run that refuses to start. The one failure that is
    /// **not** swallowed is an egress policy denying the reference host, which
    /// io-harness raises before this is ever reached — see
    /// [`crate::failure::advice_with`], which is what turns that refusal into a
    /// sentence naming the key that turns the catalogue off.
    async fn warm_sizing(&self) -> io_harness::Result<()> {
        // The one loop this module is permitted, over `self.links` and nothing
        // else — the shape `tests/dependencies.rs` holds this file to by exact
        // path. Spelled `self.links.iter()` rather than `&self.links` because
        // that gate matches the spelling, not the meaning: it is a text sweep
        // over a file it cannot execute, and a needle loose enough to accept both
        // spellings would accept a loop over something else whose name merely
        // began the same way.
        for link in self.links.iter() {
            let _ = link.warm_sizing().await;
        }
        Ok(())
    }

    /// The smallest assumption any link makes, for `context_window`'s reason
    /// exactly: this is the number that is divided into, and a chain that assumed
    /// its largest link's window would overflow on its smallest.
    ///
    /// It cannot be empty — `Chain::of` refuses an empty list — but `min` returns
    /// an `Option` regardless, and the trait's own default is the honest answer if
    /// one ever did.
    fn assumed_window(&self) -> u64 {
        self.links
            .iter()
            .map(Provider::assumed_window)
            .min()
            .unwrap_or(io_harness::context::FALLBACK_WINDOW)
    }

    fn name(&self) -> &str {
        self.head().name()
    }

    fn prompt_family(&self) -> PromptFamily {
        self.head().prompt_family()
    }

    /// Every link, or none of them — io-harness's own rule for the same question.
    ///
    /// Reporting the head's answer would let an image reach a link that cannot read
    /// it on the one call that matters, the fall-through. The link would refuse it
    /// anyway, so the conjunction only changes *when* the operator finds out: before
    /// the run rather than midway through a failure.
    fn accepts_images(&self) -> bool {
        self.links.iter().all(Provider::accepts_images)
    }

    fn endpoint(&self) -> Option<&str> {
        self.head().endpoint()
    }

    /// **Every link's hosts, and this is the reason `endpoints` exists at all.**
    /// io-harness's egress policy is deny-by-default and a run authorizes its
    /// provider's hosts before its first step, so a chain that reported only the
    /// head's host would make a fall-through a way to reach a host the policy never
    /// saw.
    fn endpoints(&self) -> Vec<&str> {
        self.links.iter().flat_map(Provider::endpoints).collect()
    }

    /// The link that answered, unless it was the head.
    ///
    /// See the type's own documentation: io-harness emits `EventKind::FellBackTo`
    /// for any `Some`, so answering `Some` for the head would report a fall-through
    /// on every step of every run that configured a chain.
    fn last_served(&self) -> Option<String> {
        match self.served.load(std::sync::atomic::Ordering::Relaxed) {
            UNSERVED | 0 => None,
            index => self.links.get(index).map(|link| {
                link.last_served()
                    .unwrap_or_else(|| link.name().to_string())
            }),
        }
    }
}

/// One link's maker: a closure that builds this vendor for a named model.
///
/// Boxed because the four arms produce four closure types and the chain holds a
/// list of them. The credential is captured here and nowhere else, which is what
/// keeps it out of every struct in this crate.
type Maker = Box<dyn Fn(&str) -> Result<Vendor, String> + Send + Sync>;

/// The maker for one spec, and the model that spec names.
fn maker_for(spec: ProviderSpec, catalogue: Option<Reference>) -> Result<(Maker, String), String> {
    match spec {
        ProviderSpec::OpenRouter { model, api_key } => {
            let key = key_for(api_key, "OPENROUTER_API_KEY")?;
            Ok((
                Box::new(move |name: &str| {
                    // **No opt-in here, and its absence is not an oversight.**
                    // OpenRouter derives its catalogue URL from its own
                    // completions endpoint, so for this vendor the reference
                    // catalogue is not a reference at all — it is the provider
                    // speaking for itself, on a host the run already authorises.
                    // `src/verify.rs` has drawn the same distinction for prices
                    // since 0.24.0.
                    Ok(Vendor::OpenRouter(OpenRouter::new(key.clone(), name)))
                }),
                model,
            ))
        }
        ProviderSpec::Anthropic { model, api_key } => {
            let key = key_for(api_key, "ANTHROPIC_API_KEY")?;
            Ok((
                Box::new(move |name: &str| {
                    let built = Anthropic::new(key.clone(), name);
                    Ok(Vendor::Anthropic(match &catalogue {
                        Some(reference) => built.with_reference_catalogue(reference.clone()),
                        None => built,
                    }))
                }),
                model,
            ))
        }
        ProviderSpec::OpenAi { model, api_key } => {
            let key = key_for(api_key, "OPENAI_API_KEY")?;
            Ok((
                Box::new(move |name: &str| {
                    let built = OpenAi::new(key.clone(), name);
                    Ok(Vendor::OpenAi(match &catalogue {
                        Some(reference) => built.with_reference_catalogue(reference.clone()),
                        None => built,
                    }))
                }),
                model,
            ))
        }
        ProviderSpec::Compatible {
            model,
            preset,
            base_url,
            api_key,
            auth,
            ..
        } => {
            let key = api_key.unwrap_or_default();
            if preset.is_none() && base_url.is_none() {
                return Err("this provider names neither a preset nor a base URL".into());
            }
            let auth = auth.unwrap_or(Auth::Bearer);
            Ok((
                Box::new(move |name: &str| match (&preset, &base_url) {
                    (Some(preset), _) => Compatible::preset(preset, key.clone(), name)
                        .map(Vendor::Compatible)
                        .map_err(|error| error.to_string()),
                    (None, Some(base)) => Ok(Vendor::Compatible(Compatible::new(
                        base.clone(),
                        auth,
                        key.clone(),
                        name,
                    ))),
                    // Refused above, before anything was built, so this arm exists
                    // only to make the match total.
                    (None, None) => {
                        Err("this provider names neither a preset nor a base URL".into())
                    }
                }),
                model,
            ))
        }
        // `ProviderSpec` is `#[non_exhaustive]`: a provider the harness gains and
        // this release has not seen is refused by name rather than driven wrongly.
        other => Err(format!(
            "this release cannot drive a {other:?} provider yet"
        )),
    }
}

/// Build the chain a configuration names and hand it to `with`.
///
/// `specs` is head first — [`chain_of`] is how a caller gets one. A single-element
/// list builds exactly one provider and no `Fallback` at all, which is what keeps
/// an operator who has configured one provider on precisely the code path they
/// were on before this release.
///
/// `model_override` is `-m/--model`: it replaces the model the configuration names
/// for the **head only**, which is why it is applied to the extracted model rather
/// than by rewriting the spec. A fallback link keeps the model its own entry names —
/// naming one model for a whole chain would ask a second vendor for a model id only
/// the first one serves.
/// `catalogue` is the reference catalogue the two vendor providers may size
/// themselves from, from [`crate::settings::reference_catalogue`]. `None` is the
/// operator having turned it off, and it is the only way to get 0.38.2's
/// behaviour back: with it absent both providers answer `assumed_window` rather
/// than the model's real one.
///
/// **It is threaded rather than constructed here on purpose.** One `Reference`
/// is cloned into every link that takes it, and the type's cache is an
/// `Arc<OnceLock>` — so a chain of three vendor providers reads the catalogue
/// once between them, not three times. Building one per link inside this
/// function would have been three fetches and three hosts to authorise.
pub async fn build<W: WithProvider>(
    specs: Vec<ProviderSpec>,
    model_override: Option<String>,
    catalogue: Option<Reference>,
    with: W,
) -> Result<W::Out, String> {
    // **Recorded here because here is the only place that knows.** A run refused
    // because the egress policy denies the catalogue's host arrives at the turn
    // as an ordinary `Error::Refused`, fourteen call sites away in `src/main.rs`,
    // and telling it apart from a refused `web` fetch means knowing which host
    // this process is sizing from. Threading that to fourteen sites is thirteen
    // chances to miss one; a value the builder writes cannot disagree with what
    // the builder built.
    //
    // Set once and never cleared: the provider chain is constructed once per
    // process, so this is a fact about the process rather than about a turn.
    if let Some(reference) = catalogue.as_ref().and_then(Reference::host) {
        let _ = CATALOGUE_HOST.set(reference.to_string());
    }
    // **Iterators and not a `for`, which is a rule rather than a taste.**
    // `tests/dependencies.rs` refuses a loop in any file that calls a provider,
    // because a loop beside a provider call is the shape of a second agent loop.
    // The one loop this module is permitted is the chain's own over its links, and
    // that gate now names it by path and holds it to iterating nothing else — so
    // turning configuration into closures, which is what this does, has to be
    // written without one.
    let built: Result<Vec<(Maker, String)>, String> = specs
        .into_iter()
        .map(|spec| maker_for(spec, catalogue.clone()))
        .collect();
    let (makers, mut models): (Vec<Maker>, Vec<String>) = built?.into_iter().unzip();
    let Some(head) = models.first().cloned() else {
        return Err("no provider is configured; run `io setup`".into());
    };
    let head = model_override.unwrap_or(head);
    // The tail's models are fixed at build time; only the head's follows the
    // maker's argument, so a `/model` switch changes what is asked first and
    // leaves every fallback answering as its own entry says it should.
    let tail: Vec<String> = models.split_off(1);
    let make = move |name: &str| {
        let head = std::iter::once(makers[0](name)?);
        let rest: Vec<Vendor> = makers[1..]
            .iter()
            .zip(tail.iter())
            .map(|(maker, model)| maker(model))
            .collect::<Result<_, String>>()?;
        Chain::of(head.chain(rest).collect())
            .ok_or_else(|| "no provider is configured; run `io setup`".to_string())
    };
    Ok(with.call(make, head).await)
}

/// A provider that keeps a copy of every request on the way past.
///
/// **The only way io-cli can say what is in the context window.** io-harness
/// enumerates none of it — `run::prompts::compose` and `workspace_tools()` are
/// both `pub(super)`, and `EventKind::PromptComposed` carries a byte count and no
/// text — so the alternative to reading the wire is io-cli reconstructing a
/// prompt it did not compose. This does not reconstruct anything: the request
/// that goes out is the request that is reported, which is why the catalogue on
/// the page includes tools this crate never registered and could not have named.
///
/// **It wraps and never constructs**, which is what keeps it on the right side of
/// `tests/provider.rs`'s one-construction-site rule: the four constructors are
/// still written once, in [`build`], and this newtype only decorates whatever
/// they made. Both entry points reach it through the same maker, so the headless
/// path cannot drift from the interactive one.
///
/// Every method delegates. That is not boilerplate to be trimmed: the trait has
/// eight defaulted methods and a default that fired here would change behaviour
/// silently — `name()` would record `"provider"` in the trace instead of the
/// vendor, `prompt_family()` would re-derive a family from a hint this type does
/// not have, `accepts_images()` would refuse an attachment the provider accepts,
/// and `complete_streaming` would deliver a whole answer in one piece. A
/// decorator that observes must be invisible in every other respect.
///
/// The recording is a lock taken and released inside one statement, before the
/// future is returned — so nothing is held across an await, and the delegating
/// methods hand back the inner future itself rather than wrapping it in a state
/// machine of their own.
pub struct Watched<P> {
    inner: P,
    seen: Seen,
}

impl<P> Watched<P> {
    /// Wrap `inner`, reporting into `seen`.
    pub fn new(inner: P, seen: Seen) -> Self {
        Self { inner, seen }
    }
}

impl<P: Provider> Provider for Watched<P> {
    fn complete(
        &self,
        request: CompletionRequest,
    ) -> impl std::future::Future<Output = io_harness::Result<CompletionResponse>> + Send {
        self.seen.record(&request);
        self.inner.complete(request)
    }

    fn complete_streaming(
        &self,
        request: CompletionRequest,
        on_token: &(dyn Fn(&str) + Send + Sync),
    ) -> impl std::future::Future<Output = io_harness::Result<CompletionResponse>> {
        self.seen.record(&request);
        self.inner.complete_streaming(request, on_token)
    }

    fn complete_streaming_calls(
        &self,
        request: CompletionRequest,
        on_token: &(dyn Fn(&str) + Send + Sync),
        on_call: &(dyn Fn(usize, &ToolCall) + Send + Sync),
    ) -> impl std::future::Future<Output = io_harness::Result<CompletionResponse>> {
        self.seen.record(&request);
        self.inner
            .complete_streaming_calls(request, on_token, on_call)
    }

    fn models(
        &self,
    ) -> impl std::future::Future<Output = io_harness::Result<Vec<ModelInfo>>> + Send {
        self.inner.models()
    }

    fn reachable(&self) -> impl std::future::Future<Output = io_harness::Result<bool>> + Send {
        self.inner.reachable()
    }

    fn model_hint(&self) -> Option<&str> {
        self.inner.model_hint()
    }

    fn context_window(&self) -> Option<u64> {
        self.inner.context_window()
    }

    fn max_output_tokens(&self) -> Option<u64> {
        self.inner.max_output_tokens()
    }

    /// Delegated because it is defaulted, which is the whole argument: a decorator
    /// that does not forward `warm_sizing` never warms the provider it wraps, and
    /// the run then sizes from an assumption with nothing to say it did.
    fn warm_sizing(&self) -> impl std::future::Future<Output = io_harness::Result<()>> + Send {
        self.inner.warm_sizing()
    }

    fn assumed_window(&self) -> u64 {
        self.inner.assumed_window()
    }

    fn name(&self) -> &str {
        self.inner.name()
    }

    fn prompt_family(&self) -> PromptFamily {
        self.inner.prompt_family()
    }

    fn accepts_images(&self) -> bool {
        self.inner.accepts_images()
    }

    fn endpoint(&self) -> Option<&str> {
        self.inner.endpoint()
    }

    fn endpoints(&self) -> Vec<&str> {
        self.inner.endpoints()
    }

    fn last_served(&self) -> Option<String> {
        self.inner.last_served()
    }
}

/// A provider that can be printed, which is the only thing io-harness's own
/// reviewer is missing.
///
/// `ModelReviewer<P>` implements `Reviewer` — the trait `TaskContract::with_reviewer`
/// takes — only for `P: Provider + Debug + Send + Sync`, because `ModelReviewer`
/// derives `Debug` and the derive puts the bound on `P`. **None of io-harness's
/// own provider types implements `Debug`**, so a `ModelReviewer` built over one of
/// them cannot be handed to `with_reviewer` at all, and the model-judged half of
/// the verification pillar is unreachable from any crate that uses the providers
/// the harness ships. Reported upstream as io-harness#213; this is io-cli
/// shipping around it rather than waiting.
///
/// The `Debug` impl prints the vendor name and **never the credential**. That is
/// the whole reason it is written by hand rather than derived: a derive here would
/// put an API key into every error that formatted a reviewer.
///
/// Every method delegates, for the reason [`Watched`] gives at length — a default
/// that fired here would change behaviour silently. This wrapper records nothing;
/// a reviewer's request is not the operator's conversation and has no business in
/// what `/context` reports.
pub struct Printable<P> {
    inner: P,
}

impl<P> Printable<P> {
    /// Wrap `inner` so it can satisfy a `Debug` bound.
    pub fn new(inner: P) -> Self {
        Self { inner }
    }
}

impl<P: Provider> std::fmt::Debug for Printable<P> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        // The name and nothing else. `endpoint()` is safe but uninformative here,
        // and every other accessor either carries or is derived from the key.
        f.debug_struct("Printable")
            .field("provider", &self.inner.name())
            .finish()
    }
}

impl<P: Provider> Provider for Printable<P> {
    fn complete(
        &self,
        request: CompletionRequest,
    ) -> impl std::future::Future<Output = io_harness::Result<CompletionResponse>> + Send {
        self.inner.complete(request)
    }

    fn complete_streaming(
        &self,
        request: CompletionRequest,
        on_token: &(dyn Fn(&str) + Send + Sync),
    ) -> impl std::future::Future<Output = io_harness::Result<CompletionResponse>> {
        self.inner.complete_streaming(request, on_token)
    }

    fn complete_streaming_calls(
        &self,
        request: CompletionRequest,
        on_token: &(dyn Fn(&str) + Send + Sync),
        on_call: &(dyn Fn(usize, &ToolCall) + Send + Sync),
    ) -> impl std::future::Future<Output = io_harness::Result<CompletionResponse>> {
        self.inner
            .complete_streaming_calls(request, on_token, on_call)
    }

    fn models(
        &self,
    ) -> impl std::future::Future<Output = io_harness::Result<Vec<ModelInfo>>> + Send {
        self.inner.models()
    }

    fn reachable(&self) -> impl std::future::Future<Output = io_harness::Result<bool>> + Send {
        self.inner.reachable()
    }

    fn model_hint(&self) -> Option<&str> {
        self.inner.model_hint()
    }

    fn context_window(&self) -> Option<u64> {
        self.inner.context_window()
    }

    fn max_output_tokens(&self) -> Option<u64> {
        self.inner.max_output_tokens()
    }

    /// Delegated because it is defaulted, which is the whole argument: a decorator
    /// that does not forward `warm_sizing` never warms the provider it wraps, and
    /// the run then sizes from an assumption with nothing to say it did.
    fn warm_sizing(&self) -> impl std::future::Future<Output = io_harness::Result<()>> + Send {
        self.inner.warm_sizing()
    }

    fn assumed_window(&self) -> u64 {
        self.inner.assumed_window()
    }

    fn name(&self) -> &str {
        self.inner.name()
    }

    fn prompt_family(&self) -> PromptFamily {
        self.inner.prompt_family()
    }

    fn accepts_images(&self) -> bool {
        self.inner.accepts_images()
    }

    fn endpoint(&self) -> Option<&str> {
        self.inner.endpoint()
    }

    fn endpoints(&self) -> Vec<&str> {
        self.inner.endpoints()
    }

    fn last_served(&self) -> Option<String> {
        self.inner.last_served()
    }
}

/// The same maker, making watched providers.
///
/// A maker in and a maker out, rather than a provider in and a provider out,
/// because a session builds a provider more than once: `/model` calls the maker
/// again on every switch. Wrapping the *maker* means one line at the top of the
/// driver covers the switch too, and there is no second place for a session to
/// end up holding an unwatched provider — which would show as a `/context` page
/// that quietly stopped updating after the first model change.
pub fn watching<P: Provider>(
    make: impl Fn(&str) -> Result<P, String>,
    seen: Seen,
) -> impl Fn(&str) -> Result<Watched<P>, String> {
    move |name| make(name).map(|inner| Watched::new(inner, seen.clone()))
}

/// A spec built from the environment rather than from a configuration file.
///
/// `key` and `model` are passed in rather than read here so that the decision
/// this function makes — which variable is missing, and what to say about it —
/// is testable without a test mutating the process's environment, which two
/// tests running at once cannot do safely.
///
/// The credential is left as `None` on purpose: [`key_for`] reads the same
/// variable a moment later, so the key travels one path whether it came from a
/// file or from the shell, and a key never sits in a struct longer than it must.
pub fn spec_from(
    which: FromEnv,
    key: Option<String>,
    model: Option<String>,
) -> Result<ProviderSpec, String> {
    let (key_var, model_var) = which.vars();
    if key.is_none_or(|key| key.is_empty()) {
        return Err(format!(
            "`--provider` needs a credential and ${key_var} is not set"
        ));
    }
    let model = match model {
        Some(model) if !model.is_empty() => model,
        _ => {
            return Err(format!(
                "`--provider` needs a model: set ${model_var}, or pass `-m <model>`"
            ))
        }
    };
    Ok(match which {
        FromEnv::OpenRouter => ProviderSpec::OpenRouter {
            model,
            api_key: None,
        },
        FromEnv::Anthropic => ProviderSpec::Anthropic {
            model,
            api_key: None,
        },
        FromEnv::OpenAi => ProviderSpec::OpenAi {
            model,
            api_key: None,
        },
    })
}

/// The key from the configuration, or from the provider's own environment
/// variable.
///
/// The variable names are io-harness's own — the ones its `from_env`
/// constructors read — so a shell that already works with the harness works here.
pub fn key_for(api_key: Option<String>, var: &str) -> Result<String, String> {
    if let Some(key) = api_key {
        return Ok(key);
    }
    match std::env::var(var) {
        Ok(key) if !key.is_empty() => Ok(key),
        _ => Err(format!(
            "no key in the configuration and ${var} is not set; run `io setup`"
        )),
    }
}
