//! The ACP wire: framing, dispatch agreement, and the capability handshake.
//!
//! These tests drive the transport rather than describe it. A protocol adapter
//! is the one part of this product whose correctness is entirely about bytes
//! somebody else will read, so an assertion here that inspects a configuration
//! or a constant instead of an emitted frame is asserting the wrong thing.

mod support;

use io_cli::acp::{self, AgentCapabilities, ErrorCode, Incoming, Outgoing, GATED, SERVED};
use io_cli::acp_map::{self, Update};
use io_cli::approval::Answer;
use io_harness::{EventKind, RunEvent};
use serde_json::{json, Value};

/// The frame a well-formed request arrives as.
fn request(id: Value, method: &str, params: Value) -> String {
    json!({ "jsonrpc": "2.0", "id": id, "method": method, "params": params }).to_string()
}

/// **F1 — a frame is one line, and nothing this module emits contains an
/// interior newline.**
///
/// ACP's transport page is explicit: *"Messages are delimited by newlines
/// (`\n`), and MUST NOT contain embedded newlines."* So the obligation runs both
/// ways, and the emitting half is the half this crate can break. A pretty-printer
/// in place of `serde_json::to_string` would split one message into as many
/// frames as it has lines and desynchronise the session permanently — every
/// subsequent frame parsed against the wrong boundary.
///
/// **The content is hostile on purpose.** A message that merely round-trips
/// proves nothing: the interesting case is a payload that itself contains
/// newlines, tabs and quotes, because that is what a real agent's output and a
/// real error string look like, and it is where an escaping bug shows up.
///
/// Sabotage: `serde_json::to_string_pretty` in `acp::encode`. It fails here and
/// nowhere else in the suite.
#[test]
fn f1_no_emitted_frame_carries_an_interior_newline() {
    let hostile = "first line\nsecond line\r\nthird\ttab \"quoted\" \\ backslash";

    let frames = [
        Outgoing::Result {
            id: json!(1),
            result: json!({ "text": hostile, "nested": { "more": hostile } }),
        },
        Outgoing::Error {
            id: json!("abc"),
            code: ErrorCode::Internal,
            message: hostile.to_string(),
        },
        Outgoing::Notification {
            method: "session/update".into(),
            params: json!({ "chunk": hostile, "list": [hostile, hostile] }),
        },
        Outgoing::unattributed(ErrorCode::Parse, hostile),
    ];

    for frame in &frames {
        let line = acp::encode(frame);
        assert!(
            !line.contains('\n'),
            "an emitted frame carries a newline, so a client reading by line would \
             read it as two messages and every frame after it against the wrong \
             boundary: {line:?}",
        );
        assert!(
            !line.contains('\r'),
            "an emitted frame carries a carriage return: {line:?}",
        );
        // And it must still be the message it was. A frame made single-line by
        // dropping the content would pass the assertion above and be useless.
        let parsed: Value =
            serde_json::from_str(&line).expect("an emitted frame is a JSON document");
        assert_eq!(parsed["jsonrpc"], "2.0", "every frame names the version");
    }

    // The escaping is real rather than incidental: the hostile text survives the
    // round trip with its newlines intact, which is what distinguishes escaping
    // from stripping.
    let line = acp::encode(&frames[0]);
    let parsed: Value = serde_json::from_str(&line).expect("a JSON document");
    assert_eq!(
        parsed["result"]["text"], hostile,
        "the payload was altered to make it fit on one line, rather than escaped",
    );
}

/// **F2 — the declared capabilities and the served methods agree, in both
/// directions.**
///
/// One direction is the obvious one: a capability declared supported whose method
/// answers "method not found" is a promise the adapter cannot keep, and a client
/// discovers it mid-session.
///
/// The other direction is the one this repository has actually shipped. 0.30.0
/// documented `io skill` while the argv door had no variant for it, and 1,609
/// tests passed over the gap because every one of them entered a layer below the
/// door. A method served but never declared is the same defect wearing the
/// protocol's clothes: the specification says *"Clients and Agents MUST treat all
/// capabilities omitted in the `initialize` request as UNSUPPORTED"*, so a client
/// will never call it and the code is dead.
///
/// Asserted as one equality per gated method, so whichever side moves, the
/// failure names the method.
///
/// Sabotage: flip `load_session` to `true` while `session/load` stays out of
/// `SERVED`. Or add `"session/load"` to `SERVED` with the capability left
/// `false`. Either fails here alone.
#[test]
fn f2_every_gated_method_is_served_exactly_when_its_capability_is_declared() {
    let capabilities = AgentCapabilities::default();

    // The control: a table with no rows would make the loop below vacuous, and a
    // capability lookup that answered `false` for everything would make half of
    // each equality trivially true.
    assert!(
        !GATED.is_empty(),
        "no gated method is listed, so the agreement below asserts nothing",
    );

    for (method, capability) in GATED {
        assert_eq!(
            acp::serves(method),
            capabilities.declares(capability),
            "`{method}` is served: {}, but capability `{capability}` is declared: {}. \
             A capability without its method is a promise the adapter breaks at \
             runtime; a method without its capability is code no client will ever \
             call, because the specification says an omitted capability is \
             unsupported.",
            acp::serves(method),
            capabilities.declares(capability),
        );
    }
}

