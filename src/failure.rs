//! What a turn's failure says to the person who typed the prompt.
//!
//! io-harness reports a failure the way a library must: the kind, the HTTP
//! status, and the vendor's own body, unaltered. That is the right thing for a
//! library to carry and the wrong thing to put on a prompt line. An operator who
//! attached a screenshot and got
//!
//! ```text
//! error: provider error (Request, HTTP 404): {"error":{"message":"No endpoints found that support image input","code":404}}
//! ```
//!
//! has been told, in the vocabulary of a routing layer, that the model they
//! chose cannot look at pictures — and has not been told the one thing they can
//! act on, which is to pick a different model or send the question without the
//! image.
//!
//! **The vendor's words are never thrown away.** Every sentence below is put in
//! front of the harness's own text rather than in place of it: a message this
//! release has not seen still arrives whole, and one it has seen arrives with a
//! sentence saying what to do about it. Guessing wrong then costs a reader a
//! line, and never the error.

use io_harness::Error;

/// The operator's sentence for `error`, and the error itself under it.
///
/// One `String`, because the caller commits one line — see `App::say`. The
/// leading sentence is this crate's; everything after the dash is io-harness's.
///
/// **One failure has its harness line dropped rather than kept underneath**, and
/// it is the only one. [`Error::Conflict`]'s own `Display` reads `run {run_id} is
/// held by another owner until {expires_at}`, which is written for the run-lease
/// shape. On the session-head shape `run_id` holds a *session* id under the word
/// "run" and `expires_at` is empty, so the line names the wrong noun and ends on
/// the word "until". This module's rule is that terse text is worth keeping in
/// front of the operator; it was never a rule about text that is wrong.
pub fn said(error: &Error) -> String {
    match advice(error) {
        Some(advice) if head_conflict(error) => advice,
        Some(advice) => format!("{advice}\n{error}"),
        None => error.to_string(),
    }
}

/// Whether a conflict is the session-head shape rather than the run-lease one.
///
/// **The two shapes are not fully separable from the value, so this errs towards
/// the head.** Every head conflict has an empty `owner` — `set_session_head_if`
/// writes it so, because a head has a value that moved rather than a holder — but
/// a *lease* conflict is built with an empty `owner` too when the lease row has
/// already gone by the time the refusal is described (`conflict_from`, in
/// io-harness's `state.rs`). An empty `owner` therefore means "this value knows of
/// no holder and no expiry", which is exactly the case where the lease sentence
/// cannot be said honestly either: it would name a holder that is not there and an
/// expiry that is empty. The head wording is the least wrong answer for both, and
/// neither of them calls the id a run.
fn head_conflict(error: &Error) -> bool {
    matches!(error, Error::Conflict { owner, .. } if owner.is_empty())
}

/// The sentence, if this release recognises the failure.
///
/// Matched on the *text* rather than on a status code, and that is deliberate:
/// the same condition arrives as a 404 from one gateway, a 400 from another and
/// a 422 from a third, while the sentence they write is stable enough to match
/// on. A miss is silent — the harness's own line is already correct, only terse.
///
/// [`Error::Conflict`] is the exception and is matched on the **value**, before
/// any text is looked at, because on that variant the text is the defect rather
/// than the evidence — see [`said`]. It is also why this returns an owned
/// `String`: a lost head race is worth naming the session it was lost on, and a
/// `&'static str` cannot.
pub fn advice(error: &Error) -> Option<String> {
    if let Error::Conflict { run_id, owner, .. } = error {
        return Some(if owner.is_empty() {
            // `run_id` is the session id here, and is called one. No expiry is
            // rendered: a head does not have one, and the field is empty.
            format!(
                "another `io` moved session {run_id} on first. This was refused rather \
                 than written over that process's turn — nothing retried and nothing \
                 forced it. `/resume` re-reads the conversation as it now stands."
            )
        } else {
            // The shape the harness's own line was written for, so it says the
            // owner and the expiry underneath and this only says what to do.
            "another process is already running this. Wait for it to finish or for its \
             lease to lapse, then ask again."
                .to_string()
        });
    }
    let said = error.to_string().to_lowercase();
    if said.contains("image input")
        || said.contains("does not support image")
        || said.contains("image_url")
        || said.contains("vision")
    {
        return Some(
            "this model cannot look at pictures. Pick one that can with /model, or \
             ask the question without the attachment — the image was not sent."
                .to_string(),
        );
    }
    if said.contains("insufficient") && (said.contains("credit") || said.contains("quota")) {
        return Some("the account is out of credit. Nothing was spent on this turn.".to_string());
    }
    if said.contains("rate limit") || said.contains("429") {
        return Some("the provider is rate-limiting this key. Wait and ask again.".to_string());
    }
    if said.contains("no auth") || said.contains("unauthorized") || said.contains("401") {
        return Some(
            "the provider rejected the credential. `/provider` runs the setup again, \
             and the key it writes lives in io-harness's configuration file."
                .to_string(),
        );
    }
    if said.contains("no endpoints found") || said.contains("no allowed providers") {
        return Some(
            "no provider behind this gateway will serve that model as asked. Pick a \
             different model with /model."
                .to_string(),
        );
    }
    if said.contains("context length")
        || said.contains("context_length")
        || said.contains("too many tokens")
    {
        return Some(
            "the conversation is longer than this model will take. `/clear` starts a \
             new one, and `/resume` still has this one."
                .to_string(),
        );
    }
    None
}
