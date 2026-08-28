//! The second model, the one that judges the work rather than doing it.
//!
//! **A module of its own, and `tests/provider.rs` is the reason.** That gate says
//! every vendor constructor is written exactly once outside the wizard's
//! handshake and the `/provider` panel, so that the interactive and the headless
//! entry points cannot drift apart — one site builds the provider a turn runs on.
//! A reviewer is not that provider. It is a third construction, like
//! [`crate::verify`]'s credential ping: it answers a question and is thrown away,
//! it never drives a turn, and it deliberately runs a *different* model from the
//! one doing the work. So it is excluded by name, exactly as the handshake is,
//! and held to that boundary by a test rather than trusted.
//!
//! The gate caught this release writing the construction into
//! [`crate::gates`] first, then into [`crate::provider`] beside the real one. Both
//! would have been a second site with its own key resolution. This is the third
//! answer and the honest one.
//!
//! **Nothing here calls a provider.** It constructs `ModelReviewer`, which holds
//! its own provider and its own model, and hands it to io-harness to call when the
//! run loop reaches the gate. That is what keeps `tests/dependencies.rs`'s count of
//! provider calls in this crate at one — the wizard's ping — which is the gate that
//! says this crate has not grown an agent loop.

use std::sync::Arc;

use io_harness::{
    Anthropic, Compatible, ModelReviewer, OpenAi, OpenRouter, ProviderSpec, Reviewer,
};

use crate::provider::{key_for, Printable};

/// A reviewer that asks `model` through `spec`, ready for `with_reviewer`.
///
/// `Provider` is not dyn-compatible, so each variant is constructed in its own arm
/// and boxed as `Arc<dyn Reviewer>` afterwards; `Reviewer` *is* dyn-compatible,
/// which is what makes one return type possible at all. Each provider is wrapped
/// in [`Printable`] to satisfy the `Debug` bound io-harness's own provider types
/// cannot — see that type for the upstream report.
///
/// The key falls back to the vendor's own environment variable through
/// [`key_for`], the same resolver every other provider in this crate gets: a
/// reviewer configured the way the session's provider is configured should
/// authenticate the way the session does.
///
/// The error is the provider's own message wherever there is one. A refusal an
/// operator can act on names the endpoint and the key; "could not build a
/// reviewer" names neither.
pub fn build(spec: &ProviderSpec, model: &str) -> Result<Arc<dyn Reviewer>, String> {
    match spec {
        ProviderSpec::OpenRouter { api_key, .. } => {
            let key = key_for(api_key.clone(), "OPENROUTER_API_KEY")?;
            Ok(Arc::new(ModelReviewer::new(
                Printable::new(OpenRouter::new(key, model)),
                model,
            )))
        }
        ProviderSpec::Anthropic { api_key, .. } => {
            let key = key_for(api_key.clone(), "ANTHROPIC_API_KEY")?;
            Ok(Arc::new(ModelReviewer::new(
                Printable::new(Anthropic::new(key, model)),
                model,
            )))
        }
        ProviderSpec::OpenAi { api_key, .. } => {
            let key = key_for(api_key.clone(), "OPENAI_API_KEY")?;
            Ok(Arc::new(ModelReviewer::new(
                Printable::new(OpenAi::new(key, model)),
                model,
            )))
        }
        ProviderSpec::Compatible {
            base_url,
            preset,
            api_key,
            auth,
            ..
        } => {
            // The model is the reviewer's, never the endpoint's configured one:
            // the entire criterion is that a *second* model reads the work, and
            // reusing the spec's model is the mistake it exists to prevent.
            let secret = api_key.clone().unwrap_or_default();
            let compatible = match (preset, base_url) {
                (Some(preset), _) => {
                    Compatible::preset(preset, secret, model).map_err(|error| error.to_string())?
                }
                (None, Some(base)) => Compatible::new(
                    base,
                    auth.unwrap_or(io_harness::Auth::Bearer),
                    secret,
                    model,
                ),
                (None, None) => {
                    return Err("this endpoint names neither a preset nor a base URL".into())
                }
            };
            Ok(Arc::new(ModelReviewer::new(
                Printable::new(compatible),
                model,
            )))
        }
        // `ProviderSpec` is `#[non_exhaustive]`. A provider this release has not
        // seen cannot be built into a reviewer, and refusing where the operator is
        // still looking at what they typed is the whole point of refusing at all.
        //
        // **The spec is never formatted into the message.** Every variant holds
        // `api_key: Option<String>` verbatim and `ProviderSpec` derives `Debug`,
        // so `{other:?}` would put the operator's credential into a refusal that
        // this crate then records into the scrollback. That is the same trap
        // `Printable`'s hand-written `Debug` exists to avoid two files away.
        _ => Err(
            "this release does not know how to review with that kind of provider yet".to_string(),
        ),
    }
}
