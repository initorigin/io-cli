//! The MCP policy preflight: whether a server the operator just added will start.
//!
//! Two things are asserted here and they fail for different reasons.
//!
//! **The normaliser is a copy, so it is tested as a copy.** `io_cli::preflight::target`
//! reproduces io-harness's `pub(crate) net::target`, and nothing in either crate can
//! notice the day the two stop agreeing. A happy-path assertion — `https://example.com`
//! becomes `example.com:443` — passes against essentially any URL parser ever written,
//! so it would go on passing through a rewrite that changed every answer that matters.
//! What is enumerated below instead is the case table: the scheme defaults, every shape
//! that must return `None`, the userinfo drop, and both IPv6 spellings. Each of those is
//! a line of the original that a re-derivation would get differently.
//!
//! **The verdict half is asserted through `io_harness::Policy`, never through a rendered
//! string alone.** The policy is built the way a session builds it —
//! `approval::session_policy(base, posture, remembered)`, the same call `tests/policy.rs`
//! makes and the same value the next turn runs under — so what is checked is the merged
//! stack, not a policy invented for the test. The sentence is asserted too, but only
//! after the outcome is: a report that names the right rule while reaching the wrong
//! conclusion is the failure this whole module exists to prevent.
//!
//! The load-bearing case is [`an_unresolvable_url_is_a_refusal_and_never_a_start`].
//! `NetGuard::check` refuses a URL it cannot resolve (io-harness 0.69.0,
//! `src/net.rs:275-284`) before it consults the policy at all. Reading that `None` as
//! "nothing to check, so nothing objects" is a one-character mistake that reports
//! *permitted* for a server the runtime is certain to refuse — the preflight lying in
//! the only direction that costs a turn.

use io_cli::approval;
use io_cli::preflight::{self, Outcome, Preflight};
use io_cli::settings::Posture;
use io_harness::{Act, Config, Effect, McpServer, Policy, Rule};

/// A stdio server named `id` spawning `command`.
fn stdio(id: &str, command: &str) -> McpServer {
    McpServer::stdio(id, command)
}

/// An http server named `id` at `url`.
fn http(id: &str, url: &str) -> McpServer {
    McpServer::http(id, url)
}

/// A rule the operator allowed for the session, as `/…  allow` records one.
fn remembered(act: Act, effect: Effect, pattern: &str) -> Rule {
    Rule {
        act,
        effect,
        pattern: pattern.to_string(),
    }
}

// ---------------------------------------------------------------------------
// The normaliser. One assertion per line of the function it copies.
// ---------------------------------------------------------------------------

/// The port is filled from the scheme, and all four schemes that reach a host are
/// spelled out. A copy that knew only `http`/`https` would pass every other test in
/// this file and silently report every websocket server as unresolvable.
#[test]
fn target_fills_the_port_from_the_scheme() {
    assert_eq!(
        preflight::target("https://mcp.example.com").as_deref(),
        Some("mcp.example.com:443"),
    );
    assert_eq!(
        preflight::target("http://mcp.example.com").as_deref(),
        Some("mcp.example.com:80"),
    );
    assert_eq!(
        preflight::target("wss://mcp.example.com").as_deref(),
        Some("mcp.example.com:443"),
    );
    assert_eq!(
        preflight::target("ws://mcp.example.com").as_deref(),
        Some("mcp.example.com:80"),
    );
}

/// A port the URL states is the port that is checked. Overwriting it with the
/// scheme's default would make `allow_net("host:8080")` fail to cover the server it
/// was written for, and — worse — make `deny_net("host:8080")` fail to refuse it.
#[test]
fn target_keeps_a_port_the_url_states() {
    assert_eq!(
        preflight::target("https://mcp.example.com:8443").as_deref(),
        Some("mcp.example.com:8443"),
    );
    assert_eq!(
        preflight::target("http://127.0.0.1:3000/mcp").as_deref(),
        Some("127.0.0.1:3000"),
    );
}

/// The authority ends at the first `/`, `?` or `#`. A path, a query or a fragment
/// carried into the target would be compared against host rules that can never match
/// it, and a rule that misses fails open.
#[test]
fn target_stops_at_the_path_the_query_and_the_fragment() {
    for url in [
        "https://mcp.example.com/mcp/v1",
        "https://mcp.example.com?token=abc",
        "https://mcp.example.com#frag",
    ] {
        assert_eq!(
            preflight::target(url).as_deref(),
            Some("mcp.example.com:443"),
            "the authority ends before the first `/`, `?` or `#`: {url}",
        );
    }
}

