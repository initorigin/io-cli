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

/// A catalogue read, whole rows and not just ids, and **unfiltered**.
///
/// **The rows carry prices, and until 0.22.0 this read threw them away.** It read
/// the same catalogue, mapped every [`io_harness::ModelInfo`] down to its `id`,
/// and dropped the `price`, `price_tiers` and `price_source` on the same row —
/// while the interface over it reported token counts and called the money question
/// unanswerable. [`crate::prices`] is what keeps them.
///
/// `source` names the catalogue to read, for the operator on a self-hosted or
/// `compatible` endpoint the reference catalogue has never heard of. `None` is
/// io-harness's own default.
///
/// **Narrowing happens at the caller and not here, and that is the whole of what
/// makes `source_url` mean anything.** [`named`] narrows the default catalogue —
/// one vendor's view of the entire field — down to the provider in force, and for
/// a `compatible` endpoint it can only answer "none of these", because a reference
/// list cannot say what a server it has never heard of serves. An operator who
/// sets `source_url` has already answered that question: they have pointed io-cli
/// at the catalogue their own endpoint publishes, and every row of it is theirs.
/// Narrowing here would have applied the wrong filter to the right catalogue,
/// which is what it did — the key was inert, over a test asserting the empty
/// result, while `src/settings.rs` and the README both called it the only way a
/// self-hosted operator gets prices at all.
///
/// An error is not fatal and comes back as an empty vector: a catalogue that
/// cannot be read is a reason to make the user type a model, not a reason to stop.
/// A caller that needs to tell "nothing was served" from "nothing was priced" has
/// [`crate::prices::Catalogue::served`] for it.
pub async fn served(source: Option<&str>) -> Vec<io_harness::ModelInfo> {
    let reference = match source {
        Some(url) if !url.is_empty() => Reference::at(url),
        _ => Reference::new(),
    };
    reference.models().await.unwrap_or_default()
}

/// The catalogue filtered to what `spec` serves, spelled the way `spec` names it.
///
/// Separated from the fetch so the whole of this decision is testable without a
/// socket. The **spelling matters beyond the wizard's list**: the id here is the
/// key a price is stored under, and it has to match the `model` io-harness records
/// on a provider call — which is the name the operator configured, not the
/// catalogue's namespaced one.
pub fn named(
    spec: &ProviderSpec,
    models: Vec<io_harness::ModelInfo>,
) -> Vec<io_harness::ModelInfo> {
    match spec {
        // The reference catalogue is OpenRouter's own, so for OpenRouter it is not
        // a reference at all — it is the provider speaking for itself.
        ProviderSpec::OpenRouter { .. } => models,
        ProviderSpec::Anthropic { .. } => strip(models, "anthropic/"),
        ProviderSpec::OpenAi { .. } => strip(models, "openai/"),
        // Anything else serves whatever it serves and the reference cannot say.
        _ => Vec::new(),
    }
}

/// The models this provider serves, as ids, sorted and deduplicated.
///
/// What the wizard puts in front of the user. Unchanged in behaviour since 0.1.0;
/// it is now one `map` over [`served`] rather than its own read.
pub async fn catalogue(spec: &ProviderSpec) -> Vec<String> {
    let mut ids: Vec<String> = named(spec, served(None).await)
        .into_iter()
        .map(|model| model.id)
        .collect();
    ids.sort();
    ids.dedup();
    ids
}

/// The priced rows of a catalogue, sorted by model.
///
/// **Takes rows [`named`] has already spelled, and does not filter again.**
/// Filtering is not idempotent — `named` strips a provider's prefix, and running
/// it twice on an Anthropic catalogue finds no `anthropic/` left to strip and
/// discards every row. So the caller filters once and hands the result both here
/// and to whatever else needs a count of it.
///
/// **A model served with no price is absent, never entered at zero.**
/// io-harness's `PriceTable::price` returning `None` is what makes
/// `Spend::unpriced_calls` count that call, which is what lets `/cost` say its
/// total is a floor rather than reporting a partial sum as a total. A zero here
/// would silently claim the model is free.
pub fn priced(models: Vec<io_harness::ModelInfo>) -> Vec<(String, io_harness::pricing::Price)> {
    let mut rows: Vec<(String, io_harness::pricing::Price)> = models
        .into_iter()
        .filter_map(|model| model.price.map(|price| (model.id, price)))
        .collect();
    rows.sort_by(|a, b| a.0.cmp(&b.0));
    rows.dedup_by(|a, b| a.0 == b.0);
    rows
}

fn strip(models: Vec<io_harness::ModelInfo>, prefix: &str) -> Vec<io_harness::ModelInfo> {
    models
        .into_iter()
        .filter_map(|mut model| {
            let id = model.id.strip_prefix(prefix)?.to_string();
            model.id = id;
            Some(model)
        })
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
