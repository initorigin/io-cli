//! The configuration scope the rest of the suite's fixtures are built on.
//!
//! io-harness 0.74.0 refuses `[[provider]]`, `[[mcp]]`, `[[lsp]]` and — in
//! `io.local.toml` as well as `io.toml` — `[[hook]]` from any file that lives in
//! a workspace, along with ten widening values and an absolute `run.skills` or
//! `run.templates`. `Scope::User` is the only exemption, because `$IO_CONFIG` is
//! outside every workspace and a run that can write its own root cannot reach it.
//!
//! Every fixture in this suite that declares one of those sections therefore has
//! to be user-scoped, and `support::user_scope` is the one way it is done. This
//! file is that helper's own gate. It exists because the helper is load-bearing
//! for eighteen test binaries and a silent regression in it would not fail
//! honestly — it would make a hundred tests fail somewhere else, for a reason
//! none of them names.

mod support;

/// The section 0.74.0 refuses in a workspace file, in the smallest form that
/// still parses.
///
/// `kind` and `model` and nothing else: `model` is required and `api_key` is not,
/// so the fixture names no credential at all. An earlier draft indirected through
/// `${env:}`, which io-harness resolves at parse time and refuses when unset — it
/// made every test in this file depend on a process-global variable it also had
/// to set, for a field the parser never wanted.
const DECLARES_A_PROVIDER: &str = "\
[[provider]]
kind = \"openrouter\"
model = \"anthropic/claude-sonnet-4.5\"
";

#[test]
fn the_helper_produces_a_configuration_the_harness_accepts() {
    let scope = support::user_scope(DECLARES_A_PROVIDER);

    assert!(
        scope.config.provider_spec().is_some(),
        "a user-scoped file may declare a provider, and the fixture has to prove it did — the \
         assertion is on the parsed spec rather than on the absence of an error, because a \
         configuration that dropped the section would also parse"
    );
}

#[test]
fn the_user_file_is_not_inside_the_workspace_the_configuration_was_discovered_against() {
    let scope = support::user_scope(DECLARES_A_PROVIDER);

    let file = scope.path();
    let root = scope.root();

    assert!(
        !file.starts_with(root),
        "the user-scope file is at {} and the discovery root is {} — a file inside the root is a \
         candidate twice, once through IO_CONFIG as Scope::User and once as root/io.toml under \
         Scope::Project, and the project read is the one that refuses it",
        file.display(),
        root.display()
    );
}

/// The trap, asserted from the other side.
///
/// This is the shape every fixture in this repository had before 0.74.0, and the
/// reason a hundred of them went red at once. It is asserted rather than
/// described so that "pointing `IO_CONFIG` at it is enough" cannot quietly become
/// true again in a fixture somebody writes next year.
#[test]
fn the_same_file_inside_the_discovery_root_is_refused_however_io_config_points_at_it() {
    let _guard = support::env_lock();

    {
        let dir = tempfile::tempdir().expect("a directory");
        let path = dir.path().join("io.toml");
        std::fs::write(&path, DECLARES_A_PROVIDER).expect("the fixture is written");

        std::env::set_var("IO_CONFIG", &path);
        let discovered = io_harness::Config::discover(dir.path());
        std::env::remove_var("IO_CONFIG");

        let error = discovered.expect_err(
            "a file that is both the IO_CONFIG target and the discovery root's own io.toml is read \
             as Scope::Project as well, and 0.74.0 refuses a provider there",
        );
        let said = error.to_string();
        assert!(
            said.contains("provider"),
            "the refusal names the section it refused: {said}"
        );
    }
}

#[test]
fn a_project_file_beside_a_user_file_is_still_refused() {
    // No outer `env_lock` here: `user_scope_with_project` takes it for itself and
    // `std::sync::Mutex` is not reentrant, so holding it across the call is a
    // deadlock rather than extra safety.
    let error = support::user_scope_with_project("[run]\nmax_steps = 40\n", DECLARES_A_PROVIDER)
        .expect_err("the project half declares a provider, which is refused");

    let said = error.to_string();
    assert!(
        said.contains("provider"),
        "the operator needs to be told which section was refused, not that a file could not be \
         read: {said}"
    );
}

#[test]
fn the_kept_variant_leaves_io_config_set_and_the_plain_one_does_not() {
    // This one asserts over the process environment itself, so it has to hold the
    // lock across both halves and use the forms that do not take it again.
    let _guard = support::env_lock();

    {
        let _scope = support::user_scope_locked(DECLARES_A_PROVIDER, true);
        assert!(
            std::env::var_os("IO_CONFIG").is_some(),
            "user_scope_kept exists for the product paths that re-discover for themselves"
        );
    }
    assert!(
        std::env::var_os("IO_CONFIG").is_none(),
        "and it puts the environment back when the fixture goes out of scope, or the next test in \
         this binary reads a file that has been deleted"
    );

    let _scope = support::user_scope_locked(DECLARES_A_PROVIDER, false);
    assert!(
        std::env::var_os("IO_CONFIG").is_none(),
        "the plain variant unsets it before it returns"
    );
}

/// A refusal has to be reachable as a value, not only as a panic.
#[test]
fn the_fallible_form_hands_back_the_refusal_rather_than_panicking() {
    let _guard = support::env_lock();

    // `sandbox.force_floor = false` is one of the ten widening values, and it is
    // refused in a workspace file — but this fixture is user-scoped, so it is
    // accepted here. The refusal below is a different thing entirely: a key no
    // `[run]` table has.
    let refused = support::try_user_scope_locked("[run]\nnot_a_key = 1\n", false);

    assert!(
        refused.is_err(),
        "an unknown key is a deserialization failure in any scope"
    );
}