/// **F2's other half — `initialize` answers the version this module implements.**
///
/// The version is an integer major and the client refuses a mismatch, so an
/// adapter that answered a version it had not implemented would be worse than one
/// that declined. v2 exists as a draft and is deliberately not spoken.
#[test]
fn f2_initialize_answers_protocol_version_one_and_needs_no_authentication() {
    let result = acp::initialize_result("0.36.0");

    assert_eq!(
        result["protocolVersion"], 1,
        "this adapter implements ACP v1 and must say so",
    );
    assert_eq!(result["agentInfo"]["name"], "io");
    assert_eq!(result["agentInfo"]["version"], "0.36.0");

    // An empty list is legal and is the honest answer: `io setup` writes
    // io-harness's configuration file, it is not an ACP authentication method, and
    // credentials stay where they are.
    assert_eq!(
        result["authMethods"],
        json!([]),
        "no authentication step is required of the client",
    );

    // Everything omitted is unsupported by the specification's own rule, so the
    // three prompt capabilities being present and false is a statement rather
    // than an oversight.
    let prompt = &result["agentCapabilities"]["promptCapabilities"];
    for name in ["image", "audio", "embeddedContext"] {
        assert_eq!(
            prompt[name],
            json!(false),
            "`{name}` is not carried into a run yet and must not be promised",
        );
    }
    assert_eq!(result["agentCapabilities"]["loadSession"], json!(false));
}

/// **A malformed frame is answered, never fatal.**
///
/// A client that sends one bad frame must not lose its conversation. The caller
/// distinguishes the two cases by the `Result`, and `Err` here is a frame to
/// write back rather than a reason to stop reading — so every arm below produces
/// something sendable.
#[test]
fn a_frame_that_cannot_be_understood_is_answered_rather_than_fatal() {
    let cases = [
        ("", ErrorCode::Parse, "an empty line"),
        ("   ", ErrorCode::Parse, "whitespace only"),
        ("not json at all", ErrorCode::Parse, "not JSON"),
        ("{\"jsonrpc\":\"2.0\"", ErrorCode::Parse, "truncated"),
        (
            r#"{"jsonrpc":"1.0","id":1,"method":"initialize"}"#,
            ErrorCode::InvalidRequest,
            "the wrong JSON-RPC revision",
        ),
        (
            r#"{"id":1,"method":"initialize"}"#,
            ErrorCode::InvalidRequest,
            "no jsonrpc member",
        ),
        (
            // Neither a method nor an id: not a request, not a notification, and
            // not a response either. A frame carrying an id and no method *is* a
            // response and is handled in `a_response_is_correlated_not_refused`.
            r#"{"jsonrpc":"2.0"}"#,
            ErrorCode::InvalidRequest,
            "a frame that is none of the three shapes",
        ),
    ];

    for (line, expected, why) in cases {
        let answer = acp::decode(line).expect_err(why);
        match answer {
            Outgoing::Error { code, .. } => assert_eq!(
                code, expected,
                "{why} was answered with the wrong error code",
            ),
            other => panic!("{why} produced {other:?} rather than an error"),
        }
        // The answer has to be sendable, which is the property that makes this a
        // recoverable condition rather than a described one.
        let encoded = acp::encode(&answer);
        assert!(!encoded.contains('\n'), "the error frame is one line");
    }
}

/// **A parse failure answers a null id, because there is no id to answer to.**
///
/// JSON-RPC 2.0 requires it, and the temptation is to invent one or reuse the
/// last — either of which correlates an error to a request that did not cause it.
#[test]
fn a_frame_that_did_not_parse_is_answered_with_a_null_id() {
    let answer = acp::decode("{ this is not json }").expect_err("not JSON");
    let encoded = acp::encode(&answer);
    let parsed: Value = serde_json::from_str(&encoded).expect("a JSON document");
    assert_eq!(
        parsed["id"],
        Value::Null,
        "a frame whose id could not be read is answered with null, never a guess",
    );
}