/// Credentials are not part of the host. A target of `user:pw@host:443` would match
/// no rule an operator would ever write — and it would put the password into every
/// rendered line and every trace row that quotes the target.
#[test]
fn target_drops_the_userinfo() {
    assert_eq!(
        preflight::target("https://user:pw@mcp.example.com/mcp").as_deref(),
        Some("mcp.example.com:443"),
    );
    assert_eq!(
        preflight::target("https://token@mcp.example.com").as_deref(),
        Some("mcp.example.com:443"),
    );
}

/// An IPv6 literal keeps its brackets, which is the only thing that makes the
/// trailing `:port` split unambiguous. Strip them and `::1` splits into host `:` and
/// port `1`.
#[test]
fn target_keeps_the_brackets_on_an_ipv6_literal() {
    assert_eq!(
        preflight::target("https://[::1]").as_deref(),
        Some("[::1]:443")
    );
    assert_eq!(
        preflight::target("http://[::1]:8080/mcp").as_deref(),
        Some("[::1]:8080"),
    );
    assert_eq!(
        preflight::target("https://[2001:db8::1]").as_deref(),
        Some("[2001:db8::1]:443"),
    );
}

/// A scheme that never opens a connection has no host to check. `None`, and the
/// caller must turn that into a refusal rather than into silence — which is what
/// [`an_unresolvable_url_is_a_refusal_and_never_a_start`] asserts.
#[test]
fn target_refuses_a_scheme_that_never_dials() {
    for url in [
        "file:///etc/passwd",
        "ftp://mcp.example.com",
        "stdio://server",
        "HTTPX://mcp.example.com",
    ] {
        assert_eq!(
            preflight::target(url),
            None,
            "only http, https, ws and wss resolve to a host: {url}",
        );
    }
}

/// Malformed input is `None` too, and the empty-authority cases are the ones a
/// hand-rolled split gets wrong: `https://` and `https:///mcp` both parse far enough
/// to produce an empty string, and an empty host is not a host.
#[test]
fn target_refuses_input_with_no_authority() {
    for url in [
        "mcp.example.com",
        "https:/mcp.example.com",
        "",
        "https://",
        "https:///mcp",
        "https://?q=1",
        "https://:443",
    ] {
        assert_eq!(preflight::target(url), None, "there is no host in {url:?}");
    }
}

// ---------------------------------------------------------------------------
// The verdict.
// ---------------------------------------------------------------------------

/// A `deny_exec` naming the server binary refuses the spawn, and the report carries
/// the rule and the layer *the verdict* gave it — not strings this crate composed.
/// An operator sent to the wrong file is no better off than one told nothing.
#[test]
fn a_denied_binary_is_refused_and_names_the_rule_and_the_layer() {
    let base = Policy::default()
        .layer("ops-baseline")
        .deny_exec("github-mcp-server");
    let policy = approval::session_policy(&base, Some(Posture::Workspace), &[]);

    let p = preflight::check(&stdio("github", "github-mcp-server"), &policy);

    assert_eq!(p.act, Act::Exec, "a stdio server is gated as an exec");
    assert_eq!(p.target, "github-mcp-server", "the command, verbatim");
    assert_eq!(p.outcome, Outcome::Refused);
    assert!(!p.starts());
    assert_eq!(p.rule.as_deref(), Some("github-mcp-server"));
    assert_eq!(p.layer.as_deref(), Some("ops-baseline"));

    let line = preflight::line(&p);
    assert!(
        line.contains("github-mcp-server") && line.contains("ops-baseline"),
        "the sentence must name the rule and the layer that decided: {line:?}",
    );
    assert!(
        !line.contains("will start"),
        "a refusal must not read as a start: {line:?}",
    );
}

/// The same, over the network act. The rule names the bare host and the target
/// carries the port, which is the pairing `Policy::explain` handles by trying both
/// forms — a preflight that only ever compared the `host:port` form would report this
/// deny as permitted.
#[test]
fn a_denied_host_is_refused_and_names_the_rule_and_the_layer() {
    let base = Policy::default()
        .layer("ops-baseline")
        .deny_net("mcp.example.com");
    let policy = approval::session_policy(&base, Some(Posture::Workspace), &[]);

    let p = preflight::check(&http("remote", "https://mcp.example.com/mcp"), &policy);

    assert_eq!(p.act, Act::Net, "an http server is gated as a net act");
    assert_eq!(p.target, "mcp.example.com:443", "normalised, with its port");
    assert_eq!(p.outcome, Outcome::Refused);
    assert!(!p.starts());
    assert_eq!(p.rule.as_deref(), Some("mcp.example.com"));
    assert_eq!(p.layer.as_deref(), Some("ops-baseline"));
    assert!(
        preflight::line(&p).contains("ops-baseline"),
        "the layer is where the operator goes to change it",
    );
}

