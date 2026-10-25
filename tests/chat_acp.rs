//! Offline ACP contracts: real subprocess framing plus controller failure/replay
//! semantics. No model, network, GUI, credentials or global configuration.
use canopy_desktop::chat::{
    acp::{
        AcpSession, HostCapabilities, HostOperation, Start,
        wire::{self, RequestId},
    },
    process::Event,
    session::{Draft, Status},
    transcript::{Body, Decision, Outcome},
};
use serde_json::{Value, json};

fn feed(s: &mut AcpSession, v: Value) -> Vec<Value> {
    s.on_message(wire::parse_line(v.to_string().as_bytes()).unwrap().unwrap())
}
fn answer(s: &mut AcpSession, line: &Value, result: Value) -> Vec<Value> {
    feed(s, json!({"jsonrpc":"2.0","id":line["id"],"result":result}))
}
fn open(start: Start) -> AcpSession {
    let (mut s, lines) = AcpSession::new(start).unwrap();
    let lines = answer(
        &mut s,
        &lines[0],
        json!({"protocolVersion":1,"agentCapabilities":{"loadSession":true,"promptCapabilities":{"image":true}}}),
    );
    answer(
        &mut s,
        &lines[0],
        json!({"sessionId":"opaque/session:not-a-uuid"}),
    );
    assert_eq!(s.status(), &Status::Ready);
    s
}
fn update(s: &mut AcpSession, u: Value) {
    feed(
        s,
        json!({"jsonrpc":"2.0","method":"session/update","params":{"sessionId":"opaque/session:not-a-uuid","update":u}}),
    );
}
fn config() -> Value {
    json!({"configOptions":[{"id":"reasoning.setting/v2","name":"Reasoning","type":"select","category":"thought_level","currentValue":"vendor:auto","options":[{"value":"vendor:auto","name":"Automatic"},{"value":"vendor:deep","name":"More"}]}]})
}

#[test]
fn load_replay_is_atomic_and_never_duplicates_a_checkpoint() {
    let mut source = open(Start::new(std::env::temp_dir()));
    let lines = source.submit(&Draft::text("hello"), "message-1").unwrap();
    update(
        &mut source,
        json!({"sessionUpdate":"agent_message_chunk","content":{"type":"text","text":"saved answer"}}),
    );
    answer(&mut source, &lines[0], json!({"stopReason":"end_turn"}));
    let checkpoint = source.transcript().items().to_vec();
    for have_checkpoint in [true, false] {
        let mut start = Start::new(std::env::temp_dir());
        start.resume = Some("opaque/session:not-a-uuid".into());
        start.has_checkpoint = have_checkpoint;
        if have_checkpoint {
            start.checkpoint = checkpoint.clone();
        }
        let (mut s, lines) = AcpSession::new(start).unwrap();
        let lines = answer(
            &mut s,
            &lines[0],
            json!({"protocolVersion":1,"agentCapabilities":{"loadSession":true}}),
        );
        assert_eq!(lines[0]["method"], "session/load");
        update(
            &mut s,
            json!({"sessionUpdate":"user_message_chunk","content":{"type":"text","text":"hello"}}),
        );
        update(
            &mut s,
            json!({"sessionUpdate":"user_message_chunk","content":{"type":"text","text":" again"}}),
        );
        update(
            &mut s,
            json!({"sessionUpdate":"agent_message_chunk","content":{"type":"text","text":"saved answer"}}),
        );
        assert_eq!(
            s.transcript().items().len(),
            if have_checkpoint { checkpoint.len() } else { 0 }
        );
        answer(&mut s, &lines[0], json!({}));
        if have_checkpoint {
            assert_eq!(s.transcript().items(), checkpoint);
        } else {
            assert_eq!(s.transcript().items().len(), 2);
            assert!(
                matches!(&s.transcript().items()[0].body,Body::User(m) if m.text=="hello again")
            );
        }
    }
    let mut start = Start::new(std::env::temp_dir());
    start.resume = Some("opaque/session:not-a-uuid".into());
    let (mut s, lines) = AcpSession::new(start).unwrap();
    let lines = answer(
        &mut s,
        &lines[0],
        json!({"protocolVersion":1,"agentCapabilities":{"loadSession":true}}),
    );
    update(
        &mut s,
        json!({"sessionUpdate":"agent_message_chunk","content":{"type":"text","text":"must not leak"}}),
    );
    feed(
        &mut s,
        json!({"jsonrpc":"2.0","id":lines[0]["id"],"error":{"code":-32000,"message":"load failed"}}),
    );
    assert!(s.status().ended());
    assert!(s.transcript().items().is_empty());
}