/// **A request and a notification are different things, and answering a
/// notification is a protocol violation.**
///
/// The distinction is the presence of an id and nothing else. It matters at one
/// place in this adapter and it is the place that would be easy to get wrong:
/// `session/cancel` is a notification, so a dispatcher that answered everything
/// would send an unsolicited response mid-turn.
#[test]
fn an_id_is_what_separates_a_request_from_a_notification() {
    let req = acp::decode(&request(json!(7), "session/prompt", json!({ "x": 1 })))
        .expect("a well-formed request");
    match req {
        Incoming::Request { id, method, params } => {
            assert_eq!(id, json!(7));
            assert_eq!(method, "session/prompt");
            assert_eq!(params, json!({ "x": 1 }));
        }
        other => panic!("a frame with an id is a request, got {other:?}"),
    }

    let note = acp::decode(r#"{"jsonrpc":"2.0","method":"session/cancel","params":{"sessionId":"s"}}"#)
        .expect("a well-formed notification");
    match note {
        Incoming::Notification { method, .. } => assert_eq!(method, "session/cancel"),
        other => panic!("a frame with no id is a notification, got {other:?}"),
    }

    // A string id is as valid as a number and must survive unchanged — a client
    // that uses UUIDs gets its own id back, not a number this adapter coined.
    let string_id = acp::decode(&request(json!("req-9f3a"), "initialize", json!({})))
        .expect("a string id is valid");
    match string_id {
        Incoming::Request { id, .. } => assert_eq!(id, json!("req-9f3a")),
        other => panic!("expected a request, got {other:?}"),
    }
}

/// **Absent `params` is an empty object, not an error.**
///
/// The specification makes the member optional and several methods take none, so
/// refusing a frame without it would reject well-formed traffic.
#[test]
fn a_request_carrying_no_params_is_well_formed() {
    let decoded = acp::decode(r#"{"jsonrpc":"2.0","id":1,"method":"initialize"}"#)
        .expect("params is an optional member");
    assert_eq!(decoded.params(), &json!({}));
    assert_eq!(decoded.method(), "initialize");
}

/// **F4 — every kind the locked harness declares reaches a mapping or a listed
/// no-op, and the table names nothing the harness has stopped emitting.**
///
/// This is the assertion the release stands on. `EventKind` is
/// `#[non_exhaustive]`, so `acp_map::translate`'s wildcard arm is mandatory and
/// the compiler will never complain about a variant that falls through it. The
/// failure that produces is the worst one this release can ship: an editor that
/// silently never shows that the agent edited a file, behind a fully green suite.
/// io-harness 0.73.0 did a smaller version of it to `read_skill` one release ago
/// and the existing test stayed green over garbage output.
///
/// So the set is read from the **locked harness's own source**, the same way
/// `tests/triage.rs` reads it, and compared by name in both directions. A count
/// alone would be satisfied by a table that dropped one kind and invented
/// another.
///
/// Sabotage: delete any row from `MAPPING`. This fails naming that kind, rather
/// than a number going down.
#[test]
fn f4_the_mapping_is_total_over_the_locked_harnesss_event_kinds() {
    let declared = support::harness_event_kinds();
    let mapped: Vec<&str> = acp_map::MAPPING.iter().map(|(name, ..)| *name).collect();

    // The control. A source reader that returned nothing would satisfy the
    // untranslated check below while asserting nothing at all.
    assert!(
        declared.len() > 40,
        "the locked harness reader found {} kinds, which is not a plausible enum; \
         every assertion below is vacuous until this is fixed",
        declared.len(),
    );

    let untranslated: Vec<&String> = declared
        .iter()
        .filter(|name| !mapped.contains(&name.as_str()))
        .collect();
    assert!(
        untranslated.is_empty(),
        "io-harness emits kinds this adapter has no answer for: {untranslated:?}. \
         Decide for each whether it becomes a `session/update` or is a no-op, and \
         add it to `acp_map::MAPPING` with the reason. A kind left out reaches an \
         ACP client as nothing at all, and no client can report what it was never \
         sent.",
    );

    let gone: Vec<&&str> = mapped
        .iter()
        .filter(|name| !declared.contains(&(**name).to_string()))
        .collect();
    assert!(
        gone.is_empty(),
        "these names are no longer io-harness event kinds: {gone:?}",
    );

    // Every no-op carries its reason. A row with an empty one is a silence
    // nobody chose, which is the state this table exists to make impossible.
    for (name, update, why) in acp_map::MAPPING {
        assert!(
            !why.trim().is_empty(),
            "{name} maps to {update:?} and says nothing about why",
        );
    }
}

/// **F4's second half — the table and the translator agree.**
///
/// A row can say `MessageChunk` while `translate` sends nothing for it, and the
/// table would still be total. That is the same defect the table exists to
/// prevent, one level down: documentation density is not coverage, which 0.32.0's
/// sabotage pass established here by finding two criteria whose only evidence was
/// prose.
///
/// So each row is driven through the real translator with a constructed event,
/// and the answer's presence and its `sessionUpdate` string are checked against
/// what the row claims.
///
/// Sabotage: change one row's `Update` without changing `translate`. This fails
/// naming the kind and both spellings.
#[test]
fn f4_every_row_claims_what_the_translator_actually_sends() {
    let cases: &[(EventKind, &str, &str)] = &[
        (
            EventKind::Token {
                text: "hello".into(),
            },
            "token",
            "agent_message_chunk",
        ),
        (
            EventKind::Reasoning {
                text: "thinking".into(),
                tokens: 3,
            },
            "reasoning",
            "agent_thought_chunk",
        ),
        (
            EventKind::ToolCall {
                name: "read_file".into(),
                target: "src/main.rs".into(),
            },
            "tool_call",
            "tool_call",
        ),
        (
            EventKind::Refused {
                act: "write".into(),
                target: "/etc/hosts".into(),
                rule: None,
                layer: None,
            },
            "refused",
            "tool_call_update",
        ),
    ];

    for (kind, name, expected) in cases {
        let claimed = acp_map::update_for(name).unwrap_or_else(|| panic!("{name} is in MAPPING"));
        assert_ne!(
            claimed,
            Update::None,
            "{name} is listed as a no-op but this case expects it on the wire",
        );

        let sent = acp_map::translate(&RunEvent::new(1, 2, kind.clone()))
            .unwrap_or_else(|| panic!("{name} claims {claimed:?} and the translator sent nothing"));
        assert_eq!(
            sent["sessionUpdate"], *expected,
            "{name} was sent as {} where the table claims {claimed:?}",
            sent["sessionUpdate"],
        );
    }

    // And the other direction, on a row that claims nothing: a kind the table
    // calls a no-op must actually send nothing. `finished` is the one worth
    // asserting — sending it *and* the prompt result would end the turn twice.
    assert_eq!(acp_map::update_for("finished"), Some(Update::None));
    let finished = RunEvent::new(
        1,
        9,
        EventKind::Finished {
            outcome: "success".into(),
            steps: 3,
            tokens: 100,
        },
    );
    assert!(
        acp_map::translate(&finished).is_none(),
        "`finished` reached the client as a notification as well as the prompt's \
         own result, which ends the turn twice",
    );
}

/// **F3 — every `RunOutcome` the locked harness declares maps to one of ACP's
/// five stop reasons.**
///
/// Read out of the locked source rather than listed here, so a harness that gains
/// an outcome fails this instead of quietly taking the wildcard. That is the
/// discipline `tests/exec.rs` already uses for the exit-code table.
///
/// The five are fixed by the specification, so an answer outside them is a frame
/// a client will reject.
///
/// Sabotage: return `"done"` for any arm. Fails here naming the outcome.
#[test]
fn f3_every_outcome_maps_to_one_of_the_five_acp_stop_reasons() {
    const REASONS: &[&str] = &[
        "end_turn",
        "max_tokens",
        "max_turn_requests",
        "refusal",
        "cancelled",
    ];

    let outcomes = [
        io_harness::RunOutcome::Success { steps: 1 },
        io_harness::RunOutcome::StepCapReached { steps: 1 },
        io_harness::RunOutcome::VerificationFailed { steps: 1 },
        io_harness::RunOutcome::AwaitingRecovery {
            attempt_id: 1,
            steps: 1,
        },
        io_harness::RunOutcome::TimeBudgetExceeded { steps: 1 },
        io_harness::RunOutcome::CostBudgetExceeded { steps: 1 },
        io_harness::RunOutcome::Denied { steps: 1 },
        io_harness::RunOutcome::AwaitingApproval {
            request_id: 1,
            steps: 1,
        },
        io_harness::RunOutcome::AwaitingAnswer {
            question_id: 1,
            steps: 1,
        },
        io_harness::RunOutcome::AwaitingPlan { plan_id: 1, steps: 1 },
        io_harness::RunOutcome::PlanRejected { steps: 1 },
        io_harness::RunOutcome::Stalled { steps: 1 },
        io_harness::RunOutcome::Escalated {
            steps: 1,
            retryable: true,
        },
        io_harness::RunOutcome::BudgetCeilingReached { steps: 1 },
        io_harness::RunOutcome::Refused { steps: 1 },
        io_harness::RunOutcome::Cancelled { steps: 1 },
        io_harness::RunOutcome::Finished { steps: 1 },
    ];

    // The list above is hand-built because `RunOutcome` is an enum with no
    // iterator, so it can go stale. This is what stops it: the locked source's own
    // variant count must match, and a harness that adds one fails here by number
    // with the file to read named in the message.
    assert_eq!(
        outcomes.len(),
        support::harness_run_outcomes().len(),
        "the locked io-harness declares {} run outcomes and this test constructs \
         {}. Read `RunOutcome` in the pinned source and map the new one — the \
         wildcard in `acp_map::stop_reason` will otherwise answer `end_turn` for \
         an outcome that is not one.",
        support::harness_run_outcomes().len(),
        outcomes.len(),
    );

    for outcome in &outcomes {
        let reason = acp_map::stop_reason(outcome);
        assert!(
            REASONS.contains(&reason),
            "{outcome:?} maps to {reason:?}, which is not one of ACP's five stop \
             reasons; a client rejects the frame",
        );
    }

    // The distinctions that change what a client does are asserted by name rather
    // than left to the loop above, which any single constant would satisfy.
    assert_eq!(
        acp_map::stop_reason(&io_harness::RunOutcome::Cancelled { steps: 1 }),
        "cancelled",
    );
    assert_eq!(
        acp_map::stop_reason(&io_harness::RunOutcome::Denied { steps: 1 }),
        "refusal",
    );
    assert_eq!(
        acp_map::stop_reason(&io_harness::RunOutcome::Success { steps: 1 }),
        "end_turn",
    );
    // A pause is not a refusal. The run stopped with work outstanding and has
    // refused nothing; telling a client otherwise is a claim about the agent's
    // willingness that is not true.
    assert_eq!(
        acp_map::stop_reason(&io_harness::RunOutcome::AwaitingAnswer {
            question_id: 1,
            steps: 1,
        }),
        "end_turn",
        "a parked run was reported to the client as a refusal",
    );
}

/// **A tool name becomes an ACP `ToolKind`, and an unknown one becomes `other`
/// rather than a guess.**
///
/// The nine kinds are the specification's. `other` is a legal answer: a bundle's
/// own tool and an MCP server's tool are names this crate has never seen, and
/// inferring `edit` from a substring would tell a client a read was a write.
#[test]
fn a_tool_name_maps_to_one_of_the_nine_acp_tool_kinds() {
    const KINDS: &[&str] = &[
        "read", "edit", "delete", "move", "search", "execute", "think", "fetch", "other",
    ];

    for name in [
        "read_file",
        "write_file",
        "grep",
        "exec",
        "browser_navigate",
        "some_bundle__tool",
        "",
    ] {
        assert!(
            KINDS.contains(&acp_map::tool_kind(name)),
            "`{name}` mapped to `{}`, which is not an ACP ToolKind",
            acp_map::tool_kind(name),
        );
    }

    // **The real gate: every tool io-harness declares is classified
    // deliberately.** Asserted on `classified` and never on `tool_kind != other`,
    // because `other` is the correct answer for several of them — asking the
    // operator a question is not a read, an edit or an execution in ACP's
    // vocabulary. The two states that must stay distinguishable are "listed as
    // other" and "nobody looked at it", and only a table can tell them apart.
    // Read from the locked source, so a pin that adds a tool fails here rather
    // than waiting for a reviewer to notice a call drawn unclassified.
    let harness_tools = support::harness_tool_names();
    assert!(
        harness_tools.len() > 20,
        "the locked harness reader found {} tool names, which is not plausible; \
         the assertion below is vacuous until this is fixed",
        harness_tools.len(),
    );
    let unclassified: Vec<&String> = harness_tools
        .iter()
        .filter(|name| !acp_map::classified(name))
        .collect();
    assert!(
        unclassified.is_empty(),
        "io-harness declares tools this adapter has never looked at: \
         {unclassified:?}. Each needs a row in `acp_map::TOOL_KINDS` — writing \
         `other` there is a decision and is fine; leaving the name out is a gap, \
         and the symptom is an editor drawing a write as an unclassified call.",
    );

    // And the table names nothing the harness has stopped declaring, so a tool
    // removed upstream does not sit here forever as a row nobody can reach.
    let stale: Vec<&str> = acp_map::TOOL_KINDS
        .iter()
        .map(|(name, _)| *name)
        .filter(|name| !harness_tools.iter().any(|tool| tool == name))
        .collect();
    assert!(
        stale.is_empty(),
        "these are no longer io-harness tools: {stale:?}",
    );

    assert_eq!(acp_map::tool_kind("read_file"), "read");
    assert_eq!(acp_map::tool_kind("edit_file"), "edit");
    assert_eq!(acp_map::tool_kind("grep"), "search");
    assert_eq!(acp_map::tool_kind("shell"), "execute");
    assert_eq!(
        acp_map::tool_kind("github__create_issue"),
        "other",
        "a server's tool is a name this crate has never seen and must not be guessed at",
    );
    assert_eq!(
        acp_map::tool_kind("git_status"),
        "read",
        "a git reader is a read; 0.75.0 made it speculable for exactly that reason",
    );
    assert_eq!(
        acp_map::tool_kind("git_commit"),
        "execute",
        "a commit writes to the repository and is not a read",
    );
}

/// **A frame with an id and no method is an answer to something this adapter
/// asked, and is correlated rather than refused.**
///
/// `session/request_permission` is the only request this adapter makes, so its
/// answer arrives this way. A transport that refused it would make the permission
/// round trip impossible while every other test still passed.
///
/// The `error` case is asserted beside the `result` case because they must stay
/// distinguishable: a client that refused the request has not decided anything,
/// and reading a protocol error as a permission decision is the one mistake this
/// seam cannot be allowed to make.
#[test]
fn a_response_is_correlated_not_refused() {
    let ok = acp::decode(r#"{"jsonrpc":"2.0","id":4,"result":{"outcome":"cancelled"}}"#)
        .expect("a response is a well-formed frame");
    match ok {
        Incoming::Response { id, outcome } => {
            assert_eq!(id, json!(4), "the id is what correlates it to the request");
            assert_eq!(
                outcome.expect("a result, not an error")["outcome"],
                json!("cancelled"),
            );
        }
        other => panic!("expected a response, got {other:?}"),
    }

    let refused =
        acp::decode(r#"{"jsonrpc":"2.0","id":5,"error":{"code":-32601,"message":"no such method"}}"#)
            .expect("an error response is still a well-formed frame");
    match refused {
        Incoming::Response { outcome, .. } => assert_eq!(
            outcome.expect_err("an error object is not a result"),
            "no such method",
            "the client's own message is carried, not a substitute",
        ),
        other => panic!("expected a response, got {other:?}"),
    }
}

/// **F6 — the options offered are the ones io-harness can actually carry out,
/// and every answer maps to a decision.**
///
/// ACP names four `PermissionOption` kinds. Three are offered, and the missing
/// fourth is the assertion that matters: `io_harness::Decision::Deny` carries a
/// reason and nothing else, while `Approve` carries `remember: Vec<Rule>` — so
/// `allow_always` is expressible and `reject_always` is not. Offering it anyway
/// and behaving as `reject_once` would be a surface accepting an instruction it
/// cannot carry out and reporting success, which is the defect shape this product
/// has now found in itself several times.
///
/// Sabotage: add a `reject_always` row to `PERMISSION_OPTIONS`. Fails here.
#[test]
fn f6_three_permission_options_are_offered_and_reject_always_is_not() {
    let kinds: Vec<&str> = acp::PERMISSION_OPTIONS
        .iter()
        .map(|(_, _, kind)| *kind)
        .collect();

    assert_eq!(kinds, vec!["allow_once", "allow_always", "reject_once"]);
    assert!(
        !kinds.contains(&"reject_always"),
        "`reject_always` is offered but io-harness cannot remember a refusal: \
         `Decision::Deny` carries a reason and no rules, so a later matching \
         action would ask again and the operator was told otherwise",
    );

    // Every offered id maps to an answer, and the three are distinct — an option
    // list where two ids meant the same thing would be a choice that is not one.
    let answers: Vec<Answer> = acp::PERMISSION_OPTIONS
        .iter()
        .map(|(id, ..)| acp::answer_for(id).unwrap_or_else(|| panic!("{id} maps to an answer")))
        .collect();
    assert_eq!(answers, vec![Answer::Once, Answer::Session, Answer::Deny]);

    // And the ids in the emitted frame are the ids the mapping accepts. They are
    // written twice — in the table and in the match — and a disagreement between
    // them is a client answer that is silently a denial.
    let params = acp::permission_params("s-1", "write", "/tmp/x", "1-2");
    for option in params["options"].as_array().expect("an options array") {
        let id = option["optionId"].as_str().expect("an optionId");
        assert!(
            acp::answer_for(id).is_some(),
            "the frame offers `{id}`, which `answer_for` does not recognise, so \
             choosing it would be read as a denial",
        );
    }
    assert_eq!(params["sessionId"], "s-1");
    assert_eq!(params["toolCall"]["title"], "write /tmp/x");
}

/// **F6's other half — every answer that is not an offered selection is a
/// denial.**
///
/// There is exactly one safe direction to be wrong in on a permission surface.
/// A cancellation, a missing outcome, an option id this adapter never offered,
/// and a shape the specification does not name all mean the same thing here, and
/// each is asserted rather than left to a wildcard nobody checked.
///
/// Sabotage: make the unknown-option arm `Answer::Once`. Fails on three of these.
#[test]
fn f6_anything_that_is_not_a_selection_is_a_denial() {
    let cases = [
        (json!({ "outcome": "cancelled" }), "the client cancelled"),
        (json!({}), "no outcome at all"),
        (json!({ "outcome": "selected" }), "selected, with no option"),
        (
            json!({ "outcome": "selected", "optionId": "allow-everything-forever" }),
            "an option that was never offered",
        ),
        (
            json!({ "outcome": "something-new" }),
            "an outcome this adapter has never seen",
        ),
        (json!("not even an object"), "not an object"),
    ];

    for (result, why) in cases {
        assert_eq!(
            acp::permission_answer(&result),
            Answer::Deny,
            "{why} was not read as a denial",
        );
    }

    // The control: a real selection is not a denial, or every assertion above is
    // satisfied by a function that always denies.
    assert_eq!(
        acp::permission_answer(&json!({ "outcome": "selected", "optionId": "allow-once" })),
        Answer::Once,
    );
    assert_eq!(
        acp::permission_answer(&json!({ "outcome": "selected", "optionId": "allow-session" })),
        Answer::Session,
    );
}

/// **F5 — a cancel stops the run, and it stops it before the next notification is
/// written.**
///
/// Asserted on the flag reaching `Flow::Cancel`, never on a timing:
/// `tests/timing.rs` forbids a clock in any test and is right — a wall-clock
/// assertion is flaky and says nothing about *why* a path stopped.
///
/// The ordering half is the part worth writing. Reading the flag after
/// translating would send one more `session/update` after the client asked to
/// stop, and a client that cancelled and then received another chunk would
/// reasonably conclude the cancel was ignored.
///
/// Sabotage: move the flag check below the translate. The `Flow` assertions still
/// pass; the "nothing further was written" one fails.
#[test]
fn f5_a_cancel_reaches_the_observer_and_silences_it_at_once() {
    use io_harness::{Flow, Observer};
    use std::sync::atomic::Ordering;

    let (updates, mut drain) = tokio::sync::mpsc::unbounded_channel();
    let (reporter, cancelled) = acp::Reporter::new("s-7", updates);

    // Before the cancel: a token is a message chunk and the run continues.
    let token = RunEvent::new(
        1,
        1,
        EventKind::Token {
            text: "hello".into(),
        },
    );
    assert_eq!(reporter.event(&token), Flow::Continue);
    let first = drain.try_recv().expect("a chunk was written");
    match first {
        Outgoing::Notification { method, params } => {
            assert_eq!(method, "session/update");
            assert_eq!(params["sessionId"], "s-7", "the session is named on every update");
            assert_eq!(params["update"]["sessionUpdate"], "agent_message_chunk");
        }
        other => panic!("expected a notification, got {other:?}"),
    }

    // The client cancels.
    cancelled.store(true, Ordering::Relaxed);

    assert_eq!(
        reporter.event(&token),
        Flow::Cancel,
        "the run was told to continue after the client cancelled",
    );
    assert!(
        drain.try_recv().is_err(),
        "a `session/update` was written after the cancel; the client asked to stop \
         and was sent more of the thing it stopped",
    );

    // And it stays cancelled. A flag that reset itself would let the run resume
    // on the next event, which is worse than never having stopped.
    assert_eq!(reporter.event(&token), Flow::Cancel);
}

/// **A kind the table calls a no-op writes nothing at all.**
///
/// The reporter is where `MAPPING`'s no-ops become real. A `Dialed` event that
/// reached the client as an empty update would be a frame a client must parse and
/// can do nothing with.
#[test]
fn a_no_op_kind_produces_no_frame_at_all() {
    use io_harness::Observer;

    let (updates, mut drain) = tokio::sync::mpsc::unbounded_channel();
    let (reporter, _cancel) = acp::Reporter::new("s-8", updates);

    assert_eq!(acp_map::update_for("dialed"), Some(Update::None));
    reporter.event(&RunEvent::new(
        1,
        0,
        EventKind::Dialed {
            host: "openrouter.ai".into(),
            port: 443,
            allowed: true,
        },
    ));
    assert!(
        drain.try_recv().is_err(),
        "a kind the mapping calls a no-op still put a frame on the wire",
    );
}

/// A handler that records what it was asked and answers from a script.
struct Recorder {
    seen: Vec<String>,
    answers: Vec<Vec<Outgoing>>,
}

impl acp::Handler for Recorder {
    async fn handle(&mut self, incoming: Incoming) -> Vec<Outgoing> {
        self.seen.push(incoming.method().to_string());
        if self.answers.is_empty() {
            return Vec::new();
        }
        self.answers.remove(0)
    }
}

/// **The loop reads every frame in the stream, answers each, and ends on
/// end-of-input rather than on a bad frame.**
///
/// This drives the real `serve` against in-memory buffers. A transport reachable
/// only from the binary is a transport no test has ever run — 0.32.0 spent a
/// whole task undoing exactly that shape, and the generic streams here exist so
/// this release does not repeat it.
///
/// The stream deliberately mixes good and bad: a valid request, then a frame that
/// is not JSON, then a notification, then another valid request. A loop that
/// exited on the bad frame would answer two of the four and the session would be
/// silently lost.
#[tokio::test]
async fn the_serve_loop_answers_a_bad_frame_and_keeps_reading() {
    let input = [
        request(json!(1), "initialize", json!({})),
        "{ not json }".to_string(),
        r#"{"jsonrpc":"2.0","method":"session/cancel","params":{}}"#.to_string(),
        request(json!(2), "session/prompt", json!({})),
    ]
    .join("\n")
        + "\n";

    let mut handler = Recorder {
        seen: Vec::new(),
        answers: vec![
            vec![Outgoing::Result {
                id: json!(1),
                result: json!({ "ok": true }),
            }],
            // A notification is answered with nothing. Answering one is a
            // protocol violation, and the loop must not add a frame of its own.
            Vec::new(),
            vec![
                Outgoing::Notification {
                    method: "session/update".into(),
                    params: json!({ "chunk": "hello" }),
                },
                Outgoing::Result {
                    id: json!(2),
                    result: json!({ "stopReason": "end_turn" }),
                },
            ],
        ],
    };

    let mut out: Vec<u8> = Vec::new();
    acp::serve(input.as_bytes(), &mut out, &mut handler)
        .await
        .expect("the loop ends on end-of-input, not on an error");

    // The handler saw the three decodable frames, in order. The malformed one
    // never reached it — it was answered by the transport.
    assert_eq!(
        handler.seen,
        vec!["initialize", "session/cancel", "session/prompt"],
        "the loop stopped reading at the malformed frame, or passed it to the handler",
    );

    let written = String::from_utf8(out).expect("frames are UTF-8");
    let lines: Vec<&str> = written.lines().collect();
    assert_eq!(
        lines.len(),
        4,
        "expected the initialize result, the parse error, and the prompt's \
         notification and result; got:\n{written}",
    );

    let parsed: Vec<Value> = lines
        .iter()
        .map(|line| serde_json::from_str(line).expect("every written line is one JSON document"))
        .collect();

    assert_eq!(parsed[0]["id"], json!(1));
    assert_eq!(parsed[0]["result"]["ok"], json!(true));

    // The parse error is second, sits between the two answers, and carries a null
    // id because the frame it refers to had none to read.
    assert_eq!(parsed[1]["error"]["code"], json!(-32700));
    assert_eq!(parsed[1]["id"], Value::Null);

    assert_eq!(parsed[2]["method"], json!("session/update"));
    assert!(
        parsed[2].get("id").is_none(),
        "a notification carries no id: {}",
        lines[2],
    );
    assert_eq!(parsed[3]["id"], json!(2));
    assert_eq!(parsed[3]["result"]["stopReason"], json!("end_turn"));
}

/// **A stream that ends without a trailing newline still delivers its last
/// frame.**
///
/// `read_line` returns the remainder with no `\n` on it, and a loop that treated
/// a missing delimiter as a malformed frame would drop the last message of every
/// client that does not send one.
#[tokio::test]
async fn a_final_frame_with_no_trailing_newline_is_still_read() {
    let input = request(json!(1), "initialize", json!({}));
    let mut handler = Recorder {
        seen: Vec::new(),
        answers: vec![vec![Outgoing::Result {
            id: json!(1),
            result: json!({}),
        }]],
    };

    let mut out: Vec<u8> = Vec::new();
    acp::serve(input.as_bytes(), &mut out, &mut handler)
        .await
        .expect("end-of-input is not an error");

    assert_eq!(handler.seen, vec!["initialize"]);
    let written = String::from_utf8(out).expect("frames are UTF-8");
    assert!(
        written.ends_with('\n'),
        "every emitted frame is terminated, whatever the client sent: {written:?}",
    );
}

/// **The served list is the dispatch table, and it is not empty.**
///
/// A control on every other assertion that walks `SERVED`.
#[test]
fn the_served_methods_are_the_ones_this_release_implements() {
    assert!(acp::serves("initialize"));
    assert!(acp::serves("session/new"));
    assert!(acp::serves("session/prompt"));
    assert!(acp::serves("session/cancel"));

    assert!(
        !acp::serves("session/load"),
        "`session/load` is excluded from 0.36.0 and must not answer",
    );
    assert!(!acp::serves("fs/read_text_file"), "a client method, not ours");
    assert!(!acp::serves("nonsense/method"));

    assert_eq!(
        SERVED.len(),
        4,
        "the dispatch table changed without this test being updated",
    );
}
