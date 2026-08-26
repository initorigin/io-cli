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
    OpenRouter, PromptFamily, Provider, ProviderSpec, ToolCall,
};

use crate::cli::FromEnv;
use crate::context::Seen;

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

pub async fn build<W: WithProvider>(
    spec: ProviderSpec,
    model_override: Option<String>,
    with: W,
) -> Result<W::Out, String> {
    let model = |configured: String| model_override.unwrap_or(configured);
    match spec {
        ProviderSpec::OpenRouter { model: m, api_key } => {
            let key = key_for(api_key, "OPENROUTER_API_KEY")?;
            let make = move |name: &str| Ok(OpenRouter::new(key.clone(), name));
            Ok(with.call(make, model(m)).await)
        }
        ProviderSpec::Anthropic { model: m, api_key } => {
            let key = key_for(api_key, "ANTHROPIC_API_KEY")?;
            let make = move |name: &str| Ok(Anthropic::new(key.clone(), name));
            Ok(with.call(make, model(m)).await)
        }
        ProviderSpec::OpenAi { model: m, api_key } => {
            let key = key_for(api_key, "OPENAI_API_KEY")?;
            let make = move |name: &str| Ok(OpenAi::new(key.clone(), name));
            Ok(with.call(make, model(m)).await)
        }
        ProviderSpec::Compatible {
            model: m,
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
            let make = move |name: &str| match (&preset, &base_url) {
                (Some(preset), _) => {
                    Compatible::preset(preset, key.clone(), name).map_err(|error| error.to_string())
                }
                (None, Some(base)) => Ok(Compatible::new(base.clone(), auth, key.clone(), name)),
                // Refused above, before anything was built, so this arm exists
                // only to make the match total.
                (None, None) => Err("this provider names neither a preset nor a base URL".into()),
            };
            Ok(with.call(make, model(m)).await)
        }
        // `ProviderSpec` is `#[non_exhaustive]`: a provider the harness gains and
        // this release has not seen is refused by name rather than driven wrongly.
        other => Err(format!(
            "this release cannot drive a {other:?} provider yet"
        )),
    }
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
        self.inner.complete_streaming_calls(request, on_token, on_call)
    }

    fn models(&self) -> impl std::future::Future<Output = io_harness::Result<Vec<ModelInfo>>> + Send {
        self.inner.models()
    }

    fn reachable(&self) -> impl std::future::Future<Output = io_harness::Result<bool>> + Send {
        self.inner.reachable()
    }

    fn model_hint(&self) -> Option<&str> {
        self.inner.model_hint()
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