#[test]
fn opaque_permission_choice_first_answer_wins_and_cancel_keeps_turn_open() {
    let mut s = open(Start::new(std::env::temp_dir()));
    let prompt = s.submit(&Draft::text("work"), "m").unwrap().remove(0);
    for id in [json!(7), json!("7")] {
        feed(
            &mut s,
            json!({"jsonrpc":"2.0","id":id,"method":"session/request_permission","params":{"sessionId":"opaque/session:not-a-uuid","toolCall":{"toolCallId":"tool","title":"Run something"},"options":[{"optionId":"deny-looking-id","name":"Allow","kind":"allow_once"},{"optionId":"allow-looking-id","name":"Deny","kind":"reject_once"}]}}),
        );
    }
    let id = RequestId::Number(7.into());
    assert!(s.respond(&id, Some("invented")).is_err());
    let reply = s.respond(&id, Some("deny-looking-id")).unwrap();
    assert_eq!(reply["result"]["outcome"]["optionId"], "deny-looking-id");
    assert!(s.respond(&id, Some("allow-looking-id")).is_err());
    assert_eq!(s.status(), &Status::NeedsDecision);
    let lines = s.interrupt();
    assert_eq!(lines.len(), 2);
    assert_eq!(lines[1]["method"], "session/cancel");
    assert!(lines[1].get("id").is_none());
    assert_eq!(s.status(), &Status::Interrupting);
    assert!(s.submit(&Draft::text("premature"), "x").is_err());
    answer(&mut s, &prompt, json!({"stopReason":"cancelled"}));
    assert_eq!(s.status(), &Status::Ready);
    assert!(
        s.transcript()
            .items()
            .iter()
            .any(|i| matches!(&i.body,Body::Permission(p) if p.decision==Decision::Allowed))
    );
    assert!(
        matches!(&s.transcript().items().last().unwrap().body,Body::TurnEnd(end) if end.outcome==Outcome::Interrupted)
    );
}

#[test]
fn rejected_config_retains_selection_and_services_require_capabilities_and_session_scope() {
    let mut s = open(
        Start::new(std::env::temp_dir()).with_host(HostCapabilities::default().files(true, false)),
    );
    let mut config_update = config();
    config_update["sessionUpdate"] = json!("config_option_update");
    update(&mut s, config_update);
    let line = s
        .set_option("reasoning.setting/v2", "vendor:deep")
        .unwrap()
        .remove(0);
    assert_eq!(line["params"]["configId"], "reasoning.setting/v2");
    assert_eq!(s.options()[0].value, "vendor:auto");
    feed(
        &mut s,
        json!({"jsonrpc":"2.0","id":line["id"],"error":{"code":-1,"message":"secret must not become diagnostic"}}),
    );
    assert_eq!(s.options()[0].value, "vendor:auto");
    assert!(!s.diagnostics().any(|d| d.contains("secret")));
    let line = s
        .set_option("reasoning.setting/v2", "vendor:deep")
        .unwrap()
        .remove(0);
    let mut changed = config();
    changed["configOptions"][0]["currentValue"] = json!("vendor:deep");
    answer(&mut s, &line, changed);
    assert_eq!(s.options()[0].value, "vendor:deep");
    let read = json!({"jsonrpc":"2.0","id":"read","method":"fs/read_text_file","params":{"sessionId":"opaque/session:not-a-uuid","path":"/workspace/file","line":2,"limit":3}});
    assert!(feed(&mut s, read.clone()).is_empty());
    let requests = s.take_host_requests();
    assert_eq!(requests.len(), 1);
    assert!(matches!(
        &requests[0].operation,
        HostOperation::ReadTextFile {
            line: Some(2),
            limit: Some(3),
            ..
        }
    ));
    let reply = s
        .complete_host_request(&requests[0].id, Ok(json!({"content":"host editor buffer"})))
        .unwrap();
    assert_eq!(reply["result"]["content"], "host editor buffer");
    assert!(
        s.complete_host_request(&requests[0].id, Ok(json!({})))
            .is_err()
    );
    let mut write = read.clone();
    write["method"] = json!("fs/write_text_file");
    assert_eq!(feed(&mut s, write)[0]["error"]["code"], -32601);
    let mut wrong = read;
    wrong["params"]["sessionId"] = json!("another");
    assert_eq!(feed(&mut s, wrong)[0]["error"]["code"], -32602);
}

