//! A turn's failure, said to the person who typed the prompt.
//!
//! The subject is `io_cli::failure`, which is a function of an `io_harness::Error`
//! and nothing else — so every arm is a branch a test can flip without a
//! provider, a session or a network.

use io_cli::failure::{advice, said};
use io_harness::{Error, ProviderErrorKind};

/// The error an operator actually met: a screenshot attached to a model that
/// cannot look at one, reported by OpenRouter as a routing failure.
fn no_image_endpoint() -> Error {
    Error::Provider {
        kind: ProviderErrorKind::Request,
        status: Some(404),
        message:
            r#"{"error":{"message":"No endpoints found that support image input","code":404}}"#
                .to_string(),
        retry_after: None,
    }
}

#[test]
fn a_model_that_cannot_see_is_said_in_words_the_operator_can_act_on() {
    let advice = advice(&no_image_endpoint()).expect("a recognised failure");
    assert!(
        advice.contains("cannot look at pictures"),
        "the sentence names the condition: {advice}",
    );
    assert!(
        advice.contains("/model"),
        "and the thing to do about it: {advice}",
    );
    assert!(
        advice.contains("was not sent"),
        "and what happened to the image, which is the operator's next question: \
         {advice}",
    );
}

/// **The vendor's own text is never thrown away.** A sentence this crate wrote
/// stands in front of the harness's line, never in place of it: a wrong guess
/// then costs a reader one line rather than the error itself.
#[test]
fn the_harness_line_survives_underneath_the_sentence() {
    let error = no_image_endpoint();
    let said = said(&error);

    assert!(
        said.starts_with("this model cannot look at pictures"),
        "{said}"
    );
    assert!(
        said.contains(&error.to_string()),
        "the harness's own line is still there in full: {said}",
    );
    assert!(said.contains("HTTP 404"), "{said}");
}

#[test]
fn an_unrecognised_failure_is_the_harness_line_and_nothing_added() {
    let error = Error::Provider {
        kind: ProviderErrorKind::Request,
        status: Some(418),
        message: "the model is a teapot".to_string(),
        retry_after: None,
    };
    assert_eq!(advice(&error), None);
    assert_eq!(said(&error), error.to_string());
}

/// The five other conditions an operator meets, each matched on the words rather
/// than on a status: the same condition arrives as a 404 from one gateway, a 400
/// from another and a 422 from a third.
#[test]
fn the_recognised_failures_are_matched_on_what_they_say() {
    let cases = [
        (
            "Insufficient credits to complete this request",
            "out of credit",
        ),
        ("Rate limit exceeded for this key", "rate-limiting"),
        ("No auth credentials found", "rejected the credential"),
        (
            "No endpoints found for this model",
            "no provider behind this gateway",
        ),
        (
            "This request exceeds the maximum context length",
            "longer than this model",
        ),
    ];
    for (message, expected) in cases {
        let error = Error::Provider {
            kind: ProviderErrorKind::Request,
            status: None,
            message: message.to_string(),
            retry_after: None,
        };
        let advice = advice(&error).unwrap_or_else(|| panic!("{message:?} was not recognised"));
        assert!(
            advice.contains(expected),
            "{message:?} was matched to the wrong sentence: {advice}",
        );
    }
}

/// The image sentence outranks the routing one. Both match the OpenRouter body —
/// it says "No endpoints found that support image input" — and the specific
/// answer is the useful one.
#[test]
fn the_image_sentence_wins_over_the_general_routing_one() {
    let advice = advice(&no_image_endpoint()).expect("a recognised failure");
    assert!(advice.contains("pictures"), "{advice}");
}

/// The session-head shape of `Error::Conflict`: `run_id` holds a **session** id,
/// and `owner` and `expires_at` are empty because a head is a value that moved
/// rather than something a process is holding. Written by
/// `Store::set_session_head_if`, which is what every head write in this product
/// now goes through.
fn head_moved_under_us() -> Error {
    Error::Conflict {
        run_id: 7,
        owner: String::new(),
        expires_at: String::new(),
    }
}

/// The run-lease shape, which is the one the variant's own `Display` was written
/// for: a real run, a named holder, a real expiry.
fn run_lease_held() -> Error {
    Error::Conflict {
        run_id: 7,
        owner: "io-cli@another-terminal".to_string(),
        expires_at: "2026-08-27T11:04:00.000Z".to_string(),
    }
}

/// **Asserted by absence, and it has to be.** `Error::Conflict`'s `Display` is
/// `run {run_id} is held by another owner until {expires_at}`, so on the head
/// shape it renders a session id under the word "run" and stops on the word
/// "until" with nothing after it — and the operator meets that sentence after a
/// turn they have already paid for. A test that only checked the new sentence was
/// present would pass with that line still sitting underneath it, which is
/// exactly the state this arm exists to end.
#[test]
fn a_lost_head_race_never_calls_the_session_a_run_or_prints_an_empty_expiry() {
    let error = head_moved_under_us();
    let said = said(&error);

    assert!(
        said.contains("session 7"),
        "the id is named, and named as the session it is: {said}",
    );
    assert!(
        said.contains("another `io`"),
        "and what happened is that another process moved first: {said}",
    );

    assert!(
        !said.contains("run 7") && !said.contains("run "),
        "the id must never be presented as a run id: {said}",
    );
    assert!(
        !said.to_lowercase().contains("until"),
        "there is no expiry on a head, so nothing may be rendered as one: {said}",
    );
    assert!(
        !said.contains("owner"),
        "and no holder, because a head has none: {said}",
    );
    assert!(
        !said.contains(&error.to_string()),
        "this is the one line this module drops rather than keeps underneath — it \
         is wrong rather than terse: {said}",
    );
}

/// The other shape, which the harness's line describes correctly, so it keeps it.
/// Rendering both shapes the same way would only move the lie: a real lease with a
/// real holder and a real expiry called a session would be the same defect the
/// other way round.
#[test]
fn a_run_another_process_holds_still_says_the_holder_and_the_expiry_it_has() {
    let error = run_lease_held();
    let said = said(&error);

    assert!(
        said.starts_with("another process is already running this"),
        "the sentence says what to do about a held run: {said}",
    );
    assert!(
        said.contains(&error.to_string()),
        "and the harness's own line is right for this shape, so it stays: {said}",
    );
    assert!(
        said.contains("2026-08-27T11:04:00.000Z"),
        "including the expiry this one actually has: {said}",
    );
    assert!(
        !said.contains("session"),
        "a held run is not a moved head: {said}",
    );
}
