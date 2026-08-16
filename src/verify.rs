//! The wizard's live checks: does this credential work, and what models are on
//! offer.
//!
//! Both are one call each, made against the endpoint the user just named. The
//! point of doing it here rather than on the first real prompt is that a wizard
//! can explain a failure and put the cursor back on the field that caused it,
//! whereas a first turn can only fail.
//!
//! `Provider` is not dyn-compatible — its methods return `impl Future` — so each
//! variant is constructed and pinged in its own arm rather than through a
//! trait object. That is the shape of the harness's trait, not a choice made here.

use io_harness::{
    Anthropic, Compatible, CompletionRequest, OpenAi, OpenRouter, Provider, ProviderSpec, Reference,
};

/// The user text of the verification call. Short on purpose: this is a
/// handshake, and the answer is thrown away.
const PING: &str = "Reply with the single word: ok";

/// Check a credential against the live endpoint.
///
/// `Ok(())` means the provider answered. `Err` carries **the provider's own
/// message**, because every provider reports a bad credential differently and the
/// difference is the information — "401 No auth credentials found" tells a user
/// what to do, and "verification failed" does not.
pub async fn credential(spec: &ProviderSpec) -> Result<(), String> {
    match spec {
        ProviderSpec::OpenRouter { model, api_key } => {
            let key = resolve(api_key, "OPENROUTER_API_KEY")?;
            ping(OpenRouter::new(key, model)).await
        }
        ProviderSpec::Anthropic { model, api_key } => {
            let key = resolve(api_key, "ANTHROPIC_API_KEY")?;
            ping(Anthropic::new(key, model)).await
        }
        ProviderSpec::OpenAi { model, api_key } => {
            let key = resolve(api_key, "OPENAI_API_KEY")?;
            ping(OpenAi::new(key, model)).await
        }
        ProviderSpec::Compatible {
            model,
            base_url,
            preset,
            api_key,
            auth,
            ..
        } => {
            let key = api_key.clone().unwrap_or_default();
            let compatible = match (preset, base_url) {
                (Some(preset), _) => {
                    Compatible::preset(preset, key, model).map_err(|error| error.to_string())?
                }
                (None, Some(base)) => {
                    Compatible::new(base, auth.unwrap_or(io_harness::Auth::Bearer), key, model)
                }
                (None, None) => {
                    return Err("this endpoint names neither a preset nor a base URL".into())
                }
            };
            ping(compatible).await
        }
        // `ProviderSpec` is `#[non_exhaustive]`: a provider the harness gains and
        // this release has not seen cannot be verified, and saying so is better
        // than reporting a pass it never made.
        other => Err(format!(
            "this release does not know how to verify a {other:?} provider yet"
        )),
    }
}

/// The provider's model catalogue, as ids.
///
/// io-harness reads and prices catalogues already; this filters the reference
/// catalogue down to the models the chosen provider actually serves. An error is
/// not fatal — the wizard offers the provider's usual default instead, because a
/// catalogue that cannot be read is a reason to make the user type a model, not a
/// reason to stop.
pub async fn catalogue(spec: &ProviderSpec) -> Vec<String> {
    let Ok(models) = Reference::new().models().await else {
        return Vec::new();
    };
    let mut ids: Vec<String> = match spec {
        // The reference catalogue is OpenRouter's own, so for OpenRouter it is not
        // a reference at all — it is the provider speaking for itself.
        ProviderSpec::OpenRouter { .. } => models.into_iter().map(|model| model.id).collect(),
        ProviderSpec::Anthropic { .. } => strip(models, "anthropic/"),
        ProviderSpec::OpenAi { .. } => strip(models, "openai/"),
        // Anything else serves whatever it serves and the reference cannot say.
        _ => Vec::new(),
    };
    ids.sort();
    ids.dedup();
    ids
}

fn strip(models: Vec<io_harness::ModelInfo>, prefix: &str) -> Vec<String> {
    models
        .into_iter()
        .filter_map(|model| model.id.strip_prefix(prefix).map(str::to_string))
        .collect()
}

/// The key from the spec, or from the provider's own environment variable.
fn resolve(api_key: &Option<String>, var: &str) -> Result<String, String> {
    if let Some(key) = api_key {
        return Ok(key.clone());
    }
    match std::env::var(var) {
        Ok(key) if !key.is_empty() => Ok(key),
        _ => Err(format!(
            "no key was given and ${var} is not set in this shell"
        )),
    }
}

async fn ping<P: Provider>(provider: P) -> Result<(), String> {
    let request = CompletionRequest {
        user: PING.into(),
        ..Default::default()
    };
    match provider.complete(request).await {
        Ok(_) => Ok(()),
        // The message, not a category. A transport failure and a rejected key are
        // both reasons to stay on this screen, and the text is what distinguishes
        // them for the person reading it.
        Err(error) => Err(error.to_string()),
    }
}