/// A stdio server the policy allows by name starts, and says so.
#[test]
fn an_allowed_binary_starts() {
    let base = Policy::default()
        .layer("app")
        .allow_exec("github-mcp-server");
    let policy = approval::session_policy(&base, Some(Posture::ReadOnly), &[]);

    let p = preflight::check(&stdio("github", "github-mcp-server"), &policy);

    assert_eq!(p.outcome, Outcome::Permitted);
    assert!(p.starts());
    assert_eq!(p.layer.as_deref(), Some("app"));
    assert!(
        preflight::line(&p).contains("will start"),
        "{:?}",
        preflight::line(&p),
    );
}

/// An http server permitted by a rule the operator allowed **for this session**.
///
/// This is the merged stack doing its job: every posture denies `net` by default
/// (`settings::Posture::defaults`), so without the remembered layer this server is
/// refused. Asserting it here is what proves the preflight reads the same three
/// layers the turn will — file, posture, session — and not just the file's own.
#[test]
fn a_host_allowed_for_the_session_starts() {
    let base = Policy::default();
    let server = http("remote", "https://mcp.example.com/mcp");

    let without = approval::session_policy(&base, Some(Posture::Workspace), &[]);
    let p = preflight::check(&server, &without);
    assert_eq!(
        p.outcome,
        Outcome::Refused,
        "every posture denies net by default, so this must not start unaided",
    );

    let allowed = [remembered(Act::Net, Effect::Allow, "mcp.example.com")];
    let with = approval::session_policy(&base, Some(Posture::Workspace), &allowed);
    let p = preflight::check(&server, &with);

    assert_eq!(p.outcome, Outcome::Permitted);
    assert!(p.starts());
    assert_eq!(
        p.layer.as_deref(),
        Some("remembered"),
        "the session's own layer is the one that decided, and is named as such",
    );
}

/// **The one that must fail loudly.** A URL with no host is a refusal, because
/// `NetGuard::check` refuses on exactly this before the policy is consulted. The
/// assertions are written as three separate claims so that making `None` mean "fine"
/// cannot pass any of them: the outcome is not `Permitted`, the server does not
/// start, and the sentence does not say it will.
#[test]
fn an_unresolvable_url_is_a_refusal_and_never_a_start() {
    // The most permissive policy that exists. Nothing here denies anything, so a
    // preflight that consulted the policy at all would answer *permitted* — the
    // refusal has to come from the unresolvable target itself.
    let policy = approval::session_policy(&Policy::permissive(), None, &[]);

    for url in ["file:///usr/local/bin/server", "not-a-url", "https://"] {
        let p = preflight::check(&http("broken", url), &policy);

        assert_eq!(
            p.outcome,
            Outcome::Unresolvable,
            "{url}: an unresolvable target is its own refusal",
        );
        assert_ne!(
            p.outcome,
            Outcome::Permitted,
            "{url}: reporting permitted here is the preflight lying about a server \
             io-harness will refuse",
        );
        assert!(
            !p.starts(),
            "{url}: `NetGuard::check` returns `Error::Refused` for this, so it does not start",
        );
        assert_eq!(
            p.target, url,
            "the URL as written is what the refusal carries"
        );
        assert_eq!(
            p.rule, None,
            "no rule decided it — there was nothing to check"
        );
        assert_eq!(p.layer, None);

        let line = preflight::line(&p);
        assert!(
            !line.contains("will start"),
            "{url}: the sentence must not promise a start: {line:?}",
        );
    }
}

/// A file with no `[policy]` section yields `Config::policy() == None`, and that
/// absence is `Policy::default()` — never "nothing denies anything".
///
/// `Policy::default()` asks before an exec and denies net outright, so the honest
/// answer for a freshly-added server in an unconfigured file is that neither
/// transport starts. A caller that read the `None` as [`Policy::permissive`] would
/// report both as permitted, which is the same fail-open shape as the unresolvable
/// URL arriving through a different door.
#[test]
fn a_file_with_no_policy_section_is_the_default_policy_and_not_permission() {
    let config = Config::from_toml("[app.io-cli]\ntheme = \"dark\"\n")
        .expect("a file with no policy section parses");
    assert!(
        config.policy().is_none(),
        "the premise of this test: no section means no policy value",
    );

    let policy = config.policy().unwrap_or_default();

    let spawned = preflight::check(&stdio("local", "some-mcp-server"), &policy);
    assert_eq!(
        spawned.outcome,
        Outcome::Ask,
        "the default exec tier asks, and a spawn is not asked about",
    );
    assert!(
        !spawned.starts(),
        "`authorize_spawn` refuses anything short of allow, so an asking default does \
         not start a server — reporting this one as permitted is the failure",
    );

    let dialled = preflight::check(&http("remote", "https://mcp.example.com"), &policy);
    assert_eq!(
        dialled.outcome,
        Outcome::Refused,
        "the default net tier is deny, and no rule named this host",
    );
    assert!(!dialled.starts());

    for p in [&spawned, &dialled] {
        assert_eq!(
            p.rule, None,
            "a tier default decided, so there is no rule to name",
        );
        assert!(
            preflight::line(p).contains("default"),
            "with no rule to name, the sentence has to point at the default instead: {:?}",
            preflight::line(p),
        );
    }
}