#[test]
fn bad_handshake_and_unsupported_resume_do_not_start_a_new_session() {
    for result in [
        json!({"protocolVersion":2}),
        json!({"protocolVersion":1,"agentCapabilities":{}}),
    ] {
        let start =
            Start::new(std::env::temp_dir()).resume("opaque/session:not-a-uuid".into(), vec![]);
        let (mut s, lines) = AcpSession::new(start).unwrap();
        assert!(answer(&mut s, &lines[0], result).is_empty());
        assert!(matches!(s.status(), Status::Failed(_)));
    }
}

#[cfg(unix)]
#[test]
fn fake_acp_process_runs_initialize_new_prompt_tool_and_shutdown() {
    use canopy_desktop::chat::{agent::AgentDefinition, launch::prepare_acp, process::Process};
    use canopy_desktop::terminal::environment::ShellEnvironment;
    use std::time::{Duration, Instant};
    let script = r#"
IFS= read -r init
case "$init" in *'"method":"initialize"'*) ;; *) exit 4 ;; esac
printf '%s\n' '{"jsonrpc":"2.0","id":"canopy-1","result":{"protocolVersion":1,"agentCapabilities":{}}}'
IFS= read -r new
case "$new" in *'"method":"session/new"'*) ;; *) exit 5 ;; esac
printf '%s\n' '{"jsonrpc":"2.0","id":"canopy-2","result":{"sessionId":"opaque/session:not-a-uuid"}}'
IFS= read -r prompt
case "$prompt" in *'"method":"session/prompt"'*) ;; *) exit 6 ;; esac
printf '%s\n' '{"jsonrpc":"2.0","method":"session/update","params":{"sessionId":"opaque/session:not-a-uuid","update":{"sessionUpdate":"tool_call","toolCallId":"t/1","title":"Inspect","kind":"read","status":"in_progress","locations":[{"path":"/project/a"}]}}}'
printf '%s\n' '{"jsonrpc":"2.0","method":"session/update","params":{"sessionId":"opaque/session:not-a-uuid","update":{"sessionUpdate":"tool_call_update","toolCallId":"t/1","status":"completed","content":[{"type":"content","content":{"type":"text","text":"file content"}}]}}}'
printf '%s\n' '{"jsonrpc":"2.0","method":"session/update","params":{"sessionId":"opaque/session:not-a-uuid","update":{"sessionUpdate":"agent_message_chunk","content":{"type":"text","text":"Done"}}}}'
printf '%s\n' '{"jsonrpc":"2.0","id":"canopy-3","result":{"stopReason":"end_turn"}}'
while IFS= read -r rest; do :; done
"#;
    for id in ["fake-acp-a", "another-compatible-agent"] {
        let mut definition = AgentDefinition::acp(id.into(), id.into(), "/bin/sh".into());
        definition.arguments = vec!["-c".into(), script.into()];
        let environment = ShellEnvironment {
            shell: "/bin/sh".into(),
            vars: Default::default(),
        };
        let launch = prepare_acp(&definition, std::env::temp_dir(), &environment).unwrap();
        let (process, events) =
            Process::start(launch.spec, launch.files, wire::parse_line, 1024 * 1024).unwrap();
        let (mut s, lines) = AcpSession::new(Start::new(std::env::temp_dir())).unwrap();
        for line in lines {
            process.send(&line).unwrap();
        }
        let deadline = Instant::now() + Duration::from_secs(10);
        let mut sent = false;
        loop {
            assert!(Instant::now() < deadline, "fake ACP timed out");
            match events.try_recv() {
                Ok(event) => {
                    for line in s.on_event(event) {
                        process.send(&line).unwrap();
                    }
                }
                Err(_) => std::thread::sleep(Duration::from_millis(5)),
            }
            if s.status() == &Status::Ready {
                if sent {
                    break;
                }
                for line in s
                    .submit(&Draft::text("read file"), "durable-message")
                    .unwrap()
                {
                    process.send(&line).unwrap();
                }
                sent = true;
            }
        }
        let tools = s
            .transcript()
            .items()
            .iter()
            .filter(|i| matches!(&i.body, Body::Tool(_)))
            .count();
        assert_eq!(tools, 1);
        assert!(
            s.transcript()
                .items()
                .iter()
                .any(|i| matches!(&i.body,Body::Text(t) if t.text=="Done" && !t.streaming))
        );
        process.close().unwrap();
        // Local close finishes process lifetime; it is not coupled to a remote viewer.
        s.on_event(Event::Exited(canopy_desktop::chat::process::Exit {
            code: Some(0),
            signal: None,
            stderr: String::new(),
        }));
        assert!(s.status().ended());
    }
}

