//! The ACP wire: framing, dispatch agreement, and the capability handshake.
//!
//! These tests drive the transport rather than describe it. A protocol adapter
//! is the one part of this product whose correctness is entirely about bytes
//! somebody else will read, so an assertion here that inspects a configuration
//! or a constant instead of an emitted frame is asserting the wrong thing.

use io_cli::acp::{self, AgentCapabilities, ErrorCode, Incoming, Outgoing, GATED, SERVED};
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
            r#"{"jsonrpc":"2.0","id":1}"#,
            ErrorCode::InvalidRequest,
            "a request naming no method",
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
