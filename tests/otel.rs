//! F13 — a run exports as OpenTelemetry spans, and io says only what it knows.
//!
//! io-harness has shipped an OTLP exporter since its 0.78.0, behind a feature
//! flag this crate did not enable — so a capability the harness released was
//! unreleased in practice for anyone whose only interface is `io`. Turning the
//! flag on is one line of a manifest; what this file gates is the part that is
//! io-cli's own, which is the seam it is attached at and the sentence it comes
//! with.
//!
//! **The sentence is the interesting half.** io-harness reports export success or
//! loss through no public value: a batch the collector refused, or one that
//! failed three times and was dropped, goes to a `tracing::warn` and nowhere
//! else, and `Export::send` propagates no error at all because it runs on a task
//! nobody awaits. Reading that account would take a tracing subscriber, and a
//! subscriber is a dependency in a set whose whole argument is that it is ten
//! names. So io-cli says what it *configured* and never what arrived — and these
//! arms hold it to that, because a line saying "exporting" is exactly the kind of
//! claim an operator would stop looking behind.

mod support;

use io_harness::Config;

/// A configuration with an `[otel]` section, and nothing else io-cli reads.
fn configured(section: &str) -> Config {
    Config::from_toml(section).expect("the section parses")
}

#[test]
fn f13_no_section_configures_nothing_and_says_nothing() {
    let (exporter, said) = io_cli::contract::otel(&configured(""));
    assert!(
        exporter.is_none(),
        "an exporter was built for a configuration that asked for none",
    );
    assert!(
        said.is_none(),
        "a session with no [otel] section said something about telemetry: {said:?}",
    );
}

/// **F13 — what is configured is named, and what arrived is not claimed.**
///
/// Driven through `contract::otel_said` rather than through a `Config`, and that
/// is forced rather than convenient: io-harness refuses an `[otel]` section in
/// any file inside a workspace — a collector is a host every span of every run is
/// posted to, and `io.toml` arrives with a `git clone` — so a config carrying one
/// cannot be built from a string at all. The sentence is split out pure for
/// exactly this reason.
///
/// Sabotage: add the word "exporting", or any past-tense claim about delivery, to
/// that sentence. This fails on the vocabulary sweep, which is the only thing
/// standing between an operator and a line they would trust.
#[test]
fn f13_the_line_names_the_endpoint_and_claims_no_delivery() {
    let said = io_cli::contract::otel_said("http://localhost:4318/v1/traces", "io-under-test");

    assert!(
        said.contains("http://localhost:4318"),
        "the endpoint an operator configured is the one thing they will check \
         this line against: {said}",
    );
    assert!(
        said.contains("io-under-test"),
        "and the service name, which is how they will find it in a collector: {said}",
    );

    // The claims this line may not make. Each is a word an operator would read as
    // "it arrived", and none of them is something this process can know.
    for forbidden in [
        "exported",
        "exporting",
        "delivered",
        "received",
        "reached the collector",
        "spans sent",
    ] {
        assert!(
            !said.to_lowercase().contains(forbidden),
            "the line claims `{forbidden}`, and io-harness reports delivery \
             through no public value — a dropped batch is a log line this process \
             cannot read: {said}",
        );
    }
}

/// **F13 — a collector is user-scope only, and io-cli does not work around it.**
///
/// io-harness refuses an `[otel]` section in any file inside a workspace, for the
/// reason it refuses `[[provider]]` there: the collector is a host every span of
/// every run is posted to, reached with whatever credential the same table names,
/// and `io.toml` arrives with a `git clone`. A repository could otherwise ship a
/// file that quietly forwards every run of everyone who cloned it.
///
/// This asserts the refusal reaches an operator as io-harness wrote it rather
/// than being caught and softened here — the same rule `src/configure.rs` follows
/// for every other widening refusal, which is that the harness owns what may be
/// declared where.
#[test]
fn f13_a_workspace_file_may_not_name_a_collector() {
    let refusal = Config::from_toml("[otel]\nendpoint = \"http://localhost:4318\"\n")
        .expect_err("a project-scoped collector is refused");
    let said = refusal.to_string();
    assert!(
        said.contains("otel"),
        "the refusal has to name the section: {said}",
    );
    assert!(
        said.contains("user-scope") || said.contains("IO_CONFIG"),
        "and where it does belong, or an operator has been told no and nothing \
         else: {said}",
    );
}

// The documentation half of this — that no shipped page promises a span
// arrived — is `n5_no_shipped_page_claims_a_span_was_delivered` in
// `tests/docs.rs`, which is where `shipped_prose` lives and where every other
// sweep of that kind is kept.