#[test]
fn semantic_cards_follow_protocol_kind_and_keep_locations_on_partial_updates() {
    use canopy_desktop::chat::{activity, backend::ToolKind};
    let mut s = open(Start::new(std::env::temp_dir()));
    s.submit(&Draft::text("inspect"), "m").unwrap();
    update(
        &mut s,
        json!({"sessionUpdate":"tool_call","toolCallId":"unknown","title":"Bash","kind":"vendor_special"}),
    );
    update(
        &mut s,
        json!({"sessionUpdate":"tool_call","toolCallId":"read","title":"Read anything","kind":"read","locations":[{"path":"/work/a"}]}),
    );
    update(
        &mut s,
        json!({"sessionUpdate":"tool_call_update","toolCallId":"read","rawInput":{"a":1},"status":"completed"}),
    );
    update(
        &mut s,
        json!({"sessionUpdate":"plan","entries":[{"content":"Inspect","priority":"high","status":"completed"}]}),
    );
    let calls = s
        .transcript()
        .items()
        .iter()
        .filter_map(|i| match &i.body {
            Body::Tool(t) => Some(t),
            _ => None,
        })
        .collect::<Vec<_>>();
    assert_eq!(
        calls[0].semantic_kind(),
        ToolKind::Other,
        "unknown ACP kinds must not inherit Claude semantics from a title"
    );
    assert_eq!(calls[1].semantic_kind(), ToolKind::Read);
    assert_eq!(calls[1].file_path(), Some("/work/a"));
    assert_eq!(activity::todos(s.transcript()).unwrap().items.len(), 1);
    // Existing records without a kind remain readable without a database migration.
    let mut legacy = serde_json::to_value(calls[1]).unwrap();
    legacy.as_object_mut().unwrap().remove("kind");
    legacy["name"] = json!("Read");
    let legacy: canopy_desktop::chat::transcript::ToolCall =
        serde_json::from_value(legacy).unwrap();
    assert_eq!(legacy.semantic_kind(), ToolKind::Read);
}

#[test]
fn delivery_needs_the_agents_evidence_and_an_ended_run_leaves_it_unknown() {
    use canopy_desktop::chat::backend::Receipt;
    // Writing the prompt is not delivery; the first update of its turn is.
    let mut s = open(Start::new(std::env::temp_dir()));
    let lines = s.submit(&Draft::text("one"), "message-1").unwrap();
    assert!(s.take_receipts().is_empty());
    assert!(!s.ready_for_message(), "one dispatched message at a time");
    assert!(s.submit(&Draft::text("two"), "message-2").is_err());
    update(
        &mut s,
        json!({"sessionUpdate":"agent_message_chunk","content":{"type":"text","text":"ok"}}),
    );
    assert_eq!(
        s.take_receipts(),
        vec![("message-1".into(), Receipt::Delivered)]
    );
    answer(&mut s, &lines[0], json!({"stopReason":"end_turn"}));
    assert!(s.take_receipts().is_empty(), "settled once");
    assert!(s.ready_for_message());

    // An error answer before any turn event: the agent didn't take it.
    let lines = s.submit(&Draft::text("refused"), "message-2").unwrap();
    feed(
        &mut s,
        json!({"jsonrpc":"2.0","id":lines[0]["id"],"error":{"code":-32000,"message":"no"}}),
    );
    assert_eq!(
        s.take_receipts(),
        vec![("message-2".into(), Receipt::Refused)]
    );

    // The process ends with no evidence: uncertain, never assumed either way.
    s.submit(&Draft::text("lost"), "message-3").unwrap();
    s.on_event(Event::Exited(canopy_desktop::chat::process::Exit {
        code: Some(1),
        signal: None,
        stderr: String::new(),
    }));
    assert_eq!(
        s.take_receipts(),
        vec![("message-3".into(), Receipt::Unknown)]
    );
}
