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

use io_harness::{Anthropic, Auth, Compatible, OpenAi, OpenRouter, Provider, ProviderSpec};

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
        other => Err(format!("this release cannot drive a {other:?} provider yet")),
    }
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
