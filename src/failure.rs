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
pub fn said(error: &Error) -> String {
    match advice(error) {
        Some(advice) => format!("{advice}\n{error}"),
        None => error.to_string(),
    }
}

/// The sentence, if this release recognises the failure.
///
/// Matched on the *text* rather than on a status code, and that is deliberate:
/// the same condition arrives as a 404 from one gateway, a 400 from another and
/// a 422 from a third, while the sentence they write is stable enough to match
/// on. A miss is silent — the harness's own line is already correct, only terse.
pub fn advice(error: &Error) -> Option<&'static str> {
    let said = error.to_string().to_lowercase();
    if said.contains("image input")
        || said.contains("does not support image")
        || said.contains("image_url")
        || said.contains("vision")
    {
        return Some(
            "this model cannot look at pictures. Pick one that can with /model, or \
             ask the question without the attachment — the image was not sent.",
        );
    }
    if said.contains("insufficient") && (said.contains("credit") || said.contains("quota")) {
        return Some("the account is out of credit. Nothing was spent on this turn.");
    }
    if said.contains("rate limit") || said.contains("429") {
        return Some("the provider is rate-limiting this key. Wait and ask again.");
    }
    if said.contains("no auth") || said.contains("unauthorized") || said.contains("401") {
        return Some(
            "the provider rejected the credential. `/provider` runs the setup again, \
             and the key it writes lives in io-harness's configuration file.",
        );
    }
    if said.contains("no endpoints found") || said.contains("no allowed providers") {
        return Some(
            "no provider behind this gateway will serve that model as asked. Pick a \
             different model with /model.",
        );
    }
    if said.contains("context length")
        || said.contains("context_length")
        || said.contains("too many tokens")
    {
        return Some(
            "the conversation is longer than this model will take. `/clear` starts a \
             new one, and `/resume` still has this one.",
        );
    }
    None
}
