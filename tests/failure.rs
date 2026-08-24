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
        message: r#"{"error":{"message":"No endpoints found that support image input","code":404}}"#
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

    assert!(said.starts_with("this model cannot look at pictures"), "{said}");
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
        ("Insufficient credits to complete this request", "out of credit"),
        ("Rate limit exceeded for this key", "rate-limiting"),
        ("No auth credentials found", "rejected the credential"),
        ("No endpoints found for this model", "no provider behind this gateway"),
        ("This request exceeds the maximum context length", "longer than this model"),
    ];
    for (message, expected) in cases {
        let error = Error::Provider {
            kind: ProviderErrorKind::Request,
            status: None,
            message: message.to_string(),
            retry_after: None,
        };
        let advice = advice(&error)
            .unwrap_or_else(|| panic!("{message:?} was not recognised"));
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