/// A deny in a project layer beats an allow in the user layer beneath it, and beats
/// the posture's tier default over both. The answer follows the merged stack, which
/// is the only reading that matches `Policy::explain` — it resolves deny-first across
/// every layer, and reaches the tier default only when no rule matched at all.
#[test]
fn a_project_deny_beats_the_user_allow_and_the_tier_default() {
    let user = Policy::permissive().layer("user").allow_exec("*");
    let project = Policy::permissive()
        .layer("project")
        .deny_exec("secrets-mcp-server");
    // `Posture::Workspace` allows exec by default, so the tier default would start
    // both of these. Only the merged layers say otherwise.
    let policy = approval::session_policy(&user.merge(project), Some(Posture::Workspace), &[]);
    assert_eq!(
        policy.defaults.exec,
        Effect::Allow,
        "the premise: the default on its own would permit the denied server",
    );

    let denied = preflight::check(&stdio("secrets", "secrets-mcp-server"), &policy);
    assert_eq!(denied.outcome, Outcome::Refused);
    assert!(!denied.starts());
    assert_eq!(
        denied.layer.as_deref(),
        Some("project"),
        "the deny came from the project layer and that is what the operator is told",
    );

    let allowed = preflight::check(&stdio("notes", "notes-mcp-server"), &policy);
    assert_eq!(allowed.outcome, Outcome::Permitted);
    assert_eq!(
        allowed.layer.as_deref(),
        Some("user"),
        "the rule that decided is the user layer's `*`, not the tier default — a \
         verdict attributed to the wrong layer sends the operator to the wrong file",
    );
}

/// An asking policy stops one transport and permits the other, and both halves are
/// upstream's behaviour rather than this crate's reading.
///
/// `authorize_spawn` returns `Error::Refused` on anything that is not `Allow`
/// (`mcp.rs:571-589`), so an `ask` exec never reaches an approver. `NetGuard::check`
/// returns `Ask` as a verdict and `McpSession::connect` discards it
/// (`mcp.rs:336-339`), so an `ask` net dials anyway. A preflight that mapped `Ask` to
/// one answer for both would be wrong for whichever transport it did not pick.
#[test]
fn an_asking_policy_stops_a_spawn_and_lets_a_dial_through() {
    let base = Policy::default()
        .layer("cautious")
        .rule(Act::Exec, Effect::Ask, "local-mcp-server")
        .rule(Act::Net, Effect::Ask, "mcp.example.com");
    let policy = approval::session_policy(&base, Some(Posture::Workspace), &[]);

    let spawned = preflight::check(&stdio("local", "local-mcp-server"), &policy);
    assert_eq!(spawned.outcome, Outcome::Ask);
    assert!(
        !spawned.starts(),
        "a server spawn is refused rather than asked about, before the first step",
    );
    assert!(
        !preflight::line(&spawned).contains("will start"),
        "{:?}",
        preflight::line(&spawned),
    );

    let dialled = preflight::check(&http("remote", "https://mcp.example.com"), &policy);
    assert_eq!(dialled.outcome, Outcome::Ask);
    assert!(
        dialled.starts(),
        "the connect path discards the ask verdict, so the connection is opened",
    );
}

/// The report is about the server it was handed. A preflight that answered from the
/// id, or from the first server in some list, would be right often enough in a
/// one-server configuration to ship.
#[test]
fn the_report_is_about_the_server_it_was_given() {
    let policy = approval::session_policy(&Policy::permissive(), None, &[]);
    let p: Preflight = preflight::check(&stdio("notes", "notes-mcp-server"), &policy);

    assert_eq!(p.server, "notes");
    assert_eq!(p.target, "notes-mcp-server");
    assert!(
        preflight::line(&p).contains("notes-mcp-server"),
        "the sentence names the thing that was checked: {:?}",
        preflight::line(&p),
    );
}
